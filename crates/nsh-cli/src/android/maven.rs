//! `nsh maven`: Android libraries published to Maven repositories (Google's
//! and Maven Central), resolved with their dependencies once and vendored
//! into the app, so that `nsh package` keeps reading local files only
//! (`nsh.json` `android.libraries`, [`super::libraries`]).
//!
//! The app names the libraries it uses directly, as Gradle's
//! `implementation` does (`com.google.firebase:firebase-messaging:26.0.1`).
//! [`resolve`] finds what they depend on as Gradle resolves an Android
//! app's runtime classpath:
//!
//! - Gradle module metadata (`.module`) when the POM says it was published
//!   with it: the variant an Android app runs (`java-runtime`, Android
//!   before plain JVM, release before debug), its `available-at`
//!   (Kotlin Multiplatform roots point at their `-jvm` or `-android`
//!   module), its dependencies, constraints and files with their
//!   checksums; a platform (a BOM) adds constraints.
//! - Otherwise the POM: its parents, properties, `dependencyManagement`
//!   with imported BOMs, `compile` and `runtime` dependencies that are not
//!   optional, and exclusions.
//! - One version per module, the highest any of the graph asks for (a
//!   constraint raises a module only if the graph has it), and the graph
//!   rebuilt from the chosen versions until nothing changes, so what an
//!   evicted version asked for does not count.
//!
//! [`vendor`] downloads the files (checked against the repository's
//! checksums), writes them to `android/maven/`, records the libraries asked
//! for and what they resolved to in `android/maven/maven.json`, and puts
//! the files in `nsh.json` `android.libraries` after the app's own.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fmt, fs,
    io::Read as _,
    path::Path,
};

use anyhow::{Context as _, bail};
use quick_xml::events::Event;
use serde_json::{Value, json};
use sha2::Digest as _;

/// The repositories searched, in order, when none are given.
pub(crate) const DEFAULT_REPOSITORIES: &[&str] = &[
    "https://dl.google.com/dl/android/maven2",
    "https://repo.maven.apache.org/maven2",
];

/// Where `nsh maven` keeps the files, relative to the app's `nsh.json`.
pub(crate) const VENDOR_DIR: &str = "android/maven";

/// A module: group and artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Module {
    pub(crate) group: String,
    pub(crate) artifact: String,
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.group, self.artifact)
    }
}

impl Module {
    fn new(group: &str, artifact: &str) -> Self {
        Module {
            group: group.to_owned(),
            artifact: artifact.to_owned(),
        }
    }

    /// Its directory in a repository.
    fn dir(&self) -> String {
        format!("{}/{}", self.group.replace('.', "/"), self.artifact)
    }
}

/// A library the app asks for: `group:artifact:version`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Coordinate {
    pub(crate) module: Module,
    pub(crate) version: String,
}

impl fmt::Display for Coordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.module, self.version)
    }
}

impl std::str::FromStr for Coordinate {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = text.trim().split(':').collect();
        let ok = |p: &str| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        };
        match parts.as_slice() {
            [group, artifact, version] if ok(group) && ok(artifact) && ok(version) => {
                Ok(Coordinate {
                    module: Module::new(group, artifact),
                    version: (*version).to_owned(),
                })
            }
            _ => bail!("`{text}` is no Maven library: name one as `group:artifact:version`"),
        }
    }
}

/// Where files come from: a repository's paths (`com/example/x/1.0/x-1.0.pom`).
pub(crate) trait Source {
    /// The file at `path`, `None` when the repository has none.
    fn get(&mut self, path: &str) -> anyhow::Result<Option<Vec<u8>>>;
    /// Where `path` is, for messages.
    fn describe(&self, path: &str) -> String;
}

/// Repositories by URL, `https://` or `file://`, searched in order.
pub(crate) struct Repositories {
    urls: Vec<String>,
    agent: Option<ureq::Agent>,
}

impl Repositories {
    pub(crate) fn new(urls: &[String]) -> anyhow::Result<Self> {
        for url in urls {
            if !(url.starts_with("https://") || url.starts_with("file://")) {
                bail!("repository `{url}`: give an https:// or file:// URL");
            }
        }
        Ok(Repositories {
            urls: urls
                .iter()
                .map(|u| u.trim_end_matches('/').to_owned())
                .collect(),
            agent: None,
        })
    }

    fn fetch(&mut self, url: &str) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(path) = url.strip_prefix("file://") {
            return match fs::read(path) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e).with_context(|| format!("read {path}")),
            };
        }
        let agent = self.agent.get_or_insert_with(|| {
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .user_agent(concat!("nsh/", env!("CARGO_PKG_VERSION")))
                .build()
                .into()
        });
        let mut response = agent
            .get(url)
            .call()
            .with_context(|| format!("GET {url}"))?;
        match response.status().as_u16() {
            200 => {
                let mut body = Vec::new();
                response
                    .body_mut()
                    .as_reader()
                    .read_to_end(&mut body)
                    .with_context(|| format!("reading {url}"))?;
                Ok(Some(body))
            }
            404 | 410 => Ok(None),
            code => bail!("GET {url}: HTTP {code}"),
        }
    }
}

impl Source for Repositories {
    fn get(&mut self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        for base in self.urls.clone() {
            if let Some(bytes) = self.fetch(&format!("{base}/{path}"))? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    fn describe(&self, path: &str) -> String {
        match self.urls.as_slice() {
            [one] => format!("{one}/{path}"),
            _ => format!("{path} (in {})", self.urls.join(", ")),
        }
    }
}

/// Compares versions as Gradle does: numeric parts by value, a numeric part
/// above a word, words in the order `dev` < others (alphabetically) < `rc`
/// < `snapshot` < `final` < `ga` < `release` < `sp`; a longer version is
/// higher when what it adds starts with a number (`1.0.1` > `1.0`), lower
/// otherwise (`1.0-rc1` < `1.0`).
pub(crate) fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn parts(v: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        for c in v.chars() {
            if ".-_+".contains(c) {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
            if let Some(last) = cur.chars().last()
                && last.is_ascii_digit() != c.is_ascii_digit()
            {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }
    fn rank(word: &str) -> (u8, String) {
        let lower = word.to_ascii_lowercase();
        let rank = match lower.as_str() {
            "dev" => 0,
            "rc" => 2,
            "snapshot" => 3,
            "final" => 4,
            "ga" => 5,
            "release" => 6,
            "sp" => 7,
            _ => 1,
        };
        (rank, lower)
    }
    let numeric = |p: &str| p.chars().all(|c| c.is_ascii_digit());
    let (a, b) = (parts(a), parts(b));
    for (x, y) in a.iter().zip(&b) {
        let order = match (numeric(x), numeric(y)) {
            (true, true) => {
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                x.len().cmp(&y.len()).then_with(|| x.cmp(y))
            }
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => rank(x).cmp(&rank(y)),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    match a.len().cmp(&b.len()) {
        Ordering::Equal => Ordering::Equal,
        Ordering::Greater => {
            if numeric(&a[b.len()]) {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        Ordering::Less => {
            if numeric(&b[a.len()]) {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    }
}

/// A version requirement as published: a version, or a Maven range.
#[derive(Debug, Clone, PartialEq)]
enum Requirement {
    Version(String),
    /// `[low,high)` and the like: (bound, inclusive) each side.
    Range {
        low: Option<(String, bool)>,
        high: Option<(String, bool)>,
    },
}

impl Requirement {
    fn parse(text: &str) -> anyhow::Result<Self> {
        let text = text.trim();
        let Some(open) = text.chars().next().filter(|c| *c == '[' || *c == '(') else {
            if text.is_empty() {
                bail!("an empty version");
            }
            return Ok(Requirement::Version(text.to_owned()));
        };
        let close = text.chars().last().filter(|c| *c == ']' || *c == ')');
        let (Some(close), true) = (close, text.len() >= 2) else {
            bail!("`{text}` is no version range");
        };
        let inner = &text[1..text.len() - 1];
        if inner.contains("],") || inner.contains("),") {
            bail!("`{text}`: a range of several parts is not read");
        }
        match inner.split_once(',') {
            // `[1.0]`: exactly that version.
            None if open == '[' && close == ']' => {
                Ok(Requirement::Version(inner.trim().to_owned()))
            }
            None => bail!("`{text}` is no version range"),
            Some((low, high)) => {
                let bound = |v: &str, inclusive: bool| {
                    let v = v.trim();
                    (!v.is_empty()).then(|| (v.to_owned(), inclusive))
                };
                Ok(Requirement::Range {
                    low: bound(low, open == '['),
                    high: bound(high, close == ']'),
                })
            }
        }
    }

    fn allows(&self, version: &str) -> bool {
        use std::cmp::Ordering::*;
        match self {
            Requirement::Version(v) => v == version,
            Requirement::Range { low, high } => {
                let above = low.as_ref().is_none_or(|(l, inclusive)| {
                    matches!(
                        (compare_versions(version, l), inclusive),
                        (Greater, _) | (Equal, true)
                    )
                });
                let below = high.as_ref().is_none_or(|(h, inclusive)| {
                    matches!(
                        (compare_versions(version, h), inclusive),
                        (Less, _) | (Equal, true)
                    )
                });
                above && below
            }
        }
    }
}

/// A minimal XML element, enough for POMs and `maven-metadata.xml`.
#[derive(Debug, Default, Clone)]
struct Xml {
    name: String,
    text: String,
    children: Vec<Xml>,
}

impl Xml {
    fn parse(bytes: &[u8]) -> anyhow::Result<Xml> {
        let mut reader = quick_xml::Reader::from_reader(bytes);
        let mut stack = vec![Xml::default()];
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(e) => stack.push(Xml {
                    name: local(e.name().as_ref()),
                    ..Xml::default()
                }),
                Event::Empty(e) => {
                    let element = Xml {
                        name: local(e.name().as_ref()),
                        ..Xml::default()
                    };
                    stack.last_mut().expect("root").children.push(element);
                }
                Event::Text(t) => stack
                    .last_mut()
                    .expect("root")
                    .text
                    .push_str(&t.into_inner()),
                Event::CData(t) => stack
                    .last_mut()
                    .expect("root")
                    .text
                    .push_str(&t.into_inner()),
                Event::GeneralRef(r) => {
                    let c = match r.resolve_char_ref()? {
                        Some(c) => c,
                        None => match &*r {
                            "amp" => '&',
                            "lt" => '<',
                            "gt" => '>',
                            "quot" => '"',
                            "apos" => '\'',
                            other => bail!("the entity `&{other};`, which a POM has no use for"),
                        },
                    };
                    stack.last_mut().expect("root").text.push(c);
                }
                Event::End(_) => {
                    let done = stack.pop().expect("an open element");
                    let Some(parent) = stack.last_mut() else {
                        bail!("unbalanced XML");
                    };
                    parent.children.push(done);
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
        let root = stack
            .pop()
            .filter(|_| stack.is_empty())
            .context("unbalanced XML")?;
        root.children.into_iter().next().context("no XML element")
    }

    fn child(&self, name: &str) -> Option<&Xml> {
        self.children.iter().find(|c| c.name == name)
    }

    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Xml> {
        self.children.iter().filter(move |c| c.name == name)
    }

    fn text_of(&self, name: &str) -> Option<String> {
        self.child(name).map(|c| c.text.trim().to_owned())
    }
}

/// An element's name without its namespace prefix.
fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or_default().to_owned()
}

/// A module excluded below a dependency; `*` matches any.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Exclusion {
    group: String,
    artifact: String,
}

impl Exclusion {
    fn matches(&self, module: &Module) -> bool {
        (self.group == "*" || self.group == module.group)
            && (self.artifact == "*" || self.artifact == module.artifact)
    }
}

/// A dependency of a module version.
#[derive(Debug, Clone, PartialEq)]
struct Dependency {
    module: Module,
    /// `None`: the dependency names no version of its own, and takes the
    /// one a constraint or another edge gives the module.
    requirement: Option<Requirement>,
    exclusions: Vec<Exclusion>,
    /// Asked for as a platform (`org.gradle.category` `platform`): a BOM
    /// whose `dependencyManagement` constrains the graph.
    platform: bool,
}

/// A file of a module version, with the checksum its metadata gives.
#[derive(Debug, Clone, PartialEq)]
struct Artifact {
    /// Its path in the repository.
    path: String,
    /// Its file name.
    name: String,
    sha256: Option<String>,
}

/// What a module version is made of for an Android app.
#[derive(Debug, Clone, PartialEq, Default)]
struct Component {
    dependencies: Vec<Dependency>,
    /// Versions it asks for of modules the graph may have.
    constraints: Vec<(Module, Requirement)>,
    /// What it constrains when asked for as a platform: a POM-only BOM's
    /// `dependencyManagement`.
    platform_constraints: Vec<(Module, Requirement)>,
    files: Vec<Artifact>,
}

/// An effective POM: what a module's POM and its parents say.
#[derive(Debug, Default, Clone)]
struct Pom {
    group: String,
    version: String,
    packaging: String,
    properties: HashMap<String, String>,
    /// `dependencyManagement`, imports resolved: version and scope by module.
    managed: BTreeMap<Module, (String, Option<String>)>,
    dependencies: Vec<Xml>,
    gradle_metadata: bool,
}

/// Reads metadata from a [`Source`], each file once.
struct Reader<'a> {
    source: &'a mut dyn Source,
    poms: HashMap<(Module, String), Pom>,
    components: HashMap<(Module, String), Component>,
    /// The module versions whose metadata could not be read, and why.
    unreadable: HashMap<(Module, String), String>,
    listings: HashMap<Module, Vec<String>>,
}

impl<'a> Reader<'a> {
    fn new(source: &'a mut dyn Source) -> Self {
        Reader {
            source,
            poms: HashMap::new(),
            components: HashMap::new(),
            unreadable: HashMap::new(),
            listings: HashMap::new(),
        }
    }

    fn file(&mut self, path: &str) -> anyhow::Result<Vec<u8>> {
        self.source
            .get(path)?
            .with_context(|| format!("{} not found", self.source.describe(path)))
    }

    fn pom(&mut self, module: &Module, version: &str, depth: usize) -> anyhow::Result<Pom> {
        let key = (module.clone(), version.to_owned());
        if let Some(pom) = self.poms.get(&key) {
            return Ok(pom.clone());
        }
        if depth > 16 {
            bail!("{module}:{version}: POM parents nest too deep");
        }
        let path = format!(
            "{}/{version}/{}-{version}.pom",
            module.dir(),
            module.artifact
        );
        let bytes = self.file(&path)?;
        let xml = Xml::parse(&bytes).with_context(|| format!("{module}:{version}: its POM"))?;
        let text = String::from_utf8_lossy(&bytes);
        let mut pom = match xml.child("parent") {
            Some(parent) => {
                let (g, a, v) = (
                    parent.text_of("groupId").unwrap_or_default(),
                    parent.text_of("artifactId").unwrap_or_default(),
                    parent.text_of("version").unwrap_or_default(),
                );
                let mut pom = self
                    .pom(&Module::new(&g, &a), &v, depth + 1)
                    .with_context(|| format!("{module}:{version}: its parent"))?;
                pom.properties
                    .insert("project.parent.version".into(), v.clone());
                pom.properties.insert("parent.version".into(), v);
                pom.packaging = String::new();
                pom
            }
            None => Pom::default(),
        };
        if let Some(g) = xml.text_of("groupId") {
            pom.group = g;
        }
        pom.version = xml.text_of("version").unwrap_or_else(|| version.to_owned());
        pom.packaging = xml.text_of("packaging").unwrap_or_else(|| "jar".into());
        pom.gradle_metadata = text.contains("published-with-gradle-metadata");
        if let Some(properties) = xml.child("properties") {
            for p in &properties.children {
                pom.properties
                    .insert(p.name.clone(), p.text.trim().to_owned());
            }
        }
        for (key, value) in [
            ("project.groupId", pom.group.clone()),
            ("pom.groupId", pom.group.clone()),
            ("project.version", pom.version.clone()),
            ("pom.version", pom.version.clone()),
            ("version", pom.version.clone()),
            ("project.artifactId", module.artifact.clone()),
        ] {
            pom.properties.insert(key.into(), value);
        }
        let mut own_managed = BTreeMap::new();
        for d in xml
            .child("dependencyManagement")
            .and_then(|m| m.child("dependencies"))
            .into_iter()
            .flat_map(|ds| ds.all("dependency"))
        {
            let g = interpolate(&d.text_of("groupId").unwrap_or_default(), &pom.properties);
            let a = interpolate(
                &d.text_of("artifactId").unwrap_or_default(),
                &pom.properties,
            );
            let v = interpolate(&d.text_of("version").unwrap_or_default(), &pom.properties);
            let scope = d.text_of("scope").map(|s| interpolate(&s, &pom.properties));
            if scope.as_deref() == Some("import") {
                let bom = self
                    .pom(&Module::new(&g, &a), &v, depth + 1)
                    .with_context(|| {
                        format!("{module}:{version}: the BOM {g}:{a}:{v} it imports")
                    })?;
                for (m, entry) in bom.managed {
                    own_managed.entry(m).or_insert(entry);
                }
            } else {
                own_managed.insert(Module::new(&g, &a), (v, scope));
            }
        }
        // The module's own entries win over its parent's.
        for (m, entry) in own_managed {
            pom.managed.insert(m, entry);
        }
        if let Some(ds) = xml.child("dependencies") {
            pom.dependencies.extend(ds.all("dependency").cloned());
        }
        self.poms.insert(key, pom.clone());
        Ok(pom)
    }

    /// The versions a repository lists for `module` (`maven-metadata.xml`).
    fn versions(&mut self, module: &Module) -> anyhow::Result<Vec<String>> {
        if let Some(v) = self.listings.get(module) {
            return Ok(v.clone());
        }
        let path = format!("{}/maven-metadata.xml", module.dir());
        let xml = Xml::parse(&self.file(&path)?)?;
        let versions: Vec<String> = xml
            .child("versioning")
            .and_then(|v| v.child("versions"))
            .into_iter()
            .flat_map(|vs| vs.all("version"))
            .map(|v| v.text.trim().to_owned())
            .collect();
        self.listings.insert(module.clone(), versions.clone());
        Ok(versions)
    }

    /// The version `requirement` picks: its own, or the highest a range
    /// allows.
    fn pick(&mut self, module: &Module, requirement: &Requirement) -> anyhow::Result<String> {
        match requirement {
            Requirement::Version(v) => Ok(v.clone()),
            Requirement::Range { .. } => {
                let versions = self.versions(module).with_context(|| {
                    format!("{module}: the versions of its range {requirement:?}")
                })?;
                versions
                    .into_iter()
                    .filter(|v| requirement.allows(v))
                    .max_by(|a, b| compare_versions(a, b))
                    .with_context(|| format!("{module}: no version in the range {requirement:?}"))
            }
        }
    }

    fn component(&mut self, module: &Module, version: &str) -> anyhow::Result<Component> {
        let key = (module.clone(), version.to_owned());
        if let Some(c) = self.components.get(&key) {
            return Ok(c.clone());
        }
        if let Some(why) = self.unreadable.get(&key) {
            bail!("{why}");
        }
        let component = self.read_component(module, version);
        match &component {
            Ok(c) => {
                self.components.insert(key, c.clone());
            }
            Err(e) => {
                self.unreadable.insert(key, format!("{e:#}"));
            }
        }
        component
    }

    fn read_component(&mut self, module: &Module, version: &str) -> anyhow::Result<Component> {
        let pom = self.pom(module, version, 0)?;
        let path = format!(
            "{}/{version}/{}-{version}.module",
            module.dir(),
            module.artifact
        );
        let component = match pom
            .gradle_metadata
            .then(|| self.source.get(&path))
            .transpose()?
            .flatten()
        {
            Some(bytes) => {
                let json: Value = serde_json::from_slice(&bytes)
                    .with_context(|| format!("{module}:{version}: its .module is not JSON"))?;
                from_module_metadata(module, version, &json)
                    .with_context(|| format!("{module}:{version}: its .module"))?
            }
            None => self
                .pom_component(module, version, &pom)
                .with_context(|| format!("{module}:{version}: its POM"))?,
        };
        Ok(component)
    }

    fn pom_component(
        &mut self,
        module: &Module,
        version: &str,
        pom: &Pom,
    ) -> anyhow::Result<Component> {
        let mut component = Component::default();
        for d in &pom.dependencies {
            let p = &pom.properties;
            let field = |name: &str| d.text_of(name).map(|t| interpolate(&t, p));
            let target = Module::new(
                &field("groupId").unwrap_or_default(),
                &field("artifactId").unwrap_or_default(),
            );
            let managed = pom.managed.get(&target);
            let scope = field("scope")
                .or_else(|| managed.and_then(|(_, s)| s.clone()))
                .unwrap_or_else(|| "compile".into());
            if !matches!(scope.as_str(), "compile" | "runtime")
                || field("optional").as_deref() == Some("true")
                || field("type").as_deref() == Some("pom")
            {
                continue;
            }
            let version = field("version")
                .filter(|v| !v.is_empty())
                .or_else(|| managed.map(|(v, _)| v.clone()))
                .with_context(|| {
                    format!("{target} has no version, nor one in dependencyManagement")
                })?;
            let exclusions = d
                .child("exclusions")
                .into_iter()
                .flat_map(|e| e.all("exclusion"))
                .map(|e| Exclusion {
                    group: interpolate(&e.text_of("groupId").unwrap_or("*".into()), p),
                    artifact: interpolate(&e.text_of("artifactId").unwrap_or("*".into()), p),
                })
                .collect();
            component.dependencies.push(Dependency {
                requirement: Some(
                    Requirement::parse(&version)
                        .with_context(|| format!("{target}: its version"))?,
                ),
                module: target,
                exclusions,
                platform: false,
            });
        }
        let packaging = interpolate(&pom.packaging, &pom.properties);
        if packaging == "pom" {
            for (m, (v, _)) in &pom.managed {
                if let Ok(requirement) = Requirement::parse(v) {
                    component
                        .platform_constraints
                        .push((m.clone(), requirement));
                }
            }
        }
        if packaging != "pom" {
            let first = if packaging == "aar" { "aar" } else { "jar" };
            let other = if first == "aar" { "jar" } else { "aar" };
            let mut found = None;
            for ext in [first, other] {
                let name = format!("{}-{version}.{ext}", module.artifact);
                let path = format!("{}/{version}/{name}", module.dir());
                if self.source.get(&path)?.is_some() {
                    found = Some(Artifact {
                        path,
                        name,
                        sha256: None,
                    });
                    break;
                }
            }
            component.files.push(
                found
                    .with_context(|| format!("its {first} file is in none of the repositories"))?,
            );
        }
        Ok(component)
    }
}

/// `text` with its `${name}` properties put in, as far as they are known.
fn interpolate(text: &str, properties: &HashMap<String, String>) -> String {
    let mut text = text.trim().to_owned();
    for _ in 0..8 {
        let Some(start) = text.find("${") else { break };
        let Some(len) = text[start..].find('}') else {
            break;
        };
        let name = &text[start + 2..start + len];
        let Some(value) = properties.get(name) else {
            break;
        };
        text = format!("{}{value}{}", &text[..start], &text[start + len + 1..]);
    }
    text
}

/// Whether a variant's attributes are what an Android app's runtime
/// classpath takes, and how well (higher is better).
fn variant_score(attributes: &serde_json::Map<String, Value>) -> Option<u32> {
    let attr = |k: &str| attributes.get(k).and_then(Value::as_str);
    if !matches!(
        attr("org.gradle.category"),
        None | Some("library") | Some("platform")
    ) || attr("org.gradle.docstype").is_some()
        || !matches!(
            attr("org.gradle.usage"),
            None | Some("java-runtime") | Some("java-api")
        )
        || !matches!(
            attr("org.jetbrains.kotlin.platform.type"),
            None | Some("androidJvm") | Some("jvm")
        )
        || matches!(attr("org.gradle.jvm.environment"), Some("non-jvm"))
    {
        return None;
    }
    let mut score = 0;
    match attr("org.gradle.usage") {
        Some("java-runtime") => score += 16,
        Some(_) => score += 8,
        None => {}
    }
    if attr("org.jetbrains.kotlin.platform.type") == Some("androidJvm")
        || attr("org.gradle.jvm.environment") == Some("android")
    {
        score += 4;
    }
    if attr("com.android.build.api.attributes.BuildTypeAttr") != Some("debug") {
        score += 2;
    }
    if attr("org.gradle.category") != Some("platform") {
        score += 1;
    }
    Some(score)
}

/// The [`Component`] a `.module` file describes for an Android app.
fn from_module_metadata(module: &Module, version: &str, json: &Value) -> anyhow::Result<Component> {
    let variants = json["variants"].as_array().context("no variants")?;
    let best = variants
        .iter()
        .filter_map(|v| {
            let attributes = v["attributes"].as_object()?;
            variant_score(attributes).map(|s| (s, v))
        })
        .max_by_key(|(s, _)| *s)
        .map(|(_, v)| v)
        .context("no variant an Android app runs (`java-runtime` of a library)")?;
    let mut component = Component::default();
    if let Some(at) = best.get("available-at") {
        let target = Module::new(
            at["group"].as_str().context("available-at: no group")?,
            at["module"].as_str().context("available-at: no module")?,
        );
        let version = at["version"].as_str().context("available-at: no version")?;
        component.dependencies.push(Dependency {
            module: target,
            requirement: Some(Requirement::Version(version.to_owned())),
            exclusions: Vec::new(),
            platform: false,
        });
        return Ok(component);
    }
    let requirement = |d: &Value| -> anyhow::Result<Option<Requirement>> {
        let v = &d["version"];
        let text = v["strictly"]
            .as_str()
            .or(v["requires"].as_str())
            .or(v["prefers"].as_str());
        text.map(Requirement::parse).transpose()
    };
    let platform = best["attributes"]["org.gradle.category"].as_str() == Some("platform");
    for d in best["dependencies"].as_array().into_iter().flatten() {
        let target = Module::new(
            d["group"].as_str().context("a dependency without group")?,
            d["module"]
                .as_str()
                .context("a dependency without module")?,
        );
        let requirement = requirement(d)?;
        let exclusions = d["excludes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| Exclusion {
                group: e["group"].as_str().unwrap_or("*").to_owned(),
                artifact: e["module"].as_str().unwrap_or("*").to_owned(),
            })
            .collect();
        let platform = matches!(
            d["attributes"]["org.gradle.category"].as_str(),
            Some("platform" | "enforced-platform")
        );
        component.dependencies.push(Dependency {
            module: target,
            requirement,
            exclusions,
            platform,
        });
    }
    for c in best["dependencyConstraints"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let target = Module::new(
            c["group"].as_str().context("a constraint without group")?,
            c["module"]
                .as_str()
                .context("a constraint without module")?,
        );
        if let Some(requirement) = requirement(c)? {
            component.constraints.push((target, requirement));
        }
    }
    if !platform {
        for f in best["files"].as_array().into_iter().flatten() {
            let name = f["name"].as_str().context("a file without name")?;
            let url = f["url"].as_str().unwrap_or(name);
            if url.contains("..") || url.starts_with('/') {
                bail!("file `{url}` is outside its version's directory");
            }
            component.files.push(Artifact {
                path: format!("{}/{version}/{url}", module.dir()),
                name: name.to_owned(),
                sha256: f["sha256"].as_str().map(str::to_owned),
            });
        }
    }
    Ok(component)
}

/// A module in the resolved graph: its version and files.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Resolved {
    pub(crate) module: Module,
    pub(crate) version: String,
    files: Vec<Artifact>,
}

/// Resolves `roots` and their dependencies, one version per module, in the
/// order the graph reaches them (roots first). See the module docs.
pub(crate) fn resolve(
    source: &mut dyn Source,
    roots: &[Coordinate],
) -> anyhow::Result<Vec<Resolved>> {
    let mut reader = Reader::new(source);
    let mut chosen: BTreeMap<Module, String> = BTreeMap::new();
    for pass in 0.. {
        if pass == 64 {
            bail!("the versions do not settle: the libraries ask for each other in a loop");
        }
        let (order, asked, failures) = walk(&mut reader, roots, &chosen)?;
        if asked == chosen {
            // What could not be read counts only for the versions chosen:
            // an evicted one's metadata may be gone from a repository.
            if let Some(error) = failures.into_iter().next() {
                return Err(error);
            }
            let mut out = Vec::new();
            for module in order {
                let version = chosen[&module].clone();
                let component = reader.component(&module, &version)?;
                out.push(Resolved {
                    module,
                    version,
                    files: component.files,
                });
            }
            return Ok(out);
        }
        chosen = asked;
    }
    unreachable!()
}

/// What [`walk`] finds: the modules in the order reached, the version each
/// is asked for at most, and the module versions whose metadata could not
/// be read (not expanded).
type Walk = (Vec<Module>, BTreeMap<Module, String>, Vec<anyhow::Error>);

/// One walk of the graph with `chosen` versions (a module not chosen yet
/// takes what the first edge reaching it asks for), constraints counted.
fn walk(
    reader: &mut Reader<'_>,
    roots: &[Coordinate],
    chosen: &BTreeMap<Module, String>,
) -> anyhow::Result<Walk> {
    let mut asked: BTreeMap<Module, String> = BTreeMap::new();
    fn ask(asked: &mut BTreeMap<Module, String>, module: &Module, version: String) {
        let entry = asked
            .entry(module.clone())
            .or_insert_with(|| version.clone());
        if compare_versions(&version, entry).is_gt() {
            *entry = version;
        }
    }
    let mut order = Vec::new();
    // The exclusions each module was reached with: it is visited again
    // when reached with fewer, as Gradle excludes only what every path
    // excludes.
    let mut reached: HashMap<Module, BTreeSet<Exclusion>> = HashMap::new();
    let mut queue: VecDeque<(Module, BTreeSet<Exclusion>)> = VecDeque::new();
    for root in roots {
        let version = reader.pick(&root.module, &Requirement::parse(&root.version)?)?;
        ask(&mut asked, &root.module, version);
        queue.push_back((root.module.clone(), BTreeSet::new()));
    }
    // The modules asked for as a platform, whose BOM constraints count.
    let mut platforms: BTreeSet<Module> = BTreeSet::new();
    let mut constraints = Vec::new();
    // Which module version first asked for each, for errors.
    let mut via: HashMap<Module, String> = HashMap::new();
    let mut failures = Vec::new();
    while let Some((module, exclusions)) = queue.pop_front() {
        let now = match reached.get(&module) {
            None => {
                order.push(module.clone());
                exclusions
            }
            Some(had) => {
                let fewer: BTreeSet<Exclusion> = had.intersection(&exclusions).cloned().collect();
                if &fewer == had {
                    continue;
                }
                fewer
            }
        };
        reached.insert(module.clone(), now.clone());
        let version = chosen
            .get(&module)
            .or_else(|| asked.get(&module))
            .cloned()
            .expect("asked before queued");
        let component = match reader.component(&module, &version) {
            Ok(component) => component,
            Err(error) => {
                failures.push(error.context(match via.get(&module) {
                    Some(parent) => format!("{module}:{version}, a dependency of {parent}"),
                    None => format!("{module}:{version}"),
                }));
                continue;
            }
        };
        constraints.extend(component.constraints.iter().cloned());
        if platforms.contains(&module) {
            constraints.extend(component.platform_constraints.iter().cloned());
        }
        for d in &component.dependencies {
            if now.iter().any(|e| e.matches(&d.module)) {
                continue;
            }
            if d.platform && platforms.insert(d.module.clone()) && reached.contains_key(&d.module) {
                // Reached before as a library: its BOM constraints now count.
                let version = chosen
                    .get(&d.module)
                    .or_else(|| asked.get(&d.module))
                    .cloned();
                if let Some(Ok(c)) = version.map(|v| reader.component(&d.module, &v)) {
                    constraints.extend(c.platform_constraints);
                }
            }
            // One without a version takes its module's constraint's, else
            // what another edge asks for.
            let requirement = d.requirement.clone().or_else(|| {
                component
                    .constraints
                    .iter()
                    .find(|(m, _)| *m == d.module)
                    .map(|(_, r)| r.clone())
            });
            match requirement {
                Some(requirement) => {
                    let picked = reader.pick(&d.module, &requirement).with_context(|| {
                        format!("{module}:{version}: its dependency {}", d.module)
                    })?;
                    ask(&mut asked, &d.module, picked);
                }
                None if chosen.contains_key(&d.module) || asked.contains_key(&d.module) => {}
                None => {
                    failures.push(anyhow::anyhow!(
                        "{module}:{version}: its dependency {} names no version, and nothing else asks for one",
                        d.module
                    ));
                    continue;
                }
            }
            via.entry(d.module.clone())
                .or_insert_with(|| format!("{module}:{version}"));
            let mut below = now.clone();
            below.extend(d.exclusions.iter().cloned());
            queue.push_back((d.module.clone(), below));
        }
    }
    for (module, requirement) in constraints {
        if asked.contains_key(&module) {
            let version = reader.pick(&module, &requirement)?;
            ask(&mut asked, &module, version);
        }
    }
    Ok((order, asked, failures))
}

/// What `android/maven/maven.json` records: the libraries asked for, and
/// the files they resolved to.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct Record {
    /// `group:artifact:version`, as the app named them.
    pub(crate) libraries: Vec<String>,
    /// The repositories, when not the default ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) repositories: Vec<String>,
    /// Every file, relative to `nsh.json`, with what it is and its SHA-256.
    pub(crate) resolved: Vec<RecordedFile>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Clone)]
pub(crate) struct RecordedFile {
    pub(crate) path: String,
    pub(crate) library: String,
    pub(crate) sha256: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads `resolved`'s files into `dir` (the app's `android/maven`),
/// each checked against the checksum its metadata or repository gives.
fn download(
    source: &mut dyn Source,
    resolved: &[Resolved],
    dir: &Path,
) -> anyhow::Result<Vec<RecordedFile>> {
    let mut out = Vec::new();
    for r in resolved {
        for file in &r.files {
            let bytes = source
                .get(&file.path)?
                .with_context(|| format!("{} not found", source.describe(&file.path)))?;
            let sha256 = hex(&sha2::Sha256::digest(&bytes));
            let expected = match &file.sha256 {
                Some(s) => Some(("SHA-256", s.to_ascii_lowercase(), sha256.clone())),
                None => match source.get(&format!("{}.sha256", file.path))? {
                    Some(s) => Some(("SHA-256", checksum_text(&s), sha256.clone())),
                    None => source.get(&format!("{}.sha1", file.path))?.map(|s| {
                        let sha1 =
                            ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &bytes);
                        ("SHA-1", checksum_text(&s), hex(sha1.as_ref()))
                    }),
                },
            };
            match expected {
                Some((kind, want, have)) if want != have => bail!(
                    "{}: its {kind} is {have}, the repository says {want}",
                    source.describe(&file.path)
                ),
                Some(_) => {}
                None => crate::ui::warn(format!(
                    "{}: the repository gives no checksum to check it against",
                    source.describe(&file.path)
                )),
            }
            let relative = format!("{}/{}", r.module.group, file.name);
            let path = dir.join(&relative);
            fs::create_dir_all(path.parent().expect("a file in a directory"))?;
            fs::write(&path, &bytes).with_context(|| format!("write {}", path.display()))?;
            out.push(RecordedFile {
                path: format!("{VENDOR_DIR}/{relative}"),
                library: format!("{}:{}", r.module, r.version),
                sha256,
            });
        }
    }
    Ok(out)
}

/// The hex digest at the start of a checksum file (`<hex>  name`).
fn checksum_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// `nsh maven`: adds `add` to the libraries the app at or above `cwd`
/// takes from Maven, drops those `remove` names (`group:artifact`),
/// resolves them with `repositories` (default: Google's and Maven
/// Central), and vendors the result ([`vendor`]).
pub(crate) fn run(
    cwd: &Path,
    add: &[String],
    remove: &[String],
    repositories: &[String],
) -> anyhow::Result<()> {
    let app = cwd
        .ancestors()
        .find(|d| d.join("nsh.json").is_file())
        .with_context(|| format!("no nsh.json in {} or above it", cwd.display()))?;
    let record_path = app.join(VENDOR_DIR).join("maven.json");
    let mut record: Record = match fs::read(&record_path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("{} is not what nsh maven writes", record_path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Record::default(),
        Err(e) => return Err(e).with_context(|| format!("read {}", record_path.display())),
    };
    let mut wanted: Vec<Coordinate> = record
        .libraries
        .iter()
        .map(|l| l.parse())
        .collect::<anyhow::Result<_>>()?;
    for name in remove {
        let before = wanted.len();
        wanted.retain(|c| c.module.to_string() != *name);
        if wanted.len() == before {
            bail!("`{name}` is not one of the libraries nsh maven added (group:artifact)");
        }
    }
    for text in add {
        let c: Coordinate = text.parse()?;
        wanted.retain(|w| w.module != c.module);
        wanted.push(c);
    }
    if !repositories.is_empty() {
        record.repositories = repositories.to_vec();
    }
    let urls: Vec<String> = if record.repositories.is_empty() {
        DEFAULT_REPOSITORIES
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    } else {
        record.repositories.clone()
    };
    let mut source = Repositories::new(&urls)?;
    vendor(app, &mut source, &wanted, record.repositories)
}

/// Resolves `wanted` from `source` and makes the app at `app` take the
/// result: the files in `android/maven/` (the previous ones removed),
/// `android/maven/maven.json`, and `nsh.json` `android.libraries`, where
/// they follow the app's own entries.
pub(crate) fn vendor(
    app: &Path,
    source: &mut dyn Source,
    wanted: &[Coordinate],
    repositories: Vec<String>,
) -> anyhow::Result<()> {
    let resolved = resolve(source, wanted)?;
    let dir = app.join(VENDOR_DIR);
    let old: Record = fs::read(dir.join("maven.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    // Into a fresh directory first, so a failed download leaves the app as
    // it was.
    let staging = app.join(".nsh/maven-staging");
    let _ = fs::remove_dir_all(&staging);
    let files = download(source, &resolved, &staging)?;
    for f in &old.resolved {
        let _ = fs::remove_file(app.join(&f.path));
    }
    for f in &files {
        let from = staging.join(
            f.path
                .strip_prefix(&format!("{VENDOR_DIR}/"))
                .expect("vendored"),
        );
        let to = app.join(&f.path);
        fs::create_dir_all(to.parent().expect("in a directory"))?;
        fs::rename(&from, &to).with_context(|| format!("move {} into place", to.display()))?;
    }
    let _ = fs::remove_dir_all(&staging);
    remove_empty_dirs(&dir);
    let record = Record {
        libraries: wanted.iter().map(ToString::to_string).collect(),
        repositories,
        resolved: files,
    };
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("maven.json"),
        serde_json::to_string_pretty(&record)? + "\n",
    )?;
    list_in_nsh_json(app, &old, &record)?;
    let modules = resolved.len();
    crate::ui::note(format!(
        "{} libraries from Maven ({modules} modules, {} files) are in {VENDOR_DIR}/ and nsh.json `android.libraries`",
        wanted.len(),
        record.resolved.len()
    ));
    Ok(())
}

fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                remove_empty_dirs(&e.path());
                let _ = fs::remove_dir(e.path());
            }
        }
    }
}

/// Puts `record`'s files in `nsh.json` `android.libraries` in place of
/// `old`'s, after the app's other entries.
fn list_in_nsh_json(app: &Path, old: &Record, record: &Record) -> anyhow::Result<()> {
    let path = app.join("nsh.json");
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut config: Value = serde_json::from_str(&text).context("nsh.json is not JSON")?;
    let object = config
        .as_object_mut()
        .context("nsh.json is not an object")?;
    let android = object
        .entry("android")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("nsh.json `android` is not an object")?;
    let ours: BTreeSet<&str> = old
        .resolved
        .iter()
        .chain(&record.resolved)
        .map(|f| f.path.as_str())
        .collect();
    let mut libraries: Vec<Value> = match android.get("libraries") {
        None => Vec::new(),
        Some(Value::Array(list)) => list
            .iter()
            .filter(|v| !v.as_str().is_some_and(|s| ours.contains(s)))
            .cloned()
            .collect(),
        Some(_) => bail!("nsh.json `android.libraries` is not a list"),
    };
    libraries.extend(record.resolved.iter().map(|f| Value::from(f.path.clone())));
    if libraries.is_empty() {
        android.remove("libraries");
    } else {
        android.insert("libraries".into(), Value::Array(libraries));
    }
    fs::write(&path, serde_json::to_string_pretty(&config)? + "\n")?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A repository in memory.
    #[derive(Default)]
    pub(crate) struct Fake(pub(crate) BTreeMap<String, Vec<u8>>);

    impl Source for Fake {
        fn get(&mut self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.0.get(path).cloned())
        }
        fn describe(&self, path: &str) -> String {
            format!("fake/{path}")
        }
    }

    impl Fake {
        fn pom(&mut self, g: &str, a: &str, v: &str, body: &str) -> &mut Self {
            let path = format!("{}/{a}/{v}/{a}-{v}.pom", g.replace('.', "/"));
            let xml = format!(
                "<project xmlns=\"http://maven.apache.org/POM/4.0.0\"><modelVersion>4.0.0</modelVersion><groupId>{g}</groupId><artifactId>{a}</artifactId><version>{v}</version>{body}</project>"
            );
            self.0.insert(path, xml.into_bytes());
            self
        }
        fn file(&mut self, g: &str, a: &str, v: &str, ext: &str) -> &mut Self {
            let path = format!("{}/{a}/{v}/{a}-{v}.{ext}", g.replace('.', "/"));
            self.0.insert(path, format!("{a}-{v}").into_bytes());
            self
        }
        /// A library `g:a:v` of `ext` depending on `deps` (`g:a:v`).
        fn lib(&mut self, g: &str, a: &str, v: &str, ext: &str, deps: &[&str]) -> &mut Self {
            let deps: String = deps
                .iter()
                .map(|d| {
                    let p: Vec<&str> = d.split(':').collect();
                    format!(
                        "<dependency><groupId>{}</groupId><artifactId>{}</artifactId><version>{}</version></dependency>",
                        p[0], p[1], p[2]
                    )
                })
                .collect();
            self.pom(
                g,
                a,
                v,
                &format!("<packaging>{ext}</packaging><dependencies>{deps}</dependencies>"),
            )
            .file(g, a, v, ext)
        }
    }

    fn names(resolved: &[Resolved]) -> Vec<String> {
        resolved
            .iter()
            .map(|r| format!("{}:{}", r.module, r.version))
            .collect()
    }

    fn coordinate(text: &str) -> Coordinate {
        text.parse().unwrap()
    }

    #[test]
    fn versions_compare_as_gradle_compares_them() {
        use std::cmp::Ordering::*;
        for (a, b, order) in [
            ("1.0", "1.0", Equal),
            ("1.10", "1.9", Greater),
            ("1.0.1", "1.0", Greater),
            ("1.0-rc1", "1.0", Less),
            ("1.0-alpha01", "1.0-beta01", Less),
            ("1.0-beta01", "1.0-rc01", Less),
            ("1.0-dev", "1.0-alpha", Less),
            ("1.0-SNAPSHOT", "1.0-rc", Greater),
            ("1.0-final", "1.0-snapshot", Greater),
            ("2.2.10", "2.1.21", Greater),
            ("9999.0-empty-to-avoid-conflict-with-guava", "1.0", Greater),
            ("18.0.0", "18.0", Greater),
            ("1.0.1", "1.0-rc1", Greater),
        ] {
            assert_eq!(compare_versions(a, b), order, "{a} vs {b}");
            assert_eq!(compare_versions(b, a), order.reverse(), "{b} vs {a}");
        }
    }

    #[test]
    fn requirements_and_ranges() {
        assert_eq!(
            Requirement::parse("1.2").unwrap(),
            Requirement::Version("1.2".into())
        );
        assert_eq!(
            Requirement::parse("[24.0.0]").unwrap(),
            Requirement::Version("24.0.0".into())
        );
        let range = Requirement::parse("[1.0,2.0)").unwrap();
        assert!(range.allows("1.0") && range.allows("1.9.9") && !range.allows("2.0"));
        let open = Requirement::parse("(1.0,]").unwrap();
        assert!(!open.allows("1.0") && open.allows("7"));
        assert!(Requirement::parse("[1.0").is_err());
        assert!(Requirement::parse("[1,2),[3,4)").is_err());
        assert!("a:b".parse::<Coordinate>().is_err());
        assert!("a:b:1;rm".parse::<Coordinate>().is_err());
        assert_eq!(coordinate("com.a:b-c:1.0").to_string(), "com.a:b-c:1.0");
    }

    #[test]
    fn the_highest_version_asked_for_wins_and_evicted_edges_go() {
        let mut repo = Fake::default();
        repo.lib("app", "a", "1", "aar", &["lib:x:1", "lib:y:1"])
            .lib("lib", "x", "1", "jar", &["lib:z:1"])
            .lib("lib", "y", "1", "aar", &["lib:x:2"])
            // x 2 no longer needs z: z goes with x 1.
            .lib("lib", "x", "2", "jar", &[])
            .lib("lib", "z", "1", "jar", &[]);
        let resolved = resolve(&mut repo, &[coordinate("app:a:1")]).unwrap();
        assert_eq!(names(&resolved), ["app:a:1", "lib:x:2", "lib:y:1"]);
        // An evicted version whose metadata the repository no longer has
        // does not matter.
        repo.0.remove("lib/x/1/x-1.pom");
        let resolved = resolve(&mut repo, &[coordinate("app:a:1")]).unwrap();
        assert_eq!(names(&resolved), ["app:a:1", "lib:x:2", "lib:y:1"]);
        assert_eq!(resolved[0].files[0].name, "a-1.aar");
        assert_eq!(resolved[1].files[0].name, "x-2.jar");
    }

    #[test]
    fn poms_take_parents_properties_managed_versions_and_exclusions() {
        let mut repo = Fake::default();
        repo.pom(
            "org.bom",
            "bom",
            "3",
            "<packaging>pom</packaging><dependencyManagement><dependencies><dependency><groupId>lib</groupId><artifactId>m</artifactId><version>3.1</version></dependency></dependencies></dependencyManagement>",
        )
        .pom(
            "org.p",
            "parent",
            "1",
            "<packaging>pom</packaging><properties><x.version>4</x.version></properties><dependencyManagement><dependencies><dependency><groupId>org.bom</groupId><artifactId>bom</artifactId><version>3</version><type>pom</type><scope>import</scope></dependency></dependencies></dependencyManagement>",
        )
        .pom(
            "app",
            "a",
            "1",
            "<parent><groupId>org.p</groupId><artifactId>parent</artifactId><version>1</version></parent><packaging>aar</packaging><dependencies>\
             <dependency><groupId>lib</groupId><artifactId>x</artifactId><version>${x.version}</version><exclusions><exclusion><groupId>lib</groupId><artifactId>gone</artifactId></exclusion></exclusions></dependency>\
             <dependency><groupId>lib</groupId><artifactId>m</artifactId></dependency>\
             <dependency><groupId>lib</groupId><artifactId>t</artifactId><version>1</version><scope>test</scope></dependency>\
             <dependency><groupId>lib</groupId><artifactId>o</artifactId><version>1</version><optional>true</optional></dependency>\
             </dependencies>",
        )
        .file("app", "a", "1", "aar")
        .lib("lib", "x", "4", "jar", &["lib:gone:1"])
        .lib("lib", "m", "3.1", "jar", &[]);
        let resolved = resolve(&mut repo, &[coordinate("app:a:1")]).unwrap();
        assert_eq!(names(&resolved), ["app:a:1", "lib:x:4", "lib:m:3.1"]);
        // Excluded on one path only: kept.
        repo.lib("app", "b", "1", "jar", &["lib:gone:1"])
            .lib("lib", "gone", "1", "jar", &[]);
        let resolved = resolve(&mut repo, &[coordinate("app:a:1"), coordinate("app:b:1")]).unwrap();
        assert!(names(&resolved).contains(&"lib:gone:1".to_owned()));
        // Reached first through the path that excludes, then through one
        // that does not: what is below it is taken after all.
        repo.pom(
            "app",
            "c",
            "1",
            "<packaging>jar</packaging><dependencies>\
             <dependency><groupId>lib</groupId><artifactId>p</artifactId><version>1</version><exclusions><exclusion><groupId>lib</groupId><artifactId>deep</artifactId></exclusion></exclusions></dependency>\
             <dependency><groupId>lib</groupId><artifactId>q</artifactId><version>1</version></dependency>\
             </dependencies>",
        )
        .file("app", "c", "1", "jar")
        .lib("lib", "q", "1", "jar", &["lib:p:1"])
        .lib("lib", "p", "1", "jar", &["lib:deep:1"])
        .lib("lib", "deep", "1", "jar", &[]);
        let resolved = resolve(&mut repo, &[coordinate("app:c:1")]).unwrap();
        assert_eq!(
            names(&resolved),
            ["app:c:1", "lib:p:1", "lib:q:1", "lib:deep:1"]
        );
    }

    #[test]
    fn module_metadata_takes_the_android_variant_redirects_and_constraints() {
        let mut repo = Fake::default();
        let marker = "<!-- do_not_remove: published-with-gradle-metadata -->";
        let module = |variants: Value| {
            serde_json::to_vec(&json!({ "formatVersion": "1.1", "variants": variants })).unwrap()
        };
        // A Kotlin Multiplatform root, whose JVM variant is elsewhere.
        repo.pom("kmp", "core", "1", marker);
        repo.0.insert(
            "kmp/core/1/core-1.module".into(),
            module(json!([
                { "name": "js", "attributes": { "org.gradle.usage": "kotlin-runtime", "org.jetbrains.kotlin.platform.type": "js" },
                  "available-at": { "url": "x", "group": "kmp", "module": "core-js", "version": "1" } },
                { "name": "jvm", "attributes": { "org.gradle.category": "library", "org.gradle.usage": "java-runtime", "org.jetbrains.kotlin.platform.type": "jvm" },
                  "available-at": { "url": "x", "group": "kmp", "module": "core-jvm", "version": "1" } },
            ])),
        );
        repo.pom("kmp", "core-jvm", "1", marker);
        repo.0.insert(
            "kmp/core-jvm/1/core-jvm-1.module".into(),
            module(json!([
                { "name": "api", "attributes": { "org.gradle.category": "library", "org.gradle.usage": "java-api" },
                  "dependencies": [{ "group": "lib", "module": "api-only", "version": { "requires": "1" } }] },
                { "name": "runtime", "attributes": { "org.gradle.category": "library", "org.gradle.usage": "java-runtime", "org.jetbrains.kotlin.platform.type": "jvm" },
                  "dependencies": [{ "group": "kmp", "module": "bom", "version": { "requires": "1" } }],
                  "files": [{ "name": "core-jvm-1.jar", "url": "core-jvm-1.jar", "sha256": "abc" }] },
            ])),
        );
        repo.file("kmp", "core-jvm", "1", "jar");
        // A platform: its constraints raise `lib:raised` only once the
        // graph has it, and leave `lib:absent` out.
        repo.pom("kmp", "bom", "1", marker);
        repo.0.insert(
            "kmp/bom/1/bom-1.module".into(),
            module(json!([
                { "name": "platform", "attributes": { "org.gradle.category": "platform", "org.gradle.usage": "java-runtime" },
                  "dependencyConstraints": [
                      { "group": "lib", "module": "raised", "version": { "requires": "2" } },
                      { "group": "lib", "module": "absent", "version": { "requires": "[9,10)" } },
                  ] },
            ])),
        );
        repo.lib("app", "a", "1", "aar", &["kmp:core:1", "lib:raised:1"])
            .lib("lib", "raised", "1", "jar", &[])
            .lib("lib", "raised", "2", "jar", &[]);
        let resolved = resolve(&mut repo, &[coordinate("app:a:1")]).unwrap();
        assert_eq!(
            names(&resolved),
            [
                "app:a:1",
                "kmp:core:1",
                "lib:raised:2",
                "kmp:core-jvm:1",
                "kmp:bom:1"
            ]
        );
        // A root and a platform have no files; the JVM module its own, with
        // the checksum its metadata gives.
        assert!(resolved[1].files.is_empty() && resolved[4].files.is_empty());
        assert_eq!(resolved[3].files[0].sha256.as_deref(), Some("abc"));
        // The Android variant goes before the JVM one, release before debug.
        let attrs = |v: Value| v.as_object().unwrap().clone();
        let android = variant_score(&attrs(
            json!({ "org.gradle.usage": "java-runtime", "org.jetbrains.kotlin.platform.type": "androidJvm", "com.android.build.api.attributes.BuildTypeAttr": "release" }),
        ));
        let debug = variant_score(&attrs(
            json!({ "org.gradle.usage": "java-runtime", "org.jetbrains.kotlin.platform.type": "androidJvm", "com.android.build.api.attributes.BuildTypeAttr": "debug" }),
        ));
        let jvm = variant_score(&attrs(
            json!({ "org.gradle.usage": "java-runtime", "org.jetbrains.kotlin.platform.type": "jvm" }),
        ));
        assert!(
            android > debug && debug > jvm,
            "{android:?} {debug:?} {jvm:?}"
        );
        assert_eq!(
            variant_score(&attrs(
                json!({ "org.gradle.usage": "java-runtime", "org.gradle.docstype": "sources" })
            )),
            None
        );
    }

    #[test]
    fn platforms_unversioned_dependencies_and_variants_without_a_usage() {
        let mut repo = Fake::default();
        let marker = "<!-- do_not_remove: published-with-gradle-metadata -->";
        let module =
            |variants: Value| serde_json::to_vec(&json!({ "variants": variants })).unwrap();
        repo.pom(
            "kx",
            "bom",
            "1",
            "<packaging>pom</packaging><dependencyManagement><dependencies><dependency><groupId>kx</groupId><artifactId>other</artifactId><version>3</version></dependency></dependencies></dependencyManagement>",
        );
        repo.pom("kx", "core", "1", marker)
            .file("kx", "core", "1", "jar");
        repo.0.insert(
            "kx/core/1/core-1.module".into(),
            module(json!([
                { "name": "runtime", "attributes": { "org.gradle.category": "library", "org.gradle.usage": "java-runtime" },
                  "dependencies": [
                      { "group": "kx", "module": "bom", "version": { "requires": "1" }, "attributes": { "org.gradle.category": "platform" } },
                      { "group": "org.jetbrains.kotlin", "module": "kotlin-stdlib-common", "version": { "requires": "2.2.10" } },
                      { "group": "kx", "module": "unversioned" },
                  ],
                  "dependencyConstraints": [{ "group": "kx", "module": "unversioned", "version": { "requires": "4" } }],
                  "files": [{ "name": "core-1.jar", "url": "core-1.jar" }] },
            ])),
        );
        repo.lib("kx", "unversioned", "4", "jar", &[]);
        // Kotlin's stdlib-common since 1.9.20: one variant without a usage
        // and without files, which Gradle takes.
        repo.pom(
            "org.jetbrains.kotlin",
            "kotlin-stdlib-common",
            "2.2.10",
            marker,
        );
        repo.0.insert(
            "org/jetbrains/kotlin/kotlin-stdlib-common/2.2.10/kotlin-stdlib-common-2.2.10.module".into(),
            module(json!([{ "name": "stdlibCommonElements", "attributes": { "org.gradle.category": "library" } }])),
        );
        repo.lib("app", "a", "1", "aar", &["kx:core:1", "kx:other:1"])
            .lib("kx", "other", "1", "jar", &[])
            .lib("kx", "other", "3", "jar", &[]);
        let resolved = resolve(&mut repo, &[coordinate("app:a:1")]).unwrap();
        assert_eq!(
            names(&resolved),
            [
                "app:a:1",
                "kx:core:1",
                "kx:other:3",
                "kx:bom:1",
                "org.jetbrains.kotlin:kotlin-stdlib-common:2.2.10",
                "kx:unversioned:4"
            ]
        );
        assert!(resolved[4].files.is_empty());
        // The same BOM asked for as a library constrains nothing.
        let mut plain = Fake(repo.0.clone());
        plain.lib("app", "b", "1", "aar", &["kx:bom:1", "kx:other:1"]);
        let resolved = resolve(&mut plain, &[coordinate("app:b:1")]).unwrap();
        assert_eq!(names(&resolved), ["app:b:1", "kx:bom:1", "kx:other:1"]);
        // A dependency nothing gives a version is an error.
        let mut lone = Fake(repo.0.clone());
        lone.pom("kx", "lone", "1", marker)
            .file("kx", "lone", "1", "jar");
        lone.0.insert(
            "kx/lone/1/lone-1.module".into(),
            module(
                json!([{ "name": "runtime", "attributes": { "org.gradle.usage": "java-runtime" },
                "dependencies": [{ "group": "kx", "module": "nowhere" }] }]),
            ),
        );
        let error = format!(
            "{:#}",
            resolve(&mut lone, &[coordinate("kx:lone:1")]).unwrap_err()
        );
        assert!(error.contains("kx:nowhere names no version"), "{error}");
    }

    #[test]
    fn ranges_take_the_highest_listed_version_they_allow() {
        let mut repo = Fake::default();
        repo.pom(
            "app",
            "a",
            "1",
            "<packaging>aar</packaging><dependencies><dependency><groupId>lib</groupId><artifactId>r</artifactId><version>[1.0,2.0)</version></dependency></dependencies>",
        )
        .file("app", "a", "1", "aar")
        .lib("lib", "r", "1.5", "jar", &[]);
        repo.0.insert(
            "lib/r/maven-metadata.xml".into(),
            b"<metadata><versioning><versions><version>1.0</version><version>1.5</version><version>2.0</version></versions></versioning></metadata>".to_vec(),
        );
        let resolved = resolve(&mut repo, &[coordinate("app:a:1")]).unwrap();
        assert_eq!(names(&resolved), ["app:a:1", "lib:r:1.5"]);
    }

    #[test]
    fn what_cannot_be_resolved_says_which_library() {
        let mut repo = Fake::default();
        repo.lib("app", "a", "1", "aar", &["lib:missing:1"]);
        let error = format!(
            "{:#}",
            resolve(&mut repo, &[coordinate("app:a:1")]).unwrap_err()
        );
        assert!(
            error.contains("app:a:1") && error.contains("lib/missing/1/missing-1.pom"),
            "{error}"
        );
        // A file outside the version's directory is refused.
        let bad = from_module_metadata(
            &Module::new("a", "b"),
            "1",
            &json!({ "variants": [{ "attributes": { "org.gradle.usage": "java-runtime" }, "files": [{ "name": "x", "url": "../../x" }] }] }),
        );
        assert!(bad.is_err());
    }

    #[test]
    fn vendoring_writes_the_files_the_record_and_nsh_json() {
        let app = std::env::temp_dir().join(format!("nsh-cli-maven-{}", std::process::id()));
        let _ = fs::remove_dir_all(&app);
        fs::create_dir_all(&app).unwrap();
        fs::write(
            app.join("nsh.json"),
            r#"{ "name": "A", "android": { "libraries": ["java/own.jar"] } }"#,
        )
        .unwrap();
        let mut repo = Fake::default();
        repo.lib("app", "a", "1", "aar", &["lib:x:1"])
            .lib("lib", "x", "1", "jar", &[])
            .lib("app", "a", "2", "aar", &[]);
        let x = hex(&sha2::Sha256::digest(b"x-1"));
        repo.0.insert(
            "lib/x/1/x-1.jar.sha256".into(),
            format!("{x}  x-1.jar").into_bytes(),
        );
        vendor(&app, &mut repo, &[coordinate("app:a:1")], Vec::new()).unwrap();
        let config: Value =
            serde_json::from_str(&fs::read_to_string(app.join("nsh.json")).unwrap()).unwrap();
        assert_eq!(
            config["android"]["libraries"],
            json!([
                "java/own.jar",
                "android/maven/app/a-1.aar",
                "android/maven/lib/x-1.jar"
            ])
        );
        assert_eq!(
            fs::read(app.join("android/maven/lib/x-1.jar")).unwrap(),
            b"x-1"
        );
        let record: Record =
            serde_json::from_slice(&fs::read(app.join("android/maven/maven.json")).unwrap())
                .unwrap();
        assert_eq!(record.libraries, ["app:a:1"]);
        assert_eq!(record.resolved[1].sha256, x);
        // Upgrading replaces the old files and entries.
        vendor(&app, &mut repo, &[coordinate("app:a:2")], Vec::new()).unwrap();
        let config: Value =
            serde_json::from_str(&fs::read_to_string(app.join("nsh.json")).unwrap()).unwrap();
        assert_eq!(
            config["android"]["libraries"],
            json!(["java/own.jar", "android/maven/app/a-2.aar"])
        );
        assert!(!app.join("android/maven/lib").exists());
        // A checksum that does not match stops it, and changes nothing.
        repo.0
            .insert("app/a/2/a-2.aar.sha1".into(), b"0000".to_vec());
        let error = vendor(&app, &mut repo, &[coordinate("app:a:2")], Vec::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("SHA-1"), "{error}");
        assert!(app.join("android/maven/app/a-2.aar").exists());
        fs::remove_dir_all(&app).unwrap();
    }
}

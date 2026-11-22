//! Android libraries the app's crates ship (`android.libraries` in
//! `[package.metadata.nsh]`: `.aar` and `.jar` files in the crate), packaged
//! without Gradle: their classes dexed with the SDK's `d8`, their resources
//! linked as `aapt2` overlays with an `R` class per library, their manifests
//! merged element by element, and their Java resources, native libraries and
//! assets copied.

use std::{
    collections::BTreeMap,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};

/// A library unpacked in the build's work directory.
#[derive(Debug, Clone)]
pub(crate) struct Library {
    /// Its file, for errors.
    pub(crate) source: PathBuf,
    /// Where it was unpacked: `classes.jar`, `res/`, `AndroidManifest.xml`,
    /// `R.txt`, `jni/`, `assets/`, `libs/*.jar` as an `.aar` lays them out
    /// (a `.jar` is `classes.jar` alone).
    pub(crate) dir: PathBuf,
}

impl Library {
    /// Unpacks `source` (an `.aar` or a `.jar`) into `dir`.
    pub(crate) fn unpack(source: &Path, dir: &Path) -> anyhow::Result<Self> {
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        std::fs::create_dir_all(dir)?;
        let is_jar = source.extension().is_some_and(|e| e == "jar");
        if is_jar {
            std::fs::copy(source, dir.join("classes.jar"))
                .with_context(|| format!("copy {}", source.display()))?;
        } else {
            let file = std::fs::File::open(source)
                .with_context(|| format!("open {}", source.display()))?;
            let mut zip = zip::ZipArchive::new(file)
                .with_context(|| format!("{} is no .aar (a zip)", source.display()))?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                let Some(name) = entry.enclosed_name() else {
                    bail!("{}: an entry leads outside it", source.display());
                };
                let to = dir.join(name);
                if entry.is_dir() {
                    std::fs::create_dir_all(&to)?;
                    continue;
                }
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                std::fs::write(&to, bytes)?;
            }
            if !dir.join("AndroidManifest.xml").is_file() {
                bail!(
                    "{} is no .aar: it has no AndroidManifest.xml",
                    source.display()
                );
            }
        }
        Ok(Library {
            source: source.to_path_buf(),
            dir: dir.to_path_buf(),
        })
    }

    /// Its `.aar`'s metadata (`aar-metadata.properties`), as key and value.
    fn metadata(&self) -> BTreeMap<String, String> {
        std::fs::read_to_string(
            self.dir
                .join("META-INF/com/android/build/gradle/aar-metadata.properties"),
        )
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
    }

    /// The oldest platform it compiles against (`minCompileSdk`).
    pub(crate) fn min_compile_sdk(&self) -> u32 {
        self.metadata()
            .get("minCompileSdk")
            .and_then(|v| v.split('.').next()?.parse().ok())
            .unwrap_or(0)
    }

    /// Checks NSH can package it: no core library desugaring, which needs
    /// a step NSH does not run.
    pub(crate) fn check(&self) -> anyhow::Result<()> {
        if self
            .metadata()
            .get("coreLibraryDesugaringEnabled")
            .is_some_and(|v| v == "true")
        {
            bail!(
                "{} needs core library desugaring, which NSH does not run",
                self.source.display()
            );
        }
        Ok(())
    }

    /// Its manifest's `package`: the package of its `R` class.
    pub(crate) fn package(&self) -> anyhow::Result<Option<String>> {
        let path = self.dir.join("AndroidManifest.xml");
        if !path.is_file() {
            return Ok(None);
        }
        let root = parse(&std::fs::read_to_string(&path)?)
            .with_context(|| format!("{}: AndroidManifest.xml", self.source.display()))?;
        Ok(root.attr("package").map(str::to_owned))
    }

    /// Its jars: `classes.jar` and `libs/*.jar`.
    pub(crate) fn jars(&self) -> Vec<PathBuf> {
        let mut jars = Vec::new();
        if self.dir.join("classes.jar").is_file() {
            jars.push(self.dir.join("classes.jar"));
        }
        let mut more: Vec<PathBuf> = std::fs::read_dir(self.dir.join("libs"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jar"))
            .collect();
        more.sort();
        jars.extend(more);
        jars
    }
}

// ---------------------------------------------------------------- XML ----

/// An XML element: its name, its attributes as written (values not
/// unescaped, so they are written back as they were), its child elements.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Element {
    pub(crate) name: String,
    pub(crate) attrs: Vec<(String, String)>,
    pub(crate) children: Vec<Element>,
}

impl Element {
    pub(crate) fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn set_attr(&mut self, key: &str, value: &str) {
        match self.attrs.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value.to_owned(),
            None => self.attrs.push((key.to_owned(), value.to_owned())),
        }
    }

    /// What identifies it among its siblings for a merge: its name and
    /// `android:name` (an `<intent-filter>` has none, and is never merged
    /// with another).
    fn key(&self) -> Option<(String, String)> {
        self.attr("android:name")
            .map(|n| (self.name.clone(), n.to_owned()))
    }

    /// It written out, `tools:` attributes and the `tools` namespace left
    /// out (NSH runs no manifest merger, and `aapt2` refuses an unbound
    /// prefix).
    pub(crate) fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (key, value) in &self.attrs {
            if key.starts_with("tools:") || key == "xmlns:tools" {
                continue;
            }
            out.push_str(&format!(" {key}=\"{value}\""));
        }
        if self.children.is_empty() {
            out.push_str(" />");
            return;
        }
        out.push('>');
        for child in &self.children {
            child.write(out);
        }
        out.push_str(&format!("</{}>", self.name));
    }
}

/// The root element of `xml`; text, comments, the declaration and
/// `<!DOCTYPE>` left out.
pub(crate) fn parse(xml: &str) -> anyhow::Result<Element> {
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        for (open, close) in [
            ("<!--", "-->"),
            ("<![CDATA[", "]]>"),
            ("<?", "?>"),
            ("<!", ">"),
        ] {
            if let Some(after) = rest.strip_prefix(open) {
                let end = after
                    .find(close)
                    .with_context(|| format!("an unclosed `{open}`"))?;
                rest = &after[end + close.len()..];
                break;
            }
        }
        if !rest.starts_with('<') || rest.starts_with("<!") || rest.starts_with("<?") {
            continue;
        }
        // The tag, to its `>` outside quotes.
        let mut quote = None;
        let end = rest
            .char_indices()
            .find(|&(_, c)| match quote {
                Some(q) => {
                    if c == q {
                        quote = None;
                    }
                    false
                }
                None if c == '"' || c == '\'' => {
                    quote = Some(c);
                    false
                }
                None => c == '>',
            })
            .map(|(i, _)| i)
            .context("an unclosed tag")?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            let done = stack.pop().context("a closing tag with no element open")?;
            if done.name != name.trim() {
                bail!("<{}> closed by </{}>", done.name, name.trim());
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => root = Some(done),
            }
            continue;
        }
        let closed = tag.trim_end().ends_with('/');
        let tag = tag.trim_end().trim_end_matches('/');
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let element = Element {
            name: tag[..name_end].to_owned(),
            attrs: attributes(&tag[name_end..])?,
            children: Vec::new(),
        };
        if closed {
            match stack.last_mut() {
                Some(parent) => parent.children.push(element),
                None => root = Some(element),
            }
        } else {
            stack.push(element);
        }
    }
    if let Some(open) = stack.last() {
        bail!("<{}> is not closed", open.name);
    }
    root.context("no element")
}

/// The attributes of a tag past its name (`a="1" b='2'`).
fn attributes(mut rest: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut attrs = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Ok(attrs);
        }
        let eq = rest.find('=').context("an attribute without a value")?;
        let key = rest[..eq].trim().to_owned();
        let value = rest[eq + 1..].trim_start();
        let quote = value
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
            .context("an unquoted attribute value")?;
        let close = value[1..]
            .find(quote)
            .context("an unclosed attribute value")?;
        attrs.push((key, value[1..1 + close].replace('"', "&quot;")));
        rest = &value[close + 2..];
    }
}

// ----------------------------------------------------------- manifests ----

/// What the libraries' manifests add to the app's: the elements under
/// `<manifest>` (permissions, `<uses-feature>`), `<queries>`, and the
/// components of `<application>` merged with the app's own.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Manifest {
    /// `<uses-permission>`, `<permission>` and `<uses-feature>` elements
    /// the app does not declare already, written out.
    pub(crate) top: Vec<String>,
    pub(crate) queries: Vec<String>,
    /// Every component of `<application>`: the app's and its crates' (as
    /// they are, or merged with a library's of the same name), then the
    /// libraries'. When `replaces_app`, written instead of the app's.
    pub(crate) application: Vec<String>,
    pub(crate) replaces_app: bool,
    /// What was left out, for a note.
    pub(crate) notes: Vec<String>,
}

/// A `tools:node="remove"` or `"removeAll"` a library asks for, applied
/// once every library is merged, so the order they are listed in does not
/// change what is removed.
#[derive(Debug, Clone, PartialEq)]
struct Removal {
    /// The component it is in (`None`: under `<manifest>`, `<queries>` or
    /// `<application>`).
    parent: Option<(String, String)>,
    /// The kind of element it removes, and which.
    element: String,
    what: Removes,
}

/// Which elements of a kind a [`Removal`] removes.
#[derive(Debug, Clone, PartialEq)]
enum Removes {
    /// The one of this `android:name`.
    Named(String),
    /// One with no name, equal to this one (`tools:` attributes aside): an
    /// `<intent-filter>`, a `<uses-feature android:glEsVersion>`.
    Like(Element),
    /// Every one (`removeAll`).
    All,
}

impl Removal {
    /// The removal `element` (with `tools:node="remove"` or `"removeAll"`)
    /// asks for, in `parent`.
    fn of(element: &Element, parent: Option<(String, String)>) -> Self {
        let what = if element.attr("tools:node") == Some("removeAll") {
            Removes::All
        } else if let Some(name) = element.attr("android:name") {
            Removes::Named(name.to_owned())
        } else {
            Removes::Like(without_tools(element))
        };
        Removal {
            parent,
            element: element.name.clone(),
            what,
        }
    }

    fn matches(&self, element: &Element) -> bool {
        element.name == self.element
            && match &self.what {
                Removes::Named(name) => element.attr("android:name") == Some(name.as_str()),
                Removes::Like(like) => without_tools(element) == *like,
                Removes::All => true,
            }
    }
}

/// Whether `element` asks for a removal (`tools:node="remove"` or
/// `"removeAll"`).
fn is_removal(element: &Element) -> bool {
    matches!(element.attr("tools:node"), Some("remove" | "removeAll"))
}

/// `element` without its `tools:` attributes, at every depth.
fn without_tools(element: &Element) -> Element {
    Element {
        name: element.name.clone(),
        attrs: element
            .attrs
            .iter()
            .filter(|(k, _)| !k.starts_with("tools:") && k != "xmlns:tools")
            .cloned()
            .collect(),
        children: element.children.iter().map(without_tools).collect(),
    }
}

/// Puts the app's package in `${applicationId}` of `element`'s attribute
/// values (comments and text are not read), refusing another placeholder.
fn fill_placeholders(element: &mut Element, package: &str) -> anyhow::Result<()> {
    for (_, value) in &mut element.attrs {
        *value = value.replace("${applicationId}", package);
        if let Some(at) = value.find("${") {
            let end = value[at..].find('}').map_or(value.len(), |e| at + e + 1);
            bail!(
                "its manifest needs the placeholder {}, which NSH does not fill",
                &value[at..end]
            );
        }
    }
    for child in &mut element.children {
        fill_placeholders(child, package)?;
    }
    Ok(())
}

/// Merges the manifests of `libraries` into what they add to the app
/// `package`'s, whose manifest already declares `app_permissions` (NSH's
/// and the app's) and `app_application` (the components the app and its
/// crates declare, whose attribute values win over a library's of the same
/// name and which are merged with it).
pub(crate) fn merge_manifests(
    libraries: &[Library],
    package: &str,
    app_permissions: &[String],
    app_application: &[String],
) -> anyhow::Result<Manifest> {
    let mut manifest = Manifest::default();
    let mut top: Vec<(Element, PathBuf)> = Vec::new();
    let mut queries: Vec<Element> = Vec::new();
    let mut application: Vec<(Element, PathBuf)> = Vec::new();
    let mut removals: Vec<Removal> = Vec::new();
    for library in libraries {
        let path = library.dir.join("AndroidManifest.xml");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let source = &library.source;
        let context = || format!("{}: AndroidManifest.xml", source.display());
        let mut root = parse(&text).with_context(context)?;
        fill_placeholders(&mut root, package).with_context(context)?;
        for child in root.children {
            match child.name.as_str() {
                "uses-sdk" => {
                    if let Some(min) = child.attr("android:minSdkVersion")
                        && min
                            .parse::<u32>()
                            .is_ok_and(|m| m > crate::android::MIN_SDK)
                    {
                        bail!(
                            "{} needs Android API {min}, above NSH's minimum, {}",
                            source.display(),
                            crate::android::MIN_SDK
                        );
                    }
                }
                "uses-permission" | "uses-permission-sdk-23" | "uses-feature" | "permission" => {
                    merge_top(&mut top, &mut removals, child, source)?;
                }
                "queries" => {
                    for query in child.children {
                        if is_removal(&query) {
                            removals.push(Removal::of(&query, None));
                        } else if !queries
                            .iter()
                            .any(|q| without_tools(q) == without_tools(&query))
                        {
                            queries.push(query);
                        }
                    }
                }
                "application" => {
                    if child.attrs.iter().any(|(k, _)| !k.starts_with("tools:")) {
                        manifest.notes.push(format!(
                            "{}: the attributes of its <application> are left out",
                            source.display()
                        ));
                    }
                    for component in child.children {
                        merge_component(&mut application, &mut removals, None, component, source)?;
                    }
                }
                other => manifest.notes.push(format!(
                    "{}: <{other}> of its manifest is left out",
                    source.display()
                )),
            }
        }
    }
    apply_removals(&mut application, &mut top, &removals);
    queries.retain(|q| !removals.iter().any(|r| r.parent.is_none() && r.matches(q)));
    // The app's components first, each merged with a library's of its
    // name, its own values winning; the libraries' others after.
    let mut merged: Vec<(Element, Option<PathBuf>)> = Vec::new();
    let mut unread = Vec::new();
    for entry in app_application {
        let Ok(mut own) = parse(entry) else {
            unread.push(entry.trim().to_owned());
            continue;
        };
        let mut from = None;
        if let Some(key) = own.key()
            && let Some(at) = application
                .iter()
                .position(|(e, _)| e.key().as_ref() == Some(&key))
        {
            let (library, source) = application.remove(at);
            own = merge_app_over(own, library);
            from = Some(source);
        }
        merged.push((own, from));
    }
    merged.extend(application.into_iter().map(|(e, s)| (e, Some(s))));
    // As it reaches Android: a library's component with a filter needs
    // `exported`, which the app's entry of its name may give it.
    for (component, source) in &merged {
        let Some(source) = source else { continue };
        let exported = component.attr("android:exported").is_some();
        if !exported && component.children.iter().any(|c| c.name == "intent-filter") {
            bail!(
                "{}: <{} android:name=\"{}\"> has an <intent-filter> but no android:exported, which Android 12 and later need",
                source.display(),
                component.name,
                component.attr("android:name").unwrap_or("?")
            );
        }
    }
    manifest.application = unread
        .into_iter()
        .chain(merged.iter().map(|(e, _)| e.to_xml()))
        .collect();
    manifest.replaces_app = true;
    manifest.top = top
        .iter()
        .filter(|(e, _)| {
            !(e.name == "uses-permission"
                && e.attr("android:name")
                    .is_some_and(|n| app_permissions.iter().any(|p| p == n)))
        })
        .map(|(e, _)| e.to_xml())
        .collect();
    manifest.queries = queries.iter().map(Element::to_xml).collect();
    Ok(manifest)
}

/// Merges an element under `<manifest>` into `top`, one per kind and name
/// (or per equal element, for one without a name): for a permission two
/// libraries ask for, the broader `maxSdkVersion` (none when either has
/// none); a feature is required when either requires it; a `<permission>`
/// two libraries define differently is an error.
fn merge_top(
    top: &mut Vec<(Element, PathBuf)>,
    removals: &mut Vec<Removal>,
    element: Element,
    source: &Path,
) -> anyhow::Result<()> {
    if is_removal(&element) {
        removals.push(Removal::of(&element, None));
        return Ok(());
    }
    let Some(key) = element.key() else {
        if !top
            .iter()
            .any(|(e, _)| without_tools(e) == without_tools(&element))
        {
            top.push((element, source.to_path_buf()));
        }
        return Ok(());
    };
    let Some((have, first)) = top.iter_mut().find(|(e, _)| e.key().as_ref() == Some(&key)) else {
        top.push((element, source.to_path_buf()));
        return Ok(());
    };
    match element.name.as_str() {
        "uses-permission" | "uses-permission-sdk-23" => {
            let max = |e: &Element| {
                e.attr("android:maxSdkVersion")
                    .and_then(|v| v.parse::<u32>().ok())
            };
            match (max(have), max(&element)) {
                (Some(a), Some(b)) => have.set_attr("android:maxSdkVersion", &a.max(b).to_string()),
                _ => have.attrs.retain(|(k, _)| k != "android:maxSdkVersion"),
            }
        }
        "uses-feature" => {
            if element.attr("android:required") != Some("false") {
                have.attrs.retain(|(k, _)| k != "android:required");
            }
        }
        _ => {
            if without_tools(have) != without_tools(&element) {
                bail!(
                    "{} and {} define <{} android:name=\"{}\"> differently",
                    first.display(),
                    source.display(),
                    element.name,
                    key.1
                );
            }
        }
    }
    Ok(())
}

/// Merges `component` (of `source`) into `into`, the children of `parent`:
/// an element of the same name and `android:name` takes its attributes
/// (the same value, or one it lacks; `tools:replace` lets it change one,
/// `tools:remove` drops one) and children, merged the same way.
/// `tools:node="replace"` puts it in place of that element; `"remove"` and
/// `"removeAll"` are noted in `removals`.
fn merge_component(
    into: &mut Vec<(Element, PathBuf)>,
    removals: &mut Vec<Removal>,
    parent: Option<(String, String)>,
    mut component: Element,
    source: &Path,
) -> anyhow::Result<()> {
    let key = component.key();
    if is_removal(&component) {
        removals.push(Removal::of(&component, parent));
        return Ok(());
    }
    let existing = key
        .as_ref()
        .and_then(|k| into.iter().position(|(e, _)| e.key().as_ref() == Some(k)));
    let Some(at) = existing else {
        if key.is_none()
            && into
                .iter()
                .any(|(e, _)| without_tools(e) == without_tools(&component))
        {
            return Ok(());
        }
        // Its children go through the same rules: a removal in them is
        // noted, never written.
        let mut children = Vec::new();
        for child in std::mem::take(&mut component.children) {
            merge_component(&mut children, removals, key.clone(), child, source)?;
        }
        component.children = children.into_iter().map(|(c, _)| c).collect();
        into.push((component, source.to_path_buf()));
        return Ok(());
    };
    // A `replace` takes the place of what it names, whichever library
    // comes first; it keeps its marker (never written out), so a later
    // declaration of the name leaves it as it is.
    if component.attr("tools:node") == Some("replace") {
        into[at] = (component, source.to_path_buf());
        return Ok(());
    }
    if into[at].0.attr("tools:node") == Some("replace") {
        return Ok(());
    }
    let (have, first) = &mut into[at];
    // `tools:replace` and `tools:remove` count from whichever side has
    // them, and stay on the merged element (never written out) for a later
    // library.
    let replace_new = marked(&component, "tools:replace");
    let replace_old = marked(have, "tools:replace");
    let mut remove = marked(have, "tools:remove");
    remove.extend(marked(&component, "tools:remove"));
    // Required (Android's default) when either requires it.
    let optional = |e: &Element| e.attr("android:required") == Some("false");
    if matches!(
        have.name.as_str(),
        "uses-library" | "uses-native-library" | "uses-feature"
    ) {
        if !optional(have) || !optional(&component) {
            have.attrs.retain(|(k, _)| k != "android:required");
        }
        component.attrs.retain(|(k, _)| k != "android:required");
    }
    for (attr, value) in &component.attrs {
        if attr.starts_with("tools:") {
            continue;
        }
        match have.attr(attr) {
            Some(same) if same == value => {}
            Some(_) if replace_new.contains(attr) => have.set_attr(attr, value),
            Some(_) if replace_old.contains(attr) => {}
            Some(other) => bail!(
                "{} and {} declare <{} android:name=\"{}\"> with {attr}=\"{other}\" and \"{value}\"",
                first.display(),
                source.display(),
                have.name,
                have.attr("android:name").unwrap_or("?")
            ),
            None => have.set_attr(attr, value),
        }
    }
    let mut replace = replace_old;
    replace.extend(replace_new);
    for (key, list) in [("tools:replace", &replace), ("tools:remove", &remove)] {
        if !list.is_empty() {
            have.set_attr(key, &list.join(","));
        }
    }
    have.attrs.retain(|(k, _)| !remove.contains(k));
    let first = first.clone();
    let mut children: Vec<(Element, PathBuf)> = have
        .children
        .drain(..)
        .map(|c| (c, first.clone()))
        .collect();
    for child in component.children {
        merge_component(&mut children, removals, key.clone(), child, source)?;
    }
    have.children = children.into_iter().map(|(c, _)| c).collect();
    Ok(())
}

/// The attribute names a `tools:replace` or `tools:remove` (`key`) of
/// `element` lists.
fn marked(element: &Element, key: &str) -> Vec<String> {
    element
        .attr(key)
        .map(|r| {
            r.split(',')
                .map(|k| k.trim().to_owned())
                .filter(|k| !k.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Applies the libraries' removals to the merged components and the
/// elements under `<manifest>`.
fn apply_removals(
    application: &mut Vec<(Element, PathBuf)>,
    top: &mut Vec<(Element, PathBuf)>,
    removals: &[Removal],
) {
    let matches = |e: &Element, r: &Removal| r.matches(e);
    for removal in removals {
        match &removal.parent {
            None => {
                top.retain(|(e, _)| !matches(e, removal));
                application_retain(application, removal, &matches);
            }
            Some(parent) => {
                for (component, _) in application.iter_mut() {
                    if component.key().as_ref() == Some(parent) {
                        component.children.retain(|c| !matches(c, removal));
                    }
                }
            }
        }
    }
}

/// Drops from `application` what `removal` (of `<application>`'s own
/// children) names.
fn application_retain(
    application: &mut Vec<(Element, PathBuf)>,
    removal: &Removal,
    matches: &impl Fn(&Element, &Removal) -> bool,
) {
    application.retain(|(e, _)| !matches(e, removal));
}

/// The app's (or a crate's) component `own` merged with a library's of its
/// name: the app's attribute values win, children are joined, the app's
/// child of a name winning over the library's.
fn merge_app_over(own: Element, library: Element) -> Element {
    let mut merged = library;
    for (attr, value) in &own.attrs {
        if !attr.starts_with("tools:") {
            merged.set_attr(attr, value);
        }
    }
    for child in own.children {
        match child.key().and_then(|k| {
            merged
                .children
                .iter()
                .position(|c| c.key().as_ref() == Some(&k))
        }) {
            Some(at) => {
                let library_child = merged.children.remove(at);
                merged
                    .children
                    .insert(at, merge_app_over(child, library_child));
            }
            None if child.key().is_none()
                && merged
                    .children
                    .iter()
                    .any(|c| without_tools(c) == without_tools(&child)) => {}
            None => merged.children.push(child),
        }
    }
    merged
}

// ------------------------------------------------------------- R classes --

/// One symbol of an `R.txt`: `int string app_name 0x7f010000` or
/// `int[] styleable Gauge { 0x7f010000, 0x7f010001 }`.
#[derive(Debug, Clone, PartialEq)]
struct Symbol {
    kind: String,
    name: String,
    value: SymbolValue,
}

#[derive(Debug, Clone, PartialEq)]
enum SymbolValue {
    Int(i32),
    Array(Vec<i32>),
}

fn parse_int(text: &str) -> Option<i32> {
    let text = text.trim();
    match text.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok().map(|v| v as i32),
        None => text.parse().ok(),
    }
}

/// The symbols of an `R.txt`.
fn symbols(text: &str) -> Vec<Symbol> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, ' ');
            let (form, kind, name, value) =
                (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            let value = match form {
                "int" => SymbolValue::Int(parse_int(value)?),
                "int[]" => SymbolValue::Array(
                    value
                        .trim()
                        .trim_start_matches('{')
                        .trim_end_matches('}')
                        .split(',')
                        .filter(|v| !v.trim().is_empty())
                        .map(parse_int)
                        .collect::<Option<Vec<_>>>()?,
                ),
                _ => return None,
            };
            Some(Symbol {
                kind: kind.to_owned(),
                name: name.to_owned(),
                value,
            })
        })
        .collect()
}

/// The `R` class files of the library `package` whose `R.txt` is `own`:
/// its symbols, with the values the app's link gave them (`linked`, the
/// link's `--output-text-symbols`), as the Android Gradle plugin writes
/// them (`BytecodeRClassWriter`): a `pkg/R$<type>.class` per type, `public
/// static final` fields. Returns (path in a jar, bytes).
pub(crate) fn r_classes(package: &str, own: &str, linked: &str) -> Vec<(String, Vec<u8>)> {
    let linked: BTreeMap<(String, String), SymbolValue> = symbols(linked)
        .into_iter()
        .map(|s| ((s.kind, s.name), s.value))
        .collect();
    let mut types: BTreeMap<String, Vec<(String, SymbolValue)>> = BTreeMap::new();
    for symbol in symbols(own) {
        // Only what the app's link has: a name it lacks (a resource of a
        // library not packaged) would alias another resource's id.
        let Some(value) = linked
            .get(&(symbol.kind.clone(), symbol.name.clone()))
            .cloned()
        else {
            continue;
        };
        let fields = types.entry(symbol.kind).or_default();
        if !fields.iter().any(|(n, _)| *n == symbol.name) {
            fields.push((symbol.name, value));
        }
    }
    let base = package.replace('.', "/");
    types
        .into_iter()
        .map(|(kind, fields)| {
            let class = format!("{base}/R${kind}");
            (format!("{class}.class"), class_file(&class, &fields))
        })
        .collect()
}

/// A class file of `class` (internal name) with `public static final int`
/// fields (a `ConstantValue` each) and `int[]` fields filled in a
/// straight-line `<clinit>`, needing no stack map.
fn class_file(class: &str, fields: &[(String, SymbolValue)]) -> Vec<u8> {
    struct Pool {
        bytes: Vec<u8>,
        count: u16,
        utf8: BTreeMap<String, u16>,
        ints: BTreeMap<i32, u16>,
    }
    impl Pool {
        fn utf8(&mut self, text: &str) -> u16 {
            if let Some(i) = self.utf8.get(text) {
                return *i;
            }
            self.bytes.push(1);
            self.bytes.extend((text.len() as u16).to_be_bytes());
            self.bytes.extend(text.as_bytes());
            self.count += 1;
            self.utf8.insert(text.to_owned(), self.count);
            self.count
        }
        fn class(&mut self, name: &str) -> u16 {
            let name = self.utf8(name);
            self.bytes.push(7);
            self.bytes.extend(name.to_be_bytes());
            self.count += 1;
            self.count
        }
        fn int(&mut self, value: i32) -> u16 {
            if let Some(i) = self.ints.get(&value) {
                return *i;
            }
            self.bytes.push(3);
            self.bytes.extend(value.to_be_bytes());
            self.count += 1;
            self.ints.insert(value, self.count);
            self.count
        }
        fn field_ref(&mut self, class: u16, name: &str, descriptor: &str) -> u16 {
            let name = self.utf8(name);
            let descriptor = self.utf8(descriptor);
            self.bytes.push(12);
            self.bytes.extend(name.to_be_bytes());
            self.bytes.extend(descriptor.to_be_bytes());
            self.count += 1;
            let name_and_type = self.count;
            self.bytes.push(9);
            self.bytes.extend(class.to_be_bytes());
            self.bytes.extend(name_and_type.to_be_bytes());
            self.count += 1;
            self.count
        }
    }
    let mut pool = Pool {
        bytes: Vec::new(),
        count: 0,
        utf8: BTreeMap::new(),
        ints: BTreeMap::new(),
    };
    let this = pool.class(class);
    let object = pool.class("java/lang/Object");
    let constant = pool.utf8("ConstantValue");
    let code_name = pool.utf8("Code");
    // Fields: (access, name, descriptor, constant).
    let mut field_bytes = Vec::new();
    let mut code = Vec::new();
    // `public static final`.
    let access: u16 = 0x0001 | 0x0008 | 0x0010;
    let push_int = |code: &mut Vec<u8>, pool: &mut Pool, value: i32| match value {
        -1..=5 => code.push((0x03 + value) as u8),
        -128..=127 => code.extend([0x10, value as u8]),
        -32768..=32767 => {
            code.push(0x11);
            code.extend((value as i16).to_be_bytes());
        }
        _ => {
            let index = pool.int(value);
            code.push(0x13);
            code.extend(index.to_be_bytes());
        }
    };
    for (name, value) in fields {
        let name_index = pool.utf8(name);
        match value {
            SymbolValue::Int(v) => {
                let descriptor = pool.utf8("I");
                let value_index = pool.int(*v);
                field_bytes.extend(access.to_be_bytes());
                field_bytes.extend(name_index.to_be_bytes());
                field_bytes.extend(descriptor.to_be_bytes());
                field_bytes.extend(1u16.to_be_bytes());
                field_bytes.extend(constant.to_be_bytes());
                field_bytes.extend(2u32.to_be_bytes());
                field_bytes.extend(value_index.to_be_bytes());
            }
            SymbolValue::Array(values) => {
                let descriptor = pool.utf8("[I");
                field_bytes.extend(access.to_be_bytes());
                field_bytes.extend(name_index.to_be_bytes());
                field_bytes.extend(descriptor.to_be_bytes());
                field_bytes.extend(0u16.to_be_bytes());
                // length; newarray int; per item: dup, index, value, iastore;
                // putstatic.
                push_int(&mut code, &mut pool, values.len() as i32);
                code.extend([0xbc, 10]);
                for (i, v) in values.iter().enumerate() {
                    code.push(0x59);
                    push_int(&mut code, &mut pool, i as i32);
                    push_int(&mut code, &mut pool, *v);
                    code.push(0x4f);
                }
                let field = pool.field_ref(this, name, "[I");
                code.push(0xb3);
                code.extend(field.to_be_bytes());
            }
        }
    }
    let mut methods = Vec::new();
    let mut method_count = 0u16;
    if !code.is_empty() {
        code.push(0xb1); // return
        let clinit = pool.utf8("<clinit>");
        let void = pool.utf8("()V");
        methods.extend(0x0008u16.to_be_bytes()); // static
        methods.extend(clinit.to_be_bytes());
        methods.extend(void.to_be_bytes());
        methods.extend(1u16.to_be_bytes());
        methods.extend(code_name.to_be_bytes());
        methods.extend((12 + code.len() as u32).to_be_bytes());
        methods.extend(4u16.to_be_bytes()); // max stack
        methods.extend(0u16.to_be_bytes()); // max locals
        methods.extend((code.len() as u32).to_be_bytes());
        methods.extend(&code);
        methods.extend(0u16.to_be_bytes()); // exception table
        methods.extend(0u16.to_be_bytes()); // attributes
        method_count = 1;
    }
    let mut out = Vec::new();
    out.extend(0xCAFE_BABE_u32.to_be_bytes());
    out.extend(0u16.to_be_bytes()); // minor
    out.extend(52u16.to_be_bytes()); // Java 8
    out.extend((pool.count + 1).to_be_bytes());
    out.extend(&pool.bytes);
    out.extend((0x0001u16 | 0x0010 | 0x0020).to_be_bytes()); // public final super
    out.extend(this.to_be_bytes());
    out.extend(object.to_be_bytes());
    out.extend(0u16.to_be_bytes()); // interfaces
    out.extend((fields.len() as u16).to_be_bytes());
    out.extend(field_bytes);
    out.extend(method_count.to_be_bytes());
    out.extend(methods);
    out.extend(0u16.to_be_bytes()); // attributes
    out
}

/// Writes `classes` (path, bytes) into a jar at `path`, which `d8` takes.
pub(crate) fn write_jar(path: &Path, classes: &[(String, Vec<u8>)]) -> anyhow::Result<()> {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path)?);
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in classes {
        zip.start_file(name.as_str(), options)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;
    Ok(())
}

// -------------------------------------------------------- Java resources --

/// Whether a jar entry's path is one the Android Gradle plugin leaves out
/// of a package by default (`PackagingOptionsUtils.kt`): licenses, Maven and
/// ProGuard files, Kotlin metadata.
fn excluded(path: &str) -> bool {
    let upper = path.to_ascii_uppercase();
    path.ends_with(".class")
        || path.ends_with('/')
        || upper == "META-INF/MANIFEST.MF"
        || (upper.starts_with("META-INF/")
            && (upper.ends_with(".SF")
                || upper.ends_with(".DSA")
                || upper.ends_with(".RSA")
                || upper.ends_with(".EC")))
        || upper.starts_with("META-INF/LICENSE")
        || upper.starts_with("META-INF/NOTICE")
        || upper.starts_with("LICENSE")
        || upper.starts_with("NOTICE")
        || path.starts_with("META-INF/maven/")
        || path.starts_with("META-INF/proguard/")
        || path.starts_with("META-INF/com.android.tools/")
        || path.ends_with(".kotlin_metadata")
}

/// Whether `path` at the root of an APK is the package's own, which no
/// Java resource may take.
fn reserved(path: &str) -> bool {
    let dex = path
        .strip_prefix("classes")
        .and_then(|rest| rest.strip_suffix(".dex"))
        .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()));
    dex || path == "AndroidManifest.xml"
        || path == "resources.arsc"
        || ["res/", "lib/", "assets/"]
            .iter()
            .any(|dir| path.starts_with(dir))
}

/// The Java resources of `jars` (each non-class entry the package carries
/// at its root): `META-INF/services/*` files joined, a file two jars hold
/// alike kept once, two that differ an error.
pub(crate) fn java_resources(jars: &[PathBuf]) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let mut out: Vec<(String, Vec<u8>, PathBuf)> = Vec::new();
    for jar in jars {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(jar)?)
            .with_context(|| format!("{} is no jar", jar.display()))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name()?.into_owned();
            if excluded(&name) {
                continue;
            }
            if reserved(&name) {
                bail!(
                    "{} holds {name}, which is a path of the package itself, not a Java resource",
                    jar.display()
                );
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            match out.iter_mut().find(|(n, _, _)| *n == name) {
                None => out.push((name, bytes, jar.clone())),
                Some((_, have, _)) if *have == bytes => {}
                Some((_, have, _)) if name.starts_with("META-INF/services/") => {
                    if !have.ends_with(b"\n") {
                        have.push(b'\n');
                    }
                    have.extend(bytes);
                }
                Some((_, _, first)) => bail!(
                    "{} and {} both hold {name}, with other contents",
                    first.display(),
                    jar.display()
                ),
            }
        }
    }
    Ok(out.into_iter().map(|(n, b, _)| (n, b)).collect())
}

/// Adds the asset `name` of the library `source` to `assets`, where two
/// libraries may hold one alike, never with other contents.
pub(crate) fn add_asset(
    assets: &mut Vec<(String, Vec<u8>)>,
    name: String,
    bytes: Vec<u8>,
    source: &Path,
) -> anyhow::Result<()> {
    match assets.iter().find(|(have, _)| *have == name) {
        Some((_, same)) if *same == bytes => {}
        Some(_) => bail!(
            "two Android libraries hold the asset {name}, with other contents ({} is one)",
            source.display()
        ),
        None => assets.push((name, bytes)),
    }
    Ok(())
}

/// Whether an asset of this name is stored uncompressed, as the Android
/// Gradle plugin keeps files that are compressed already or read in place
/// (`.tflite` models, which are memory-mapped).
pub(crate) fn store_uncompressed(name: &str) -> bool {
    const KEPT: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "webp", "wav", "mp2", "mp3", "ogg", "aac", "mpg", "mpeg",
        "mid", "midi", "smf", "jet", "rtttl", "imy", "xmf", "mp4", "m4a", "m4v", "3gp", "3gpp",
        "3g2", "3gpp2", "amr", "awb", "wma", "wmv", "webm", "mkv", "tflite", "lite", "zip",
    ];
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| KEPT.contains(&e.to_ascii_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `.aar` at `path` with `manifest`, `R.txt` and the entries `more`.
    fn aar(path: &Path, manifest: &str, r: &str, more: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("AndroidManifest.xml", options).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.start_file("R.txt", options).unwrap();
        zip.write_all(r.as_bytes()).unwrap();
        for (name, bytes) in more {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    /// androidx.startup's manifest, as `startup-runtime` 1.1.1 ships it.
    const STARTUP: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!-- Copyright -->
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:tools="http://schemas.android.com/tools"
    package="androidx.startup" >
    <uses-sdk android:minSdkVersion="14" android:targetSdkVersion="31" />
    <application>
        <provider
            android:name="androidx.startup.InitializationProvider"
            android:authorities="${applicationId}.androidx-startup"
            android:exported="false"
            tools:node="merge" />
    </application>
</manifest>"#;

    /// `lifecycle-process` 2.7.0's: the same provider, with its initializer.
    const PROCESS: &str = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:tools="http://schemas.android.com/tools" package="androidx.lifecycle.process" >
    <uses-sdk android:minSdkVersion="19" />
    <application>
        <provider
            android:name="androidx.startup.InitializationProvider"
            android:authorities="${applicationId}.androidx-startup"
            android:exported="false"
            tools:node="merge" >
            <meta-data
                android:name="androidx.lifecycle.ProcessLifecycleInitializer"
                android:value="androidx.startup" />
        </provider>
    </application>
</manifest>"#;

    fn libraries(dir: &Path, manifests: &[(&str, &str)]) -> Vec<Library> {
        manifests
            .iter()
            .map(|(name, manifest)| {
                let path = dir.join(format!("{name}.aar"));
                aar(&path, manifest, "", &[]);
                Library::unpack(&path, &dir.join(name)).unwrap()
            })
            .collect()
    }

    #[test]
    fn manifests_parse_and_write_back() {
        let root = parse(STARTUP).unwrap();
        assert_eq!(root.name, "manifest");
        assert_eq!(root.attr("package"), Some("androidx.startup"));
        let provider = &root.children[1].children[0];
        assert_eq!(provider.name, "provider");
        // `tools:` attributes are not written: aapt2 has no `tools` prefix.
        assert_eq!(
            provider.to_xml(),
            r#"<provider android:name="androidx.startup.InitializationProvider" android:authorities="${applicationId}.androidx-startup" android:exported="false" />"#
        );
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a x=1/>").is_err());
        assert_eq!(parse("<a x='1 > 2'/>").unwrap().attr("x"), Some("1 > 2"));
    }

    #[test]
    fn components_two_libraries_declare_merge_into_one() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-aar-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let libs = libraries(&dir, &[("startup", STARTUP), ("process", PROCESS)]);
        let merged = merge_manifests(&libs, "com.example.app", &[], &[]).unwrap();
        // One provider, with the initializer of the second library, and
        // the app's package in its authority.
        assert_eq!(merged.application.len(), 1, "{merged:?}");
        let provider = &merged.application[0];
        assert!(
            provider.contains(r#"android:authorities="com.example.app.androidx-startup""#),
            "{provider}"
        );
        assert!(
            provider.contains("androidx.lifecycle.ProcessLifecycleInitializer"),
            "{provider}"
        );
        assert!(!provider.contains("tools:"), "{provider}");
        // The app's own component of a name merges with the libraries':
        // its values win, both its and their children stay.
        let app = [r#"<provider android:name="androidx.startup.InitializationProvider" android:authorities="com.example.app.init"><meta-data android:name="com.example.AppInit" android:value="androidx.startup"/></provider>"#.to_owned()];
        let merged = merge_manifests(&libs, "com.example.app", &[], &app).unwrap();
        assert!(merged.replaces_app);
        assert_eq!(merged.application.len(), 1, "{merged:?}");
        let provider = &merged.application[0];
        assert!(
            provider.contains(r#"android:authorities="com.example.app.init""#),
            "{provider}"
        );
        assert!(
            provider.contains("ProcessLifecycleInitializer")
                && provider.contains("com.example.AppInit"),
            "{provider}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_a_merge_cannot_settle_is_an_error() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-aar-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = |inner: &str| {
            format!(
                r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="a.b"><application>{inner}</application></manifest>"#
            )
        };
        let error = |manifests: &[(&str, &str)]| {
            format!(
                "{:#}",
                merge_manifests(&libraries(&dir, manifests), "com.example.app", &[], &[])
                    .unwrap_err()
            )
        };
        // One component, two values of one attribute.
        let a = manifest(r#"<service android:name="x.S" android:exported="false"/>"#);
        let b = manifest(r#"<service android:name="x.S" android:exported="true"/>"#);
        assert!(error(&[("a", &a), ("b", &b)]).contains("android:exported"));
        // A filter without `exported`.
        let c = manifest(
            r#"<receiver android:name="x.R"><intent-filter><action android:name="a"/></intent-filter></receiver>"#,
        );
        assert!(error(&[("c", &c)]).contains("no android:exported"));
        // A placeholder NSH does not fill, and too high a minimum.
        let d = manifest(r#"<meta-data android:name="k" android:value="${apiKey}"/>"#);
        assert!(error(&[("d", &d)]).contains("${apiKey}"));
        let e = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="e"><uses-sdk android:minSdkVersion="30"/></manifest>"#;
        assert!(error(&[("e", e)]).contains("API 30"));
        // A missing attribute is no conflict: `directBootAware` on one.
        let f = manifest(
            r#"<service android:name="x.D" android:exported="false" android:directBootAware="true"/>"#,
        );
        let g = manifest(r#"<service android:name="x.D" android:exported="false"/>"#);
        let merged =
            merge_manifests(&libraries(&dir, &[("f", &f), ("g", &g)]), "p", &[], &[]).unwrap();
        assert!(merged.application[0].contains("directBootAware"));
        // A placeholder in a comment is no attribute's.
        let h = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="h"><!-- ${apiKey} --><application/></manifest>"#;
        assert!(merge_manifests(&libraries(&dir, &[("h", h)]), "p", &[], &[]).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tools_rules_apply_whatever_the_order() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-aar-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = |top: &str, inner: &str| {
            format!(
                r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" xmlns:tools="http://schemas.android.com/tools" package="a.b">{top}<application>{inner}</application></manifest>"#
            )
        };
        let declares = manifest(
            r#"<uses-permission android:name="android.permission.WAKE_LOCK"/><uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30"/>"#,
            r#"<provider android:name="x.Init" android:authorities="a" android:exported="false"><meta-data android:name="x.A" android:value="v"/><meta-data android:name="x.B" android:value="v"/></provider><service android:name="x.S" android:exported="false"/>"#,
        );
        let removes = manifest(
            r#"<uses-permission android:name="android.permission.WAKE_LOCK" tools:node="remove"/><uses-permission android:name="android.permission.BLUETOOTH"/>"#,
            r#"<provider android:name="x.Init" android:authorities="a" android:exported="false"><meta-data android:name="x.B" tools:node="remove"/></provider><service android:name="x.S" android:exported="true" tools:node="replace"/>"#,
        );
        for order in [
            [("one", &declares), ("two", &removes)],
            [("two", &removes), ("one", &declares)],
        ] {
            let merged = merge_manifests(
                &libraries(&dir, &order.map(|(n, m)| (n, m.as_str()))),
                "p",
                &["android.permission.INTERNET".to_owned()],
                &[],
            )
            .unwrap();
            let all = merged.application.join("") + &merged.top.join("");
            // `remove` drops x.B and WAKE_LOCK, never adds a marker.
            assert!(all.contains("x.A") && !all.contains("x.B"), "{all}");
            assert!(
                !all.contains("WAKE_LOCK") && !all.contains("tools:"),
                "{all}"
            );
            // A permission one library limits and another does not: not
            // limited.
            assert!(
                all.contains("BLUETOOTH") && !all.contains("maxSdkVersion"),
                "{all}"
            );
        }
        // `replace` takes the element as it is, with no conflict.
        let merged = merge_manifests(
            &libraries(&dir, &[("one", &declares), ("two", &removes)]),
            "p",
            &[],
            &[],
        )
        .unwrap();
        assert!(
            merged
                .application
                .iter()
                .any(|c| c.contains(r#"x.S" android:exported="true""#)),
            "{merged:?}"
        );
        // A permission the app declares already is not written twice.
        let both = manifest(
            r#"<uses-permission android:name="android.permission.INTERNET"/>"#,
            "",
        );
        let merged = merge_manifests(
            &libraries(&dir, &[("both", &both)]),
            "p",
            &["android.permission.INTERNET".to_owned()],
            &[],
        )
        .unwrap();
        assert!(merged.top.is_empty(), "{merged:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unnamed_elements_and_markers_merge_from_either_side() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-aar-sides-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = |top: &str, inner: &str| {
            format!(
                r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" xmlns:tools="http://schemas.android.com/tools" package="a.b">{top}<application>{inner}</application></manifest>"#
            )
        };
        let filter = |action: &str| {
            format!(r#"<intent-filter><action android:name="{action}"/></intent-filter>"#)
        };
        let declares = manifest(
            r#"<uses-feature android:glEsVersion="0x00030000"/><uses-feature android:name="android.hardware.camera" android:required="false"/><uses-library android:name="org.apache.http.legacy" android:required="false"/><queries><package android:name="com.one"/><package android:name="com.two"/></queries>"#,
            &format!(
                r#"<activity android:name="x.A" android:exported="true" android:theme="@style/One" tools:replace="android:theme">{}{}</activity><service android:name="x.S" android:exported="false" android:process=":one"/><uses-library android:name="com.lib" android:required="false"/>"#,
                filter("x.VIEW"),
                filter("x.SEND")
            ),
        );
        let other = manifest(
            r#"<uses-feature android:name="android.hardware.camera"/><uses-feature android:glEsVersion="0x00020000" tools:node="remove"/><queries><package android:name="com.two" tools:node="remove"/></queries>"#,
            r#"<activity android:name="x.A" android:theme="@style/Two"><intent-filter tools:node="remove"><action android:name="x.SEND"/></intent-filter></activity><service android:name="x.S" tools:remove="android:process"/><uses-library android:name="com.lib"/>"#,
        );
        for order in [
            [("one", &declares), ("two", &other)],
            [("two", &other), ("one", &declares)],
        ] {
            let merged = merge_manifests(
                &libraries(&dir, &order.map(|(n, m)| (n, m.as_str()))),
                "p",
                &[],
                &[],
            )
            .unwrap();
            let top = merged.top.join("");
            let application = merged.application.join("");
            let queries = merged.queries.join("");
            // A nameless `remove` takes the element equal to it, not every
            // element of its kind.
            assert!(top.contains("0x00030000"), "{top}");
            assert!(
                application.contains("x.VIEW") && !application.contains("x.SEND"),
                "{application}"
            );
            assert!(
                queries.contains("com.one") && !queries.contains("com.two"),
                "{queries}"
            );
            // `tools:replace` and `tools:remove` hold from either library.
            assert!(
                application.contains("@style/One") && !application.contains("@style/Two"),
                "{application}"
            );
            assert!(!application.contains(":one"), "{application}");
            // Required when either requires it.
            assert!(
                top.contains("android.hardware.camera") && !top.contains("required"),
                "{top}"
            );
            assert!(!application.contains("required"), "{application}");
            assert!(!(top + &application).contains("tools:"));
        }
        // Two definitions of one permission that differ are an error.
        let permission = |level: &str| {
            manifest(
                &format!(
                    r#"<permission android:name="x.READ" android:protectionLevel="{level}"/>"#
                ),
                "",
            )
        };
        let (signature, normal) = (permission("signature"), permission("normal"));
        let error = merge_manifests(
            &libraries(&dir, &[("one", &signature), ("two", &normal)]),
            "p",
            &[],
            &[],
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("x.READ") && error.contains("differently"),
            "{error}"
        );
        merge_manifests(
            &libraries(&dir, &[("one", &signature), ("two", &signature)]),
            "p",
            &[],
            &[],
        )
        .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_apps_entries_settle_exported_and_keep_one_of_each_filter() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-aar-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let library = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="a.b"><application><activity android:name="x.A"><intent-filter><action android:name="x.VIEW"/></intent-filter></activity></application></manifest>"#;
        let libraries = libraries(&dir, &[("one", library)]);
        let error = merge_manifests(&libraries, "p", &[], &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("android:exported"), "{error}");
        // The app's entry of that name gives it `exported`, and its filter,
        // the library's own, is kept once.
        let merged = merge_manifests(
            &libraries,
            "p",
            &[],
            &[r#"<activity android:name="x.A" android:exported="false"><intent-filter><action android:name="x.VIEW"/></intent-filter></activity>"#.to_owned()],
        )
        .unwrap();
        let application = merged.application.join("");
        assert_eq!(application.matches("x.VIEW").count(), 1, "{application}");
        assert!(
            application.contains(r#"android:exported="false""#),
            "{application}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn libraries_may_share_an_asset_only_alike() {
        let mut assets = Vec::new();
        let source = Path::new("one.aar");
        add_asset(&mut assets, "a.txt".into(), b"a".to_vec(), source).unwrap();
        add_asset(&mut assets, "a.txt".into(), b"a".to_vec(), source).unwrap();
        assert_eq!(assets.len(), 1);
        let error = add_asset(
            &mut assets,
            "a.txt".into(),
            b"b".to_vec(),
            Path::new("two.aar"),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("a.txt") && error.contains("two.aar"),
            "{error}"
        );
    }

    #[test]
    fn a_java_resource_cannot_take_a_path_of_the_package() {
        for path in [
            "AndroidManifest.xml",
            "resources.arsc",
            "classes.dex",
            "classes2.dex",
            "res/raw/x",
            "lib/arm64-v8a/libx.so",
            "assets/x",
        ] {
            assert!(reserved(path), "{path}");
        }
        for path in [
            "classes.dexx",
            "classesX.dex",
            "kotlin/x.kotlin_builtins",
            "META-INF/services/x",
            "okhttp3/publicsuffixes.gz",
        ] {
            assert!(!reserved(path), "{path}");
        }
    }

    #[test]
    fn r_classes_take_the_linked_values() {
        let own = "int string greeting 0x7f010000\nint string gone 0x7f010001\nint[] styleable Gauge { 0x7f020000, 0x7f020001 }\nint styleable Gauge_tint 1\n";
        let linked = "int string greeting 0x7f0a0003\nint[] styleable Gauge { 0x7f020004, 0x7f020005, 0x7f020006 }\nint styleable Gauge_tint 2\nint string other 0x7f0a0004\n";
        let classes = r_classes("com.example.lib", own, linked);
        let names: Vec<&str> = classes.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "com/example/lib/R$string.class",
                "com/example/lib/R$styleable.class"
            ]
        );
        let string = &classes[0].1;
        assert!(string.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE]));
        // The linked value, not the library's own; and only its names.
        assert!(string.windows(4).any(|w| w == 0x7f0a0003_i32.to_be_bytes()));
        assert!(!string.windows(4).any(|w| w == 0x7f010000_i32.to_be_bytes()));
        assert!(!string.windows(5).any(|w| w == b"other"));
        // A name the link does not have is left out, not given a stale id.
        assert!(!string.windows(4).any(|w| w == b"gone"));
        let styleable = &classes[1].1;
        assert!(styleable.windows(8).any(|w| w == b"<clinit>"));
        assert!(
            styleable
                .windows(4)
                .any(|w| w == 0x7f020006_i32.to_be_bytes())
        );
    }

    #[test]
    fn java_resources_merge_as_gradle_merges_them() {
        let dir = std::env::temp_dir().join(format!("nsh-cli-aar-res-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jar = |name: &str, entries: &[(&str, &[u8])]| {
            let path = dir.join(name);
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            for (entry, bytes) in entries {
                zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
            path
        };
        let a = jar(
            "a.jar",
            &[
                ("a/A.class", b"cafe"),
                ("META-INF/MANIFEST.MF", b"x"),
                ("META-INF/proguard/coroutines.pro", b"one"),
                ("META-INF/services/x.Factory", b"a.Impl\n"),
                ("kotlin/kotlin.kotlin_builtins", b"k"),
                ("a.properties", b"v=1"),
            ],
        );
        let b = jar(
            "b.jar",
            &[
                ("META-INF/proguard/coroutines.pro", b"two"),
                ("META-INF/services/x.Factory", b"b.Impl"),
                ("a.properties", b"v=1"),
            ],
        );
        let resources = java_resources(&[a.clone(), b]).unwrap();
        let names: Vec<&str> = resources.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "META-INF/services/x.Factory",
                "kotlin/kotlin.kotlin_builtins",
                "a.properties"
            ]
        );
        assert_eq!(resources[0].1, b"a.Impl\nb.Impl");
        let c = jar("c.jar", &[("a.properties", b"v=2")]);
        assert!(
            java_resources(&[a, c])
                .unwrap_err()
                .to_string()
                .contains("other contents")
        );
        assert!(store_uncompressed("model/text.tflite"));
        assert!(!store_uncompressed("config.json"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

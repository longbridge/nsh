//! Content blocker rules for WebView2, which has no content rule engine.
//!
//! A [`Blocker`] compiles the WebKit content blocker rules an app sets
//! (`WebProps::content_rules`, which macOS and Linux hand to WebKit):
//!
//! - `block` and `ignore-previous-rules` rules become WebView2 request
//!   filtering: each `WebResourceRequested` event is matched against the
//!   rules in the order WebKit applies them, and a blocked request is
//!   answered with `403 Forbidden` instead of its resource, so nothing of
//!   it reaches the page (it is not hidden with CSS).
//! - `css-display-none` rules become a script, injected before the
//!   page's own scripts in the main frame, that hides the selectors whose
//!   domains and URL the page matches: the cosmetic half of the same
//!   rules, which WebKit applies natively.
//!
//! # Strictness
//!
//! This is not WebKit's engine, and it never silently weakens a caller's
//! policy: a rule list with a malformed rule, an unknown key, an action
//! this backend cannot apply (`block-cookies`, `make-https`), or a
//! trigger condition it cannot distinguish (an SVG document or popup
//! resource type) fails the whole set with an error naming the rule. The
//! caller keeps the rules that were already installed and reports the
//! error. A URL filter must be a regular expression this backend's linear
//! matcher supports; WebKit-only constructs (look-around, back references)
//! are rejected explicitly rather than approximated.
//!
//! # What a trigger's domains mean
//!
//! WebKit's `if-domain` and `unless-domain` name the **top-level
//! document's** domain, not the domain of the resource being loaded. A
//! request for a script on `cdn.test` from a page on `site.test` is
//! matched against `*site.test`, not `*cdn.test`. A main-document request
//! is its own top document, while a subframe document is matched against
//! its top document.
//!
//! WebView2 does not identify a document request's frame. The caller
//! therefore passes [`Top`]: `Main` for the top navigation's own document,
//! `Frame` after that document was claimed (with the top URL), and
//! `Unknown` when the backend cannot prove which it is (a redirect before
//! the main document, or a same-URL nested load). A rule that depends on
//! the top document and gets `Unknown` makes the decision
//! [`Decision::Unknown`]; the caller refuses the load rather than applying
//! a guess. Rules that do not use the top document still apply normally.
//!
//! A rule's `url-filter` is WebKit's regular expression. It is compiled
//! by the same engine the app's other regexes use (`regex::bytes`, ASCII
//! case-insensitive unless `url-filter-is-case-sensitive`), so matching a
//! URL is linear in the URL's length whatever the pattern: WebView2 calls
//! the request handler on the UI thread, and a backtracking matcher could
//! be made to do exponential work by a long page URL. A filter's host
//! (`||host`) and the literal runs it proves narrow the rules a request
//! is matched against before any regex runs.

use std::collections::HashMap;

use nsh_core::ContentRuleList;
use serde_json::{Value, json};

/// What a load is, as WebKit's `resource-type` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resource {
    Document,
    StyleSheet,
    Image,
    Media,
    Font,
    Script,
    /// XHR, fetch, WebSocket and everything else: WebKit's `raw`.
    Raw,
    Ping,
}

impl Resource {
    /// The resource type a rule lists, when this backend can tell it
    /// apart. `svg-document` and `popup` cannot be distinguished from
    /// WebView2's contexts, so they are rejected instead of approximated.
    fn parse(name: &str) -> Result<Resource, String> {
        Ok(match name {
            "document" => Resource::Document,
            "style-sheet" => Resource::StyleSheet,
            "image" => Resource::Image,
            "media" => Resource::Media,
            "font" => Resource::Font,
            "script" => Resource::Script,
            "raw" => Resource::Raw,
            "ping" => Resource::Ping,
            "svg-document" | "popup" => {
                return Err(format!(
                    "resource-type `{name}` is not supported by this backend"
                ));
            }
            _ => return Err(format!("unknown resource-type `{name}`")),
        })
    }
}

/// What the top-level document of a request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Top {
    /// The request is the main document: it is its own top document.
    Main,
    /// The request is a resource or a frame document: `page` is the top.
    Frame,
    /// The backend cannot tell whether this document load is the main
    /// frame or a subframe. Rules that depend on the top document are
    /// refused rather than approximated.
    Unknown,
}

/// Whether a load is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    Block,
    Allow,
    /// A matching rule depends on the top document, which is unknown: the
    /// caller refuses the load rather than guessing.
    Unknown,
}

/// The compiled content blocker rules of every list a web view has.
#[derive(Debug, Default)]
pub(crate) struct Blocker {
    lists: Vec<List>,
}

impl Blocker {
    /// Compiles the lists' rules (off the UI thread: a list is tens of
    /// thousands of regexes). A list this backend cannot apply exactly
    /// makes the whole set fail (the caller keeps the rules already
    /// installed and reports it).
    pub(crate) fn compile(rules: &[ContentRuleList]) -> Result<Blocker, String> {
        let mut lists = Vec::with_capacity(rules.len());
        let mut problems = Vec::new();
        for rule in rules {
            match List::parse(&rule.rules) {
                Ok(list) => lists.push(list),
                Err(error) => problems.push(format!("content rule list {}: {error}", rule.id)),
            }
        }
        if problems.is_empty() {
            Ok(Blocker { lists })
        } else {
            Err(problems.join("; "))
        }
    }

    /// Whether the load of `url`, from the page at `page`, is blocked.
    ///
    /// `page` is the top-level document's URL for a resource or frame
    /// load; `top` says whether this load is the main document or whether
    /// the backend could not tell (in which case top-dependent rules are
    /// reported as [`Decision::Unknown`]).
    pub(crate) fn blocks(&self, page: &str, url: &str, resource: Resource, top: Top) -> Decision {
        let target = Target::new(url);
        let top_target = match top {
            Top::Main => Target::new(url),
            Top::Frame => Target::new(page),
            Top::Unknown => Target::new(""),
        };
        let top_known = top != Top::Unknown;
        let mut unknown = false;
        for list in &self.lists {
            match list.blocks(&target, &top_target, resource, top_known) {
                Decision::Block => return Decision::Block,
                Decision::Unknown => unknown = true,
                Decision::Allow => {}
            }
        }
        if unknown {
            Decision::Unknown
        } else {
            Decision::Allow
        }
    }

    /// The script that hides the elements the cosmetic rules name on the
    /// page it runs in, or `None` when no rule hides anything.
    pub(crate) fn cosmetic_script(&self) -> Option<String> {
        let mut lists = Vec::new();
        for list in &self.lists {
            if list.cosmetic.is_empty() {
                continue;
            }
            let steps: Vec<Value> = list.cosmetic.iter().map(CosmeticStep::json).collect();
            lists.push(Value::Array(steps));
        }
        if lists.is_empty() {
            return None;
        }
        let lists = serde_json::to_string(&lists).ok()?;
        Some(format!(
            "(function () {{ var L = {lists};\n{}",
            COSMETIC_BODY
        ))
    }
}

/// What a cosmetic rule does, in the page's script: its trigger's URL
/// filters stay regular expressions for the page's own engine (`RegExp`),
/// as WebKit takes them.
impl CosmeticStep {
    fn json(&self) -> Value {
        let mut step = serde_json::Map::new();
        let trigger = match self {
            CosmeticStep::Hide { selector, trigger } => {
                step.insert("h".into(), json!(selector));
                trigger
            }
            CosmeticStep::Clear { trigger } => {
                step.insert("x".into(), json!(true));
                trigger
            }
        };
        let mut condition = serde_json::Map::new();
        if !trigger.if_domain.is_empty() {
            condition.insert("i".into(), json!(trigger.if_domain));
        }
        if !trigger.unless_domain.is_empty() {
            condition.insert("u".into(), json!(trigger.unless_domain));
        }
        if !trigger.if_top_url.is_empty() {
            let patterns: Vec<&str> = trigger
                .if_top_url
                .iter()
                .map(|filter| filter.pattern.as_str())
                .collect();
            condition.insert("t".into(), json!(patterns));
        }
        if !trigger.unless_top_url.is_empty() {
            let patterns: Vec<&str> = trigger
                .unless_top_url
                .iter()
                .map(|filter| filter.pattern.as_str())
                .collect();
            condition.insert("ut".into(), json!(patterns));
        }
        if trigger.top_case_sensitive
            && (!trigger.if_top_url.is_empty() || !trigger.unless_top_url.is_empty())
        {
            condition.insert("tc".into(), json!(true));
        }
        if trigger.filter.pattern != ".*" {
            condition.insert("f".into(), json!(trigger.filter.pattern));
        }
        if trigger.case_sensitive {
            condition.insert("cs".into(), json!(true));
        }
        step.insert("c".into(), Value::Object(condition));
        Value::Object(step)
    }
}

/// The script after the rules: applies the hide rules of the lists in
/// order (an exception clears what came before it, as WebKit's
/// `ignore-previous-rules` does), then adds a style sheet with the
/// selectors, each rule alone so one selector WebKit cannot parse does not
/// drop the others. URL filters are the rules' own regular expressions,
/// matched by the page as WebKit matches them (case-insensitive unless a
/// rule says otherwise; top URL filters follow the rule's own setting).
const COSMETIC_BODY: &str = r#"var host = location.hostname.toLowerCase(), href = location.href;
function dom(d) {
  d = String(d).toLowerCase();
  if (d.charAt(0) === '*') { var r = d.slice(1); return host === r || host.slice(-(r.length + 1)) === '.' + r; }
  return host === d;
}
function re(p, cs) { try { return new RegExp(p, cs ? '' : 'i'); } catch (e) { return null; } }
function cond(c) {
  if (c.i && c.i.length && !c.i.some(dom)) return false;
  if (c.u && c.u.some(dom)) return false;
  if (c.t && c.t.length && !c.t.some(function (p) { var r = re(p, !!c.tc); return r && r.test(href); })) return false;
  if (c.ut && c.ut.some(function (p) { var r = re(p, !!c.tc); return r && r.test(href); })) return false;
  if (c.f) { var f = re(c.f, c.cs); if (f && !f.test(href)) return false; }
  return true;
}
var css = '';
L.forEach(function (list) {
  var sel = [];
  list.forEach(function (step) {
    if (!cond(step.c)) return;
    if (step.x) { sel = []; } else if (step.h) { sel.push(step.h); }
  });
  sel.forEach(function (s) { css += s + '{display:none !important}\n'; });
});
if (!css) return;
function apply() {
  try {
    var sheet = new CSSStyleSheet();
    sheet.replaceSync(css);
    document.adoptedStyleSheets = document.adoptedStyleSheets.concat([sheet]);
  } catch (e) {
    var style = document.createElement('style');
    style.textContent = css;
    (document.head || document.documentElement).appendChild(style);
  }
}
if (document.documentElement) { apply(); }
else { document.addEventListener('DOMContentLoaded', apply, { once: true }); }
})();"#;

/// The rules of one content rule list, in the order WebKit applies them.
#[derive(Debug, Default)]
struct List {
    rules: Vec<NetRule>,
    /// The rules whose URL filter names a host, by that host.
    by_host: HashMap<String, Vec<u32>>,
    /// The rules whose URL filter names no host, by the first byte of a
    /// literal it requires.
    by_byte: HashMap<u8, Vec<u32>>,
    /// The rules with no evidence of a literal (matched every time).
    wild: Vec<u32>,
    /// The element hiding rules, and the exceptions before them.
    cosmetic: Vec<CosmeticStep>,
}

impl List {
    /// The rules of `json` (a WebKit content blocker rule array), or an
    /// error naming the first rule this backend cannot apply.
    fn parse(json: &str) -> Result<List, String> {
        let rules: Vec<Value> = serde_json::from_str(json)
            .map_err(|error| format!("not a JSON array of rules: {error}"))?;
        let mut list = List::default();
        for (index, rule) in rules.iter().enumerate() {
            let object = rule
                .as_object()
                .ok_or_else(|| format!("rule {index} is not an object"))?;
            for key in object.keys() {
                if key != "trigger" && key != "action" {
                    return Err(format!("rule {index}: unsupported key `{key}`"));
                }
            }
            let trigger = object
                .get("trigger")
                .and_then(Value::as_object)
                .ok_or_else(|| format!("rule {index} has no trigger object"))?;
            let action = object
                .get("action")
                .and_then(Value::as_object)
                .ok_or_else(|| format!("rule {index} has no action object"))?;
            let condition = Condition::parse(trigger, index)?;
            let resource_types = resource_types(trigger, index)?;
            let kind = action
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("rule {index}: the action has no type"))?;
            for key in action.keys() {
                if key != "type" && key != "selector" {
                    return Err(format!("rule {index}: unsupported action key `{key}`"));
                }
            }
            match kind {
                "css-display-none" => {
                    let selector =
                        action
                            .get("selector")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!("rule {index}: `css-display-none` has no selector")
                            })?;
                    valid_selector(selector)
                        .map_err(|error| format!("rule {index}: selector {error}"))?;
                    if condition.load_type.is_some() {
                        return Err(format!(
                            "rule {index}: load-type is not supported for css-display-none"
                        ));
                    }
                    if !resource_types.is_empty() && !resource_types.contains(&Resource::Document) {
                        return Err(format!(
                            "rule {index}: this css-display-none rule does not apply to documents"
                        ));
                    }
                    list.cosmetic.push(CosmeticStep::Hide {
                        selector: selector.to_owned(),
                        trigger: condition,
                    });
                }
                "ignore-previous-rules" => {
                    reject_selector(action, index)?;
                    // An exception for documents undoes the element hiding
                    // rules before it (WebKit's own use of the type). The
                    // cosmetic script cannot evaluate load-type, so such a
                    // rule would silently become unconditional.
                    if applies_to_documents(&resource_types) && condition.load_type.is_some() {
                        return Err(format!(
                            "rule {index}: load-type is not supported for a cosmetic ignore-previous-rules"
                        ));
                    }
                    if applies_to_documents(&resource_types) {
                        list.cosmetic.push(CosmeticStep::Clear {
                            trigger: condition.clone(),
                        });
                    }
                    list.add(NetRule {
                        blocked: false,
                        condition,
                        resource_types,
                    });
                }
                "block" => {
                    reject_selector(action, index)?;
                    list.add(NetRule {
                        blocked: true,
                        condition,
                        resource_types,
                    });
                }
                "block-cookies" | "make-https" => {
                    return Err(format!(
                        "rule {index}: action `{kind}` is not supported by this backend"
                    ));
                }
                _ => return Err(format!("rule {index}: unknown action `{kind}`")),
            }
        }
        Ok(list)
    }

    fn add(&mut self, rule: NetRule) {
        let index = self.rules.len() as u32;
        let bucket = rule.condition.filter.bucket.clone();
        self.rules.push(rule);
        match bucket {
            Bucket::Host(host) => self.by_host.entry(host).or_default().push(index),
            Bucket::Byte(byte) => self.by_byte.entry(byte).or_default().push(index),
            Bucket::Every => self.wild.push(index),
        }
    }

    /// Whether the list blocks the load: its rules apply in order, and a
    /// rule that does not apply changes nothing. A rule whose condition
    /// depends on an unknown top document makes the whole answer unknown.
    fn blocks(
        &self,
        target: &Target,
        top: &Target,
        resource: Resource,
        top_known: bool,
    ) -> Decision {
        let mut candidates: Vec<u32> = Vec::new();
        // A URL without an authority can still match a protocol/path regex.
        if target.host.is_empty() {
            for rules in self.by_host.values() {
                candidates.extend_from_slice(rules);
            }
        }
        for suffix in suffixes(&target.host) {
            if !suffix.is_empty()
                && let Some(rules) = self.by_host.get(suffix)
            {
                candidates.extend_from_slice(rules);
            }
        }
        if !self.by_byte.is_empty() {
            let mut seen = [false; 256];
            for &byte in target.lower.as_bytes() {
                let slot = &mut seen[byte as usize];
                if !*slot {
                    *slot = true;
                    if let Some(rules) = self.by_byte.get(&byte) {
                        candidates.extend_from_slice(rules);
                    }
                }
            }
        }
        candidates.extend_from_slice(&self.wild);
        candidates.sort_unstable();
        candidates.dedup();
        let mut blocked = false;
        let mut unknown = false;
        for index in candidates {
            let rule = &self.rules[index as usize];
            match rule.applies(target, top, resource, top_known) {
                Match::Yes => {
                    blocked = rule.blocked;
                    // A definite later rule decides the current outcome.
                    unknown = false;
                }
                Match::Unknown => {
                    // Introduce uncertainty only when this rule's action
                    // could change the current definite outcome; preserve
                    // uncertainty that is already present.
                    if !unknown && rule.blocked != blocked {
                        unknown = true;
                    }
                }
                Match::No => {}
            }
        }
        if unknown {
            Decision::Unknown
        } else if blocked {
            Decision::Block
        } else {
            Decision::Allow
        }
    }
}

/// A rule's three-valued application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Match {
    Yes,
    No,
    Unknown,
}

/// A load a rule may block or let through.
#[derive(Debug)]
struct NetRule {
    /// What a match does (an `ignore-previous-rules` sets it back).
    blocked: bool,
    condition: Condition,
    resource_types: Vec<Resource>,
}

impl NetRule {
    fn applies(&self, target: &Target, top: &Target, resource: Resource, top_known: bool) -> Match {
        if !self.resource_types.is_empty() && !self.resource_types.contains(&resource) {
            return Match::No;
        }
        // A URL filter mismatch proves the rule inapplicable whatever the
        // top document is.
        if !self.condition.filter.matches(target) {
            return Match::No;
        }
        if !top_known && self.condition.uses_top() {
            return Match::Unknown;
        }
        if self.condition.applies(target, top) {
            Match::Yes
        } else {
            Match::No
        }
    }
}

/// What a rule's trigger asks of a load.
#[derive(Debug, Clone)]
struct Condition {
    filter: UrlFilter,
    case_sensitive: bool,
    if_domain: Vec<String>,
    unless_domain: Vec<String>,
    if_top_url: Vec<UrlFilter>,
    unless_top_url: Vec<UrlFilter>,
    top_case_sensitive: bool,
    /// `Some(true)` for first-party only, `Some(false)` for third-party
    /// only, `None` for every load.
    load_type: Option<bool>,
}

impl Condition {
    fn parse(trigger: &serde_json::Map<String, Value>, index: usize) -> Result<Condition, String> {
        const KEYS: &[&str] = &[
            "url-filter",
            "url-filter-is-case-sensitive",
            "if-domain",
            "unless-domain",
            "if-top-url",
            "unless-top-url",
            "top-url-filter-is-case-sensitive",
            "resource-type",
            "load-type",
        ];
        for key in trigger.keys() {
            if !KEYS.contains(&key.as_str()) {
                return Err(format!("rule {index}: unsupported trigger key `{key}`"));
            }
        }
        let filter = trigger
            .get("url-filter")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("rule {index}: the trigger has no url-filter"))?;
        if filter.is_empty() {
            return Err(format!("rule {index}: url-filter is empty"));
        }
        let case_sensitive = boolean(trigger, "url-filter-is-case-sensitive", index)?;
        let top_case_sensitive = boolean(trigger, "top-url-filter-is-case-sensitive", index)?;
        let filter = UrlFilter::parse(filter, case_sensitive)
            .map_err(|error| format!("rule {index}: url-filter: {error}"))?;
        let if_domain = domains(trigger, "if-domain", index)?;
        let unless_domain = domains(trigger, "unless-domain", index)?;
        let if_top_url = top_patterns(trigger, "if-top-url", top_case_sensitive, index)?;
        let unless_top_url = top_patterns(trigger, "unless-top-url", top_case_sensitive, index)?;
        Ok(Condition {
            filter,
            case_sensitive,
            if_domain,
            unless_domain,
            if_top_url,
            unless_top_url,
            top_case_sensitive,
            load_type: load_types(trigger, index)?,
        })
    }

    /// Whether this condition uses the top-level document (so it cannot be
    /// evaluated when the top document is unknown).
    fn uses_top(&self) -> bool {
        !self.if_domain.is_empty()
            || !self.unless_domain.is_empty()
            || !self.if_top_url.is_empty()
            || !self.unless_top_url.is_empty()
            || self.load_type.is_some()
    }

    /// Whether the rule's trigger matches the load. `top` is the
    /// top-level document (or the document itself, for a main document).
    fn applies(&self, target: &Target, top: &Target) -> bool {
        // WebKit's domains name the top-level document, not the resource.
        if !self.if_domain.is_empty()
            && !self.if_domain.iter().any(|d| domain_matches(&top.host, d))
        {
            return false;
        }
        if self
            .unless_domain
            .iter()
            .any(|d| domain_matches(&top.host, d))
        {
            return false;
        }
        if !self.if_top_url.is_empty() && !self.if_top_url.iter().any(|f| f.matches(top)) {
            return false;
        }
        if self.unless_top_url.iter().any(|f| f.matches(top)) {
            return false;
        }
        if let Some(first_party) = self.load_type {
            if is_first_party(&target.host, &top.host) != first_party {
                return false;
            }
        }
        self.filter.matches(target)
    }
}

/// An element hiding rule, or the exception that clears the ones before
/// it.
#[derive(Debug)]
enum CosmeticStep {
    Hide {
        selector: String,
        trigger: Condition,
    },
    Clear {
        trigger: Condition,
    },
}

/// A WebKit URL filter: its pattern (as WebKit's regex), compiled, and
/// what narrows the rules an event is matched against.
#[derive(Debug, Clone)]
struct UrlFilter {
    /// The pattern as the rule has it (the cosmetic script gets this: the
    /// page matches it as a `RegExp`, as WebKit does).
    pattern: String,
    regex: regex::bytes::Regex,
    case_sensitive: bool,
    /// The host the filter requires (`||host`), when it names one.
    bucket: Bucket,
    /// The literals the filter requires, in order (lowercased unless the
    /// filter is case sensitive).
    literals: Vec<String>,
}

/// What narrows the rules an event is matched against: evidence a rule
/// needs of the load's URL, or every load.
#[derive(Debug, Clone)]
enum Bucket {
    /// On this host (and its subdomains).
    Host(String),
    /// The load's URL has this byte (a literal's first).
    Byte(u8),
    /// Every load.
    Every,
}

impl UrlFilter {
    fn parse(pattern: &str, case_sensitive: bool) -> Result<UrlFilter, String> {
        // WebKit's filters are ASCII regular expressions, case-insensitive
        // unless the rule says otherwise. A construct this matcher does
        // not support fails the list instead of matching something else.
        let regex = regex::bytes::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .unicode(false)
            .build()
            .map_err(|error| {
                format!("{pattern:?} is not a regular expression this backend supports: {error}")
            })?;
        let host = extract_host(pattern);
        // WebKit's filters are ASCII; another pattern's characters do not
        // fold as its (Unicode) case folding would, so it is matched
        // without the literal prefilter.
        let literals = if pattern.is_ascii() {
            required_literals(pattern, case_sensitive)
        } else {
            Vec::new()
        };
        let bucket = match &host {
            Some(host) => Bucket::Host(host.clone()),
            None => {
                if case_sensitive {
                    // A case sensitive literal's first byte need not be in
                    // the lowercased URL: look at every load.
                    Bucket::Every
                } else {
                    literals
                        .iter()
                        .max_by_key(|literal| literal.len())
                        .and_then(|literal| literal.as_bytes().first().copied())
                        .map(Bucket::Byte)
                        .unwrap_or(Bucket::Every)
                }
            }
        };
        Ok(UrlFilter {
            pattern: pattern.to_owned(),
            regex,
            case_sensitive,
            bucket,
            literals,
        })
    }

    /// Whether the URL at `target` matches.
    fn matches(&self, target: &Target) -> bool {
        if let Bucket::Host(host) = &self.bucket
            && !target.host.is_empty()
            && !suffix_matches(&target.host, host)
        {
            return false;
        }
        let text = if self.case_sensitive {
            target.raw
        } else {
            target.lower.as_str()
        };
        for literal in &self.literals {
            if !text.contains(literal.as_str()) {
                return false;
            }
        }
        self.regex.is_match(target.raw.as_bytes())
    }
}

/// A URL, and its host and lowercased text, for matching.
#[derive(Debug)]
struct Target<'a> {
    raw: &'a str,
    lower: String,
    host: String,
}

impl<'a> Target<'a> {
    fn new(url: &'a str) -> Target<'a> {
        let lower = url.to_ascii_lowercase();
        let host = host_of(&lower);
        Target {
            raw: url,
            lower,
            host,
        }
    }
}

/// The host of `url` (which is lowercased), without its user info or
/// port; "" for URLs with no host (data:, about:, blob:, …).
fn host_of(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else {
        return String::new();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(rest) = host.strip_prefix('[') {
        // An IPv6 literal: keep the address so party comparison can treat
        // it as the address it is.
        return match rest.find(']') {
            Some(end) => rest[..end].to_ascii_lowercase(),
            None => String::new(),
        };
    }
    host.split(':')
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_owned()
}

/// The host and every parent of it (`a.b.c` → `a.b.c`, `b.c`, `c`).
fn suffixes(host: &str) -> impl Iterator<Item = &str> + '_ {
    std::iter::successors((!host.is_empty()).then_some(host), |host| {
        host.split_once('.').map(|(_, parent)| parent)
    })
}

/// Whether `host` is `domain` or a subdomain of it.
fn suffix_matches(host: &str, domain: &str) -> bool {
    host == domain
        || (host.len() > domain.len()
            && host.ends_with(domain)
            && host.as_bytes()[host.len() - domain.len() - 1] == b'.')
}

/// Whether `host` meets a content blocker domain (`*example.com`: itself
/// and its subdomains; `example.com`: exactly).
fn domain_matches(host: &str, domain: &str) -> bool {
    if let Some(domain) = domain.strip_prefix('*') {
        !host.is_empty() && suffix_matches(host, domain)
    } else {
        host == domain
    }
}

/// Whether a load from `target` to `top` is first party, by their
/// registrable domains (the public suffix list, so independently hosted
/// sites under `github.io` are different sites).
fn is_first_party(target: &str, top: &str) -> bool {
    if target.is_empty() || top.is_empty() {
        return false;
    }
    let target_ip = target.parse::<std::net::IpAddr>().ok();
    let top_ip = top.parse::<std::net::IpAddr>().ok();
    if target_ip.is_some() || top_ip.is_some() {
        return target_ip == top_ip;
    }
    registrable(target) == registrable(top)
}

fn registrable(host: &str) -> &str {
    psl::domain_str(host).unwrap_or(host)
}

/// The host a `||host` filter names (`^[^:]+://+([^:/]+\.)?host…`), when
/// the pattern names a complete host (not one a class or wildcard
/// continues).
fn extract_host(pattern: &str) -> Option<String> {
    // Alternate branches or inline modes need the general regex path.
    if pattern.contains('|') || pattern.contains("(?") {
        return None;
    }
    const PREFIX: &str = r"^[^:]+://+([^:/]+\.)?";
    let rest = pattern.strip_prefix(PREFIX)?;
    let mut host = String::new();
    let mut at = 0;
    let chars: Vec<char> = rest.chars().collect();
    while let Some(&c) = chars.get(at) {
        match c {
            '\\' => {
                let Some(&next) = chars.get(at + 1) else {
                    break;
                };
                if next == '.' || next == '-' || next == '_' {
                    host.push(next);
                    at += 2;
                } else {
                    break;
                }
            }
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' => {
                host.push(c.to_ascii_lowercase());
                at += 1;
            }
            _ => break,
        }
    }
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') {
        return None;
    }
    // A colon can separate userinfo rather than a port, so only separators
    // that end the authority prove a host. An exhausted pattern does not:
    // its regex still matches a prefix of a longer hostname.
    let rest: String = chars[at..].iter().collect();
    let tail = rest
        .strip_prefix('/')
        .or_else(|| rest.strip_prefix('$'))
        .or_else(|| rest.strip_prefix('#'))
        .or_else(|| rest.strip_prefix(r"\/"))
        .or_else(|| rest.strip_prefix(r"\?"))
        .or_else(|| rest.strip_prefix(r"\#"))?;
    if tail
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '?' | '*' | '{'))
    {
        return None;
    }
    Some(host)
}

/// The literal runs a pattern proves a match needs: top-level literals
/// outside groups and character classes, with a trailing `*`/`?` taking
/// its last character (that atom is optional). A pattern with alternation,
/// a quantifier brace or an escaped class (`\d`, `\w`) proves nothing, so
/// it returns none and its rules are matched every time.
fn required_literals(pattern: &str, case_sensitive: bool) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut run = String::new();
    let mut at = 0;
    fn flush(run: &mut String, out: &mut Vec<String>) {
        if run.len() >= 4 {
            out.push(std::mem::take(run));
        } else {
            run.clear();
        }
    }
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => {
                let Some(&next) = bytes.get(at + 1) else {
                    break;
                };
                if !next.is_ascii() || next.is_ascii_alphanumeric() {
                    // A class (`\d`, `\w`, `\b`) or non-ASCII text, not a
                    // literal: no evidence.
                    return Vec::new();
                }
                run.push(next as char);
                at += 2;
            }
            b'[' => {
                flush(&mut run, &mut out);
                at += 1;
                while at < bytes.len() {
                    match bytes[at] {
                        b'\\' => at += 2,
                        b']' => {
                            at += 1;
                            break;
                        }
                        _ => at += 1,
                    }
                }
            }
            b'(' => {
                flush(&mut run, &mut out);
                at += 1;
                let mut depth = 1;
                while at < bytes.len() && depth > 0 {
                    match bytes[at] {
                        b'\\' => at += 2,
                        b'[' => {
                            // A class inside the group: its `)` is text.
                            at += 1;
                            while at < bytes.len() {
                                match bytes[at] {
                                    b'\\' => at += 2,
                                    b']' => {
                                        at += 1;
                                        break;
                                    }
                                    _ => at += 1,
                                }
                            }
                        }
                        b'(' => {
                            depth += 1;
                            at += 1;
                        }
                        b')' => {
                            depth -= 1;
                            at += 1;
                        }
                        _ => at += 1,
                    }
                }
                if depth != 0 {
                    return Vec::new();
                }
            }
            b'|' | b'{' | b'}' => return Vec::new(),
            b'*' | b'?' => {
                // The atom before it is optional: its last character is
                // not required.
                run.pop();
                flush(&mut run, &mut out);
                at += 1;
            }
            b'.' | b'^' | b'$' | b'+' => {
                flush(&mut run, &mut out);
                at += 1;
            }
            c if c.is_ascii_alphanumeric()
                || c == b'_'
                || c == b'-'
                || c == b'/'
                || c == b':'
                || c == b'%' =>
            {
                run.push(c as char);
                at += 1;
            }
            _ => {
                flush(&mut run, &mut out);
                at += 1;
            }
        }
    }
    flush(&mut run, &mut out);
    if case_sensitive {
        out
    } else {
        out.into_iter()
            .map(|run| run.to_ascii_lowercase())
            .collect()
    }
}

/// The resource types a trigger lists (`resource-type`), when present.
fn resource_types(
    trigger: &serde_json::Map<String, Value>,
    index: usize,
) -> Result<Vec<Resource>, String> {
    let Some(value) = trigger.get("resource-type") else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("rule {index}: resource-type is not an array"))?;
    if values.is_empty() {
        return Err(format!("rule {index}: resource-type is empty"));
    }
    values
        .iter()
        .map(|value| {
            let name = value
                .as_str()
                .ok_or_else(|| format!("rule {index}: resource-type is not a string"))?;
            Resource::parse(name).map_err(|error| format!("rule {index}: {error}"))
        })
        .collect()
}

/// Whether a rule's resource types cover documents (`resource-type`
/// absent covers every type, documents included).
fn applies_to_documents(resource_types: &[Resource]) -> bool {
    resource_types.is_empty() || resource_types.contains(&Resource::Document)
}

/// The first/third party restriction a trigger lists (`load-type`), when
/// present.
fn load_types(
    trigger: &serde_json::Map<String, Value>,
    index: usize,
) -> Result<Option<bool>, String> {
    let Some(value) = trigger.get("load-type") else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("rule {index}: load-type is not an array"))?;
    if values.is_empty() {
        return Err(format!("rule {index}: load-type is empty"));
    }
    let (mut first, mut third) = (false, false);
    for value in values {
        match value.as_str() {
            Some("first-party") => first = true,
            Some("third-party") => third = true,
            Some(other) => return Err(format!("rule {index}: unknown load-type `{other}`")),
            None => return Err(format!("rule {index}: load-type is not a string")),
        }
    }
    Ok(match (first, third) {
        // Both: every load matches, as if the key were absent.
        (true, true) | (false, false) => None,
        (true, false) => Some(true),
        (false, true) => Some(false),
    })
}

/// A trigger's list of domains (`if-domain`, `unless-domain`).
fn domains(
    trigger: &serde_json::Map<String, Value>,
    key: &str,
    index: usize,
) -> Result<Vec<String>, String> {
    let Some(value) = trigger.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("rule {index}: {key} is not an array"))?;
    if values.is_empty() {
        return Err(format!("rule {index}: {key} is empty"));
    }
    values
        .iter()
        .map(|value| {
            let domain = value
                .as_str()
                .ok_or_else(|| format!("rule {index}: {key} is not a domain string"))?;
            valid_domain(domain).map_err(|error| format!("rule {index}: {key}: {error}"))?;
            Ok(domain.to_owned())
        })
        .collect()
}

/// A trigger's list of URL patterns (`if-top-url`, `unless-top-url`).
fn top_patterns(
    trigger: &serde_json::Map<String, Value>,
    key: &str,
    case_sensitive: bool,
    index: usize,
) -> Result<Vec<UrlFilter>, String> {
    let Some(value) = trigger.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("rule {index}: {key} is not an array"))?;
    if values.is_empty() {
        return Err(format!("rule {index}: {key} is empty"));
    }
    values
        .iter()
        .map(|value| {
            let pattern = value
                .as_str()
                .ok_or_else(|| format!("rule {index}: {key} is not a string"))?;
            if pattern.is_empty() {
                return Err(format!("rule {index}: {key} has an empty pattern"));
            }
            UrlFilter::parse(pattern, case_sensitive)
                .map_err(|error| format!("rule {index}: {key}: {error}"))
        })
        .collect()
}

/// A trigger's boolean key, `false` when absent.
fn boolean(
    trigger: &serde_json::Map<String, Value>,
    key: &str,
    index: usize,
) -> Result<bool, String> {
    match trigger.get(key) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("rule {index}: {key} is not a boolean")),
    }
}

/// Rejects a selector on an action that takes none.
fn reject_selector(action: &serde_json::Map<String, Value>, index: usize) -> Result<(), String> {
    if action.contains_key("selector") {
        return Err(format!("rule {index}: the action type takes no selector"));
    }
    Ok(())
}

/// Whether `domain` is one WebKit accepts: a lowercase host, optionally
/// prefixed with `*` for itself and its subdomains.
fn valid_domain(domain: &str) -> Result<(), String> {
    let body = domain.strip_prefix('*').unwrap_or(domain);
    if body.is_empty() {
        return Err(format!("`{domain}` is empty"));
    }
    if !domain.is_ascii() {
        return Err(format!("`{domain}` is not ASCII"));
    }
    if domain != domain.to_ascii_lowercase() {
        return Err(format!("`{domain}` is not lowercase"));
    }
    if body.starts_with('.') || body.ends_with('.') {
        return Err(format!("`{domain}` starts or ends with a dot"));
    }
    if body.len() > 253 {
        return Err(format!("`{domain}` is too long"));
    }
    for label in body.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(format!("`{domain}` has an empty or too long label"));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(format!("`{domain}` has a character a host cannot have"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!(
                "`{domain}` has a label starting or ending with `-`"
            ));
        }
    }
    Ok(())
}

/// Whether `selector` can be put in the cosmetic style sheet as one rule
/// without changing what it applies to.
fn valid_selector(selector: &str) -> Result<(), String> {
    if selector.trim().is_empty() {
        return Err("is empty".to_owned());
    }
    if selector.starts_with('@') {
        return Err(format!("`{selector}` is not a selector"));
    }
    if selector
        .chars()
        .any(|c| c.is_control() || matches!(c, '{' | '}' | ';'))
    {
        return Err(format!("`{selector}` has a control character or a brace"));
    }
    if selector.contains("/*") || selector.contains("*/") {
        return Err(format!("`{selector}` has a comment"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocker(json: &str) -> Blocker {
        let mut list = ContentRuleList::default();
        list.id = "test".into();
        list.rules = json.into();
        Blocker::compile(&[list]).expect("a compilable rule list")
    }

    fn error(json: &str) -> String {
        let mut list = ContentRuleList::default();
        list.id = "test".into();
        list.rules = json.into();
        Blocker::compile(&[list]).expect_err("a rejected rule list")
    }

    fn blocked(json: &str, page: &str, url: &str, resource: Resource) -> bool {
        let top = if resource == Resource::Document {
            Top::Main
        } else {
            Top::Frame
        };
        blocker(json).blocks(page, url, resource, top) == Decision::Block
    }

    fn decision(json: &str, page: &str, url: &str, resource: Resource, top: Top) -> Decision {
        blocker(json).blocks(page, url, resource, top)
    }

    #[test]
    fn blocks_a_host_and_its_subdomains() {
        let rules = r#"[
            {"trigger":{"url-filter":"^[^:]+://+([^:/]+\\.)?ads\\.example\\.com[/:]",
                        "resource-type":["script"]},
             "action":{"type":"block"}}]"#;
        assert!(blocked(
            rules,
            "https://site.test/",
            "https://ads.example.com/a.js",
            Resource::Script
        ));
        assert!(blocked(
            rules,
            "https://site.test/",
            "https://sub.ads.example.com/a.js",
            Resource::Script
        ));
        assert!(!blocked(
            rules,
            "https://site.test/",
            "https://ads.example.com/a.png",
            Resource::Image
        ));
        assert!(!blocked(
            rules,
            "https://site.test/",
            "https://notads.example.com/a.js",
            Resource::Script
        ));
    }

    #[test]
    fn exceptions_undo_previous_rules() {
        let rules = r#"[
            {"trigger":{"url-filter":"^[^:]+://+([^:/]+\\.)?ads\\.example\\.com[/:]"},
             "action":{"type":"block"}},
            {"trigger":{"url-filter":".*","if-domain":["*good.test"]},
             "action":{"type":"ignore-previous-rules"}}]"#;
        assert!(blocked(
            rules,
            "https://site.test/",
            "https://ads.example.com/a.js",
            Resource::Raw
        ));
        assert!(!blocked(
            rules,
            "https://good.test/",
            "https://ads.example.com/a.js",
            Resource::Raw
        ));
    }

    #[test]
    fn domains_name_the_top_document_not_the_resource() {
        // WebKit checks `if-domain` against the page, not `cdn.test`.
        let rules = r#"[
            {"trigger":{"url-filter":"\\.js","if-domain":["*site.test"]},
             "action":{"type":"block"}}]"#;
        assert!(blocked(
            rules,
            "https://site.test/page",
            "https://cdn.test/a.js",
            Resource::Script
        ));
        assert!(!blocked(
            rules,
            "https://other.test/page",
            "https://cdn.test/a.js",
            Resource::Script
        ));
        // `unless-domain` too.
        let rules = r#"[
            {"trigger":{"url-filter":"\\.js","unless-domain":["*site.test"]},
             "action":{"type":"block"}}]"#;
        assert!(!blocked(
            rules,
            "https://site.test/page",
            "https://cdn.test/a.js",
            Resource::Script
        ));
        assert!(blocked(
            rules,
            "https://other.test/page",
            "https://cdn.test/a.js",
            Resource::Script
        ));
    }

    #[test]
    fn frame_documents_use_the_top_document() {
        // An iframe document on other.test, in a page on site.test, is
        // matched against the page's domain, not its own.
        let rules = r#"[
            {"trigger":{"url-filter":"/frame","if-domain":["*site.test"]},
             "action":{"type":"block"}}]"#;
        assert_eq!(
            decision(
                rules,
                "https://site.test/",
                "https://other.test/frame",
                Resource::Document,
                Top::Frame
            ),
            Decision::Block
        );
        // The same URL as a top-level navigation is its own top document.
        assert_eq!(
            decision(
                rules,
                "https://site.test/",
                "https://other.test/frame",
                Resource::Document,
                Top::Main
            ),
            Decision::Allow
        );
    }

    #[test]
    fn an_unknown_top_refuses_top_dependent_rules() {
        let rules = r#"[
            {"trigger":{"url-filter":"/x","if-domain":["*site.test"]},
             "action":{"type":"block"}}]"#;
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/x",
                Resource::Document,
                Top::Unknown
            ),
            Decision::Unknown
        );
        // A rule that does not use the top document is still decided.
        let rules = r#"[
            {"trigger":{"url-filter":"/x"},"action":{"type":"block"}}]"#;
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/x",
                Resource::Document,
                Top::Unknown
            ),
            Decision::Block
        );
        // A known URL-filter mismatch proves the rule inapplicable even
        // when the top document is unknown.
        let rules = r#"[
            {"trigger":{"url-filter":"\\.js$","if-domain":["*site.test"]},
             "action":{"type":"block"}}]"#;
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/page.html",
                Resource::Document,
                Top::Unknown
            ),
            Decision::Allow
        );
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/app.js",
                Resource::Document,
                Top::Unknown
            ),
            Decision::Unknown
        );
    }

    #[test]
    fn an_unknown_exception_only_matters_when_it_could_change_the_result() {
        let rules = r#"[
            {"trigger":{"url-filter":".*","resource-type":["script"]},
             "action":{"type":"block"}},
            {"trigger":{"url-filter":".*","if-domain":["keep.test"]},
             "action":{"type":"ignore-previous-rules"}}]"#;
        // A document is already definitely allowed; the script-only block
        // does not apply and the exception cannot change that.
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/page.html",
                Resource::Document,
                Top::Unknown
            ),
            Decision::Allow
        );
        // A script is blocked; the conditional exception can change it.
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/app.js",
                Resource::Script,
                Top::Unknown
            ),
            Decision::Unknown
        );
    }

    #[test]
    fn a_later_definite_exception_resolves_unknown() {
        // Unknown conditional block, then an unconditional exception: the
        // later exception permits under every possible top document.
        let rules = r#"[
            {"trigger":{"url-filter":".*","if-domain":["*site.test"]},
             "action":{"type":"block"}},
            {"trigger":{"url-filter":".*"},"action":{"type":"ignore-previous-rules"}}]"#;
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/app.js",
                Resource::Document,
                Top::Unknown
            ),
            Decision::Allow
        );
        // A definite block, then an unknown exception: uncertainty remains.
        let rules = r#"[
            {"trigger":{"url-filter":".*"},"action":{"type":"block"}},
            {"trigger":{"url-filter":".*","if-domain":["*site.test"]},
             "action":{"type":"ignore-previous-rules"}}]"#;
        assert_eq!(
            decision(
                rules,
                "",
                "https://other.test/app.js",
                Resource::Document,
                Top::Unknown
            ),
            Decision::Unknown
        );
    }

    #[test]
    fn a_main_document_is_its_own_top_document() {
        // The page URL is the previous document for a navigation; the
        // document being loaded is the top document.
        let rules = r#"[
            {"trigger":{"url-filter":"^https://","if-domain":["*new.test"]},
             "action":{"type":"block"}}]"#;
        assert!(blocked(
            rules,
            "https://old.test/",
            "https://new.test/",
            Resource::Document
        ));
        assert!(!blocked(
            rules,
            "https://old.test/",
            "https://other.test/",
            Resource::Document
        ));
    }

    #[test]
    fn domains_and_load_types_use_the_page() {
        let rules = r#"[
            {"trigger":{"url-filter":"\\.gif","load-type":["third-party"]},
             "action":{"type":"block"}}]"#;
        assert!(blocked(
            rules,
            "https://site.test/",
            "https://track.test/a.gif",
            Resource::Image
        ));
        assert!(!blocked(
            rules,
            "https://site.test/",
            "https://site.test/a.gif",
            Resource::Image
        ));
        // A main document is first party with itself, even though the
        // previous page was another site.
        assert!(!blocked(
            rules,
            "https://old.test/",
            "https://new.test/a.gif",
            Resource::Document
        ));
    }

    #[test]
    fn top_url_conditions() {
        let rules = r#"[
            {"trigger":{"url-filter":"\\.js","if-top-url":["^https://site\\.test/"]},
             "action":{"type":"block"}},
            {"trigger":{"url-filter":"\\.css","unless-top-url":["^https://keep\\.test/"]},
             "action":{"type":"block"}}]"#;
        assert!(blocked(
            rules,
            "https://site.test/a",
            "https://cdn.test/a.js",
            Resource::Script
        ));
        assert!(!blocked(
            rules,
            "https://other.test/a",
            "https://cdn.test/a.js",
            Resource::Script
        ));
        // The second rule does not apply on the excluded site.
        assert!(!blocked(
            rules,
            "https://keep.test/a",
            "https://elsewhere.test/a.css",
            Resource::StyleSheet
        ));
        assert!(blocked(
            rules,
            "https://other.test/a",
            "https://elsewhere.test/a.css",
            Resource::StyleSheet
        ));
    }

    #[test]
    fn cosmetic_script_carries_selectors_and_patterns() {
        let rules = r#"[
            {"trigger":{"url-filter":".*","unless-domain":["*keep.test"],
                        "unless-top-url":["^https://admin\\."]},
             "action":{"type":"css-display-none","selector":".ad, #sponsor"}}]"#;
        let script = blocker(rules).cosmetic_script().expect("a script");
        assert!(script.contains(".ad, #sponsor"));
        assert!(script.contains("*keep.test"));
        // The pattern is serialized into the script's JSON string, where
        // its backslash is escaped once more.
        assert!(script.contains(r"^https://admin\\."));
        assert!(script.contains("CSSStyleSheet"));
        // URL filters reach the page as their patterns, not as ASTs.
        assert!(!script.contains("url-filter"));
    }

    #[test]
    fn an_exception_clears_cosmetic_rules_without_a_resource_type() {
        let rules = r#"[
            {"trigger":{"url-filter":".*"},
             "action":{"type":"css-display-none","selector":".ad"}},
            {"trigger":{"url-filter":".*","if-domain":["*keep.test"]},
             "action":{"type":"ignore-previous-rules"}}]"#;
        let script = blocker(rules).cosmetic_script().expect("a script");
        // The clear step is present.
        assert!(script.contains(r#""x":true"#), "{script}");
        assert!(script.contains("*keep.test"));
    }

    #[test]
    fn network_only_rules_make_no_script() {
        let rules = r#"[
            {"trigger":{"url-filter":"\\.js"},"action":{"type":"block"}}]"#;
        assert!(blocker(rules).cosmetic_script().is_none());
    }

    #[test]
    fn regex_filters_use_the_rules_engine() {
        let filter =
            UrlFilter::parse(r"^https://a\.test/x[0-9]+(y)?z$", false).expect("a supported regex");
        assert!(filter.matches(&Target::new("https://a.test/x12yz")));
        assert!(filter.matches(&Target::new("https://A.TEST/x12z")));
        assert!(!filter.matches(&Target::new("https://a.test/xz")));
    }

    #[test]
    fn long_repeated_urls_are_matched_quickly_and_correctly() {
        // The request handler runs on WebView2's UI thread: a matcher
        // that backtracks would take exponential time on this URL.
        let filter = UrlFilter::parse(r"^[^:]+://+([^:/]+\.)?ads\.example\.com[/:].*b$", false)
            .expect("a supported regex");
        let mut url = String::from("https://ads.example.com/");
        url.push_str(&"a".repeat(100_000));
        assert!(!filter.matches(&Target::new(&url)));
        url.push('b');
        assert!(filter.matches(&Target::new(&url)));
    }

    #[test]
    fn literal_prefilters_only_claim_required_text() {
        let host = required_literals(r"^[^:]+://+([^:/]+\.)?ads\.example\.com[/:]", false);
        assert!(host.iter().any(|l| l == "ads.example.com"));
        assert!(required_literals(r"ads|track", false).is_empty());
        let optional = required_literals(r"abc?defg", false);
        assert!(optional.iter().all(|l| l == "defg"));
        assert!(required_literals(r"ads\d+", false).is_empty());
    }

    #[test]
    fn host_buckets_require_a_proven_literal_boundary() {
        // The reviewed counterexample: the regex matches a prefix of
        // `ads.example.com.attacker.test`, so the host is not proven.
        let open_ended = r"^[^:]+://+([^:/]+\.)?ads\.example\.com";
        assert_eq!(extract_host(open_ended), None);
        assert!(blocked(
            &format!(
                r#"[{{"trigger":{{"url-filter":{}}},"action":{{"type":"block"}}}}]"#,
                serde_json::to_string(open_ended).expect("a JSON string")
            ),
            "https://site.test/",
            "https://ads.example.com.attacker.test/x.js",
            Resource::Script
        ));
        // A fully delimited host is proven.
        assert_eq!(
            extract_host(r"^[^:]+://+([^:/]+\.)?ads\.example\.com/"),
            Some("ads.example.com".to_owned())
        );
        // Unescaped dots are wildcards, not host characters.
        assert_eq!(extract_host(r"^[^:]+://+([^:/]+\.)?ads.example.com/"), None);
        // A raw `?` makes the preceding character optional.
        assert_eq!(
            extract_host(r"^[^:]+://+([^:/]+\.)?ads\.example\.co?m/"),
            None
        );
        // An escaped query separator is a boundary.
        assert_eq!(
            extract_host(r"^[^:]+://+([^:/]+\.)?ads\.example\.com\?x"),
            Some("ads.example.com".to_owned())
        );
    }

    #[test]
    fn regex_boundaries_and_alternatives_match_without_host_assumptions() {
        for (pattern, url) in [
            (
                r"^[^:]+://+([^:/]+\.)?ads\.example\.com/?",
                "https://ads.example.com.attacker.test/x.js",
            ),
            (
                r"^[^:]+://+([^:/]+\.)?ads\.example\.com/",
                "https://ads.example.com.attacker.test/x.js",
            ),
            (
                r"^[^:]+://+([^:/]+\.)?ads\.example\.com/|https://other\.test/",
                "https://other.test/x.js",
            ),
            (
                r"^[^:]+://+([^:/]+\.)?ads\.example\.com/",
                "file:///ads.example.com/x.js",
            ),
        ] {
            let expected = regex::Regex::new(pattern).unwrap().is_match(url);
            let actual = blocked(
                &format!(
                    r#"[{{"trigger":{{"url-filter":{}}},"action":{{"type":"block"}}}}]"#,
                    serde_json::to_string(pattern).unwrap()
                ),
                "https://site.test/",
                url,
                Resource::Script,
            );
            assert_eq!(actual, expected, "{pattern:?} against {url:?}");
        }
    }

    #[test]
    fn userinfo_prefix_does_not_hide_a_matching_block_rule() {
        let filter = r"^[^:]+://+([^:/]+\.)?ads\.example\.com[/:]";
        let url = "https://ads.example.com:password@attacker.test/x.js";
        assert!(regex::Regex::new(filter).unwrap().is_match(url));
        assert!(blocked(
            &format!(
                r#"[{{"trigger":{{"url-filter":{}}},"action":{{"type":"block"}}}}]"#,
                serde_json::to_string(filter).unwrap()
            ),
            "https://site.test/",
            url,
            Resource::Script,
        ));
    }

    #[test]
    fn ipv6_hosts_keep_their_address_for_party_checks() {
        let rules = r#"[
            {"trigger":{"url-filter":"\\.png","load-type":["third-party"]},
             "action":{"type":"block"}}]"#;
        // Same address: first party.
        assert_eq!(
            decision(
                rules,
                "http://[::1]/page",
                "http://[::1]/logo.png",
                Resource::Image,
                Top::Frame
            ),
            Decision::Allow
        );
        // Different address: third party.
        assert_eq!(
            decision(
                rules,
                "http://[::1]/page",
                "http://[::2]/logo.png",
                Resource::Image,
                Top::Frame
            ),
            Decision::Block
        );
    }

    #[test]
    fn parties() {
        assert!(is_first_party("a.example.com", "example.com"));
        assert!(!is_first_party("a.example.com", "other.com"));
        assert!(is_first_party("shop.example.co.uk", "www.example.co.uk"));
        // Independently hosted sites under a private suffix are different
        // sites.
        assert!(!is_first_party("a.github.io", "b.github.io"));
        assert!(!is_first_party("127.0.0.1", "128.0.0.1"));
        assert!(is_first_party("127.0.0.1", "127.0.0.1"));
        assert!(is_first_party("::1", "0:0:0:0:0:0:0:1"));
    }

    #[test]
    fn rejects_malformed_rule_json() {
        assert!(error("not json").contains("not a JSON array"));
        assert!(error(r#"{"trigger":{}}"#).contains("not a JSON array"));
        assert!(error(r#"[1]"#).contains("rule 0 is not an object"));
        assert!(error(r#"[{"action":{"type":"block"}}]"#).contains("no trigger"));
        assert!(error(r#"[{"trigger":{"url-filter":".*"}}]"#).contains("no action"));
        assert!(error(r#"[{"trigger":{"url-filter":".*"},"action":{}}]"#).contains("no type"));
    }

    #[test]
    fn rejects_unknown_keys_and_actions() {
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","load-context":["top-frame"]},
                       "action":{"type":"block"}}]"#
            )
            .contains("unsupported trigger key `load-context`")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block","css":"x"}}]"#)
                .contains("unsupported action key `css`")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block"},"note":"x"}]"#)
                .contains("unsupported key `note`")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"make-https"}}]"#)
                .contains("not supported")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block-cookies"}}]"#)
                .contains("not supported")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"wat"}}]"#)
                .contains("unknown action")
        );
    }

    #[test]
    fn rejects_rules_this_backend_would_approximate() {
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","resource-type":["svg-document"]},
                       "action":{"type":"block"}}]"#
            )
            .contains("not supported")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":"(?=ads)"},"action":{"type":"block"}}]"#)
                .contains("not a regular expression")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":"(a)\\1"},"action":{"type":"block"}}]"#)
                .contains("not a regular expression")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","resource-type":["nope"]},
                       "action":{"type":"block"}}]"#
            )
            .contains("unknown resource-type")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","load-type":["nope"]},
                       "action":{"type":"block"}}]"#
            )
            .contains("unknown load-type")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","load-type":["first-party"]},
                       "action":{"type":"css-display-none","selector":".ad"}}]"#
            )
            .contains("load-type is not supported for css-display-none")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","resource-type":["image"]},
                       "action":{"type":"css-display-none","selector":".ad"}}]"#
            )
            .contains("does not apply to documents")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","load-type":["third-party"]},
                       "action":{"type":"ignore-previous-rules"}}]"#
            )
            .contains("cosmetic ignore-previous-rules")
        );
    }

    #[test]
    fn rejects_malformed_conditions() {
        assert!(
            error(r#"[{"trigger":{"url-filter":""},"action":{"type":"block"}}]"#)
                .contains("url-filter is empty")
        );
        assert!(error(r#"[{"trigger":{},"action":{"type":"block"}}]"#).contains("no url-filter"));
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","if-domain":[]},
                       "action":{"type":"block"}}]"#
            )
            .contains("if-domain is empty")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","if-domain":["Example.com"]},
                       "action":{"type":"block"}}]"#
            )
            .contains("not lowercase")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","if-domain":["https://example.com"]},
                       "action":{"type":"block"}}]"#
            )
            .contains("character a host cannot have")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","resource-type":[]},
                       "action":{"type":"block"}}]"#
            )
            .contains("resource-type is empty")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","if-top-url":[]},
                       "action":{"type":"block"}}]"#
            )
            .contains("if-top-url is empty")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*","url-filter-is-case-sensitive":"yes"},
                       "action":{"type":"block"}}]"#
            )
            .contains("not a boolean")
        );
    }

    #[test]
    fn rejects_selectors_that_would_change_the_style_sheet() {
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*"},
                       "action":{"type":"css-display-none","selector":""}}]"#
            )
            .contains("selector is empty")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*"},
                       "action":{"type":"css-display-none","selector":".a{}body{color:red"}}]"#
            )
            .contains("brace")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*"},
                       "action":{"type":"css-display-none","selector":"@media x"}}]"#
            )
            .contains("not a selector")
        );
        assert!(
            error(r#"[{"trigger":{"url-filter":".*"},"action":{"type":"css-display-none"}}]"#)
                .contains("no selector")
        );
        assert!(
            error(
                r#"[{"trigger":{"url-filter":".*"},
                       "action":{"type":"block","selector":".a"}}]"#
            )
            .contains("takes no selector")
        );
    }

    #[test]
    fn one_bad_list_fails_the_whole_set() {
        let mut good = ContentRuleList::default();
        good.id = "good".into();
        good.rules = r#"[{"trigger":{"url-filter":"\\.js"},"action":{"type":"block"}}]"#.into();
        let mut bad = ContentRuleList::default();
        bad.id = "bad".into();
        bad.rules = r#"[{"trigger":{"url-filter":"(?=x)"},"action":{"type":"block"}}]"#.into();
        let failure = Blocker::compile(&[good, bad]).expect_err("a rejected set");
        assert!(failure.contains("bad"), "{failure}");
        assert!(!failure.contains("good:"));
    }
}

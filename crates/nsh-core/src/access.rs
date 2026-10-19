//! Capabilities: what an app may reach outside its VM.
//!
//! An app declares them in `nsh.json` (`"capabilities"`); the app embedding
//! nsh may grant less ([`Access`]): a request must be allowed by both.
//! Everything is denied unless listed, and unknown keys are errors, so a
//! typo is caught at build time rather than becoming a missing grant.
//!
//! ```json
//! "capabilities": {
//!   "network": {
//!     "hosts": ["api.example.com", "*.cdn.example.com"],
//!     "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }]
//!   },
//!   "webview": { "hosts": ["*.example.com"] },
//!   "statusBar": true
//! }
//! ```
//!
//! `network` covers `fetch` (every redirect is checked again), remote
//! images (also canvas images and `Image.getSize` / `prefetch`) and video
//! sources; `asset:` URIs are always allowed. `webview` lists the hosts a
//! web view may show and navigate to (its pages' own subresources are the
//! web content's business). An app without `"capabilities"` (a standalone
//! app, its developer's own) may do everything.

use serde_json::{Map, Value};

/// A host pattern: `*` (any host), `*.example.com` (its subdomains) or a
/// host name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPattern(String);

impl HostPattern {
    /// Parses a pattern (lower-cased).
    pub fn new(pattern: &str) -> Result<Self, String> {
        let p = pattern.trim().to_ascii_lowercase();
        let host = p.strip_prefix("*.").unwrap_or(&p);
        let valid = p == "*"
            || (!host.is_empty()
                && host.chars().all(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')
                }));
        if valid {
            Ok(Self(p))
        } else {
            Err(format!("invalid host pattern `{pattern}`"))
        }
    }

    /// Whether `host` (lower case) matches.
    pub fn matches(&self, host: &str) -> bool {
        match self.0.strip_prefix("*.") {
            _ if self.0 == "*" => true,
            Some(suffix) => {
                host.len() > suffix.len() + 1 && host.ends_with(suffix) && {
                    let dot = host.len() - suffix.len() - 1;
                    host.as_bytes()[dot] == b'.'
                }
            }
            None => self.0 == host,
        }
    }

    /// The pattern as written (lower case).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A finer HTTP grant: a host, optionally limited to methods, schemes and a
/// path prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpGrant {
    /// The host.
    pub host: HostPattern,
    /// Upper-case methods; empty: any.
    pub methods: Vec<String>,
    /// Lower-case schemes; empty: any of http(s) and ws(s).
    pub schemes: Vec<String>,
    /// The path must start with it.
    pub path_prefix: Option<String>,
}

/// `capabilities.network`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkGrants {
    /// Reachable with any method.
    pub hosts: Vec<HostPattern>,
    /// Finer grants.
    pub http: Vec<HttpGrant>,
}

/// What an app may do (one declaration, or one grant).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// `fetch`, remote images and video.
    pub network: NetworkGrants,
    /// The hosts web views may show; `None`: no web views.
    pub webview: Option<Vec<HostPattern>>,
    /// The app may set the status bar.
    pub status_bar: bool,
    /// The app may read and write the system clipboard (`nsh/clipboard`).
    pub clipboard: bool,
}

/// The parts of a URL the checks look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlParts<'a> {
    /// Lower-case scheme.
    pub scheme: String,
    /// Lower-case host (without port or user info).
    pub host: String,
    /// The path (and query), `/` when empty.
    pub path: &'a str,
}

/// Splits an absolute URL into scheme, host and path; `None` when it has no
/// `scheme://host`.
pub fn parse_url(url: &str) -> Option<UrlParts<'_>> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    // User info ends at the last `@`: `https://good.com@evil.com/` is evil.com.
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        format!("[{}]", v6.split_once(']')?.0)
    } else {
        host_port.split(':').next().unwrap_or_default().to_owned()
    };
    if host.is_empty() {
        return None;
    }
    Some(UrlParts {
        scheme: scheme.to_ascii_lowercase(),
        host: host.to_ascii_lowercase().trim_end_matches('.').to_owned(),
        path: if path.is_empty() { "/" } else { path },
    })
}

impl Capabilities {
    /// Everything: apps without a declaration, development tools.
    pub fn all() -> Self {
        let any = vec![HostPattern("*".into())];
        Self {
            network: NetworkGrants {
                hosts: any.clone(),
                http: Vec::new(),
            },
            webview: Some(any),
            status_bar: true,
            clipboard: true,
        }
    }

    /// Parses `capabilities` from `nsh.json` (or a grant in the same form).
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let obj = object(value, "capabilities")?;
        known(
            obj,
            "capabilities",
            &["network", "webview", "statusBar", "clipboard"],
        )?;
        let mut caps = Capabilities::default();
        if let Some(network) = obj.get("network") {
            let network = object(network, "capabilities.network")?;
            known(network, "capabilities.network", &["hosts", "http"])?;
            if let Some(hosts) = network.get("hosts") {
                caps.network.hosts = patterns(hosts, "capabilities.network.hosts")?;
            }
            if let Some(http) = network.get("http") {
                let list = http
                    .as_array()
                    .ok_or("capabilities.network.http must be an array")?;
                for grant in list {
                    let g = object(grant, "capabilities.network.http[]")?;
                    known(
                        g,
                        "capabilities.network.http[]",
                        &["host", "methods", "schemes", "pathPrefix"],
                    )?;
                    let host = g
                        .get("host")
                        .and_then(Value::as_str)
                        .ok_or("capabilities.network.http[] needs a `host`")?;
                    let strings = |key: &str, upper: bool| -> Result<Vec<String>, String> {
                        match g.get(key) {
                            None => Ok(Vec::new()),
                            Some(v) => strings(v, &format!("capabilities.network.http[].{key}"))
                                .map(|l| {
                                    l.into_iter()
                                        .map(|s| {
                                            if upper {
                                                s.to_ascii_uppercase()
                                            } else {
                                                s.to_ascii_lowercase()
                                            }
                                        })
                                        .collect()
                                }),
                        }
                    };
                    caps.network.http.push(HttpGrant {
                        host: HostPattern::new(host)?,
                        methods: strings("methods", true)?,
                        schemes: strings("schemes", false)?,
                        path_prefix: match g.get("pathPrefix") {
                            None => None,
                            Some(v) => Some(
                                v.as_str()
                                    .ok_or(
                                        "capabilities.network.http[].pathPrefix must be a string",
                                    )?
                                    .to_owned(),
                            ),
                        },
                    });
                }
            }
        }
        if let Some(webview) = obj.get("webview") {
            let webview = object(webview, "capabilities.webview")?;
            known(webview, "capabilities.webview", &["hosts"])?;
            caps.webview = Some(match webview.get("hosts") {
                Some(hosts) => patterns(hosts, "capabilities.webview.hosts")?,
                None => Vec::new(),
            });
        }
        if let Some(status_bar) = obj.get("statusBar") {
            caps.status_bar = status_bar
                .as_bool()
                .ok_or("capabilities.statusBar must be a boolean")?;
        }
        if let Some(clipboard) = obj.get("clipboard") {
            caps.clipboard = clipboard
                .as_bool()
                .ok_or("capabilities.clipboard must be a boolean")?;
        }
        Ok(caps)
    }

    /// The JSON form [`Capabilities::from_json`] reads.
    pub fn to_json(&self) -> Value {
        let list =
            |l: &[HostPattern]| Value::from(l.iter().map(|p| p.0.clone()).collect::<Vec<_>>());
        let mut network = Map::new();
        network.insert("hosts".into(), list(&self.network.hosts));
        let http: Vec<Value> = self
            .network
            .http
            .iter()
            .map(|g| {
                let mut o = Map::new();
                o.insert("host".into(), g.host.0.clone().into());
                if !g.methods.is_empty() {
                    o.insert("methods".into(), g.methods.clone().into());
                }
                if !g.schemes.is_empty() {
                    o.insert("schemes".into(), g.schemes.clone().into());
                }
                if let Some(p) = &g.path_prefix {
                    o.insert("pathPrefix".into(), p.clone().into());
                }
                Value::Object(o)
            })
            .collect();
        network.insert("http".into(), http.into());
        let mut obj = Map::new();
        obj.insert("network".into(), Value::Object(network));
        if let Some(hosts) = &self.webview {
            let mut w = Map::new();
            w.insert("hosts".into(), list(hosts));
            obj.insert("webview".into(), Value::Object(w));
        }
        obj.insert("statusBar".into(), self.status_bar.into());
        obj.insert("clipboard".into(), self.clipboard.into());
        Value::Object(obj)
    }

    fn allows_http(&self, method: &str, url: &UrlParts<'_>) -> bool {
        if !matches!(url.scheme.as_str(), "http" | "https" | "ws" | "wss") {
            return false;
        }
        self.network.hosts.iter().any(|h| h.matches(&url.host))
            || self.network.http.iter().any(|g| {
                g.host.matches(&url.host)
                    && (g.methods.is_empty()
                        || g.methods.iter().any(|m| m.eq_ignore_ascii_case(method)))
                    && (g.schemes.is_empty() || g.schemes.contains(&url.scheme))
                    && g.path_prefix
                        .as_deref()
                        .is_none_or(|p| url.path.starts_with(p))
            })
    }

    fn allows_webview(&self, url: &UrlParts<'_>) -> bool {
        matches!(url.scheme.as_str(), "http" | "https")
            && self
                .webview
                .as_ref()
                .is_some_and(|hosts| hosts.iter().any(|h| h.matches(&url.host)))
    }
}

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{what} must be an object"))
}

fn known(obj: &Map<String, Value>, what: &str, keys: &[&str]) -> Result<(), String> {
    match obj.keys().find(|k| !keys.contains(&k.as_str())) {
        Some(k) => Err(format!(
            "{what}: unknown key `{k}` (known: {})",
            keys.join(", ")
        )),
        None => Ok(()),
    }
}

fn strings(value: &Value, what: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .and_then(|a| a.iter().map(|v| v.as_str().map(str::to_owned)).collect())
        .ok_or_else(|| format!("{what} must be an array of strings"))
}

fn patterns(value: &Value, what: &str) -> Result<Vec<HostPattern>, String> {
    strings(value, what)?
        .iter()
        .map(|p| HostPattern::new(p))
        .collect()
}

/// What an app may do: its declaration and the embedder's grant (when the
/// embedder limits it); an action must be allowed by both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    /// What the app declared (everything without a declaration).
    pub declared: Capabilities,
    /// What the embedding app grants it at most (`None`: what it declared).
    pub granted: Option<Capabilities>,
}

impl Default for Access {
    fn default() -> Self {
        Self::all()
    }
}

impl Access {
    /// Everything allowed.
    pub fn all() -> Self {
        Self {
            declared: Capabilities::all(),
            granted: None,
        }
    }

    fn each(&self, f: impl Fn(&Capabilities) -> bool) -> bool {
        f(&self.declared) && self.granted.as_ref().is_none_or(f)
    }

    /// True when nothing is restricted (no check needed, redirects may be
    /// followed by the platform).
    pub fn is_unrestricted(&self) -> bool {
        let all = Capabilities::all();
        self.declared == all && self.granted.as_ref().is_none_or(|g| *g == all)
    }

    /// Whether `fetch(url, { method })` may go out. `asset:` URIs are always
    /// allowed.
    pub fn allows_http(&self, method: &str, url: &str) -> bool {
        if url.starts_with("asset:") {
            return true;
        }
        let Some(parts) = parse_url(url) else {
            return false;
        };
        self.each(|c| c.allows_http(method, &parts))
    }

    /// Whether a media URL (image, video, canvas image) may be loaded.
    /// Relative and `asset:` URIs are the app's own files.
    pub fn allows_media(&self, uri: &str) -> bool {
        if !uri.contains("://") || uri.starts_with("asset:") || uri.starts_with("data:") {
            return true;
        }
        self.allows_http("GET", uri)
    }

    /// The next URL of a media load that was redirected from `base` to
    /// `location`, if the app may load it: the redirect must stay on
    /// `http(s)` and pass the same check as a first request, so loaders
    /// follow redirects themselves, one hop at a time, and never contact
    /// a target the app is not granted.
    pub fn media_redirect(&self, base: &str, location: &str) -> Result<String, String> {
        let url = resolve_reference(base, location);
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(format!("redirect to `{url}` is not an http(s) URL"));
        }
        if !self.is_unrestricted() && !self.allows_media(&url) {
            return Err(format!(
                "redirect to `{url}` is not granted by capabilities.network"
            ));
        }
        Ok(url)
    }

    /// Whether web views are granted at all (inline HTML needs only this).
    pub fn allows_webviews(&self) -> bool {
        self.each(|c| c.webview.is_some())
    }

    /// Whether a web view may show (or navigate to) `url`; `about:blank`
    /// whenever web views are granted.
    pub fn allows_webview(&self, url: &str) -> bool {
        if url == "about:blank" {
            return self.allows_webviews();
        }
        let Some(parts) = parse_url(url) else {
            return false;
        };
        self.each(|c| c.allows_webview(&parts))
    }

    /// Whether the app may set the status bar.
    pub fn allows_status_bar(&self) -> bool {
        self.each(|c| c.status_bar)
    }

    /// Whether the app may read and write the system clipboard.
    pub fn allows_clipboard(&self) -> bool {
        self.each(|c| c.clipboard)
    }
}

/// `location` against `base` (absolute, scheme-relative, absolute-path or
/// relative references), as a redirect's `Location` is read.
pub fn resolve_reference(base: &str, location: &str) -> String {
    if location.contains("://") {
        return location.to_owned();
    }
    let Some((scheme, rest)) = base.split_once("://") else {
        return location.to_owned();
    };
    if let Some(net) = location.strip_prefix("//") {
        return format!("{scheme}://{net}");
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let origin = format!("{scheme}://{}", &rest[..authority_end]);
    if location.starts_with('/') {
        return format!("{origin}{location}");
    }
    let path = &rest[authority_end..];
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    format!("{origin}{dir}/{location}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(json: &str) -> Capabilities {
        Capabilities::from_json(&serde_json::from_str(json).unwrap()).unwrap()
    }

    #[test]
    fn denies_unless_listed() {
        let access = Access {
            declared: caps("{}"),
            granted: None,
        };
        assert!(!access.allows_http("GET", "https://example.com/"));
        assert!(!access.allows_media("https://example.com/a.png"));
        assert!(access.allows_media("asset:/logo.png"));
        assert!(access.allows_media("logo.png"));
        assert!(!access.allows_webview("https://example.com/"));
        assert!(!access.allows_status_bar());
        assert!(!access.allows_clipboard());
        assert!(Access::all().is_unrestricted());
    }

    #[test]
    fn hosts_and_finer_grants() {
        let c = caps(
            r#"{ "network": {
                "hosts": ["*.example.org"],
                "http": [{ "host": "api.example.com", "methods": ["get"], "pathPrefix": "/v1/" }]
            } }"#,
        );
        let a = Access {
            declared: c,
            granted: None,
        };
        assert!(a.allows_http("POST", "https://api.example.org/x"));
        assert!(!a.allows_http("GET", "https://example.org/"));
        assert!(!a.allows_http("GET", "https://example.org.evil.io/"));
        assert!(!a.allows_http("GET", "https://evilexample.org/"));
        assert!(a.allows_http("GET", "https://API.example.com/v1/quote"));
        assert!(!a.allows_http("POST", "https://api.example.com/v1/quote"));
        assert!(!a.allows_http("GET", "https://api.example.com/v2/"));
        assert!(!a.allows_http("GET", "file://api.example.com/v1/"));
        // User info does not fool the host check.
        assert!(!a.allows_http("GET", "https://api.example.org@evil.io/"));
        assert!(a.allows_http("GET", "https://user:pw@api.example.org:8443/"));
    }

    #[test]
    fn the_embedder_grants_at_most() {
        let a = Access {
            declared: caps(
                r#"{ "network": { "hosts": ["a.com", "b.com"] }, "statusBar": true, "clipboard": true }"#,
            ),
            granted: Some(caps(
                r#"{ "network": { "hosts": ["b.com"] }, "clipboard": true }"#,
            )),
        };
        assert!(a.allows_clipboard());
        assert!(!a.allows_http("GET", "https://a.com/"));
        assert!(a.allows_http("GET", "https://b.com/"));
        assert!(!a.allows_status_bar());
    }

    #[test]
    fn typos_are_errors() {
        let bad =
            |json: &str| Capabilities::from_json(&serde_json::from_str(json).unwrap()).unwrap_err();
        assert!(bad(r#"{ "netwrok": {} }"#).contains("unknown key `netwrok`"));
        assert!(bad(r#"{ "network": { "host": [] } }"#).contains("unknown key `host`"));
        assert!(bad(r#"{ "network": { "hosts": ["exa mple.com"] } }"#).contains("invalid host"));
        let c = caps(
            r#"{ "network": { "hosts": ["a.com"], "http": [{ "host": "b.com", "methods": ["GET"] }] }, "webview": { "hosts": ["*"] } }"#,
        );
        assert_eq!(Capabilities::from_json(&c.to_json()).unwrap(), c);
    }

    #[test]
    fn media_redirects_are_checked() {
        let a = Access {
            declared: Capabilities::from_json(
                &serde_json::json!({ "network": { "hosts": ["a.com"] } }),
            )
            .unwrap(),
            granted: None,
        };
        let from = "https://a.com/x/y.png";
        assert_eq!(
            a.media_redirect(from, "/z.png").unwrap(),
            "https://a.com/z.png"
        );
        assert_eq!(
            a.media_redirect(from, "z.png").unwrap(),
            "https://a.com/x/z.png"
        );
        assert!(a.media_redirect(from, "https://b.com/z.png").is_err());
        assert!(a.media_redirect(from, "//b.com/z.png").is_err());
        assert!(a.media_redirect(from, "file:///etc/passwd").is_err());
        assert!(
            Access::default()
                .media_redirect(from, "//b.com/z.png")
                .is_ok()
        );
        assert!(
            Access::default()
                .media_redirect(from, "ftp://b.com/z.png")
                .is_err()
        );
    }
}

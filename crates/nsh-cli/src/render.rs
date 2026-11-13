//! `nsh render`: an app's page as static HTML at build time, without a
//! browser (`docs/dev/native-render.md`). The app runs as `nsh test` runs it,
//! on a headless engine (QuickJS and the core, no platform) on a manual
//! clock, started at the page's address (`Headless::start_at_page`: the
//! app's `UrlScheme.url()`, which it opens the routes of, as on the web) with
//! relative requests resolved against the site; once it settles, its mounted
//! tree ([`nsh_html::Document`]) is written as markup a browser lays out at
//! any width, with the page's head, links and the JSON responses the app read
//! (`fetches`, which the page hands the app so its first frame shows the
//! same). One page per run, on an engine of its own; the page is the same for
//! every visitor.

use std::{
    collections::{BTreeMap, HashMap},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use nsh_core::{
    geometry::{Size, px},
    platform::{ColorScheme, Environment, FetchEvent, HttpBody, HttpRequest, Idiom, Locale, Os},
};
use nsh_engine::{App, Bundle, Clock, EngineConfig, FetchEvents, FetchTask, Network, Policy};
use nsh_html::{Document, Head};
use nsh_testing::{DirectoryAssets, Headless, UreqNetwork};
use serde_json::{Value, json};
use url::Url;

/// A page to render.
struct Request {
    /// The URL path the app opens (`/notes/apple`), with its query.
    path: String,
    /// The site's URL: relative requests and the head's canonical link
    /// resolve against it.
    base: String,
    /// The viewport the core lays out at, in points: what is in view
    /// (virtualized lists); the HTML fits any width.
    width: f32,
    height: f32,
    /// The appearance the app renders in: `"light"` (default) or `"dark"`.
    appearance: Option<String>,
    /// The locale the app reads (`en-US`).
    locale: Option<String>,
    /// How long the app may take to settle, in milliseconds.
    timeout: u64,
}

fn default_width() -> f32 {
    1280.0
}
fn default_height() -> f32 {
    900.0
}
fn default_timeout() -> u64 {
    5000
}

/// The app, compiled once for every render.
struct Renderer {
    code: String,
    policy: Policy,
    dir: PathBuf,
}

/// `nsh render`: the page at `path`, as JSON on standard output.
pub(crate) fn render(
    entry: &Path,
    path: Option<&str>,
    base: &str,
    appearance: &str,
) -> anyhow::Result<()> {
    let entry = crate::entry::resolve(entry)?;
    let built = crate::build::compile(&entry, true).map_err(|e| anyhow::anyhow!(e.plain))?;
    let policy = Policy {
        declared: built.capabilities,
        ..Policy::default()
    };
    let renderer = Renderer {
        code: built.code,
        policy,
        dir: entry.parent().unwrap_or(Path::new(".")).to_path_buf(),
    };
    let request = Request {
        path: path.unwrap_or("/").to_owned(),
        base: base.to_owned(),
        width: default_width(),
        height: default_height(),
        locale: None,
        appearance: Some(appearance.to_owned()),
        timeout: default_timeout(),
    };
    let answer = renderer.answer(&request);
    writeln!(
        std::io::stdout().lock(),
        "{}",
        serde_json::to_string_pretty(&answer)?
    )?;
    Ok(())
}

impl Renderer {
    fn answer(&self, request: &Request) -> Value {
        let started = Instant::now();
        match self.page(request) {
            Ok(mut page) => {
                page["ms"] = json!(started.elapsed().as_millis() as u64);
                page
            }
            Err(e) => json!({ "path": request.path, "error": e }),
        }
    }

    fn page(&self, request: &Request) -> Result<Value, String> {
        let base = site_base(&request.base)?;
        let page_url = page_url(&base, &request.path)?;
        // The page's base is the site's (`document.baseURI` of a page whose
        // `<base href>` is the site's directory): relative requests and the
        // app's addresses resolve against it, wherever the page is.
        let bundle = Bundle::source(self.code.clone())
            .with_policy(self.policy.clone())
            .with_base_url(base.as_str());
        let fetches = Arc::new(Mutex::new(BTreeMap::new()));
        let network = Arc::new(Capture {
            inner: Arc::new(UreqNetwork::new()),
            site: base.clone(),
            fetches: fetches.clone(),
        });
        let locale = request
            .locale
            .as_deref()
            .and_then(Locale::parse)
            .unwrap_or(Locale::EN_US);
        let default = EngineConfig::new(App::Js(bundle));
        let config = EngineConfig {
            viewport: Size::new(px(request.width), px(request.height)),
            network: Some(network),
            assets: Some(Arc::new(DirectoryAssets::new(self.dir.clone(), Vec::new()))),
            environment: Environment {
                os: Os::Web,
                idiom: Idiom::Desktop,
                locale,
                color_scheme: match request.appearance.as_deref() {
                    Some("dark") => ColorScheme::Dark,
                    _ => ColorScheme::Light,
                },
                ..Default::default()
            },
            // The sets this command line links (an app's own Rust components
            // render with a runner built for it, not yet).
            ui_sets: crate::own_sets().to_vec(),
            // Time moves only as the render moves it: the same page each time.
            clock: Clock::manual(SystemTime::now()),
            ..default
        };
        let mut app =
            // The app starts at the page's address, as on the web: its
            // `UrlScheme.url()` is the page's URL, which it opens the routes of.
            Headless::start_at_page(config, page_url.as_str())
                .map_err(|e| format!("the engine did not start: {e}"))?;
        let document = Arc::new(Mutex::new(Document::new()));
        let batches = Arc::new(AtomicUsize::new(0));
        {
            let document = document.clone();
            let batches = batches.clone();
            app.observe(move |_, batch| {
                if batch.items.is_empty() {
                    return;
                }
                batches.fetch_add(1, Ordering::SeqCst);
                let mut document = document.lock().unwrap_or_else(|e| e.into_inner());
                for item in &batch.items {
                    document.apply(item);
                }
            });
        }
        app.first_render()?;
        let timed_out = !settle(&mut app, &batches, Duration::from_millis(request.timeout))?;
        let ids = document.lock().unwrap_or_else(|e| e.into_inner()).views();
        let (tx, rx) = std::sync::mpsc::channel();
        app.engine().control().layout_css(ids, move |layouts| {
            let _ = tx.send(layouts);
        });
        let layouts: HashMap<_, _> = rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "the engine did not answer the layout".to_owned())?
            .into_iter()
            .collect();
        let document = document.lock().unwrap_or_else(|e| e.into_inner());
        let page = document.page(&layouts);
        let head = document.head();
        let links: Vec<String> = page
            .links
            .iter()
            .filter_map(|href| site_path(&base, href))
            .collect();
        let fetches = fetches.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Ok(json!({
            "path": request.path,
            "title": head.title,
            "lang": head.lang,
            "head": head_html(&head, &base),
            "html": page.html,
            "css": page.css,
            "links": links,
            "fetches": fetches,
            "timedOut": timed_out,
        }))
    }
}

/// How far the clock moves to see whether the page still changes.
const SETTLE_WINDOW: Duration = Duration::from_millis(100);

/// Settles the page: the driver's own settling ([`Headless::settle`]: the
/// engine idle and its batches applied), then no request in flight and a
/// [`SETTLE_WINDOW`] of the clock (the timers due within it run) that
/// changes nothing, so data loaded after a `setTimeout(…, 0)` is in the page
/// while an interval far off (prices polled every few seconds) does not
/// hold it back. A request in flight is waited for in real time with the
/// clock still: the page the API answers is the page. False when `timeout`
/// ended it (a settling under way is not cut short).
fn settle(app: &mut Headless, batches: &AtomicUsize, timeout: Duration) -> Result<bool, String> {
    let control = app.engine().control();
    let idle = || {
        let (tx, rx) = std::sync::mpsc::channel();
        control.flush(move |idle| {
            let _ = tx.send(idle);
        });
        rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
    };
    let deadline = Instant::now() + timeout;
    // The batches applied when the clock was last moved.
    let mut moved_at = None;
    while Instant::now() < deadline {
        app.settle();
        // Fails with what the app threw meanwhile.
        app.wait(Duration::ZERO)?;
        // Settled but not idle: a request is in flight, which `settle`
        // waited a while for.
        if !idle() {
            continue;
        }
        let applied = batches.load(Ordering::SeqCst);
        if moved_at == Some(applied) {
            return Ok(true);
        }
        moved_at = Some(applied);
        app.wait(SETTLE_WINDOW)?;
    }
    Ok(false)
}

/// The network: the JSON bodies of the site's requests are kept (`fetches`,
/// by path and query). Requests elsewhere go as they are.
struct Capture {
    inner: Arc<dyn Network>,
    /// The site's URL: a request of its origin is the site's.
    site: Url,
    fetches: Arc<Mutex<BTreeMap<String, Value>>>,
}

impl Network for Capture {
    fn fetch(&self, request: HttpRequest, events: FetchEvents) -> Box<dyn FetchTask> {
        let Some(key) = site_path(&self.site, &request.url) else {
            return self.inner.fetch(request, events);
        };
        // A GET is kept by its path and query; another request by its method,
        // path and body as sent (`POST /api/posts {"limit":5}`), so a read
        // done with a POST (a search, a list with filters) is kept too, under
        // the key the app looks it up by.
        let key = if request.method.eq_ignore_ascii_case("GET") {
            key
        } else {
            match &request.body {
                HttpBody::Bytes(bytes) => format!(
                    "{} {key} {}",
                    request.method.to_ascii_uppercase(),
                    String::from_utf8_lossy(bytes)
                ),
                _ => format!("{} {key}", request.method.to_ascii_uppercase()),
            }
        };
        let fetches = self.fetches.clone();
        let mut status = 0;
        let mut body = Vec::new();
        let mut events = events;
        let events: FetchEvents = Box::new(move |event| {
            match &event {
                FetchEvent::Head(head) => {
                    status = head.status;
                }
                FetchEvent::Data(chunk) => body.extend_from_slice(chunk),
                FetchEvent::End if status == 200 => {
                    if let Ok(value) = serde_json::from_slice::<Value>(&body) {
                        fetches
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(key.clone(), value);
                    }
                }
                _ => {}
            }
            events(event);
        });
        self.inner.fetch(request, events)
    }
}

/// The page's `<head>` elements from what its `Head`s set, as the web
/// runtime writes them (`web/browser/page.js`): description, canonical link
/// (against the site's URL), Open Graph's title and description unless the
/// app's own meta set them, the app's meta, structured data.
fn head_html(head: &Head, base: &Url) -> String {
    let attr = crate::web::escape_html;
    let mut out = String::new();
    let mut push = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    if let Some(d) = &head.description {
        push(format!(
            "<meta name=\"description\" content=\"{}\">",
            attr(d)
        ));
    }
    if let Some(c) = head.canonical.as_deref().and_then(|c| site_url(base, c)) {
        push(format!(
            "<link rel=\"canonical\" href=\"{}\">",
            attr(c.as_str())
        ));
    }
    let has = |property: &str| {
        head.meta
            .iter()
            .any(|m| m.get("property").and_then(Value::as_str) == Some(property))
    };
    if let Some(t) = head.title.as_deref().filter(|_| !has("og:title")) {
        push(format!(
            "<meta property=\"og:title\" content=\"{}\">",
            attr(t)
        ));
    }
    if let Some(d) = head
        .description
        .as_deref()
        .filter(|_| !has("og:description"))
    {
        push(format!(
            "<meta property=\"og:description\" content=\"{}\">",
            attr(d)
        ));
    }
    for m in &head.meta {
        // `Document::head` left out the ones without a content.
        let Some(content) = m.get("content").and_then(Value::as_str) else {
            continue;
        };
        let key = match (
            m.get("property").and_then(Value::as_str),
            m.get("name").and_then(Value::as_str),
        ) {
            (Some(p), _) => format!("property=\"{}\"", attr(p)),
            (None, Some(n)) => format!("name=\"{}\"", attr(n)),
            _ => continue,
        };
        push(format!("<meta {key} content=\"{}\">", attr(content)));
    }
    if let Some(ld) = &head.json_ld {
        // Nothing in it may close the script.
        let json = ld.to_string().replace('<', "\\u003c");
        push(format!(
            "<script type=\"application/ld+json\">{json}</script>"
        ));
    }
    out
}

/// The site's URL (`--base`) as the directory its pages are under: an
/// `http(s)` URL, without a query or fragment, `/` added to a path that
/// does not end with one.
fn site_base(base: &str) -> Result<Url, String> {
    let mut url = Url::parse(base).map_err(|e| format!("--base {base}: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("--base {base}: the site's URL is http(s)"));
    }
    url.set_query(None);
    url.set_fragment(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// The URL of the page at `path` (`/notes/apple?tab=2`, from the site's
/// directory).
fn page_url(base: &Url, path: &str) -> Result<Url, String> {
    let page = base
        .join(path.trim_start_matches('/'))
        .map_err(|e| format!("--path {path}: {e}"))?;
    if page.origin() != base.origin() {
        return Err(format!("--path {path}: not a path of the site"));
    }
    Ok(page)
}

/// A URL the app gives (`Head`'s `canonical`), as the web runtime resolves
/// it (`web/browser/page.js` `siteURL`): a path (`/notes/q3`) starts at the
/// site's directory, anything else (a relative path, `//host/x`, an absolute
/// URL) resolves against it as a browser resolves a link. None for what is
/// no URL.
fn site_url(base: &Url, url: &str) -> Option<Url> {
    let from_site = url.strip_prefix('/').filter(|rest| !rest.starts_with('/'));
    base.join(from_site.unwrap_or(url)).ok()
}

/// The path within the site (`/notes/apple`, with its query and fragment)
/// of `href`, a link's or a request's URL resolved against the site's as a
/// browser resolves it, or none for one of another origin
/// (`http://localhost.evil.example/` is not `http://localhost`'s).
fn site_path(base: &Url, href: &str) -> Option<String> {
    let url = base.join(href).ok()?;
    (url.origin() == base.origin()).then(|| url[url::Position::BeforePath..].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(url: &str) -> Url {
        site_base(url).unwrap()
    }

    const APP: &str = r#"import { mount, signal, Text, UrlScheme, View } from "nsh";
import { Link } from "nsh/base";
import { Head } from "nsh/web";

function App() {
  const url = UrlScheme.url() ?? "";
  const slug = new URL(url).pathname.split("/").pop();
  const title = signal("Loading");
  // What comes a turn after the first frame is in the page.
  setTimeout(() => title.set(`${slug} · Notes`), 0);
  return (
    <View>
      <Head
        title={title()}
        description={'Notes & "quotes"'}
        canonical={`/notes/${slug}`}
        lang="zh-CN"
        meta={[{ name: "robots", content: "index" }]}
      />
      <Text role="header" accessibilityLevel={1}>{title()}</Text>
      <Text class="hidden">Gone</Text>
      <Link href="/notes/nvidia"><Text>Next</Text></Link>
      <Link href="https://elsewhere.example.com/x"><Text>Elsewhere</Text></Link>
      <Text>{url}</Text>
      <Text>{"腾讯 raised $26 & $& <b>"}</Text>
    </View>
  );
}

mount(App);
"#;

    /// The whole path: the app's `<Head>` and views, compiled, run at the
    /// page's address and settled, are the page's title, head and markup.
    #[test]
    fn an_apps_head_and_views_are_the_rendered_pages() {
        let dir = std::env::temp_dir().join(format!("nsh-render-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("App.tsx");
        std::fs::write(&entry, APP).unwrap();
        let built = crate::build::compile(&entry, true)
            .map_err(|e| e.plain)
            .unwrap();
        let renderer = Renderer {
            code: built.code,
            policy: Policy {
                declared: built.capabilities,
                ..Policy::default()
            },
            dir: dir.clone(),
        };
        let page = renderer
            .page(&Request {
                path: "/notes/apple?tab=2".to_owned(),
                base: "https://notes.example.com/app".to_owned(),
                width: default_width(),
                height: default_height(),
                appearance: None,
                locale: None,
                timeout: default_timeout(),
            })
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(page["timedOut"], false, "{page}");
        // The head, as the app's `<Head>` set it once its data came.
        assert_eq!(page["title"], "apple · Notes");
        assert_eq!(page["lang"], "zh-CN");
        assert_eq!(
            page["head"],
            "<meta name=\"description\" content=\"Notes &amp; &quot;quotes&quot;\">\n\
             <link rel=\"canonical\" href=\"https://notes.example.com/app/notes/apple\">\n\
             <meta property=\"og:title\" content=\"apple · Notes\">\n\
             <meta property=\"og:description\" content=\"Notes &amp; &quot;quotes&quot;\">\n\
             <meta name=\"robots\" content=\"index\">\n"
        );
        // The first screen: the app started at the page's address.
        let html = page["html"].as_str().unwrap();
        assert!(html.contains("<h1 "), "{html}");
        assert!(html.contains(">apple · Notes</span>"), "{html}");
        assert!(
            html.contains(">https://notes.example.com/app/notes/apple?tab=2</span>"),
            "{html}"
        );
        // A text out of the layout is not shown, and the head is no view.
        let gone = &html[..html.find(">Gone<").expect("the hidden text")];
        let gone = &gone[gone.rfind("<div").unwrap()..];
        assert!(gone.contains("display:none;"), "{gone}");
        assert!(!gone.contains("display:block;"), "{gone}");
        assert_eq!(html.matches("class=\"nsh-v").count(), 9, "{html}");
        // Text in any script, with what HTML and replacement patterns read.
        assert!(
            html.contains(">腾讯 raised $26 &amp; $&amp; &lt;b&gt;</span>"),
            "{html}"
        );
        // The site's links, not those elsewhere.
        assert_eq!(page["links"], json!(["/notes/nvidia"]));
        assert_eq!(page["fetches"], json!({}));
    }

    #[test]
    fn the_sites_url_is_an_http_directory_and_a_page_a_path_under_it() {
        assert_eq!(base("http://localhost").as_str(), "http://localhost/");
        assert_eq!(
            base("https://notes.example.com/app?x=1#top").as_str(),
            "https://notes.example.com/app/"
        );
        assert!(site_base("notes.example.com").is_err());
        assert!(site_base("file:///site/").is_err());

        let site = base("https://notes.example.com/app/");
        let page = |path| page_url(&site, path).map(String::from);
        assert_eq!(page("/").unwrap(), "https://notes.example.com/app/");
        assert_eq!(
            page("/notes/apple?tab=2").unwrap(),
            "https://notes.example.com/app/notes/apple?tab=2"
        );
        assert_eq!(
            page("notes/apple").unwrap(),
            "https://notes.example.com/app/notes/apple"
        );
        // A path cannot leave the site.
        assert_eq!(
            page("//evil.example/x").unwrap(),
            "https://notes.example.com/app/evil.example/x"
        );
        assert!(page("https://evil.example/x").is_err());
    }

    #[test]
    fn a_link_is_the_sites_by_its_origin_not_by_its_prefix() {
        let site = base("http://localhost/");
        let path = |href| site_path(&site, href);
        assert_eq!(path("/notes/apple").as_deref(), Some("/notes/apple"));
        assert_eq!(
            path("notes/apple?tab=2").as_deref(),
            Some("/notes/apple?tab=2")
        );
        assert_eq!(path("http://localhost").as_deref(), Some("/"));
        assert_eq!(path("http://localhost/a#b").as_deref(), Some("/a#b"));
        // Another host, port or scheme that starts with the site's.
        assert_eq!(path("http://localhost.evil.example/x"), None);
        assert_eq!(path("http://localhost:8080/x"), None);
        assert_eq!(path("https://localhost/x"), None);
        assert_eq!(path("//evil.example/x"), None);
        assert_eq!(path("mailto:a@example.com"), None);
    }

    #[test]
    fn the_head_is_written_as_the_web_runtime_writes_it() {
        let site = base("https://notes.example.com/app/");
        let canonical = |url: &str| {
            let mut head = Head::default();
            head.canonical = Some(url.to_owned());
            head_html(&head, &site)
        };
        // A path starts at the site's directory; the rest resolves as a link.
        assert_eq!(
            canonical("/notes/q3"),
            "<link rel=\"canonical\" href=\"https://notes.example.com/app/notes/q3\">\n"
        );
        assert_eq!(
            canonical("notes/q3"),
            "<link rel=\"canonical\" href=\"https://notes.example.com/app/notes/q3\">\n"
        );
        assert_eq!(
            canonical("https://www.example.org/q3?a=1&b=2"),
            "<link rel=\"canonical\" href=\"https://www.example.org/q3?a=1&amp;b=2\">\n"
        );
        assert_eq!(
            canonical("//cdn.example.org/q3"),
            "<link rel=\"canonical\" href=\"https://cdn.example.org/q3\">\n"
        );

        let mut head = Head::default();
        head.title = Some("Q3 \"notes\" <1>".to_owned());
        head.description = Some("Margins & mix".to_owned());
        head.meta = vec![
            json!({ "property": "og:description", "content": "Its own" }),
            json!({ "name": "robots", "content": "" }),
            json!({ "name": "author" }),
            json!({ "content": "no key" }),
        ];
        head.json_ld = Some(json!({ "name": "</script><b>" }));
        assert_eq!(
            head_html(&head, &site),
            "<meta name=\"description\" content=\"Margins &amp; mix\">\n\
             <meta property=\"og:title\" content=\"Q3 &quot;notes&quot; &lt;1&gt;\">\n\
             <meta property=\"og:description\" content=\"Its own\">\n\
             <meta name=\"robots\" content=\"\">\n\
             <script type=\"application/ld+json\">{\"name\":\"\\u003c/script>\\u003cb>\"}</script>\n"
        );
    }
}

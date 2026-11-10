---
title: WebView
description: "The WebView element: web pages and inline HTML inside an app, with messaging both ways."
order: 80
group: primitives
demo: el-web-view
---

# WebView

A web view shows a web page, or HTML you pass in, inside the app: WKWebView on iOS and Mac Catalyst,
`android.webkit.WebView` on Android. Because NSH renders with native views, a web view sits in the layout like any other
view, in order with the views around it. Rust makes one with `web_view(url)` or `web_view_html(html, base_url)`, TSX
with `<WebView>` from `nsh`.

```rust tabs
use nsh_core::prelude::*;

fn browser() -> impl IntoElement {
    let title = signal(SharedString::from("Loading…"));
    view()
        .flex_1()
        .child(text(title).number_of_lines(1))
        .child(
            web_view("https://example.com")
                .flex_1()
                .on_navigation_state_change(move |e: &WebNavigation| title.set(e.title.clone().into())),
        )
}
```

```tsx tabs
import { signal, Text, View, WebView, type WebViewNavigation } from "nsh";

function Browser() {
  const nav = signal<WebViewNavigation | null>(null);
  return (
    <View style={{ flex: 1 }}>
      <Text style={{ numberOfLines: 1 }}>{nav()?.title ?? "Loading…"}</Text>
      <WebView
        src="https://example.com"
        style={{ flex: 1 }}
        onNavigationStateChange={(e) => nav.set(e)}
      />
    </View>
  );
}
```

In TSX a web view takes one of `src` and `html` (see the [WebView API](../modules/nsh-webview.md)). The
[web view and video guide](../docs/web-view-and-video.md) covers messaging patterns and capabilities.

## Props

A web view takes the props of a [view](./view.md) (it has no children) plus:

| Prop | Rust | Type | Default | Description |
|---|---|---|---|---|
| `src` | `web_view(url)` | `string` | | The URL of the page to show. |
| `html` | `web_view_html(html, base_url)`, `.html(html, base_url)` | `string` | | Inline HTML, shown instead of `src`. |
| `baseUrl` | `base_url` of `web_view_html` / `.html` | `string` | | What `html`'s relative links resolve against. It needs the same grant as a page there (`webview.hosts`); without it the HTML loads with no base URL and `onError` reports `ERR_ACCESS_DENIED`. |
| `injectedJavaScript` | `.injected_java_script(..)` | `string` | | Runs after each page loads. |
| `injectedJavaScriptBeforeContentLoaded` | `.injected_java_script_before_content(..)` | `string` | | Runs before each page's own scripts. |
| `userAgent` | `.user_agent(..)` | `string` | | The browser's user agent string. |
| `javaScriptEnabled` | `.java_script_enabled(..)` | `boolean` | `true` | Pages run scripts. |
| `scrollable` | `.scrollable(..)` | `boolean` | `true` | The user can scroll the page. |
| `incognito` | `.incognito(bool)` | `boolean` | `false` | The pages' website data (cookies, storage, the cache) lives only as long as the web view, apart from the app's; on Android the web view loads without the HTTP cache and saves no form data ([website data](../docs/web-view-and-video.md#website-data)). Read when the web view is made. |

| `ios` key | Rust | Type | Description |
|---|---|---|---|
| `allowsBackForwardNavigationGestures` | `.back_forward_gestures(..)` | `boolean` | Swipe from the edges to go back and forward. |

| `desktop` key | Rust | Type | Description |
|---|---|---|---|
| `allowsBackForwardNavigationGestures` | `.back_forward_gestures(..)` | `boolean` | macOS: swipe with two fingers on the trackpad to go back and forward; Windows: swipe on a touchscreen; Linux: swipe on the touchpad. |

## Events

| Event | Rust | Payload | When |
|---|---|---|---|
| `onLoadStart` | `.on_load_start` | `WebViewNavigation` | A page started loading. |
| `onLoad` | `.on_load` | `WebViewNavigation` | A page finished loading. |
| `onError` | `.on_error` | `{ message, code? }` (`&str`, the message) | Loading failed; `code` is `"ERR_ACCESS_DENIED"` for a page, `html` or `baseUrl` the app's capabilities do not grant, and `"ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"` where the platform shows no web view (Rust: also `.on_unavailable`, the message). |
| `onNavigationStateChange` | `.on_navigation_state_change` | `WebViewNavigation` | The URL, title, loading state or history changed. |
| `onMessage` | `.on_message` | `{ data: string }` (`&str`) | The page called `window.nsh.postMessage(data)`. |
| `onLoadProgress` | `.on_progress` | `{ progress: number \| null }` (`WebProgress`) | Loading advanced on a desktop web view; `null` means the engine cannot estimate it. |
| `onDownload` | `.on_download` | `WebViewDownload` (`WebDownload`) | A desktop download started, finished or failed. |

`WebViewNavigation` is `{ url, title, loading, canGoBack, canGoForward }`; in Rust `WebNavigation` with `url`, `title`,
`loading`, `can_go_back` and `can_go_forward`.

`WebView.available()` (Rust `web_view_available()`) resolves whether a web view shows a page here: `false` where the
platform has no web engine or the app's capabilities do not grant `webview` ([When there is no web
engine](../docs/web-view-and-video.md#when-there-is-no-web-engine)).

`WebViewDownload` is `{ state, id, url, filename, path, bytes?, message? }`: `state` is `"started"`, `"finished"`
(with the file's `bytes`) or `"failed"` (with its `message`). With an `onDownload` listener, what the page downloads
(an attachment, a type the web view does not show, a `download` link) is saved in the app's data directory as
`downloads/<id>/<filename>`, where `path` points, and the app reads it with `fs/promises`.

On macOS, Windows and Linux, adding the listener lets any loaded page allowed by the `webview` capability save downloads
automatically. The app is responsible for its storage budget and for removing completed files from `downloads/`
when it has finished using them. Removing the web view cancels downloads in progress and removes their partial
directories. Downloads report through `onDownload`; converting a navigation into a download keeps `onError`
for actual navigation failures. Filenames have control characters and bidirectional formatting controls removed,
and long names are shortened to 200 bytes while preserving an extension that fits.

## Ref methods

| Method | Rust | Description |
|---|---|---|
| `reload()` | `reload()` | Reloads the page. |
| `goBack()` / `goForward()` | `go_back()` / `go_forward()` | Moves in the page's history. |
| `stopLoading()` | `stop_loading()` | Stops loading. |
| `injectJavaScript(script)` | `inject_java_script(script)` | Runs JavaScript in the page. |
| `postMessage(data)` | `post_message(data)` | Sends a string to the page, which receives a `message` event on `window`. |
| `load({ src, headers })` | `load(WebLoad::new(src, headers)?)` | Loads `src` with `headers` (e.g. `Authorization`) on the page's own request only, not its subresources or later navigations ([guide](../docs/web-view-and-video.md#signed-in-pages-request-headers)). |

### Desktop controls in Rust

Rust hosts use the web view's `ElementRef` to capture its visible page, manage its cookies and inspection tools,
and decide downloads. Await each control in a component task. An unavailable control, an unmounted ref or a native
failure returns an error; pending requests also end when the view is removed or their deadline expires.

| Rust method | Successful result | Purpose |
|---|---|---|
| `capture_page()` | `Vec<u8>` | A PNG of the visible page. |
| `get_cookies(uri)` | `Vec<WebCookie>` | Cookies matching the URL, or all cookies in this view's store with `None`. |
| `set_cookies(cookies)` | `()` | Sets cookies after validating the complete input batch. |
| `set_inspectable(enabled)` | `()` | Enables or disables the platform's inspection tools. |
| `open_devtools()` | `()` | Opens developer tools on Linux and Windows. macOS uses its inspection menu. |
| `accept_download(destination)` | `()` | Accepts a pending download's destination; transfer completion arrives through `on_download`. |
| `cancel_download(id)` | `()` | Cancels a pending or active download. |

On macOS, enable inspection with `set_inspectable(true)` and use WebKit's inspection menu;
`open_devtools()` returns an unsupported-operation error.

`WebCookie` is in `nsh_core::webview`. It carries `name`, `value`, `domain`, `path`, `http_only`, `secure`,
`expires` (Unix milliseconds, `None` for a session cookie), and `same_site` (`CookieSameSite`). Cookie access uses
the target view's own store, including its `incognito` store. Views sharing a store share its cookies.
A domain beginning with `.` also matches its subdomains; an undotted domain is host-only.

#### Choosing a download destination

Automatic download behavior is the default. A Rust host chooses `WebDownloadBehavior::Manual` with
`.download_behavior(..)` and handles `.on_download_pending(..)`. The pending event supplies the download's `id`,
`url` and suggested `filename`; the host accepts or cancels that id. Destination acceptance and final transfer
completion are separate outcomes.

```rust
use nsh_core::prelude::*;
use nsh_core::webview::{WebDownloadBehavior, WebDownloadDestination, WebDownloadPending};

let web = element_ref();
web_view("https://example.com")
    .element_ref(&web)
    .download_behavior(WebDownloadBehavior::Manual)
    .on_download_pending(move |event: &WebDownloadPending| {
        let id = event.id.clone();
        spawn(async move {
            // A host may instead ask the user for an authorized destination.
            let mut destination = WebDownloadDestination::default();
            destination.id = id;
            destination.path = std::env::temp_dir().join("download.bin");
            if let Err(error) = web.accept_download(destination).await {
                log::warn!("Download destination rejected: {error}");
            }
        });
    });
```

A destination is an absolute host path. Existing files are protected unless the host explicitly enables
`overwrite`. The host is responsible for selecting a path it is authorized to use. Removing the view cancels
unanswered decisions and active downloads and cleans their owned partial files.
Manual transfers use a separate staging file and publish the final path only on completion. The default
no-overwrite publication requires filesystem support for hard links; an unsupported filesystem reports an error
rather than replacing an existing file.

#### Content filtering

`.content_rules(..)` takes `ContentRuleList` values, each with an `id` and WebKit-style JSON `rules`. The desktop
host installs the policy before loading the page. Replacing the lists replaces the policy; an empty list clears
it. Compilation failures report through `on_error` and retain the previously installed policy.

macOS and Linux compile rules with WebKit. Windows supports `block`, `ignore-previous-rules` and main-document
`css-display-none`, with strict validation of the supported triggers and resource types. `block-cookies`,
`make-https`, unsupported resource types and load-type-conditioned cosmetic exceptions return errors. Windows
uses the Public Suffix List for site boundaries; if it cannot determine a document request's top frame, a matching
top-dependent rule refuses that request with HTTP 403 instead of guessing. Choose rules supported by the engines
the app targets.


## Messaging

The app and the page exchange strings. The page posts with `window.nsh.postMessage(data)`; the app posts
with the ref's `postMessage(data)` (`post_message(..)` in Rust). Send JSON for structured data.

```rust tabs
use nsh_core::prelude::*;

const PAGE: &str = r#"<!doctype html>
<meta name="viewport" content="width=device-width">
<button onclick="window.nsh.postMessage('hello from the page')">Post to the app</button>
<p id="got"></p>
<script>
  window.addEventListener("message", (e) => { document.getElementById("got").textContent = e.data; });
</script>"#;

fn bridge() -> impl IntoElement {
    let message = signal(String::new());
    let web = element_ref();
    view()
        .gap_3()
        .child(
            web_view_html(PAGE.to_string(), None)
                .element_ref(&web)
                .h(px(200.0))
                .on_message(move |data: &str| message.set(data.to_string())),
        )
        .child(
            text(move || format!("Post to the page · got: {}", message.get()))
                .on_press(move || web.post_message("hi from the app")),
        )
}
```

```tsx tabs
import { signal, Text, View, WebView, type WebViewRef } from "nsh";

const page = `<!doctype html>
<meta name="viewport" content="width=device-width">
<button onclick="window.nsh.postMessage('hello from the page')">Post to the app</button>
<p id="got"></p>
<script>
  window.addEventListener("message", (e) => { document.getElementById("got").textContent = e.data; });
</script>`;

function Bridge() {
  const message = signal("");
  let web!: WebViewRef;
  return (
    <View style={{ gap: 12 }}>
      <WebView ref={web} html={page} style={{ height: 200 }} onMessage={(e) => message.set(e.data)} />
      <Text onPress={() => web.postMessage("hi from the app")}>Post to the page · got: {message()}</Text>
    </View>
  );
}
```

## Capabilities

When the app declares [capabilities](../docs/sandbox-and-capabilities.md) in `nsh.json`, a web view shows and
navigates to the hosts listed under `webview`. A `src` on another host, or `html` in an app without `webview`, shows an
empty view; a `baseUrl` on another host is dropped, and the HTML loads without it; a top-level navigation to another
host (a link the user taps) is cancelled natively. Each reports
`onError` with `code` `"ERR_ACCESS_DENIED"`.

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

## Platform notes

- iOS loads WebKit the first time a web view is created, so it costs an app something only once it shows one.
- On macOS `onDownload` saves downloads with WebKit's `WKDownload`.
- On Android the page's `postMessage` calls arrive on a background thread and are forwarded to the app in order.
- To round the corners of a web view, give it a corner radius and `overflow: "hidden"` (`.overflow_hidden()`).

---
title: WebView
description: WebView, a native web page view (WKWebView, android.webkit.WebView), with its props, events and ref methods.
order: 2.35
---

# WebView

`nsh` exports `WebView`: a web page shown by the platform's own browser engine (`WKWebView` on iOS and Mac Catalyst, `android.webkit.WebView` on Android). It is a native view laid out like any other, so it scrolls, clips and overlaps other views correctly.

```ts
import { WebView, type WebViewNavigation, type WebViewProps } from "nsh";
```

See [Web view and video](../docs/web-view-and-video.md) for the guide and [`<WebView>`](../elements/web-view.md) for its props.

## WebView

```ts
const WebView: (props: WebViewProps) => JSX.Element;

interface WebViewProps extends Omit<ViewProps, "children" | "ios" | "desktop">, WebViewHostProps {
  ios?: IOSWebViewHostProps;
  desktop?: DesktopWebViewHostProps;
}
```

```tsx
import { mount, signal, Text, View, WebView, type WebViewNavigation, type WebViewRef } from "nsh";

function Browser() {
  const nav = signal<WebViewNavigation | null>(null);
  let web!: WebViewRef;
  return (
    <View style={{ flex: 1 }}>
      <View style={{ flexDirection: "row", gap: 16, padding: 12 }}>
        <Text onPress={() => web.goBack()} style={{ opacity: nav()?.canGoBack ? 1 : 0.4 }}>Back</Text>
        <Text onPress={() => web.reload()}>Reload</Text>
        <Text style={{ numberOfLines: 1, flex: 1 }}>{nav()?.title ?? ""}</Text>
      </View>
      <WebView ref={web} src="https://example.com" style={{ flex: 1 }} onNavigationStateChange={(n) => nav.set(n)} />
    </View>
  );
}

mount(Browser);
```

## Props

`WebView` takes the [view props](../elements/view.md) (style, classes, layout, accessibility, `ref`; no children), plus:

| Prop | Type | Default | Description |
|---|---|---|---|
| `src` | `string` | | The URL of the page to show. |
| `html` | `string` | | Inline HTML, shown instead of `src`. |
| `baseUrl` | `string` | | What `html`'s relative links resolve against. It needs the same grant as a page there (`webview.hosts`); without it the HTML loads with no base URL and `onError` reports `ERR_ACCESS_DENIED`. |
| `injectedJavaScript` | `string` | | Runs after each page loads. |
| `injectedJavaScriptBeforeContentLoaded` | `string` | | Runs before each page's own scripts. |
| `userAgent` | `string` | platform's | The user agent string. |
| `scrollable` | `boolean` | `true` | The user can scroll the page. |
| `javaScriptEnabled` | `boolean` | `true` | Pages run scripts. |
| `incognito` | `boolean` | `false` | The pages' website data (cookies, storage, the cache) lives only as long as the web view, apart from the app's; on Android the web view loads without the HTTP cache and saves no form data ([website data](../docs/web-view-and-video.md#website-data)). Read when the web view is made. |
| `ios` | `{ allowsBackForwardNavigationGestures?: boolean }` | | iOS only: swipe from the edges to go back and forward. |
| `desktop` | `{ allowsBackForwardNavigationGestures?: boolean }` | | macOS: two-finger trackpad swipes go back and forward; Windows: touchscreen swipes; Linux: touchpad swipes. |

## Events

| Event | Payload | When |
|---|---|---|
| `onLoadStart` | `WebViewNavigation` | A page starts loading. |
| `onLoad` | `WebViewNavigation` | A page finished loading. |
| `onError` | `{ message: string }` | A page failed to load. |
| `onNavigationStateChange` | `WebViewNavigation` | The URL, title, loading state or history changed. |
| `onMessage` | `{ data: string }` | The page called `window.nsh.postMessage(data)`. |

```ts
interface WebViewNavigation {
  url: string;
  title: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}
```

## Ref methods

| Method | Description |
|---|---|
| `reload()` | Reloads the page. |
| `goBack()` | Goes back in its history. |
| `goForward()` | Goes forward in its history. |
| `stopLoading()` | Stops loading. |
| `injectJavaScript(script)` | Runs `script` in the page. |
| `evaluateJavaScript(script)` | Resolves the script's strict JSON completion value; see [Awaiting JavaScript](#awaiting-javascript). |
| `postMessage(data)` | Sends `data` (a string) to the page: a `message` event on `window`, with `data` as `event.data`. |
| `load({ src, headers? })` | Loads `src` with `headers` (`Record<string, string>`) on the page's own request only, not on its subresources, later navigations or reloads; for a signed-in page (`Authorization`). The host must be one `webview.hosts` allows. An invalid header throws a `TypeError`. The browser backend does nothing. See [Signed-in pages](../docs/web-view-and-video.md#signed-in-pages-request-headers). |

## Talking to the page

The page sends strings to the app with `window.nsh.postMessage(…)`; the app sends strings to the page with `ref.postMessage(…)`. Send JSON for structured data.

```tsx
const page = `<!doctype html><html><head><meta name="viewport" content="width=device-width"></head>
<body>
  <button onclick="window.nsh.postMessage('hello from the page')">Post to the app</button>
  <p id="got"></p>
  <script>
    window.addEventListener('message', (e) => { document.getElementById('got').textContent = e.data; });
  </script>
</body></html>`;

let web!: WebViewRef;
const message = signal("");

<WebView ref={web} html={page} style={{ height: 200 }} onMessage={(e) => message.set(e.data)} />
<Text onPress={() => web.postMessage("hi from the app")}>Post to the page</Text>
```

## Capabilities

When your app declares `capabilities` in `nsh.json`, a web view may only show and navigate to the hosts listed under `webview`; other top-level navigations are refused natively. An app without a `capabilities` key may show anything; on macOS its files keep to one folder: a web view whose `src` is a `file://` page may open files in that page's folder (and below it), and no file outside it. See [Sandbox and capabilities](../docs/sandbox-and-capabilities.md).

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

## Awaiting JavaScript

`await ref.evaluateJavaScript(script)` evaluates a script in this WebView's
main document and resolves its completion value. Call after `onLoad` when the
operation needs the loaded document; evaluation itself does not wait for loading.
A returned Promise or thenable is awaited on iOS and Android. To use `await`
inside the script, return an async IIFE:

```tsx
const title = await web.evaluateJavaScript("document.title");
const buttons = await web.evaluateJavaScript(`
  Array.from(document.querySelectorAll("button")).map(button => ({
    text: button.textContent,
    disabled: button.disabled
  }))
`);
const value = await web.evaluateJavaScript("(async () => await Promise.resolve(42))()");
```

The value is strict JSON: strings, finite numbers, booleans, `null`, arrays and
plain objects (including null-prototype objects). `undefined`, functions, symbols,
BigInt, NaN/Infinity, circular values and nonplain objects such as DOM nodes,
Date and Map reject, including when nested. Convert DOM content explicitly as in
the example. Enumerable object properties and array entries are copied; getters
run during copying. Serialization failures reject instead of silently dropping
properties or substituting `null`. Results are limited to 1 MiB of serialized
UTF-8, nesting to 64 levels, scripts to 1 MiB, and outstanding calls to 64 per
engine. An evaluation has a 30-second deadline.

Pending calls reject when a new main-document navigation starts, even if that
navigation later fails. Explicit `load`, `reload`, `goBack` and `goForward` cancel
before issuing their native operation (including a history command that stays
within one document). Prop changes that reload content also cancel. A blocked
navigation, a subframe load, or a page-initiated same-document hash/history update
keeps calls pending. Disposing the WebView or losing its renderer rejects calls;
late callbacks cannot settle another call. Cancelling a call releases its native
bookkeeping but does not undo JavaScript side effects or forcibly stop page code.

Failures reject with an `Error` carrying a stable `code`:

| Code | Meaning |
|---|---|
| `ERR_NSH_WEBVIEW_JAVASCRIPT` | Script exception, syntax error or Promise rejection. |
| `ERR_NSH_WEBVIEW_RESULT` | Unsupported value, serialization failure or invalid result. |
| `ERR_NSH_WEBVIEW_NAVIGATION` | Navigation invalidated the pending evaluation. |
| `ERR_NSH_WEBVIEW_DISPOSED` | The ref no longer names a live WebView. |
| `ERR_NSH_WEBVIEW_NATIVE` | Native evaluation, disabled scripting or renderer failure. |
| `ERR_NSH_QUOTA_EXCEEDED` | Script, result or outstanding-request limit exceeded. |
| `ETIMEDOUT` | The evaluation exceeded its deadline. |
| `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` | Evaluation requested on another backend. |

Wrong arguments reject with a `TypeError`; the method returns a Promise rather
than throwing synchronously. iOS uses WKWebView's native completion handler;
Android uses `evaluateJavascript`, with a shared wrapper to report exceptions
that Android's native callback otherwise cannot distinguish from `null`. Returned
Promises use bounded native polling. Replies are separate from `onMessage`.
The page's Content Security Policy applies to the evaluated source; a blocked
`eval` rejects with `ERR_NSH_WEBVIEW_JAVASCRIPT`.
Page results remain untrusted. Existing WebView host restrictions and sandbox
boundaries apply; evaluation is only in the app-owned WebView's page world.
`injectJavaScript` remains the fire-and-forget method.

Rust components use their existing element ref on the engine thread:

```rust
let web = element_ref();
// Attach with web_view("https://example.com").element_ref(&web), then await from an event/task.
spawn(async move {
    match web.evaluate_java_script("document.title").await {
        Ok(value) => log::info!("Page title: {value}"),
        Err(error) => log::warn!("Evaluation failed: {}", error.message()),
    }
});
```

The future returns `Result<nsh_core::JsonValue, nsh_core::element::JsError>`;
dropping it cancels the request. Component tasks end with their owner.

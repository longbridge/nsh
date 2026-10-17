---
title: WebView
description: "The WebView component: web pages and inline HTML inside an app, with messaging both ways."
order: 9
---

# WebView

A web view shows a web page, or HTML you pass in, inside the app: WKWebView on iOS and Mac Catalyst,
`android.webkit.WebView` on Android. Because NSH renders with native views, a web view sits in the layout like any other
view: no overlay, no layering problems with views around it.

```tsx
import { signal, Text, View } from "nsh";
import { WebView, type WebViewNavigation } from "nsh/webview";

function Browser() {
  const nav = signal<WebViewNavigation | null>(null);
  return (
    <View style={{ flex: 1 }}>
      <Text numberOfLines={1}>{nav()?.title ?? "Loading…"}</Text>
      <WebView
        source="https://example.com"
        style={{ flex: 1 }}
        onNavigationStateChange={(e) => nav.set(e)}
      />
    </View>
  );
}
```

`WebView` comes from [`nsh/webview`](../modules/nsh-webview.md); `source` is required. The
[web view and video guide](../docs/web-view-and-video.md) covers messaging patterns and capabilities.

## Props

`WebView` takes the props of a [view](./view.md) (without `children`) plus:

| Prop | Type | Default | Description |
|---|---|---|---|
| `source` | `string \| { uri: string } \| { html: string; baseUrl?: string }` | | A URL, or an HTML string with the base URL its relative links resolve against. |
| `injectedJavaScript` | `string` | | Runs after each page loads. |
| `injectedJavaScriptBeforeContentLoaded` | `string` | | Runs before each page's own scripts. |
| `userAgent` | `string` | | The browser's user agent string. |
| `javaScriptEnabled` | `boolean` | `true` | Pages run scripts. |
| `scrollEnabled` | `boolean` | `true` | The user can scroll the page. |

| `ios` key | Type | Description |
|---|---|---|
| `allowsBackForwardNavigationGestures` | `boolean` | Swipe from the edges to go back and forward. |

| `desktop` key | Type | Description |
|---|---|---|
| `allowsBackForwardNavigationGestures` | `boolean` | macOS: swipe with two fingers on the trackpad to go back and forward; Windows: swipe on a touchscreen; Linux: swipe on the touchpad. |

## Events

| Event | Payload | When |
|---|---|---|
| `onLoadStart` | `WebViewNavigation` | A page started loading. |
| `onLoad` | `WebViewNavigation` | A page finished loading. |
| `onError` | `{ message }` | Loading failed. |
| `onNavigationStateChange` | `WebViewNavigation` | The URL, title, loading state or history changed. |
| `onMessage` | `{ data: string }` | The page called `window.ReactNativeWebView.postMessage(data)`. |

`WebViewNavigation` is `{ url, title, loading, canGoBack, canGoForward }`.

## Ref methods

| Method | Description |
|---|---|
| `reload()` | Reloads the page. |
| `goBack()` / `goForward()` | Moves in the page's history. |
| `stopLoading()` | Stops loading. |
| `injectJavaScript(script)` | Runs JavaScript in the page. |
| `postMessage(data)` | Sends a string to the page, which receives a `message` event on `window`. |

## Messaging

The app and the page exchange strings. The page posts with `window.ReactNativeWebView.postMessage(data)`; the app posts
with the ref's `postMessage(data)`. Send JSON for structured data.

```tsx
import { signal, Text, View, type YNode } from "nsh";
import { WebView } from "nsh/webview";

const page = `<!doctype html>
<meta name="viewport" content="width=device-width">
<button onclick="window.ReactNativeWebView.postMessage('hello from the page')">Post to the app</button>
<p id="got"></p>
<script>
  window.addEventListener("message", (e) => { document.getElementById("got").textContent = e.data; });
</script>`;

function Bridge() {
  const message = signal("");
  let web!: YNode;
  return (
    <View style={{ gap: 12 }}>
      <WebView ref={web} source={{ html: page }} style={{ height: 200 }} onMessage={(e) => message.set(e.data)} />
      <Text onPress={() => web.postMessage("hi from the app")}>Post to the page · got: {message()}</Text>
    </View>
  );
}
```

## Capabilities

If the app declares [capabilities](../docs/sandbox-and-capabilities.md) in `nsh.json`, a web view may only show and
navigate to the hosts listed under `webview`. A refused `source` shows nothing; a refused top-level navigation (a
link the user taps) is cancelled natively.

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

## Platform notes

- iOS loads WebKit the first time a web view is created, so apps that never show one do not pay for it.
- On Android the page's `postMessage` calls arrive on a background thread and are forwarded to the app in order.
- To round the corners of a web view, give it `borderRadius` and `overflow: "hidden"`.

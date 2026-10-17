---
title: Web view and video
description: Showing web pages with WebView and playing video with Video - sources, messaging between the page and the app, navigation, playback control, refs and capabilities.
order: 45
group: ui
---

# Web view and video

`WebView` from `nsh/webview` and `Video` from `nsh/video` embed the platform's own web engine and media player as ordinary views. Because NSH renders with native views, they sit in your layout like any other element: they scroll with their parent, take rounded corners and clipping, and other views can overlap them without layering problems.

| Component | iOS | Android | macOS | Linux | Windows |
|---|---|---|---|---|---|
| `WebView` | WKWebView | `android.webkit.WebView` | WKWebView | WebKitGTK 6.0's `WebKitWebView` | WebView2, in a child window |
| `Video` | AVPlayerLayer; AVPlayerViewController with `controls` | MediaPlayer on a TextureView; MediaController with `controls` | AVPlayerLayer; AVKit's AVPlayerView with `controls` | GTK's media stream (GStreamer) in a `GtkPicture`; `GtkVideo` with `controls` | Media Foundation's media engine, frames drawn with Direct2D; NSH's own control bar with `controls` |

The frameworks behind them (WebKit, AVFoundation) are loaded the first time an app uses one, so apps that never show a web page or a video do not pay for them at launch. Import them from `nsh/webview` and `nsh/video`.

On [macOS](./macos.md) the web view has no scroll view of its own: with `scrollEnabled={false}` the mouse wheel and the
trackpad scroll the app's scroll views around it instead of the page, which still scrolls with its keyboard and its
own scripts.

On [Linux](./linux.md) a web view is a WebKitGTK page drawn inside its view: NSH content over it (a dialog, a sheet)
covers it, and its parents clip it. The `webview` capability also applies to the navigations of frames there (an
`iframe` of a host that is not granted stays empty), and `scrollEnabled: false` stops the page itself from scrolling
(elements inside it still scroll). Apps need WebKitGTK 6.0 installed; a runtime built without it shows a neutral
placeholder. Videos play the formats the installed GStreamer plugins decode; `rate` is not supported (they
play at 1×), and `onEnd` only comes for videos that do not loop.

On [Windows](./windows.md) a web view is a WebView2 control in a child window that follows its view: it is clipped to
its parents and rounded corners, but NSH content drawn over it (a dialog, a sheet) stays under it. It needs the
WebView2 Runtime (part of Windows 11; installed with Edge on Windows 10), else it shows a placeholder;
`source.baseUrl` for HTML, `scrollEnabled` and `allowsBackForwardNavigationGestures` are not supported. Videos are
ordinary content (clipped, transformed and covered like any view) and play the formats the installed Media Foundation
codecs decode; `rate` works, there is no poster, and the control bar always shows while `controls` is on.

## WebView

```tsx
import { mount } from "nsh";
import { WebView } from "nsh/webview";

function App() {
  return <WebView source="https://en.m.wikipedia.org/wiki/Swift_(programming_language)" style={{ flex: 1 }} />;
}

mount(App);
```

### Sources

| `source` | Shows |
|---|---|
| `"https://…"` or `{ uri: "https://…" }` | A web page. |
| `{ html: "<!doctype html>…" }` | HTML from a string. |
| `{ html, baseUrl: "https://example.com/" }` | HTML whose relative URLs resolve against `baseUrl`. |

Changing `source` loads the new page.

### Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `source` | `string \| { uri } \| { html, baseUrl? }` | | What to show. |
| `injectedJavaScript` | `string` | | Runs after each page loads. |
| `injectedJavaScriptBeforeContentLoaded` | `string` | | Runs before each page's own scripts. |
| `javaScriptEnabled` | `boolean` | `true` | Pages run scripts. |
| `scrollEnabled` | `boolean` | `true` | The user can scroll the page. |
| `userAgent` | `string` | | The browser's user agent string. |
| `ios={{ allowsBackForwardNavigationGestures }}` | `boolean` | | iOS: swipe back and forward through the page history. |
| `desktop={{ allowsBackForwardNavigationGestures }}` | `boolean` | | macOS: swipe with two fingers on the trackpad to go back and forward; Windows: swipe on a touchscreen; Linux: swipe on the touchpad. |

Events:

| Event | Payload | When |
|---|---|---|
| `onLoadStart` | `WebViewNavigation` | A page started loading. |
| `onLoad` | `WebViewNavigation` | A page finished loading. |
| `onError` | `{ message }` | Loading failed. |
| `onNavigationStateChange` | `WebViewNavigation` | The URL, title, loading state or history changed. |
| `onMessage` | `{ data: string }` | The page posted a message (below). |

`WebViewNavigation` is `{ url, title, loading, canGoBack, canGoForward }`.

### A browser with back and forward

The ref has `reload()`, `goBack()`, `goForward()`, `stopLoading()`, `injectJavaScript(script)` and `postMessage(data)`:

```tsx
import { mount, signal, Text, View, type YNode } from "nsh";
import { WebView, type WebViewNavigation } from "nsh/webview";

function Browser() {
  let web!: YNode;
  const nav = signal<WebViewNavigation | null>(null);
  const button = (label: string, enabled: boolean, action: () => void) => (
    <Text onPress={enabled ? action : undefined} style={{ padding: 12, color: enabled ? "#0969da" : "#8c959f" }}>
      {label}
    </Text>
  );
  return (
    <View style={{ flex: 1, paddingTop: 50 }}>
      <Text numberOfLines={1} style={{ paddingHorizontal: 12, color: "#57606a" }}>
        {nav()?.loading ? "Loading…" : nav()?.title ?? ""}
      </Text>
      <WebView
        ref={web}
        source="https://developer.mozilla.org/"
        style={{ flex: 1 }}
        onNavigationStateChange={(n) => nav.set(n)}
        ios={{ allowsBackForwardNavigationGestures: true }}
      />
      <View style={{ flexDirection: "row", justifyContent: "space-around" }}>
        {button("Back", !!nav()?.canGoBack, () => web.goBack())}
        {button("Forward", !!nav()?.canGoForward, () => web.goForward())}
        {button("Reload", true, () => web.reload())}
      </View>
    </View>
  );
}

mount(Browser);
```

### Messaging between the page and the app

Pages talk to the app with `window.ReactNativeWebView.postMessage(string)`, which arrives as `onMessage`. The app talks to the page with the ref's `postMessage(string)`, which arrives in the page as a `message` event on `window`. Messages are strings; send JSON for structured data.

```tsx
import { mount, signal, Text, View, type YNode } from "nsh";
import { WebView } from "nsh/webview";

const page = `<!doctype html>
<html><head><meta name="viewport" content="width=device-width"></head>
<body style="font-family: -apple-system, sans-serif; margin: 16px">
  <button onclick="window.ReactNativeWebView.postMessage(JSON.stringify({ type: 'hello', at: Date.now() }))">
    Post to the app
  </button>
  <p id="got">Nothing from the app yet.</p>
  <script>
    window.addEventListener("message", (e) => {
      document.getElementById("got").textContent = "The app says: " + e.data;
    });
  </script>
</body></html>`;

function App() {
  let web!: YNode;
  const last = signal("");
  return (
    <View style={{ flex: 1, paddingTop: 50, gap: 12 }}>
      <WebView
        ref={web}
        source={{ html: page }}
        style={{ height: 240 }}
        onMessage={(e) => last.set(JSON.parse(e.data).type)}
      />
      <Text onPress={() => web.postMessage("hi from nsh")} style={{ padding: 16, color: "#0969da" }}>
        Post to the page
      </Text>
      <Text style={{ paddingHorizontal: 16 }}>Last message: {last()}</Text>
    </View>
  );
}

mount(App);
```

`injectJavaScript(script)` runs code in the current page at any time; `injectedJavaScript` and `injectedJavaScriptBeforeContentLoaded` run on every page load, before or after the page's own scripts, which is the place to install hooks:

```tsx
<WebView
  source="https://example.com"
  injectedJavaScriptBeforeContentLoaded={`window.isNativeApp = true;`}
  injectedJavaScript={`window.ReactNativeWebView.postMessage(document.title);`}
  onMessage={(e) => console.log("title", e.data)}
/>
```

### Which sites a web view may show

If your `nsh.json` declares `"capabilities"`, a web view may only show and navigate to the hosts listed under `webview`:

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

The check covers the web view's `source` and every top-level navigation the page makes (links, redirects, `location` changes); it is decided natively when the platform asks whether a navigation may proceed. A page's own subresources (scripts, images, frames) are the web content's business and are not checked. `*.example.com` matches subdomains only; list `example.com` as well for the bare domain. An app without a `"capabilities"` key may show any page. See [Sandbox and capabilities](./sandbox-and-capabilities.md).

## Video

```tsx
import { mount, signal, Text, View, type YNode } from "nsh";
import { Video } from "nsh/video";

function Player() {
  let video!: YNode;
  const paused = signal(true);
  const time = signal("");
  return (
    <View style={{ flex: 1, justifyContent: "center", padding: 16, gap: 12 }}>
      <Video
        ref={video}
        source="https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4"
        paused={paused()}
        repeat
        resizeMode="contain"
        style={{ height: 220, borderRadius: 12, overflow: "hidden", backgroundColor: "#000000" }}
        onProgress={(e) => time.set(`${e.currentTime.toFixed(1)} / ${e.duration.toFixed(1)} s`)}
        onEnd={() => paused.set(true)}
      />
      <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
        <Text onPress={() => paused.set(!paused())} style={{ color: "#0969da", fontWeight: "600" }}>
          {paused() ? "Play" : "Pause"}
        </Text>
        <Text onPress={() => video.seek(0)} style={{ color: "#0969da" }}>
          Restart
        </Text>
        <Text style={{ color: "#57606a" }}>{time()}</Text>
      </View>
    </View>
  );
}

mount(Player);
```

Playback is controlled by props: set `paused`, `muted`, `volume` and `rate` from signals and the player follows. The ref has `seek(seconds)`.

| Prop | Type | Default | Description |
|---|---|---|---|
| `source` | `string \| { uri }` | | The video's URL. |
| `paused` | `boolean` | `false` | Playback is paused. |
| `muted` | `boolean` | `false` | No sound. |
| `volume` | `number` | `1` | 0..1. |
| `rate` | `number` | `1` | The playback speed. |
| `repeat` | `boolean` | `false` | Plays again from the start at the end. |
| `resizeMode` | `"cover" \| "contain" \| "stretch" \| "center"` | | How the picture fits the view. |
| `controls` | `boolean` | `false` | The platform's playback controls. |
| `progressUpdateInterval` | `number` | `250` | Milliseconds between `onProgress` events. |

| Event | Payload | When |
|---|---|---|
| `onLoadStart` | | Loading started. |
| `onLoad` | `{ duration, width, height }` | The video is ready: its length in seconds and its size. |
| `onProgress` | `{ currentTime, duration }` | While playing, every `progressUpdateInterval` ms. |
| `onBuffer` | `{ isBuffering }` | Playback stalled waiting for data, or resumed. |
| `onEnd` | | Playback reached the end. |
| `onError` | `{ message }` | Loading or playback failed. |

With `controls`, the platform draws its own play button, scrubber and full-screen button. Without, draw your own with views over or below the video; since the video is a native view in your layout, overlays work naturally:

```tsx
<View style={{ height: 220 }}>
  <Video source={url} paused={paused()} style={{ flex: 1 }} />
  <View
    onPress={() => paused.set(!paused())}
    style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, alignItems: "center", justifyContent: "center" }}
  >
    <If when={paused()}>
      <Text style={{ fontSize: 48, color: "#ffffff" }}>▶</Text>
    </If>
  </View>
</View>
```

Video sources are network requests: with declared capabilities, the video's host must be allowed under `network` in `nsh.json`, as for `fetch` and remote images. A refused source shows nothing, and the refusal is logged once with the key to add.

## Related

- [`WebView`](../elements/web-view.md) and [`Video`](../elements/video.md): the element references.
- [`nsh/webview`](../modules/nsh-webview.md) and [`nsh/video`](../modules/nsh-video.md).
- [Sandbox and capabilities](./sandbox-and-capabilities.md): `webview` and `network` grants.
- [Native components](./native-components.md): embedding other native views your host app provides.

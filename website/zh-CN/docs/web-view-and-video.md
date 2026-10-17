---
title: 网页视图与视频
description: 用 WebView 显示网页、用 Video 播放视频：来源、页面与应用之间的消息、导航、播放控制、ref 以及能力声明。
order: 45
group: ui
---

# 网页视图与视频

`nsh/webview` 中的 `WebView` 和 `nsh/video` 中的 `Video` 把平台自己的网页引擎和媒体播放器作为普通视图嵌入。由于 NSH 用原生视图渲染，它们在布局中与其他元素无异：随父视图滚动、支持圆角和裁剪，其他视图可以覆盖在它们上面而不会出现层级问题。

| 组件 | iOS | Android | macOS | Linux | Windows |
|---|---|---|---|---|---|
| `WebView` | WKWebView | `android.webkit.WebView` | WKWebView | WebKitGTK 6.0 的 `WebKitWebView` | WebView2，位于子窗口中 |
| `Video` | AVPlayerLayer；设置 `controls` 时为 AVPlayerViewController | TextureView 上的 MediaPlayer；设置 `controls` 时为 MediaController | AVPlayerLayer；设置 `controls` 时为 AVKit 的 AVPlayerView | `GtkPicture` 中的 GTK 媒体流（GStreamer）；设置 `controls` 时为 `GtkVideo` | Media Foundation 媒体引擎，画面用 Direct2D 绘制；设置 `controls` 时为 NSH 自己的控制栏 |

它们背后的框架（WebKit、AVFoundation）在应用第一次用到时才加载，所以从不显示网页或视频的应用不会在启动时为它们付出代价。请从 `nsh/webview` 和 `nsh/video` 导入它们。

在 [macOS](./macos.md) 上，Web 视图没有自己的滚动视图：`scrollEnabled={false}` 时，鼠标滚轮和触控板滚动的是它外面的应用滚动视图，
而不是页面；页面仍可用键盘和它自己的脚本滚动。

在 [Linux](./linux.md) 上，Web 视图是绘制在它的视图之内的 WebKitGTK 页面：绘制在它上方的 NSH 内容（对话框、sheet）会遮住它，父视图会裁剪它。
那里的 `webview` 权限同样作用于框架的导航（未获授权的主机的 `iframe` 保持空白），`scrollEnabled: false` 让页面本身不滚动（页面中的元素仍可滚动）。
应用需要装有 WebKitGTK 6.0；不带它构建的运行时显示中性的占位视图。视频能播放哪些格式取决于已安装的
GStreamer 插件；不支持 `rate`（始终以 1× 播放），`onEnd` 只对不循环的视频触发。

在 [Windows](./windows.md) 上，Web 视图是位于子窗口中的 WebView2 控件，跟随它的视图移动：它会被父视图和圆角裁剪，但绘制在它上方的
NSH 内容（对话框、sheet）会被它遮住。它需要 WebView2 Runtime（Windows 11 自带，Windows 10 上随 Edge 安装），否则显示占位视图；
不支持 HTML 的 `source.baseUrl`、`scrollEnabled` 和 `allowsBackForwardNavigationGestures`。视频是普通内容（像其他视图一样被裁剪、
变换和遮挡），能播放哪些格式取决于已安装的 Media Foundation 解码器；`rate` 可用，没有封面图（poster），开启 `controls` 时控制栏始终显示。

## WebView

```tsx
import { mount } from "nsh";
import { WebView } from "nsh/webview";

function App() {
  return <WebView source="https://en.m.wikipedia.org/wiki/Swift_(programming_language)" style={{ flex: 1 }} />;
}

mount(App);
```

### 来源

| `source` | 显示内容 |
|---|---|
| `"https://…"` 或 `{ uri: "https://…" }` | 一个网页。 |
| `{ html: "<!doctype html>…" }` | 字符串形式的 HTML。 |
| `{ html, baseUrl: "https://example.com/" }` | 相对 URL 以 `baseUrl` 为基准解析的 HTML。 |

改变 `source` 会加载新的页面。

### 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `source` | `string \| { uri } \| { html, baseUrl? }` | | 要显示的内容。 |
| `injectedJavaScript` | `string` | | 每个页面加载完成后运行。 |
| `injectedJavaScriptBeforeContentLoaded` | `string` | | 在每个页面自己的脚本之前运行。 |
| `javaScriptEnabled` | `boolean` | `true` | 页面可以运行脚本。 |
| `scrollEnabled` | `boolean` | `true` | 用户可以滚动页面。 |
| `userAgent` | `string` | | 浏览器的 user agent 字符串。 |
| `ios={{ allowsBackForwardNavigationGestures }}` | `boolean` | | iOS：滑动在页面历史中前进后退。 |
| `desktop={{ allowsBackForwardNavigationGestures }}` | `boolean` | | macOS：双指在触控板上轻扫，在页面历史中前进后退；Windows：在触摸屏上轻扫；Linux：在触控板上轻扫。 |

事件：

| 事件 | 参数 | 触发时机 |
|---|---|---|
| `onLoadStart` | `WebViewNavigation` | 页面开始加载。 |
| `onLoad` | `WebViewNavigation` | 页面加载完成。 |
| `onError` | `{ message }` | 加载失败。 |
| `onNavigationStateChange` | `WebViewNavigation` | URL、标题、加载状态或历史改变。 |
| `onMessage` | `{ data: string }` | 页面发来消息（见下文）。 |

`WebViewNavigation` 为 `{ url, title, loading, canGoBack, canGoForward }`。

### 带前进后退的浏览器

ref 提供 `reload()`、`goBack()`、`goForward()`、`stopLoading()`、`injectJavaScript(script)` 和 `postMessage(data)`：

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

### 页面与应用之间的消息

页面通过 `window.ReactNativeWebView.postMessage(string)` 向应用发消息，应用收到 `onMessage`。应用通过 ref 的 `postMessage(string)` 向页面发消息，页面在 `window` 上收到 `message` 事件。消息是字符串；结构化数据请发送 JSON。

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

`injectJavaScript(script)` 可以随时在当前页面中运行代码；`injectedJavaScript` 和 `injectedJavaScriptBeforeContentLoaded` 在每次页面加载时运行，分别在页面自己的脚本之后或之前，适合用来安装钩子：

```tsx
<WebView
  source="https://example.com"
  injectedJavaScriptBeforeContentLoaded={`window.isNativeApp = true;`}
  injectedJavaScript={`window.ReactNativeWebView.postMessage(document.title);`}
  onMessage={(e) => console.log("title", e.data)}
/>
```

### 网页视图可以显示哪些网站

如果 `nsh.json` 声明了 `"capabilities"`，网页视图只能显示并导航到 `webview` 下列出的主机：

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

检查覆盖网页视图的 `source` 以及页面发起的每一次顶层导航（链接、重定向、修改 `location`）；它在平台询问能否继续导航时由原生决定。页面自己的子资源（脚本、图片、iframe）属于网页内容自身的事情，不做检查。`*.example.com` 只匹配子域名；要包括裸域名，还需列出 `example.com`。没有 `"capabilities"` 键的应用可以显示任何页面。见[沙箱与能力声明](./sandbox-and-capabilities.md)。

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

播放由属性控制：用 signal 设置 `paused`、`muted`、`volume` 和 `rate`，播放器随之变化。ref 提供 `seek(seconds)`。

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `source` | `string \| { uri }` | | 视频的 URL。 |
| `paused` | `boolean` | `false` | 暂停播放。 |
| `muted` | `boolean` | `false` | 静音。 |
| `volume` | `number` | `1` | 0..1。 |
| `rate` | `number` | `1` | 播放速度。 |
| `repeat` | `boolean` | `false` | 播放到结尾后从头再播。 |
| `resizeMode` | `"cover" \| "contain" \| "stretch" \| "center"` | | 画面如何适应视图。 |
| `controls` | `boolean` | `false` | 显示平台的播放控件。 |
| `progressUpdateInterval` | `number` | `250` | 两次 `onProgress` 之间的毫秒数。 |

| 事件 | 参数 | 触发时机 |
|---|---|---|
| `onLoadStart` | | 开始加载。 |
| `onLoad` | `{ duration, width, height }` | 视频就绪：时长（秒）与尺寸。 |
| `onProgress` | `{ currentTime, duration }` | 播放期间，每 `progressUpdateInterval` 毫秒一次。 |
| `onBuffer` | `{ isBuffering }` | 因等待数据而暂停，或恢复播放。 |
| `onEnd` | | 播放到结尾。 |
| `onError` | `{ message }` | 加载或播放失败。 |

设置 `controls` 时，平台会绘制自己的播放按钮、进度条和全屏按钮。不设置时，用视图在视频上方或下方画你自己的控件；由于视频是布局中的原生视图，覆盖层可以自然地叠在上面：

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

视频来源是网络请求：声明了能力时，视频所在的主机必须在 `nsh.json` 的 `network` 下得到允许，与 `fetch` 和远程图片相同。被拒绝的来源不显示任何内容，并且会记录一次日志，指出需要添加的键。

## 相关内容

- [`WebView`](../elements/web-view.md) 与 [`Video`](../elements/video.md)：元素参考。
- [`nsh/webview`](../modules/nsh-webview.md) 与 [`nsh/video`](../modules/nsh-video.md)。
- [沙箱与能力声明](./sandbox-and-capabilities.md)：`webview` 与 `network` 授权。
- [原生组件](./native-components.md)：嵌入宿主应用提供的其他原生视图。

---
title: WebView
description: WebView 组件：在应用中显示网页和内联 HTML，并支持双向通信。
order: 9
---

# WebView

web view 在应用内显示一个网页，或你传入的一段 HTML：iOS 和 Mac Catalyst 上是 WKWebView，Android 上是 `android.webkit.WebView`。由于 NSH 用原生视图渲染，web view 和其他视图一样处在布局中：没有覆盖层，与周围视图之间也不存在层级问题。

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

`WebView` 从 [`nsh/webview`](../modules/nsh-webview.md) 导入，`source` 是必填的。[Web view 与视频指南](../docs/web-view-and-video.md)介绍了通信模式和能力声明。

## 属性

`WebView` 接受 [view](./view.md) 的属性（不含 `children`），另外还有：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `source` | `string \| { uri: string } \| { html: string; baseUrl?: string }` | | 一个 URL，或一段 HTML 字符串及其相对链接所基于的 base URL。 |
| `injectedJavaScript` | `string` | | 每个页面加载完成后执行。 |
| `injectedJavaScriptBeforeContentLoaded` | `string` | | 在每个页面自身的脚本之前执行。 |
| `userAgent` | `string` | | 浏览器的 user agent 字符串。 |
| `javaScriptEnabled` | `boolean` | `true` | 页面可以运行脚本。 |
| `scrollEnabled` | `boolean` | `true` | 用户可以滚动页面。 |

| `ios` 键 | 类型 | 说明 |
|---|---|---|
| `allowsBackForwardNavigationGestures` | `boolean` | 从边缘轻扫以后退和前进。 |

| `desktop` 键 | 类型 | 说明 |
|---|---|---|
| `allowsBackForwardNavigationGestures` | `boolean` | macOS：双指在触控板上轻扫以后退和前进；Windows：在触摸屏上轻扫；Linux：在触控板上轻扫。 |

## 事件

| 事件 | 载荷 | 时机 |
|---|---|---|
| `onLoadStart` | `WebViewNavigation` | 页面开始加载。 |
| `onLoad` | `WebViewNavigation` | 页面加载完成。 |
| `onError` | `{ message }` | 加载失败。 |
| `onNavigationStateChange` | `WebViewNavigation` | URL、标题、加载状态或历史记录发生变化。 |
| `onMessage` | `{ data: string }` | 页面调用了 `window.ReactNativeWebView.postMessage(data)`。 |

`WebViewNavigation` 为 `{ url, title, loading, canGoBack, canGoForward }`。

## Ref 方法

| 方法 | 说明 |
|---|---|
| `reload()` | 重新加载页面。 |
| `goBack()` / `goForward()` | 在页面历史中后退 / 前进。 |
| `stopLoading()` | 停止加载。 |
| `injectJavaScript(script)` | 在页面中执行 JavaScript。 |
| `postMessage(data)` | 向页面发送一个字符串，页面会在 `window` 上收到 `message` 事件。 |

## 通信

应用和页面之间交换的是字符串。页面通过 `window.ReactNativeWebView.postMessage(data)` 发送；应用通过 ref 的 `postMessage(data)` 发送。结构化数据请用 JSON。

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

## 能力声明

如果应用在 `nsh.json` 中声明了[能力](../docs/sandbox-and-capabilities.md)，web view 只能显示和跳转到 `webview` 下列出的主机。被拒绝的 `source` 不会显示任何内容；被拒绝的顶层跳转（用户点击的链接）会在原生侧取消。

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

## 平台说明

- iOS 在第一次创建 web view 时才加载 WebKit，从不显示 web view 的应用不必为此付出开销。
- 在 Android 上，页面的 `postMessage` 调用会到达后台线程，再按顺序转发给应用。
- 要给 web view 加圆角，设置 `borderRadius` 和 `overflow: "hidden"`。

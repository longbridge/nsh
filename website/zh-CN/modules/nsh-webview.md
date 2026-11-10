---
title: WebView
description: WebView：原生网页视图（WKWebView、android.webkit.WebView），以及它的属性、事件和 ref 方法。
order: 2.35
---

# WebView

`nsh` 导出 `WebView`：由平台自带的浏览器引擎显示的网页（iOS 和 Mac Catalyst 上是 `WKWebView`，Android 上是 `android.webkit.WebView`）。它是一个和其他视图一样布局的原生视图，因此滚动、裁剪以及与其他视图的层叠都是正确的。

```ts
import { WebView, type WebViewNavigation, type WebViewProps } from "nsh";
```

教程见[网页视图与视频](../docs/web-view-and-video.md)，属性见 [`<WebView>`](../elements/web-view.md)。

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

## 属性

`WebView` 接受[视图属性](../elements/view.md)（样式、class、布局、无障碍、`ref`；不接受子元素），另外还有：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `src` | `string` | | 要显示的页面的 URL。 |
| `html` | `string` | | 内联 HTML，代替 `src` 显示。 |
| `baseUrl` | `string` | | `html` 中相对链接的解析基准。它需要与该地址的页面相同的授权（`webview.hosts`）；未授权时 HTML 不带基准 URL 加载，`onError` 报告 `ERR_ACCESS_DENIED`。 |
| `injectedJavaScript` | `string` | | 每个页面加载完成后运行。 |
| `injectedJavaScriptBeforeContentLoaded` | `string` | | 在每个页面自身的脚本之前运行。 |
| `userAgent` | `string` | 平台默认 | 用户代理字符串。 |
| `scrollable` | `boolean` | `true` | 用户可以滚动页面。 |
| `javaScriptEnabled` | `boolean` | `true` | 页面可以运行脚本。 |
| `incognito` | `boolean` | `false` | 页面的网站数据（cookie、存储、缓存）只与网页视图同存亡，与应用的数据分开；Android 上网页视图加载时不使用 HTTP 缓存、不保存表单数据（见[网站数据](../docs/web-view-and-video.md#网站数据)）。在网页视图创建时读取。 |
| `ios` | `{ allowsBackForwardNavigationGestures?: boolean }` | | 仅 iOS：从边缘轻扫以后退和前进。 |
| `desktop` | `{ allowsBackForwardNavigationGestures?: boolean }` | | macOS：双指在触控板上轻扫以后退和前进；Windows：在触摸屏上轻扫；Linux：在触控板上轻扫。 |

## 事件

| 事件 | 数据 | 时机 |
|---|---|---|
| `onLoadStart` | `WebViewNavigation` | 页面开始加载。 |
| `onLoad` | `WebViewNavigation` | 页面加载完成。 |
| `onError` | `{ message: string }` | 页面加载失败。 |
| `onNavigationStateChange` | `WebViewNavigation` | URL、标题、加载状态或历史记录发生变化。 |
| `onMessage` | `{ data: string }` | 页面调用了 `window.nsh.postMessage(data)`。 |

```ts
interface WebViewNavigation {
  url: string;
  title: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}
```

## ref 方法

| 方法 | 说明 |
|---|---|
| `reload()` | 重新加载页面。 |
| `goBack()` | 在历史记录中后退。 |
| `goForward()` | 在历史记录中前进。 |
| `stopLoading()` | 停止加载。 |
| `injectJavaScript(script)` | 在页面中运行 `script`。 |
| `evaluateJavaScript(script)` | 返回脚本的严格 JSON 完成值，参见[等待 JavaScript 结果](#等待-javascript-结果)。 |
| `postMessage(data)` | 向页面发送 `data`（字符串）：在 `window` 上触发 `message` 事件，`event.data` 即为 `data`。 |
| `load({ src, headers? })` | 加载 `src`，只给页面自身的请求带上 `headers`（`Record<string, string>`），子资源、之后的导航和重新加载都不带；用于已登录的页面（`Authorization`）。主机必须是 `webview.hosts` 允许的。请求头无效时抛出 `TypeError`。浏览器后端不做任何事。见[已登录的页面](../docs/web-view-and-video.md#已登录的页面请求头)。 |

## 与页面通信

页面用 `window.nsh.postMessage(…)` 向应用发送字符串；应用用 `ref.postMessage(…)` 向页面发送字符串。结构化数据请用 JSON 传递。

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

## 权限

如果应用在 `nsh.json` 中声明了 `capabilities`，网页视图只能显示和导航到 `webview` 下列出的主机；其他顶层导航会在原生侧被拒绝。没有 `capabilities` 键的应用可以显示任何内容；在 macOS 上，文件只限一个文件夹：`src` 为 `file://` 页面的网页视图可以打开该页面所在文件夹（及其子文件夹）中的文件，不能打开文件夹以外的文件。见[沙箱与权限](../docs/sandbox-and-capabilities.md)。

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

## 等待 JavaScript 结果

`await ref.evaluateJavaScript(script)` 在当前 WebView 的主文档中执行脚本，
返回脚本的完成值。需要已加载文档时，在 `onLoad` 之后调用；此方法本身不等待加载。
iOS 和 Android 都会等待返回的 Promise 或 thenable。脚本内需要 `await` 时，
使用返回 Promise 的异步 IIFE：

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

结果为严格 JSON：字符串、有限数字、布尔值、`null`、数组和普通对象（包括空原型对象）。
`undefined`、函数、Symbol、BigInt、NaN/Infinity、循环引用以及 DOM 节点、Date、Map
等非普通对象会导致拒绝，嵌套值也一样。请像示例一样显式提取 DOM 数据。
复制对象的可枚举属性和数组元素时会执行 getter。序列化失败会拒绝，不会静默丢弃
属性或替换为 `null`。序列化结果最多 1 MiB UTF-8，嵌套最多 64 层，脚本最多
1 MiB，每个引擎最多同时等待 64 个调用。每次调用的超时为 30 秒。

主文档开始新导航时，等待中的调用会被拒绝，即使导航随后失败。
显式 `load`、`reload`、`goBack`、`goForward` 在发起原生操作前取消调用
（包括仍停留在同一文档内的历史操作）。因 props 变化而重新加载时也会取消。
被策略阻止的导航、子框架加载，以及页面自己发起的同文档 hash/history 更新不会取消。
销毁 WebView 或渲染进程终止会拒绝调用；迟到的回调不能完成其他请求。
取消会释放原生请求状态，但不会撤销脚本副作用，也不会强制停止页面代码。

失败以带稳定 `code` 的 `Error` 拒绝：

| Code | 含义 |
|---|---|
| `ERR_NSH_WEBVIEW_JAVASCRIPT` | 脚本异常、语法错误或 Promise 拒绝。 |
| `ERR_NSH_WEBVIEW_RESULT` | 不支持的值、序列化失败或无效结果。 |
| `ERR_NSH_WEBVIEW_NAVIGATION` | 导航使等待中的执行失效。 |
| `ERR_NSH_WEBVIEW_DISPOSED` | ref 已不再对应存活的 WebView。 |
| `ERR_NSH_WEBVIEW_NATIVE` | 原生执行失败、脚本被禁用或渲染进程异常。 |
| `ERR_NSH_QUOTA_EXCEEDED` | 超出脚本、结果或等待请求的限制。 |
| `ETIMEDOUT` | 超过执行期限。 |
| `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` | 在其他后端请求执行。 |

参数错误以 `TypeError` 拒绝；方法返回 Promise，不同步抛出。
iOS 使用 WKWebView 原生完成回调，Android 使用 `evaluateJavascript`；
共享包装脚本将 Android 原生回调无法与 `null` 区分的异常转为明确错误。
返回的 Promise 通过有期限的原生轮询完成。结果通道与 `onMessage` 分离，页面结果始终
是不可信数据。现有 WebView 主机限制、来源和沙箱边界保持不变；只能在应用拥有的
WebView 页面环境中执行。`injectJavaScript` 仍用于无需等待结果的执行。
页面的内容安全策略（CSP）适用于被执行的脚本；被策略阻止的 `eval`
以 `ERR_NSH_WEBVIEW_JAVASCRIPT` 拒绝。

Rust 组件在引擎线程上使用现有元素 ref：

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

future 返回 `Result<nsh_core::JsonValue, nsh_core::element::JsError>`；
丢弃 future 会取消请求。组件任务随其 owner 一起结束。

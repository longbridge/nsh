---
title: WebView
description: WebView 元素：在应用中显示网页和内联 HTML，并支持双向通信。
order: 80
group: primitives
demo: el-web-view
---

# WebView

web view 在应用内显示一个网页，或你传入的一段 HTML：iOS 和 Mac Catalyst 上是 WKWebView，Android 上是 `android.webkit.WebView`。由于 NSH 用原生视图渲染，web view 和其他视图一样处在布局中，与周围视图按顺序排列。Rust 用 `web_view(url)` 或 `web_view_html(html, base_url)` 创建，TSX 用从 `nsh` 导入的 `<WebView>`。

```rust tabs
use nsh_core::prelude::*;

fn browser() -> impl IntoElement {
    let title = signal(SharedString::from("加载中…"));
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
      <Text style={{ numberOfLines: 1 }}>{nav()?.title ?? "加载中…"}</Text>
      <WebView
        src="https://example.com"
        style={{ flex: 1 }}
        onNavigationStateChange={(e) => nav.set(e)}
      />
    </View>
  );
}
```

在 TSX 中 web view 接受 `src` 和 `html` 二者之一（参见 [WebView API](../modules/nsh-webview.md)）。[Web view 与视频指南](../docs/web-view-and-video.md)介绍了通信模式和能力声明。

## 属性

web view 接受 [view](./view.md) 的属性（它没有子元素），另外还有：

| 属性 | Rust | 类型 | 默认值 | 说明 |
|---|---|---|---|---|
| `src` | `web_view(url)` | `string` | | 要显示的页面的 URL。 |
| `html` | `web_view_html(html, base_url)`、`.html(html, base_url)` | `string` | | 内联 HTML，代替 `src` 显示。 |
| `baseUrl` | `web_view_html` / `.html` 的 `base_url` | `string` | | `html` 中相对链接的解析基准。它需要与该地址的页面相同的授权（`webview.hosts`）；未授权时 HTML 不带基准 URL 加载，`onError` 报告 `ERR_ACCESS_DENIED`。 |
| `injectedJavaScript` | `.injected_java_script(..)` | `string` | | 每个页面加载完成后执行。 |
| `injectedJavaScriptBeforeContentLoaded` | `.injected_java_script_before_content(..)` | `string` | | 在每个页面自身的脚本之前执行。 |
| `userAgent` | `.user_agent(..)` | `string` | | 浏览器的 user agent 字符串。 |
| `javaScriptEnabled` | `.java_script_enabled(..)` | `boolean` | `true` | 页面可以运行脚本。 |
| `scrollable` | `.scrollable(..)` | `boolean` | `true` | 用户可以滚动页面。 |
| `incognito` | `.incognito(bool)` | `boolean` | `false` | 页面的网站数据（cookie、存储、缓存）只与网页视图同存亡，与应用的数据分开；Android 上网页视图加载时不使用 HTTP 缓存、不保存表单数据（见[网站数据](../docs/web-view-and-video.md#网站数据)）。在网页视图创建时读取。 |

| `ios` 键 | Rust | 类型 | 说明 |
|---|---|---|---|
| `allowsBackForwardNavigationGestures` | `.back_forward_gestures(..)` | `boolean` | 从边缘轻扫以后退和前进。 |

| `desktop` 键 | Rust | 类型 | 说明 |
|---|---|---|---|
| `allowsBackForwardNavigationGestures` | `.back_forward_gestures(..)` | `boolean` | macOS：双指在触控板上轻扫以后退和前进；Windows：在触摸屏上轻扫；Linux：在触控板上轻扫。 |

## 事件

| 事件 | Rust | 载荷 | 时机 |
|---|---|---|---|
| `onLoadStart` | `.on_load_start` | `WebViewNavigation` | 页面开始加载。 |
| `onLoad` | `.on_load` | `WebViewNavigation` | 页面加载完成。 |
| `onError` | `.on_error` | `{ message, code? }`（`&str`，即消息） | 加载失败；页面、`html` 或 `baseUrl` 不在应用的能力范围内时 `code` 为 `"ERR_ACCESS_DENIED"`，平台无法显示网页视图时为 `"ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"`（Rust 中也可用 `.on_unavailable`，参数为消息）。 |
| `onNavigationStateChange` | `.on_navigation_state_change` | `WebViewNavigation` | URL、标题、加载状态或历史记录发生变化。 |
| `onMessage` | `.on_message` | `{ data: string }`（`&str`） | 页面调用了 `window.nsh.postMessage(data)`。 |
| `onLoadProgress` | `.on_progress` | `{ progress: number \| null }`（`WebProgress`） | 桌面网页加载进度变化；引擎无法估计时为 `null`。 |
| `onDownload` | `.on_download` | `WebViewDownload`（`WebDownload`） | 桌面下载开始、完成或失败。 |

`WebViewNavigation` 为 `{ url, title, loading, canGoBack, canGoForward }`；Rust 中为 `WebNavigation`，字段是 `url`、`title`、`loading`、`can_go_back` 和 `can_go_forward`。

`WebView.available()`（Rust 为 `web_view_available()`）得到网页视图在当前环境下能否显示页面：平台没有网页引擎，或应用的能力未授予 `webview` 时为 `false`（见[没有网页引擎时](../docs/web-view-and-video.md#没有网页引擎时)）。

`WebViewDownload` 为 `{ state, id, url, filename, path, bytes?, message? }`：`state` 为 `"started"`、`"finished"`（带文件大小 `bytes`）或 `"failed"`（带原因 `message`）。设置了 `onDownload` 后，页面下载的内容（附件、web view 不显示的类型、`download` 链接）保存在应用数据目录的 `downloads/<id>/<filename>`，即 `path` 所指的位置，应用用 `fs/promises` 读取。

在 macOS、Windows 和 Linux 上，添加监听器后，`webview` 能力允许加载的任何页面都可以自动保存下载文件。应用负责管理存储空间，并在使用完毕后删除 `downloads/` 中已完成的文件。移除 web view 会取消正在进行的下载，并删除其未完成的目录。下载通过 `onDownload` 报告；导航转为下载时，`onError` 只报告实际的导航失败。文件名会移除控制字符和双向格式控制符；过长的名称会缩短到 200 字节，并保留能容纳的扩展名。

## ref 方法

| 方法 | Rust | 说明 |
|---|---|---|
| `reload()` | `reload()` | 重新加载页面。 |
| `goBack()` / `goForward()` | `go_back()` / `go_forward()` | 在页面历史中后退 / 前进。 |
| `stopLoading()` | `stop_loading()` | 停止加载。 |
| `injectJavaScript(script)` | `inject_java_script(script)` | 在页面中执行 JavaScript。 |
| `postMessage(data)` | `post_message(data)` | 向页面发送一个字符串，页面会在 `window` 上收到 `message` 事件。 |
| `load({ src, headers })` | `load(WebLoad::new(src, headers)?)` | 加载 `src`，只给页面自身的请求带上 `headers`（如 `Authorization`），子资源和之后的导航不带（[指南](../docs/web-view-and-video.md#已登录的页面请求头)）。 |

### Rust 桌面控制

Rust 宿主通过网页视图的 `ElementRef` 截取可见页面、管理 Cookie 和检查工具，以及决定下载。
在组件任务中等待各项操作：平台不支持、ref 未挂载或原生操作失败时返回错误；移除视图或到达截止时间也会结束待处理请求。

| Rust 方法 | 成功结果 | 用途 |
|---|---|---|
| `capture_page()` | `Vec<u8>` | 可见页面的 PNG。 |
| `get_cookies(uri)` | `Vec<WebCookie>` | 匹配 URL 的 Cookie；`None` 表示该视图存储中的全部 Cookie。 |
| `set_cookies(cookies)` | `()` | 完整验证输入后设置 Cookie。 |
| `set_inspectable(enabled)` | `()` | 启用或禁用平台检查工具。 |
| `open_devtools()` | `()` | 在 Linux 和 Windows 打开开发者工具；macOS 使用检查菜单。 |
| `accept_download(destination)` | `()` | 接受待处理下载的目的地；传输完成通过 `on_download` 报告。 |
| `cancel_download(id)` | `()` | 取消待处理或正在传输的下载。 |

macOS 通过 `set_inspectable(true)` 启用检查，再使用 WebKit 的检查菜单；`open_devtools()` 返回不支持该操作的错误。

`nsh_core::webview::WebCookie` 包含 `name`、`value`、`domain`、`path`、`http_only`、`secure`、
`expires`（Unix 毫秒，`None` 表示会话 Cookie）和 `same_site`（`CookieSameSite`）。操作针对目标视图实际使用的存储，
包括 `incognito` 的独立存储；共享存储的视图也共享 Cookie。
以 `.` 开头的域也匹配其子域；不带前导点的域只匹配该主机。

#### 选择下载目的地

默认采用自动下载。Rust 宿主通过 `.download_behavior(WebDownloadBehavior::Manual)` 选择手动处理，
并监听 `.on_download_pending(..)`。事件提供下载的 `id`、`url` 和建议文件名 `filename`，宿主据此接受或取消下载。
接受目的地和传输完成是两个独立结果。

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

目的地是宿主的绝对路径。除非宿主显式启用 `overwrite`，已有文件会受到保护。宿主负责选择其有权使用的路径。
移除视图会取消未回答的下载决策及正在传输的下载，并清理这些下载自己的未完成文件。
手动下载先写独立的暂存文件，完成后才发布到最终路径。默认的不覆盖发布需要文件系统支持硬链接；
不支持时会报告错误，而不会替换已有文件。

#### 内容过滤

`.content_rules(..)` 接受 `ContentRuleList`，每项包含 `id` 和 WebKit 风格的 JSON `rules`。
桌面宿主在加载页面前安装策略；替换列表会替换策略，空列表清除策略。编译失败通过 `on_error` 报告，并保留已安装的策略。

macOS 和 Linux 使用 WebKit 编译规则。Windows 支持 `block`、`ignore-previous-rules` 和主文档的
`css-display-none`，并严格验证支持的条件及资源类型。`block-cookies`、`make-https`、不支持的资源类型，
以及带 `load-type` 条件的外观例外规则会返回错误。Windows 使用公共后缀列表区分站点；若无法确定某个文档请求
所属的顶层页面，匹配到依赖顶层页面的规则时会以 HTTP 403 拒绝该请求，而不会猜测。应用应选用目标引擎支持的规则。


## 通信

应用和页面之间交换的是字符串。页面通过 `window.nsh.postMessage(data)` 发送；应用通过 ref 的 `postMessage(data)`（Rust 中为 `post_message(..)`）发送。结构化数据请用 JSON。

```rust tabs
use nsh_core::prelude::*;

const PAGE: &str = r#"<!doctype html>
<meta name="viewport" content="width=device-width">
<button onclick="window.nsh.postMessage('来自页面的问候')">发给应用</button>
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
            text(move || format!("发给页面 · 收到：{}", message.get()))
                .on_press(move || web.post_message("来自应用的问候")),
        )
}
```

```tsx tabs
import { signal, Text, View, WebView, type WebViewRef } from "nsh";

const page = `<!doctype html>
<meta name="viewport" content="width=device-width">
<button onclick="window.nsh.postMessage('来自页面的问候')">发给应用</button>
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
      <Text onPress={() => web.postMessage("来自应用的问候")}>发给页面 · 收到：{message()}</Text>
    </View>
  );
}
```

## 能力声明

应用在 `nsh.json` 中声明了[能力](../docs/sandbox-and-capabilities.md)时，web view 显示和跳转到 `webview` 下列出的主机。指向其他主机的 `src`，或未声明 `webview` 的应用中的 `html`，显示为空视图；指向其他主机的 `baseUrl` 会被丢弃，HTML 不带它加载；跳到其他主机的顶层导航（用户点击的链接）会在原生侧取消。每种情况都会触发 `onError`，其 `code` 为 `"ERR_ACCESS_DENIED"`。

```json
{
  "capabilities": {
    "webview": { "hosts": ["example.com", "*.example.com"] }
  }
}
```

## 平台说明

- iOS 在第一次创建 web view 时才加载 WebKit，应用在显示 web view 时才为此付出开销。
- 在 macOS 上，`onDownload` 通过 WebKit 的 `WKDownload` 保存下载内容。
- 在 Android 上，页面的 `postMessage` 调用会到达后台线程，再按顺序转发给应用。
- 要给 web view 加圆角，设置圆角半径和 `overflow: "hidden"`（`.overflow_hidden()`）。

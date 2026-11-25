---
title: 沙箱与能力
description: NSH 应用运行时的限制（内存、时间、节点）、release 构建的加固，以及应用在 nsh.json 中声明、宿主应用授予的能力。
order: 21
group: delivery
---

# 沙箱与能力

沙箱让宿主应用可以运行别人写的 TSX 和 JavaScript，比如下载的[签名包](./signed-packages.md)或第三方小程序，并由宿主决定规则。Rust 代码不受沙箱约束：它是编译进应用的原生代码，和应用其他部分一样可信。

每个视图的 JS 跑在独立的 QuickJS VM 里。JS 在 VM 之外做的任何事，都要经过 NSH 的原生运行时。运行时提供三层保护：

- **限制**：约束单个应用能用的内存、时间和原生资源。
- **加固**：release 构建中去掉所有把字符串变成代码的途径。
- **能力**：列出应用能访问什么，比如能请求哪些主机、web view 能显示哪些页面、能否修改状态栏。应用自己声明，宿主应用可以只授予其中一部分。

bundle 里只有应用自己的代码。响应式图、渲染器和应用导入的所有模块都是原生实现。NSH 不信任任何 JS，所有限制和检查都在 Rust 中执行，应用没有可以篡改的 JS 来绕过它们。Rust 代码调用 NSH 服务（`fetch`、图片、web view、状态栏）时，也走和 TSX 相同的原生检查，同样受宿主授权约束。

## 限制

限制作用于 VM，所以只管 TSX 和 JavaScript。

- **嵌入的应用**：使用宿主应用设定的限制。
- **独立应用**（用 `nsh package` 打包，包括 Web，或用 `nsh run` 运行）：是你自己的代码，和原生应用一样，设备给多少内存和时间就用多少。它的解释器栈仍是 4096 KB，深递归会抛出可捕获的 `RangeError`。

嵌入应用的默认限制：

| 限制 | 默认值 | 超出时 |
|---|---|---|
| JS 堆 | 256 MB | VM 停止。 |
| 解释器栈 | 4096 KB（也是上限） | 深递归抛出 `RangeError`，应用可以捕获。 |
| 执行 bundle 的时间 | 10 s | 执行被中断。 |
| 其他每次调用 JS 的时间 | 2 s | 该次调用被中断，应用继续运行。 |
| 保留的节点数 | 200 000 | VM 停止。 |
| 一次排队的修改（操作队列） | 8 000 000 个值 | VM 停止。 |
| 一次 canvas 绘制的操作数 | 1 000 000 | VM 停止。 |

**时间按调用计，不按应用计。** 一次调用是运行时交给 JS 的一件事：

- 处理一个事件，包括它引起的渲染
- 绑定一行列表
- 运行一个定时器
- 运行一帧的 `requestAnimationFrame` 回调
- 清空微任务队列

应用运行再久也不会累积时间，只有单次调用超时才会被中断。嵌套调用共用外层调用的预算。

计入的是调用实际运行的时间：

| 平台 | 计时方式 |
|---|---|
| Linux、Android、macOS、iOS | 线程 CPU 时间 |
| Windows、Web | 单调时钟 |

被系统冻结、在后台挂起或等待 CPU 的时间都不算。

被中断的调用以一个无法被 `catch` 吞掉的异常结束，错误里会写明超出的预算：

```text
the script was interrupted: a call into JS exceeded its budget of 2000 ms
```

VM 继续运行，下一个事件、定时器或帧照常执行。

**内存耗尽或超出配额会停止 VM。** 错误只报告一次：交给宿主应用的错误处理器，开发构建中还会显示在应用上方。之后不再运行任何 JS，视图保持原样。设节点和操作配额，是因为原生视图和排队的修改占用原生内存，不在 JS 堆限制之内。没有这两项配额，应用就能通过 UI 耗尽设备内存。

**NSH 在原生代码中读取的数组最多 2^20（1 048 576）项，所有应用都一样。** 这些数组在调用预算之外读取，而且数组长度以 `length` 为准，空数组也一样（`new Array(2 ** 32 - 1)`）。适用于：

- 子节点（嵌套数组合并计算）、`<Each>` 的条目
- 菜单（子菜单合并计算）、托盘标题
- `Animated.sequence` / `parallel` / `stagger`、动画的区间和缓动、组合动画包含的动画（各自合并计算）
- `split` 的 key、导航栈的路由
- 原生组件的 `props`、`events` 以及传给它的值
- 拖动手势的 `touches`、canvas 调用的参数
- 样式（嵌套样式、阴影和变换）、属性中的列表

超长时的结果：

| 位置 | 结果 |
|---|---|
| 大多数情况 | 抛出可捕获的 `RangeError` |
| 菜单或托盘 | 抛出 `TypeError` |
| 动画的 `start` | Promise 被 reject |
| canvas 调用 | 忽略并给出警告 |
| 导航栈 | 最多 2^20 个页面，超出后 `push` 抛出 `RangeError` |

子节点、子菜单和样式嵌套超过 64 层时抛出 `TypeError`。`<List>` 的 `each` 行数不限。

**视图最多嵌套 1024 层，所有应用都一样。** 布局和挂载在原生代码中逐层遍历视图树，解释器的栈限制管不到这里。嵌套更深的应用会被停止，和超出配额一样：

```text
the app nested views more than 1024 deep and was stopped
```

> [!TIP]
> 这些限制对普通应用很宽松。如果碰到 2 s 的调用预算，说明这项工作本来就不适合放在一个事件里：它会让应用的更新卡住这么久。用 `setTimeout` 拆开，或者移出事件处理函数。

宿主应用按视图设定限制，从下一次加载开始生效：

| 宿主 | API |
|---|---|
| [iOS](./ios.md) | `NshAppView.limits` |
| [Android](./android.md) | `NshView.setLimits` |
| [GPUI 应用](./gpui.md) | `AppOptions::limits` / `AppView::set_limits` |

开发时想看 TSX 应用在宿主限制下的表现，用 [`--limits`](./cli.md#nsh-run) 运行：

- `nsh run App.tsx --limits` 使用上面的默认值。
- `nsh run --limits memory=128MB,call=500ms` 像宿主应用那样设置这两项，其余保持默认。

## 加固

release 构建（所有不从开发服务器加载的代码）在加载时会被加固：

- 删除 `eval`，`Function`、`AsyncFunction`、`GeneratorFunction` 和 `AsyncGeneratorFunction` 构造函数会抛错。NSH 不运行由字符串生成的代码：

  ```text
  TypeError: the Function constructor is disabled: nsh runs no code from strings (development builds allow it)
  ```

- 冻结所有模块共享的原型（`Object`、`Array`、`Function`、`String`、`Number`、`Promise`、`Map`、类型化数组等），一个模块或库无法替其他人修改 `Array.prototype`。`Error.prototype` 保持可写，方便错误子类设置 `this.name`。

冻结原型通常会破坏一种常见写法，即 "override mistake"：继承来的属性被冻结后，`Sub.prototype.constructor = Sub` 或 `obj.toString = …` 会抛错。NSH 把常被覆盖的成员（`constructor`、`toString`、`valueOf`、`toJSON` 等）改成访问器，其 setter 会在赋值的对象上直接定义属性。这类代码照常工作，axios 等依赖它的库也一样。

`nsh dev` / `nsh run` 提供的 bundle（无论是否带 `--release`）不加固，方便用 `eval` 调试。如果代码依赖动态代码，发布前用 `nsh test`（会加固）或打包后的构建测试 release 行为。

## 能力

应用在 `nsh.json` 中声明它能访问 VM 之外的哪些东西：

```json
{
  "name": "Quotes",
  "id": "com.example.quotes",
  "version": "1.0.0",
  "build": 3,
  "capabilities": {
    "network": {
      "hosts": ["api.example.com", "*.cdn.example.com"],
      "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }],
      "sockets": ["feed.example.com:7000"]
    },
    "webview": { "hosts": ["*.example.com"], "input": true },
    "statusBar": true,
    "clipboard": true,
    "picker": true,
    "share": true,
    "speech": true,
    "notifications": true,
    "audio": true,
    "application": true,
    "homeDir": true,
    "host": ["getUserInfo", "bridge.*"],
    "openUrl": { "schemes": ["https", "mailto"] },
    "windows": { "kinds": ["normal", "dialog", "panel"], "max": 4 }
  }
}
```

规则：

- **没有 `"capabilities"`：什么都能做。** 独立应用归开发者自己，不做限制。例外是以下几项在任何应用中都必须按名授予：原始 TCP socket（`network.sockets`）、宿主函数（`host`），以及下文标注"只有声明了才会授予"的键。
- **有 `"capabilities"`：只能做列出的事**，其余一律拒绝。
- **NSH 读不懂的键或值不授予任何东西。** 构建时会给出警告，拼写错误在构建时就能发现，而不是到运行时才表现为缺少权限。
- 更新版本 NSH 生成的[签名包](./signed-packages.md#由更新版本的-nsh-签名的包)可能声明旧运行时不认识的能力，旧运行时不会授予它们。

### `network`

涵盖 `fetch`（包括每一次重定向）、WebSocket、远程图片（`<Image>`、`placeholder`、canvas 图片、`Image.getSize` / `Image.prefetch`）、视频和声音源，以及原始 TCP socket（`net.connect`）。

| 键 | 类型 | 说明 |
|---|---|---|
| `hosts` | `string[]` | 应用可以用任意方法和协议（`http`、`https`、`ws`、`wss`）访问的主机。 |
| `http` | `object[]` | 更细粒度的授权：一个主机，由下面任意的键进一步限定。 |
| `http[].host` | `string` | 主机或主机模式（必填）。 |
| `http[].methods` | `string[]` | 允许的方法，例如 `["GET", "HEAD"]`。缺省：任意。 |
| `http[].schemes` | `string[]` | 允许的协议（`scheme`：以字符串写一个）。缺省：`http`、`https`、`ws`、`wss` 中的任意一种。 |
| `http[].port` | `number` | 端口；URL 未写端口时按其协议的默认端口（`https` 为 443）。缺省：任意。 |
| `http[].paths` | `string[]` | 精确路径，例如 `["/v1/quote"]`（查询参数不属于路径）。 |
| `http[].pathPrefixes` | `string[]` | 按完整路径段匹配的前缀：`/v1` 覆盖 `/v1` 和 `/v1/quote`，`/v1/` 覆盖其后的路径。`pathPrefix`（以字符串写一个）和 `path_prefixes` 含义相同。 |
| `sockets` | `string[]` | [`net.connect`](../modules/standard-modules.md) 可连接的原始 TCP 目标，每项是精确的 `"host:port"`（IPv6 写作 `"[::1]:9000"`）。 |

`http` 规则没写 `paths` 和 `pathPrefixes` 时覆盖所有路径；两者都写时，匹配任一即可。

**路径匹配。** NSH 匹配的是请求实际发出的路径，即 WHATWG URL 解析器处理完点号路径段（包括百分号编码的）之后的结果：

- `/v1/../admin` 和 `/v1/%2e%2e/admin` 都是 `/admin`，`pathPrefix: "/v1"` 不覆盖它。
- 请求用检查过的 URL 发出，服务器收到的就是规则匹配的路径。
- 规则自身的路径也这样解析：`/my photos` 覆盖 `/my%20photos`。规则路径含 `.` 或 `..` 段会报错。
- 编码后的 `/` 或 `\`（`%2F`、`%5C`）保持原样，所以 `/v1/x%2F..%2F..%2Fadmin` 属于 `/v1`。如果服务器先解码再处理点号段，实际提供的会是另一个路径。

**主机模式**有三种：`*`（任意主机）、`*.example.com`（`example.com` 的任意子域名，不含 `example.com` 本身）、精确主机名。匹配不区分大小写，也不看端口。主机取最后一个 `@` 之后的部分，所以 `https://api.example.com@evil.io/` 按 `evil.io` 检查。

有些 URL 在不同平台上会被解析成不同的主机或路径，只有 `hosts` 中的 `*`（或不限端口和路径的 `*` 规则）能放行：

- 查询参数之前有反斜杠（请写成 `%5C`）
- 含控制字符
- 主机含百分号编码，或含主机名不允许的字符：空格、`<`、`>`、`^`、`|`，以及 IPv6 地址之外的 `[` / `]`

**WebSocket。** 握手是一次 HTTP GET：`ws://` 按 `http://` 检查，`wss://` 按 `https://` 检查，所以允许 GET 该 URL 的授权也允许这个 WebSocket。**原始 socket** 绕开所有 HTTP 规则，因此只有 `sockets` 能授予，且每项都要写精确的主机和端口。

**媒体与资源源。**

- `asset:` URI（应用自己的文件）始终允许，只要路径不出应用文件范围，规则和 `fetch` 相同：不能有空段、`.` 或 `..` 段（编码与否都算），不能有反斜杠、冒号或编码后的 `/`。
- 图片、视频和声音还可以用 `data:` 和 `blob:` URI。
- 相对路径在 Web 上相对于页面。原生应用没有页面，所以有能力声明的应用在原生端用相对路径什么也加载不到（声音的相对路径指 bundle 内的文件）。
- 以 `//` 开头的源指定了主机。在 Web 上沿用页面的协议并按该 URL 检查；原生应用没有可沿用的协议，什么也不加载，请写成 `https://…`。
- 其他带协议的源按 URL 检查，加载的就是检查过的那个 URL。
- 源前后的空格和控制字符会被去掉（和 URL 解析器一样），加载清理后的 URL。
- 各平台解析不一致的源（含反斜杠，内部含控制字符、制表符或换行，主机异常）只有 `*` 能放行，并按原样加载。原生应用中以 `//` 开头的仍然什么也不加载。

**本地文件。** 媒体源指向本地文件时（`file:` URL，或原生应用中的绝对路径，如 `/Users/a/x.png`、`C:\pictures\x.png`），在所有原生平台上都按 [`fs`](../modules/standard-modules.md) 能力的 `read` 根目录读取，和 `fs/promises` 一样。设置源时会解析符号链接，平台加载解析后的路径，所以已授权根目录里的链接无法指向外面。源保持设置期间，之后新建的链接会被跟随。在 Web 上，`/x.png` 是页面自己的路径。没有 `capabilities` 的应用照旧加载本地文件。

### `webview`

| 键 | 类型 | 说明 |
|---|---|---|
| `hosts` | `string[]` | [web view](./web-view-and-video.md) 可以显示和导航到的主机（`http` 和 `https`）。没有 `webview` 时，应用完全不能显示网页。 |
| `input` | `boolean` | web view 的 ref 可以向页面发送可信输入（[`sendInput`](./web-view-and-video.md#像用户一样输入)）。只有声明了才会授予，没有 `"capabilities"` 的应用也不例外。 |
| `media` | `string[]` | 网页可以通过 `getUserMedia` 采集的媒体：`"microphone"`、`"camera"`。只有声明了才会授予，没有 `"capabilities"` 的应用也不例外。 |
| `runtimeOrigin` | `boolean` | web view 的 `allowOrigin` 可以在应用运行时为这个 web view 增加一个可显示的源（[`allowOrigin`](../elements/web-view.md)）。只有声明了才会授予，没有 `"capabilities"` 的应用也不例外。 |

**检查范围。** web view 内的顶层导航在加载前由平台的导航代理检查。页面自己的子资源（脚本、图片、XHR）不检查。

**内联内容。** `about:blank` 和内联 HTML（`html`）只要求授予了 web view：`"webview": { "hosts": [] }` 允许它们，但不允许任何远程页面。设置 `baseUrl` 后，原生 web view 中的内联 HTML 会获得该 URL 的源，所以它的主机必须在 `hosts` 中。

**Blob URL。** 页面可以显示和下载自己创建的 `blob:` URL（`URL.createObjectURL`、`<a download>` 链接）。blob URL 带有创建它的文档的源，这个源在 `hosts` 中时才允许。页面无权显示的主机的 blob，或不透明的 blob（`blob:null/…`），都会被拒绝。所有平台检查方式相同。

**下载。** 响应来自 `hosts` 之外主机的下载不会保存，`onDownload` 报告为 `failed`，`code` 为 `"ERR_ACCESS_DENIED"`。在 iOS、macOS 和 Linux 上，无论重定向到哪里都遵守这条规则；在 iOS 和 macOS 上，重定向到这类主机会在发出请求前被取消。下载页面自身内容（`data:` 和 `blob:` URL）不来自任何主机，照常保存。

**为什么 `input` 单独授予。** 可信输入带有用户激活状态，`injectJavaScript` 永远拿不到。有了它，页面可以打开窗口、写剪贴板、打开系统文件面板、发起 WebAuthn 或权限请求，还能点击任何检查 `isTrusted` 的按钮。没有授权时，core 会丢弃 `sendInput`，并在日志中说明要加哪个键。

**`media`** 处理 web view 自己发出的采集请求。在 macOS、iOS、Android、Windows 和 Linux 上，声明过的类型放行，其余拒绝。系统仍会在第一次时询问用户，所以 `nsh package` 会写入包需要的声明：

- `NSMicrophoneUsageDescription` / `NSCameraUsageDescription`（可用 `ios.infoPlist` / `macos.infoPlist` 改写文案）
- macOS 的 `com.apple.security.device.audio-input` / `com.apple.security.device.camera` entitlement
- Android 的 `RECORD_AUDIO` / `CAMERA` 权限，页面第一次需要时由应用申请

开发宿主（`nsh run`）为所有应用带上了用途说明。`getUserMedia` 要求安全上下文（`https:` 或 `localhost`）。

```json
{ "capabilities": { "webview": { "hosts": ["phone.example.com"], "media": ["microphone"] } } }
```

### `statusBar`

`"statusBar": true` 允许应用修改状态栏：[`nsh/base`](../modules/nsh-base.md#status-bar) 中页面和对话框的 `statusBar`，以及 `nsh.json` 里的 `"statusBar"` 默认值。

### `clipboard`

`"clipboard": true` 允许应用读写系统剪贴板（[`Clipboard`](../modules/nsh.md#clipboard)）。从可选中文本的编辑菜单复制是用户自己的操作，不需要这项能力。

### `picker`

`"picker": true` 允许应用打开系统照片选择器（[`Picker`](../modules/nsh.md#picker)），由用户决定把什么交给应用。

启用 `nsh-extra` 的 [`Photos`](./nshm.md#photos) 时，它也授予相册访问：读取最近的照片、缩略图和原图。用户的授权对整个应用生效，所以由这项能力决定哪些代码能用。

### `share`

`"share": true` 允许应用打开系统分享面板（[`Share`](../modules/nsh.md#share)），由用户选择分享到哪里。分享的文件还必须在 `fs.read` 下可读。

### `speech`

`"speech": true` 允许应用用 `nsh-extra` 的 [`Speech`](./nshm.md#speech) 识别语音：使用麦克风和系统语音识别器，识别文本交给应用。系统仍会询问用户一次。

### `audio`

`"audio": true` 允许应用播放声音（[`Sound` 和 `AudioPlayer`](../modules/nsh.md#sound)）。远程音源还需要该主机的 `network` 授权。

### `notifications`

`"notifications": true` 允许应用显示系统通知、设置图标角标（[`Notifications`](../modules/nsh.md#notifications)）。系统需要询问的平台上，会询问用户一次。

启用 `nsh-extra` 的 [`Push`](./nshm.md#push) 时，它也授予远程推送：注册设备并读取 token、监听投递和点击、决定前台如何显示通知、移除某个会话已投递的通知。注册和后台投递从不询问用户，用户的授权也对整个应用生效，所以由这项能力决定哪些代码能用。

### `application`

`"application": true` 允许应用操作整个应用：

- **桌面**（[`nsh/desktop`](../modules/nsh-desktop.md)）：应用菜单（`Menu.setApplicationMenu`）、托盘、全局快捷键、`Application.quit()`、`Application.setQuitMode()`、`Application.restart()` 和 `Application.setLaunchAtLogin()`。
- **iOS 和 Android**：系统设置中本应用的页面（`nsh/mobile` 的 [`AppSettings.open()`](../modules/nsh-mobile.md#appsettings)；嵌入的视图打开宿主应用的页面）。

没有这项能力时它们什么也不做：`Application.launchAtLogin()` 和 `AppSettings.available()` 返回 `false`，`setLaunchAtLogin` 和 `AppSettings.open()` 以 `ERR_ACCESS_DENIED`（`permission` 为 `"application"`）reject。

右键菜单（`Menu.popup`）显示在应用自己的视图里，应用打开的窗口由 [`windows`](#windows) 授权，两者都不需要 `application`。

### `homeDir`

`"homeDir": true` 允许 [`os.homedir()`](../modules/standard-modules.md) 返回用户主目录；没有时调用抛出 `code` 为 `ERR_ACCESS_DENIED` 的错误。拿到的只是路径名，读写那里仍需要 `fs` 授权。

### `openUrl`

允许应用在外部打开 URL（[`openUrl`](../modules/nsh.md#openurl)，以及 `links="open"` 的 `TextView` 链接）：

- `"openUrl": true` 允许所有协议。
- `"openUrl": { "schemes": ["https", "mailto"] }` 只允许列出的协议。

协议未授权的 URL 以 `ERR_ACCESS_DENIED` reject（`permission` 为 `"openUrl"`，`resource` 为该 URL）。会执行代码或触及应用自身数据的 URL（`javascript:`、`data:`、`file:` 等）无论如何都不会打开。

### `windows`

允许应用在桌面上打开自己的窗口（[`Window`](../modules/nsh-desktop.md#窗口)）。`"windows": true` 允许浮层以外的所有种类，也可以写成对象来缩小范围：

| 键 | 类型 | 说明 |
|---|---|---|
| `kinds` | `string[]` | 可以打开的种类：`"normal"`、`"dialog"`、`"panel"`、`"popup"`、`"overlay"`。缺省：除 `"overlay"` 外的所有种类。 |
| `levels` | `string[]` | 窗口可以使用的层级：`"normal"`、`"floating"`、`"aboveDock"`、`"desktopIcon"`。缺省：所有层级。 |
| `max` | `number` | 同时可以打开的自己的窗口数，包含第一个窗口。缺省：不限。 |
| `overlay` | `boolean` | 浮层和点击穿透的窗口（`pointerEvents: "none"`）。只有声明了才会授予，没有 `"capabilities"` 的应用也不例外。 |

`overlay` 单独授予，是因为透明、可点击穿透、盖在所有窗口之上的窗口可以冒充系统界面。每个窗口都需要授权，第一个也不例外，没有特权窗口。

### 存储

每个应用都可以用 [`Storage`](../modules/nsh.md#storage) 在设备上保存自己的数据，不需要能力：存储里只有该应用的数据，存放位置由嵌入它的应用决定。

### `host`

`host` 列出 `nsh` 的 `host.call(name, args)` 可以调用嵌入方应用的哪些函数：

- 精确名称，如 `"getUserInfo"`
- 以 `*` 结尾的前缀：`"bridge.*"` 匹配 `bridge.call` 和 `bridge.share`
- `"host": true` 允许所有函数

未列出的函数一律拒绝，没有 `"capabilities"` 的应用也一样：宿主函数从不默认授予。应用从 `nsh` 导入了 `host` 却没有声明 `host` 时，构建会警告所有调用都将被拒绝。

函数名长 1 到 128 个字符，只能用 `A`–`Z`、`a`–`z`、`0`–`9`、`_`、`.`、`:` 和 `-`，且不能含 `..`。其他名称在读取授权之前就被拒绝。前缀要以分隔符结尾（`"bridge.*"`），否则 `"bridge*"` 也会匹配 `bridgeAdmin`。

宿主发出的事件（`host.on(event, listener)`）会原样送到应用的监听器。

```ts
import { host } from "nsh";

if (host.available()) {
  const user = await host.call("getUserInfo");
  const stop = host.on("loginChanged", (payload) => console.log(payload));
  stop();
}
```

`host.available()` 在嵌入了宿主的平台上为 true；其他平台上 `host.call` 以 `code` `"ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"` reject。

### 被拒绝时的表现

| 请求 | 结果 |
|---|---|
| `fetch` | Promise 像每个失败的请求一样以 `TypeError` 被 reject，其 `code` 为 `ERR_ACCESS_DENIED`，`permission` 为 `"network"`，`resource` 为该 URL（请求的或重定向的）；请求根本不会发到网络。 |
| `Image.getSize`、`Image.prefetch` | Promise 以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"network"`，`resource` 为该 URL；图片不会被请求。 |
| 远程图片、视频源、web view 源或 `html` | 不显示任何内容，元素的 `onError` 以 `code` `"ERR_ACCESS_DENIED"` 报告。web view 跳到未授权主机的导航会被取消，并以同样方式报告。 |
| 状态栏修改、画布图片 | 不修改也不绘制任何东西；拒绝会记录在日志中。 |
| 剪贴板 | `Clipboard` 的每个函数都以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"clipboard"`，`resource` 为 `""`；不会访问系统剪贴板。 |
| 选择器 | `pickImages` 和 `pickFiles` 不显示任何内容，以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"picker"`，`resource` 为 `""`。 |
| 分享 | `Share.available()` resolve 为 `false`，`Share.show` 不显示任何内容，以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"share"`，`resource` 为 `""`。 |
| 声音 | `Sound.load` reject、`new AudioPlayer` 抛出 `ERR_ACCESS_DENIED`，`permission` 为 `"audio"`，`resource` 为音源；不读取也不播放任何内容。 |
| 语音 | `Speech.start` 抛出 `ERR_ACCESS_DENIED`，`permission` 为 `"speech"`，`resource` 为 `""`；麦克风不会打开。 |
| 通知 | `Notifications` 的函数 reject 或抛出 `ERR_ACCESS_DENIED`，`permission` 为 `"notifications"`，`resource` 为 `""`；不会显示任何东西。 |
| 相册 | `nsh-extra` 的 `Photos` 除 `access()` 外的函数 reject 或抛出 `ERR_ACCESS_DENIED`，`permission` 为 `"picker"`，`resource` 为 `""`；不读取相册，也不询问用户。 |
| 推送 | `nsh-extra` 的 `Push` 除 `available()` 外的函数 reject 或抛出 `ERR_ACCESS_DENIED`，`permission` 为 `"notifications"`，`resource` 为 `""`；设备不会注册，token 和投递都不会到达应用。 |
| `openUrl` | Promise 以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"openUrl"`，`resource` 为该 URL；不会打开任何东西。 |
| 宿主调用 | `host.call` 以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"host"`，`resource` 为函数名（`host.post` 抛出同样的错误）；调用不会到达宿主。 |
| 文件（`fs/promises`） | Promise 以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"fs"`，`resource` 为该路径；不会访问磁盘。 |
| 进程（`process`） | `run` 被 reject，`spawn` 和 `exit` 抛出，错误为 `ERR_ACCESS_DENIED`，`permission` 为 `"process"`，`resource` 为该命令（`exit` 为 `"exit"`）；不会启动子进程。 |
| TCP socket（`net.connect`） | Promise 以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"network"`，`resource` 为 `"host:port"`；发生在任何 DNS 查询之前。 |
| 窗口（`Window.open`、`<Window>`） | 抛出 `ERR_ACCESS_DENIED`，`permission` 为 `"windows"`，`resource` 为窗口的 id；不会打开任何窗口。`set()` 中未授权的属性保持不变，并给出警告。 |
| WebSocket | `websocket` 模块的 `connect` 以 `ERR_ACCESS_DENIED` 被 reject，`permission` 为 `"network"`，`resource` 为该 URL；全局 `WebSocket` 像连接失败时那样先触发 `error`，再触发 `close`（code 为 1006）。 |

[标准模块](../modules/standard-modules.md#失败)和 [`nsh`](../modules/nsh.md) 模块的拒绝错误都带 `code` `"ERR_ACCESS_DENIED"`，应用可以据此区分"被拒绝"和"操作失败"。`permission` 是所需的能力，`resource` 是请求的对象。`fetch` 用 `TypeError` reject，这是所有失败请求的错误类。

```ts
import { Clipboard } from "nsh";

try {
  await Clipboard.writeText(link);
} catch (error) {
  if (error.code !== "ERR_ACCESS_DENIED") throw error;
  console.warn(`${error.permission} is not granted`); // "clipboard"
}
```

Rust 组件以 Rust 的方式收到拒绝：

| 调用 | 被拒绝时 |
|---|---|
| 读剪贴板 | 等同于剪贴板为空 |
| 选择文件 | 没有文件 |
| `open_url` | `false` |
| `fetch` | 带错误信息的 `Err` |

每种拒绝只记录一次日志，并说明要加哪个键：

```text
fetch GET https://evil.example.org/x is not granted: add `evil.example.org` to capabilities.network in nsh.json
```

重定向由 NSH 自己跟随，而不是交给平台的 HTTP 栈，所以每一跳都会检查。流式请求体不会发送两次，因此流式上传遇到 `307` / `308` 重定向会失败。

### 能力随应用传递

声明会跟着应用走到每个地方：

- `nsh build` 和 `nsh package` 把它写在 bundle 旁边的 `capabilities.json` 中。
- `nsh publish` 把它放进[签名包](./signed-packages.md)头部，受签名保护。
- `nsh dev` 每次构建都会发送它，开发时执行同样的规则。修改 `capabilities` 会重新加载应用，而不是热更新。
- 根节点是 Rust 组件的应用（`nsh.json` 中的 `"root"`），声明和根组件一起编译进去。所有运行器（包括 Web 和 `nsh test`）都按它检查应用的调用，和 TSX 应用一样。没有 `"capabilities"` 时，行为与其他未声明的应用相同。

## 授权：宿主应用允许什么

嵌入 NSH 的宿主应用可以用同样的 JSON 格式授予一组能力，作为**上限**：

```rust
// iOS, in Rust (nsh_platform_ios::AppView)
let grant = nsh_engine::Capabilities::from_json(&serde_json::json!({
    "network": { "hosts": ["api.example.com"] }
}))?;
app.set_granted_capabilities(Some(grant));
```

```swift
// iOS
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
```

```java
// Android
nsh.setGrantedCapabilities("{ \"network\": { \"hosts\": [\"api.example.com\"] } }");
```

请求必须**同时**被应用的声明和宿主的授权允许。按上面的授权，即使应用声明了 `"hosts": ["*"]`，也只能访问 `api.example.com`，不能显示网页，也不能修改状态栏。没有授权时（`nil` / `null`，默认值），按各应用自己的声明执行。

授权从下一次加载开始生效。第三方小程序的常见做法：每个小程序只授予它自己的 API 主机，从你审核过声明的[签名包](./signed-packages.md)运行，限制保持默认或调得更低。

> [!WARNING]
> 宿主应用注册的[原生组件](./native-components.md)是宿主自己的代码，能力对它们不起作用。只注册对该视图可能运行的每个应用都安全的组件。

## 小结

| 层 | 由谁执行 | 由谁设定 |
|---|---|---|
| 限制（堆、栈、时间、节点） | 运行时，按 VM | 宿主应用按视图设定（嵌入的应用） |
| 加固 | 运行时，在加载时 | 开发环境之外始终开启 |
| 能力 | 运行时（fetch、图片、视频、web view、状态栏、窗口、文件、进程、socket） | 应用（`nsh.json`）和宿主应用（授权） |
| 代码完整性 | 签名验证 | `nsh publish`；宿主应用信任的根密钥 |

## 另请参阅

- [签名包](./signed-packages.md)
- [网络](./networking.md)
- [嵌入 iOS 应用](./ios.md)、[嵌入 Android 应用](./android.md)

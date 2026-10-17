---
title: 沙箱与能力
description: 每个 NSH 应用运行时受到的限制（内存、时间、节点），release 构建的语言加固，以及在 nsh.json 中声明、由宿主应用授予的能力。
order: 21
group: delivery
---

# 沙箱与能力

每个 NSH 应用都运行在自己的 QuickJS VM 中，应用在 VM 之外能做的一切都要经过 NSH 的原生运行时。因此，你可以在宿主应用
制定的规则下运行并非自己编写的代码，例如第三方小程序或下载的更新：

- **限制（Limits）** 约束单个应用可使用的内存、时间和原生资源。
- **加固（Hardening）** 在 release 构建中移除所有把字符串变成代码的途径。
- **能力（Capabilities）** 列出应用可以访问什么：可以向哪些主机发起 fetch、web view 可以显示哪些页面、是否可以修改
  状态栏。能力由应用声明；宿主应用可以授予得更少。

bundle 中只有应用自己的代码：响应式图、渲染器以及应用导入的所有模块都是原生的。因此 JS 中的任何东西都不受信任，所有
限制和检查都在 Rust 中执行；应用没有任何 JS 代码可以打补丁来绕过它们。

## 限制

每个 VM 都有以下限制：

| 限制 | 默认值 | 触发时 |
|---|---|---|
| JS 堆 | 独立运行的应用在 macOS、Windows 和 Linux（以及 Mac Catalyst）上为 1 GB，在 iOS 和 Android 上为 512 MB；嵌入在其他应用中的为 256 MB（嵌入方可另行设置） | VM 停止。 |
| 解释器栈 | 4096 KB（也是上限） | 过深的递归抛出 `RangeError`，应用可以捕获。 |
| 执行 bundle 的时间 | 10 s | 执行被中断。 |
| 其他任意一次进入 JS 的调用的时间 | 2 s | 该调用被中断；应用继续运行。 |
| 保留的节点数 | 200 000 | VM 停止。 |
| 一次性的变更（排队的操作） | 8 000 000 个值 | VM 停止。 |
| 单次 canvas 绘制中的操作数 | 1 000 000 | VM 停止。 |

**时间按调用计算，而不是按应用计算。** 一次“调用”是运行时让 JS 做的一件事：处理一个事件（包括它引起的渲染）、绑定
一个列表行、运行一个定时器、运行一帧的 `requestAnimationFrame` 回调、清空微任务队列。长时间运行的应用不会累积时间
而触及限制；只有某一次没能按时返回的调用才会被中断。嵌套在另一次调用中的调用共享外层调用的预算。

被中断的调用会以一个任何 `catch` 都无法吞掉的异常结束，报告的错误中会写明超出的预算：

```text
the script was interrupted: a call into JS exceeded its budget of 2000 ms
```

VM 会继续运行：下一个事件、定时器或帧照常执行。

**内存耗尽或超出配额会停止 VM。** 错误只报告一次（报告给宿主应用的错误处理器，在开发构建中还会显示在应用上方），此后
不再运行任何 JS，视图保持原样。之所以有节点和操作配额，是因为原生视图和排队的变更位于原生内存中，不受 JS 堆限制约束：
如果没有这些配额，应用就可能通过 UI 耗尽设备内存。

> [!TIP]
> 对普通应用来说，这些限制相当宽松。如果你触到了 2 s 的调用预算，说明这项工作本来就不该放在一个事件里：它会让应用的
> 更新冻结那么久。可以用 `setTimeout` 把它拆开，或者把它移出事件处理函数。

宿主应用可以按视图修改限制：[iOS](./embedding-ios.md) 上用 `NshAppView.limits`，
[Android](./embedding-android.md) 上用 `NshView.setLimits`。修改从下一次加载开始生效。

## 加固

release 构建（所有并非从开发服务器加载的代码）在加载时会被加固：

- 移除 `eval`，`Function`、`AsyncFunction`、`GeneratorFunction` 和 `AsyncGeneratorFunction` 构造函数会抛出异常。
  NSH 不运行由字符串生成的代码：

  ```text
  TypeError: the Function constructor is disabled: nsh runs no code from strings (development builds allow it)
  ```

- 所有模块共享的原型（`Object`、`Array`、`Function`、`String`、`Number`、`Promise`、`Map`、类型化数组等）都被冻结，
  因此一个模块（或一个库）无法为其他所有人修改 `Array.prototype`。`Error.prototype` 保持可写，以便错误子类设置
  `this.name`。

冻结原型通常会破坏一种常见写法，即所谓的 “override mistake”：当继承来的属性已被冻结时，赋值
`Sub.prototype.constructor = Sub` 或 `obj.toString = …` 会抛出异常。NSH 把常被覆盖的成员（`constructor`、`toString`、
`valueOf`、`toJSON` 等）改成访问器，其 setter 会在被赋值的对象上定义该属性，因此这类代码（以及 axios 等使用它的库）
依然可以正常工作。

由 `nsh dev` / `nsh run` 提供的 bundle（无论是否带 `--release`）都不加固，所以你可以用 `eval` 调试。在发布依赖动态
代码的功能之前，请用 `nsh trace`（会加固）或打包后的构建测试 release 下的行为。

## 能力

应用在 `nsh.json` 中声明它可以访问 VM 之外的哪些东西：

```json
{
  "name": "Quotes",
  "id": "com.example.quotes",
  "version": "1.0.0",
  "build": 3,
  "capabilities": {
    "network": {
      "hosts": ["api.example.com", "*.cdn.example.com"],
      "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }]
    },
    "webview": { "hosts": ["*.example.com"] },
    "statusBar": true,
    "clipboard": true
  }
}
```

**没有 `"capabilities"` 的应用可以做任何事**：独立应用归开发者自己所有，不受任何限制。**有 `"capabilities"` 的应用
只能做其中列出的事**：其他一律拒绝。未知的键会导致构建错误，因此拼写错误会在构建时被发现，而不会变成运行时缺失的权限。

### `network`

涵盖 `fetch`（包括每一次重定向）、远程图片（`<Image>`、`defaultSource`、canvas 图片、`Image.getSize` /
`Image.prefetch`）以及视频源。

| 键 | 类型 | 说明 |
|---|---|---|
| `hosts` | `string[]` | 应用可以用任意方法和协议（`http`、`https`、`ws`、`wss`）访问的主机。 |
| `http` | `object[]` | 更细粒度的授权，每个主机一条。 |
| `http[].host` | `string` | 主机（必填）。 |
| `http[].methods` | `string[]` | 允许的方法，例如 `["GET", "HEAD"]`。为空或缺省：任意。 |
| `http[].schemes` | `string[]` | 允许的协议。为空或缺省：`http`、`https`、`ws`、`wss` 中的任意一种。 |
| `http[].pathPrefix` | `string` | URL 的路径必须以它开头。 |

主机模式可以是 `*`（任意主机）、`*.example.com`（`example.com` 的任意子域名，不包括 `example.com` 本身）或一个确切的
主机名。匹配时忽略大小写和端口。主机取最后一个 `@` 之后的部分，因此 `https://api.example.com@evil.io/` 会按 `evil.io`
检查。

`asset:` URI（应用自己的文件）始终允许；图片和视频还可以使用 `data:` URI 和相对路径。

### `webview`

| 键 | 类型 | 说明 |
|---|---|---|
| `hosts` | `string[]` | [web view](./web-view-and-video.md) 可以显示和导航到的主机（`http` 和 `https`）。没有 `webview` 时，应用完全不能显示网页。 |

web view 内的顶层导航也会在加载前由平台的导航代理检查。页面自身的子资源（脚本、图片、XHR）属于网页内容自己的事，
不做检查。`about:blank` 和内联 HTML（`source={{ html }}`）只要求 web view 被授予即可：`"webview": { "hosts": [] }`
允许它们，但不允许任何远程页面。

### `statusBar`

`"statusBar": true` 允许应用修改状态栏（[`nsh/ui/mobile`](../components/status-bar.md) 中的 `StatusBar` 组件及其函数）。

### `clipboard`

`"clipboard": true` 允许应用读写系统剪贴板（[`nsh/clipboard`](../modules/nsh-clipboard.md)）。从可选文本的编辑菜单复制是用户自己的操作，不需要这项能力。

### 被拒绝时的表现

| 请求 | 结果 |
|---|---|
| `fetch` | Promise 被 reject；请求根本不会发到网络。 |
| 远程图片 | 图片加载失败，触发 `onError`。 |
| 视频源、web view 源、状态栏修改 | 不显示也不修改任何东西。 |
| 剪贴板 | `setString` 不做任何事；`getString` 返回 `""`，`hasString` 返回 `false`。 |

每种拒绝只记录一次日志，并说明需要添加哪个键：

```text
fetch GET https://evil.example.org/x is not granted: add `evil.example.org` to capabilities.network in nsh.json
```

重定向由 NSH 跟随，而不是由平台的 HTTP 栈跟随，因此每一跳都会按能力检查。（流式请求体不会发送两次，所以流式上传遇到
`307` / `308` 重定向会失败。）

### 能力随应用传递

声明会随应用一起走到任何地方：

- `nsh build` 和 `nsh package` 把它作为 `capabilities.json` 写在 bundle 旁边；
- `nsh publish` 把它放进[签名包](./signed-packages.md)的头部，受签名保护；
- `nsh dev` 每次构建都会发送它，因此开发时执行的是同样的规则。修改 `capabilities` 会重新加载应用，而不是热更新。

## 授权：宿主应用允许什么

嵌入 NSH 的宿主应用可以用同样的 JSON 格式授予 **至多** 哪些能力：

```swift
// iOS
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
```

```java
// Android
nsh.setGrantedCapabilities("{ \"network\": { \"hosts\": [\"api.example.com\"] } }");
```

一个请求必须 **同时** 被应用的声明和宿主的授权所允许。在上面的授权下，即使应用声明了 `"hosts": ["*"]`，也只能访问
`api.example.com`，并且不能显示网页或修改状态栏。没有授权时（`nil` / `null`，即默认值），以各应用自己的声明为准。

授权从下一次加载开始生效。第三方小程序的典型配置是：只授予每个小程序它自己的 API 主机，从你审核过声明的
[签名包](./signed-packages.md)运行它，并保持默认限制或调低限制。

> [!WARNING]
> 宿主应用注册的[原生组件](./native-components.md)是宿主自己的代码：能力对它们不起作用。只注册对该视图可能运行的
> 每个应用都安全的组件。

## 小结

| 层 | 由谁执行 | 由谁设定 |
|---|---|---|
| 限制（堆、栈、时间、节点） | 运行时，按 VM | 默认值；宿主应用按视图设定 |
| 加固 | 运行时，在加载时 | 开发环境之外始终开启 |
| 能力 | 运行时（fetch、图片、视频、web view、状态栏） | 应用（`nsh.json`）和宿主应用（授权） |
| 代码完整性 | 签名验证 | `nsh publish`；宿主应用信任的根密钥 |

## 另请参阅

- [签名包](./signed-packages.md)
- [网络](./networking.md)
- [嵌入 iOS 应用](./embedding-ios.md)、[嵌入 Android 应用](./embedding-android.md)

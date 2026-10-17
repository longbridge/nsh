---
title: 简介
description: Native Shell 是什么：在 iOS、Android、macOS、Windows 和 Linux 上，由应用在发布之后加载、经过签名并运行在沙箱中的 TSX 模块，以原生界面呈现。
order: 1
group: start
---

# 简介

Native Shell（简称 NSH）是为**由 AI 生成、动态下发的原生界面**而设计的。模型用 TSX 写出一个小模块，原生应用在运行时加载它，用平台自己的原生界面显示，并运行在应用掌控的沙箱里。同一套机制也让原生应用整体具备扩展能力——插件、功能、整个页面都可以在发布之后下发。

NSH 同时运行在手机和桌面上：iOS（UIKit）、Android（Android 视图）、macOS（AppKit）、Windows（用 Direct2D 绘制的 Win32 窗口）和 Linux（GTK 4）。每个平台都有自己的原生后端，同一个模块可以在所有这些平台上运行（[平台](./platforms.md)）。

> [!IMPORTANT]
> NSH 是嵌在应用里的一层，负责那些在发布之后才生成、修改或新增的界面。无论在手机还是桌面上，应用仍然是原生应用，照原来的方式构建和发布。

## 为什么做 NSH

**首要目标：由 AI 生成的界面。** 应用里的助手可以用一个界面而不是一段文字来回答。模型写出一个小的 TSX 模块（一个表单、一张图表、一张预订卡片），服务器检查、构建并签名后，应用下载并在 `NshAppView` 中显示：是原生界面，运行在沙箱里，只能做你授予的事，而且不需要更新应用。NSH 围绕这个场景设计，整体架构都是 AI 友好的：

- **模型擅长的格式。** 一个模块就是一个 TSX 文件，用到信号、少量宿主元素和 Tailwind 类——都是模型在 Web 上早已熟悉的词汇，不需要构建配置，也无需 NPM。
- **精确的、自动生成的契约。** 每个元素、属性、样式键和模块都有从 Rust 源码生成的 TypeScript 声明（`.nsh/types`），生成的代码可以做类型检查，也可以把准确的 API 交给模型，而不是让它去猜。本文档也以纯文本形式提供给模型（[`llms.txt`](/llms.txt)、[`llms-full.txt`](/llms-full.txt)）。
- **下发之前就能验证。** `nsh trace` 可以无界面、不需要设备地运行模块，报告错误和它生成的视图；构建错误附带代码定位。生成流程可以编译、运行、读取结果再重试。编译器也能作为库在进程内运行（`nsh_bundler::compile_module`），甚至在浏览器里运行（[在线试用](/zh-CN/playground)）。
- **不可信也能安全运行。** 生成的代码就是不可信代码。每个模块运行在独立的 QuickJS 虚拟机里，受内存、调用时长和节点数限制，只能使用宿主授予的能力，并以签名包的形式到达、先经应用校验。
- **原生，却无需写原生代码。** 产出的是平台自己的界面——UIKit、Android 视图、AppKit、GTK 4，或用 Direct2D 绘制的 Win32——拥有平台的文本、输入和无障碍能力，但模型从不需要写 Swift、Kotlin、Java 或 Rust，应用也不需要发新版本。

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/ai-flow-light.svg" alt="从提示词到原生界面：模型写出 TSX 模块；服务器构建、检查并签名；跨过信任边界后，应用校验包，在设定了限制和能力的沙箱里运行，并用平台自己的界面显示。">
<img class="diagram diagram-dark" src="/diagrams/ai-flow-dark.svg" alt="从提示词到原生界面：模型写出 TSX 模块；服务器构建、检查并签名；跨过信任边界后，应用校验包，在设定了限制和能力的沙箱里运行，并用平台自己的界面显示。">
</div>

**其次：扩展原生应用，动态下发界面。** 同一套机制适用于任何模块，不只是生成的模块。应用在需要显示模块的地方嵌入一个 `NshAppView`（iOS）、`NshView`（Android）或 `AppView`（macOS、Windows、Linux），NSH 在其中运行模块：

- **发布之后再下发。** 模块是应用在运行时下载的签名包（`.nshpkg`）。应用在运行它之前，会用自己信任的密钥校验签名，并检查吊销列表（[签名包](./signed-packages.md)）。改动无需发新版本，也无需重新提交应用商店审核。
- **宿主始终掌控。** 每个模块运行在独立的沙箱虚拟机中，受内存、时间和节点数限制，只能访问宿主授予的能力：网络主机、网页、状态栏（[沙箱与能力](./sandbox-and-capabilities.md)）。多个模块可以同时运行，没有全局单例。
- **与应用其余部分浑然一体。** 模块用平台自己的界面渲染，文本、输入、无障碍和滚动都是系统自己的。宿主还可以注册自己的原生视图——地图、播放器、设计系统组件——供模块使用（[原生组件](./native-components.md)）。

**再次：独立应用。** 同样的模块也可以组成一个完整的应用。`nsh package` 用预编译的运行时把它们打成可安装的 iOS、Android、macOS、Windows 或 Linux 应用，这也是试用 NSH 最快的方式：[快速上手](./getting-started.md)就是这样做的。这是受支持的用法，但不是 NSH 设计的出发点。

## 为什么在 GPUI Shell 之外还要做 Native Shell

[长桥（Longbridge）](https://longbridge.com)把 NSH 作为手机客户端的插件层来建设，桌面客户端也需要同样的一层。[GPUI Shell](https://gpui-kit.com/shell) 让 [GPUI](https://gpui-kit.com) 桌面应用可以用 JavaScript 扩展，最初的想法也是把它用到手机上。但它并不适合，因为它的模块由 GPUI 自己在 GPU 上绘制，而不是使用平台的视图：

- **系统能力都得重新实现。** 嵌在原生页面里的自绘界面，没有平台的文本选区，也没有选区弹出的菜单（拷贝、分享、翻译、查询、全选），更没有系统输入法、无障碍，以及每一代 iOS 带来的新系统视觉效果。在 GPU 渲染器里复刻这些是一项永远做不完的大工程，而且复刻品总会和真正的系统行为走样。
- **手机团队得接手一套新的 UI 技术栈。** 要在现有手机客户端里大规模运营一层自绘界面，iOS 和 Android 团队就得接手一个陌生的渲染器，以及它在各种系统版本和设备上的兼容性问题和缺陷。

因此 NSH 用每个平台自己的界面来渲染模块：[iOS](./embedding-ios.md) 上用 UIKit，[Android](./embedding-android.md) 上用 Android 视图，[macOS](./macos.md) 上用 AppKit，[Windows](./windows.md) 上用 Direct2D 绘制的 Win32 窗口，[Linux](./linux.md) 上用 GTK 4。同一种模块格式和同一个运行时可以运行在所有这些宿主上。

两者的定位不同：GPUI Shell 用于扩展 GPUI 桌面应用；Native Shell 面向每个平台上的原生应用，手机和桌面一视同仁。

## 一个模块

模块用 TypeScript 和 JSX 编写：

```tsx
import { mount, signal, Text, View } from "nsh";

function Counter() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <Text style={{ fontSize: 48, fontWeight: "700" }}>{count()}</Text>
      <View
        onPress={() => count.update((n) => n + 1)}
        activeOpacity={0.6}
        class="rounded-lg bg-blue-600 px-5 py-3"
      >
        <Text class="text-white text-lg">Tap</Text>
      </View>
    </View>
  );
}

mount(Counter);
```

`<View>` 在 iOS 上变成 `UIView`，在 Android 上变成 `ViewGroup`，在 macOS 上变成 `NSView`，在 Linux 上变成 GTK 控件，`<Text>` 变成原生文本视图，按钮的按压反馈（`activeOpacity`）在主线程上完成，不需要往返 JavaScript。点击时，`count.update` 只修改一个信号，只有读取 `count()` 的那段文本会更新：组件函数不会重新执行。

宿主应用只需几行原生代码就能显示它（[iOS](./embedding-ios.md)、[Android](./embedding-android.md)、[macOS](./macos.md#嵌入-mac-应用)、[Windows](./windows.md#嵌入-win32-应用)、[Linux](./linux.md#嵌入-gtk-应用)）。在 iOS 上：

```swift
let app = NshAppView()                         // 一个 UIView：可以放在任何地方
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
let info = try await app.loadPackage(at: downloadedFile, trust: trust)   // 先校验，再运行
```

只有模块自己的代码是 JavaScript。它调用的一切——响应式图、渲染器、布局、文本，以及它导入的每个模块——都是用 Rust 实现的原生代码。

## 模块如何运行

一个模块（在 CLI 里称为「应用」）是一组带入口的 `.tsx` / `.ts` 文件，入口通常是 `App.tsx`。工具链只有一个 `nsh` 可执行文件：

1. **构建。** `nsh` 打包模块的各个文件（借助 rolldown 和 Oxc），并用自己的编译器编译 JSX。对 `nsh` 与 `nsh/*` 的导入保留为导入：运行时以原生方式提供它们，因此 bundle 里只有你的代码。bundle 会被编译为 QuickJS 字节码。
2. **运行。** 在设备上，引擎线程用 [QuickJS](https://github.com/quickjs-ng/quickjs) 执行字节码。编译后的 JSX 直接调用原生渲染器；一次 JavaScript 执行轮次中的变更被收集起来，由 [Taffy](https://github.com/DioxusLabs/taffy)（flexbox 与 CSS grid）完成布局，再以每帧一批视图操作的形式交给主线程。
3. **渲染。** 主线程把这批操作应用到原生视图上。文本在引擎线程上测量和排版一次（Apple 平台用 CoreText，Android 用 `StaticLayout`，Windows 用 DirectWrite，Linux 用 Pango），主线程只负责绘制。

由于 JavaScript 只描述界面、处理事件，解释器里的工作量很小。解析样式、解析 Tailwind 类名、差异比较、布局和文本处理都在 Rust 中完成。

> [!NOTE]
> 主线程从不等待 JavaScript。系统同步提出的问题（这个输入框还能再接受一个字符吗、用户可以侧滑返回吗、这个面板可以关闭吗）由原生层根据你事先设置的 props 直接回答，例如 `maxLength`、`preventBack` 或 `dismissible`。事件随后才会到达你的代码。

## 你能得到什么

- **嵌入。** `NshAppView`（Swift）、`NshView`（Java/Kotlin）和桌面端的 `AppView`（AppKit、GTK、Win32）可以把模块放进原生应用，数量不限（[iOS](./embedding-ios.md)、[Android](./embedding-android.md)、[macOS](./macos.md#嵌入-mac-应用)、[Linux](./linux.md#嵌入-gtk-应用)、[Windows](./windows.md#嵌入-win32-应用)）。
- **签名、可吊销的下发。** Ed25519 签名的包，发布密钥由离线根密钥认证，支持吊销列表（[签名包](./signed-packages.md)）。
- **由宿主配置的沙箱。** 在 iOS 和 Android SDK 中，每个视图单独设置虚拟机限制和授予的能力（[沙箱与能力](./sandbox-and-capabilities.md)）。
- **模块可以使用宿主的视图。** 用 Swift、Java 或 Rust 注册原生组件（[原生组件](./native-components.md)）。
- **原生视图。** 嵌入原生地图、WebView 或视频播放器没有层级问题，无障碍使用平台自己的树，文本输入是系统的输入。
- **细粒度响应式。** 与 Solid 类似的[信号](./reactivity.md)，在原生层实现。组件只运行一次，只有读取了变化信号的表达式会重新执行。
- **样式与 Tailwind**，在原生层解析：flexbox、CSS grid、阴影、渐变、滤镜、过渡（[样式](./styling.md)、[Tailwind](./tailwind.md)）。
- **移动端 UI 组件集**（`nsh/ui/mobile`），组件用 Rust 编写、由 NSH 绘制，在每个平台上外观一致：[Button](../components/button.md)、[Card](../components/card.md)、[ListView](../components/list-view.md)、[导航](../components/navigation.md) 等。桌面布局用基础元素加上 `desktop={{…}}` 属性和 `Platform.select` 搭建（[平台特定代码](./platform-specific.md)），桌面 UI 套件也在路上。
- **单一可执行文件的工具链。** 一个 `nsh` 可执行文件负责构建模块、生成类型声明、以热模块替换的方式提供开发服务、打包和签名。

## 运行平台

| 平台 | 状态 | 后端 | 说明 |
|---|---|---|---|
| iOS | 已支持（iOS 15.0+） | UIKit | 真机和模拟器；通过 `NativeShell.xcframework` 嵌入（[iOS](./embedding-ios.md)） |
| Android | 已支持（API 28，Android 9+） | Android 视图 | `arm64-v8a` 和 `x86_64`；通过 `nsh.aar` 嵌入（[Android](./embedding-android.md)） |
| macOS | 已支持（macOS 14+） | AppKit | 独立应用；通过 `AppView` 嵌入（[macOS](./macos.md)） |
| Windows | 已支持（Windows 10 1809+） | Win32、Direct2D | 独立应用；通过 `AppView` 嵌入（[Windows](./windows.md)） |
| Linux | 已支持（GTK 4.14+） | GTK 4 | 独立应用；通过 `AppView` 嵌入（[Linux](./linux.md)） |
| GPUI（macOS、Windows、Linux） | 已支持 | GPUI | 独立应用；通过 `AppView` 嵌入 GPUI 和 GPUI Kit 应用（[GPUI](./gpui.md)） |
| Web | 已支持（现代浏览器） | HTML | 静态网站；在网页中用 `createPlayer` 或 iframe 嵌入（[Web](./web.md)） |
| Mac Catalyst | 用于开发 | UIKit | `nsh run` 在 Mac 上打开的 iPhone 尺寸窗口，用来查看模块在手机上的样子 |

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/platforms-light.svg" alt="一个模块，多种宿主：同一个 bundle 和 Rust 运行时，运行在 iOS、Android、Linux、macOS AppKit 和 Windows Win32 / Direct2D、三个桌面平台上的 GPUI，以及用 HTML 绘制的网页。">
<img class="diagram diagram-dark" src="/diagrams/platforms-dark.svg" alt="一个模块，多种宿主：同一个 bundle 和 Rust 运行时，运行在 iOS、Android、Linux、macOS AppKit 和 Windows Win32 / Direct2D、三个桌面平台上的 GPUI，以及用 HTML 绘制的网页。">
</div>

## 与其他方案的比较

在应用里显示发布之后才到来的界面，常见做法有 WebView、用 JSON 描述的服务端驱动 UI，以及小程序容器；构建完整应用则有 React Native、Flutter、Electron 和 Tauri 这类跨平台框架。WebView 渲染的是网页，NSH 模块渲染的是原生界面；服务端驱动的协议只能重新组合内置组件，NSH 模块有自己的状态和逻辑；而且它运行在宿主用代码配置的沙箱里。[方案对比](./comparison.md)逐一做了比较。

## 下一步

- [安装](./installation.md)和[快速上手](./getting-started.md)：安装 `nsh`，编写第一个模块，并在每个平台上运行。
- [平台](./platforms.md)：各平台支持什么，以及如何在 iOS、Android、macOS、Windows 和 Linux 上运行、打包和嵌入。
- [签名包](./signed-packages.md)和[沙箱与能力](./sandbox-and-capabilities.md)：下发模块，并决定它们能做什么。
- [JSX 与组件](./jsx-and-components.md)和[响应式](./reactivity.md)：其余一切都建立在这两个概念之上。

---
title: 方案对比
description: Native Shell 与 React Native、Flutter、Qt、GPUI、Slint、Electron、Tauri 的比较，以及与 WebView、服务端驱动 UI 等发布后下发界面方案的比较。
order: 2
group: start
---

# 方案对比

NSH 是一个在 iOS、Android、macOS、Windows、Linux 和 Web 上编写完整应用的框架，也是向已发布应用下发界面的运行时。本页先把它与人们编写应用所用的跨平台框架比较，再与应用承载安装包之外的界面的几种做法比较。比较关注的是架构——界面由什么绘制、运行在哪里、应用用什么语言编写。其中几个框架还拥有庞大而成熟的生态。

## 跨平台框架

| | 由什么绘制 | 平台 | 应用代码 | 发布后下发的代码 |
|---|---|---|---|---|
| **React Native** | iOS 和 Android 上是平台的视图；桌面由独立项目支持 | iOS、Android；Windows 和 macOS 在主仓库之外 | JavaScript 或 TypeScript，配合 React | 可信应用代码的热更新 |
| **Flutter** | 自己的引擎绘制每个像素 | iOS、Android、macOS、Windows、Linux、Web | Dart | 不属于框架本身 |
| **Qt** | Qt Widgets 与 Qt Quick，由 Qt 绘制 | 桌面、移动、嵌入式系统、Web | C++、QML、Python | 应用加载的 QML |
| **GPUI**（[GPUI Fast](https://github.com/longbridge/gpui-fast)） | 自己的 GPU 渲染器 | macOS、Windows、Linux | Rust | 通过 GPUI Shell 加载的 JavaScript 模块 |
| **Slint** | 自己的渲染器 | 桌面、嵌入式系统、Android、Web | Slint 语言，配合 Rust、C++、JavaScript 或 Python | 不属于框架本身 |
| **Electron** | 随应用打包的 Chromium | macOS、Windows、Linux | HTML、CSS、JavaScript | 应用加载的网页内容 |
| **Tauri** | 系统的 WebView | macOS、Windows、Linux、iOS、Android | HTML、CSS、JavaScript；Native 部分用 Rust | 应用加载的网页内容 |
| **NSH** | 每个平台自己的界面：UIKit、Android 视图、AppKit、用 Direct2D 绘制的 Win32、GTK 4、HTML | iOS、Android、macOS、Windows、Linux、Web | Rust 应用、TSX，或两者结合；运行时是 Rust | 签名的 TSX 包，按视图校验并运行在沙箱中 |

NSH 的不同之处：

- **一套应用模型下，使用每个平台自己的界面。** NSH 统一组件、状态、逻辑和 API，把绘制交给每个平台：文本选择、输入法、无障碍和系统外观都来自平台，系统新版本对它们的改进，应用直接获得（[简介](./index.md#Native 的含义)）。
- **交互随设备而变。** 同一个 `Menu` 在有指针的设备上是贴着触发点的弹出菜单，在手机上是从底部升起的面板；`ContextMenu` 用右键或长按打开。跨平台覆盖的是交互，而不只是代码复用。
- **Rust First，TSX 并行。** 整个应用可以从头到尾都是 Rust，带着 Rust 的类型、线程和 crates，以 Native 速度计算；TSX 服务于快速编写的界面和发布之后下发的界面，运行在同一个引擎上。一个 `nsh` 可执行文件同时构建、测试和打包两者，并自带 TSX 工具。
- **Web 是独立的平台。** 在 Web 上，NSH 渲染用 CSS 设置样式的 HTML 元素，浏览器负责滚动、选择文本和处理输入；发布出来的应用是一个静态网站，像其他网站一样缓存和分发（[Web](./web.md)）。
- **插件和生成的界面运行在同一个引擎上。** NSH 构建的应用——以及用各平台自己的工具编写的 Native 应用——都可以在沙箱中承载签名的 TSX 模块（[动态界面](./index.md#动态界面与-ai-生成的界面)）。

### React Native

React Native 在目标上与 NSH 最接近：两者都希望减少重复的客户端开发，并使用平台的视图。React Native 的中心是 JavaScript 或 TypeScript、React 及其包生态，面向 iOS 和 Android，桌面由独立项目支持。NSH 的中心是 Rust：它的运行时、渲染器和模块都是 Rust，Rust 本身就是应用语言，TSX 是两条路径之一；它的六个平台，包括桌面和 Web，都是同一个框架的后端。

### Flutter

Flutter 用自己的引擎绘制每个像素，因此应用在每个平台上看起来都一样。NSH 走的是另一条路：使用每个平台的视图、文本和文本输入，在 Web 上使用 HTML 元素以及浏览器自己的滚动、选择和输入。文本编辑、输入法和选区都是平台的，而不是在每个平台上重新实现。

### Qt

Qt 是成熟的工具包，有 Qt Widgets 和 Qt Quick 两种界面技术，以及长期积累的工具和平台支持。NSH 的不同在于基础和模型：以 Rust 为核心语言并提供 TSX 路径，工具链为自动化（以及 AI 驱动的）开发而设计，Web 渲染为 HTML，并在一套组件模型里适配移动、桌面和 Web 的交互。

### GPUI 与 GPUI Fast

[GPUI](https://gpui-kit.com) 是用 GPU 绘制的 Rust UI 框架，面向要求很高的桌面应用；[GPUI Fast](https://github.com/longbridge/gpui-fast) 是长桥维护的分支，带来 Retained Mode 渲染——复用没有变化的视图、布局和文本测量——以及滚动图层和 Native 窗口合成。NSH 则建立在每个平台的视图之上，因此文本输入、选区、输入法和系统交互都是平台自己的，并且同样覆盖手机和 Web；NSH 的 GPUI 后端可以把 NSH 应用嵌入 GPUI 和 GPUI Kit 应用（[GPUI](./gpui.md)）。

两者适合不同的负载。在 [NSH 性能基准](https://github.com/longbridge/nsh-perf-bench)中（Linux，500 行行情表，NSH 运行在 GTK 4 后端上），日常负载的开销相同：每秒 1,000 条行情时，NSH Rust 占用 18% 的单核 CPU，GPUI Fast 为 20%。在持续、密集的更新下 GPUI Fast 领先：每秒 20,000 条行情时它占用 25%，NSH 为 73–78%；1,000 个方块逐帧计算的动画，它的 CPU 约为 NSH 的三分之一。

### Slint

Slint 是声明式 UI 工具包，有自己的语言和轻量的渲染器，面向桌面和嵌入式设备，可以从 Rust、C++、JavaScript 或 Python 使用。两者都重视 Native 性能；NSH 的路线是每个平台自己的界面和交互、Web 上的 HTML、Rust 与 TSX 两种应用语言，以及覆盖六个平台的 AI 驱动开发。

## 发布之后下发的界面

应用想要显示安装包里没有的界面——由模型生成的回答、来自合作方或其他团队的功能、无需发版就能变化的页面——无论在手机还是桌面上，都已有几种成熟的做法。它们的区别在于：界面由什么绘制，下发的代码能做什么，以及宿主应用要为承载它们做多大的改动。

| | 由什么绘制 | 能表达什么 | 隔离方式 | 是否适合生成的界面 | 宿主应用的成本 |
|---|---|---|---|---|---|
| **WebView（H5）** | 浏览器引擎 | HTML、CSS 和 JS 能做的一切 | 同源策略；通过手写的桥访问 Native 能力 | 模型擅长写 HTML；结果看起来、用起来都像网页 | 每个视图一个浏览器引擎；需要维护桥 |
| **服务端驱动 UI** | 应用自己的 Native 视图 | 只有应用内置的组件和动作，用 JSON 描述 | 完全受限：schema 就是边界 | 容易生成，能表达的很少：除了 schema 提供的，没有本地状态和逻辑 | 每个应用里都要为每个组件及其各个版本写渲染器 |
| **小程序容器** | 通常是网页引擎加专有的一层 | 平台特定的框架和 API | 容器的权限模型 | 专有方言，模型不太熟悉 | 体积很大的容器 SDK |
| **跨平台应用的热更新**（例如 React Native 配合代码推送服务） | 该框架的渲染器 | 整个框架 | 无：更新是可信的应用代码 | 这类更新不是为按请求生成的代码设计的 | 应用必须用该框架编写 |
| **NSH** | 平台自己的界面：UIKit、Android 视图、AppKit、GTK 4、用 Direct2D 绘制的 Win32、HTML | 带状态、逻辑、动画和布局的组件；宿主注册的 Native 视图 | 签名包，每个视图一个由宿主设定限制的虚拟机，按视图授予能力 | 为此而设计：一个 TSX 模块、类型化的 API、无界面检查 | 在现有 Native 应用里放一个视图和一个库 |

NSH 在这里的不同之处：

- **平台自己的界面。** 模块由平台的视图组成，因此文本选择及其菜单、输入法、无障碍和滚动都是系统自己的——这也是在 [GPUI Shell](./index.md#native-shell-与-gpui-shell) 之外还要做 NSH 的原因。
- **不只是 schema。** 服务端驱动 UI 只能重新组合应用已经内置的东西。NSH 模块有自己的状态、逻辑、动画和布局，模型可以生成应用开发者从未预想到的界面。
- **由宿主隔离。** 下发的代码是不可信的。NSH 在运行任何东西之前检查签名和吊销列表，为每个视图分配一个有内存、调用时长和节点数限制的虚拟机，并由宿主决定模块能访问哪些网络主机、网页和系统设置（[沙箱与能力](./sandbox-and-capabilities.md)）。
- **嵌入在应用里。** 应用保留自己的工具链；NSH 是放在模块应当出现位置的一个视图（[平台](./platforms.md#嵌入 Native 应用)）。
- **一个模块，所有平台。** 同一个 bundle 运行在 iOS、Android、macOS、Windows、Linux 和 Web 上，每次都使用该平台自己的界面（[平台](./platforms.md)）。

## 其他方案更合适的情况

- 界面以数千个视图持续、密集的更新为主——每秒推送数万条行情的交易终端、每帧重画的自定义可视化：GPUI Fast 的保留式 GPU 渲染只占用其中一小部分 CPU（见 [GPUI 与 GPUI Fast](#gpui-与-gpui-fast)）。
- 应用必须在每个平台上像素级一致：Flutter、Slint 这类自己绘制一切的框架天生如此。
- 你现在就需要庞大的第三方包生态：React Native、Flutter 和 Qt 积累了多年。
- 界面是一份文档或一个现成的网页：WebView 能原样显示它。
- 页面集合固定且很少，变化的只是内容和顺序：服务端驱动 UI 完全不需要运行时。

## 另请参阅

- [简介](./index.md)：NSH 是什么，以及它所说的 Native 是什么意思。
- [架构](./architecture.md)：NSH 内部什么在哪里运行。
- [性能](./performance.md)：NSH 遵循的规则，以及如何测量一个应用。

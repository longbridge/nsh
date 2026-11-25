---
title: 简介
description: Native Shell 是 Rust First 的全平台 UI 框架，支持 iOS、Android、macOS、Windows、Linux 和 Web，每个平台都用系统自己的界面与交互。应用用 TSX 编写，需要时加入 Rust。
order: 1
group: start
---

# 简介

Native Shell（简称 NSH）是一个 UI 框架，支持 **iOS、Android、macOS、Windows、Linux 和 Web**。应用只写一份，在每个平台上都是真正的原生应用：用平台自己的视图搭建，用平台自己的文本系统，按平台的习惯交互。

应用用 **TSX** 编写。需要原生性能、系统能力或某个 crate 时，通过 [nshm](./nshm.md) 加入 **Rust**。整个应用也可以全用 Rust 写，两者跑在同一个引擎上。

已经发布的应用还能在运行时加载新界面。界面可以由团队编写，也可以由模型生成，以签名模块的形式下发，运行在宿主应用配置的沙箱里。

> [!IMPORTANT]
> NSH 提供原生视图与交互，视觉设计由应用决定：颜色、字体、间距和形状都归应用。组件、状态和 API 跨平台共享；文本输入、焦点和菜单遵循各设备的习惯。详见[目标与设计原则](./goals.md)。

## Native 的含义

NSH 说的 Native 分两层：平台的基础设施，和平台的交互方式。外观仍由应用自己定义；在支持的平台上，也可以用 Liquid Glass 等系统材质。

**平台的基础设施。** 每个平台都有独立的后端，直接建在平台已有的 UI 上：

| 平台 | 视图 | 文本 | 文本输入 |
|---|---|---|---|
| iOS | UIKit | CoreText | 系统的文本框与文本视图 |
| Android | Android 视图 | `StaticLayout` | `EditText` |
| macOS | AppKit、Core Animation | CoreText | `NSTextField`、`NSTextView` |
| Windows | 用 Direct2D 绘制的 Win32 窗口 | DirectWrite | NSH 的编辑器，配合系统输入法（IMM32） |
| Linux | GTK 4 | Pango | GTK 的 `Text` 与 `TextView` |
| Web | 用 CSS 设置样式的 HTML 元素 | 浏览器的字体 | 浏览器的输入框 |

文本选择和选择菜单、输入法、无障碍、滚动，都由平台提供。系统升级后带来的改进，应用自动获得。地图、网页视图、视频播放器等原生组件，以及宿主注册的任何视图，都和普通视图在同一棵树里，一起布局、裁剪和滚动（[原生组件](./native-components.md)）。

**平台的交互方式。** [`nsh/base`](../modules/nsh-base.md) 里的组件会按各设备的习惯工作，应用只需写一次：

| | 桌面（以及桌面浏览器） | 手机和平板 |
|---|---|---|
| `Menu` | 在触发点弹出；子菜单在菜单项旁展开 | 从底部升起的面板；子菜单替换当前菜单项 |
| `ContextMenu` | 右键在指针处打开 | 长按触发元素，以面板形式打开 |
| `Tooltip` | 指针停留在触发元素上时显示 | 由应用打开时显示 |
| `Popover` | 点击外部关闭，点击同时传给指针下的内容 | 点击外部关闭 |

页面、对话框、键盘和状态栏也是这样。导航栈用平台自己的动效推入、弹出；返回键、Escape 和侧滑返回的行为都和平台一致（[导航](./navigation.md)）。

## 从手机到桌面，再到 Web

NSH 不是把一个界面放大缩小，而是让每种设备拿到它该有的应用：

- **一套应用模型。** 组件、状态、业务逻辑和页面共享，各后端负责映射到自己的平台。
- **布局会重组。** 所有平台的 Flexbox 和 CSS Grid 都由 [Taffy](https://github.com/DioxusLabs/taffy) 排版。Tailwind 断点（`md:`、`lg:`）和窗口尺寸（Rust 中的 `window()`）改变的是结构，不只是大小。比如聊天应用在宽窗口里并排显示侧栏、会话和消息，在手机上一次只显示一屏（[布局](./layout.md)、[Tailwind](./tailwind.md)）。
- **各设备的输入方式。** 桌面有指针、悬停、右键、快捷键和焦点；手机有触摸、手势、软键盘和页面导航。需要按平台区别处理时，用 `desktop={{…}}` 属性和 `platform.select`（[平台相关代码](./platform-specific.md)）。
- **每个目标都能测。** `nsh test` 可以无界面运行测试，也可以在 macOS、Windows、Linux 和 GPUI 窗口、iOS 模拟器、Android 设备或浏览器里，用平台真实输入运行同一批测试（[测试与 trace](./testing-and-trace.md)）。

以聊天客户端为例：在桌面上用 `nsh run` 开发，用 `nsh run --target ios` 看手机布局，用 `nsh run --target web` 看网页版，始终是同一个工程。最后 `nsh package` 打出安装包和网站。以前每个平台要一个团队，现在一个工程师就能做。

## Rust First

引擎、布局、渲染和各平台层都用 Rust 编写，编译成原生代码。应用在上层用 TSX 写，划算的地方用 Rust：

- **用 TSX 写应用。** 界面、状态、逻辑和样式都是 TSX，用 Tailwind 类和 signal。每次保存都会热更新，并保留应用状态。只有你自己的代码是 JavaScript；响应式图、布局、渲染和所有导入的模块都是 Rust（[JSX 与组件](./jsx-and-components.md)）。`nsh init` 默认就是这种方式。
- **需要时用 Rust。** 在 `ext/` 放应用自己的 crate，或使用 crates.io 上的 [nshm](./nshm.md)，为 TSX 提供组件、模块和类。适合繁重计算、解析、大数据、系统 API 和 crates.io 生态。即使由 TSX 调用，也按原生速度运行。
- **全 Rust 应用。** 核心是复杂业务逻辑的应用，可以全部用 Rust 写，用上所有权、类型和线程，没有 JavaScript runtime（[用 Rust 开发](./rust.md)）。
- **一个工具链。** 一个 `nsh` 可执行文件就能创建、构建、类型检查、热更新运行、测试、打包和签名，覆盖所有目标。它自带 TSX 打包器和 TypeScript 检查器，所以 TSX 应用只要装 `nsh`。应用有自己的 Rust 代码时，才需要 Rust 工具链（[安装](./installation.md)、[命令行](./cli.md)）。

## 一个应用

同一个计数器，分别用 TSX 和 Rust 编写：

```tsx tabs
import { mount, signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";

function Counter() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <Text style={{ fontSize: 48, fontWeight: "700" }}>{count()}</Text>
      <View
        onPress={() => count.update((n) => n + 1)}
        activeStyle={{ opacity: 0.6 }}
        class="rounded-lg bg-blue-600 px-5 py-3"
      >
        <Text class="text-white text-lg">Tap</Text>
      </View>
    </View>
  );
}

mount(() => <Window id="main"><Counter /></Window>);
```

```rust tabs
use nsh_core::prelude::*;

#[component]
pub fn Counter() -> impl IntoElement {
    let count = signal(0);
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_3()
        .child(text(move || count.get().to_string()).text_5xl().font_weight(FontWeight::BOLD))
        .child(
            view()
                .rounded_lg()
                .bg(rgb(0x2563eb))
                .px_5()
                .py_3()
                .active(|s| s.opacity(0.6))
                .on_press(move || count.update(|n| *n += 1))
                .child(text("Tap").text_lg().text_color(white())),
        )
}
```

每个视图（`view()`、`<View>`）都会变成真实的平台视图：

| 平台 | 视图 |
|---|---|
| iOS | `UIView` |
| Android | `ViewGroup` |
| macOS | `NSView` |
| Linux | GTK 控件 |
| Windows | 用 Direct2D 绘制的保留视图 |
| Web | HTML 元素 |

文本是原生文本视图，按钮的按压反馈（`.active(..)`、`activeStyle`）在主线程执行。点击时 `count.update` 只改一个 signal，也只更新读取它的那段文本，组件函数不会重新运行。TSX 应用里只有模块自己的代码是 JavaScript；Rust 应用则全部是原生代码。

## Retained Mode

NSH 是 Retained Mode：每个元素都是平台保留的视图，更新只改动变化的部分。

- **细粒度更新。** [Signal](./reactivity.md) 的模型和 Solid 一样，但由原生实现。组件只运行一次，变化时只重跑读取该 signal 的表达式。
- **只有变化跨线程。** 同一轮的变化一起排版，每帧打成一批视图操作交给主线程。没变的样式和位置不发送，没动的视图保持原样。
- **动画在原生侧运行。** 时间曲线、弹簧、衰减动画、手势和滚动联动值都跟着平台的帧时钟计算，不需要每帧调用 JS（[动画](./animation.md)）。

大多数应用的界面正是这样：聊天、邮件、表单、列表、文档、设置、流式输出的 AI 回复。每次只有少数视图在变。两次更新之间几乎不占 CPU，在 ProMotion 屏幕上滚动和动画稳定在 120 FPS。

在 [NSH 性能基准](https://github.com/longbridge/nsh-perf-bench)中，一个 500 行的行情表每秒更新 1,000 次。在 Linux 上，NSH Rust 版占单核 18%，直接用 [GPUI Fast](https://github.com/longbridge/gpui-fast) 写的同一界面占 20%。

重计算请放到 Rust。TSX 跑在解释器 QuickJS 上，而 Rust 模块即使由 TSX 调用，也是原生速度：排序 10 万行约 3 ms，和纯 Rust 一样（[性能](./performance.md)）。

## 如何运行

工具链只有一个 `nsh` 可执行文件。应用可以用 TSX、Rust 或两者一起写，最后都进入同一个 Rust 引擎。

### TSX 应用

TSX 应用（即模块）是一组 `.tsx` / `.ts` 文件，入口通常是 `App.tsx`：

1. **构建。** `nsh` 用 rolldown 和 Oxc 打包，用自带编译器编译 JSX。`nsh` 和 `nsh/*` 的导入原样保留，由运行时原生提供，所以 bundle 里只有你的代码，最后编译成 QuickJS 字节码。
2. **运行。** 引擎线程在 [QuickJS](https://github.com/quickjs-ng/quickjs) 中执行字节码，每个视图一个虚拟机。编译后的 JSX 直接调用原生渲染器，JavaScript 只负责描述界面和处理事件。样式解析、Tailwind 类解析、diff、布局和文本都在 Rust 里完成。

### Rust 应用

Rust 应用就是一个 crate，没有 JavaScript runtime：

1. **构建。** `nsh` 为每个目标（iOS、Android、macOS、Windows、Linux 或 Web）构建运行器，并链接你的 crate。组件、状态、事件处理和引擎一起编译成原生代码。
2. **运行。** 根组件以 Rust 形式跑在引擎线程上。运行器里没有 JavaScript 引擎，任何平台上的 Rust 应用都不带 QuickJS。

### 两者的区别

| | TSX 应用 | Rust 应用 |
|---|---|---|
| 编写语言 | TSX（TypeScript 与 JSX） | Rust |
| 构建产物 | QuickJS 字节码，由运行时加载 | 原生代码，链接进每个目标的运行器 |
| JavaScript 引擎 | QuickJS，每个视图一个虚拟机 | 无 |
| 开发时（`nsh run`） | 每次保存后热更新 | 每次保存后重新构建并重启 |
| 发布之后 | 随应用发布；也可在运行时以签名包下发，运行在沙箱里 | 随应用发布 |
| 适合 | 大多数应用、插件、运行时下发或模型生成的界面 | 核心是繁重计算或系统工作的应用 |

两者可以混用。TSX 应用通过 nshm 导入 Rust 组件、模块和类，可以是 `ext/` 里自己的，也可以是已发布的，它们都以原生代码运行（[nshm](./nshm.md)）。Rust 应用也能用同样方式承载 TSX 页面（[用 Rust 开发](./rust.md)）。

### 共用的引擎

进入引擎之后，两条路径完全相同：

1. 收集每一轮的变化。
2. 由 [Taffy](https://github.com/DioxusLabs/taffy) 排版（Flexbox 和 CSS Grid）。
3. 每帧把一批视图操作交给主线程，主线程应用到原生视图上。

文本只在引擎线程上测量和排版一次：Apple 平台用 CoreText，Android 用 `StaticLayout`，Windows 用 DirectWrite，Linux 用 Pango，Web 用浏览器的字体度量。主线程只负责绘制。

在 Web 上，同一个引擎跑在页面里。浏览器负责显示 CSS 样式的 HTML 元素、滚动、选择文本和处理输入。`nsh package --target web` 把应用发布成静态网站，文件按内容命名，可以直接放到任何 CDN（[Web](./web.md)）。

> [!NOTE]
> 主线程从不等待应用代码。系统有时会同步询问：这个输入框还能不能输入字符？用户能不能侧滑返回？这个面板能不能关？NSH 根据事先设置的属性在原生侧直接回答，例如 `maxLength`、`popOnBack`、`closeOnSwipe`。事件之后才交给应用的处理函数。

## 动态界面与 AI 生成的界面

NSH 最初就是为了让原生应用在发布后还能加载新界面，这至今仍是主要用途之一。应用在需要的位置嵌入一个视图，NSH 在里面运行 TSX 模块：

| 平台 | 嵌入方式 |
|---|---|
| iOS | `NshAppView` |
| Android | `NshView` |
| macOS、Windows、Linux、GPUI 应用 | `AppView` |
| 网页 | `createPlayer` |

- **发布后下发。** 模块是签名包（`.nshpkg`），应用在运行时下载。运行前，应用用自己信任的密钥和吊销列表校验签名（[签名包](./signed-packages.md)）。宿主应用集成运行时，业务团队按它的 API 开发模块，按自己的节奏下发。
- **宿主说了算。** 每个模块有独立的 QuickJS 虚拟机，内存、调用时长和节点数都有上限。模块只能用宿主授予的能力：网络主机、网页、状态栏（[沙箱与能力](./sandbox-and-capabilities.md)）。多个模块可以同时运行。
- **和应用风格一致。** 模块可以复用应用的组件样式。宿主还可以注册自己的原生视图给模块用，比如地图、播放器、设计系统组件（[原生组件](./native-components.md)）。

同一套机制还能让应用**直接用界面回答**。模型写一个小的 TSX 模块，比如表单、图表或预订卡片；你的服务器检查、构建并签名；应用用原生方式显示：

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/ai-flow-light.svg" alt="从提示词到 Native 界面：模型写出 TSX 模块；你的服务器构建、检查并签名；越过信任边界后，应用校验包，在它设定的限制和能力下于沙箱中运行，并用平台自己的界面显示。">
<img class="diagram diagram-dark" src="/diagrams/ai-flow-dark.svg" alt="从提示词到 Native 界面：模型写出 TSX 模块；你的服务器构建、检查并签名；越过信任边界后，应用校验包，在它设定的限制和能力下于沙箱中运行，并用平台自己的界面显示。">
</div>

在 iOS 上，宿主只需三行：

```swift
let app = NshAppView()                         // 一个 UIView：放在任意位置
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
let info = try await app.loadPackage(at: downloadedFile, trust: trust)   // 校验通过后运行
```

## AI-Friendly

NSH 为 AI 编写的软件而设计，不论是完整应用还是生成的界面：

- **所有平台一套模型。** 模型只学一种写界面的方式，结果就能跑在手机、桌面和 Web 上。交互由 NSH 按平台适配，不用把同一个界面生成六遍。
- **TSX 为主，必要处用 Rust。** 应用、页面、插件和按请求生成的界面用 TSX；性能关键的模块或完整应用用 Rust。两者共用一个引擎。
- **精确的生成契约。** 每个元素、属性、样式键和模块都有从 Rust 源码生成的 TypeScript 声明（`.nsh/types`），Rust API 本身也有类型，生成的代码都要过编译器检查。文档也提供纯文本版给模型读（[`llms.txt`](/llms.txt)、[`llms-full.txt`](/llms-full.txt)）。`npx skills add longbridge/nsh` 会把编码指南和设计指南装成编程 Agent 的技能。
- **一个命令行走完整个循环。** Agent 通过 `nsh` 创建、构建、运行、测试、修复和打包应用。`nsh test` 不需要设备，无界面运行，并报告错误和生成的视图。编译器也能在进程内（`nsh_bundler::compile_module`）和浏览器里运行（[Playground](/playground)）。
- **不可信代码也能安全运行。** 运行时下发的生成代码按不可信处理：必须带签名，跑在有限制的虚拟机里，只能用宿主授予的能力。

## 运行平台

| 平台 | 系统要求 | 后端 | 嵌入方式 |
|---|---|---|---|
| [iOS](./ios.md) | iOS 15.0+ | UIKit | `NativeShell.xcframework` |
| [Android](./android.md) | Android 8+ | Android 视图 | `nsh.aar` |
| [macOS](./macos.md) | macOS 14+ | AppKit | `AppView` |
| [Windows](./windows.md) | Windows 10 1809+ | Win32、Direct2D | `AppView` |
| [Linux](./linux.md) | GTK 4.14+ | GTK 4 | `AppView` |
| [Web](./web.md) | 现代浏览器 | HTML | `createPlayer`、iframe |
| [TUI](./tui.md) | macOS、Windows、Linux 上的终端 | ratatui | `AppView`（ratatui 区域） |
| [GPUI](./gpui.md) | 桌面平台 | GPUI | `AppView`（GPUI、GPUI Kit 应用） |

开发时，可以用 Mac Catalyst（`--target catalyst`）在 Mac 上运行 iOS 后端，处理 Catalyst 相关的问题。

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/platforms-light.svg" alt="一个模块，多个宿主：同一个 bundle 和 Rust 运行时运行在 iOS、Android、Linux、使用 AppKit 的 macOS 和使用 Win32 / Direct2D 的 Windows 上，GPUI 覆盖三个桌面平台，网页则用 HTML 绘制。">
<img class="diagram diagram-dark" src="/diagrams/platforms-dark.svg" alt="一个模块，多个宿主：同一个 bundle 和 Rust 运行时运行在 iOS、Android、Linux、使用 AppKit 的 macOS 和使用 Win32 / Direct2D 的 Windows 上，GPUI 覆盖三个桌面平台，网页则用 HTML 绘制。">
</div>

## Native Shell 与 GPUI Shell

[长桥（Longbridge）](https://longbridge.com)为自己的客户端开发 NSH。它最早是移动客户端的插件层，起点是 [GPUI Shell](https://gpui-kit.com/shell)。GPUI Shell 让 [GPUI](https://gpui-kit.com) 桌面应用可以用 JavaScript 扩展。

如果在手机上也用 GPUI 自绘界面，就得重做平台已有的能力：文本选择和选择菜单（拷贝、分享、翻译、查询）、输入法、无障碍，以及每个 iOS 版本新增的视觉材质。每个移动团队还得学一套新的 UI 技术栈。所以 NSH 改用各平台自己的 UI 渲染，并从插件层发展成能写完整应用的全平台框架。

两者定位不同。GPUI Shell 用来扩展 GPUI 桌面应用；Native Shell 用来开发手机、桌面和 Web 上的原生应用，它的 GPUI 后端还能把 NSH 应用嵌进 GPUI 和 GPUI Kit 应用（[GPUI](./gpui.md)）。

## 与其他方案的比较

做完整应用时，NSH 的同类是 React Native、Flutter、Qt、GPUI、Slint、Electron 和 Tauri 等跨平台框架；做发布后下发的界面时，同类是网页视图、服务端驱动 UI 和小程序容器。NSH 的选择是：一套应用模型，各平台用自己的界面和交互；用 TSX 写，跑在 Rust 引擎上，需要时直接用 Rust。[方案比较](./comparison.md)逐一做了对比。

## 下一步

- [安装](./installation.md)与[快速上手](./getting-started.md)：安装 `nsh`，写第一个应用，在各平台上运行。
- [nshm](./nshm.md)：用 Rust 组件、模块和类扩展 TSX 应用，可以是应用自己的，也可以是已发布的。
- [用 Rust 开发](./rust.md)：从头到尾用 Rust 编写的应用。
- [平台](./platforms.md)：哪些平台能运行什么，以及如何在每个平台上运行、打包和嵌入。
- [JSX 与组件](./jsx-and-components.md)和[响应式](./reactivity.md)：两个核心概念，TSX 和 Rust 通用。
- [签名包](./signed-packages.md)与[沙箱与能力](./sandbox-and-capabilities.md)：下发模块，并决定它们能做什么。

---
title: 平台概览
description: "NSH 支持 iOS、Android、macOS、Windows、Linux、GPUI、终端和 Web。介绍各平台如何运行、打包和嵌入 TSX 或 Rust 应用，以及需要什么环境。"
order: 10
group: platforms
---

# 平台概览

NSH 支持 iOS、Android、macOS、Windows、Linux、GPUI、终端和 Web。每个平台有自己的后端，用平台原生界面绘制：iPhone 上是 UIKit 视图，Mac 上是 AppKit 视图，Linux 上是 GTK 4 控件，GPUI 应用里是 GPUI 元素，终端里是字符 Cell，浏览器里是 HTML。文本、输入、无障碍和深色模式都用平台自己的实现。

后端之上全部共享同一份 Rust 代码：响应式图、渲染器、Taffy 布局、组件和原生模块。所以一个应用能跑在所有平台上。Rust 应用就是这份代码加你的 crate，不带 JavaScript 虚拟机；TSX 应用在此之上用 QuickJS 运行模块（[用 Rust 开发](./rust.md)）。

每个平台上，应用都有两种用法：

- 用 `nsh package` 打包成该平台的可安装应用。
- 嵌入你自己的原生应用。宿主可以链接 Rust 根组件，也可以运行时加载 TSX。

开发时用 `nsh run`：改 Rust 会重新构建并重启，改 TSX 直接热更新。

## 一览

| 平台 | 后端；文本 | 运行环境 | 开发 | 打包 |
|---|---|---|---|---|
| [iOS](./ios.md) | UIKit；CoreText | iOS 15.0+，真机和模拟器 | `nsh run --target ios` | `.app`（模拟器）、签名的 `.ipa` |
| [Android](./android.md) | Android 视图；`StaticLayout` | Android 8（API 26）+，`arm64-v8a`、`armeabi-v7a` 和 `x86_64` | `nsh run --target android` | 签名的 `.apk` |
| [macOS](./macos.md) | AppKit 与 Core Animation；CoreText | macOS 14+ | `nsh run --target macos` | ad hoc 签名的 `.app` |
| [Windows](./windows.md) | 用 Direct2D 绘制的 Win32 窗口；DirectWrite | Windows 10 1809+、Windows 11 | `nsh run`（Windows 上的默认目标） | 一个目录、一个 `.zip` 和一个安装程序 |
| [Linux](./linux.md) | GTK 4 控件；Pango | GTK 4.14+ | `nsh run`（Linux 上的默认目标） | 一个目录和一个 `.tar.gz` |
| [GPUI](./gpui.md) | GPUI；GPUI 的文字系统 | macOS、Windows、Linux（Vulkan） | 在你的 GPUI 应用里；TSX 也可用 `nsh run --target gpui` | 随嵌入它的 GPUI 应用发布 |
| [Web](./web.md) | HTML 与 CSS；浏览器的字体 | 现代浏览器，桌面和手机 | `nsh run --target web` | 一个静态网站 |
| [TUI](./tui.md) | 在终端 Cell 网格上用 ratatui 绘制；终端的字体 | macOS、Windows 和 Linux 上的终端 | `nsh run --target tui` | 随嵌入它的终端应用发布 |

- **[GPUI](./gpui.md)** 用 GPUI Kit 背后的 [GPUI](https://gpui-kit.com) 绘制，支持 macOS、Windows 和 Linux。它用于把 NSH 嵌入 GPUI 和 GPUI Kit 应用，宿主把 `AppView` 放进自己的元素树。独立桌面应用则用各系统自己的后端（AppKit、Win32、GTK 4）。
- **[TUI](./tui.md)** 在终端的字符 Cell 网格上绘制，使用终端配色，以键盘为主，也支持鼠标。`nsh run --target tui` 在当前终端运行应用；ratatui 应用用 `AppView` 嵌入。
- **[Web](./web.md)** 把应用渲染成 HTML。`nsh package --target web` 生成静态网站，可部署到任意 Web 服务器或 CDN；Rust 应用的引擎和 crate 编译为 WebAssembly。你的网页可以用 `createPlayer` 或 iframe 嵌入应用。本站的在线示例和 [Playground](/playground) 就是这样运行的。

社区也基于同一引擎做了更多平台，各自独立成仓库，`nsh` 命令行按名字调用。例如 [`nsh-platform-miniprogram`](https://github.com/huacnlee/nsh-platform-miniprogram) 可在微信小程序中运行应用（`--target miniprogram`）。做法见[构建平台](./platform-extensions.md)。

**开发工具：**`--target catalyst` 在 Mac 上用 iPhone 尺寸的窗口（390×844）运行 iOS 后端。只在需要检查 Catalyst 特有行为、或把 NSH 嵌入 Mac Catalyst 应用时使用。做 Mac 应用用默认的 `macos`；看手机效果用 `--target ios`。

## 开发与打包

不传 `--target` 时，`nsh run` 使用本机平台：macOS 用 AppKit，Linux 用 GTK，Windows 用 Win32。手机需要通过电脑开发：iOS 要装了 Xcode 的 Mac；Android 要在 macOS、Linux 或 Windows 上装 Android SDK（[安装](./installation.md#各平台可用的命令)）。

Rust 应用在 `nsh.json` 里写明 `"root"` 根组件，在项目目录下运行和打包。`nsh` 会为目标平台构建专属 runner，包含你的 crate，不带 QuickJS。TSX 应用需要指定入口文件：

```sh
# Rust 应用，在它的目录里
nsh run --target macos             # 或 ios、android、windows、linux、tui、web、catalyst
nsh package --target macos         # 或 ios、ios-simulator、android、windows、linux、web

# TSX 应用
nsh run App.tsx --target macos     # 或 ios、android、windows、linux、gpui、tui、web、catalyst
nsh package App.tsx --target macos
```

桌面应用要在对应系统上打包：Mac 应用在 Mac 上，Windows 应用在 Windows 上，Linux 应用在 Linux 上。各目标的产物、签名和图标见[打包](./packaging.md)。

## 嵌入原生应用

各平台的分步指引见[嵌入现有应用](./embedding.md)。

| 平台 | 宿主视图 | 运行 | 发布形式 | 宿主语言 |
|---|---|---|---|---|
| macOS | `AppView`（一个 `NSView`） | Rust 根组件、TSX bundle | `nsh-platform-macos` crate | Rust |
| Windows | `AppView`（一个子窗口） | Rust 根组件、TSX bundle | `nsh-platform-windows` crate | Rust |
| Linux | `AppView`（一个 GTK 控件） | Rust 根组件、TSX bundle | `nsh-platform-linux` crate | Rust |
| GPUI（macOS、Windows、Linux） | `AppView`（一个 GPUI 视图） | Rust 根组件、TSX bundle、签名包 | `nsh-platform-gpui` crate | Rust |
| iOS | `AppView`（一个 `UIView`） | Rust 根组件、TSX bundle | `nsh-platform-ios` crate | Rust |
| iOS | `NshAppView`（一个 `UIView`） | TSX bundle、签名包 | `NativeShell.xcframework` | Swift |
| Android | `NshView`（一个 `FrameLayout`） | TSX bundle、签名包 | `nsh.aar` | Java 或 Kotlin |
| Web | `createPlayer`（一个页面元素）或 iframe | TSX bundle；Rust 应用以发布的网站嵌入 | `nsh package --target web` 生成的运行时文件 | JavaScript 或 HTML |
| TUI | `AppView`（ratatui frame 中的一块区域） | Rust 根组件、TSX bundle | `nsh-platform-tui` crate | Rust |

Rust 宿主以 `AppSource::Rust` 或 `App::Rust` 挂载 Rust 根组件（`Root::of::<App>()`），再用 `set_ui_sets` 把应用的 UI 集合交给引擎。这样加载的 TSX 也能导入应用自己的组件和模块。一个视图对应一个引擎，一个应用可以同时显示多个视图。各平台的宿主都能注册自己的原生视图供应用使用（[原生组件](./native-components.md)）。

> [!NOTE]
> Rust 宿主用 NSH 仓库里的平台 crate 嵌入。macOS、Windows 和 Linux 的 `AppView` 从应用自带的文件（或源代码）加载 bundle，使用默认限制。GPUI crate 和 iOS、Android SDK 还能加载[签名包](./signed-packages.md)，并按视图设置限制和能力（[沙箱与能力](./sandbox-and-capabilities.md)）。

### 共享移动宿主的 Tokio 运行时

链接了 Rust 的移动宿主，可以让 NSH 的异步任务跑在自己已有的 Tokio 运行时上。每个视图持有运行时 `Handle` 的克隆。运行时归应用所有，应用要让它一直运行（开启 IO 和时间驱动），直到视图停止。传 `None` 则用 NSH 的共享运行时。

iOS 在 `AppView`（或 `Surface`）的选项里设置，视图每次加载都会用它：

```rust
let mut options = SurfaceOptions::default();
options.runtime = Some(runtime.handle().clone());
let view = AppView::new(mtm, frame, options);
view.load(bundle);
```

Android 用 `NshView` 的原生句柄（`handle` 字段）调用 `nsh::set_runtime`：

- 在 UI 线程调用，且要早于视图加载应用或连接开发服务器。
- 设置只作用于该视图，从下一次加载起生效。已在运行的引擎继续用启动时的运行时。

句柄由应用自己库里的 JNI 函数读取：

```rust
#[unsafe(no_mangle)]
extern "system" fn Java_com_example_app_RuntimeBridge_configure(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    view: JObject<'_>,
) {
    let Ok(handle) = env.get_field(&view, "handle", "J").and_then(|v| v.j()) else {
        return;
    };
    // SAFETY: called on the UI thread with a live NshView of this library.
    unsafe { nsh::set_runtime(handle, Some(app_runtime().clone())) };
}
```

开启混淆时，要保留桥接类的名称（`-keep class com.example.app.RuntimeBridge { *; }`）。官方分发的 AAR 已保留 `com.longbridge.nsh` 包，包括 `NshView.handle`。

## 为每个平台编写

大部分代码不用判断平台：元素、样式、布局和模块在各平台行为一致。确有差异时，NSH 把差异集中在一处处理（[平台特定代码](./platform-specific.md)）：

- **判断运行环境**：Rust 用 `os()`、`idiom()` 和 `is_desktop()`（`nsh_core::prelude`）；TSX 用 `nsh` 导出的 `platform.os`、`platform.idiom`、`platform.desktop` 和 `platform.select`。OS 取值为 iOS、Android、macOS、Windows、Linux 或 Web。在终端里，idiom 是 `"terminal"`，OS 是本机系统。
- **平台专属属性**：TSX 里写在 `ios={{…}}`、`android={{…}}`、`macos={{…}}`、`windows={{…}}`、`web={{…}}` 或 `desktop={{…}}` 中。`desktop` 包括 macOS、Windows、Linux、桌面浏览器和终端。
- **输入随平台。** 手机用触摸和软键盘，桌面用鼠标、触控板和键盘。

组件都由 NSH 基础元素绘制，所以在桌面和手机上运行方式、外观都一致。桌面布局用基础元素（`view`、`text`、`scroll`、`virtual_list` 等；TSX 里是 `View`、`Text`、`Scroll`、`List` 等）加一个桌面判断即可，gallery 的桌面布局就是这样写的：

```tsx tabs
import { View } from "nsh";
import { platform } from "nsh";

export function Shell() {
  // macOS、Windows、Linux 和桌面浏览器用侧边栏；手机用标签页。
  return <View class="flex-1">{platform.desktop ? <SidebarLayout /> : <TabLayout />}</View>;
}

function SidebarLayout() {
  return <View class="flex-1 flex-row" />;
}

function TabLayout() {
  return <View class="flex-1 flex-col" />;
}
```

```rust tabs
use nsh_core::prelude::*;

#[component]
pub fn Shell() -> impl IntoElement {
    // macOS、Windows、Linux 和桌面浏览器用侧边栏；手机用标签页。
    view()
        .flex_1()
        .child(if is_desktop() { sidebar_layout() } else { tab_layout() })
}

fn sidebar_layout() -> AnyElement {
    view().flex_1().flex_row().into_element()
}

fn tab_layout() -> AnyElement {
    view().flex_1().flex_col().into_element()
}
```

各后端的特殊之处见：[macOS](./macos.md#macos-上的差异)、[Windows](./windows.md#windows-上的差异)、[Linux](./linux.md#linux-上的差异)、[GPUI](./gpui.md#差异)、[TUI](./tui.md)。

## 另请参阅

- [快速开始](./getting-started.md)：写第一个应用，并在各平台运行。
- [用 Rust 开发](./rust.md)：纯 Rust 应用，以及 Rust 加 TSX。
- [安装](./installation.md)：各平台需要的工具。
- [架构](./architecture.md)：哪些部分共享，各后端负责什么。
- [构建平台](./platform-extensions.md)：基于 NSH 引擎构建自己的平台。

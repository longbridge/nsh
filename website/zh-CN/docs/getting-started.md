---
title: 快速开始
description: 用 nsh init 创建 NSH 应用，在 iOS、Android、macOS、Windows、Linux 或 Web 上运行，再发布：作为签名 TSX 包嵌进你的应用，或打包成独立应用。
order: 4
group: start
demo: guide-getting-started
---

# 快速开始

本指南带你创建一个 NSH 应用，运行起来，再发布出去。

NSH 应用用 TSX 编写，开发时支持热更新。你的代码跑在 QuickJS 上，底下的引擎、布局、渲染和模块都是原生的。需要原生性能、系统能力或某个 crate 时，用 [nshm](./nshm.md) 加 Rust。也可以整个应用都用 Rust 写，不带 JavaScript 虚拟机。下面每个例子都同时给出 TSX 和 Rust 两种写法。

先安装 `nsh` 并加入 `PATH`（[安装](./installation.md)）。在 NSH 源码仓库里用 `./target/debug/nsh`。

> [!TIP]
> 不需要现成的应用。前两节让应用单独跑在桌面窗口、iOS 模拟器、Android 设备或浏览器里。之后可以直接[打包成独立应用](#或者作为独立应用发布)。

## 创建应用

`nsh init` 会让你选模板：`ts`（默认）、`rust` 或 `mixed`（TSX 加 `ext/` 里的 Rust）。加 `--template` 可跳过询问。

### 用 TSX

```sh
nsh init my-app --template ts
```

TSX 应用由一个或多个 `.tsx` 文件组成，有一个入口。`nsh run` 运行这个入口，宿主应用把它当模块加载。

| 文件 | 作用 |
|---|---|
| `App.tsx` | 入口模块：用 `mount` 渲染应用 |
| `App.test.tsx` | 它的测试（`nsh test`） |
| `nsh.json` | 应用的名称、id、版本和构建号（[项目结构](./project-structure.md#nshjson)） |
| `tsconfig.json` | 继承 NSH 的 TypeScript 设置 `.nsh/tsconfig.json`，把 `nsh` 指向 `.nsh/types` |
| `.gitignore` | 忽略 `.nsh/` 和 `target/` |
| `.nsh/types/` | 生成的所有 NSH 模块的声明，供编辑器使用 |

### 用 Rust

```sh
nsh init my-app --template rust
```

这会新建一个目录，里面是 Cargo 项目：

| 文件 | 作用 |
|---|---|
| `src/lib.rs` | 应用本身：根组件、状态和事件处理 |
| `src/main.rs` | 为 `cargo run` 在 macOS、Windows 或 Linux 上直接挂载同一个根组件 |
| `Cargo.toml` | crate 清单，依赖与 CLI 同版本的 `nsh-core` 和 `nsh-base` |
| `nsh.json` | 应用的名称、id、版本和构建号，它的 crate（`"rust": "."`）和根组件（`"root": "App"`）（[项目结构](./project-structure.md#nshjson)） |
| `App.test.tsx` | 根组件的测试，用 TypeScript 编写（`nsh test`），附带供编辑器使用的 `tsconfig.json` 和 `.nsh/types` |
| `README.md` | 如何运行、测试和打包应用，以及之后如何添加 TSX 入口 |
| `.gitignore` | 忽略 `.nsh/` 和 `target/` |

Rust 应用需要 Rust 工具链（[rustup](https://rustup.rs)），构建推荐用 [huacnlee/cargo](https://github.com/huacnlee/cargo)。`nsh` 用 crates.io 上与 CLI 同版本的 NSH crate 构建运行器。

两种模板都要注意：

- TSX 应用可以建在已有目录里，已存在的文件不会覆盖。Rust 应用总是新建目录。
- 显示名称默认取目录名，用 `--name "My App"` 修改。
- id 是 `com.example.<name>`，发布前在 `nsh.json` 里改掉。

### 用 Rust 扩展的 TSX

```sh
nsh init my-app --template mixed
```

除了 TSX 应用，还会在 `ext/<name>-native/` 生成一个 nshm crate，`App.tsx` 导入其中的组件、模块和类。更适合用 Rust 写的部分放这里（[应用自己的 Rust：ext/](./nshm.md#应用自己的-rustext)）。`ts` 应用以后想加，用 `nsh add`。

### 根组件

两种模板生成同一个计数器应用：

```tsx tabs
// App.tsx
import { mount, signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";

function App() {
  const count = signal(0);
  return (
    <View class="flex-1 items-center justify-center gap-4 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <Text class="text-3xl font-semibold text-foreground">{count()}</Text>
      <View
        onPress={() => count.update((c) => c + 1)}
        role="button"
        class="rounded-(--radius) bg-primary px-5 py-3 active:bg-primary/90"
      >
        <Text class="text-lg text-primary-foreground">Tap</Text>
      </View>
    </View>
  );
}

// 桌面平台上，应用在它自己声明的窗口里显示；`App` 是第一个窗口的内容。
// 非桌面平台上，它就是应用的屏幕。
mount(() => <Window id="main">{App}</Window>);
```

```rust tabs
// src/lib.rs
use nsh_base::theme;
use nsh_core::{element::desktop::window as app_window, prelude::*};

/// crate 的 UI 集合：TSX 入口可以从 `my-app/native` 导入的内容。
#[native_module("my-app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// 应用的根组件。
#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    app_window().id("main").content(move || {
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(
            view()
                .flex_1()
                .p_safe_area()
                .child(
                    view()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(text(move || format!("Count: {}", count.get())).text_size(px(28.0)))
                        .child(
                            view()
                                .px_5()
                                .py_3()
                                .rounded(px(10.0))
                                .bg(move || theme().primary)
                                .active(|s| s.opacity(0.6))
                                .on_press(move || count.update(|value| *value += 1))
                                .child(
                                    text("Tap")
                                        .text_size(px(17.0))
                                        .text_color(move || theme().primary_foreground),
                                ),
                        ),
                ),
        )
    })
}
```

TSX 版：

- **`from "nsh"`**：核心模块，包含 signal、`mount`、其他响应式原语和宿主元素。它由原生运行时提供，不打包进应用。**`nsh/base`** 提供基础组件。
- **`function App()`** 是组件，只运行**一次**。**`{count()}`** 读取 signal，编译器会让这段文本随 signal 更新。
- **`<View onPress>`** 是响应按压的原生视图，`active:` 类是按下时的样式。`View`、`Text`、`Image`、`Scroll` 等[宿主元素](../elements/index.md)从 `nsh` 导入，`Input` 从 `nsh/base` 导入。只有 `If` 和 `Each` 不用导入。
- **`class`** 写 [Tailwind](./tailwind.md) 类。`style={{ … }}` 写 camelCase 样式属性，数字单位是点。`bg-background`、`bg-primary`、`text-primary-foreground` 和 `rounded-(--radius)` 是主题 token（[主题 token](./tailwind.md#主题-token)），自动适配深色模式，可用 `nsh.json` 的 `"theme"` 改样式。
- **布局**用 flexbox，子元素默认纵向排列（[布局](./layout.md)）。
- **`mount(App)`** 渲染根组件。桌面平台上，根组件用 `<Window>` 声明窗口。

Rust 版：

- **`use nsh_core::prelude::*`** 导入基本元素（`view()`、`text()`、`scroll()`、`input()`、`image()` 等）、[signal](./reactivity.md) 和带类型的样式方法。`.p_safe_area()` 按[安全区域](../elements/safe-area.md)加内边距。
- **`nsh_base`** 提供无样式的基础组件（`button()`、键盘、对话框）和 `theme()`，即应用的颜色和圆角（[主题](./rust.md#主题)）。
- **`#[component] pub fn App()`** 是组件，只运行**一次**，返回自己的元素。`nsh.json` 里的 `"root": "App"` 指向它，`nsh run` 负责挂载。`signal(0_u32)` 保存一个会变的值。
- **`text(move || format!(…, count.get()))`** 把文本绑定到 signal。`count` 变了，NSH 重跑闭包，只更新这段文本。样式也能传闭包：`.bg(move || theme().primary)` 会跟随深色模式。
- **`.on_press(..)`** 处理按压，`.active(|s| s.opacity(0.6))` 是按下时的样式，由原生层应用。按钮是应用自己画的：NSH 提供基本元素和能力，界面由应用组合。
- **`.px_5()`、`.rounded(px(10.0))`、`.gap_4()`** 是带类型的样式方法，命名沿用 Tailwind，单位是点（`px`）。
- **`#[native_module("my-app/native")]`** 声明 TSX 入口能从这个 crate 导入什么，方便以后加 TSX 页面（[用 Rust 开发](./rust.md#用-tsx-扩展-rust-应用)）。

> [!IMPORTANT]
> 文字必须放在文本元素里：TSX 用 `<Text>`，Rust 用 `text(..)`。TSX 中直接写在 `<View>` 里的文字（`<View>Hello</View>`）会构建报错；表达式产生的字符串（`<View>{name}</View>`）会在日志里警告。

## 开发

### 运行

`nsh run` 负责构建、启动，并在你修改时重新构建，日志输出到终端。传入应用目录即可：TSX 应用运行 `App.tsx`，Rust 应用运行根组件。要换入口，写成 `nsh run my-app/App.tsx`。

| 应用 | `nsh run` 做什么 |
|---|---|
| TSX | 启动开发服务器和开发宿主，宿主从服务器加载应用，每次保存都给运行中的应用打补丁。`ext/` 里的 Rust 编进宿主，改动后重新构建。 |
| Rust | 为目标平台构建应用自己的运行器：包含你的 crate、挂载根组件、不带 QuickJS。Rust 代码一改就重新构建并重启。 |

Rust 应用也可以在目录里直接 `cargo run`，在桌面后端启动同一个根组件。

不加 `--target` 时跑在本机：macOS 用 AppKit，Windows 用 Win32，Linux 用 GTK。其他平台用 `--target`：

| 目标 | 命令 | 在哪里运行 |
|---|---|---|
| iOS 模拟器 | `nsh run my-app --target ios` | 装有 Xcode 的 macOS |
| Android | `nsh run my-app --target android` | macOS、Windows、Linux |
| macOS（AppKit） | `nsh run my-app` | macOS（默认） |
| Windows | `nsh run my-app` | Windows（默认） |
| Linux（GTK 4） | `nsh run my-app` | Linux（默认） |
| Web | `nsh run my-app --target web` | macOS、Windows、Linux |

#### iOS 模拟器

```sh
nsh run my-app --target ios
nsh run my-app --target ios --device "iPhone 16 Pro"
```

`nsh` 优先用已启动的模拟器，没有就启动最新的 iPhone，然后装入 Rust 应用的运行器或开发宿主。`--device` 按名称或 UDID 指定模拟器。

#### Android

```sh
nsh run my-app --target android
```

先启动模拟器或连上设备（`adb devices` 可查看）。NSH 用 `--device <serial>` 或 `ANDROID_SERIAL` 指定的设备，否则用第一台；连了多台时会告诉你用的是哪台。

`nsh` 安装 Rust 应用自己的 `libnsh.so`，或开发宿主 APK（只有运行时变了才重建）。开发服务器端口通过 `adb reverse` 转发，插 USB 就能用，不需要同一网络。

#### macOS（AppKit）

```sh
nsh run my-app --target macos
```

应用以 Mac 应用的形式在可调整大小的 AppKit 窗口中打开，用鼠标、触控板和键盘操作。需要 macOS 14 及以上，见 [macOS](./macos.md)。

#### Windows（Windows 上默认）

```sh
nsh run my-app
```

应用在 1200×800 点的窗口中打开，支持鼠标、触控板、触摸和键盘。需要 Windows 10 1809 及以上，见 [Windows](./windows.md)。

#### Linux（Linux 上默认）

```sh
nsh run my-app
```

应用在 390×844 点的 GTK 窗口中打开。需要 GTK 4.14 及以上，见 [Linux](./linux.md)。

#### Web

```sh
nsh run my-app --target web
```

`nsh` 会打印页面地址，用浏览器打开即可。Rust 应用以 WebAssembly 运行，改动后页面会自动加载新构建。见 [Web](./web.md)。

### 编辑

保持 `nsh run` 运行，修改按钮的文字：

```tsx tabs
<Text style={{ color: "white", fontSize: 17 }}>Add one</Text>
```

```rust tabs
text("Add one")
    .text_size(px(17.0))
    .text_color(move || theme().primary_foreground)
```

保存。TSX 应用会收到 CLI 发来的补丁，宿主重跑改动的模块，只重新渲染代码变了的组件。这里变的是持有 `count` 的 `App`，所以计数归零。Rust 应用会重新构建并重启，计数同样归零。

可以把一部分界面拆成单独的组件，或拆成返回元素的普通函数：

```tsx tabs
function TapButton(props: { onPress: () => void }) {
  return (
    <View onPress={props.onPress} activeStyle={{ opacity: 0.6 }} style={{ padding: 12, borderRadius: 10, backgroundColor: "#0969da" }}>
      <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
    </View>
  );
}

// 在 App 中：<TapButton onPress={() => count.update((c) => c + 1)} />
```

```rust tabs
/// 应用的按钮：一个响应按压的视图。
fn tap_button(on_press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .p_3()
        .rounded(px(10.0))
        .bg(move || theme().primary)
        .active(|s| s.opacity(0.6))
        .on_press(on_press)
        .child(text("Tap").text_size(px(17.0)).text_color(move || theme().primary_foreground))
}

// 在 App 中：.child(tap_button(move || count.update(|value| *value += 1)))
```

在 TSX 中，改动不持有状态的组件时，状态会保留。先点几下，再改 `TapButton` 的颜色：按钮更新了，计数不变。

热替换的规则：

- 以顶层组件为单位，即名称首字母大写、函数体里有 JSX 的函数。
- 组件自身代码变了，或模块里组件之外的代码（它用到的常量、辅助函数）变了，组件就重新渲染。
- 无法原地应用的改动会让宿主重新加载整个应用，例如导入了运行中的应用还没导入过的 NSH 模块，或修改了 `nsh.json` 的 capabilities。

#### 错误

| 错误 | 显示在哪里 |
|---|---|
| Rust 编译错误 | 终端，附 cargo 输出。下次保存时 `nsh run` 重新构建。 |
| TSX 构建错误 | 终端，以及设备上的红色浮层，标出文件、行号和列号。应用继续运行上一次成功的构建。下次构建成功或点击浮层后关闭。 |
| 运行时异常 | 同样的浮层，调用栈映射回你的 `.tsx` 源码。 |

### 开发工具

#### Mac Catalyst

```sh
nsh run my-app --target catalyst
```

应用以 iOS（UIKit）后端在 Mac 上一个 390×844 点的窗口中打开。只在检查 Catalyst 特有行为，或在 Mac Catalyst 应用里嵌入 NSH 时使用。做 Mac 应用用默认的 `macos`，看手机效果用 `--target ios`。

## 承载到你的应用里

原生应用通过一个视图显示 NSH：

| 平台 | 视图 |
|---|---|
| iOS | `NshAppView`（一个 `UIView`） |
| Android | `NshView`（一个 Android `View`） |
| macOS、Windows、Linux、GPUI | `AppView` |

它和普通视图一样摆放，任何页面都能放，数量不限。

Rust 根组件直接编进应用：应用链接你的 crate，在 macOS、Windows、Linux、GPUI 和 iOS 的 `AppView` 上用 `AppSource::Rust` 挂载（[用 Rust 开发](./rust.md#完全用-rust-编写的应用)）。

TSX 应用则是视图加载的模块：开发时来自开发服务器，发布后来自构建好的 bundle 或下载的签名包。下面几节讲的都是加载 TSX 模块。

### iOS

在 Debug 配置中链接 `NativeShellDev.xcframework`，在 Release 中链接 `NativeShell.xcframework`（[在 iOS 应用中嵌入](./ios.md)），然后：

```swift
import NativeShell

let app = NshAppView()
app.onError = { message in print("nsh error: \(message)") }
view.addSubview(app)   // 再像普通视图一样设置约束

#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)   // 在 Mac 上运行 `nsh dev my-app`
#else
try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "nshc", subdirectory: "nsh")!)
#endif
```

`nsh dev my-app` 向应用提供模块，每次保存都推送更新，和 `nsh run` 一样。发布时用 `nsh build my-app --release --out MyApp/nsh` 把 bundle 写到一个文件夹，再把文件夹加进应用资源。

### Android

调试版添加 `nsh-dev.aar`，发布版添加 `nsh.aar`（[在 Android 应用中嵌入](./android.md)），然后：

```kotlin
val nsh = NshView(this)
nsh.setErrorListener { message -> Log.e("nsh", message) }
setContentView(nsh)   // 或者加入任意布局

if (BuildConfig.DEBUG) {
    nsh.connectDevServer("ws://127.0.0.1:8790/ws")   // 配合 `adb reverse tcp:8790 tcp:8790`
} else {
    nsh.loadAssetBundle("nsh")   // assets/nsh，由 `nsh build my-app --release --out app/src/main/assets/nsh` 生成
}
```

### macOS、Windows 和 Linux

Rust 桌面应用用 `AppView` 嵌入 NSH，它来自 `nsh-platform-macos`、`nsh-platform-windows`、`nsh-platform-linux` 和 `nsh-platform-gpui`。它分别是 `NSView`、子窗口、GTK 控件或 GPUI 视图，可挂载 Rust 根组件或加载构建好的 bundle。见
[macOS](./macos.md#嵌入-mac-应用)、[Windows](./windows.md#嵌入-win32-应用)、[Linux](./linux.md#嵌入-gtk-应用) 和
[GPUI](./gpui.md)；各平台宿主的对照见[平台](./platforms.md#嵌入原生应用)。

### 模块能做什么

在 iOS 和 Android 上，宿主在加载前决定模块的权限：`grantCapabilities` 指定它能访问的网络主机和其他能力，限额控制内存、调用时长和节点数（[沙箱与能力](./sandbox-and-capabilities.md)）。

想让模块用上你自己的原生视图（地图、图表、设计系统组件），把它们注册为[原生组件](./native-components.md)。

## 以签名包下发更新

应用上线后下载的 TSX 模块都是签名包。先生成根密钥并离线保管，用它认证发布密钥，然后发布：

```sh
nsh keygen keys/root && nsh keygen keys/release
nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
nsh publish my-app --key keys/release.secret --cert keys/release.cert --build 2
```

应用信任根公钥后加载下载的包。NSH 在执行任何代码前校验签名、证书和吊销列表：

```swift
let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
let trust = try NshTrust(stateDirectory: support.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)   // keys/root.pub 的内容
let info = try await app.loadPackage(at: downloadedFile, trust: trust)
```

Android 流程相同，用 `NshTrust` 和 `NshPackage`。托管、下发和吊销详见[签名包](./signed-packages.md)。

## 或者作为独立应用发布

`nsh package` 把 TSX 或 Rust 应用打成安装包，不需要自己的 Xcode 工程或 Gradle 构建：

```sh
nsh package my-app --target ios-simulator --run
nsh package my-app --target android
nsh package my-app --target macos
nsh package my-app --target linux
nsh package my-app --target windows
nsh package my-app --target web
```

TSX 应用用预编译的运行时；`ext/` 里有 Rust 时，用连同这些 Rust 一起构建的运行时。Rust 应用用自己的运行器，由你的 crate 和 NSH 的 crate 构建。选项、签名和图标见[打包](./packaging.md)。

## 在终端中试运行

`nsh test` 无界面运行应用，不需要设备或窗口。传入 TSX 入口或 Rust 应用的 `nsh.json`，它会打开应用并打印挂载的内容。加 `--trace` 还会打印原生视图操作和每次提交的耗时（[测试](./testing-and-trace.md)）：

```sh
nsh test my-app/App.tsx --trace      # TSX 应用的入口
nsh test my-app/nsh.json --trace     # Rust 应用的根组件
```

传入目录则运行应用的测试：`nsh test my-app`。每个模板的 `App.test.tsx` 都会打开应用、按下按钮并检查结果。Rust 应用的测试用 `open(".")` 打开根组件；也可以在 `cargo test` 里用 `nsh_testing::Headless` 测试（[用 Rust 开发](./rust.md#测试)）。

## 下一步

- [JSX 与组件](./jsx-and-components.md)和[响应式](./reactivity.md)：TSX 与 Rust 中的组件和 signal。
- [nshm](./nshm.md)：在 TSX 应用中加入 Rust，放在 `ext/` 里或来自 crates.io，以及使用已发布的 nshm。
- [用 Rust 开发](./rust.md)：从头到尾用 Rust 编写的应用、它的主题，以及在 Rust crate 之上使用 TSX。
- [平台](./platforms.md)：每个平台支持什么，以及各平台自己的页面（[iOS](./ios.md)、[Android](./android.md)、[macOS](./macos.md)、[Windows](./windows.md)、[Linux](./linux.md)、[Web](./web.md)、[TUI](./tui.md)）。
- [签名包](./signed-packages.md)和[沙箱与能力](./sandbox-and-capabilities.md)：下发方式，以及模块能做什么。
- [项目结构](./project-structure.md)：`nsh.json`、资源和文件。

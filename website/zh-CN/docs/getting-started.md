---
title: 快速开始
description: 用 nsh init 编写一个模块，借助热重载在 iOS、Android、macOS、Windows 或 Linux 上运行，把它承载到你的应用里，并以签名包下发更新；也可以打包成独立应用。
order: 4
group: start
demo: guide-getting-started
---

# 快速开始

NSH 的主要用途是在原生应用里承载模块（见[简介](./index.md)）。本指南沿着这条路线走：编写一个模块，在你面向的平台上借助热重载开发，在你的应用里显示它，再以签名包下发一次更新。前提是已经安装 `nsh` 并加入 `PATH`（[安装](./installation.md)）；在源码仓库中请改用 `./target/debug/nsh`。

> [!TIP]
> 手头没有应用工程？前两节都不需要：`nsh run` 会在 NSH 自带的开发宿主里显示模块：你电脑上的一个窗口、iOS 模拟器或 Android 设备。你可以到此为止，或者把它[打包成独立应用](#或者作为独立应用发布)。

## 创建模块

```sh
nsh init my-app
```

模块就是宿主应用加载的东西：一个或多个带入口的 `.tsx` 文件。`nsh init` 会在目录不存在时创建它，并写入：

| 文件 | 作用 |
|---|---|
| `App.tsx` | 入口模块：用 `mount` 渲染应用 |
| `nsh.json` | 应用的名称、id、版本和构建号（[项目结构](./project-structure.md#nshjson)） |
| `tsconfig.json` | TypeScript 设置，把 `nsh` 解析到 `.nsh/types` 中的声明 |
| `.gitignore` | 忽略 `.nsh/` 和 `out/` |
| `.nsh/types/` | 生成的所有 NSH 模块的声明，供编辑器使用 |

显示名称默认是目录名；可以用 `--name "My App"` 指定其他名称。id 会是 `com.example.<name>`：发布前请在 `nsh.json` 中修改。已存在的文件会被保留，所以你可以在已经有部分文件的目录里运行 `nsh init`。

### 入口模块

```tsx
import { mount, signal, Text, View } from "nsh";
import { SafeAreaView } from "nsh/ui/mobile";

function App() {
  const count = signal(0);
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <Text style={{ fontSize: 28, fontWeight: "600" }}>{count()}</Text>
      <View
        onPress={() => count.update((c) => c + 1)}
        activeOpacity={0.6}
        accessibilityRole="button"
        style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10, backgroundColor: "#0969da" }}
      >
        <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
      </View>
    </SafeAreaView>
  );
}

mount(App);
```

逐行来看：

- **`import { mount, signal, Text, View } from "nsh"`**：`nsh` 是核心模块，提供 [signals](./reactivity.md)、`mount`、其他响应式原语，以及宿主元素 `View` 和 `Text`。它由原生运行时提供，不会被打包。
- **`import { SafeAreaView } from "nsh/ui/mobile"`**：移动端 UI 组件集。[`SafeAreaView`](../components/safe-area-view.md) 会为内容加上内边距，使其避开刘海、状态栏和 Home 指示条；桌面上安全区域为 0，它不会加内边距。
- **`function App()`** 是一个组件：返回 JSX 的函数。它只运行**一次**。`signal(0)` 创建一个会随时间变化的值。
- **`{count()}`** 在 JSX 中读取 signal。编译器看到这是一次调用，就让这段文本跟随该 signal：`count` 变化时，运行时只更新这一段文本，其他什么都不动。
- **`<View onPress>`** 是一个响应按压的原生视图；`activeOpacity` 让它在按下时以原生方式变淡。`View`、`Text`、`Image`、`Scroll`、`Input` 等[宿主元素](../elements/index.md)和其他组件一样从 NSH 的模块导入；只有 `If` 和 `Each` 无需导入。
- **`style={{ … }}`** 接受由 camelCase 样式属性组成的对象，数字的单位是点。布局是 flexbox，子元素默认按列排列（[布局](./layout.md)）。
- **`mount(App)`** 把组件渲染到应用的界面上。

> [!IMPORTANT]
> 文本必须放在 `<Text>` 中。直接写在 `<View>` 里的字符串（`<View>Hello</View>`）不会显示。

## 开发

### 运行

`nsh run` 会启动开发服务器、构建应用、启动加载该应用的开发宿主，并在你编辑时持续重新构建。应用的日志（`console.log`、错误）会显示在终端中。

不传 `--target` 时，`nsh run` 使用你所在的机器：Windows 上是一个 Win32 窗口，Linux 上是一个 GTK 窗口，macOS 上是一个以 iPhone 的样子显示应用的 Mac Catalyst 窗口。用 `--target` 选择其他平台：

| 目标 | 命令 | 在哪里运行 |
|---|---|---|
| iOS 模拟器 | `nsh run my-app/App.tsx --target ios` | 装有 Xcode 的 macOS |
| Android | `nsh run my-app/App.tsx --target android` | macOS、Windows、Linux |
| macOS（AppKit） | `nsh run my-app/App.tsx --target macos` | macOS |
| Windows | `nsh run my-app/App.tsx` | Windows（默认） |
| Linux（GTK 4） | `nsh run my-app/App.tsx` | Linux（默认） |
| Mac Catalyst | `nsh run my-app/App.tsx` | macOS（默认） |

#### iOS 模拟器

```sh
nsh run my-app/App.tsx --target ios
nsh run my-app/App.tsx --target ios --device "iPhone 16 Pro"
```

`nsh` 会使用已启动的模拟器；如果没有正在运行的模拟器，就启动最新的 iPhone，并把开发宿主安装进去。`--device` 按名称或 UDID 选择模拟器。

#### Android

```sh
nsh run my-app/App.tsx --target android
```

先启动模拟器或连接设备（`adb devices` 会列出它们；NSH 使用第一个）。`nsh` 会安装一个开发宿主 APK（只在运行时变化时重新构建），并通过 `adb reverse` 把开发服务器的端口转发到设备上，因此通过 USB 即可工作，无需共享网络。

#### macOS（AppKit）

```sh
nsh run my-app/App.tsx --target macos
```

应用会作为桌面 Mac 应用打开（AppKit，窗口可调整大小），使用鼠标、触控板和键盘输入而不是触摸。需要 macOS 14 及以上；见
[macOS](./macos.md)。

#### Windows（Windows 上默认）

```sh
nsh run my-app/App.tsx
```

应用会在一个桌面窗口（1200×800 点）中打开，使用鼠标、触控板、触摸和键盘输入。需要 Windows 10 1809 及以上；见
[Windows](./windows.md)。

#### Linux（Linux 上默认）

```sh
nsh run my-app/App.tsx
```

应用会在一个 GTK 窗口（390×844 点）中打开。需要 GTK 4.14 及以上；见 [Linux](./linux.md)。

#### Mac Catalyst（macOS 上默认）

```sh
nsh run my-app/App.tsx
```

应用会在 Mac 上一个 iPhone 尺寸（390×844 点）的窗口中打开。这是查看模块在手机上效果最快的方式：不需要启动模拟器。它和 iOS 一样使用 UIKit，所以看到的效果就是 iPhone 上的效果，只有 Mac 特有的行为除外，例如用鼠标滚轮滚动。要运行 Mac 应用本身，请用 `--target macos`。

### 用热重载编辑

保持 `nsh run` 运行，修改 `App.tsx` 中的按钮文字：

```tsx
<Text style={{ color: "white", fontSize: 17 }}>Add one</Text>
```

保存。CLI 重新构建并发送补丁；宿主重新运行变化的模块，并只重新渲染代码有变化的组件。这里变化的是 `App`，它持有 `count` signal，所以计数会归零。当你编辑的组件并不持有状态时，状态会被保留。把按钮移到单独的组件里：

```tsx
function TapButton(props: { onPress: () => void }) {
  return (
    <View onPress={props.onPress} activeOpacity={0.6} style={{ padding: 12, borderRadius: 10, backgroundColor: "#0969da" }}>
      <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
    </View>
  );
}
```

在 `App` 中使用 `<TapButton onPress={() => count.update((c) => c + 1)} />`，点几下，然后修改 `TapButton` 的颜色：按钮会更新，计数保持不变。

热替换以顶层组件为单位：名称首字母大写、函数体中包含 JSX 的函数。组件在自身代码变化时，或模块中组件之外的代码（它用到的常量、辅助函数）变化时重新渲染。如果某次编辑无法原地应用（例如导入了运行中的应用尚未导入的 NSH 模块，或修改了 `nsh.json` 中的 capabilities），宿主会重新加载整个应用。

#### 错误

构建错误会打印在终端中，并在设备上以红色浮层显示文件、行号和列号；应用继续运行上一次成功的构建。下次构建成功后浮层会消失（也可以点击提前关闭）。运行时抛出的异常以同样方式显示，调用栈会映射回你的 `.tsx` 源码。

## 承载到你的应用里

应用用一个专门的视图显示模块：iOS 上是 `NshAppView`（一个 `UIView`），Android 上是 `NshView`（一个 Android `View`），macOS、Windows 和 Linux 上是 `AppView`。像其他视图一样摆放即可，可以放在任何页面，数量不限。开发时让它连接开发服务器；发布时加载构建好的 bundle 或下载的包。

### iOS

在 Debug 配置中链接 `NativeShellDev.xcframework`，在 Release 中链接 `NativeShell.xcframework`（[在 iOS 应用中嵌入](./embedding-ios.md)），然后：

```swift
import NativeShell

let app = NshAppView()
app.onError = { message in print("nsh error: \(message)") }
view.addSubview(app)   // 再像普通视图一样设置约束

#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)   // 在 Mac 上运行 `nsh dev my-app`
#else
try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "qbc", subdirectory: "nsh")!)
#endif
```

`nsh dev my-app/App.tsx` 把模块提供给应用，并在每次保存后推送更新，就像 `nsh run` 对它自带的宿主所做的那样。发布版用 `nsh build my-app/App.tsx --release --out MyApp/nsh` 把 bundle 写进一个文件夹，再把它加入应用的资源。

### Android

调试版添加 `nsh-dev.aar`，发布版添加 `nsh.aar`（[在 Android 应用中嵌入](./embedding-android.md)），然后：

```kotlin
val nsh = NshView(this)
nsh.setErrorListener { message -> Log.e("nsh", message) }
setContentView(nsh)   // 或者加入任意布局

if (BuildConfig.DEBUG) {
    nsh.connectDevServer("ws://127.0.0.1:8790/ws")   // 配合 `adb reverse tcp:8790 tcp:8790`
} else {
    nsh.loadAssetBundle("nsh")   // assets/nsh，由 `nsh build my-app/App.tsx --release --out app/src/main/assets/nsh` 生成
}
```

### macOS、Windows 和 Linux

用 Rust 编写的桌面应用通过 `AppView` 嵌入模块，它来自 `nsh-platform-macos`、`nsh-platform-windows`、`nsh-platform-linux` 和 `nsh-platform-gpui` 这几个 crate：一个 `NSView`、一个子窗口、一个 GTK 控件或一个 GPUI 视图，加载构建好的 bundle。见 [macOS](./macos.md#嵌入-mac-应用)、[Windows](./windows.md#嵌入-win32-应用)、[Linux](./linux.md#嵌入-gtk-应用) 和 [GPUI](./gpui.md)；各平台宿主的对照见[平台](./platforms.md#嵌入原生应用)。

### 模块能做什么

在 iOS 和 Android 上，由宿主在加载之前决定模块能做什么：`grantCapabilities` 设置它能访问的网络主机和其他能力，各项限制约束它的内存、调用时长和节点数（[沙箱与能力](./sandbox-and-capabilities.md)）。要让模块使用你自己的原生视图（地图、图表、设计系统组件），把它们注册为[原生组件](./native-components.md)。

## 以签名包下发更新

应用在发布之后下载的模块，以签名包的形式提供。先创建一个根密钥并离线保管，用它认证一个发布密钥，然后发布：

```sh
nsh keygen keys/root && nsh keygen keys/release
nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
nsh publish my-app/App.tsx --key keys/release.secret --cert keys/release.cert --build 2
```

应用信任根公钥，并加载下载到的包；NSH 在运行任何代码之前校验签名、证书和吊销列表：

```swift
let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
let trust = try NshTrust(stateDirectory: support.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)   // keys/root.pub 的内容
let info = try await app.loadPackage(at: downloadedFile, trust: trust)
```

Android 上用 `NshTrust` 和 `NshPackage`，流程相同。承载、下发和吊销的细节见[签名包](./signed-packages.md)。

## 或者作为独立应用发布

同一个模块也可以组成一个完整的应用。`nsh package` 用预编译的运行时把它打成可安装的应用，不需要你自己的 Xcode 工程或 Gradle 构建：

```sh
nsh package my-app --target ios-simulator --run
nsh package my-app --target android
nsh package my-app --target macos
nsh package my-app --target linux
nsh package my-app --target windows
```

这在每个平台上都是受支持的用法，也是把模块装到设备或桌面上最快的方式，但 NSH 的设计出发点是在原生应用中承载模块。选项、签名和图标见[打包](./packaging.md)。

## 在终端中试运行

`nsh trace` 以无界面方式（不需要设备）运行应用，打印它产生的原生视图操作及耗时，然后点击第一个可按压的视图两次：

```sh
nsh trace my-app/App.tsx
```

这是检查应用能否正常启动、查看一次交互引起多少工作量的快捷方式（[测试与 trace](./testing-and-trace.md)）。

## 下一步

- [平台](./platforms.md)：每个平台支持什么，以及各平台自己的页面（[iOS](./embedding-ios.md)、[Android](./embedding-android.md)、[macOS](./macos.md)、[Windows](./windows.md)、[Linux](./linux.md)）。
- [签名包](./signed-packages.md)和[沙箱与能力](./sandbox-and-capabilities.md)：下发方式，以及模块能做什么。
- [JSX 与组件](./jsx-and-components.md)和[响应式](./reactivity.md)：编写模块。
- [项目结构](./project-structure.md)：`nsh.json`、资源和文件。

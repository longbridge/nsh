---
title: iOS
description: 在 iOS 上用 UIKit 视图运行 NSH 应用（模拟器或真机），打包成 .app 和 .ipa，并通过 NativeShell.xcframework 和 NshAppView 嵌入现有 iOS 应用。
order: 11
group: platforms
---

# iOS

NSH 应用在 iOS 上用 UIKit 视图和 CoreText 文本运行，支持真机和模拟器，要求 iOS 15.0 及以上。主要用途是嵌入：现有应用在
`NshAppView` 中显示模块，加载它下载的签名包，并按自己设定的限制和能力运行它们。也可以运行和打包完整应用，方式与其他平台相同。在 iOS
上，`Platform.OS` 是 `"ios"`，`ios={{…}}` 属性生效。

## 环境要求

- Mac 上的 **Xcode**，以及一个 iOS 模拟器运行时（`xcodebuild -downloadPlatform iOS` 可以安装）。见[安装](./installation.md#ios-与-mac-catalyst)。
- 应用需要 **iOS 15.0** 及以上，支持 iPhone 和 iPad。

## 运行应用

```sh
nsh run my-app --target ios
nsh run my-app --target ios --device "iPhone 16 Pro"
```

`nsh` 会使用已启动的模拟器，或者启动最新的 iPhone，把开发宿主安装进去，并在你保存时更新应用，日志显示在终端中。`--device`
按名称或 UDID 选择模拟器。想要不用模拟器、更快的开发循环，在 Mac 上直接运行 `nsh run`，会在一个 iPhone 尺寸的
[Mac Catalyst](./getting-started.md#mac-catalystmacos-上默认) 窗口中打开同一个 UIKit 后端。

## 打包

```sh
nsh package my-app --target ios-simulator --run      # out/<Name>.app，安装到模拟器并启动
nsh package my-app --target ios --sign "Apple Distribution: Example Inc (ABCDE12345)" --profile AppStore.mobileprovision
```

`--target ios` 为真机、TestFlight 或 App Store 生成签名的 `.ipa`。签名身份、描述文件和图标见[打包](./packaging.md#ios-真机)。

## 嵌入 iOS 应用

现有的 iOS 应用可以在自己的界面中显示 NSH 应用。运行时以 `NativeShell.xcframework` 的形式提供，这是一个带 Swift
模块的静态 framework：创建一个 `NshAppView`（一个 `UIView`），把它加到视图层级的任意位置，再往里加载一个应用即可。你的原生页面、导航和
NSH 应用共处同一个 UIKit 视图层级。

每个 `NshAppView` 都是一个独立的引擎，拥有自己的 JS VM；没有全局单例，所以一个应用可以同时显示多个 NSH 应用，比如每个标签页一个。

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/host-app-light.svg" alt="一个宿主应用里有两个 NshAppView 区域：每个都是独立的引擎，有自己的线程、虚拟机、限制和能力；宿主授予能力、设置限制，并注册地图这样的原生组件。">
<img class="diagram diagram-dark" src="/diagrams/host-app-dark.svg" alt="一个宿主应用里有两个 NshAppView 区域：每个都是独立的引擎，有自己的线程、虚拟机、限制和能力；宿主授予能力、设置限制，并注册地图这样的原生组件。">
</div>

## 获取 framework

发行包中包含两个版本：

```text
dist/nsh/embed/ios/NativeShell.xcframework      release: bundles and signed packages
dist/nsh/embed/ios/NativeShellDev.xcframework   also connects to a `nsh dev` server
```

如果你有 NSH 的源码，可以只构建这两个 framework：

```sh
cargo xtask ios-embed    # writes dist/nsh/embed/ios/{NativeShell,NativeShellDev}.xcframework
```

每个 XCFramework 包含三个 arm64 切片：iOS 真机、iOS 模拟器（Apple 芯片）和 Mac Catalyst。最低部署目标为 iOS 15.0。Swift
模块启用了 library evolution 编译，因此用其他版本 Swift 编译器构建的应用也能导入它；framework 已链接了它需要的一切，你无需再添加其他东西。

## 加入 Xcode 项目

1. 把 `NativeShell.xcframework`（开发构建则用 `NativeShellDev.xcframework`）拖到应用 target 的
   **Frameworks, Libraries, and Embedded Content** 中。它是静态 framework，所以选择 **Do Not Embed**。
2. 在 Swift 中 `import NativeShell`。

> [!TIP]
> 在 Debug 配置中链接 `NativeShellDev.xcframework`，在 Release 配置中链接 `NativeShell.xcframework`，这样只有开发构建能连接开发服务器。

## 随应用附带 bundle

把 NSH 应用构建到一个文件夹里，再以名为 `nsh` 的文件夹引用（folder reference）把它加入应用资源：

```sh
nsh build App.tsx --release --out MyApp/nsh
```

`nsh build` 会写出：

| 文件 | 内容 |
|---|---|
| `bundle.nshc` | 编译后的应用字节码：加载最快。 |
| `bundle.js` | 同一个应用的 JS 源码。 |
| `bundle.js.map` | 它的 source map（堆栈跟踪会指向你的 `.tsx` 文件）。 |
| `capabilities.json` | 应用声明的[能力](./sandbox-and-capabilities.md)，仅当 `nsh.json` 中有声明时生成。 |
| `assets/` | 应用导入的图片和字体，路径相对于入口文件。 |

## 显示应用

```swift
import NativeShell
import UIKit

final class EmbedController: UIViewController {
    private let app = NshAppView()

    override func viewDidLoad() {
        super.viewDidLoad()
        app.translatesAutoresizingMaskIntoConstraints = false
        app.onError = { message in print("nsh error: \(message)") }
        view.addSubview(app)
        NSLayoutConstraint.activate([
            app.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            app.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            app.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            app.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        guard let url = Bundle.main.url(forResource: "bundle", withExtension: "qbc", subdirectory: "nsh") else { return }
        do {
            try app.loadBundle(at: url)
        } catch {
            print("cannot load: \(error)")
        }
    }

    // Let the embedded app's StatusBar decide the status bar.
    override var preferredStatusBarStyle: UIStatusBarStyle { NshAppView.preferredStatusBarStyle }
    override var prefersStatusBarHidden: Bool { NshAppView.prefersStatusBarHidden }
    override var preferredStatusBarUpdateAnimation: UIStatusBarAnimation {
        NshAppView.preferredStatusBarUpdateAnimation
    }
}
```

视图的尺寸就是应用的窗口：NSH 按视图的 bounds 布局应用，bounds 变化时重新布局。再次加载会替换正在运行的应用；视图被释放时应用随之停止。

完整示例是 `examples/embed-ios/main.swift`：一个在自己的原生标题栏下方显示 NSH 应用的 UIKit 应用。

## `NshAppView`

`NshAppView` 是 `UIView` 的子类，隔离在 main actor 上。

| 成员 | 说明 |
|---|---|
| `init(frame: CGRect = .zero, trace: Bool = false)` | 创建视图。`trace` 会记录每次提交的耗时。`init?(coder:)` 支持 storyboard 和 nib。 |
| `onError: ((String) -> Void)?` | 在主线程上接收应用的致命错误（bundle 加载失败、未捕获的 JS 异常、VM 因触及限制而停止）。错误总会写入日志；开发构建还会把错误显示在应用上方。 |
| `limits: NshLimits` | 此后加载的应用的 VM 限制。 |
| `grantCapabilities(_ json: String?) throws` | 此后加载的应用最多能做什么，JSON 格式与 `nsh.json` 的 `capabilities` 相同；`nil` 表示授予每个应用所声明的能力。JSON 无效时抛出 `NshError`。 |
| `loadBundle(at url: URL) throws` | 运行 `nsh build` 写出的 bundle。资源取自同目录下的 `assets/`；同目录下的 `bundle.js.map` 和 `capabilities.json` 也会被读取。 |
| `run(_ package: NshPackage)` | 运行一个已验证的签名包（见下文）。一个包只能运行一次。 |
| `loadPackage(at:trust:) async throws -> NshPackage.Info` | 在主线程之外验证指定 URL 的包，然后运行它。被拒绝时抛出原因；正在运行的应用保持不变。 |
| `connectDevServer(_ url: URL) throws` | 开发用：从 `nsh dev` 服务器加载，每次修改后重新加载。使用不含开发支持的 `NativeShell.xcframework` 时会抛出错误。 |
| `register(component:_:)` | 注册一个[原生组件](./native-components.md)。 |
| `static preferredStatusBarStyle`、`prefersStatusBarHidden`、`preferredStatusBarUpdateAnimation` | 正在运行的应用通过 `StatusBar` 请求的状态栏设置：在宿主视图控制器的对应重写中返回它们。 |

> [!IMPORTANT]
> `bundle.nshc`（字节码）只能从应用包内部加载。QuickJS 不校验字节码，其他来源的字节码可能破坏内存：对于应用包之外的文件，请加载
> `bundle.js`（JS 源码），更好的做法是使用[签名包](./signed-packages.md)。

`limits` 和 `grantCapabilities` 从下一次加载开始生效：请在 `loadBundle` / `run` 之前设置。

## 限制

```swift
app.limits = NshLimits(memoryMB: 128, callBudget: 1, maxNodes: 50_000)
```

| `NshLimits` 字段 | 默认值 | 含义 |
|---|---|---|
| `memoryMB: UInt32?` | 256 | JS 堆大小（MB）。耗尽时应用停止。 |
| `stackKB: UInt32?` | 4096（也是上限） | 解释器栈大小（KB）。递归更深会抛出 `RangeError`。 |
| `loadBudget: TimeInterval?` | 10 s | 执行 bundle 的时间。 |
| `callBudget: TimeInterval?` | 2 s | 其他每次 JS 调用（一个事件、一个定时器、一帧）的时间。超时的调用会被中断。 |
| `maxNodes: UInt32?` | 200 000 | 应用可保留的节点数。 |

`nil` 表示使用默认值。各项限制触发时会发生什么，见[沙箱与能力](./sandbox-and-capabilities.md)。

## 能力

应用在 `nsh.json` 中声明它可以访问什么；嵌入方应用可以授予更少的能力。一个请求必须同时被两者允许：

```swift
try app.grantCapabilities("""
{
  "network": { "hosts": ["api.example.com", "*.cdn.example.com"] },
  "webview": { "hosts": ["help.example.com"] }
}
""")
try app.loadBundle(at: url)
```

这里应用不能设置状态栏（没有 `statusBar`），即使它的 `nsh.json` 申请了也不行。详见[沙箱与能力](./sandbox-and-capabilities.md)。

## 签名包

要运行应用下载的代码（更新、第三方小程序），请使用[签名包](./signed-packages.md)：`nsh publish` 会写出一个
`.nshpkg`，由经你的根密钥认证的发布密钥签名，应用在运行前对它进行验证。

```swift
let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
let trust = try NshTrust(stateDirectory: support.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)   // the contents of root.pub from `nsh keygen`

Task {
    do {
        let info = try await app.loadPackage(at: downloadedFile, trust: trust)
        print("running \(info.id) \(info.version) (build \(info.build))")
    } catch {
        print("refused: \(error)")   // the running app stays
    }
}
```

如果想先验证、稍后再运行，可以把两步拆开：

```swift
let package = try await trust.openPackage(at: downloadedFile)   // verifies off the main thread
// ... decide, ask the user, ...
app.run(package)
```

| API | 说明 |
|---|---|
| `NshTrust(stateDirectory: URL) throws` | 打开或创建信任库。它的状态（最新的吊销列表、每个应用已接受的最高构建号）持久化在该目录中，包的资源也解压到其下。在调用 `addRoot` 之前不信任任何密钥。线程安全。 |
| `addRoot(_ publicKey: String) throws` | 信任一个根公钥（base64，即 `.pub` 文件的内容）。可以信任多个根。 |
| `updateRevocations(_ json: String) throws` | 应用 `nsh revoke` 写出的吊销列表；该列表须由受信任的根签名，且比信任库现有的更新。 |
| `openPackage(at: URL) async throws -> NshPackage` | 验证包、解压资源并接受该包（此后该应用更旧的构建都会被拒绝）。 |
| `NshPackage.info` | 包内应用的 `id`、`name`、`version`、`build`。 |
| `NshError` | 所有会抛出错误的调用抛出的错误类型；`message` 说明原因。 |

## 开发

使用 `NativeShellDev.xcframework` 时，可以让视图连接开发服务器，而不是加载 bundle：

```swift
#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)
#else
try app.loadBundle(at: bundledURL)
#endif
```

在 Mac 上运行 `nsh dev App.tsx`。模拟器与 Mac 共享网络，所以 `127.0.0.1` 就能访问服务器；真机则使用 `nsh dev`
打印的局域网地址（`devices on this network: ws://192.168.1.20:8790/ws`）。每次保存都会就地更新应用，日志显示在 `nsh dev`
终端中，错误显示在应用上方。参见[开发服务器与调试](./dev-server-and-debugging.md)。

> [!NOTE]
> 在真机上，与 Mac 的连接是普通的本地网络流量，使用明文的 `ws://` 和 `http://`（资源）。iOS 可能会请求用户授予本地网络访问权限，应用的
> App Transport Security 设置也必须允许这种连接。

## 注意事项

- **线程。** `NshAppView` 隔离在 main actor 上。`NshTrust`、`NshPackage` 和 `NshComponentContext` 是线程安全的。JS
  引擎运行在自己的线程上；主线程从不等待它。
- **状态栏。** 嵌入应用的 `StatusBar` 会改变 `NshAppView.preferredStatusBarStyle` 等属性的返回值；宿主视图控制器必须像上面的示例那样转发它们。
- **安全区域。** 应用照常使用 [`nsh/ui/mobile`](../components/safe-area-view.md) 中的 `SafeAreaView` 或
  `useSafeAreaInsets()`。放在你自己标题栏下方的 `NshAppView` 拿到的是适用于它自身的 insets，而不是整个窗口的。
- **Mac Catalyst。** 同一个 framework 也可用于 Catalyst 应用。

## 另请参阅

- [嵌入 Android 应用](./embedding-android.md)
- [原生组件](./native-components.md)
- [签名包](./signed-packages.md)
- [沙箱与能力](./sandbox-and-capabilities.md)

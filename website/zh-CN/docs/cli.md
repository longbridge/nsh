---
title: 命令行
description: NSH 的全部子命令与参数：init、build、trace、dev、run、package，以及签名相关的 keygen、certify、publish、verify 和 revoke。
order: 6
group: start
---

# 命令行

`nsh` 是 NSH 应用唯一需要的工具。所有构建应用的子命令都接受 `[ENTRY]`：入口模块，或应用目录（以其中的 `App.tsx` 为入口；默认为当前目录，所以 `cd my-app && nsh run` 即可）。命令会读取入口旁边的 `nsh.json`，并刷新 `.nsh/types` 中的类型声明。

```text
nsh <COMMAND>

  init     Create an app
  build    Bundle an app and compile it to QuickJS bytecode
  trace    Run an app headless and print the mount instructions and timings
  dev      Serve an app to dev hosts (simulators, devices) and push rebuilds
  run      Like dev, and also run the app in a development host
  package  Package an installable app from the prebuilt runtime
  keygen   Create a signing key
  certify  Certify a release key with a root key
  publish  Build an app in release mode and sign it into a package (.nshpkg)
  verify   Verify a package
  revoke   Sign a revocation list
```

`nsh <command> --help` 打印某个命令的参数；`nsh --version` 打印版本号。

## 输出

每个命令每个步骤打印一行，只在出错时打印细节：

```text
✓ built the Rust crate gallery-native (5.0s)
✓ built App.tsx (500 B js, 809 B bytecode, 21ms)
→ dev server ws://127.0.0.1:8790/ws
  devices on this network: ws://192.168.1.20:8790/ws
✓ built the development host for the iOS simulator (12.7s)
• iOS Simulator (iPhone 16 Plus) connected, build 1
warn iOS Simulator: image failed: An SSL error has occurred and a secure connection to the server cannot be made.
✓ rebuilt App.tsx (18ms), 1 device
```

`✓` 表示完成的步骤，`→` 是连接地址，`•` 是设备的动态；`warn` 和 `error` 行是问题，不带符号的行是应用自己的输出
（`console.log`）。在终端里，正在进行的步骤会显示一个转动的指示符，各行带颜色（设置了 `NO_COLOR` 时不带）；重定向到
文件时只写入结果行，不带颜色。NSH 调用的工具（应用 Rust crate 的 cargo 构建、开发宿主）出错时，它们的输出打印在
`error` 行下面。

`-v, --verbose`（所有命令都可用；也可以设置 `NSH_VERBOSE=1`）原样打印全部内容：每条日志及其级别和来源、cargo 的
输出以及宿主自己的输出。

## nsh init

```sh
nsh init [DIR] [--name <NAME>]
```

在 `DIR`（默认为当前目录，不存在时会创建）中创建应用：`App.tsx`、`nsh.json`、`tsconfig.json`、`.gitignore` 和 `.nsh/types`。已存在的文件会保留；只有当这些文件全部已存在时才会失败。

| 参数 | 说明 |
|---|---|
| `--name <NAME>` | 写入 `nsh.json` 的显示名称。默认为目录名。id 会是 `com.example.<name>`（只保留字母和数字，转为小写）。 |

## nsh run

```sh
nsh run [ENTRY] [--target catalyst|ios|android|macos|linux|windows|gpui] [--device <DEVICE>] [--port <PORT>] [--release] [--trace] [--fps]
```

启动开发服务器（同 `nsh dev`），启动一个从它加载应用的开发宿主，并在每次保存时重新构建。热模块替换会就地修补正在运行的应用；应用的日志和错误会打印在终端里。用 Ctrl-C 停止（或关闭 Catalyst、macOS、Linux、Windows 或 GPUI 窗口）。

| 参数 | 默认值 | 说明 |
|---|---|---|
| `--target <TARGET>` | macOS 上为 `catalyst`，Linux 上为 `linux`，Windows 上为 `windows` | `catalyst`：Mac Catalyst 宿主，一个 iPhone 大小的窗口（390×844）。`ios`：iOS 模拟器中的宿主。`android`：Android 设备或模拟器上的宿主。`macos`：AppKit 宿主，一个 1200×800 的窗口（[macOS](./macos.md)）。`linux`：GTK 宿主，一个 390×844 的窗口（[Linux](./linux.md)）。`windows`：Win32 宿主，一个 1200×800 的窗口（[Windows](./windows.md)）。`gpui`：GPUI 宿主，一个 1200×800 的窗口，可在 macOS、Windows 或 Linux 上运行（[GPUI](./gpui.md)）。 |
| `--device <DEVICE>` | | 要使用的 iOS 模拟器，按名称（`"iPhone 16 Pro"`）或 UDID 指定。默认：已启动的模拟器，否则启动最新的 iPhone。 |
| `--port <PORT>` | `8790` | 开发服务器端口。被占用时使用下一个空闲端口。 |
| `--release` | | 压缩代码并去掉调试信息。release 包没有热模块替换：每次保存都会重新加载应用。 |
| `--trace` | | 宿主把每次 commit 的耗时（JS、布局、挂载项）记录到终端。 |
| `--fps` | | 在应用上方显示性能监视器：帧率、每帧和每次 commit 的开销、CPU、GPU 和内存（[macOS](./macos.md#性能监视器)；目前仅限 `--target macos`）。 |

使用 `--target android` 时，会选用 `adb devices` 列出的第一台设备；开发服务器的端口通过 `adb reverse` 转发到设备上。参见[快速开始](./getting-started.md)和[开发服务器与调试](./dev-server-and-debugging.md)。

## nsh dev

```sh
nsh dev [ENTRY] [--port <PORT>] [--release]
```

只运行开发服务器：构建应用、监视入口所在目录，并把每次重新构建推送给连接到它的宿主。它会打印本机宿主使用的 URL，以及每个局域网地址对应的设备 URL：

```text
✓ built App.tsx (3.2 KB js, 9.8 KB bytecode, 21ms)
→ dev server ws://127.0.0.1:8790/ws
  devices on this network: ws://192.168.1.20:8790/ws
```

适合配合你自己启动的宿主使用，例如嵌入了开发版 SDK（`NativeShellDev.xcframework`、`nsh-dev.aar`）的应用。参数与 `nsh run` 相同。

## nsh trace

```sh
nsh trace [ENTRY] [--taps <N>] [--idle-ms <MS>] [--scroll <POINTS>] [--width <W>] [--height <H>]
```

以无界面方式运行应用：在 CLI 自己的进程里，使用与设备上相同的 core 和运行时，只是没有视图；它会打印每条挂载指令（create、insert、style、frame、text）和每次 commit 的耗时。之后它会点击第一个可按压的视图，并打印每次点击带来的变化。可以用它检查应用能否无错误地启动、统计一次交互引起的工作量，也可以在 CI 中使用（[测试与 trace](./testing-and-trace.md)）。

```text
✓ bundled App.tsx (3255 B in 12.6 ms)
perf commit: 56 ops, 412µs (list js 6µs), 49 mount items, 21 nodes
── initial render: 49 items
   Create { id: 2, kind: View }
   ...
── tap #1 on view 20: 3 items
   Text { id: 14, text: "1", content: 71x77 @ 0,0 }
```

| 参数 | 默认值 | 说明 |
|---|---|---|
| `--taps <N>` | `2` | 发送给第一个可按压视图的点击次数。没有引起任何变化的点击视为错误（退出码 1）。 |
| `--idle-ms <MS>` | `1200` | 点击前的空闲时间。默认值让引擎的空闲垃圾回收先运行，从而暴露对象保留类的 bug。 |
| `--scroll <POINTS>` | | 让第一个滚动视图或列表滚动这么多点，每帧 10 点，并打印每一帧的批次。 |
| `--width <W>` | `390` | 视口宽度，单位为点。 |
| `--height <H>` | `844` | 视口高度，单位为点。 |

`nsh trace` 按 iOS 运行：`Platform.OS` 为 `"ios"`，`ios:` 类和 `ios={{…}}` 属性都会生效。

## nsh build

```sh
nsh build [ENTRY] [-o <OUT>] [--release]
```

打包应用并编译为 QuickJS 字节码，供嵌入 NSH、自行分发 bundle 的应用使用。

| 参数 | 默认值 | 说明 |
|---|---|---|
| `-o, --out <OUT>` | `out` | 输出目录。 |
| `--release` | | 压缩代码并去掉调试信息。 |

输出内容：

| 文件 | 内容 |
|---|---|
| `bundle.js` | JavaScript 形式的 bundle |
| `bundle.js.map` | 它的 source map（堆栈会映射回你的源码） |
| `bundle.nshc` | 编译后的字节码 bundle：应用实际加载的就是它 |
| `capabilities.json` | `nsh.json` 中声明的 `capabilities`（未声明时删除） |
| `assets/` | 所有被导入的图片和字体，保持相对于应用根目录的路径 |

## nsh package

```sh
nsh package [ENTRY] --target ios|ios-simulator|android|macos|linux|windows|gpui [-o <OUT>] [flags]
```

用预构建的运行时以 release 模式构建可安装的应用。整个过程不涉及 Rust，也不需要 Xcode 工程或 Gradle 构建：NSH 自己组装应用包，并使用 Xcode 的 `actool` 和 `codesign`（iOS）或 SDK 中的 `aapt2`、`zipalign` 和 `apksigner`（Android）；macOS 则用 `codesign` 组装一个 ad hoc 签名的 `.app`（图标用 `sips` 转换）；Linux 则复制运行器并用 `tar` 生成 `.tar.gz`；Windows 则复制运行器、嵌入图标并生成 `.zip`，不需要任何工具；GPUI 则按本机操作系统的方式打包，只是换成 GPUI 运行器。元数据来自 `nsh.json`，可被参数覆盖。

| 参数 | 说明 |
|---|---|
| `--target <TARGET>` | 必填。`ios`：iOS 设备，签名的 `.ipa`（不带 `--sign` 时为未签名的 `.app`）。`ios-simulator`：ad hoc 签名的 `.app`。`android`：签名的 `.apk`。`macos`：ad hoc 签名的 `.app`（[macOS](./macos.md#打包)）。`linux`：一个目录 `<Name>/`（可执行文件、`nsh/`、`.desktop` 文件）和 `<Name>.tar.gz`（[Linux](./linux.md#打包)）。`windows`：一个目录 `<Name>\`（`<Name>.exe`、`nsh\`）和 `<Name>.zip`（[Windows](./windows.md#打包)）。`gpui`：本机上 `macos`、`windows` 或 `linux` 生成的产物，但使用 GPUI 运行器（[GPUI](./gpui.md#打包)）。 |
| `-o, --out <OUT>` | 输出目录（默认 `out`）。 |
| `--name <NAME>` | 显示名称。 |
| `--id <ID>` | Bundle identifier，例如 `com.example.app`。 |
| `--app-version <VERSION>` | 版本号，例如 `1.2.0`。 |
| `--build <BUILD>` | 构建号。 |
| `--icon <ICON>` | 应用图标：正方形 PNG，最好是 1024×1024。 |
| `--sign <IDENTITY>` | iOS 设备：代码签名身份，例如 `"Apple Development: …"`。 |
| `--profile <FILE>` | iOS 设备：描述文件（`.mobileprovision`），entitlements 取自其中。 |
| `--keystore <FILE>` | Android：release keystore。默认使用 `~/.android/debug.keystore` 中的调试密钥（不存在时创建）。 |
| `--key-alias <ALIAS>` | Android：keystore 中的密钥别名。 |
| `--keystore-password <PASSWORD>` | Android：keystore 密码，也可从 `NSH_KEYSTORE_PASSWORD` 读取。 |
| `--run` | 打包完成后安装并启动应用：在模拟器中（`ios-simulator`）或在已连接的 Android 设备上；`macos`、`linux`、`windows` 和 `gpui` 则直接启动它。 |

```sh
nsh package App.tsx --target ios-simulator --run
nsh package App.tsx --target ios --sign "Apple Development: Jane Doe (ABCDE12345)" --profile dev.mobileprovision
nsh package App.tsx --target android --keystore release.jks --key-alias upload
nsh package App.tsx --target macos
nsh package App.tsx --target linux
nsh package App.tsx --target windows
nsh package App.tsx --target gpui
```

参见[打包](./packaging.md)。

## 签名命令

这些命令用于生成和校验签名包（`.nshpkg`）：嵌入 NSH 的应用会下载这些应用包，并在运行前进行校验。[签名包](./signed-packages.md)介绍了整个模型（由离线保存的根密钥为某些应用 id 认证 release 密钥）。

### nsh keygen

```sh
nsh keygen <NAME>
```

创建一对 Ed25519 密钥：`<NAME>.secret` 和 `<NAME>.pub`（例如 `nsh keygen keys/root`）。根密钥的 `.secret` 应离线保存；应用信任的是它的 `.pub`。

### nsh certify

```sh
nsh certify --root <ROOT.secret> --key <KEY.pub> --apps <PATTERN>... [--days <DAYS>] -o <OUT>
```

用根密钥为 release 密钥签发证书，限定可签名的应用 id 和有效时间段；签名包会携带该证书。

| 参数 | 说明 |
|---|---|
| `--root <ROOT>` | 根密钥的私钥。 |
| `--key <KEY>` | release 密钥的公钥。 |
| `--apps <APPS>` | 允许签名的应用 id：`*`、`com.example.*` 或具体 id。可重复。 |
| `--days <DAYS>` | 从现在起可签名的天数（默认 365）。 |
| `-o, --out <OUT>` | 要写入的证书。 |

### nsh publish

```sh
nsh publish [ENTRY] --key <KEY.secret> --cert <CERT> [-o <OUT>] [--id <ID>] [--app-version <V>] [--build <N>]
```

以 release 模式构建应用并签名为一个包。元数据来自 `nsh.json`，可被参数覆盖。

| 参数 | 说明 |
|---|---|
| `--key <KEY>` | release 密钥的私钥。 |
| `--cert <CERT>` | 它的证书（`nsh certify`）。 |
| `-o, --out <OUT>` | 要写入的包。默认：`out/<id>-<build>.nshpkg`。 |
| `--id <ID>` | 应用 id。 |
| `--app-version <VERSION>` | 版本号。 |
| `--build <BUILD>` | 构建号：每次发布都必须递增。 |

### nsh verify

```sh
nsh verify <PACKAGE> [--root <ROOT.pub>]... [--revocations <LIST>]
```

按信任给定根密钥的应用的方式校验一个包。不带 `--root` 时只检查包自身的签名。它会打印包的 SHA-256，`nsh revoke --package` 接受的就是这个值。

### nsh revoke

```sh
nsh revoke --root <ROOT.secret> [--extend <LIST>] [--sequence <N>] [--key <ID>]... [--package <SHA256>]... -o <OUT>
```

签发一份吊销列表：列出不允许再加载的 release 密钥和包。

| 参数 | 说明 |
|---|---|
| `--root <ROOT>` | 根密钥的私钥。 |
| `--extend <EXTEND>` | 保留这份列表（最近发布的那份）中的条目。 |
| `--sequence <N>` | 列表编号，必须大于此前发布过的任何编号。配合 `--extend` 时默认为那份列表的编号加一。 |
| `--key <ID>` | 要吊销的 release 密钥 id。可重复。 |
| `--package <SHA256>` | 要吊销的包的 SHA-256。可重复。 |
| `-o, --out <OUT>` | 要写入的列表。 |

## 环境变量

| 变量 | 使用者 | 说明 |
|---|---|---|
| `ANDROID_HOME`、`ANDROID_SDK_ROOT` | Android 目标 | Android SDK 位置。后备路径：`~/Library/Android/sdk`。 |
| `NSH_KEYSTORE_PASSWORD` | `package --target android` | keystore 密码。 |
| `NSH_RUNTIME` | `run`、`package` | 用来替代 CLI 旁边运行时的运行时目录。 |
| `NSH_VERBOSE` | 所有命令 | `1`：等同于 `--verbose`。 |
| `NO_COLOR` | 所有命令 | 设置后，终端输出不带颜色。 |

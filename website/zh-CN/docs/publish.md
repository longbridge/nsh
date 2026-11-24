---
title: 发布
description: "NSH 应用如何从 nsh package 到达每个平台的用户：应用商店、安装程序、Web 托管、凭据、发布 CI，以及各平台目前支持的内容。"
order: 18.5
group: publishing
---

# 发布

发布是应用能跑起来之后的所有事：构建 release 版本、签名、上架应用商店或放到下载页，并持续更新。[打包](./packaging.md)讲的是第一步，即各平台的 `nsh package`；本页讲剩下的部分，每个平台一节：

- [iOS](#publish-to-the-app-store)：App Store Connect 和 TestFlight
- [Android](#publish-to-google-play)：Google Play
- [macOS](#publish-for-macos)：Developer ID、公证和直接下载
- [Windows](#publish-for-windows)：安装程序、代码签名和直接下载
- [Linux](#publish-for-linux)：归档、桌面条目和直接下载
- [Web](#publish-on-the-web)：放在任意主机上的静态站点

## 各平台支持什么

| 平台 | `nsh package` 写出什么 | 发布到哪里 | `nsh publish` 做什么 |
|---|---|---|---|
| iOS | 已签名的 `.ipa` | App Store Connect：先 TestFlight，再 App Store | 上传 IPA 供 TestFlight 使用（`nsh publish ios`） |
| Android | 已签名的 `.aab`（或 `.apk`） | Google Play | 上传 bundle 并发布到内部测试轨道（`nsh publish android`） |
| macOS | 已签名的 `.app` | 直接下载，用 Apple 的工具公证 | 暂不支持：自己公证并上传 |
| Windows | 目录、`.zip` 和 Inno Setup 安装程序 | 直接下载 | 暂不支持：自己签名并上传 |
| Linux | 目录和带桌面条目的 `.tar.gz` | 直接下载 | 暂不支持：自己上传 |
| Web | 静态站点 | 任意 Web 服务器或 CDN | 无需处理：把目录复制到主机上 |

表中写“自己”的步骤不在 NSH 内完成，各平台页面给出了所需的命令。应用商店的审核提交、分阶段发布和正式版发布都在各商店的控制台中完成，没有 `nsh` 命令负责这些。

除了应用商店，还有三种方式能给已安装的应用交付更新，在所有原生平台上都可用：

| 交付内容 | 方式 | 页面 |
|---|---|---|
| 整个应用的新版本（macOS、Windows、Linux） | `nsh/updater`，从 GitHub Releases 或你自己的主机 | [应用更新](./app-updates.md) |
| 已安装应用的新 TSX，无需发布商店版本 | 由应用下载的签名包 | [OTA 更新](./ota-updates.md) |
| 嵌入其他应用中的 NSH 界面 | 由宿主应用验证的签名包 | [签名包](./signed-packages.md) |

不带平台的 `nsh publish <ENTRY> --key … --cert …` 生成的就是这种签名包；`nsh publish ios` 和 `nsh publish android` 则是本组页面介绍的商店上传。

## 一次发布，逐步进行

1. **在 `nsh.json` 中设置版本。** `version` 是用户看到的版本号（`1.4.0`）；`build` 是一个整数，在每个平台上每次上传到商店都必须递增（Apple 平台上是 `CFBundleVersion`，Android 上是 `versionCode`）。两者也都可以在命令行上传入（`--app-version`、`--build`），在 CI 给构建编号时很方便。
2. **在各平台上打包。** iOS 和 macOS 需要装有 Xcode 的 Mac；Windows 包在 Windows 上构建（只有 Windows 能嵌入图标）；Linux 包在 Linux 上构建；Android 和 Web 在哪里都可以。见[打包](./packaging.md)。
3. **签名。** iOS、Android 和 macOS 只要提供签名身份或 keystore，就会在打包时完成签名；Windows 可执行文件在打包后用 `signtool` 签名。详情见各平台的页面。
4. **发布。** 上传到应用商店，或把文件附加到一个 release。
5. **后续跟进。** 处理、审核以及从测试转为正式版都在商店的控制台中进行。

```json
{
  "name": "Notes",
  "id": "com.example.notes",
  "version": "1.4.0",
  "build": 42,
  "icon": "icon.png"
}
```

`id` 在 Apple 平台上是 bundle identifier，在 Android 上是包名；商店中的应用记录必须使用同一个值。应用一旦以某个 id 上架，就不能再更改。

## 凭据

商店凭据（App Store Connect API 密钥、Google Play 服务账号密钥、签名 keystore 和证书、密码）永远不要放进 `nsh.json` 或仓库。`nsh` 从参数或环境变量读取它们，发布 CI 从自己的 secrets 中填入：

| 平台 | 需要什么 | `nsh` 如何获取 |
|---|---|---|
| iOS | Apple Distribution 证书和 App Store 描述文件 | `nsh package --sign … --profile …`；证书放在钥匙串中 |
| iOS | App Store Connect 团队 API 密钥：issuer ID、key ID、`.p8` 文件 | `NSH_APP_STORE_ISSUER_ID`、`NSH_APP_STORE_KEY_ID`、`NSH_APP_STORE_PRIVATE_KEY` |
| Android | 上传 keystore 及其密码 | `nsh package --keystore … --key-alias …`、`NSH_KEYSTORE_PASSWORD` |
| Android | Google Cloud 服务账号 JSON 密钥 | `NSH_PLAY_CREDENTIALS` 或 `--credentials` |
| macOS | Developer ID Application 证书；用于公证的 App Store Connect API 密钥或 App 专用密码 | `nsh package --sign …`；`xcrun notarytool` |
| Windows | 代码签名证书 | `signtool` |

## 自动化

商店上传就是普通命令，发布 CI 可以像其他步骤一样运行它们：打 tag 触发工作流，各平台在各自系统的 runner 上打包，最后执行上传。下面是 GitHub Actions 的示意：

```yaml
on:
  push:
    tags: ["v*"]

jobs:
  android:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      # Install nsh, the Android SDK and a JDK, and write the keystore from a secret, then:
      - run: nsh package --target android --format aab --keystore release.jks --key-alias upload --build ${{ github.run_number }}
        env:
          NSH_KEYSTORE_PASSWORD: ${{ secrets.KEYSTORE_PASSWORD }}
      - run: nsh publish android .nsh/out/Notes.aab
        env:
          NSH_PLAY_CREDENTIALS: ${{ runner.temp }}/play.json

  ios:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      # Install nsh, import the certificate into a keychain and write the profile and .p8 key from secrets, then:
      - run: nsh package --target ios --sign "Apple Distribution: Example Inc (ABCDE12345)" --profile AppStore.mobileprovision --build ${{ github.run_number }}
      - run: nsh publish ios .nsh/out/Notes.ipa
        env:
          NSH_APP_STORE_APP_ID: ${{ vars.APP_STORE_APP_ID }}
          NSH_APP_STORE_ISSUER_ID: ${{ secrets.APP_STORE_ISSUER_ID }}
          NSH_APP_STORE_KEY_ID: ${{ secrets.APP_STORE_KEY_ID }}
          NSH_APP_STORE_PRIVATE_KEY: ${{ runner.temp }}/AuthKey.p8
```

用 run number 作为 `build`，无需每次发布都修改 `nsh.json` 就能让它保持递增。固定工作流安装的 NSH 版本，让每次发布都用同一个 NSH 构建。

## 发布到 App Store

iOS 应用通过 App Store Connect 到达用户：你上传一个构建版本，测试员在 TestFlight 中拿到它，Apple 审核通过后，App Store 就会提供它。`nsh package --target ios` 构建并签名 IPA；`nsh publish ios` 把它上传到 App Store Connect。上传之后的一切（TestFlight 群组、商店页面、审核和发布）都在 App Store Connect 中完成。

两个命令都在装有 Xcode 的 macOS 上运行，Rust、Rust + TSX 和 TSX 应用都一样。

### 首次发布之前

每个应用做一次：

1. **加入 Apple Developer Program**，使用将要发布该应用的账号（个人或组织）。
2. **注册 bundle ID。** 在“证书、标识符和描述文件”（Certificates, Identifiers & Profiles）中添加一个 App ID，其 bundle ID 是 `nsh.json` 中应用的 `id`（`com.example.notes`），并勾选应用用到的功能（推送通知、App Groups 等）。
3. **创建分发证书。** 即 *Apple Distribution* 证书，安装到负责打包的 Mac（或 CI runner）的登录钥匙串中。`security find-identity -v -p codesigning` 会把它列为 `Apple Distribution: Example Inc (ABCDE12345)`。
4. **创建 App Store 描述文件**，对应该 App ID 和证书，并下载 `.mobileprovision` 文件。把它放在仓库之外。
5. **在 App Store Connect 中创建应用**（“我的 App”→“新建 App”），使用同一个 bundle ID。“App 信息”下显示的数字 **Apple ID** 就是 `nsh publish ios` 需要的应用 id。
6. **创建团队 API 密钥**：在 App Store Connect 的“用户和访问”→“集成”→“App Store Connect API”中创建，角色为“开发者”“App 管理”或“管理”（Developer、App Manager 或 Admin）。记下它的 **Key ID** 和列表上方显示的 **Issuer ID**，并下载 `.p8` 私钥；Apple 只允许下载一次。

### 配置应用

`nsh.json` 中会进入 iOS 构建的键：

```json
{
  "name": "Notes",
  "id": "com.example.notes",
  "version": "1.4.0",
  "build": 42,
  "icon": "icon.png",
  "ios": {
    "infoPlist": {
      "NSCameraUsageDescription": "Notes uses the camera to scan documents.",
      "ITSAppUsesNonExemptEncryption": false
    },
    "entitlements": {
      "com.apple.security.application-groups": ["group.com.example.notes"]
    }
  }
}
```

| 键 | 在构建中 | 说明 |
|---|---|---|
| `id` | `CFBundleIdentifier` | 必须与 App ID、描述文件和 App Store Connect 中的应用一致。 |
| `name` | `CFBundleDisplayName` | 图标下方显示的名称。商店中的名称在 App Store Connect 中设置。 |
| `version` | `CFBundleShortVersionString` | 用户看到的版本号。新版本要先在 App Store Connect 中创建版本记录，才能提交审核。 |
| `build` | `CFBundleVersion` | 必须大于该版本已上传的所有构建号。 |
| `icon` | 应用图标 | App Store 要求的 1024×1024 正方形、无透明的 PNG（[图标](./packaging.md#icons)）。 |
| `ios.infoPlist` | `Info.plist` 键 | 应用访问的每种受保护资源的用途说明、出口合规等（[Info.plist 键](./packaging.md#infoplist-keys-and-entitlements)）。 |
| `ios.entitlements` | 应用的 entitlements | 打包时会对照描述文件检查。 |
| `ios.privacyManifest` | `PrivacyInfo.xcprivacy` | App Store Connect 检查的隐私清单（[隐私清单](./packaging.md#privacy-manifest)）。 |
| `ios.extensions` | 内嵌的应用扩展 | 各自用自己的描述文件签名（[应用扩展](./packaging.md#app-extensions)）。 |

把 `ITSAppUsesNonExemptEncryption` 设为 `false`（应用只使用系统内置的加密，如 HTTPS 时），TestFlight 的出口合规问题就只需回答一次，而不必每个构建都回答。

### 构建 IPA

```sh
nsh package --target ios \
  --sign "Apple Distribution: Example Inc (ABCDE12345)" \
  --profile /secure/AppStore.mobileprovision
```

这会写出 `.nsh/out/<Name>.ipa`。NSH 会检查可执行文件包含 `arm64` 架构，把 Xcode 和 SDK 构建信息写入 `Info.plist`，嵌入描述文件，检查描述文件覆盖应用的 `id` 及其 entitlements，先签名扩展、再用 `codesign` 签名应用，最后压缩为 `Payload/<Name>.app`。不带 `--sign` 和 `--profile` 时，写出的是无法上传的未签名 `.app`。详情见[打包：iOS 真机](./packaging.md#ios-devices)。

上传前检查结果：

```sh
unzip -q -o .nsh/out/Notes.ipa -d /tmp/notes-ipa
codesign --verify --strict --verbose=2 /tmp/notes-ipa/Payload/Notes.app
codesign -d --entitlements - /tmp/notes-ipa/Payload/Notes.app
```

### 上传到 TestFlight

`nsh publish ios <IPA>` 通过 Apple 的 [build uploads API](https://developer.apple.com/documentation/appstoreconnectapi/build-uploads) 把 IPA 上传到 App Store Connect：

```sh
export NSH_APP_STORE_APP_ID=1234567890
export NSH_APP_STORE_ISSUER_ID=69a6de70-0000-0000-0000-000000000000
export NSH_APP_STORE_KEY_ID=ABC123DEFG
export NSH_APP_STORE_PRIVATE_KEY=/secure/AuthKey_ABC123DEFG.p8
nsh publish ios .nsh/out/Notes.ipa
```

| 参数 | 环境变量 | 含义 |
|---|---|---|
| `--app-id <ID>` | `NSH_APP_STORE_APP_ID` | App Store Connect 中应用的数字 Apple ID。 |
| `--issuer-id <ID>` | `NSH_APP_STORE_ISSUER_ID` | 团队 API 密钥的 issuer ID。 |
| `--key-id <ID>` | `NSH_APP_STORE_KEY_ID` | API 密钥的 ID。 |
| `--private-key <PATH>` | `NSH_APP_STORE_PRIVATE_KEY` | `.p8` 私钥文件。 |

四项都必填；参数会覆盖对应的环境变量。把它们放在 shell 配置文件或 CI 的 secrets 中，不要放进 `nsh.json` 或仓库。

这个命令做的事：

1. 从 IPA 的 `Info.plist` 读取 bundle ID、版本号和构建号，并检查 IPA 的目标是 iOS 真机。
2. 用 API 密钥签发一个短期有效的 ES256 令牌，并检查 IPA 的 bundle ID 与 App Store Connect 中的应用一致。
3. 创建一个构建上传，按 Apple 要求的分片流式上传 IPA，并提交它的 SHA-256 校验和。
4. 打印上传 ID 和应用的 TestFlight 页面。

成功表示 Apple 已接收文件并开始处理。处理需要几分钟到一小时；之后 Apple 会检查签名、描述文件、图标和 `Info.plist`，有任何问题都会发邮件给账号持有人。

### 上传之后

在 App Store Connect 中：

- **TestFlight**：处理完成后，构建版本会立即提供给内部测试员（你团队的成员）。外部测试员需要一个群组，并且每个版本的第一个构建都要经过 Beta 版 App 审核（Beta App Review）。
- **App Store 页面**：名称、副标题、描述、关键词、各设备尺寸的截图、隐私详情（App 隐私）和年龄分级。这些都在 App Store Connect 中填写。
- **审核**：把构建版本添加到该版本，然后点击“添加以供审核”和“提交”。审核通过后，按该版本的发布选项手动发布、自动发布或在指定日期发布。

### 常见错误

| 消息 | 处理方法 |
|---|---|
| `nsh publish ios runs on macOS` | 在 Mac 或 macOS CI runner 上上传。 |
| `IPA must contain exactly one Payload/*.app/Info.plist` | 传入 `nsh package --target ios --sign … --profile …` 写出的 `.ipa`，而不是 `.app` 或模拟器构建。 |
| `IPA must target iOS devices` | 这个 IPA 是模拟器构建：用 `--target ios` 打包，而不是 `--target ios-simulator`。 |
| `IPA bundle identifier does not match --app-id` | 这个应用 id 属于 App Store Connect 中的另一个应用，或 `nsh.json` 的 `id` 与应用的 bundle ID 不同。 |
| `expected a PKCS#8 App Store Connect private key (.p8)` | `--private-key` 指向了错误的文件：使用从 App Store Connect 下载的 `AuthKey_<KEY_ID>.p8`。 |
| `App Store Connect HTTP 401: …` | issuer ID、key ID 和密钥不匹配，或密钥已被吊销。 |
| `App Store Connect HTTP 403: …` | 该密钥的角色无权上传构建：使用“开发者”“App 管理”或“管理”角色。 |
| `App Store Connect HTTP …` 提示构建已存在 | 已经上传过相同版本号和构建号的构建：在 `nsh.json` 中提高 `build` 并重新打包。 |
| `upload … did not finish; check App Store Connect before retrying` | 连接中断。每次运行都会开始一次新的上传；先在 App Store Connect 中查看打印出的上传 ID，再重新运行。 |
| `Apple rejected the uploaded file; check App Store Connect` | 文件上传后被 Apple 拒绝；原因见 App Store Connect 和 Apple 的邮件。 |
| 描述文件不覆盖应用的 id（打包时） | 为应用的 bundle ID 或覆盖它的通配 ID 创建 App Store 描述文件。 |

Apple 在处理时发现的问题（缺少用途说明、图标带透明、描述文件缺少某项 entitlement）会通过邮件通知，并显示在 App Store Connect 的构建版本上；在 `nsh.json` 中修正，提高 `build`，再重新打包上传。

## 发布到 Google Play

Google Play 接收 Android App Bundle（`.aab`）。`nsh package --target android --format aab` 不借助 Gradle 构建一个 bundle 并用你的上传密钥签名；`nsh publish android` 通过 Google Play Developer API 上传它，并发布到**内部测试**轨道。推进到封闭测试、开放测试或正式版，以及商品详情和审核，都在 Play Console 中完成。

两个命令都可以在 macOS、Linux 和 Windows 上运行。

### 首次发布之前

每个应用做一次：

1. **注册 Google Play 开发者账号**，并在 Play Console 中**创建应用**（“创建应用”）。应用的包名由你上传的第一个 bundle 确定，必须是 `nsh.json` 中应用的 `id`。
2. **创建上传密钥**，用于签名你上传的每个 bundle：

   ```sh
   keytool -genkeypair -v -keystore release.jks -alias upload -keyalg RSA -keysize 2048 -validity 10000
   ```

   把 `release.jks` 及其密码放在仓库之外并做好备份。启用 Play 应用签名（新应用的默认设置）时，设备安装的内容由 Google 用它自己的应用签名密钥签名；上传密钥丢失后可以通过 Play Console 支持重置。
3. **创建用于上传的服务账号**：在 Google Cloud 中选择或创建一个项目，启用 **Google Play Android Developer API**，创建服务账号并为它下载 JSON 密钥。
4. **给服务账号授权**：在 Play Console 的“用户和权限”→“邀请新用户”中填入服务账号的邮箱，授予它该应用的访问权限，以及发布到测试轨道的权限。
5. **在 Play Console 中完成首次发布。** 在应用于 Play Console 中发布过一次之前，Google Play 只接受通过 API 创建的草稿版本：用 `nsh publish android --draft` 上传第一个 bundle，再在 Play Console 中发布这个草稿。

### 配置应用

```json
{
  "name": "Notes",
  "id": "com.example.notes",
  "version": "1.4.0",
  "build": 42,
  "icon": {
    "android": {
      "foreground": "icon-foreground.png",
      "background": "#1F6FEB",
      "monochrome": "icon-monochrome.png"
    }
  },
  "android": {
    "targetSdk": 35,
    "permissions": ["android.permission.CAMERA"]
  }
}
```

| 键 | 在构建中 | 说明 |
|---|---|---|
| `id` | 包名 | 应用在 Google Play 上的身份；上传后不能更改。 |
| `name` | `android:label` | 图标下方显示的名称。商店中的名称在 Play Console 中设置。 |
| `version` | `versionName` | 用户看到的版本号。 |
| `build` | `versionCode` | 必须大于该应用在任何轨道上已上传的所有版本代码。 |
| `icon` | 启动器图标 | 一张 PNG，或带单色图层（用于主题图标）的自适应图标（[图标](./packaging.md#icons)）。 |
| `android.targetSdk` | `targetSdkVersion` | 默认 35。Google Play 要求新应用和更新使用较新的级别（[目标 API 级别](./packaging.md#target-api-level)）。 |
| `android.permissions` | `<uses-permission>` | 声明的权限；其中一些需要在 Play Console 中填写声明（[权限](./packaging.md#permissions)）。 |

### 构建 bundle

```sh
NSH_KEYSTORE_PASSWORD=… nsh package --target android --format aab \
  --keystore release.jks --key-alias upload
```

这会写出 `.nsh/out/<Name>.aab`，由 JDK 中的 `jarsigner` 签名。它包含 `arm64-v8a` 和 `armeabi-v7a` 的原生代码；Google Play 据此构建每台设备安装的 APK。`NSH_KEYSTORE_PASSWORD` 能让密码不进入 shell 历史（`--keystore-password` 也可以）。不带 `--keystore` 时，bundle 用 debug 密钥签名，Google Play 会拒绝。环境要求（Android SDK、Rust 应用需要的 NDK、JDK）见[打包：Android](./packaging.md#android)。

release bundle 中的 `libnsh.so` 不带符号；完整的库写在运行时旁的 `symbols/<abi>/libnsh.so`。在 Play Console 中上传它们（该 bundle 的“下载内容”→原生调试符号），就能在 Android Vitals 中看懂原生崩溃堆栈。

上传前想在设备上试用 bundle，可以用 [bundletool](https://github.com/google/bundletool)：先 `bundletool build-apks --bundle .nsh/out/Notes.aab --output notes.apks --connected-device`，再 `bundletool install-apks --apks notes.apks`。

### 上传到内部测试轨道

```sh
export NSH_PLAY_CREDENTIALS=/secure/play-service-account.json
nsh publish android .nsh/out/Notes.aab
```

```sh
nsh publish android <BUNDLE.aab> [--entry <DIR>] [--package <NAME>] [--credentials <KEY.json>] [--draft]
```

| 参数 | 默认值 | 含义 |
|---|---|---|
| `--entry <DIR>` | `.` | 应用的目录或入口，其 `nsh.json` 的 `id` 即包名。 |
| `--package <NAME>` | `nsh.json` `id` | Google Play 上的包名。 |
| `--credentials <KEY.json>` | `NSH_PLAY_CREDENTIALS` | 服务账号的 JSON 密钥。 |
| `--draft` | 关闭 | 以草稿发布；在应用于 Play Console 中完成首次发布之前，Google Play 要求这样做。 |

这个命令在 Google Play 上对应用做一次编辑（edit），步骤如下：

1. 以服务账号身份登录。
2. 打开一个编辑，上传 bundle，并读取 Google Play 从中识别出的版本代码。
3. 把内部测试轨道的版本设为该版本代码（状态为已完成，带 `--draft` 时为草稿）。
4. 提交编辑。任何一步失败都会删除这次编辑，不会出现发布了一半的情况。

它会打印现在位于内部测试轨道上的版本代码。内部测试员（最多 100 人，来自 Play Console 中的测试员列表）几分钟内就能从 Google Play 收到更新。

### 上传之后

在 Play Console 中：

- **推进发布**：“测试”→“内部测试”→该版本→“推广发布”，推进到封闭测试、开放测试或正式版。新注册的个人开发者账号必须先进行封闭测试，才能获得正式版访问权限。
- **商品详情**：应用名称、简短说明和完整说明、图标、置顶大图、手机和平板截图。
- **应用内容**：隐私权政策、数据安全、广告、内容分级和目标受众，在首次发布正式版之前必须填写。
- **审核与发布**：发布到封闭测试、开放测试和正式版轨道的版本都要经过审核；正式版可以先向一定比例的用户发布，之后再逐步提高比例。

### 常见错误

| 消息 | 处理方法 |
|---|---|
| `nsh.json has no \`id\`: give the app's package name with --package` | 在应用目录中运行命令，传入 `--entry`，或提供 `--package`。 |
| `the service account key …` | JSON 密钥文件不存在或无法读取：检查 `NSH_PLAY_CREDENTIALS` 或 `--credentials`。 |
| `Invalid JWT Signature.` | 服务账号密钥已在 Google Cloud 中被删除或轮换：下载新的 JSON 密钥。 |
| `could not open an edit on Google Play`，HTTP 403 | 服务账号在 Play Console 中没有该应用的访问权限，或其项目未启用 Google Play Android Developer API。 |
| `could not open an edit on Google Play`，HTTP 404 | Play Console 中没有这个包名的应用：创建它，或修正 `id`/`--package`。 |
| `Google Play refused the bundle` … `version code that has already been used` | 在 `nsh.json` 中提高 `build` 并重新打包。 |
| `Google Play refused the bundle` … 用错误的密钥签名 | bundle 用了 debug 密钥或其他上传密钥签名：用为该应用登记的上传 keystore 打包。 |
| `Google Play refused the internal testing release` … draft | 应用还没有在 Play Console 中发布过：传入 `--draft`，然后在 Play Console 中发布。 |

## 发布 macOS 应用

在 Mac App Store 之外，Mac 应用以已签名并经过公证的 `.app` 分发，用户从你的网站或 GitHub Release 下载。`nsh package --target macos --sign …` 构建应用并用你的 Developer ID 签名；公证、装订（staple）和上传则用 Apple 的命令行工具和你的发布主机完成。之后由[应用更新](./app-updates.md)让已安装的副本保持最新。

`nsh publish` 没有 macOS 商店上传：NSH 目前还不能为 Mac App Store 打包。

### 首次发布之前

每个开发者账号做一次：

1. **加入 Apple Developer Program。**
2. **创建 Developer ID Application 证书**（“证书、标识符和描述文件”→“证书”；只有账号持有人能创建），安装到负责打包的 Mac（或 CI runner）的登录钥匙串中。`security find-identity -v -p codesigning` 会把它列为 `Developer ID Application: Example Inc (ABCDE12345)`。
3. **为 `notarytool` 设置公证凭据**，以一个配置名称在钥匙串中保存一次。使用 App Store Connect 团队 API 密钥（与 [iOS 上传](#before-the-first-release)使用的同一种）：

   ```sh
   xcrun notarytool store-credentials notes-notary \
     --key /secure/AuthKey_ABC123DEFG.p8 --key-id ABC123DEFG --issuer 69a6de70-0000-0000-0000-000000000000
   ```

   或使用 Apple ID 和 App 专用密码：`xcrun notarytool store-credentials notes-notary --apple-id
   you@example.com --team-id ABCDE12345`。

### 配置应用

| 键 | 在构建中 | 说明 |
|---|---|---|
| `id` | `CFBundleIdentifier` | 也决定应用[存储](../modules/nsh.md#storage)的位置；保持不变。 |
| `name` | `.app` 名称和 `CFBundleName` | |
| `version`、`build` | `CFBundleShortVersionString`、`CFBundleVersion` | `version` 必须是 semver，并且要递增才能[自我更新](./app-updates.md)。 |
| `icon`（或 `icon.macos`） | `AppIcon.icns` | macOS 图标的图案内缩在透明画布上（[图标](./packaging.md#icons)）。 |
| `macos.infoPlist` | `Info.plist` 键 | 用途说明和其他键（[Info.plist 键](./packaging.md#infoplist-keys-and-entitlements)）。 |
| `macos.entitlements` | 应用的 entitlements | 与 Hardened Runtime 一起签入应用；添加应用需要的例外项，如访问摄像头。 |
| `macos.infoPlist` 中的 `LSMinimumSystemVersion` | 能打开应用的最低 macOS 版本 | 默认 macOS 14（[最低系统版本](./packaging.md#minimum-system-version)）。 |
| `macos.extensions` | `Contents/PlugIns/` | 预先构建的 `.appex` 包，由内向外签名（[macOS 应用扩展](./packaging.md#macos-app-extensions)）。 |

### 构建并签名

```sh
nsh package --target macos --arch universal \
  --sign "Developer ID Application: Example Inc (ABCDE12345)"
```

这会写出 `.nsh/out/<Name>.app`。有证书时，NSH 先签名内嵌的 framework 和扩展，再签名应用，并按公证的要求启用 Hardened Runtime（`--options runtime`）和安全时间戳（`--timestamp`），最后用 `codesign --verify --deep --strict` 验证结果。`--arch universal` 能同时在 Apple 芯片和 Intel Mac 上运行（Rust 应用需要 `rustup target add x86_64-apple-darwin`）；省略它则只面向本机的处理器。不带 `--sign` 时应用使用 ad hoc 签名，能在构建它的 Mac 上运行，但在其他 Mac 上会被 Gatekeeper 拦下。

### 公证并装订

Apple 会扫描经过公证的应用是否含有恶意内容并做记录，这样 Gatekeeper 打开它时就不会警告。提交应用的 zip，等待结果，再把票据装订到应用上，让它离线也能打开：

```sh
cd .nsh/out
ditto -c -k --keepParent Notes.app Notes-notarize.zip
xcrun notarytool submit Notes-notarize.zip --keychain-profile notes-notary --wait
xcrun stapler staple Notes.app
spctl --assess --type execute --verbose Notes.app   # "accepted, source=Notarized Developer ID"
```

如果提交结果为 `Invalid`，`xcrun notarytool log <submission-id> --keychain-profile notes-notary` 会列出所有问题。

### 分发

把装订后的应用打包成归档供下载。用 `ditto` 制作的 zip 会保留签名和扩展属性：

```sh
ditto -c -k --keepParent Notes.app Notes-1.4.0-macos.zip
```

`nsh.json` 中有 [`"updater"`](./app-updates.md#set-the-source) 时，`nsh package` 还会写出用于自我更新的 `<Name>-<version>-macos-<arch>.tar.gz`。它包含的是打包时、公证前的应用：用同样的名称重新归档装订后的应用，再按[应用更新](./app-updates.md#publish-a-release)所述写出清单：

```sh
tar -czf Notes-1.4.0-macos-aarch64.tar.gz Notes.app
nsh update-manifest *.tar.gz --notes CHANGES.md --key keys/update.secret
```

把归档、`latest.json` 及其签名上传到 GitHub Release 或你的主机。已安装的副本会检查清单、下载归档、验证归档和应用的代码签名，然后替换自己。

### 常见错误

| 问题 | 处理方法 |
|---|---|
| `codesign` 找不到签名身份 | 证书不在构建使用的钥匙串中，或缺少私钥：导入连同私钥一起导出的 `.p12`。 |
| 在 CI 上签名时出现 `errSecInternalComponent` | 钥匙串已锁定：解锁它（`security unlock-keychain`），并允许 `codesign` 使用该密钥（`security set-key-partition-list`）。 |
| 公证 `Invalid`：未使用有效的 Developer ID 证书签名 | 应用使用了 ad hoc 签名或 Apple Development 证书：用 `--sign "Developer ID Application: …"` 打包。 |
| 公证 `Invalid`：签名不包含安全时间戳 | 通过会自动添加时间戳的 `nsh package --sign` 签名；不要在不带 `--timestamp` 的情况下手动重新签名。 |
| 下载后提示“Notes 已损坏，无法打开” | 归档丢失了签名（用会丢弃属性的工具压缩，或签名后被修改）：装订之后用 `ditto` 或 `tar` 归档。 |
| 公证成功了 Gatekeeper 仍然警告 | 票据没有装订而 Mac 处于离线状态，或下载的应用不是经过公证的那个：先装订，再重新归档。 |
| 应用无法使用摄像头、麦克风或其他受保护资源 | 在 `macos.infoPlist` 中添加用途说明，并在 `macos.entitlements` 中添加 Hardened Runtime 例外项。 |

## 发布 Windows 应用

`nsh package --target windows` 写出直接下载发布所需的一切：应用目录、它的 `.zip`，以及用 Inno Setup 构建的按用户安装的安装程序。你用自己的代码签名证书签名可执行文件，再上传到你的网站或 GitHub Release；[应用更新](./app-updates.md)让已安装的副本保持最新。

`nsh publish` 没有 Windows 商店上传：NSH 目前还不能为 Microsoft Store 打包 MSIX。

### 首次发布之前

在负责打包的 Windows 机器（或 CI runner）上：

1. **安装工具。** TSX 应用不需要任何工具；Rust 应用的 runner 用 Visual Studio Build Tools 构建。安装程序需要 Inno Setup 6：`winget install JRSoftware.InnoSetup`。
2. **获取代码签名证书**：从证书颁发机构购买，或使用 Azure Trusted Signing 这样的云签名服务。未签名的可执行文件照样能运行，但 SmartScreen 会提示发布者未知，一些杀毒软件也会对它们抱有怀疑。
3. **安装 Windows SDK** 以获得 `signtool.exe`（它随 Visual Studio 安装程序的 Windows SDK 组件提供）。

### 配置应用

| 键 | 在构建中 | 说明 |
|---|---|---|
| `id` | 安装程序的 AppId、开始菜单快捷方式的 AppUserModelID | 用于在任务栏中归组应用的窗口，并在通知中标识应用；相同 id 的新版安装程序会升级已安装的应用。保持不变。 |
| `name` | `<Name>.exe`、安装目录和开始菜单项 | |
| `version` | 安装程序的版本和“已安装的应用”中的条目 | 必须是 semver，并且要递增才能[自我更新](./app-updates.md)。 |
| `icon`（或 `icon.windows`） | 嵌入 `<Name>.exe`，并写为 `nsh\icon.ico` | 在 Windows 上打包：只有 Windows 能嵌入图标（[图标](./packaging.md#icons)）。 |

### 构建

```powershell
nsh package --target windows
```

```text
.nsh\out\<Name>\           the app: <Name>.exe and nsh\
.nsh\out\<Name>.zip        the same, as an archive
.nsh\out\<Name>.iss        the Inno Setup script
.nsh\out\<Name>-setup.exe  the installer, when Inno Setup 6 is installed
```

安装程序不需要管理员权限：它为当前用户安装到 `%LOCALAPPDATA%\Programs\<Name>`，添加开始菜单快捷方式和可用于卸载的“已安装的应用”条目，并在原位升级已安装的副本（会先关闭应用）。无论升级还是卸载，都不会动应用的数据。详情见[打包：Windows](./packaging.md#windows)。

### 签名

先签名应用的可执行文件，再构建安装程序，这样安装后的应用也是已签名的；然后签名安装程序。证书位于 Windows 证书存储中时：

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /n "Example Inc" .nsh\out\Notes\Notes.exe
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" .nsh\out\Notes.iss
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /n "Example Inc" .nsh\out\Notes-setup.exe
signtool verify /pa .nsh\out\Notes-setup.exe
```

`ISCC.exe` 用 `nsh package` 写出的脚本重新编译安装程序，这次包含已签名的可执行文件；它的路径取决于 Inno Setup 的安装位置（为整台机器安装时是 `C:\Program Files (x86)\Inno Setup 6\ISCC.exe`）。如果你也发布 `.zip`，同样要从已签名的目录重新制作。始终加上时间戳（`/tr`）：这样证书过期后签名仍然有效。

### 分发

把 `Notes-setup.exe`（以及给偏好便携版的用户准备的 `.zip`）上传到你的网站或 GitHub Release。

`nsh.json` 中有 [`"updater"`](./app-updates.md#set-the-source) 时，`nsh package` 还会写出用于自我更新的 `<Name>-<version>-windows-<arch>.zip`。把已签名的应用放进去，再与其他平台的归档一起写出清单（[应用更新](./app-updates.md#publish-a-release)）：

```powershell
nsh update-manifest *.zip *.tar.gz --notes CHANGES.md --key keys\update.secret
```

### SmartScreen

SmartScreen 根据信誉做判断，信誉随着人们下载和运行同一证书签名的文件而增长。新证书或新类型的证书，即使文件已签名，也可能在一段时间内显示“Windows 已保护你的电脑”警告。每次发布都用同一个证书签名，信誉才能延续下去。

### 常见错误

| 问题 | 处理方法 |
|---|---|
| `.nsh\out` 中没有 `<Name>-setup.exe` | 没有安装 Inno Setup 6，或不在 `nsh package` 查找的位置：用 `winget install JRSoftware.InnoSetup` 安装，或用 `ISCC.exe` 编译 `.nsh\out\<Name>.iss`。 |
| 可执行文件显示默认图标 | 它是在 macOS 或 Linux 上打包的：在 Windows 上打包才能嵌入图标。 |
| 无法识别 `signtool` | 在 Developer PowerShell 中运行，或把 Windows SDK 的 `bin\<version>\x64` 目录加入 `PATH`。 |
| `SignTool Error: No certificates were found that met all the given criteria` | 证书不在 `signtool` 搜索的存储中，或 `/n` 与证书的使用者不匹配：用 `certutil -user -store My` 列出证书。 |
| 已签名的安装程序显示“Windows 已保护你的电脑” | 该证书的 SmartScreen 信誉仍在积累中（见上文）。 |
| 安装程序要求关闭应用 | 它在原位升级，会先关闭正在运行的副本；这是预期行为。 |

## 发布 Linux 应用

`nsh package --target linux` 把应用写成一个目录及其 `.tar.gz`，附带桌面条目和图标。你把归档上传到你的网站或 GitHub Release，用户解压并安装桌面条目；[应用更新](./app-updates.md)让已安装的副本保持最新。

`nsh publish` 没有 Linux 商店上传，`nsh package` 目前也还不能写出 Flatpak、AppImage、`.deb` 或 `.rpm` 包。

### 首次发布之前

在 Linux 上打包：runner 是为构建它的机器构建的，所以要在你要发布的处理器架构（`x86_64` 或 `aarch64`）上打包，并且使用对你支持的用户来说足够旧的发行版，因为比用户更新的 glibc 会让应用无法启动。Ubuntu 24.04 是面向当前桌面系统的不错选择。构建需要 GTK 4（4.14 或更高）、Pango 和 WebKitGTK 6.0 的开发包（[Linux：环境要求](./linux.md#requirements)）。

### 配置应用

| 键 | 在包中 | 说明 |
|---|---|---|
| `id` | `<id>.desktop`、`StartupWMClass`、GTK 应用 id、`<id>.png` | 桌面环境靠它把应用的窗口和启动器对应起来；使用反向 DNS 形式，如 `com.example.notes`。保持不变。 |
| `name` | 可执行文件 `<Name>`、目录和桌面条目的 `Name` | |
| `version` | 自我更新归档 | 必须是 semver，并且要递增才能[自我更新](./app-updates.md)。 |
| `icon`（或 `icon.linux`） | `<id>.png`，由桌面条目的 `Icon` 指定 | 一张铺满画布的正方形 PNG（[图标](./packaging.md#icons)）。 |

### 构建

```sh
nsh package --target linux
```

```text
.nsh/out/<Name>/
  <Name>          the executable
  nsh/            the app's code, assets and app.json
  <id>.desktop    the desktop entry
  <id>.png        the icon
.nsh/out/<Name>.tar.gz
```

详情见[打包：Linux](./packaging.md#linux)。

### 分发

把 `<Name>.tar.gz` 上传到你的网站或 GitHub Release，并告诉用户需要安装哪些库。运行时需要 GTK 4、Pango 和 WebKitGTK 6.0：

| 发行版 | 安装 |
|---|---|
| Debian、Ubuntu | `sudo apt install libgtk-4-1 libwebkitgtk-6.0-4` |
| Fedora | `sudo dnf install gtk4 webkitgtk6.0` |
| Arch | `sudo pacman -S gtk4 webkitgtk-6.0` |

为单个用户安装分四步，可以由安装脚本完成：把目录解压到一个固定的位置，把可执行文件链接到 `PATH` 上，再把桌面条目和图标复制到桌面环境查找它们的位置。

```sh
mkdir -p ~/.local/opt ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/256x256/apps
tar -xzf Notes.tar.gz -C ~/.local/opt
ln -sf ~/.local/opt/Notes/Notes ~/.local/bin/Notes
cp ~/.local/opt/Notes/com.example.notes.desktop ~/.local/share/applications/
cp ~/.local/opt/Notes/com.example.notes.png ~/.local/share/icons/hicolor/256x256/apps/
```

可执行文件会通过链接找到它的 `nsh/` 目录。发布前用 `desktop-file-validate ~/.local/share/applications/com.example.notes.desktop` 检查桌面条目。

`nsh.json` 中有 [`"updater"`](./app-updates.md#set-the-source) 时，`nsh package` 还会写出用于自我更新的 `<Name>-<version>-linux-<arch>.tar.gz`。把它和清单一起上传（[应用更新](./app-updates.md#publish-a-release)）：

```sh
nsh update-manifest *.tar.gz *.zip --notes CHANGES.md --key keys/update.secret
```

### 常见错误

| 问题 | 处理方法 |
|---|---|
| `error while loading shared libraries: libgtk-4.so.1`（或 `libwebkitgtk-6.0.so.4`） | 安装上文列出的运行时库。 |
| `version 'GLIBC_2.39' not found` | 应用在比用户更新的发行版上打包：换一个较旧的发行版打包。 |
| 应用没有出现在启动器中 | 桌面条目不在 `~/.local/share/applications` 中，或它的 `Exec` 找不到可执行文件：把可执行文件链接到 `PATH` 上。 |
| 启动器显示通用图标，或应用运行时出现第二个图标 | 图标不在图标主题目录中，或桌面条目被改了名：保持文件名为 `<id>.desktop`，`StartupWMClass` 与它对应。 |
| `exec format error` | 归档是为另一种处理器构建的：在 `x86_64` 和 `aarch64` 上分别打包。 |

## 发布到 Web

在 Web 上，应用就是一个静态站点。`nsh package --target web` 写出它；你把目录复制到任意 Web 服务器、对象存储或静态托管服务上。没有应用商店，不需要签名，也没有审核，部署之后每个用户下次加载时就会更新。这里没有 `nsh publish` 要做的事。

### 配置应用

| 键 | 在站点中 | 说明 |
|---|---|---|
| `name` | 页面标题和 Web 应用清单的名称 | |
| `id` | Web 应用清单的 id | |
| `icon`（或 `icon.web`） | favicon、`apple-touch-icon` 和清单图标 | 缩放为 32、180、192 和 512 像素（[图标](./packaging.md#icons)）。 |
| `appearance` | 页面加载时的颜色 | |
| `web.base` | 站点所在的路径 | 应用在自己的路径下打开时需要（[链接到应用内](./web.md#links-into-the-app)）。 |
| `web.host` | 页面的宿主脚本 | [宿主的函数](./web.md#the-hosts-functions)。 |

### 构建

```sh
nsh package --target web            # a Rust app, from its directory
nsh package App.tsx --target web    # a TSX app
nsh package --target web --run      # and serve it on http://127.0.0.1:4173/ to try it
```

这会写出 `.nsh/out/<Name>/`（用 `--out` 指定其他目录）：`index.html`、`manifest.webmanifest`，以及其余所有按内容命名的文件（`nsh.<digest>.wasm`、`app.<digest>.js`、`assets/…`），旁边还有它们的 Brotli 和 gzip 副本。一次发布只会改变 `index.html` 和真正变化了的文件。完整的文件列表和运行时大小见 [Web：发布应用](./web.md#publish-an-app)。

站点默认使用相对 URL，因此放在任何主机的任何目录下都能工作。如果想让文件从 CDN 提供、页面仍留在应用自己的地址，传入 `--asset-url https://cdn.example.com/notes/`。

### 部署

任何静态托管都可以。遵守两条规则，应用就能快速加载并立即更新：

| 文件 | `Cache-Control` |
|---|---|
| `index.html`、`manifest.webmanifest` | `no-cache` |
| 其他所有文件 | `public, max-age=31536000, immutable` |

以 `application/wasm` 提供 `.wasm`，并向支持的浏览器发送预压缩的 `.br` 和 `.gz` 副本（[部署](./web.md#deploy-it)中有 nginx 和 Caddy 的配置）。先上传带哈希的文件，再上传 `index.html`，这样访客永远不会拿到所引用文件尚未就位的页面。

#### GitHub Pages

```yaml
on:
  push:
    tags: ["v*"]

permissions:
  pages: write
  id-token: write

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: github-pages
    steps:
      - uses: actions/checkout@v4
      # Install nsh (pin the version), then:
      - run: nsh package --target web --out site
      - uses: actions/upload-pages-artifact@v3
        with:
          path: site
      - uses: actions/deploy-pages@v4
```

项目站点位于 `/<repo>/` 下；相对 URL 能处理这一点。在自己的路径下打开的应用要设置 `"web": { "base": "/<repo>/" }`，并添加一份 `index.html` 的副本作为 `404.html`（上传前执行 `cp site/index.html site/404.html`），GitHub Pages 会用它响应未知路径。GitHub Pages 不能按文件设置 `Cache-Control`，所以回访的用户也会重新验证带哈希的文件；能自行设置响应头的主机加载更快。

#### 对象存储和 CDN

```sh
nsh package --target web --out site
aws s3 sync site s3://example-notes --exclude index.html --exclude manifest.webmanifest \
  --cache-control "public, max-age=31536000, immutable"
aws s3 cp site/index.html s3://example-notes/ --cache-control no-cache
aws s3 cp site/manifest.webmanifest s3://example-notes/ --cache-control no-cache
```

把 CDN 配置为以 `application/wasm` 提供 `.wasm`，并用 Brotli 压缩，或直接提供 `.br` 副本。

### 支持的浏览器

Safari 16.4（iOS 和 iPadOS 16.4）、Chrome 和 Edge 97、Firefox 108 及更高版本。更早的浏览器会显示空白页面（[支持的浏览器](./web.md#supported-browsers)）。

### 常见错误

| 问题 | 处理方法 |
|---|---|
| 页面空白，控制台中出现 `Failed to load module script` | 主机以错误的内容类型提供 `.js` 或 `.wasm`，或文件没有上传：查看“网络”面板。 |
| `WebAssembly.instantiateStreaming` 因响应的 MIME 类型不正确而失败 | 以 `application/wasm` 提供 `.wasm`。 |
| 打开应用的某个路径或在该路径上刷新时返回 404 | 用 `index.html` 响应应用下的所有路径，并设置 `web.base`（[链接到应用内](./web.md#links-into-the-app)）。 |
| 部署后用户看到的仍是旧版本 | `index.html` 被缓存了：用 `Cache-Control: no-cache` 提供它。 |
| 来自 CDN 的文件被 CORS 拦截 | `--asset-url` 指向 CDN 时，CDN 必须针对页面的源返回 `Access-Control-Allow-Origin`。 |
| 在较旧的 iPhone 或 Android WebView 上页面空白 | 浏览器版本低于支持的版本。 |

## 另请参阅

- [打包](./packaging.md)：各平台的 `nsh package`
- [CLI](./cli.md)：所有命令和参数
- [应用更新](./app-updates.md)：自我更新的应用
- [OTA 更新](./ota-updates.md)：无需发布商店版本即可更新 TSX

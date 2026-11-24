---
title: 发布
description: "把 NSH 应用发布到应用商店：iOS 的 App Store Connect 和 TestFlight、Android 的 Google Play、商店凭据与发布 CI。"
order: 18.5
group: publishing
---

# 发布

发布指把打包好的应用上传到应用商店：iOS 上传到 App Store Connect，Android 上传到 Google Play，分别用 `nsh publish ios` 和 `nsh publish android`。[打包](./packaging.md)先生成文件，也介绍没有商店上传的平台：macOS、Windows、Linux 应用的签名、公证与分发，以及 Web 应用的托管。

- [iOS](#publish-to-the-app-store)：App Store Connect 和 TestFlight
- [Android](#publish-to-google-play)：Google Play

## 各商店支持什么

| 平台 | `nsh package` 写出什么 | 商店 | `nsh publish` 做什么 |
|---|---|---|---|
| iOS | 已签名的 `.ipa` | App Store Connect：先 TestFlight，再 App Store | 上传 IPA，等待处理完成，分发给 TestFlight 测试组，准备 App Store 版本和商店信息，加 `--submit` 时提交审核（`nsh publish ios`） |
| Android | 已签名的 `.aab`（或 `.apk`） | Google Play | 上传 bundle 并发布到某个轨道（未用 `--track` 指定时为内部测试），支持分阶段发布、在轨道间推进版本和更新商店信息（`nsh publish android`） |

不加显式参数，任何内容都不会公开发布：`nsh publish ios` 只有加 `--submit` 才提交 App 审核，`nsh publish android` 只有加 `--track` 才发布到内部测试以外的轨道。`nsh publish status <store>` 报告发布进展到哪一步，`nsh publish doctor <store>` 检查配置而不做任何改动。macOS、Windows、Linux 和 Web 目前在 NSH 中没有商店上传，以直接下载或静态站点的方式交付，见打包中的 [macOS](./packaging.md#sign-notarize-and-distribute)、[Windows](./packaging.md#sign-and-distribute)、[Linux](./packaging.md#distribute) 和 [Web](./packaging.md#host-the-site)。

不带平台的 `nsh publish <ENTRY> --key … --cert …` 生成的是用于 [OTA 更新](./ota-updates.md)和[签名包](./signed-packages.md)的签名包，不是商店上传。

## 一次发布，逐步进行

1. **在 `nsh.json` 中设置版本。** `version` 是用户看到的版本号（`1.4.0`）；`build` 是一个整数，在每个平台上每次上传到商店都必须递增（Apple 平台上是 `CFBundleVersion`，Android 上是 `versionCode`）。两者也都可以在命令行上传入（`--app-version`、`--build`），在 CI 给构建编号时很方便。
2. **打包。** iOS 需要装有 Xcode 的 Mac；Android 在哪里都可以打包。见[打包](./packaging.md)。
3. **签名。** 提供 Apple Distribution 签名身份和描述文件，或上传 keystore，打包时即完成签名。
4. **发布。** 用 `nsh publish ios` 或 `nsh publish android` 上传到商店。商店身份和商店信息写在 `nsh.json` 中（见[商店信息](#store-listing)）、凭据放在环境变量中时，产物就是唯一的参数。
5. **后续跟进。** `nsh publish status ios` 或 `nsh publish status android` 显示处理、审核和放量进度；商店的控制台中也能看到同样的信息。

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
| iOS | App Store Connect 团队 API 密钥：issuer ID、key ID、`.p8` 文件 | `NSH_APP_STORE_ISSUER_ID`、`NSH_APP_STORE_KEY_ID`；密钥来自 `NSH_APP_STORE_PRIVATE_KEY` 或 `~/.appstoreconnect/private_keys/AuthKey_<KEY_ID>.p8` |
| Android | 上传 keystore 及其密码 | `nsh package --keystore … --key-alias …`、`NSH_KEYSTORE_PASSWORD` |
| Android | Google Cloud 服务账号 JSON 密钥 | `NSH_PLAY_CREDENTIALS` 或 `--credentials` |

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

## 商店信息

商店信息（名称、描述、更新说明、截图）和应用在各商店中的身份都跟应用放在一起，这样发布时不需要任何参数。把它们写在 `nsh.json` 的 `"store"` 中、与 `nsh.json` 同级的 `store/` 目录中，或两者都用；`nsh.json` 中的值优先于文件。所有 `nsh publish <store>` 读取的是同一份商店信息。

```json
{
  "store": {
    "locales": {
      "en-US": {
        "name": "Notes",
        "subtitle": "Notes that sync",
        "shortDescription": "Notes that sync everywhere.",
        "releaseNotes": "Faster sync.",
        "keywords": ["notes", "sync"],
        "supportUrl": "https://example.com/support",
        "screenshots": { "phone": ["store/en-US/phone-1.png"] }
      }
    },
    "ios": { "appId": "1234567890", "usesNonExemptEncryption": false, "betaGroups": ["QA"], "release": "phased" }
  }
}
```

```text
store/
  en-US/
    name.txt               store name
    subtitle.txt           App Store subtitle
    short_description.txt  Google Play's short description
    description.txt        full description
    release_notes.txt      what's new in this release
    keywords.txt           also promotional_text, support_url, marketing_url, privacy_policy_url
    screenshots/
      phone/01.png         App Store screenshots, by device class, in file-name order
      tablet/01.png
    android/
      phoneScreenshots/    Google Play images, by image type, in file-name order
      featureGraphic/
      icon/
  zh-CN/
    ...
```

语言使用 BCP 47 标签（`en-US`、`zh-Hans`）。`releaseNotes`（`release_notes.txt`）是所有商店共用的更新说明；也接受 `whatsNew` 和 `whats_new.txt`。每个文件和键都可省略：未给出的内容保持商店中的现值。`nsh.json` 不认识的键会被忽略。

应用在各商店中的身份：在 Google Play 上是包名，即顶层的 `id`；在 App Store 上是 `store.ios.appId` 中的数字 Apple ID。凭据永远不要写在这里（见[凭据](#credentials)）。

## 发布到 App Store

iOS 应用通过 App Store Connect 到达用户：你上传一个构建版本，测试员在 TestFlight 中拿到它，Apple 审核通过后，App Store 就会提供它。`nsh package --target ios` 构建并签名 IPA；`nsh publish ios` 把它上传到 App Store Connect，等待 Apple 处理完成，分发给 TestFlight 测试组，用[商店信息](#store-listing)创建 App Store 版本，并在要求时提交 App 审核。

两个命令都在装有 Xcode 的 macOS 上运行，Rust、Rust + TSX 和 TSX 应用都一样。

### 首次发布之前

每个应用做一次：

1. **加入 Apple Developer Program**，使用将要发布该应用的账号（个人或组织）。
2. **注册 bundle ID。** 在“证书、标识符和描述文件”（Certificates, Identifiers & Profiles）中添加一个 App ID，其 bundle ID 是 `nsh.json` 中应用的 `id`（`com.example.notes`），并勾选应用用到的功能（推送通知、App Groups 等）。
3. **创建分发证书。** 即 *Apple Distribution* 证书，安装到负责打包的 Mac（或 CI runner）的登录钥匙串中。`security find-identity -v -p codesigning` 会把它列为 `Apple Distribution: Example Inc (ABCDE12345)`。
4. **创建 App Store 描述文件**，对应该 App ID 和证书，并下载 `.mobileprovision` 文件。把它放在仓库之外。
5. **在 App Store Connect 中创建应用**（“我的 App”→“新建 App”），使用同一个 bundle ID；Apple 不允许通过 API 创建应用。“App 信息”下显示的数字 **Apple ID** 写进 `nsh.json` 的 `store.ios.appId`。
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

把 `ITSAppUsesNonExemptEncryption` 设为 `false`（应用只使用系统内置的加密，如 HTTPS 时），TestFlight 的出口合规问题就只需回答一次，而不必每个构建都回答。`nsh publish ios` 也会按 `store.ios.usesNonExemptEncryption` 回答这个问题。

`"store"` 中与 App Store 有关的键（商店信息本身见[商店信息](#store-listing)）：

| 键 | 含义 |
|---|---|
| `store.ios.appId` | 应用在 App Store Connect 中的数字 Apple ID。 |
| `store.ios.usesNonExemptEncryption` | 每个构建的出口合规回答。 |
| `store.ios.betaGroups` | 接收每个构建的 TestFlight 测试组。 |
| `store.ios.release` | 审核通过的版本如何上线：`manual`（默认）、`auto` 或 `phased`。 |

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

`nsh publish ios <IPA>` 通过 Apple 的 [build uploads API](https://developer.apple.com/documentation/appstoreconnectapi/build-uploads) 把 IPA 上传到 App Store Connect。把 `.p8` 密钥放在 `~/.appstoreconnect/private_keys/AuthKey_<KEY_ID>.p8`（Apple 自己的工具也在这里查找），在 shell 配置文件中导出一次 issuer ID 和 key ID；`nsh.json` 中有 `store.ios.appId` 时，IPA 就是唯一的参数：

```sh
export NSH_APP_STORE_ISSUER_ID=69a6de70-0000-0000-0000-000000000000
export NSH_APP_STORE_KEY_ID=ABC123DEFG
nsh publish ios .nsh/out/Notes.ipa
```

| 参数 | 环境变量 | 含义 |
|---|---|---|
| `--app-id <ID>` | `NSH_APP_STORE_APP_ID` | App Store Connect 中应用的数字 Apple ID。默认：`store.ios.appId`。 |
| `--issuer-id <ID>` | `NSH_APP_STORE_ISSUER_ID` | 团队 API 密钥的 issuer ID。 |
| `--key-id <ID>` | `NSH_APP_STORE_KEY_ID` | API 密钥的 ID。 |
| `--private-key <PATH>` | `NSH_APP_STORE_PRIVATE_KEY` | `.p8` 私钥文件。默认：依次在 `./private_keys`、`~/.appstoreconnect/private_keys`、`~/.private_keys` 或 `~/private_keys` 中查找 `AuthKey_<KEY_ID>.p8`。 |

参数优先于环境变量，环境变量优先于 `nsh.json`。把密钥放在 shell 配置文件或 CI 的 secrets 中，不要放进 `nsh.json` 或仓库。

这个命令做的事：

1. 从 IPA 的 `Info.plist` 读取 bundle ID、版本号和构建号，并检查 IPA 的目标是 iOS 真机。
2. 用 API 密钥签发一个短期有效的 ES256 令牌，并检查 IPA 的 bundle ID 与 App Store Connect 中的应用一致。
3. 创建一个构建上传，按 Apple 要求的分片流式上传 IPA，提交它的 SHA-256 校验和，然后打印上传 ID 和应用的 TestFlight 页面。
4. 等待 Apple 处理构建（`--timeout` 分钟，默认 60），回答出口合规问题，并把构建分发给 TestFlight 测试组。
5. 创建或复用以 IPA 版本号命名的 App Store 版本，挂上构建，设置发布方式，并写入[商店信息](#store-listing)和截图。
6. **只有**加 `--submit` 时，才把该版本提交 App 审核。

| 参数 | 含义 |
|---|---|
| `--entry <DIR>` | `nsh.json` 中含商店信息的应用（默认：`.`）。 |
| `--no-wait` | Apple 接受上传后即停止。 |
| `--testflight-only` | 处理完成、出口合规和 TestFlight 测试组之后停止。 |
| `--beta-group <NAME>` | 接收构建的 TestFlight 测试组（可重复）。默认：`store.ios.betaGroups`。 |
| `--release manual\|auto\|phased` | 审核通过的版本如何上线（默认：`store.ios.release`，否则为 `manual`）。 |
| `--submit` | 把该版本提交 App 审核。 |

已在审核中或已上架的版本永远不会被修改：请在 `nsh.json` 中提高 `version`。截图按设备类别分组：`phone` 对应 6.9 英寸 iPhone 尺寸（`APP_IPHONE_67`），`tablet` 对应 13 英寸 iPad 尺寸（`APP_IPAD_PRO_3GEN_129`）；`APP_IPHONE_65` 这类 App Store Connect 显示类型原样使用。截图集只在图片变化时才替换。App Store Connect 不接受应用首个版本的更新说明。

### 状态与检查

```sh
nsh publish status ios --json   # exit 0 live, 10 processing/in review, 11 draft, 12 action needed
nsh publish doctor ios          # read-only: credentials, nsh.json, listing, app access
```

`doctor` 只发出读取请求（`--offline` 时不发任何请求），有检查失败时以非零状态退出。两者都可以在任何操作系统上运行。

### 上传之后

在 App Store Connect 中：

- **TestFlight**：处理完成后，构建版本会立即提供给内部测试员（你团队的成员）。外部测试员需要一个群组，并且每个版本的第一个构建都要经过 Beta 版 App 审核（Beta App Review）。
- **`nsh` 不设置的内容**：隐私详情（App 隐私）和年龄分级在 App Store Connect 中填写，每个应用一次。
- **审核**：带 `--submit` 时，该版本会进入 App 审核；不带时，在 App Store Connect 中点击“添加以供审核”和“提交”。审核通过后，按 `store.ios.release`（或 `--release`）指定的方式发布。

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

Google Play 接收 Android App Bundle（`.aab`）。`nsh package --target android --format aab` 不借助 Gradle 构建一个 bundle 并用你的上传密钥签名；`nsh publish android` 通过 Google Play Developer API 上传它，并发布到某个轨道：未用 `--track` 指定时为**内部测试**。它还能把版本从一个轨道推进到另一个轨道、进行分阶段发布，并写入[商店信息](#store-listing)。

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

### 上传到轨道

```sh
export NSH_PLAY_CREDENTIALS=/secure/play-service-account.json
nsh publish doctor android
nsh publish android .nsh/out/Notes.aab
nsh publish android --from internal --track production --rollout 0.1
nsh publish status android --json
```

```sh
nsh publish android <BUNDLE.aab> [--track <TRACK>] [--rollout <FRACTION>] [--status <STATUS>] [--listing] [--draft]
nsh publish android --from <TRACK> --track <TRACK> [--rollout <FRACTION>]
```

| 参数 | 默认值 | 含义 |
|---|---|---|
| `--entry <DIR>` | `.` | 应用的目录或入口，其 `nsh.json` 的 `id` 即包名。 |
| `--package <NAME>` | `nsh.json` `id` | Google Play 上的包名。 |
| `--credentials <KEY.json>` | `NSH_PLAY_CREDENTIALS` | 服务账号的 JSON 密钥。 |
| `--track <TRACK>` | `internal` | `internal`、`alpha`、`beta`、`production` 或自定义轨道名。 |
| `--from <TRACK>` | | 推进：把该轨道最新版本的版本代码发布到 `--track`，不上传任何文件。 |
| `--rollout <FRACTION>` | | 分阶段发布给该轨道这一比例的用户，大于 0 小于 1，例如 `0.1`。 |
| `--status <STATUS>` | `completed` | `completed`、`in-progress`（带 `--rollout` 时的默认值）、`halted`（暂停分阶段发布，需带上其 `--rollout`）或 `draft`。 |
| `--draft` | 关闭 | 以草稿发布；在应用于 Play Console 中完成首次发布之前，Google Play 要求这样做。 |
| `--store <DIR>` | `store` | 商店目录，与 `nsh.json` 同级（[商店信息](#store-listing)）。 |
| `--listing` | 关闭 | 同时按商店信息更新商店详情（名称、说明、图片），覆盖 Play 上这些语言已有的内容。 |

这个命令在 Google Play 上对应用做一次编辑（edit），步骤如下：

1. 以服务账号身份登录。
2. 打开一个编辑，上传 bundle，并读取 Google Play 从中识别出的版本代码。
3. 把轨道的版本设为该版本代码，附上各语言的更新说明；带 `--listing` 时还会设置名称和说明，并对存在的每种图片类型替换 Play 上该类型的图片。
4. 提交编辑。任何一步失败都会删除这次编辑，不会出现发布了一半的情况。

它会打印现在位于该轨道上的版本代码。内部测试员（最多 100 人，来自 Play Console 中的测试员列表）几分钟内就能从 Google Play 收到更新。要扩大分阶段发布，用更大的 `--rollout` 再次发布同一版本；要完成它，不带 `--rollout` 再发布一次。图片类型沿用 Google Play 的：`phoneScreenshots`、`sevenInchScreenshots`、`tenInchScreenshots`、`tvScreenshots`、`wearScreenshots`、`icon`、`featureGraphic` 和 `tvBanner`。

`nsh publish status android` 从一个随后删除的编辑中读取并打印每个轨道的版本（状态、版本代码、放量比例）；`nsh publish doctor android` 检查密钥、包名、商店目录、登录和对该应用的访问权限，不改动 Play 上的任何内容。

### 上传之后

在 Play Console 中：

- **推进发布**：用 `nsh publish android --from internal --track <TRACK>`，或在 Play Console 中：“测试”→“内部测试”→该版本→“推广发布”。新注册的个人开发者账号必须先进行封闭测试，才能获得正式版访问权限。
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
| `Google Play refused the release on the … track` … draft | 应用还没有在 Play Console 中发布过：传入 `--draft`，然后在 Play Console 中发布。 |

## 另请参阅

- [打包](./packaging.md)：各平台的 `nsh package`，以及不经商店的应用分发
- [CLI](./cli.md)：所有命令和参数
- [应用更新](./app-updates.md)：自我更新的应用
- [OTA 更新](./ota-updates.md)：无需发布商店版本即可更新 TSX

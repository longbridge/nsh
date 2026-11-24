---
title: 发布到 App Store
description: "把 NSH 应用交付给 iOS 用户：Apple Developer 准备工作、nsh.json 键、分发签名的 IPA、用 nsh publish ios 上传到 App Store Connect 和 TestFlight、审核以及常见错误。"
order: 19.2
group: publishing
---

# 发布到 App Store

iOS 应用通过 App Store Connect 到达用户：你上传一个构建版本，测试员在 TestFlight 中拿到它，Apple 审核通过后，App Store 就会提供它。`nsh package --target ios` 构建并签名 IPA；`nsh publish ios` 把它上传到 App Store Connect。上传之后的一切（TestFlight 群组、商店页面、审核和发布）都在 App Store Connect 中完成。

两个命令都在装有 Xcode 的 macOS 上运行，Rust、Rust + TSX 和 TSX 应用都一样。

## 首次发布之前

每个应用做一次：

1. **加入 Apple Developer Program**，使用将要发布该应用的账号（个人或组织）。
2. **注册 bundle ID。** 在“证书、标识符和描述文件”（Certificates, Identifiers & Profiles）中添加一个 App ID，其 bundle ID 是 `nsh.json` 中应用的 `id`（`com.example.notes`），并勾选应用用到的功能（推送通知、App Groups 等）。
3. **创建分发证书。** 即 *Apple Distribution* 证书，安装到负责打包的 Mac（或 CI runner）的登录钥匙串中。`security find-identity -v -p codesigning` 会把它列为 `Apple Distribution: Example Inc (ABCDE12345)`。
4. **创建 App Store 描述文件**，对应该 App ID 和证书，并下载 `.mobileprovision` 文件。把它放在仓库之外。
5. **在 App Store Connect 中创建应用**（“我的 App”→“新建 App”），使用同一个 bundle ID。“App 信息”下显示的数字 **Apple ID** 就是 `nsh publish ios` 需要的应用 id。
6. **创建团队 API 密钥**：在 App Store Connect 的“用户和访问”→“集成”→“App Store Connect API”中创建，角色为“开发者”“App 管理”或“管理”（Developer、App Manager 或 Admin）。记下它的 **Key ID** 和列表上方显示的 **Issuer ID**，并下载 `.p8` 私钥；Apple 只允许下载一次。

## 配置应用

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

## 构建 IPA

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

## 上传到 TestFlight

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

## 上传之后

在 App Store Connect 中：

- **TestFlight**：处理完成后，构建版本会立即提供给内部测试员（你团队的成员）。外部测试员需要一个群组，并且每个版本的第一个构建都要经过 Beta 版 App 审核（Beta App Review）。
- **App Store 页面**：名称、副标题、描述、关键词、各设备尺寸的截图、隐私详情（App 隐私）和年龄分级。这些都在 App Store Connect 中填写。
- **审核**：把构建版本添加到该版本，然后点击“添加以供审核”和“提交”。审核通过后，按该版本的发布选项手动发布、自动发布或在指定日期发布。

## 常见错误

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

## 另请参阅

- [发布](./publishing.md)：所有平台、凭据和发布 CI
- [打包：iOS 真机](./packaging.md#ios-devices)
- [iOS](./ios.md)：在 iOS 上运行和嵌入
- [OTA 更新](./ota-updates.md)：在两次商店发布之间为已安装应用更新 TSX

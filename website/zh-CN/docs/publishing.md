---
title: 发布
description: "NSH 应用如何从 nsh package 到达每个平台的用户：应用商店、安装程序、Web 托管、凭据、发布 CI，以及各平台目前支持的内容。"
order: 18.5
group: publishing
---

# 发布

发布是应用能跑起来之后的所有事：构建 release 版本、签名、上架应用商店或放到下载页，并持续更新。[打包](./packaging.md)讲的是第一步，即各平台的 `nsh package`；本组的其余页面讲剩下的部分，每个平台一页：

- [iOS](./publish-ios.md)：App Store Connect 和 TestFlight
- [Android](./publish-android.md)：Google Play
- [macOS](./publish-macos.md)：Developer ID、公证和直接下载
- [Windows](./publish-windows.md)：安装程序、代码签名和直接下载
- [Linux](./publish-linux.md)：归档、桌面条目和直接下载
- [Web](./publish-web.md)：放在任意主机上的静态站点

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

## 另请参阅

- [打包](./packaging.md)：各平台的 `nsh package`
- [CLI](./cli.md)：所有命令和参数
- [应用更新](./app-updates.md)：自我更新的应用
- [OTA 更新](./ota-updates.md)：无需发布商店版本即可更新 TSX

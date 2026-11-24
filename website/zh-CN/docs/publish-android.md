---
title: 发布到 Google Play
description: "把 NSH 应用交付给 Android 用户：Play Console 与服务账号准备、上传密钥、nsh.json 键、Android App Bundle、用 nsh publish android 发布到内部测试轨道、推进发布以及常见错误。"
order: 19.3
group: publishing
---

# 发布到 Google Play

Google Play 接收 Android App Bundle（`.aab`）。`nsh package --target android --format aab` 不借助 Gradle 构建一个 bundle 并用你的上传密钥签名；`nsh publish android` 通过 Google Play Developer API 上传它，并发布到**内部测试**轨道。推进到封闭测试、开放测试或正式版，以及商品详情和审核，都在 Play Console 中完成。

两个命令都可以在 macOS、Linux 和 Windows 上运行。

## 首次发布之前

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

## 配置应用

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

## 构建 bundle

```sh
NSH_KEYSTORE_PASSWORD=… nsh package --target android --format aab \
  --keystore release.jks --key-alias upload
```

这会写出 `.nsh/out/<Name>.aab`，由 JDK 中的 `jarsigner` 签名。它包含 `arm64-v8a` 和 `armeabi-v7a` 的原生代码；Google Play 据此构建每台设备安装的 APK。`NSH_KEYSTORE_PASSWORD` 能让密码不进入 shell 历史（`--keystore-password` 也可以）。不带 `--keystore` 时，bundle 用 debug 密钥签名，Google Play 会拒绝。环境要求（Android SDK、Rust 应用需要的 NDK、JDK）见[打包：Android](./packaging.md#android)。

release bundle 中的 `libnsh.so` 不带符号；完整的库写在运行时旁的 `symbols/<abi>/libnsh.so`。在 Play Console 中上传它们（该 bundle 的“下载内容”→原生调试符号），就能在 Android Vitals 中看懂原生崩溃堆栈。

上传前想在设备上试用 bundle，可以用 [bundletool](https://github.com/google/bundletool)：先 `bundletool build-apks --bundle .nsh/out/Notes.aab --output notes.apks --connected-device`，再 `bundletool install-apks --apks notes.apks`。

## 上传到内部测试轨道

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

## 上传之后

在 Play Console 中：

- **推进发布**：“测试”→“内部测试”→该版本→“推广发布”，推进到封闭测试、开放测试或正式版。新注册的个人开发者账号必须先进行封闭测试，才能获得正式版访问权限。
- **商品详情**：应用名称、简短说明和完整说明、图标、置顶大图、手机和平板截图。
- **应用内容**：隐私权政策、数据安全、广告、内容分级和目标受众，在首次发布正式版之前必须填写。
- **审核与发布**：发布到封闭测试、开放测试和正式版轨道的版本都要经过审核；正式版可以先向一定比例的用户发布，之后再逐步提高比例。

## 常见错误

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

## 另请参阅

- [发布](./publishing.md)：所有平台、凭据和发布 CI
- [打包：Android](./packaging.md#android)
- [Android](./android.md)：在 Android 上运行和嵌入
- [CLI：nsh publish android](./cli.md#nsh-publish-android)

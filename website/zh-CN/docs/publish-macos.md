---
title: 发布 macOS 应用
description: "在 App Store 之外把 NSH 应用交付给 Mac 用户：用 nsh package 进行 Developer ID 签名，用 Apple 的工具公证和装订，制作下载归档，自我更新以及常见错误。"
order: 19.4
group: publishing
---

# 发布 macOS 应用

在 Mac App Store 之外，Mac 应用以已签名并经过公证的 `.app` 分发，用户从你的网站或 GitHub Release 下载。`nsh package --target macos --sign …` 构建应用并用你的 Developer ID 签名；公证、装订（staple）和上传则用 Apple 的命令行工具和你的发布主机完成。之后由[应用更新](./app-updates.md)让已安装的副本保持最新。

`nsh publish` 没有 macOS 商店上传：NSH 目前还不能为 Mac App Store 打包。

## 首次发布之前

每个开发者账号做一次：

1. **加入 Apple Developer Program。**
2. **创建 Developer ID Application 证书**（“证书、标识符和描述文件”→“证书”；只有账号持有人能创建），安装到负责打包的 Mac（或 CI runner）的登录钥匙串中。`security find-identity -v -p codesigning` 会把它列为 `Developer ID Application: Example Inc (ABCDE12345)`。
3. **为 `notarytool` 设置公证凭据**，以一个配置名称在钥匙串中保存一次。使用 App Store Connect 团队 API 密钥（与 [iOS 上传](./publish-ios.md#before-the-first-release)使用的同一种）：

   ```sh
   xcrun notarytool store-credentials notes-notary \
     --key /secure/AuthKey_ABC123DEFG.p8 --key-id ABC123DEFG --issuer 69a6de70-0000-0000-0000-000000000000
   ```

   或使用 Apple ID 和 App 专用密码：`xcrun notarytool store-credentials notes-notary --apple-id
   you@example.com --team-id ABCDE12345`。

## 配置应用

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

## 构建并签名

```sh
nsh package --target macos --arch universal \
  --sign "Developer ID Application: Example Inc (ABCDE12345)"
```

这会写出 `.nsh/out/<Name>.app`。有证书时，NSH 先签名内嵌的 framework 和扩展，再签名应用，并按公证的要求启用 Hardened Runtime（`--options runtime`）和安全时间戳（`--timestamp`），最后用 `codesign --verify --deep --strict` 验证结果。`--arch universal` 能同时在 Apple 芯片和 Intel Mac 上运行（Rust 应用需要 `rustup target add x86_64-apple-darwin`）；省略它则只面向本机的处理器。不带 `--sign` 时应用使用 ad hoc 签名，能在构建它的 Mac 上运行，但在其他 Mac 上会被 Gatekeeper 拦下。

## 公证并装订

Apple 会扫描经过公证的应用是否含有恶意内容并做记录，这样 Gatekeeper 打开它时就不会警告。提交应用的 zip，等待结果，再把票据装订到应用上，让它离线也能打开：

```sh
cd .nsh/out
ditto -c -k --keepParent Notes.app Notes-notarize.zip
xcrun notarytool submit Notes-notarize.zip --keychain-profile notes-notary --wait
xcrun stapler staple Notes.app
spctl --assess --type execute --verbose Notes.app   # "accepted, source=Notarized Developer ID"
```

如果提交结果为 `Invalid`，`xcrun notarytool log <submission-id> --keychain-profile notes-notary` 会列出所有问题。

## 分发

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

## 常见错误

| 问题 | 处理方法 |
|---|---|
| `codesign` 找不到签名身份 | 证书不在构建使用的钥匙串中，或缺少私钥：导入连同私钥一起导出的 `.p12`。 |
| 在 CI 上签名时出现 `errSecInternalComponent` | 钥匙串已锁定：解锁它（`security unlock-keychain`），并允许 `codesign` 使用该密钥（`security set-key-partition-list`）。 |
| 公证 `Invalid`：未使用有效的 Developer ID 证书签名 | 应用使用了 ad hoc 签名或 Apple Development 证书：用 `--sign "Developer ID Application: …"` 打包。 |
| 公证 `Invalid`：签名不包含安全时间戳 | 通过会自动添加时间戳的 `nsh package --sign` 签名；不要在不带 `--timestamp` 的情况下手动重新签名。 |
| 下载后提示“Notes 已损坏，无法打开” | 归档丢失了签名（用会丢弃属性的工具压缩，或签名后被修改）：装订之后用 `ditto` 或 `tar` 归档。 |
| 公证成功了 Gatekeeper 仍然警告 | 票据没有装订而 Mac 处于离线状态，或下载的应用不是经过公证的那个：先装订，再重新归档。 |
| 应用无法使用摄像头、麦克风或其他受保护资源 | 在 `macos.infoPlist` 中添加用途说明，并在 `macos.entitlements` 中添加 Hardened Runtime 例外项。 |

## 另请参阅

- [发布](./publishing.md)：所有平台、凭据和发布 CI
- [打包：macOS](./packaging.md#macos)
- [macOS](./macos.md)：在 macOS 上运行和嵌入
- [应用更新](./app-updates.md)：自我更新的应用

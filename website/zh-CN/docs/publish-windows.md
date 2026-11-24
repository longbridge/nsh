---
title: 发布 Windows 应用
description: "把 NSH 应用交付给 Windows 用户：nsh package 生成的按用户安装的 Inno Setup 安装程序和 zip、用 signtool 进行代码签名、SmartScreen、下载托管、自我更新以及常见错误。"
order: 19.5
group: publishing
---

# 发布 Windows 应用

`nsh package --target windows` 写出直接下载发布所需的一切：应用目录、它的 `.zip`，以及用 Inno Setup 构建的按用户安装的安装程序。你用自己的代码签名证书签名可执行文件，再上传到你的网站或 GitHub Release；[应用更新](./app-updates.md)让已安装的副本保持最新。

`nsh publish` 没有 Windows 商店上传：NSH 目前还不能为 Microsoft Store 打包 MSIX。

## 首次发布之前

在负责打包的 Windows 机器（或 CI runner）上：

1. **安装工具。** TSX 应用不需要任何工具；Rust 应用的 runner 用 Visual Studio Build Tools 构建。安装程序需要 Inno Setup 6：`winget install JRSoftware.InnoSetup`。
2. **获取代码签名证书**：从证书颁发机构购买，或使用 Azure Trusted Signing 这样的云签名服务。未签名的可执行文件照样能运行，但 SmartScreen 会提示发布者未知，一些杀毒软件也会对它们抱有怀疑。
3. **安装 Windows SDK** 以获得 `signtool.exe`（它随 Visual Studio 安装程序的 Windows SDK 组件提供）。

## 配置应用

| 键 | 在构建中 | 说明 |
|---|---|---|
| `id` | 安装程序的 AppId、开始菜单快捷方式的 AppUserModelID | 用于在任务栏中归组应用的窗口，并在通知中标识应用；相同 id 的新版安装程序会升级已安装的应用。保持不变。 |
| `name` | `<Name>.exe`、安装目录和开始菜单项 | |
| `version` | 安装程序的版本和“已安装的应用”中的条目 | 必须是 semver，并且要递增才能[自我更新](./app-updates.md)。 |
| `icon`（或 `icon.windows`） | 嵌入 `<Name>.exe`，并写为 `nsh\icon.ico` | 在 Windows 上打包：只有 Windows 能嵌入图标（[图标](./packaging.md#icons)）。 |

## 构建

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

## 签名

先签名应用的可执行文件，再构建安装程序，这样安装后的应用也是已签名的；然后签名安装程序。证书位于 Windows 证书存储中时：

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /n "Example Inc" .nsh\out\Notes\Notes.exe
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" .nsh\out\Notes.iss
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /n "Example Inc" .nsh\out\Notes-setup.exe
signtool verify /pa .nsh\out\Notes-setup.exe
```

`ISCC.exe` 用 `nsh package` 写出的脚本重新编译安装程序，这次包含已签名的可执行文件；它的路径取决于 Inno Setup 的安装位置（为整台机器安装时是 `C:\Program Files (x86)\Inno Setup 6\ISCC.exe`）。如果你也发布 `.zip`，同样要从已签名的目录重新制作。始终加上时间戳（`/tr`）：这样证书过期后签名仍然有效。

## 分发

把 `Notes-setup.exe`（以及给偏好便携版的用户准备的 `.zip`）上传到你的网站或 GitHub Release。

`nsh.json` 中有 [`"updater"`](./app-updates.md#set-the-source) 时，`nsh package` 还会写出用于自我更新的 `<Name>-<version>-windows-<arch>.zip`。把已签名的应用放进去，再与其他平台的归档一起写出清单（[应用更新](./app-updates.md#publish-a-release)）：

```powershell
nsh update-manifest *.zip *.tar.gz --notes CHANGES.md --key keys\update.secret
```

## SmartScreen

SmartScreen 根据信誉做判断，信誉随着人们下载和运行同一证书签名的文件而增长。新证书或新类型的证书，即使文件已签名，也可能在一段时间内显示“Windows 已保护你的电脑”警告。每次发布都用同一个证书签名，信誉才能延续下去。

## 常见错误

| 问题 | 处理方法 |
|---|---|
| `.nsh\out` 中没有 `<Name>-setup.exe` | 没有安装 Inno Setup 6，或不在 `nsh package` 查找的位置：用 `winget install JRSoftware.InnoSetup` 安装，或用 `ISCC.exe` 编译 `.nsh\out\<Name>.iss`。 |
| 可执行文件显示默认图标 | 它是在 macOS 或 Linux 上打包的：在 Windows 上打包才能嵌入图标。 |
| 无法识别 `signtool` | 在 Developer PowerShell 中运行，或把 Windows SDK 的 `bin\<version>\x64` 目录加入 `PATH`。 |
| `SignTool Error: No certificates were found that met all the given criteria` | 证书不在 `signtool` 搜索的存储中，或 `/n` 与证书的使用者不匹配：用 `certutil -user -store My` 列出证书。 |
| 已签名的安装程序显示“Windows 已保护你的电脑” | 该证书的 SmartScreen 信誉仍在积累中（见上文）。 |
| 安装程序要求关闭应用 | 它在原位升级，会先关闭正在运行的副本；这是预期行为。 |

## 另请参阅

- [发布](./publishing.md)：所有平台、凭据和发布 CI
- [打包：Windows](./packaging.md#windows)
- [Windows](./windows.md)：在 Windows 上运行和嵌入
- [应用更新](./app-updates.md)：自我更新的应用

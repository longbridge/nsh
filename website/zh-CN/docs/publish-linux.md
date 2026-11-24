---
title: 发布 Linux 应用
description: "把 NSH 应用交付给 Linux 用户：nsh package 生成的目录和 tar.gz、桌面条目与图标、运行时依赖、安装脚本、自我更新以及常见错误。"
order: 19.6
group: publishing
---

# 发布 Linux 应用

`nsh package --target linux` 把应用写成一个目录及其 `.tar.gz`，附带桌面条目和图标。你把归档上传到你的网站或 GitHub Release，用户解压并安装桌面条目；[应用更新](./app-updates.md)让已安装的副本保持最新。

`nsh publish` 没有 Linux 商店上传，`nsh package` 目前也还不能写出 Flatpak、AppImage、`.deb` 或 `.rpm` 包。

## 首次发布之前

在 Linux 上打包：runner 是为构建它的机器构建的，所以要在你要发布的处理器架构（`x86_64` 或 `aarch64`）上打包，并且使用对你支持的用户来说足够旧的发行版，因为比用户更新的 glibc 会让应用无法启动。Ubuntu 24.04 是面向当前桌面系统的不错选择。构建需要 GTK 4（4.14 或更高）、Pango 和 WebKitGTK 6.0 的开发包（[Linux：环境要求](./linux.md#requirements)）。

## 配置应用

| 键 | 在包中 | 说明 |
|---|---|---|
| `id` | `<id>.desktop`、`StartupWMClass`、GTK 应用 id、`<id>.png` | 桌面环境靠它把应用的窗口和启动器对应起来；使用反向 DNS 形式，如 `com.example.notes`。保持不变。 |
| `name` | 可执行文件 `<Name>`、目录和桌面条目的 `Name` | |
| `version` | 自我更新归档 | 必须是 semver，并且要递增才能[自我更新](./app-updates.md)。 |
| `icon`（或 `icon.linux`） | `<id>.png`，由桌面条目的 `Icon` 指定 | 一张铺满画布的正方形 PNG（[图标](./packaging.md#icons)）。 |

## 构建

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

## 分发

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

## 常见错误

| 问题 | 处理方法 |
|---|---|
| `error while loading shared libraries: libgtk-4.so.1`（或 `libwebkitgtk-6.0.so.4`） | 安装上文列出的运行时库。 |
| `version 'GLIBC_2.39' not found` | 应用在比用户更新的发行版上打包：换一个较旧的发行版打包。 |
| 应用没有出现在启动器中 | 桌面条目不在 `~/.local/share/applications` 中，或它的 `Exec` 找不到可执行文件：把可执行文件链接到 `PATH` 上。 |
| 启动器显示通用图标，或应用运行时出现第二个图标 | 图标不在图标主题目录中，或桌面条目被改了名：保持文件名为 `<id>.desktop`，`StartupWMClass` 与它对应。 |
| `exec format error` | 归档是为另一种处理器构建的：在 `x86_64` 和 `aarch64` 上分别打包。 |

## 另请参阅

- [发布](./publishing.md)：所有平台、凭据和发布 CI
- [打包：Linux](./packaging.md#linux)
- [Linux](./linux.md)：在 Linux 上运行和嵌入
- [应用更新](./app-updates.md)：自我更新的应用

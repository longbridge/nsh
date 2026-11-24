---
title: 发布
description: "把 NSH 应用发布到应用商店：Linux 上的 Flathub 和 Windows 上的 Microsoft Store，应用标识与文案来自 nsh.json，使用 nsh publish、status 和 doctor。"
order: 21
group: delivery
---

# 发布

`nsh publish <platform>` 把应用发布到该平台的应用商店，应用标识和商店文案都来自 `nsh.json`。下面每个商店一节。生成要分发的文件
（压缩包、安装程序、AppImage、Web 构建产物）属于[打包](./packaging.md)；没有应用商店的平台（例如 Web）只在打包文档中介绍。

## Linux

Linux 上的商店是 Flathub：用户通过 GNOME Software、KDE Discover 或 `flatpak install flathub <id>` 安装。Flathub 根据
manifest 自行构建并签名应用；NSH 根据发布压缩包生成 manifest，由你提交 Pull Request。压缩包、AppImage 和本地 Flatpak
包属于打包，不是商店发布，见[打包](./packaging.md)。

### 准备工作

- 应用能在 Linux 上打包：`nsh package --target linux` 可以正常运行（见 [Linux](./linux.md#packaging)）。
- `nsh.json` 有反向域名形式的 `id`。在 Flathub 上它必须是你控制的域名，GitHub 项目可用 `io.github.<user>.<App>`。
- `nsh.json` 的 `icon` 是正方形图标，最好不小于 256×256：NSH 按 256×256 写出，需要放大时 `doctor` 会给出警告。
- 可选，供 `doctor` 使用：`appstreamcli` 和 `desktop-file-validate`（软件包 `appstream` 与 `desktop-file-utils`）。

`nsh publish doctor linux` 会逐项报告以上内容。

### 配置

```json
{
  "name": "Counter",
  "id": "io.github.example.Counter",
  "version": "1.2.0",
  "icon": "icon.png",
  "store": {
    "locales": {
      "en-US": {
        "subtitle": "Count things up and down",
        "description": "A small counter.\n\nParagraphs are separated by blank lines.",
        "keywords": "count,tally",
        "marketingUrl": "https://example.com"
      }
    },
    "linux": {
      "license": "MIT",
      "developer": "Example",
      "categories": ["Utility"],
      "screenshots": ["https://example.com/counter.png"],
      "archiveUrl": "https://github.com/example/counter/releases/download/v{version}/Counter.tar.gz"
    }
  },
  "linux": {
    "flatpak": {
      "runtimeVersion": "49",
      "finishArgs": ["--share=ipc", "--socket=wayland", "--socket=fallback-x11", "--device=dri"]
    }
  }
}
```

| 字段 | 用途 |
|---|---|
| `name` | 可执行文件、desktop 文件的 `Name` 以及各文件名 |
| `id` | desktop、metainfo 与 manifest 的文件名（`<id>.desktop`、`<id>.metainfo.xml`、`<id>.yml`）以及 `StartupWMClass` |
| `version` | metainfo 中的版本，也是 `status` 与 Flathub 比较的版本 |
| `icon` | 256×256 的 `<id>.png` |
| `store.locales.en-US.subtitle` | 一行 AppStream 摘要。Flathub 要求不超过 35 个字符、结尾不带句号 |
| `store.locales.en-US.description` | 详细描述，空行分段 |
| `store.locales.en-US.keywords` | 搜索关键词，逗号分隔（也可写成数组） |
| `store.locales.en-US.marketingUrl` | metainfo 的 `homepage` 链接，除非设置了 `store.linux.homepage` |
| `store.linux.license` | `project_license`，SPDX 表达式 |
| `store.linux.developer` | 软件中心显示的开发者名称 |
| `store.linux.categories` | desktop 文件与 metainfo 的 freedesktop 分类，默认 `Utility` |
| `store.linux.screenshots` | 截图 URL，第一张为默认截图。Flathub 至少需要一张 |
| `store.linux.archiveUrl` | 每个版本压缩包的下载地址，`{version}` 替换为 `nsh.json` 的 `version` |
| `linux.flatpak.runtimeVersion` | GNOME runtime 版本，默认为 NSH 测试所用的版本 |
| `linux.flatpak.finishArgs` | 替换沙盒权限。默认：Wayland（X11 兜底）、IPC、GPU 和网络 |

`store` 由 `nsh publish` 的所有商店共用，只需填写一次（见[商店信息](./packaging.md#store-listing)）。文案取 `en-US`
（没有时取第一个语言），也可以写在 `store/en-US/` 下的文件里（`subtitle.txt`、`description.txt`、`keywords.txt`）。Flathub
用 `id` 命名应用。

### Flathub

Flathub 不接受上传。每个应用是一个 GitHub 仓库 `flathub/<id>`，其中的 manifest 写明发布压缩包的 URL 与 SHA-256；合入该仓库的每次
改动都由 Flathub 构建并发布。

#### 首次发布

1. 运行 `nsh publish doctor linux`，直到全部通过。
2. 运行 `nsh package --target linux`，把 `.nsh/out/<Name>.tar.gz` 上传到地址固定的位置，例如 GitHub Release 的附件。
3. 写出 manifest：

   ```sh
   nsh publish linux .nsh/out/Counter.tar.gz
   ```

   压缩包地址为填入版本号后的 `store.linux.archiveUrl`（或 `--url`），输出 `.nsh/out/flathub/<id>/<id>.yml`。
4. 按 [Flathub 提交指南](https://docs.flathub.org/docs/for-app-authors/submission)向 `flathub/flathub` 提交带 manifest
   的 Pull Request。Flathub 审核后会创建 `flathub/<id>`。

#### 后续版本

提高 `version`、重新打包并上传新压缩包，然后用 `--out` 指向你检出的 `flathub/<id>`：

```sh
nsh publish linux .nsh/out/Counter.tar.gz --out ../flathub-io.github.example.Counter
```

manifest 已存在时，只更新标记为 `# nsh: release archive` 的压缩包来源（`url` 与 `sha256`），其余内容保持你或 Flathub
审核者修改后的样子。之后自行提交并在 `flathub/<id>` 上开 Pull Request，NSH 不会替你开 Pull Request。

#### 状态

```sh
nsh publish status linux [--json]
```

显示 Flathub 上列出的版本，与 `nsh.json` 的 `version` 一致前退出码为 1，CI 可以据此等待版本上线。尚未上架 Flathub 的应用会如实报告。

#### 检查

```sh
nsh publish doctor linux
```

检查 `id`、Flathub 要求的商店字段（摘要长度、许可证、截图）、图标是否为正方形（小于 256×256 时警告）；安装了相应工具时，用
`appstreamcli` 校验生成的 metainfo，用 `desktop-file-validate` 校验 desktop 文件。缺少 Flathub 必需的内容时以非零退出。

### 命令

```sh
nsh publish linux <ARCHIVE.tar.gz> [--url <URL>] [--entry <DIR>] [--out <DIR>]
nsh publish status linux [--entry <DIR>] [--json]
nsh publish doctor linux [--entry <DIR>]
```

| 参数 | 说明 |
|---|---|
| `--url <URL>` | 压缩包的下载地址（HTTPS）。默认：`store.linux.archiveUrl` |
| `--entry <DIR>` | 应用目录或入口，默认 `.` |
| `--out <DIR>` | `<id>.yml` 的输出目录，默认 `.nsh/out/flathub/<id>` |
| `--json` | 仅 `status`：输出机器可读格式 |

### 常见问题

| 现象 | 原因与解决 |
|---|---|
| `doctor`：摘要太长 | Flathub 允许 35 个字符，缩短 `en-US` 的 `subtitle` |
| `doctor`：没有截图 | 在 `store.linux.screenshots` 中至少加一个 URL |
| Flathub 构建因架构失败 | 压缩包是为本机处理器构建的（`only-arches`），需为每种架构构建并加入压缩包 |
| `nsh publish linux` 要求 `--url` | 设置 `store.linux.archiveUrl` |
| `status` 显示未上架 Flathub | 首次提交尚未合入，或 `id` 与 Flathub 仓库名不一致 |

### 未覆盖的内容

- 不支持发布到 Snap Store 或 AUR。
- 不会上传到 Flathub 或开 Pull Request：Flathub 根据你提交的 manifest 构建。

## Windows

Windows 上的商店是 Microsoft Store。`nsh publish windows` 通过 Store 提交 API 把 MSIX 包提交到 Store，由 Store 认证、签名并发布。

### 准备工作

- 已在 [Partner Center](https://partner.microsoft.com/dashboard) 保留应用名，并在那里完成首次提交（定价、年龄分级、商店文案）。之后的版本通过
  `nsh publish windows` 发布。
- 一个 MSIX 包：`nsh package --target windows --format msix`，不签名（见[打包](./packaging.md#msix)）。Linux 和 macOS 上也能生成。
- 一个关联到 Partner Center 的 Azure AD 应用（Account settings → User management → Azure AD applications，角色 Manager）。凭据只从环境变量读取：
  `NSH_MS_TENANT_ID`、`NSH_MS_CLIENT_ID` 和 `NSH_MS_CLIENT_SECRET`。

### 配置

```json
{
  "id": "com.example.notes",
  "version": "1.2.0",
  "windows": {
    "identityName": "12345Example.MyApp",
    "publisher": "CN=0A1B2C3D-1234-5678-9ABC-0123456789AB",
    "publisherDisplayName": "Example Inc."
  },
  "store": {
    "locales": { "en-US": { "releaseNotes": "Faster sync." } },
    "windows": { "productId": "9NBLGGH4R315" }
  }
}
```

| 字段 | 用途 |
|---|---|
| `store.windows.productId` | 应用的 Store ID（Partner Center → 应用 → Product identity），也接受 `storeId` |
| `windows.identityName`、`windows.publisher` | 包标识，从同一页面复制；包标识不一致时 Store 会拒绝 |
| `version` | 包版本，第四位为 0 |
| `store.locales.<tag>.releaseNotes` | 该语言商店页面的更新说明（或 `store/<tag>/release_notes.txt`） |

### 发布

```sh
nsh package --target windows --format msix
nsh publish windows .nsh/out/MyApp.msix --wait
```

以最近一次已发布的提交为基础新建提交，替换其中的包，设置各语言的更新说明，上传包并提交认证。提交认证前失败的提交会被删除。应用有待处理的提交
（Partner Center 中的草稿）时会拒绝执行，不会覆盖别人已开始的工作。`--wait` 会一直轮询到认证结束，Store 拒绝时以失败退出。

### 状态与检查

```sh
nsh publish status windows [--json]   # 退出码：0 已发布，2 进行中，1 失败
nsh publish doctor windows            # 只读：nsh.json、凭据、Store ID
```

### 命令

```sh
nsh publish windows <APP.msix> [--entry <DIR>] [--product-id <ID>] [--notes <FILE>|<LANG>=<FILE>]... [--wait]
nsh publish status windows [--entry <DIR>] [--product-id <ID>] [--submission <ID>] [--json]
nsh publish doctor windows [--entry <DIR>] [--product-id <ID>]
```

| 参数 | 说明 |
|---|---|
| `--product-id <ID>` | Store ID。默认：`store.windows.productId` |
| `--notes <FILE>` 或 `<LANG>=<FILE>` | 所有语言或某个语言的更新说明。默认：商店信息中的 `releaseNotes` |
| `--wait` | 等到认证结束 |
| `--submission <ID>` | 仅 `status`：查看最近一次之外的某个提交 |
| `--entry <DIR>` | 应用目录或入口，默认 `.` |

### 常见问题

| 现象 | 原因与解决 |
|---|---|
| `doctor`：缺少凭据 | 设置 `NSH_MS_TENANT_ID`、`NSH_MS_CLIENT_ID` 和 `NSH_MS_CLIENT_SECRET` |
| 被拒绝：存在待处理的提交 | 在 Partner Center 完成或删除草稿后再发布 |
| 认证因包标识失败 | 从 Partner Center 的 Product identity 复制 `identityName` 和 `publisher` |
| 认证因版本失败 | 提高 `version`：每次提交都需要更高的版本 |

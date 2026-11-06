---
title: nsh/updater
description: "打包后的应用自我更新：向 nsh.json 指定的来源检查新版本，带进度下载压缩包，替换应用并重启。"
order: 12.7
---

# nsh/updater

`nsh/updater` 为打包后的应用检查新版本、下载并替换应用。更新从哪里来由 [`nsh.json` 的 `"updater"`](#nshjson) 决定，调用模块的代码无法指定。每一步都在 Rust 中校验：清单签名、应用 id、版本更新、压缩包的大小和 SHA-256。界面由你自己画，模块负责返回状态和完成工作。从设置来源到发布版本的完整流程见[应用更新](../docs/app-updates.md)。

```tsx
import { checkForUpdate, downloadUpdate, installAndRestart } from "nsh/updater";
```

这个模块在所有平台上都可以导入：

| 平台 | `checkForUpdate` | `downloadUpdate`、`installAndRestart` |
| --- | --- | --- |
| macOS、Windows、Linux（打包后） | 检查来源 | 下载并替换应用 |
| iOS、Android | 检查来源；`url` 是应用商店链接（`nsh update-manifest --link`） | 拒绝：引导用户打开 `url` |
| Web、开发运行、没有 `"updater"` 的应用 | 返回 `null` | 拒绝 |

它不需要任何能力声明，`capabilities.network` 对它不生效，因为应用代码无法决定这些请求发往哪里。请求走平台自己的网络栈，和 `fetch` 一样。

## checkForUpdate

```ts
function checkForUpdate(): Promise<AppUpdate | null>;

interface AppUpdate {
  version: string;
  notes?: string | null;
  url: string;
  canInstall: boolean;
}
```

从 `nsh.json` 中的来源获取清单，返回其中描述的新版本；应用已是最新、没有 `"updater"`，或运行在不更新的环境（Web、`nsh run`）时返回 `null`。

| 字段 | 含义 |
| --- | --- |
| `version` | 新版本号，semver |
| `notes` | 更新说明，即清单的 `--notes` 文件内容 |
| `url` | 用户手动获取的地址：压缩包，或 iOS、Android 上的商店页面。`canInstall` 为 false 时交给 [`openUrl`](./nsh.md#openurl) |
| `canInstall` | 本次运行中 `downloadUpdate` 和 `installAndRestart` 是否可用：装在用户可写位置的桌面打包应用为 true。iOS、Android 上为 false，装在只有系统可写的目录（系统包）时也为 false |

来源无法访问或清单不可接受时会 reject：设置了 `publicKey` 但签名错误或缺失、清单属于其他应用、URL 不是 HTTPS。版本相同或更旧的清单不算更新，因此过期的清单无法让应用降级。

## downloadUpdate

```ts
function downloadUpdate(onProgress?: ((progress: UpdateProgress) => void) | null): Promise<void>;

interface UpdateProgress {
  received: number;
  total?: number | null;
}
```

把上一次 `checkForUpdate` 找到的更新压缩包下载到应用的数据目录，收到数据时调用 `onProgress`（服务器未给出大小时没有 `total`），然后按清单校验大小和 SHA-256。没有找到更新或 `canInstall` 为 false 时以 `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` reject；下载失败或校验不符时也会 reject。

```tsx
const progress = signal(0);
await downloadUpdate(({ received, total }) => progress.set(total ? received / total : 0));
```

## installAndRestart

```ts
function installAndRestart(): Promise<void>;
```

把下载好的压缩包解压到应用旁边，校验新应用（id、可执行文件，macOS 上还有代码签名），替换进去，启动新应用并结束当前进程。Promise 只在失败时才会结束，失败时已安装的应用保持原样：没有下载过更新或应用无法替换时 reject。调用前先保存用户会丢失的内容。

Linux 和 macOS 上整个目录或 `.app` 一起替换，保留用户给它起的名字；Windows 上逐个文件替换，有一个失败就全部恢复。

## 进行中的调用

同一时间只运行一个检查、下载或安装：其中一个进行时再调用任何一个都会 reject。进行期间请禁用按钮。

```tsx
import { openUrl, signal } from "nsh";
import { checkForUpdate, downloadUpdate, installAndRestart, type AppUpdate } from "nsh/updater";

const update = signal<AppUpdate | null>(null);
const busy = signal(false);

async function run<T>(work: () => Promise<T>) {
  busy.set(true);
  try {
    return await work();
  } finally {
    busy.set(false);
  }
}

export const check = () => run(async () => update.set(await checkForUpdate()));

export const install = () =>
  run(async () => {
    const found = update();
    if (!found) return;
    if (!found.canInstall) return openUrl(found.url);
    await downloadUpdate();
    await installAndRestart();
  });
```

## nsh.json

`"updater"` 指定一个来源：`github`，即最新 GitHub Release 中带有 `latest.json` 的 `owner/repo`；或 `url`，即自己服务器上的清单，此时必须配 `publicKey`。`publicKey` 是 `nsh keygen` 生成的 `.pub` 文件内容；配了它，未签名的清单会被拒绝。

```json
{
  "id": "com.example.notes",
  "version": "1.2.0",
  "updater": { "github": "example/notes", "publicKey": "<keys/update.pub 的内容>" }
}
```

| 键 | 类型 | 含义 |
| --- | --- | --- |
| `github` | string | `owner/repo`：清单是其最新 Release 的附件 `latest.json`，跳过 draft 和 prerelease |
| `url` | string | 自己服务器上清单的 HTTPS 地址；压缩包的相对地址相对它解析 |
| `publicKey` | string | 给清单签名（`latest.json.sig`）的 Ed25519 公钥。使用 `url` 时必填 |

`nsh package` 会校验这个键并把它放进包里，所以来源在构建应用时就已确定。`version` 必须是 semver。`nsh update-manifest` 为一个版本的压缩包生成 `latest.json` 和签名（[CLI](../docs/cli.md#nsh-update-manifest)）。

## Rust

Rust 应用（`nsh.json` 的 `"root"`）直接使用 `nsh-updater` crate，并自带 HTTP 客户端：`Config::check` 校验清单，`Download` 校验压缩包，`Installation::install_and_launch` 替换应用并启动，启动失败时回滚（[应用更新](../docs/app-updates.md#rust-应用)）。

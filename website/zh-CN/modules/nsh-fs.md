---
title: nsh/fs
description: "FileSystem：在能力授权的原生目录中异步读写 UTF-8 文件。"
order: 8.8
---

`nsh/fs` 为 Native Shell 应用提供经过能力检查的原生文本文件操作。磁盘操作在工作线程执行，结果以 Promise 返回。

```ts
import { FileSystem, isAvailable } from "nsh/fs";

const FileSystem: {
  appDataDir(): Promise<string>;
  readText(path: string): Promise<string>;
  writeText(path: string, text: string, options?: { exclusive?: boolean }): Promise<void>;
  mkdir(path: string, options?: { recursive?: boolean }): Promise<void>;
  exists(path: string): Promise<boolean>;
  rename(from: string, to: string): Promise<void>;
  remove(path: string): Promise<void>;
};
```

## 平台与授权

`isAvailable` 表示宿主是否提供此模块的原生文件支持。macOS 和 Linux 桌面宿主（包括这两个平台的 GPUI 宿主）提供此能力；应用应先检查该值，再进行文件操作。

在 `nsh.json` 中分别声明读写根目录：

```json
{
  "capabilities": {
    "fs": {
      "read": ["${dataDir}"],
      "write": ["${dataDir}"]
    }
  }
}
```

`${dataDir}` 来自宿主为当前应用指定的数据目录。`appDataDir` 也需要对此目录的读取授权。根目录可以是绝对路径或 `${dataDir}` 下的路径；嵌入宿主可以进一步缩小授权范围。每次操作都检查声明和宿主授权，拒绝路径穿越和符号链接越界。进程与文件能力在独立应用中也需要显式声明。

## 操作

| 方法 | 行为与授权 |
|---|---|
| `appDataDir` | 返回当前应用的数据目录；需要读取授权 |
| `readText` | 读取 UTF-8 文本；需要读取授权 |
| `writeText` | 创建或替换文件并同步到磁盘；需要写入授权；`exclusive: true` 拒绝覆盖已有文件 |
| `mkdir` | 创建目录；`recursive: true` 创建缺失的父目录；需要写入授权 |
| `exists` | 路径存在时返回 `true`，缺失时返回 `false`；需要读取授权 |
| `rename` | 重命名文件，在文件系统支持时原子替换目标；源与目标都需要写入授权 |
| `remove` | 删除一个文件；需要写入授权 |

`writeText` 要求父目录存在。`remove` 接受文件路径；目录使用 `mkdir` 创建。读取和写入的 UTF-8 数据每次最多 16 MiB，每个 VM 最多同时有 64 个待完成操作。需要顺序的操作应使用 `await` 连接。

权限、I/O 和配额错误会拒绝 Promise。读取缺失文件的错误消息包含 `ENOENT`；无效 UTF-8 包含 `EILSEQ`；超出大小限制包含 `EFBIG`。`exists` 只把文件缺失转换为 `false`，其他错误仍会拒绝。

## 从 TSX 保存草稿

下面的应用使用上面的能力声明：

```tsx
import { mount, signal, Text, View } from "nsh";
import { FileSystem, isAvailable } from "nsh/fs";

const status = signal("Ready");
async function save() {
  try {
    if (!isAvailable) {
      status.set("Native files require a supported host");
      return;
    }
    const dir = await FileSystem.appDataDir();
    await FileSystem.mkdir(dir, { recursive: true });
    await FileSystem.writeText(`${dir}/draft.txt`, "Hello from TSX");
    status.set(await FileSystem.readText(`${dir}/draft.txt`));
  } catch (error) {
    status.set(String(error));
  }
}
mount(() => <View onPress={save}><Text>{status()}</Text></View>);
```

键值设置可使用 [nsh/storage](./nsh-storage.md)。

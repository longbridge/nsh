---
title: nsh/storage
description: Storage：在设备上按键保存字符串，下次启动时仍在。
order: 8.6
demo: mod-storage
---

# nsh/storage

存储 API 把应用的数据保存在设备上：以字符串为键、字符串为值，应用下次启动时数据仍在。适合保存设置、草稿、用户上次打开的标签页、服务器返回内容的缓存——任何需要比应用进程活得更久的状态。Rust 代码使用 `nsh_core::storage::Storage`，TSX 从 `nsh/storage` 导入 `Storage`；两者读写同一份数据，因此同时有 Rust 入口和 TSX 入口的应用看到的是同一份数据。

```rust tabs
use nsh_core::storage::Storage;
```

```tsx tabs
import { Storage } from "nsh/storage";
```

```ts
const Storage: {
  getItem(key: string): Promise<string | null>;
  setItem(key: string, value: string): Promise<void>;
  removeItem(key: string): Promise<void>;
  keys(): Promise<string[]>;
  clear(): Promise<void>;
};
```

| TS | Rust（`nsh_core::storage::Storage`） |
|---|---|
| `await Storage.getItem(key)` | `Storage::get_item(key) -> Option<String>` |
| `await Storage.setItem(key, value)` | `Storage::set_item(key, value) -> Result<(), StorageFull>` |
| `await Storage.removeItem(key)` | `Storage::remove_item(key)` |
| `await Storage.keys()` | `Storage::get_all_keys() -> Vec<String>` |
| `await Storage.clear()` | `Storage::clear()` |

Rust 函数是同步的，因为数据就在内存中：在组件运行的地方（组件函数体、事件处理函数和任务）调用它们。

每个应用有自己的存储，键和值合计最多 5 MB。调用按应用发起的顺序执行，所以在 `setItem` 之后调用 `getItem` 无需等待写入就能读到新值。读取来自内存；写入在后台成批落盘。

| 平台 | 数据保存位置 |
|---|---|
| iOS | 应用容器内的 `Library/Application Support/nsh/<app id>/storage.json` |
| Android | 应用私有目录内的 `files/nsh/<app id>/storage.json` |
| macOS | `~/Library/Application Support/<app id>/storage.json` |
| Windows | `%APPDATA%\<app id>\storage.json` |
| Linux | `$XDG_DATA_HOME/<app id>/storage.json`（`~/.local/share/…`） |
| Web | 浏览器的 `localStorage`，键以 app id 命名（`nsh:com.example.todo:…`） |

损坏的 `storage.json` 会被保留为同目录下的 `storage.json.corrupt`，应用从空存储开始；读不了的文件则让应用的改动只留在内存中，文件本身永远不会被覆盖。

app id 是 `nsh.json` 中的 `id`，它随应用一起传递：`nsh build` 和 `nsh package` 把它写在 bundle 旁边，开发服务器会发送它，签名包则带有经发布密钥证书担保的 id。因此无论应用在哪里运行，都无需任何设置就各自保存自己的数据；没有 id 的应用保存在 `default` 下。宿主应用可以另选位置（见[在 iOS 中嵌入](../docs/ios.md)和[在 Android 中嵌入](../docs/android.md)）。

## Storage.getItem

```ts
getItem(key: string): Promise<string | null>;
```

返回 `key` 的值，没有值时返回 `null`。Rust：`Storage::get_item(key)`，没有值时为 `None`。

```rust tabs
let theme = Storage::get_item("theme").unwrap_or_else(|| "system".into());
```

```tsx tabs
const theme = (await Storage.getItem("theme")) ?? "system";
```

## Storage.setItem

```ts
setItem(key: string, value: string): Promise<void>;
```

把 `key` 设为 `value`，替换原有的值。如果存储会超过 5 MB，则 Promise 被拒绝，值不会被保存。Rust：`Storage::set_item(key, value)`，此时返回 `Err(StorageFull)`。值都是字符串：结构化数据保存为 JSON（Rust 用 `serde_json::to_string`，TSX 用 `JSON.stringify`），再读回（`serde_json::from_str`、`JSON.parse`）。

```rust tabs
let draft = serde_json::json!({ "title": title.get(), "body": body.get() });
if let Err(full) = Storage::set_item("draft", draft.to_string()) {
    log::warn!("{full}");
}
```

```tsx tabs
Storage.setItem("draft", JSON.stringify({ title: title(), body: body() }));
```

## Storage.removeItem

```ts
removeItem(key: string): Promise<void>;
```

删除 `key` 及其值。删除一个没有值的键不做任何事。Rust：`Storage::remove_item(key)`。

## Storage.keys

```ts
keys(): Promise<string[]>;
```

按顺序返回所有键。Rust：`Storage::get_all_keys()`。

## Storage.clear

```ts
clear(): Promise<void>;
```

删除应用的所有键，例如在用户退出登录时。Rust：`Storage::clear()`。

## 把 signal 保存在设备上

应用启动时读取保存的值，之后每当 signal 变化就写回去：

```rust tabs
use nsh_core::{storage::Storage, ui::prelude::*};

#[component]
pub fn App() -> impl IntoElement {
    let saved = Storage::get_item("count").and_then(|n| n.parse().ok());
    let count = signal(saved.unwrap_or(0_u32));
    effect(move || {
        if let Err(full) = Storage::set_item("count", count.get().to_string()) {
            log::warn!("{full}");
        }
    });
    view()
        .on_press(move || count.update(|n| *n += 1))
        .child(text(move || format!("按了 {} 次", count.get())))
}
```

```tsx tabs
import { effect, mount, signal, Text, View } from "nsh";
import { Storage } from "nsh/storage";

const count = signal(0);
const loaded = signal(false);
Storage.getItem("count").then((saved) => {
  count.set(Number(saved ?? 0));
  loaded.set(true);
});
effect(() => {
  if (loaded()) Storage.setItem("count", String(count()));
});

mount(() => (
  <View onPress={() => count.update((n) => n + 1)}>
    <Text>按了 {count()} 次</Text>
  </View>
));
```

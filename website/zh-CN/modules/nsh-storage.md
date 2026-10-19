---
title: nsh/storage
description: Storage：在设备上按键保存字符串，下次启动时仍在。
order: 8.6
demo: mod-storage
---

# nsh/storage

`nsh/storage` 把应用的数据保存在设备上：以字符串为键、字符串为值，应用下次启动时数据仍在。适合保存设置、草稿、用户上次打开的标签页、服务器返回内容的缓存——任何需要比应用进程活得更久的状态。

```ts
import { Storage } from "nsh/storage";
```

```ts
const Storage: {
  getItem(key: string): Promise<string | null>;
  setItem(key: string, value: string): Promise<void>;
  removeItem(key: string): Promise<void>;
  getAllKeys(): Promise<string[]>;
  clear(): Promise<void>;
};
```

每个应用有自己的存储，键和值合计最多 5 MB。调用按应用发起的顺序执行，所以在 `setItem` 之后调用 `getItem` 无需等待写入就能读到新值。读取来自内存；写入在后台成批落盘。

| 平台 | 数据保存位置 |
|---|---|
| iOS | 应用容器内的 `Library/Application Support/<app id>/storage.json`；嵌入在其他应用中时为 `…/nsh/<app id>/` |
| Android | 应用私有目录内的 `files/nsh/…/storage.json` |
| macOS | `~/Library/Application Support/<app id>/storage.json` |
| Windows | `%APPDATA%\<app id>\storage.json` |
| Linux | `$XDG_DATA_HOME/<app id>/storage.json`（`~/.local/share/…`） |
| Web | 浏览器的 `localStorage`，键以页面所在目录命名（`nsh:/apps/todo/:…`） |

损坏的 `storage.json` 会被保留为同目录下的 `storage.json.corrupt`，应用从空存储开始；读不了的文件则让应用的改动只留在内存中，文件本身永远不会被覆盖。

app id 是 `nsh.json` 中的 `id`。嵌入在其他应用中的应用，可以由宿主应用指定专属目录（见[在 iOS 中嵌入](../docs/embedding-ios.md)和[在 Android 中嵌入](../docs/embedding-android.md)）。

## Storage.getItem

```ts
getItem(key: string): Promise<string | null>;
```

返回 `key` 的值，没有值时返回 `null`。

```ts
const theme = (await Storage.getItem("theme")) ?? "system";
```

## Storage.setItem

```ts
setItem(key: string, value: string): Promise<void>;
```

把 `key` 设为 `value`，替换原有的值。如果存储会超过 5 MB，则 Promise 被拒绝，值不会被保存。值都是字符串：结构化数据用 `JSON.stringify` 保存，用 `JSON.parse` 读回。

```ts
Storage.setItem("draft", JSON.stringify({ title: title(), body: body() }));
```

## Storage.removeItem

```ts
removeItem(key: string): Promise<void>;
```

删除 `key` 及其值。删除一个没有值的键不做任何事。

## Storage.getAllKeys

```ts
getAllKeys(): Promise<string[]>;
```

按顺序返回所有键。

## Storage.clear

```ts
clear(): Promise<void>;
```

删除应用的所有键，例如在用户退出登录时。

## 把 signal 保存在设备上

应用启动时读取保存的值，之后每当 signal 变化就写回去：

```tsx
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

## 在 Rust 中使用

Rust 组件通过 `nsh_core::storage::Storage` 读写同一份存储，它的函数与 TS 一一对应，因此同时有 TSX 入口和 Rust 入口的应用看到的是同一份数据。这些函数是同步的：数据就在内存中。

```rust
use nsh_core::storage::Storage;

let count: u32 = Storage::get_item("count").and_then(|n| n.parse().ok()).unwrap_or(0);
Storage::set_item("count", (count + 1).to_string())?;
```

| TS | Rust |
|---|---|
| `await Storage.getItem(key)` | `Storage::get_item(key) -> Option<String>` |
| `await Storage.setItem(key, value)` | `Storage::set_item(key, value) -> Result<(), StorageFull>` |
| `await Storage.removeItem(key)` | `Storage::remove_item(key)` |
| `await Storage.getAllKeys()` | `Storage::get_all_keys() -> Vec<String>` |
| `await Storage.clear()` | `Storage::clear()` |

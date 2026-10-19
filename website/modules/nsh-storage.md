---
title: nsh/storage
description: "Storage: keep strings by key on the device, across launches."
order: 8.6
demo: mod-storage
---

# nsh/storage

`nsh/storage` keeps the app's data on the device: strings by string key, still there the next time the app starts. Use it for settings, drafts, the last tab the user had open, a cache of what the server sent — any state that should outlive the app's process.

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

Each app has a storage of its own, up to 5 MB of keys and values. Calls run in the order the app makes them, so a `getItem` after a `setItem` sees the new value without waiting for the write. Reads come from memory; writes reach the disk in the background, a few at a time.

| Platform | Where the data is kept |
|---|---|
| iOS | `Library/Application Support/nsh/<app id>/storage.json` in the app's container |
| Android | `files/nsh/<app id>/storage.json` in the app's private directory |
| macOS | `~/Library/Application Support/<app id>/storage.json` |
| Windows | `%APPDATA%\<app id>\storage.json` |
| Linux | `$XDG_DATA_HOME/<app id>/storage.json` (`~/.local/share/…`) |
| Web | The browser's `localStorage`, under keys named after the app id (`nsh:com.example.todo:…`) |

A `storage.json` that is damaged is kept as `storage.json.corrupt` next to it, and the app starts with an empty storage; one that cannot be read leaves the app's changes in memory, so the file is never replaced.

The app id is the `id` in `nsh.json`, and it travels with the app: `nsh build` and `nsh package` write it next to the bundle, the dev server sends it, and a signed package carries the one its release key's certificate vouches for. So every app keeps its own data with no setting, wherever it runs; an app without an id keeps it under `default`. An embedding app can choose another place (see [Embedding on iOS](../docs/embedding-ios.md) and [Embedding on Android](../docs/embedding-android.md)).

## Storage.getItem

```ts
getItem(key: string): Promise<string | null>;
```

Resolves with the value of `key`, or `null` when it has none.

```ts
const theme = (await Storage.getItem("theme")) ?? "system";
```

## Storage.setItem

```ts
setItem(key: string, value: string): Promise<void>;
```

Sets `key` to `value`, replacing what it held. Rejects when the storage would grow past 5 MB; the value is not kept then. Values are strings: keep structured data with `JSON.stringify` and read it back with `JSON.parse`.

```ts
Storage.setItem("draft", JSON.stringify({ title: title(), body: body() }));
```

## Storage.removeItem

```ts
removeItem(key: string): Promise<void>;
```

Removes `key` and its value. Removing a key that has no value does nothing.

## Storage.getAllKeys

```ts
getAllKeys(): Promise<string[]>;
```

Resolves with every key, in order.

## Storage.clear

```ts
clear(): Promise<void>;
```

Removes every key of the app, such as when the user signs out.

## Keeping a signal on the device

Read the stored value when the app starts, then write it back whenever the signal changes:

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
    <Text>Pressed {count()} times</Text>
  </View>
));
```

## From Rust

Rust components read and write the same storage with `nsh_core::storage::Storage`, whose functions mirror the TS ones, so an app with a TSX entry and a Rust entry sees one set of data. They are synchronous: the entries are in memory.

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

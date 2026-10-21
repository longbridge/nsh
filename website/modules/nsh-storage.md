---
title: nsh/storage
description: "Storage: keep strings by key on the device, across launches."
order: 8.6
demo: mod-storage
---

# nsh/storage

The storage API keeps the app's data on the device: strings by string key, still there the next time the app starts. Use it for settings, drafts, the last tab the user had open, a cache of what the server sent — any state that should outlive the app's process. Rust code uses `nsh_core::storage::Storage`, TSX imports `Storage` from `nsh/storage`; both read and write the same entries, so an app with a Rust entry and a TSX entry sees one set of data.

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

| TS | Rust (`nsh_core::storage::Storage`) |
|---|---|
| `await Storage.getItem(key)` | `Storage::get_item(key) -> Option<String>` |
| `await Storage.setItem(key, value)` | `Storage::set_item(key, value) -> Result<(), StorageFull>` |
| `await Storage.removeItem(key)` | `Storage::remove_item(key)` |
| `await Storage.keys()` | `Storage::get_all_keys() -> Vec<String>` |
| `await Storage.clear()` | `Storage::clear()` |

The Rust functions are synchronous, as the entries are in memory: call them where components run (their bodies, handlers and tasks).

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

The app id is the `id` in `nsh.json`, and it travels with the app: `nsh build` and `nsh package` write it next to the bundle, the dev server sends it, and a signed package carries the one its release key's certificate vouches for. So every app keeps its own data with no setting, wherever it runs; an app without an id keeps it under `default`. An embedding app can choose another place (see [Embedding on iOS](../docs/ios.md) and [Embedding on Android](../docs/android.md)).

## Storage.getItem

```ts
getItem(key: string): Promise<string | null>;
```

Resolves with the value of `key`, or `null` when it has none. Rust: `Storage::get_item(key)`, `None` when it has none.

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

Sets `key` to `value`, replacing what it held. Rejects when the storage would grow past 5 MB; the value is not kept then. Rust: `Storage::set_item(key, value)`, which returns `Err(StorageFull)` then. Values are strings: keep structured data as JSON (`serde_json::to_string` in Rust, `JSON.stringify` in TSX) and read it back (`serde_json::from_str`, `JSON.parse`).

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

Removes `key` and its value. Removing a key that has no value does nothing. Rust: `Storage::remove_item(key)`.

## Storage.keys

```ts
keys(): Promise<string[]>;
```

Resolves with every key, in order. Rust: `Storage::get_all_keys()`.

## Storage.clear

```ts
clear(): Promise<void>;
```

Removes every key of the app, such as when the user signs out. Rust: `Storage::clear()`.

## Keeping a signal on the device

Read the stored value when the app starts, then write it back whenever the signal changes:

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
        .child(text(move || format!("Pressed {} times", count.get())))
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
    <Text>Pressed {count()} times</Text>
  </View>
));
```

---
title: nsh/shared
description: "nsh/shared: state several surfaces share — keys and values changed in versioned transactions, followed as accessors, and events."
order: 12.8
---

# nsh/shared

`nsh/shared` is the state several surfaces share: string keys and their values, kept by NSH outside any one app's reactive graph. Each surface (an NSH view an app shows, one per engine) has a reactive graph of its own, so signals stay inside it; the shared store is what the surfaces that are given the same one read and write together. A surface on its own has a store of its own, so the module works the same in every app.

```ts
import { emit, get, on, prefix, remove, set, signal, transaction, version, type SharedTransaction } from "nsh/shared";
```

- **Keys and values.** A value is `null`, a boolean, a number, a string, an array or object of values, or bytes (an `ArrayBuffer` or typed array, read back as an `ArrayBuffer`). Values are copied in when written and are immutable in the store.
- **Transactions and versions.** Every write is a transaction: one `set` or `remove`, or a `transaction` of several. Each transaction makes one new version of the store, and readers see all of its changes together.
- **Following the store.** `signal(key)` and `prefix(prefix)` are accessors: a binding or effect that reads one runs again when a transaction changes what it read, at the surface's next turn, with every key that changed with it. A surface that is busy for a while catches up with the latest values in one turn.
- **Events.** `emit(topic, value)` sends a one-off message to the listeners of `topic` (`on`), in order. Use events for what happens (a document was opened) and keys for what is (the document that is open).

Rust components read the same store with `nsh_core::shared` (`shared::signal`, `shared::prefix`, `shared::on_event`), and Rust code on any thread writes it with `SharedStore::transaction`: a data source that runs in Rust writes its updates once and every surface that watches them shows them.

## signal

```ts
function signal<T = unknown>(key: string): Accessor<T | undefined>;
```

The value of `key`, as an accessor that follows the store. It reads `undefined` while the key has no value (and for a value only Rust reads). The key is watched while the current owner (a component, an effect) lives.

```tsx
import { mount, Text } from "nsh";
import { signal } from "nsh/shared";

function Theme() {
  const theme = signal<string>("settings/theme");
  return <Text>{`Theme: ${theme() ?? "system"}`}</Text>;
}

mount(Theme);
```

## prefix

```ts
function prefix<T = unknown>(prefix: string): Accessor<Record<string, T>>;
```

The keys starting with `prefix` and their values, as an accessor: a new object, with the full keys as its fields, each time one of them changes. Use it for a set of keys that grows and shrinks (the rows of a list); read single keys that change often with `signal`.

```tsx
import { Text, View } from "nsh";
import { prefix } from "nsh/shared";

function Documents() {
  const docs = prefix<{ title: string }>("doc/");
  return (
    <View>
      <Each of={Object.values(docs())}>{(doc) => <Text>{doc().title}</Text>}</Each>
    </View>
  );
}
```

## get

```ts
function get<T = unknown>(key: string): T | undefined;
```

The value of `key` now, without following it.

## set and remove

```ts
function set(key: string, value: unknown): number;
function remove(key: string): number;
```

Set or delete one key, as a transaction of one change. Both return the store's version after it. Writing a function or a symbol throws a `TypeError`.

## transaction

```ts
function transaction(fn: (tx: SharedTransaction) => void): number;

interface SharedTransaction {
  set(key: string, value: unknown): void;
  remove(key: string): void;
}
```

Runs `fn` and commits what it set and removed as one version when it returns: readers see all of it together. When `fn` throws, nothing is committed and the error propagates. Returns the store's version after.

```ts
import { transaction } from "nsh/shared";

transaction((tx) => {
  tx.set("selection/id", 42);
  tx.set("selection/kind", "document");
});
```

## version

```ts
function version(): number;
```

The version of the store's last transaction (0 before the first).

## emit and on

```ts
function emit(topic: string, value?: unknown): void;
function on<T = unknown>(topic: string, fn: (value: T) => void): () => void;
```

`emit` sends `value` to every listener of `topic` in every surface that shares the store, this one included, in the order emitted. `on` listens: `fn` gets each value at the surface's next turn. It returns a function that stops listening, and listening also stops with the current owner. Events reach the listeners there are when they are emitted; a surface that should know a state when it starts reads a key.

```tsx
import { signal as local, Text } from "nsh";
import { emit, on } from "nsh/shared";

function Inbox() {
  const last = local("none");
  on<string>("opened", (title) => last.set(title));
  return <Text onPress={() => emit("opened", "Report.pdf")}>{`Last opened: ${last()}`}</Text>;
}
```

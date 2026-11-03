---
title: nsh/shared
description: "nsh/shared：多个视图表面共享的状态——按带版本的事务修改的键和值、以访问器跟随，以及事件。"
order: 12.8
---

# nsh/shared

`nsh/shared` 是多个视图表面（surface）共享的状态：字符串键和它们的值，由 NSH 保存在任何一个应用的响应式图之外。每个视图表面（应用显示的一个 NSH 视图，一个引擎一个）都有自己的响应式图，signal 只在图内有效；拿到同一个共享存储的表面，通过它一起读写。单独的表面有自己的一个存储，所以这个模块在每个应用里用法都一样。

```ts
import { emit, get, on, prefix, remove, set, signal, transaction, version, type SharedTransaction } from "nsh/shared";
```

- **键和值。** 值可以是 `null`、布尔、数字、字符串、由这些值组成的数组或对象，或字节（`ArrayBuffer` 或类型化数组，读回时是 `ArrayBuffer`）。写入时值被复制进存储，在存储里不可变。
- **事务和版本。** 每次写入都是一个事务：一次 `set` 或 `remove`，或者一个包含多处修改的 `transaction`。每个事务产生存储的一个新版本，读方会同时看到它的全部修改。
- **跟随存储。** `signal(key)` 和 `prefix(prefix)` 是访问器：读过它的绑定或 effect，会在事务改动了所读内容时、在本表面的下一轮里重新运行，同一事务改动的其他键也在这一轮里一起到达。表面忙了一阵之后，会在一轮里直接拿到最新的值。
- **事件。** `emit(topic, value)` 向 `topic` 的监听者（`on`）按顺序发送一次性消息。事件表示发生了什么（打开了一个文档），键表示现在是什么（当前打开的文档）。

Rust 组件用 `nsh_core::shared`（`shared::signal`、`shared::prefix`、`shared::on_event`）读同一个存储，任何线程上的 Rust 代码用 `SharedStore::transaction` 写它：用 Rust 实现的数据源只需把更新写一次，所有关注这些键的表面都会显示出来。

## signal

```ts
function signal<T = unknown>(key: string): Accessor<T | undefined>;
```

`key` 的值，作为跟随存储的访问器。键没有值时（以及值只有 Rust 能读时）读到 `undefined`。在当前的 owner（组件、effect）存活期间关注这个键。

```tsx
import { mount, Text } from "nsh";
import { signal } from "nsh/shared";

function Theme() {
  const theme = signal<string>("settings/theme");
  return <Text>{`主题：${theme() ?? "跟随系统"}`}</Text>;
}

mount(Theme);
```

## prefix

```ts
function prefix<T = unknown>(prefix: string): Accessor<Record<string, T>>;
```

以 `prefix` 开头的键及其值，作为访问器：其中任何一个变化时，得到一个新对象，字段名是完整的键。适合会增删的一组键（列表的各行）；变化频繁的单个键用 `signal` 读。

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

`key` 现在的值，不跟随。

## set 和 remove

```ts
function set(key: string, value: unknown): number;
function remove(key: string): number;
```

设置或删除一个键，作为只含一处修改的事务。两者都返回修改后存储的版本号。写入函数或 symbol 会抛出 `TypeError`。

## transaction

```ts
function transaction(fn: (tx: SharedTransaction) => void): number;

interface SharedTransaction {
  set(key: string, value: unknown): void;
  remove(key: string): void;
}
```

运行 `fn`，在它返回时把其中的设置和删除作为一个版本提交：读方会同时看到全部修改。`fn` 抛出异常时什么都不提交，异常继续向外抛。返回提交后存储的版本号。

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

存储最后一个事务的版本号（第一个事务之前是 0）。

## emit 和 on

```ts
function emit(topic: string, value?: unknown): void;
function on<T = unknown>(topic: string, fn: (value: T) => void): () => void;
```

`emit` 把 `value` 按发出的顺序发给共享该存储的每个表面（包括本表面）里 `topic` 的每个监听者。`on` 用来监听：`fn` 在本表面的下一轮里收到每个值。它返回一个停止监听的函数，当前 owner 结束时监听也随之停止。事件送达发出时已经在监听的监听者；表面启动时需要知道的状态，用键来读。

```tsx
import { signal as local, Text } from "nsh";
import { emit, on } from "nsh/shared";

function Inbox() {
  const last = local("无");
  on<string>("opened", (title) => last.set(title));
  return <Text onPress={() => emit("opened", "Report.pdf")}>{`最近打开：${last()}`}</Text>;
}
```

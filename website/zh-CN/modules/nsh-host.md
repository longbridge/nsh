---
title: nsh/host
description: "你的应用与嵌入它的应用之间的通道：启动参数、宿主发来的事件，以及发给宿主的调用和投递。"
order: 9.1
---

# nsh/host

`nsh/host` 是你的应用与嵌入它的应用之间的通道。在视图中运行你的应用的 iOS、Android 或桌面应用，可以给它启动参数（通知要打开的会话、当前登录的用户），向它发送事件，并回答它发出的调用，中间不需要任何视图，宿主也不需要写 Rust。数据以 JSON 值传递，两侧都在原生代码中转换。

```tsx
import { host } from "nsh/host";
```

嵌入方的那一侧属于它的 NSH 视图：`NshAppView`（Swift）和 `NshView`（Java、Kotlin）上的 `params`、`send`、`handle`，以及 Rust 中各平台 `AppView::host()` 返回的 `HostChannel`。

```ts
const host: Host;

interface Host {
  readonly params: HostParams;
  on<T = HostValue>(name: string, listener: (data: T) => void): () => void;
  call<T = HostValue>(name: string, data?: unknown): Promise<T>;
  post(name: string, data?: unknown): void;
}

type HostValue = null | boolean | number | string | HostValue[] | { [key: string]: HostValue };
type HostParams = { readonly [key: string]: HostValue };
```

## host.params

嵌入方在启动你的应用之前设置的参数，从你的代码第一行起就可以同步读取；没有设置时是 `{}`。应用运行期间它们保持不变：之后的请求以事件的形式到来。

```tsx
const conversation = host.params.conversation as string | undefined;
mount(() => (conversation ? <Conversation id={conversation} /> : <Inbox />));
```

## host.on

```ts
on<T = HostValue>(name: string, listener: (data: T) => void): () => void;
```

监听嵌入方的事件 `name`。`listener` 收到事件的数据（没有数据时是 `null`）；同一名称的监听器按添加顺序运行。返回一个停止监听的函数，在组件渲染时添加的监听器随组件一起停止。

事件会等待你的应用：应用尚未监听的名称的事件会被保留，在添加该名称的第一个监听器、添加它的代码运行完之后立即送达（最多保留 256 个事件，超出时丢弃最早的）。如果最后一个监听器在接收过程中停止，剩余事件会按原顺序继续等待。嵌入方在你的应用启动之前或重新加载期间发送的事件，会交给它接下来运行的应用。因此嵌入方可以在用户点开通知的任何时候发送“打开会话 42”，无论应用正在运行、正在启动还是即将启动，都能收到。

```tsx
host.on<{ id: string }>("conversation.open", ({ id }) => stack.push(<Conversation id={id} />));
```

## host.call

```ts
call<T = HostValue>(name: string, data?: unknown): Promise<T>;
```

用 `data` 调用嵌入方对 `name` 的处理函数。promise 以它的回答 resolve，或以一个携带处理函数所给消息的 `Error` reject。嵌入方没有处理 `name` 时立即 reject，因此你的应用可以分辨自己运行在哪个宿主中。同时最多有 1024 个调用等待回答；再多一个会以 `RangeError` reject。

```tsx
try {
  const profile = await host.call<{ name: string }>("user.profile", { id: 7 });
  name.set(profile.name);
} catch (e) {
  error.set((e as Error).message);
}
```

## host.post

```ts
post(name: string, data?: unknown): void;
```

把 `data` 交给嵌入方对 `name` 的处理函数，不需要回答：统计、"关闭我"、"用户已退出登录"。没有处理函数时被丢弃。

```tsx
host.post("analytics", { screen: "conversation", id });
```

## 数据

传入的值按 `JSON.stringify` 的方式转换：函数和值为 `undefined` 的字段被省略，嵌套超过 64 层或条目超过一百万的值会抛出 `RangeError`。名称是 1 到 256 字节的字符串；其他名称会抛出 `TypeError`（`host.call` 以它 reject）。

## 嵌入方的一侧

每个视图为它加载的所有应用保留一条通道。在加载之前设置参数，随时在任意线程发送事件，按名称注册处理函数。处理函数回答一次，可以立即回答，也可以稍后回答；始终没有到来的回答会让调用失败。

```rust tabs
// 任一平台的 AppView（GPUI、macOS、Windows、Linux、iOS）：它的 HostChannel。
use serde_json::json;

let host = view.host();
let mut params = serde_json::Map::new();
params.insert("conversation".into(), json!("42"));
host.set_params(params);
// 在引擎线程运行：立即回答，或把调用移走，稍后在任意线程回答。
host.handle("user.profile", |call| {
    let id = call.data()["id"].clone();
    call.resolve(json!({ "id": id, "name": "Ada" }));
});
host.send("conversation.open", json!({ "id": "43" })).unwrap();
```

```swift tabs
appView.params = ["conversation": "42"]
try appView.loadBundle(at: bundleURL)

// 在主线程运行；返回回答，或抛出错误让调用失败。
appView.handle("user.profile") { data in
    let id = (data as? [String: Any])?["id"]
    return ["id": id ?? NSNull(), "name": try await profiles.name(of: id)]
}
appView.handle("analytics") { data, _ in analytics.log(data) }
appView.send("conversation.open", ["id": "43"])
```

```java tabs
nsh.setParams(Map.of("conversation", "42"));
nsh.loadAssetBundle("nsh");

// 在 UI 线程运行；立即或稍后在任意线程回答。
nsh.handle("user.profile", (data, reply) -> {
    Object id = ((Map<?, ?>) data).get("id");
    profiles.load(id, profile -> reply.resolve(Map.of("id", id, "name", profile.name)),
            error -> reply.reject(error.getMessage()));
});
nsh.send("conversation.open", Map.of("id", "43"));
```

参见 [iOS 嵌入](../docs/ios.md#与应用通信)、[Android 嵌入](../docs/android.md#与应用通信) 和 [GPUI](../docs/gpui.md)。

> [!NOTE]
> 这条通道传递的是嵌入方自己的数据，运行的是它自己的代码：NSH 的[能力](../docs/sandbox-and-capabilities.md)不适用于它。Rust 应用直接通过嵌入方注册的函数调用其 Rust 代码（[`nativeModule`](./nsh-native.md#nativemodule) 和 Rust 导入）。

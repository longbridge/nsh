---
title: nsh/host
description: "The channel between your app and the app it is embedded in: launch parameters, events from the host, and calls and posts to it."
order: 9.1
---

# nsh/host

`nsh/host` is your app's channel with the app it is embedded in. An iOS, Android or desktop app that runs your app in a view passes it launch parameters (the conversation a notification opens, who is signed in), sends it events, and answers its calls, with no view in between and no Rust of its own. Data crosses as JSON values, converted natively on both sides.

```tsx
import { host } from "nsh/host";
```

The embedding app's side is part of its NSH view: `params`, `send`, `handle` on `NshAppView` (Swift) and `NshView` (Java, Kotlin), and the `HostChannel` of `AppView::host()` in Rust, on every platform.

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

The parameters the embedding app set before it started your app, read synchronously from the first line of your code; `{}` when it set none. They stay the same while your app runs: later requests come as events.

```tsx
const conversation = host.params.conversation as string | undefined;
mount(() => (conversation ? <Conversation id={conversation} /> : <Inbox />));
```

## host.on

```ts
on<T = HostValue>(name: string, listener: (data: T) => void): () => void;
```

Listens to the embedding app's event `name`. `listener` gets its data (`null` when it sent none); the listeners of one name run in the order they were added. Returns a function that stops listening, and a listener added while a component renders stops with the component.

Events wait for your app: an event of a name your app does not listen to yet is kept, and comes right after the first listener of its name is added, once the code adding it has run (up to 256 events are kept, then the oldest go). Events the embedding app sends before your app starts, or while it reloads, wait for the app it runs next. So the embedding app can send "open conversation 42" whenever its user taps a notification, and your app hears it whether it is running, starting or about to start.

```tsx
host.on<{ id: string }>("conversation.open", ({ id }) => stack.push(<Conversation id={id} />));
```

## host.call

```ts
call<T = HostValue>(name: string, data?: unknown): Promise<T>;
```

Calls the embedding app's handler of `name` with `data`. The promise resolves with its answer, or rejects with an `Error` carrying the message the handler gave. It rejects at once when the embedding app handles no `name`, so your app can tell which host it runs in. At most 1024 calls wait for answers at once; one more rejects with a `RangeError`.

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

Hands `data` to the embedding app's handler of `name`, expecting no answer: analytics, "close me", "the user signed out". Without a handler it is dropped.

```tsx
host.post("analytics", { screen: "conversation", id });
```

## Data

What you pass is converted as `JSON.stringify` converts it: functions and `undefined` fields are left out, and values nested deeper than 64 levels or with more than a million entries throw a `RangeError`. Names are strings of 1 to 256 bytes; any other name throws a `TypeError` (`host.call` rejects with it).

## The embedding app's side

Each view keeps one channel for every app it loads. Set parameters before loading, send events at any time from any thread, and register handlers by name. Handlers answer once, now or later; an answer that never comes fails the call.

```rust tabs
// Any platform's AppView (GPUI, macOS, Windows, Linux, iOS): its HostChannel.
use serde_json::json;

let host = view.host();
let mut params = serde_json::Map::new();
params.insert("conversation".into(), json!("42"));
host.set_params(params);
// Runs on the engine thread: answer at once, or move the call and answer later from any thread.
host.handle("user.profile", |call| {
    let id = call.data()["id"].clone();
    call.resolve(json!({ "id": id, "name": "Ada" }));
});
host.send("conversation.open", json!({ "id": "43" })).unwrap();
```

```swift tabs
appView.params = ["conversation": "42"]
try appView.loadBundle(at: bundleURL)

// On the main thread; return the answer, or throw to reject the call.
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

// On the UI thread; answer now or later, from any thread.
nsh.handle("user.profile", (data, reply) -> {
    Object id = ((Map<?, ?>) data).get("id");
    profiles.load(id, profile -> reply.resolve(Map.of("id", id, "name", profile.name)),
            error -> reply.reject(error.getMessage()));
});
nsh.send("conversation.open", Map.of("id", "43"));
```

See [Embedding on iOS](../docs/ios.md#talking-to-the-app), [Embedding on Android](../docs/android.md#talking-to-the-app) and [GPUI](../docs/gpui.md).

> [!NOTE]
> The channel carries the embedding app's own data and runs its own code: NSH's [capabilities](../docs/sandbox-and-capabilities.md) do not apply to it. A Rust app reaches the embedding app's Rust directly, through the functions it registers ([`nativeModule`](./nsh-native.md#nativemodule) and Rust imports).

---
title: nsh/host
description: "Host: call functions of the app or page embedding NSH and listen to its events."
order: 8.6
---

# nsh/host

`nsh/host` reaches the app that embeds NSH: the native app around an `NshAppView` / `NshView`, or on the web the page that runs the player. The app calls the host's functions by name with JSON arguments and listens to the events the host sends.

```ts
import { host } from "nsh/host";
```

```ts
const host: {
  available(): boolean;
  call(name: string, args?: unknown, options?: { timeout?: number }): Promise<unknown>;
  on(event: string, listener: (payload: unknown) => void): { remove(): void };
};
```

## host.available

```ts
available(): boolean;
```

Whether the platform has a host that answers calls. On the web it is `true` when the page gives the player a host ([The host on the web](#the-host-on-the-web)).

```tsx
import { Text } from "nsh";
import { host } from "nsh/host";

<Text>{host.available() ? "Embedded" : "Standalone"}</Text>;
```

## host.call

```ts
call(name: string, args?: unknown, options?: { timeout?: number }): Promise<unknown>;
```

Calls the host's function `name` with `args` (any JSON value; `null` without one) and resolves with what it returns. The call rejects with the host's error message when the function fails, with `nsh/host: this platform has no host` when `host.available()` is `false`, and when `name` is not in the app's [`host` capability](#capability) or is no valid name (1 to 128 of `A`–`Z`, `a`–`z`, `0`–`9`, `_`, `.`, `:`, `-`, without `..`).

A call waits 30 seconds for the host's answer, then rejects with `nsh/host: <name> timed out after 30000 ms`; an answer that comes later is dropped. `options.timeout` sets the wait in milliseconds, `0` for no limit; a timeout so large the clock never gets there (about 1e22 ms) waits without limit too, and a negative, `NaN` or infinite one rejects the call with `nsh/host: invalid timeout <value>`. The time is the app's, as its timers': in `nsh test` with a manual clock it moves with the clock.

```ts
const user = (await host.call("getUserInfo")) as { name: string };
const quote = await host.call("quote.refresh", { symbol: "AAPL" }, { timeout: 5000 });
```

## host.on

```ts
on(event: string, listener: (payload: unknown) => void): { remove(): void };
```

Calls `listener` with the payload of every `event` the host sends, until `remove()`. Events the host sends before the app starts arrive once it does, so listeners registered at the top level of the app receive them; an event sent before a listener registers is not replayed to it. Read the state the app starts with through `host.call`, and follow its changes with `host.on`.

```ts
const sub = host.on("loginChanged", (payload) => console.log(payload));
// later
sub.remove();
```

## Capability

Every function the app calls must be listed in `nsh.json`'s `capabilities.host`: exact names or prefixes ending in `*`, or `true` for all of them. This holds for every app, also one without other `capabilities`: the host's functions are never granted by default. A call that is not granted rejects and is logged, and building an app that imports `nsh/host` without `capabilities.host` warns. Events need no grant. See [Sandbox and capabilities](../docs/sandbox-and-capabilities.md#host).

```json
{ "capabilities": { "host": ["getUserInfo", "bridge.*"] } }
```

## The host on the web

On the web the page is the host. `createPlayer`'s `host` option answers the calls, and `player.hostEvent(event, payload)` sends events:

```js
const player = await createPlayer(container, {
  wasm: "./nsh.wasm",
  host: { call: (name, args) => (name === "getUserInfo" ? { name: "Ada" } : Promise.reject(new Error(name))) },
});
player.hostEvent("loginChanged", { signedIn: true });
```

A published app names its host in `nsh.json`: `web.host` is a JS module in the app's directory, relative to `nsh.json`, exporting `createHost(player)`, which returns the object with `call`. `nsh package --target web` ships it with the site, `nsh run --target web` runs it on the development page, and `nsh test` runs it for the apps its tests open. `web.connect` lists the hosts the host's own requests reach, which the page's Content-Security-Policy allows alongside the app's `capabilities.network`. See [Web](../docs/web.md#the-hosts-functions).

```json
{ "web": { "host": "./host.js", "connect": ["api.example.com"] } }
```

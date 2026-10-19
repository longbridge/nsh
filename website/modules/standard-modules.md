---
title: Standard modules
description: "Pinned LLRT utilities and capability-checked NSH file, process and network adapters."
order: 14.5
---

Native Shell provides a pinned subset of LLRT standard modules inside its existing QuickJS VM, with NSH capability adapters for files, processes and networking. Applications stay in TS/TSX and use the explicit module surface below.

| Module | Surface |
|---|---|
| `buffer`, `crypto`, `path`, `url`, `zlib`, `events`, `assert`, `util`, `string_decoder` | LLRT native utility modules |
| `os` | `platform()`, `arch()`, `EOL`, `homedir()`: actual host OS metadata |
| `console` | Default export of the per-VM logger: `log`, `info`, `debug`, `warn`, `error` |
| `process` | `spawn`, `run`, `platform`, `arch`, `nextTick`; global `process.dataDir` |
| `fs`, `fs/promises` | Promise-based `readFile`, `writeFile`, `mkdir`, `rename`, `unlink`, `exists`, `access`, `readdir`, `stat`, `lstat`, `rmdir`, `constants` |
| `net` | `connect(host, port)` returning a promise-based client socket |
| `websocket` | `WebSocket` constructor, also installed globally on native hosts |

## Imports and generated types

Bare names and one `node:` prefix identify the same implementation, for example `buffer` and `node:buffer`. The bundler preserves these native imports. Generated `.nsh/types` describe the available interfaces. This is an explicit compatibility subset; full Node.js modules and event-loop semantics are outside its API contract.

LLRT implementations are pinned to `d9fd873e793c6551c5d92b2e31480f3279e3cd8d`. Utility declarations are generated from upstream source assets at that revision and projected onto actual runtime exports; file, process and network adapters carry declarations beside their implementations. `Buffer` and `crypto` are globals. NSH retains the identity of its existing URL, Blob, stream and event constructors.

Compatibility tests cover the exported subset and NSH integration; utility edge cases follow the pinned LLRT implementation.

## OS metadata and logging

`os.platform(): string` and `os.arch(): string` report the actual native host OS and architecture; `os.EOL: string` is its line separator. `os.homedir(): string` returns a configured absolute host home path and throws when none is configured. Directory paths are metadata: file operations still require `fs.read` / `fs.write` grants.

`import console from "node:console"` shares the VM's global `console` object. The module has a default export only, supporting `log`, `info`, `debug`, `warn`, `error`, each with signature `(...values: unknown[]): void`. It uses NSH's logger formatting; its public surface is this five-method object.

## Files and processes

`process.spawn` / `run` share the permissions and process lifecycle of [nsh/process](./nsh-process.md). Standard `fs` shares [nsh/fs](./nsh-fs.md)'s native dispatcher, checks `fs.read` / `fs.write`, and limits each data operation to 16 MiB; these entry points return promises. `readFile` returns a Buffer by default and a string when given an encoding. `rmdir` removes one empty directory.

## Network permissions and boundaries

Global `fetch` retains NSH's streaming HTTP implementation, checking `network.hosts` or `network.http` for requests and redirect hops. Native-host WebSocket supports `ws:` / `wss:`, text and binary messages, event listeners and closing handshakes. Permission checks map `ws` to `http` and `wss` to `https`, using GET; the handshake connects to the original URL and rejects redirects. Each VM supports at most 32 WebSockets, messages up to 8 MiB, and a send queue of eight messages with at most 8 MiB buffered. Handshake and writes time out after 30 seconds; closing waits up to 5 seconds.

`net.connect` uses only `network.hosts`, checked before DNS lookup. Host grants cover all valid ports on that host; HTTP method or path grants do not grant raw TCP access. The returned socket provides `read(size?)`, `write(text)` and `close()`; reads return `Uint8Array`, or `null` at EOF. Connections and I/O have 30-second timeouts, each read/write is limited to 1 MiB, and each direction allows one pending operation. This is a promise-based client socket interface.

Raw TCP and this WebSocket transport use native hosts. Browser apps use the platform's [global APIs](./globals.md); availability follows the actual host. Embedding grants intersect the app's capability declaration.

## WebSocket grant example

This `nsh.json` fragment permits the GET handshake for this `wss` URL, keeping its path and HTTPS scope:

```json
{
  "capabilities": {
    "network": {
      "http": [{
        "host": "chat.example.com",
        "methods": ["GET"],
        "schemes": ["https"],
        "pathPrefix": "/socket"
      }]
    }
  }
}
```

```ts
const socket = new WebSocket("wss://chat.example.com/socket");
socket.addEventListener("open", () => socket.send("hello"));
socket.addEventListener("message", (event) => console.log(event.data));
```

## TCP grant example

TCP uses a host grant. This grant covers every valid port on the host; `9000` is the connection parameter.

```json
{
  "capabilities": {
    "network": { "hosts": ["tcp.example.com"] }
  }
}
```

```ts
import { connect } from "node:net";

const socket = await connect("tcp.example.com", 9000);
try {
  await socket.write("hello\n");
  const bytes = await socket.read(1024);
  if (bytes) console.log(new TextDecoder().decode(bytes));
} finally {
  socket.close();
}
```

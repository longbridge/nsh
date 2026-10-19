---
title: Standard modules
description: "Eleven Node-shaped standard modules: pinned LLRT utilities and capability-checked files, processes and networking."
order: 14.5
---

Native Shell provides eleven standard modules, imported by their bare names: Node-shaped APIs compatible with gpui-shell's, running in the app's VM on NSH's native runtime, with their capabilities checked in Rust. Their TypeScript declarations in `.nsh/types` are generated from the same specs as the modules, so what the types show is what the runtime exports.

| Module | Exports |
|---|---|
| `buffer` | `Buffer`, `atob`, `btoa`, `constants`, default (LLRT); `Buffer` is also a global |
| `crypto` | `createHash`, `createHmac`, `randomBytes`, `randomFill`, `randomFillSync`, `randomInt`, `randomUUID`, `getRandomValues`, `webcrypto`, `crypto`, `Crc32`, `Crc32c`, `Md5`, `Sha1`, `Sha256`, `Sha384`, `Sha512`, default (LLRT); `crypto` is also a global |
| `path` | `basename`, `delimiter`, `dirname`, `extname`, `format`, `isAbsolute`, `join`, `normalize`, `parse`, `relative`, `resolve`, `sep`, default (LLRT) |
| `url` | `URL`, `URLSearchParams`, `domainToASCII`, `domainToUnicode`, `fileURLToPath`, `format`, `pathToFileURL`, `urlToHttpOptions`, default; `URL` and `URLSearchParams` are NSH's global classes |
| `zlib` | `deflate`, `deflateRaw`, `inflate`, `inflateRaw`, `gzip`, `gunzip`, `brotliCompress`, `brotliDecompress`, `zstdCompress`, `zstdDecompress`, each with its `Sync` form, default (LLRT) |
| `os` | `platform()`, `arch()`, `homedir()`, `EOL`, default |
| `console` | default: the VM's `console` (`log`, `info`, `debug`, `warn`, `error`) |
| `fs/promises` | `readFile`, `writeFile`, `readdir`, `exists`, `mkdir`, `rename`, `unlink`, `rmdir`, `stat`, `lstat`, `readlink`, default |
| `process` | `run`, `spawn`, `exit`, `platform`, `arch`, `nextTick`, `ChildProcess`, default; the default object also has `env` and `dataDir` and is the global `process` |
| `net` | `connect`, `Socket`, default |
| `websocket` | `WebSocket.connect(url, options?)` resolving a `WebSocketConnection` (`read`, `write`, `close`), `WebSocketConnection`; the global `WebSocket` constructor on native hosts |

## Imports and generated types

Each module is imported by exactly the name in the table: `import { join } from "path"`, `import fs from "fs/promises"`. The bundler keeps these imports, and the runtime serves them natively; the catalog is these eleven names. `buffer`, `crypto`, `path`, `url` and `zlib` are LLRT's implementations, pinned to `d9fd873e793c6551c5d92b2e31480f3279e3cd8d`, with declarations projected from that revision onto the exports the runtime has. The others are NSH's own. NSH keeps the identity of its URL, Blob, stream and event classes.

## How they compare with Node.js and gpui-shell

The modules follow Node's names and shapes for the subset they export, and gpui-shell's behavior where it defines one. NSH's own choices:

- `fs/promises`' `exists` is `true` for a symlink whose target is missing, and `unlink`, `rename`, `rmdir`, `lstat` and `readlink` act on a symlink itself.
- `Stats` and `Dirent` have their fields (`size`, `mtimeMs`, `name`, ...) as own properties, as Node's do, and `isFile()`, `isDirectory()` and `isSymbolicLink()` on their prototype.
- A `ChildProcess` listener receives the event's payload: the output text, or the exit's `{ code, signal }` (with `error` when the runtime ended the child). `on` returns the child.
- A child's environment is a small set from the host plus the names the app is granted, and it runs in the app's data directory or a `cwd` within an `fs.write` root ([Processes](#processes)).
- `process.dataDir` is `null` when the host has no data directory; `process.exit` asks the host to exit.
- Limits are part of each API: sizes, counts and timeouts are listed with each module below.

## OS metadata and logging

`os.platform(): string` and `os.arch(): string` report the actual native host OS and architecture; `os.EOL: string` is its line separator. `os.homedir(): string` returns a configured absolute host home path and throws when none is configured. Directory paths are metadata: file operations still require `fs.read` / `fs.write` grants.

`import console from "console"` shares the VM's global `console` object. The module has a default export only, supporting `log`, `info`, `debug`, `warn`, `error`, each with signature `(...values: unknown[]): void`. It uses NSH's logger formatting.

## Compression

`zlib` is LLRT's, with one bound NSH adds: each decompression (`inflate`, `inflateRaw`, `gunzip`, `brotliDecompress`, `zstdDecompress` and their `Sync` forms) produces at most 64 MiB, or the VM's heap limit when that is smaller. Longer output throws a `RangeError`, as Node.js does for `maxOutputLength`; an app's own `maxOutputLength` sets a lower bound for one call.

## Files

`fs/promises` reads and writes files on desktop hosts (macOS, Windows and Linux). Every path is checked against `capabilities.fs` before the operation starts: `read` roots for `readFile`, `readdir`, `stat`, `lstat`, `readlink` and `exists`, `write` roots for `writeFile`, `mkdir`, `rename`, `unlink` and `rmdir`. A path outside the grants throws a `TypeError` at the call; I/O failures reject the promise with an `Error`.

```json
{
  "capabilities": {
    "fs": { "read": ["${dataDir}"], "write": ["${dataDir}"] }
  }
}
```

Roots are absolute paths, `${dataDir}` (this app's data directory, chosen by the host), `${pluginDir}` (the app's directory), `${homeDir}`, or paths relative to the app's directory. A relative path in a call is resolved in each granted root in turn. An embedding host may narrow the grants further. Every operation stays inside its granted root, symlinks included. `lstat`, `readlink`, `exists`, `unlink`, `rename` and `rmdir` act on a symlink itself: `exists` is `true` for a symlink whose target is missing, and `unlink` removes the link and keeps its target.

Failures carry Node's error properties: `code` (`ENOENT`, `EEXIST`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `ELOOP`, `EACCES`, `EPERM`, ...; `EILSEQ` for text that is not UTF-8, `ERR_FS_FILE_TOO_LARGE` past the read limit), `syscall` and `path`, so `error.code === "ENOENT"` tells a missing file apart. A path outside the grants throws a `TypeError` with `code` `EACCES`, and `ENOSYS` where the host has no native filesystem.

`readFile` returns a `Uint8Array`, or a string with `"utf8"`, and reads files up to 64 MiB. `writeFile` writes a string or `Uint8Array` of up to 8 MiB, creating or replacing a regular file. `readdir` lists up to 10 000 names, sorted, or `Dirent`s (`name`, `isDirectory()`) with `{ withFileTypes: true }`. `stat` and `lstat` resolve a `Stats` with `size`, `mode`, `mtimeMs` / `atimeMs` / `birthtimeMs`, `mtime` / `atime` / `birthtime` as `Date`s, and `isFile()`, `isDirectory()` and `isSymbolicLink()`. `rmdir` removes one empty directory. One VM runs up to 1024 operations at a time and holds up to 64 MiB of file data in flight (data handed to `writeFile` until it is written, data read until its promise settles); a call past these limits throws a `RangeError`, a read past them rejects.

## Processes

`process.spawn` and `process.run` launch commands on desktop hosts (macOS, Windows and Linux). The runtime executes the program directly with an argument array, with no shell, and owns its pipes; stdout and stderr arrive as incremental UTF-8.

```json
{
  "capabilities": {
    "process": { "execute": ["git"], "env": ["GIT_AUTHOR_NAME"] },
    "fs": { "write": ["${dataDir}"] }
  }
}
```

Commands match `process.execute` exactly: `git` and `/usr/bin/git` are separate grants. Bare commands resolve through the absolute directories of the host's `PATH`. A child's environment is `PATH`, `HOME`, `USERPROFILE`, `SystemRoot`, `TEMP`, `TMP`, `TMPDIR`, `LANG` and `LC_*` from the host, plus the names `process.env` grants. A child runs in the app's data directory (created when missing), or in `cwd`, an existing directory within an `fs.write` root, since the child may write where it runs. An embedding host can narrow the grants.

```ts
import process from "process";

const { code, stdout } = await process.run("git", ["status", "--short"]);
const child = process.spawn("git", ["log", "--oneline"], { lines: true });
child.on("stdout", (line) => console.log(line));
child.on("exit", ({ code, signal }) => console.log("done", code, signal));
```

`run` resolves `{ code, stdout, stderr }` for any exit status and rejects on launch failures, timeouts and output past its limit. `spawn` returns a child at once; `stdin: "pipe"` enables `write` and `end`, and `lines: true` delivers complete lines. `on(event, handler)` returns the child, and a handler gets the event's payload: the text, or `{ code, signal }` once, after the final output (with `error` when the runtime ended the child: a timeout, an over-long line) (at once for a listener added later). `pid` is the child's process id. A refused command, or a `cwd` outside the grants, throws a `TypeError` with `code` `EACCES`; a launch failure carries `code` (`ENOENT` for a missing executable, `EACCES` for one that cannot run), `syscall` (`spawn <command>`) and `path`: `spawn` throws it, `run` rejects with it. Past the children limit, `spawn` throws and `run` rejects with a `RangeError`. `process.dataDir` is `null` when the host has no data directory. `kill`, a timeout, the reactive owner's cleanup and the VM's teardown end the whole process tree: on Unix its process group gets SIGTERM and then SIGKILL after 250 ms (at teardown, SIGKILL at once).

| Limit | Value |
|---|---|
| `timeoutMs` | Up to 4,294,967,295 ms; `run` defaults to 30 seconds, `spawn` runs until exit |
| `run`'s capture | 8 MiB per stream |
| One line with `lines: true` | 8 MiB |
| Children per VM | 32 |
| Output in flight per VM | 8 MiB; readers wait for JS to take it |
| Pending stdin per child | 1 MiB; `write` throws past it |
| Listeners | 64 per event per child |

## Network permissions and boundaries

Global `fetch` retains NSH's streaming HTTP implementation, checking `network.hosts` or `network.http` for requests and redirect hops. Native-host WebSocket supports `ws:` / `wss:`, text and binary messages, event listeners and closing handshakes. A WebSocket handshake is an HTTP GET, so both the global `WebSocket` and the `websocket` module's `WebSocket.connect` use the HTTP grants: `ws` is checked as `http` and `wss` as `https`, using GET (a legacy rule may also list `ws` / `wss` in `schemes`). The check runs on the normalized URL before DNS lookup, the handshake connects to that same URL and rejects redirects. Each VM supports at most 32 WebSockets across both APIs, messages up to 8 MiB, and a send queue of eight messages with at most 8 MiB buffered. Handshake and writes time out after 30 seconds; closing waits up to 5 seconds.

`net.connect(host, port)` uses only `network.sockets`, a list of exact `"host:port"` destinations (`"[::1]:9000"` for IPv6), checked before DNS lookup. Each entry grants one port on one host; host patterns, `network.hosts` and HTTP grants cover HTTP traffic only, and an app without `"capabilities"` lists its sockets too. The returned `Socket` provides `read(size?)`, `write(text)` and `close()`; reads return `Uint8Array`, or `null` at EOF. A refused grant throws a `TypeError` and a limit a `RangeError` at the call; connection and I/O failures reject with Node's `code` (`ECONNREFUSED`, `ENOTFOUND`, `ETIMEDOUT`, `ECONNRESET`, ...) and `syscall`, a connection's also with `address` and `port`, and an operation on a closed socket with `ERR_SOCKET_CLOSED`. Connections and I/O have 30-second timeouts, each read/write is limited to 1 MiB, at most 4 MiB of writes may wait to be sent, and `close()` ends pending reads and writes at once. Each VM keeps at most 32 sockets open or connecting. This is a promise-based client socket interface.

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

TCP uses a socket grant: the exact host and port the app connects to.

```json
{
  "capabilities": {
    "network": { "sockets": ["tcp.example.com:9000"] }
  }
}
```

```ts
import { connect } from "net";

const socket = await connect("tcp.example.com", 9000);
try {
  await socket.write("hello\n");
  const bytes = await socket.read(1024);
  if (bytes) console.log(new TextDecoder().decode(bytes));
} finally {
  socket.close();
}
```

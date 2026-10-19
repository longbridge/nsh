---
title: 标准模块
description: "固定版本 LLRT 标准模块与 NSH 文件、进程和网络适配器。"
order: 14.5
---

Native Shell 在同一个 QuickJS VM 中提供一组固定版本的 LLRT 标准模块，并以 NSH 自身的能力检查接入文件、进程和网络。应用继续使用 TS/TSX；运行时按下表提供明确的模块子集。

| Module | Surface |
|---|---|
| `buffer`, `crypto`, `path`, `url`, `zlib`, `events`, `assert`, `util`, `string_decoder` | LLRT native utility modules |
| `os` | `platform()`、`arch()`、`EOL`、`homedir()`：实际宿主的系统元数据 |
| `console` | 默认导出当前 VM 的日志对象：`log`、`info`、`debug`、`warn`、`error` |
| `process` | `spawn`, `run`, `platform`, `arch`, `nextTick`; global `process.dataDir` |
| `fs`, `fs/promises` | Promise-based `readFile`, `writeFile`, `mkdir`, `rename`, `unlink`, `exists`, `access`, `readdir`, `stat`, `lstat`, `rmdir`, `constants` |
| `net` | `connect(host, port)` returning a promise-based client socket |
| `websocket` | `WebSocket` constructor, also installed globally on native hosts |

## 导入与类型

裸模块名和单个 `node:` 前缀指向同一实现，例如 `buffer` 和 `node:buffer`。打包器保留这些原生模块导入。可用接口以生成的 `.nsh/types` 为准；这里提供的是明确的兼容子集，Node.js 的完整模块和事件循环行为不属于此接口承诺。

LLRT 实现固定在 `d9fd873e793c6551c5d92b2e31480f3279e3cd8d`。工具模块的声明从同一版本的上游源码资产生成，并按运行时实际导出筛选；文件、进程和网络适配器的声明与实现一起维护。`Buffer` 和 `crypto` 是全局对象；NSH 原有的 URL、Blob、流和事件构造器保留其对象身份。

兼容性测试覆盖已导出的子集及其与 NSH 的集成；工具函数的边界行为遵循固定版本的 LLRT 实现。

## 系统元数据与日志

`os.platform(): string` 和 `os.arch(): string` 返回实际原生宿主的系统与架构；`os.EOL: string` 是系统换行符。`os.homedir(): string` 返回配置为绝对路径的宿主主目录，在缺少此配置时抛出错误。返回目录路径只是元数据，文件操作仍需要 `fs.read` / `fs.write` 授权。

`import console from "node:console"` 与全局 `console` 是同一个 VM 日志对象。该模块仅默认导出，支持 `log`、`info`、`debug`、`warn`、`error`，每个方法的签名为 `(...values: unknown[]): void`。接口采用 NSH 的日志格式，公开接口为上述五方法对象。

## 文件与进程

`process.spawn` / `run` 与 [nsh/process](./nsh-process.md) 使用同一套权限和进程生命周期。标准 `fs` 与 [nsh/fs](./nsh-fs.md) 共用原生授权调度，分别检查 `fs.read` / `fs.write`，每次数据操作最多 16 MiB；这些接口返回 Promise。`readFile` 默认返回 Buffer，指定编码则返回字符串。`rmdir` 删除一个空目录。

## 网络授权与边界

全局 `fetch` 保留 NSH 的流式 HTTP 实现，按 `network.hosts` 或 `network.http` 检查每次请求与重定向。原生宿主上的 WebSocket 支持 `ws:` / `wss:`、文本及二进制消息、事件监听器与关闭握手。授权时 `ws` 映射为 `http`、`wss` 映射为 `https`，使用 GET 检查；握手按原 URL 连接，拒绝重定向。每个 VM 最多 32 个 WebSocket，单条消息最多 8 MiB，发送队列最多 8 条且缓冲总量最多 8 MiB。握手和写入超时为 30 秒，关闭最多等待 5 秒。

`net.connect` 仅使用 `network.hosts` 授权，检查发生在 DNS 查询之前。主机授权涵盖该主机的所有有效端口；HTTP 方法或路径授权不赋予原始 TCP 权限。返回的 socket 提供 `read(size?)`、`write(text)` 和 `close()`；读取返回 `Uint8Array`，EOF 返回 `null`。连接与 I/O 超时为 30 秒，单次读写最多 1 MiB；每个方向同时允许一个待完成操作。这是 Promise 客户端 socket 接口。

原始 TCP 和此 WebSocket 传输使用原生宿主。浏览器应用使用平台提供的 [全局 API](./globals.md)；平台支持以实际运行的宿主为准。嵌入宿主的能力授权与应用声明取交集。

## WebSocket 授权示例

下面的 `nsh.json` 片段允许该 `wss` 地址的 GET 握手，同时保留路径与 HTTPS 范围限制：

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

## TCP 授权示例

TCP 使用主机授权。下面的授权适用于该主机的所有有效端口；`9000` 是连接参数。

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

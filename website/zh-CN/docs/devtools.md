---
title: DevTools
description: "Native Shell DevTools（实验性）：从应用外部自动化和检查运行中的应用，可用 nsh devtools 命令、面向 AI 智能体的 MCP 服务器或 WebSocket 协议，也可以在它之上运行测试。"
order: 62
group: tools
---

# DevTools

Native Shell DevTools 是运行中的应用内部的一个端点。客户端连接上去，像测试那样驱动应用（点击、输入、滚动、等待），
也读取应用（视图树、截图、控制台）。三类客户端使用它：

- **智能体**，通过 [MCP 服务器](#mcp-服务器) `nsh devtools mcp`；
- **脚本和人**，通过 [`nsh devtools`](#命令) 命令；
- **工具**，直接使用[协议](#协议)：WebSocket 加 JSON。

DevTools 是实验性的：它的命令、MCP 工具和协议可能会变化。它在 macOS 和 Windows 上运行。

## 开启方式

- `nsh run` 和 `nsh test --target macos|windows` 启动的应用运行在开发 host 中，开发 host 带有 DevTools。
  `nsh devtools launch` 会替你启动一个。
- 用 `nsh package --devtools` 构建的打包应用带有 DevTools。应用启动时设置了环境变量 `NSH_DEVTOOLS`，端点才会监听；
  没有设置时保持关闭。这样的包用于开发和测试；发布包不带这个参数构建，不含端点。

```sh
nsh package my-app --target macos --devtools
nsh test my-app --target macos --app .nsh/out/MyApp.app
```

## 命令

`nsh devtools launch` 构建并启动应用，并留在前台；它打印端点的 URL，并在应用的 `.nsh/devtools/` 下写入一个会话文件。
其他命令通过这个文件找到运行中的应用，所以在应用的目录里运行它们（或传 `--app-dir`）。

```sh
nsh devtools launch examples/gallery --target macos   # 终端 1
nsh devtools ls                                       # 运行中的会话
nsh devtools tree --depth 3                           # 视图树，紧凑文本
nsh devtools press 'label:Navigation'
nsh devtools type 'id:name' 'hello'
nsh devtools wait 'label:Done' --timeout 5000         # --gone 等它消失
nsh devtools shot a.png                               # 窗口截图；--node SEL 裁剪到某个视图
nsh devtools logs --level warn                        # -f 持续跟随
nsh devtools close
```

| 选项 | 含义 |
|---|---|
| `--session <PID>` | 使用的会话（默认：唯一运行中的那个；有多个时命令会列出它们）。 |
| `--app-dir <DIR>` | 应用的目录（默认：当前目录）。 |
| `--json` | 以 JSON 打印协议的应答。 |

选择器与测试使用的相同：`id:`、`label:`、文本等（[测试](./testing-and-trace.md)）。
每个命令见 [`nsh devtools`](./cli.md#nsh-devtools)。

## MCP 服务器

`nsh devtools mcp` 通过标准输入输出，以 Model Context Protocol 把应用提供给 AI 智能体。用下面的命令把它加入 Claude Code：

```sh
claude mcp add nsh-devtools -- /path/to/nsh devtools mcp --app-dir /path/to/my-app
```

或者写进 `.mcp.json`：

```json
{
  "mcpServers": {
    "nsh-devtools": {
      "command": "/path/to/nsh",
      "args": ["devtools", "mcp", "--app-dir", "/path/to/my-app"]
    }
  }
}
```

工具有：`list_apps`、`launch_app`、`select_app`、`take_snapshot`、`take_screenshot`、`click`、`type`、`press_key`、
`scroll`、`hover`、`drag`、`wait_for`、`list_console`、`list_errors` 和 `close_app`。智能体先调用
`launch_app { "app": "examples/gallery" }`，用 `take_snapshot` 和 `take_screenshot` 读取屏幕，用 `click` 和 `type`
操作，最后用 `close_app` 关闭。服务器在每次操作之后释放输入，所以命令行客户端可以在智能体的两次调用之间操作。
过长的文本答复会在 20 000 个字符处截断并附上说明。

## 通过 DevTools 运行测试

在 macOS 和 Windows 上，`nsh test --target` 通过端点运行每个应用。配套有四个参数：

| 参数 | 含义 |
|---|---|
| `--jobs <N>` | 同时运行至多 N 个测试文件，每个文件在自己的进程里。 |
| `--artifacts <DIR>` | 为每个测试文件保留 `timeline.jsonl`（每个步骤和每行控制台输出各一行）和 `failure.png`（第一次失败时的窗口）。 |
| `--keep-open-on-failure` | 测试失败时让应用保持打开，并打印供客户端使用的 URL 和 token；按 Enter 继续。 |
| `--app <PATH>` | 对用 `nsh package --devtools` 构建的打包应用运行测试。 |

客户端可以连接测试运行保持打开的应用：

```text
nsh devtools: kept ws://127.0.0.1:52114/devtools open; token 3f9c…
```

## 协议

端点是回环地址上 `/devtools` 路径的 WebSocket。请求是一个 JSON 对象，包含 `id`、形如 `Domain.method` 的 `method`
和 `params`；应答带有同一个 `id`，以及 `result` 或带稳定 `code`（`ERR_DEVTOOLS_…`）的 `error`。端点还会发送事件
（`Console.entry`、`Network.request`、`Trace.batch`）。

`Protocol.describe` 返回机器可读的描述：协议版本、每个方法（含类型 `action` 或 `query`）及其作用、事件和错误码。
客户端和智能体读取它，而不必依赖本页的副本。

各个域：

| 域 | 内容 |
|---|---|
| `Session` | `hello`、`setup`、`start`、`ready`、`release`、`quit`：连接并启动应用。 |
| `Input` | 点击、输入、滚动、拖动、悬停、按键、粘贴、拖放、返回。 |
| `Tree` | 视图树快照、文本、数量、值、视图是否显示。 |
| `State` | 等待：某个视图、一次变化、一帧。 |
| `Capture` | 应用及其窗口的截图。 |
| `Environment` | 菜单、对话框、剪贴板、时钟、视口、配色方案、打开的 URL。 |
| `Console` | 应用的日志行：查询和订阅。 |
| `Host`、`Storage` | 以嵌入应用的身份向应用发送消息；应用保存的条目。 |

输入类操作会为发送它们的连接占用输入；其他连接的操作要等第一个连接发送 `Session.release` 或断开后才能进行，
读取则对所有连接开放。

## 安全

- 端点只监听回环地址。
- 每个连接都要出示应用启动时的 token（`NSH_DEVTOOLS_TOKEN`；为空时自动生成），没有就会被拒绝。`nsh devtools` 把它
  保存在只有所有者可读的会话文件里。
- 端点是开发 host 和用 `--devtools` 构建的包的功能。不带这个功能的构建不含端点、监听器和控制台收集器。

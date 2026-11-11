---
title: DevTools
description: "Native Shell DevTools (experimental): automate and inspect a running app from outside with nsh devtools commands, an MCP server for AI agents, or the WebSocket protocol; run tests over it."
order: 61.5
group: tools
---

# DevTools

Native Shell DevTools is an endpoint inside a running app. A client connects to it and drives the app as a test
does (press, type, scroll, wait) and reads it (the view tree, screenshots, the console). Three kinds of client use it:

- **Agents**, through the [MCP server](#the-mcp-server) `nsh devtools mcp`;
- **Scripts and people**, through the [`nsh devtools`](#commands) commands;
- **Tools**, through the [protocol](#the-protocol) itself: WebSocket and JSON.

DevTools is experimental: its commands, MCP tools and protocol may change. It runs on macOS and Windows.

## Turning it on

- Apps started by `nsh run` and `nsh test --target macos|windows` run in development hosts, which have DevTools.
  `nsh devtools launch` starts one for you.
- A packaged app has it when built with `nsh package --devtools`. The endpoint listens when the app starts with the
  environment variable `NSH_DEVTOOLS` set, and stays off otherwise. Such a package is for development and testing;
  release packages are built without the flag and contain no endpoint.

```sh
nsh package my-app --target macos --devtools
nsh test my-app --target macos --app .nsh/out/MyApp.app
```

## Commands

`nsh devtools launch` builds the app, starts it and stays in the foreground; it prints the endpoint's URL and writes
a session file under the app's `.nsh/devtools/`. The other commands find the running app from that file, so run them
in the app's directory (or pass `--app-dir`).

```sh
nsh devtools launch examples/gallery --target macos   # terminal 1
nsh devtools ls                                       # running sessions
nsh devtools tree --depth 3                           # the view tree as compact text
nsh devtools press 'label:Navigation'
nsh devtools type 'id:name' 'hello'
nsh devtools wait 'label:Done' --timeout 5000         # --gone waits for it to disappear
nsh devtools shot a.png                               # the window; --node SEL crops to a view
nsh devtools logs --level warn                        # -f follows
nsh devtools close
```

| Option | Meaning |
|---|---|
| `--session <PID>` | The session to use (default: the only running one; with several the command names them). |
| `--app-dir <DIR>` | The app's directory (default: the current one). |
| `--json` | Prints the protocol's answer as JSON. |

Selectors are the ones tests use: `id:`, `label:`, text and the rest ([Testing](./testing-and-trace.md)).
See [`nsh devtools`](./cli.md#nsh-devtools) for every command.

## The MCP server

`nsh devtools mcp` serves an app to an AI agent over the Model Context Protocol on standard input and output. Add it
to Claude Code with:

```sh
claude mcp add nsh-devtools -- /path/to/nsh devtools mcp --app-dir /path/to/my-app
```

or put it in `.mcp.json`:

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

The tools: `list_apps`, `launch_app`, `select_app`, `take_snapshot`, `take_screenshot`, `click`, `type`, `press_key`,
`scroll`, `hover`, `drag`, `wait_for`, `list_console`, `list_errors` and `close_app`. An agent asks for
`launch_app { "app": "examples/gallery" }`, reads the screen with `take_snapshot` and `take_screenshot`, acts with
`click` and `type`, and closes with `close_app`. The server releases the input after each action, so a command
line client can act between two of an agent's calls. Long text answers are cut at 20 000 characters with a note.

## WebDriver and Appium clients

`nsh devtools webdriver` serves apps over the W3C WebDriver protocol, so WebdriverIO, Selenium and the Appium
clients (Python, Java and the rest) drive an NSH app as they drive any other. It listens on `http://127.0.0.1:4723`
(`--port`, `--host`: loopback addresses only) and holds one session at a time.

```sh
nsh devtools webdriver --app-dir examples/counter
```

A new session starts the app. The capabilities:

| Capability | Meaning |
|---|---|
| `platformName` | `mac` (or `macos`) or `windows`. Required. |
| `appium:app` | The app's entry (a `.tsx` file or the app's directory, relative to `--app-dir`) or a packaged app built with `nsh package --devtools`. Required. |
| `appium:automationName` | `NSH`. |
| `nsh:options` | `storage`: an object of strings the app's storage starts with. `environment`: the test environment (clipboard, viewport, color scheme, clock). |

WebdriverIO:

```js
import { remote } from "webdriverio";

const driver = await remote({
  hostname: "127.0.0.1",
  port: 4723,
  capabilities: { platformName: "mac", "appium:app": "App.tsx", "appium:automationName": "NSH" },
});
const plus = await driver.$("~+1"); // accessibility id
await plus.click();
console.log(await driver.$('//*[@text="2"]').getText());
await driver.saveScreenshot("counter.png");
await driver.deleteSession(); // the app quits
```

The Appium Python client:

```python
from appium import webdriver
from appium.options.common import AppiumOptions

options = AppiumOptions()
options.load_capabilities({"platformName": "mac", "appium:app": "App.tsx", "appium:automationName": "NSH"})
driver = webdriver.Remote("http://127.0.0.1:4723", options=options)
driver.find_element("accessibility id", "+1").click()
print(driver.page_source)
driver.quit()
```

### Find strategies

| Strategy | Finds |
|---|---|
| `id` | views by `testID` (`id:<value>`). |
| `accessibility id` | views by label (`label:<value>`). |
| `-nsh selector` | a selector as tests write it (`id:`, `label:`, text and the rest). |
| `class name` | views by kind (`view`, `text`, `input`, ...). |
| `xpath` | an XPath expression on the page source, whose attributes are `id` (the view's id), `name` and `testID` (the `testID`, which the `id` strategy matches), `label`, `text`, `value`, `placeholder`, `role`, `x`, `y`, `width`, `height`, `visible`, `enabled`, `focused`, `selected` and `checked`. |

WebdriverIO's `$()` reads `-nsh selector:...` as CSS: use `findElement('-nsh selector', ...)` for that strategy.

`setTimeouts({ implicit })` makes a find wait for the view. An element's text is its text, else its value, else its
label. Element screenshots, actions (pointer and key sequences) and page source (the view tree as XML) are available.

### `execute`

`driver.execute("nsh: <name>", {...})` runs one of a fixed set of commands; other scripts answer
`unsupported operation`.

| Script | Argument | Returns |
|---|---|---|
| `nsh: console` | `{ level, text, limit }` | the app's console lines. |
| `nsh: errors` | | the errors since the last call. |
| `nsh: ready` | `{ timeoutMs }` (default 10000) | after the app is quiet. |
| `nsh: colorScheme` | `{ scheme: "dark" \| "light" }` | switches the color scheme. |

### Appium Inspector

Appium Inspector connects to `http://127.0.0.1:4723` (remote host `127.0.0.1`, port `4723`, path `/`) with the
capabilities above. It shows the view tree (page source) and the screenshot, and a tap there presses the view.

## Tests over DevTools

On macOS and Windows `nsh test --target` runs each app over its endpoint. Four flags go with it:

| Flag | Meaning |
|---|---|
| `--jobs <N>` | Runs up to N test files at once, each in a process of its own. |
| `--artifacts <DIR>` | Keeps, for every test file, `timeline.jsonl` (a line per step and per console line) and `failure.png` (the window at the first failure). |
| `--keep-open-on-failure` | Leaves a failed test's apps open and prints the URL and token for a client; press Enter to go on. |
| `--app <PATH>` | Runs the tests against a packaged app built with `nsh package --devtools`. |

A client can attach to an app a test run keeps open:

```text
nsh devtools: kept ws://127.0.0.1:52114/devtools open; token 3f9c…
```

## The protocol

The endpoint is a WebSocket at `/devtools` on the loopback address. A request is a JSON object with an `id`, a
`method` named `Domain.method` and `params`; the answer carries the `id` and a `result` or an `error` with a stable
`code` (`ERR_DEVTOOLS_…`). The endpoint also sends events (`Console.entry`, `Network.request`, `Trace.batch`).

`Protocol.describe` returns the machine-readable description: the protocol's version, every method with its
kind (`action` or `query`), what it does, the events and the error codes. Clients and agents read it instead of a
copy of this page.

The domains:

| Domain | What it covers |
|---|---|
| `Session` | `hello`, `setup`, `start`, `ready`, `release`, `quit`: connecting and starting the app. |
| `Input` | Press, type, scroll, drag, hover, keys, paste, drop, back. |
| `Tree` | Snapshot of the view tree, texts, counts, values, whether a view shows. |
| `State` | Waiting: for a view, a change, a frame. |
| `Capture` | Screenshots of the app and its windows. |
| `Environment` | Menus, dialogs, clipboard, clock, viewport, color scheme, opened URLs. |
| `Console` | The app's log lines: query and subscribe. |
| `Host`, `Storage` | Sending a message to the app as its embedding app; its stored entries. |

Input actions take the input for the connection that sends them; another connection's actions wait until the first
sends `Session.release` or disconnects, while reads stay open to everyone.

## Security

- The endpoint listens on the loopback address only.
- Every connection presents the token the app was started with (`NSH_DEVTOOLS_TOKEN`; generated when empty) and is
  refused without it. `nsh devtools` keeps it in a session file readable by its owner.
- The endpoint is a feature of development hosts and of packages built with `--devtools`. Builds without the feature
  contain no endpoint, no listener and no console collector.

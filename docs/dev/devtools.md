# NSH DevTools

NSH DevTools (experimental) is an endpoint inside a running app that lets a client automate and inspect it from
outside: the `nsh test --target` steps over a WebSocket, a tree snapshot, screenshots, the console. The
commands and MCP tools that use it are in `docs/dev/tooling.md` ("`nsh devtools`"), the user's page is
`website/docs/devtools.md`. The protocol, the commands and the MCP tools are experimental and may change.

## Parts

| Part | Code |
|---|---|
| The protocol: wire types and error codes (`protocol.rs`), the method table (`methods.rs`), the self-description `Protocol.describe` answers (`description.rs`), the tree snapshot and its compact text (`tree.rs`), the console collector (`console.rs`) | `crates/nsh-devtools` |
| The endpoint: a loopback WebSocket server, feature `server` (`server.rs`) | `crates/nsh-devtools` |
| The client the CLI uses, feature `client` (`client.rs`) | `crates/nsh-devtools` |
| The platform's `Handler`: a `Tester` of its own that answers the steps with the platform's input, the logger's feed into the collector, errors and panics (a panic's stack goes along when `RUST_BACKTRACE` is set, formatted off the panicking thread) | `crates/nsh-platform-macos/src/devtools.rs`, `crates/nsh-platform-windows/src/devtools.rs` |
| Launching and connecting a host (`launch.rs`, `host.rs`), `nsh test`'s per-file timeline and artifacts (`timeline.rs`, `artifacts.rs`), the commands (`cli.rs`) and the MCP server (`mcp.rs`) | `crates/nsh-cli/src/devtools/` |

The endpoint and the client are layers over the vocabulary that already exists: a method's params are the fields of
a `nsh_dev::protocol::TestStep`, its result the step's outcome. A client of the endpoint drives an app as `nsh test
--target` drives a host.

## Feature `devtools`

The platform crates (`nsh-platform-macos`, `nsh-platform-windows`) have a feature `devtools`; the feature `dev` turns it on.
Development hosts have it. A packaged app's runner (`nsh-macos-app`, `nsh-windows-app`) has the feature off, so a
release build holds no endpoint, no console collector and no listener: the code is not compiled in. `nsh package
--devtools` builds the runner with the feature on, from source (in a checkout, or for an app with Rust crates), for
`nsh test --app` and for tools; such a package is not for release.

## Environment variables

The endpoint is on when `NSH_DEVTOOLS` is set. The names are constants of `nsh-devtools` (`ENV_*`).

| Variable | Meaning |
|---|---|
| `NSH_DEVTOOLS` | The address to listen on (loopback only). Setting it turns the endpoint on. |
| `NSH_DEVTOOLS_TOKEN` | The token clients present in the connection's query (`/devtools?token=...`); a missing or wrong token is refused with HTTP 401. Empty: the endpoint generates one. |
| `NSH_DEVTOOLS_PORT_FILE` | A file the listening URL is written to, for the launcher to read. |
| `NSH_DEVTOOLS_WAIT` | The app waits for a client before it runs (below). |

## Launched sessions

The launcher (the CLI) starts the host with these variables, reads the port file, connects and takes the app through
a fixed order: wait, setup, start, ready.

1. With `NSH_DEVTOOLS_WAIT`, the app's load is held behind a gate (`Gate`) that `Session.start` opens: a packaged
   runner (`load_with_endpoint`) runs none of the app's code before it.
2. The client sends `Session.setup` (storage entries, tracing, the environment of the test) any number of times; it
   fixes what the app starts with and fails once the session has started.
3. `Session.start` hands the setup to the handler (`Tester::start_session`, then `Tester::set_up`) and only then
   opens the gate. The main queue is first in, first out, so the model is on the surface before the held app loads.
   A second start does nothing; a failed one gives the claim back, so a retry runs the same setup.
4. `Session.ready` waits for the first render and then until the app is quiet.

Until the gate opens, methods of the table answer `ERR_DEVTOOLS_NOT_STARTED`.

## The input lock

Methods of kind `action` (they change the app) take the input for the connection that sent them. Another connection's
action then answers `ERR_DEVTOOLS_INPUT_LOCKED` until the holder sends `Session.release` or disconnects. Queries and
the built-in methods (`Session.*`, `Console.*`, `Protocol.describe`) need no input, so any number of clients can read
an app that one client drives.

## Privileged methods

A method with `Some(PRIVILEGED)` in its row reaches what an app keeps private: `Host.send` (a message to the app as
its embedding app) and `Storage.entries` (its stored entries). `Session.hello` lists the capabilities the endpoint
grants, `["privileged"]`; the token is what authorizes a connection, there is no second gate per method.

## The console collector

`console::Collector` keeps the newest 10 000 log lines (`DEFAULT_CAPACITY`) of the process: time, level, target,
thread, message. The logger of a development host records into it (`devtools::record`, the logger of a runner built
with the feature calls it for every line), so there is one per process. `Console.query` filters (`since_ms`, `level`
and more severe, `target`, `text`, `limit` the newest) and `Console.subscribe` streams `Console.entry` events.

## The method table

Protocol methods come from the `TestStep` table in `methods.rs`: a row maps `Domain.method` to the step's tag, says
whether it is an `action` or a `query`, which capability it needs and what it does. The table is the single source:
the endpoint decodes params with `step_of`, the description (`Protocol.describe`, `Session.hello`'s method list) is
generated from it, and the input lock reads each row's kind. Nothing outside the table names a protocol method
(the built-in `Session.*`, `Console.*`, `Network.respond` and `Protocol.describe` live in `server.rs`).

To add a method:

1. Add the step to `nsh_dev::protocol::TestStep` (and make the platforms' `Tester` answer it) as for any test step.
2. Add its row to `METHODS` in `crates/nsh-devtools/src/methods.rs`: the name, the step's tag (`scene:<tag>` for a
   scene query), `Action` or `Query`, the capability, a one-line doc.
3. Add a sample of the step to `samples()` in `crates/nsh-devtools/tests/devtools/methods.rs`: the round-trip test
   checks that every sample maps to its method and back, and that every row has a sample.
4. The description picks the row up by itself; check `Protocol.describe` lists it.

## WebDriver

`nsh devtools webdriver` (`crates/nsh-cli/src/devtools/webdriver/`) is a W3C WebDriver HTTP server over the
protocol, so WebdriverIO, Selenium and the Appium clients drive an NSH app as they drive any other. It is a layer
of the CLI like the MCP server: each WebDriver command is a method of the protocol on the session of one launched
app, and the server adds nothing to the endpoint. An Appium server driver can proxy onto it later. The server
refuses what a web page in the user's browser could send (`guard`): any request with an `Origin` header (403),
any whose `Host` is not its own loopback address and port (403, against DNS rebinding), and a `POST` that is not
`application/json` (400).

| File | Holds |
|---|---|
| `mod.rs` | the command's `run`, the launcher (`HostLauncher`: `launch_devtools` for an entry on the development host or a packaged app) |
| `server.rs` | the routes, the one session, new / delete session, timeouts, the loopback check, `guard` (below) |
| `session.rs` | the capabilities (`Caps`) and the timeouts |
| `element.rs` | find strategies, element commands, stale detection |
| `actions.rs` | actions, screenshot, page source, window, the `execute` whitelist |
| `keys.rs` | WebDriver's special keys (Private Use characters) as `nsh/keymap` names |
| `error.rs` | WebDriver's error answers and the map from protocol errors |

### Mapping

| WebDriver | Protocol |
|---|---|
| new session | `launch_devtools` with `appium:app` and `nsh:options` (`storage`, `environment`), then `Session.setup`, `start`, `ready` |
| delete session | `Session.release`, `Session.quit`; the host is dropped and the app's data goes |
| find (`id`, `accessibility id`, `-nsh selector`) | `Tree.query` with `id:<value>`, `label:<value>` or the selector as written |
| find (`class name`, `xpath`) | `Tree.snapshot` filtered by node kind, or rendered as XML (`nsh_devtools::xml`) and evaluated in the server |
| click, clear | `Input.press`, `Input.clear` on `view:<id>` |
| send keys | `Input.type` for text; `Input.key` for special keys, after `Input.type` with empty text focused the view |
| text, name, attribute, property, rect, displayed, enabled, selected | `Tree.describe` (name: the node kind; text: text, else value, else label) |
| actions | `Input.pointer`, `Input.key`: consecutive actions of one source kind go in one call |
| screenshot, element screenshot | `Capture.screenshot` (with `node` for an element) |
| page source | `Tree.snapshot` rendered as XML |
| window, handles, rect | one window, `main`; its rect is the root view's size |
| `execute/sync` | `nsh: console` (`Console.query`), `nsh: errors` (`Console.takeErrors`), `nsh: ready` (`Session.ready`), `nsh: colorScheme` (`Environment.setColorScheme`) |

### One session per server

The server holds one app: `POST /session` while another is open or starting answers `session not created`;
`GET /status` reports `ready` accordingly. The server is a client of the app's input lock, so a command line client
can read the app while a script drives it. Ending the server (Ctrl-C) ends the session and quits its app. `--host`
accepts loopback addresses only.

The server ends the builds and hosts it started with it. On Windows it puts itself in a job object that kills its
processes when it closes (`run::KillOnClose::current_process`), so a server killed with `TerminateProcess`
(`Stop-Process -Force`) takes its `cargo build` and hosts along. Ctrl-C and SIGTERM end them on the other systems
(`ui::capture_all` kills the build it waits for once a stop is requested). A server killed hard on macOS or Linux
(`kill -9`) can leave a build running: it finishes alone, or end it by hand.

### Elements

An element id is the view's id. Every element command asks `Tree.describe` first: a view that left the tree is
`stale element reference`. A list cell is recycled with its id kept, so a find remembers the `testID` and text it saw
and a cell that shows others is stale too. The implicit timeout makes a find poll (every 50 ms) until the deadline.

### Errors

| Protocol error | WebDriver error (HTTP status) |
|---|---|
| `ERR_DEVTOOLS_STALE` | `stale element reference` (404) |
| `ERR_DEVTOOLS_BAD_PARAMS` | `invalid argument` (400) |
| a client timeout, or a step failure that says it timed out | `timeout` (500) |
| a step failure that names a selector | `invalid selector` (400) |
| anything else | `unknown error` (500) |

The server's own answers: `session not created`, `invalid session id`, `no such element`, `unknown command`,
`unknown method`, `unsupported operation` (window resizing, scripts, `execute/async`, wheel input) and
`move target out of bounds`. A body is `{"value":{"error","message","stacktrace"}}`.

### Adding a command

1. Add the route to the file of its kind (`server.rs` for sessions, `element.rs`, `actions.rs`); a handler takes
   `Server::current(sid)` and runs the blocking `Session` calls inside `blocking`.
2. Make it call protocol methods only; when the protocol lacks one, add it to the protocol first (the table above
   is the list of what the server needed, and a workaround here belongs in the protocol).
3. Map a failure through `WdError` (`From<ErrorBody>`), and add a test with the fake `Session` of the file's tests
   (`cargo test -p nsh-cli devtools_webdriver`).
4. Add the command to the mapping table here and to `website/docs/devtools.md` and its `zh-CN` page.

## Platforms

macOS and Windows serve the endpoint. `nsh test --target macos|windows` runs the apps over it (also `--jobs`,
`--artifacts`, `--keep-open-on-failure`, `--app`; `docs/dev/tooling.md`). The other targets keep the dev server's test
session, which is what `nsh test --target` used before.

## Status

The protocol's `version` (`PROTOCOL_VERSION`) is 1 and `Protocol.describe` reports `"stability": "experimental"`: names,
params and the commands may change without the compatibility rules of what apps use.

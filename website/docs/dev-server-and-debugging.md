---
title: Dev Server and Debugging
description: Run TSX and Rust apps with nsh run and nsh dev, connect simulators, emulators and devices, and use rebuilds, hot module replacement, logs, the error overlay and source-mapped stacks.
order: 60
group: tools
---

# Dev Server and Debugging

During development, NSH rebuilds your app on every save.

- A Rust app (one whose `nsh.json` names a `"root"`) is built into its own runner, which `nsh run` starts. A change
  to its crate rebuilds it and restarts the app.
- A TSX app is pushed to every connected host, and each host updates in place. A **host** is the native app that runs
  your NSH app, such as the development host window or your own app with the development SDK.

Two commands do this:

- `nsh run` builds the app **and** launches it: an AppKit window on macOS, a GTK window on Linux, a Win32 window
  on Windows, an iOS simulator, an Android device or emulator, or a browser page. For TSX it also starts the dev
  server, which the development host connects to.
- `nsh dev --server-only` starts the dev server for TSX, for hosts you start yourself: your own app with the
  development SDK, a device on the network, or several hosts at once.

```sh
# A Rust app, from its directory
nsh run                             # AppKit window (macOS); GTK window (Linux); Win32 window (Windows)
nsh run --target ios                # iOS simulator
nsh run --target android            # Android device or emulator

# A TSX app
nsh run App.tsx                     # AppKit window (macOS); GTK window (Linux); Win32 window (Windows)
nsh run App.tsx --target macos      # AppKit window (macOS)
nsh run App.tsx --target ios        # iOS simulator
nsh run App.tsx --target android    # Android device or emulator
nsh dev App.tsx --server-only       # server only
```

`nsh dev` also opens the native host by default. For mobile hosts, pass `--target ios` or `--target android`.

## `nsh run`

| Flag | Description |
|---|---|
| `--target <macos \| linux \| windows \| ios \| android \| gpui \| web \| catalyst>` | Where to run. Default `macos` on macOS (AppKit), `linux` on Linux, `windows` on Windows. A Rust app runs on all of them but `gpui`, which embeds its root in a GPUI app ([GPUI](./gpui.md)). |
| `--device <name, UDID or serial>` | The iOS simulator to use, by name or UDID. Default: a booted one, else the newest iPhone (booted for you). With `--target android`: the device or emulator, by its serial (as `adb devices` lists it). Default: the one `ANDROID_SERIAL` names, else the first one. |
| `--port <port>` | Dev server port, replacing the previous NSH session on it. Default: the port this app had last, replacing its previous session, else the first free one from 8790. |
| `--release` | Serve release builds of TSX (minified, no debug info, no hot updates). |
| `--trace` | Log the time of every commit on the host (TSX apps). |

What each target does:

- **`macos`** (macOS) runs the AppKit development host, a desktop app in a 1200×800 window. Stopping `nsh run`
  (Ctrl-C, or `kill`) closes the window, and closing the window ends `nsh run`. Cmd+R reloads the latest build.
  Requires macOS 14 or later ([macOS](./macos.md)).
- **`linux`** (Linux) runs the GTK development host in a 390×844 window. It starts and stops the same way as `macos`.
  Requires GTK 4.14 or later ([Linux](./linux.md)).
- **`windows`** (Windows) runs the Win32 development host in a 1200×800 window, the same way. The host never outlives
  `nsh run`, even when the console is closed. Requires Windows 10 1809 or later ([Windows](./windows.md)).
- **`ios`** installs the development host in the simulator, launches it pointed at the dev server and keeps watching.
  The simulator shares the Mac's network, so the host reaches the server on `127.0.0.1`. Stopping `nsh run` (Ctrl-C)
  closes the host. Otherwise it would join the next session. Requires Xcode.
- **`android`** installs the development host APK, and reinstalls it only when the runtime changed. It runs
  `adb reverse tcp:<port> tcp:<port>` so the device's own `127.0.0.1` reaches the server, then starts the host. It
  works with an emulator or a USB device. Requires the Android SDK (`ANDROID_HOME`). When `nsh run` ends (Ctrl-C
  included), it stops the host and removes the port forwarding, so the host does not join the next session.

**Development tool:** `catalyst` (macOS) runs the iOS development host as a Mac Catalyst app. Use it only to check
Catalyst-specific behavior, or for an app that embeds NSH in a Mac Catalyst app. It starts and stops with `nsh run`
like `macos`.

However `nsh run` ends, it stops the host it started. It closes the macOS, Linux or Windows host, terminates the
development host in the simulator, force-stops it on the device, or quits the Catalyst app.

For a Rust app, each target runs the app's own runner, built with its crate. On the desktops this is a debug build
that runs in place, with its output in the terminal. On iOS and Android it is an installed package. For TSX the hosts
are generic: they run any NSH app the server sends, so nothing is built per app. An app with its own crate gets hosts
built with that crate.

## `nsh dev`

```sh
nsh dev App.tsx --server-only
```

```text
✓ built App.tsx (3.2 KB js, 9.8 KB bytecode, 21ms)
→ dev server ws://127.0.0.1:8790/ws
  devices on this network: ws://192.168.1.20:8790/ws
```

| Flag | Description |
|---|---|
| `--port <port>` | Dev server port. As for `nsh run`; with `--server-only`, the default is 8790, replacing the previous NSH session on it. |
| `--host <ipv4>` | IPv4 address to bind. Defaults to `0.0.0.0` for `--server-only` and `--target ios`, `android` or `web`, and `127.0.0.1` for local targets. With `--target android`, only `127.0.0.1` and `0.0.0.0` are supported (`adb reverse` forwards to this machine's `127.0.0.1`). |
| `--release` | Serve release builds. |

By default, `nsh dev --server-only` listens on every address of this computer (`0.0.0.0`), so phones and other
machines on the network can connect. `nsh run` and `nsh dev` with a host on this computer (a window, the terminal,
Catalyst) default to `127.0.0.1`. With `--target ios`, `android` or `web` they listen on every address, like
`--server-only`. Pass `--host <ipv4>` to choose the bind address. For example,
`nsh dev App.tsx --server-only --host 127.0.0.1` keeps a server-only session on loopback only.

Connect hosts to the printed URL:

- **Your own iOS app**, linked with `NativeShellDev.xcframework`:
  `try app.connectDevServer(URL(string: "ws://192.168.1.20:8790/ws")!)`. See [Embedding in iOS apps](./ios.md#development).
- **Your own Android app**, with `nsh-dev.aar`: `nsh.connectDevServer("ws://192.168.1.20:8790/ws")`, or
  `adb reverse tcp:8790 tcp:8790` and `ws://127.0.0.1:8790/ws`. See [Embedding in Android apps](./android.md#development).

Any number of hosts can connect at once, and every build goes to all of them. A host that connects later receives
the current build right away. Some hosts cannot run the app because they lack a UI set the app imports, for example
a development host built for another app. Such a host gets no build. It shows the reason, and the terminal prints a
`warn` line with the command that installs the right host.

> [!NOTE]
> A device on the LAN must reach your computer on the dev server's port. It needs the same network and no firewall
> in between. The same port serves assets over plain HTTP: the files the app imports, and the images, fonts, audio
> and video in the app's directory, except in hidden directories. Sources and configuration (`nsh.json`, `.env`) stay
> on your computer. The WebSocket serves the development hosts and the dev server's own web page.

## Rust edits

Rust is compiled, so it can't be patched in place. A change to the app's crate, or to a crate it depends on by path,
rebuilds the app, and `nsh run` restarts it with the new code. The app's state starts fresh. Keep what must survive a
restart in [storage](../modules/nsh.md#storage). Build errors print in the terminal, and the next save that compiles
runs again. In an app with both TSX and Rust, TSX edits are hot updates (below) and Rust edits restart the app.

## Hot module replacement

Hot module replacement updates a running app without restarting it. TSX development builds keep one module per
source file. When you save, NSH rebuilds, compares the modules with the last good build and sends a **patch** with
only the changed modules. The host applies it in place in well under a millisecond. From saving a file to seeing the
change takes roughly 120 ms.

What happens to your app:

- **Only the components you edited render again.** Every top-level component (a capitalized function that returns JSX)
  is wrapped in a stable proxy. When its code changes, or other code in its module changes, its instances render
  again with the new code. Components whose code did not change keep their instances, with their signals and state.
  Edit a button's style and the screen stays where it was, with the same input text and scroll position.
- **Modules that changed run again.** So do modules that import a changed module which cannot take the update by
  itself, up to the entry. Module-level code therefore runs again, except for what keeps its state:
- **Top-level signals and navigation stacks keep their state.** A `const` made by `signal(…)` or `createNavStack(…)`
  at the top of a module keeps its value (for a stack, its pages) across edits, as long as its initializer is the
  same. Change the initializer (`signal(0)` to `signal(10)`) to start fresh.
- **Top-level `context()` objects keep their identity**, taking the new default value, so providers and consumers in
  different modules keep matching.
- **Editing the entry keeps the app mounted.** When `mount(App)` (or `mount(() => <App />)`) runs again with the
  same component, it keeps what it mounted. Only the components you edited render again. If you edit `App` itself,
  it renders again along with its children, whose own state starts fresh. A navigator made at the top of the module
  still keeps its routes, so a pushed screen stays pushed. If the `mount` call itself changed (another component or
  props), the app mounts again.
- **A component that throws** reports the error and renders nothing until you fix it. The rest of the app keeps
  running. The next good save recovers.

The host **reloads the whole app** instead of patching when:

- the change imports an NSH module the running app did not import yet (for example the first `import … from "nsh"`);
- the app's `capabilities` in `nsh.json` changed;
- the host is not running the build the patch is based on (it just connected, or a previous load failed);
- a patch cannot be applied.

> [!TIP]
> State a component keeps in its own signals resets when that component, or one that renders it, is edited. Keep
> state that must survive edits in top-level signals (or a store module), and make navigators at the top of a module.

`--release` serves release builds without hot updates, so every save reloads the app. Use it to check the minified
build. Anything loaded from a dev server skips the runtime's hardening, with or without `--release`. Hardening is the
set of restrictions applied to production code, such as removing `eval`. See
[Sandbox and capabilities](./sandbox-and-capabilities.md#hardening).

## Logs

A Rust app logs with the `log` crate (`log::info!`, `log::warn!`, `log::error!`). On the desktops and in the iOS
simulator, `nsh run` shows these lines in its terminal, with the app's output.

In TSX, `console.log`, `console.info`, `console.debug`, `console.warn` and `console.error` write to the native log
with target `js`. While a host is connected, its log lines are forwarded to the terminal running `nsh run` or
`nsh dev`. Your `console.log` lines appear as they are, prefixed with the device's name when several devices are
connected. Warnings and errors always follow the device's name:

```text
• iPhone 16 Pro connected, build 1
loading more after 1000
warn iPhone 16 Pro: deprecated option
error iPhone 16 Pro: uncaught exception: TypeError: cannot read property 'id' of undefined
          at Row (demos/Photos.tsx:18:22)
```

`console.log` formats values like a browser console: strings as they are, errors with their stack, functions by
name, objects and arrays as JSON. When no host is connected, logs go to Xcode's console on iOS (when Xcode runs the
app) and to `adb logcat` on Android. A packaged app logs to the platform's log
([A packaged app's logs](#a-packaged-apps-logs)).

An unhandled rejection is also logged as an error, as in a browser. A rejection is unhandled when the promise still
has no handler after its microtasks have run. Examples are an `async` function that throws while nobody awaits it, a
`Promise.reject(…)` nobody catches, or a failed call into Rust that nothing waits for. The line goes wherever
`console.error` goes (the terminal, the platform's log, the browser console on the web). Its stack points at your
sources:

```text
error iPhone 16 Pro: unhandled rejection: Error: HTTP 404
          at loadProfile (App.tsx:12:11)
```

A handler attached later in the same turn (`const p = load(); …; p.catch(…)`) still counts. Where the failure
matters, await the promise or give it a `.catch`.

Other terminal output is one line per step: `✓` for done, `→` for where to connect, `•` for what a device did. Output
from the tools NSH runs (cargo, the hosts) appears only when one of them fails. `nsh run -v` (or `NSH_VERBOSE=1`)
prints everything: every log line with its level and source, cargo's output and the hosts' output.

`--trace` adds the cost of every commit:

```text
perf commit: 10 ops, 151µs (list js 2µs), 8 mount items, 27 nodes
```

See [Testing](./testing-and-trace.md) for what these numbers mean.

### A packaged app's logs

On macOS and iOS, a packaged app writes to the unified log, Apple's system log. It logs its warnings and errors,
and its `js` lines (`console.log`, `console.info`, `console.warn`, `console.error`, unhandled rejections). The
subsystem is the app's id (`nsh.json` `"id"`, its bundle identifier). The category is the line's target: `js` for
`console.*`, or a Rust module's path for Rust lines (`nsh_engine::updates`). In the Console app, pick the Mac or a
connected iPhone, start streaming and search for `subsystem:com.example.notes`. In a terminal, use `log`:

```sh
log stream --predicate 'subsystem == "com.example.notes"'                              # as they come
log show --last 1h --predicate 'subsystem == "com.example.notes" AND category == "js"'   # the last hour's console.* lines
xcrun simctl spawn booted log stream --predicate 'subsystem == "com.example.notes"'     # an app in the iOS simulator
```

Errors are logged as the log's errors (`messageType == error` in a predicate). Warnings start with `WARN`. A line
longer than about 1 KB continues in the following entries.

A Mac app also prints its lines to standard error, wherever that goes: the terminal it was started from, a file
(`2> app.log`, `open --stderr app.log`, launchd's `StandardErrorPath`) or a pipe. An iOS app that embeds
`NativeShell.xcframework` logs the same lines under its own bundle identifier. Xcode's console shows them while Xcode
runs the app, even with `OS_ACTIVITY_MODE=disable` in the scheme.

Android apps log to logcat (`adb logcat -s nsh`), Windows apps to the debugger's output (`OutputDebugString`, which
DebugView and Visual Studio show) and Linux apps to their standard error.

#### Log files and levels on the desktop

A packaged macOS, Windows or Linux app also writes its log lines to `logs/app.log` in its data directory, in
addition to the platform's log. The table shows the default paths. To use another directory, set `logDir` in
`nsh.json` ([Project structure](./project-structure.md)).

| Platform | Log file |
|---|---|
| macOS | `~/Library/Application Support/<app id>/logs/app.log` |
| Windows | `%APPDATA%\<app id>\logs\app.log` |
| Linux | `$XDG_DATA_HOME/<app id>/logs/app.log` (`~/.local/share/<app id>/logs/app.log`) |

Each line has the time in UTC, the level, the target and the message:
`2026-11-12T08:30:05.123Z INFO  js: signed in`. At 5 MB the file is renamed `app.1.log` and a new `app.log` starts.
NSH keeps the three previous files (`app.1.log` to `app.3.log`). If the file cannot be written, the app keeps running
as usual.

`log_file()` in Rust and `process.logFile` in TSX return the file's path. The app can attach the file to a
diagnostics report or offer to open its folder. The value is `None` (`null`) when lines go only to the platform's
log: in development hosts, embedding apps, mobile and the web.

```tsx tabs
import process from "process";

const report = { logFile: process.logFile, version: "1.4.0" };
```

```rust tabs
use nsh_core::prelude::*;

// The log file shown selected in Finder, File Explorer or the file manager.
if let Some(file) = log_file() {
    spawn(async move {
        let _ = reveal_path(file).await;
    });
}
```

By default a packaged app logs its warnings, errors and `js` lines. Under `nsh run` it logs every line from info
up. The `RUST_LOG` environment variable overrides the levels, in `env_logger` syntax. It applies to both the file and
the platform's log:

```sh
RUST_LOG=info ./notes                                  # every line from info up
RUST_LOG=warn,notes=debug,js=info ./notes              # the app's crate in detail
```

On Windows, set it for one run in PowerShell (`$env:RUST_LOG="info"; .\Notes.exe`), or for the user in the
system's environment variables.

When Rust code panics, the file records it at any log level. It writes an `ERROR panic:` line with Rust's panic
message (the thread, the location and the message). With `RUST_BACKTRACE=1`, it also writes the backtrace. A
packaged app's standard error is often hidden, for example in a Windows GUI program or a Mac app opened from Finder.
So this file is where you can read the cause of a crash.

## The error overlay

These errors show a red overlay over the app on the host, with the message and stack:

- a build failure (a syntax error, an unknown element, a missing import), with a code frame pointing at the line;
- an error while the bundle is evaluated;
- an uncaught exception in an event handler, timer or effect.

Tap the overlay to dismiss it. It also disappears on the next successful reload. Build failures are printed in the
terminal too:

```text
error building App.tsx failed
App.tsx

  x <Buton> names nothing in scope
   ,-[3:11]
 2 | function App() {
 3 |   return <Buton />;
   :           ^^^^^
 4 | }
```

The bundler removes TypeScript types without checking them. For type errors, run [`nsh check`](./cli.md#nsh-check)
or rely on your editor. `nsh check` uses TypeScript's native compiler with the `tsconfig.json` that `nsh init`
writes, and needs no Node.

In release builds (an embedding app with the release SDK, or a packaged app), errors are not shown over the app.
They are logged and passed to the host app's error handler (`NshAppView.onError`, `NshView.setErrorListener`).

## Source maps and stack traces

Bundling produces a source map that covers every step (TypeScript → JSX → the bundle). The dev server sends it with
each build. Before printing or showing a stack trace, the runtime maps bundle positions back to your sources. Stacks
point at `App.tsx:2:28`, not at the bundle:

```text
error Mac Catalyst: uncaught exception: Error: tap boom
          at <anonymous> (App.tsx:2:58)
```

`nsh build` writes the map as `bundle.js.map`. An embedding app that loads a bundle from a folder reads the map from
next to the bundle. [Signed packages](./signed-packages.md) carry the map too. You can instead keep it next to the
package and apply it with `nsh symbolicate`
([Errors from published packages](./signed-packages.md#errors-from-published-packages)).

## Debugging tips

- **Nothing updates after an edit**: check the terminal for a build error. The overlay shows it on the host too.
- **The app keeps old state you expected to reset**: hot replacement keeps unchanged components' state, top-level
  signals and navigators on purpose. Restart with Ctrl-C and `nsh run`, or change the state's initializer.
- **A request fails only in a packaged or embedded build**: check the app's [capabilities](./sandbox-and-capabilities.md).
  Refusals are logged with the key to add.
- **Something is slow**: reproduce it headless with [`nsh test --trace`](./testing-and-trace.md), which prints every
  native view operation. Or run a TSX app with `--trace`.
- **Type errors in the editor**: `nsh run` and `nsh dev` keep `.nsh/types` up to date. See
  [Project structure](./project-structure.md).

## See also

- [CLI](./cli.md)
- [Testing](./testing-and-trace.md)
- [Embedding in iOS apps](./ios.md), [Embedding in Android apps](./android.md)

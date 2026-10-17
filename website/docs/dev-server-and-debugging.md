---
title: Dev Server and Debugging
description: Run apps with nsh run and nsh dev, connect simulators, emulators and devices, and use hot module replacement, logs, the error overlay and source-mapped stacks.
order: 60
group: tools
---

# Dev Server and Debugging

During development, NSH builds your app on every save and pushes it to every connected host, which updates in place.
Two commands do this:

- `nsh run` starts the dev server **and** launches a development host: a Mac Catalyst or AppKit window, a GTK window on
  Linux, a Win32 window on Windows, an iOS simulator or an Android device or emulator.
- `nsh dev` starts only the dev server, for hosts you start yourself: your own app with the development SDK, a device
  on the network, or several hosts at once.

```sh
nsh run App.tsx                     # Mac Catalyst window, 390×844 (macOS); GTK window (Linux); Win32 window (Windows)
nsh run App.tsx --target macos      # AppKit window (macOS)
nsh run App.tsx --target ios        # iOS simulator
nsh run App.tsx --target android    # Android device or emulator
nsh dev App.tsx                     # server only
```

## `nsh run`

| Flag | Description |
|---|---|
| `--target <catalyst \| ios \| android \| macos \| linux \| windows>` | Where to run. Default `catalyst` on macOS (an iPhone-sized, 390×844 Mac window), `linux` on Linux, `windows` on Windows. |
| `--device <name or UDID>` | The iOS simulator to use. Default: a booted one, else the newest iPhone (booted for you). |
| `--port <port>` | Dev server port. Default 8790; the next free port is used if it is taken. |
| `--release` | Serve release builds (minified, no debug info, no hot updates). |
| `--trace` | Log the time of every commit on the host. |

What each target does:

- **`catalyst`** (macOS) runs the development host as a Mac app. Stopping `nsh run` (Ctrl-C, or `kill`) closes it, and
  closing the window ends `nsh run`.
- **`macos`** (macOS) runs the AppKit development host, a desktop app in a 1200×800 window, the same way; Cmd+R
  reloads the latest build. Requires macOS 14 or later ([macOS](./macos.md)).
- **`linux`** (Linux) runs the GTK development host in a 390×844 window, the same way: stopping `nsh run` closes
  it, closing the window ends `nsh run`. Requires GTK 4.14 or later ([Linux](./linux.md)).
- **`windows`** (Windows) runs the Win32 development host in a 1200×800 window, the same way; the host never outlives
  `nsh run`, even when the console is closed. Requires Windows 10 1809 or later ([Windows](./windows.md)).
- **`ios`** installs the development host in the simulator, launches it pointed at the dev server and keeps watching.
  The simulator shares the Mac's network, so the host reaches the server on `127.0.0.1`. Stopping `nsh run` (Ctrl-C)
  closes the host, which would otherwise join the next session. Requires Xcode.
- **`android`** installs the development host APK (again only when the runtime changed), runs
  `adb reverse tcp:<port> tcp:<port>` so the device's own `127.0.0.1` reaches the server, and starts it. Works with an
  emulator or a USB device. Requires the Android SDK (`ANDROID_HOME`). When `nsh run` ends (Ctrl-C too), it stops the
  host and removes that port forwarding, so the host does not join the next session.

When `nsh run` ends, however it ends, it stops the host it started: it quits the Catalyst app or closes the macOS, Linux or Windows host, terminates the
development host in the simulator, or force-stops it on the device.

The hosts are generic: they run any NSH app the server sends, so there is nothing to build per app.

## `nsh dev`

```sh
nsh dev App.tsx
```

```text
✓ built App.tsx (3.2 KB js, 9.8 KB bytecode, 21ms)
→ dev server ws://127.0.0.1:8790/ws
  devices on this network: ws://192.168.1.20:8790/ws
```

| Flag | Description |
|---|---|
| `--port <port>` | Dev server port. Default 8790 (the next free one if taken). |
| `--release` | Serve release builds. |

Connect hosts to the printed URL:

- **Your own iOS app**, linked with `NativeShellDev.xcframework`:
  `try app.connectDevServer(URL(string: "ws://192.168.1.20:8790/ws")!)`. See [Embedding in iOS apps](./embedding-ios.md#development).
- **Your own Android app**, with `nsh-dev.aar`: `nsh.connectDevServer("ws://192.168.1.20:8790/ws")`, or
  `adb reverse tcp:8790 tcp:8790` and `ws://127.0.0.1:8790/ws`. See [Embedding in Android apps](./embedding-android.md#development).

Any number of hosts can be connected at once; every build goes to all of them. A host that connects later receives the
current build right away. A host that cannot run the app, because it lacks a UI set the app imports (a development
host built for another app), gets no build: it shows why, and the terminal prints a `warn` line with the command that
installs the app's host.

> [!NOTE]
> A device on the LAN needs to reach your computer on the dev server's port: same network, no firewall in between.
> Assets (images, fonts) are served over plain HTTP from the same port.

## Hot module replacement

Development builds keep one module per source file. When you save, NSH rebuilds, compares the modules with the last
good build and sends a **patch** with only the modules that changed. The host applies it in place in well under a
millisecond; from saving a file to seeing the change takes roughly 120 ms.

What happens to your app:

- **Only the components you edited render again.** Every top-level component (a capitalized function that returns JSX)
  is wrapped in a stable proxy. When its code changes (or the rest of its module's code, which it may use), its
  instances render again with the new code; components whose code did not change keep their instances, and with them
  their signals and state. Edit a button's style and the
  screen stays where it was, with the same input text and scroll position.
- **Modules that changed run again**, and so do the modules importing a changed module that cannot take the update
  by itself, up to the entry. Module-level code therefore runs again, except for what keeps its state:
- **Top-level signals and navigators keep their state**: a `const` made by `signal(…)` or `createNavigator(…)` at the
  top of a module keeps its value (a navigator its screens) across edits, as long as its own initializer is the same;
  change the initializer (`signal(0)` to `signal(10)`) to start afresh.
- **Top-level `context()` objects keep their identity**, taking the new default value, so providers and consumers in
  different modules keep matching.
- **Editing the entry keeps the app mounted.** `mount(App)` (or `mount(() => <App />)`) run again with the same
  component keeps what it mounted: only the components you edited render again. Edit `App` itself and it renders
  again, with what it renders (their own state starts afresh), but a navigator made at the top of the module keeps its
  routes, so a pushed screen stays pushed. A `mount` call that changed (another component, props) mounts the app
  again.
- **A component that throws** reports the error and renders nothing until you fix it; the rest of the app keeps
  running. The next good save recovers.

The host **reloads the whole app** instead of patching when:

- the change imports an NSH module the running app did not import yet (for example the first `import … from "nsh/list"`);
- the app's `capabilities` in `nsh.json` changed;
- the host is not running the build the patch is based on (it just connected, or a previous load failed);
- a patch cannot be applied.

> [!TIP]
> State a component keeps in its own signals resets when that component, or one that renders it, is edited. Keep
> state that must survive edits in top-level signals (or a store module), and make navigators at the top of a module.

`--release` serves release builds, without hot updates: every save reloads the app. Use it to check the minified build.
Anything loaded from a dev server, `--release` or not, skips the runtime's hardening (`eval` stays available); see
[Sandbox and capabilities](./sandbox-and-capabilities.md#hardening).

## Logs

`console.log`, `console.info`, `console.debug`, `console.warn` and `console.error` go to the native log with target
`js`. While a host is connected, its log lines are forwarded to the terminal running `nsh run` / `nsh dev`: your
`console.log` lines as they are (prefixed with the device's name when several devices are connected), warnings and
errors after the device's name:

```text
• iPhone 16 Pro connected, build 1
loading more after 1000
warn iPhone 16 Pro: deprecated option
error iPhone 16 Pro: uncaught exception: TypeError: cannot read property 'id' of undefined
          at Row (demos/Photos.tsx:18:22)
```

`console.log` formats values like a browser console: strings as they are, errors with their stack, functions by name,
objects and arrays as JSON. When the host is not connected, logs go to the platform's log (Xcode's console / the
unified log on iOS, `adb logcat` on Android).

The rest of the terminal is one line per step (`✓` done, `→` where to connect, `•` what a device did); the output of
the tools NSH runs (cargo, the hosts themselves) is only printed when one fails. `nsh run -v` (or `NSH_VERBOSE=1`)
prints everything: every log line with its level and source, cargo's output and the hosts' own.

`--trace` adds the cost of every commit:

```text
perf commit: 10 ops, 151µs (list js 2µs), 8 mount items, 27 nodes
```

See [Testing with nsh trace](./testing-and-trace.md) for what these numbers mean.

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

The bundler removes TypeScript types without checking them; run [`nsh check`](./cli.md#nsh-check) (TypeScript's
native compiler with the `tsconfig.json` `nsh init` writes; no Node needed) or rely on your editor for type errors.

In release builds (an embedding app with the release SDK, a packaged app), errors are logged and passed to the host
app's error handler (`NshAppView.onError`, `NshView.setErrorListener`) but not shown over the app.

## Source maps and stack traces

Bundling produces a source map that composes every step (TypeScript → JSX → the bundle), and it is sent along with each
build. Before a stack trace is printed or shown, the runtime rewrites the bundle's positions to your sources, so stacks
point at `App.tsx:2:28`, not at the bundle:

```text
error Mac Catalyst: uncaught exception: Error: tap boom
          at <anonymous> (App.tsx:2:58)
```

`nsh build` writes the map as `bundle.js.map`; an embedding app that loads a bundle from a folder reads it from next to
the bundle, and [signed packages](./signed-packages.md) carry it too.

## Debugging tips

- **Nothing updates after an edit**: check the terminal for a build error; the overlay shows it on the host too.
- **The app keeps old state you expected to reset**: hot replacement keeps unchanged components' state, top-level
  signals and navigators on purpose. Restart with Ctrl-C and `nsh run`, or change the state's initializer.
- **A request fails only in a packaged or embedded build**: check the app's [capabilities](./sandbox-and-capabilities.md);
  refusals are logged with the key to add.
- **Something is slow**: run with `--trace`, or reproduce it headless with [`nsh trace`](./testing-and-trace.md), which
  prints every native view operation.
- **Type errors in the editor**: `nsh run` and `nsh dev` keep `.nsh/types` up to date; see
  [Project structure](./project-structure.md).

## See also

- [CLI](./cli.md)
- [Testing with nsh trace](./testing-and-trace.md)
- [Embedding in iOS apps](./embedding-ios.md), [Embedding in Android apps](./embedding-android.md)

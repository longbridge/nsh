# Rust application

The application has two entry points sharing the Rust UI in `src/lib.rs`:

```sh
cargo run
```

This starts the Rust root directly on the machine's native desktop backend: GTK 4 on Linux, AppKit on macOS, Win32 on Windows. Install the backend's development libraries before building.

```sh
nsh run                      # this machine's backend; --target ios, android, web, macos, linux, windows or gpui
nsh package --target web     # or ios, android, macos, linux, windows, gpui
```

`nsh.json` names the root (`"root": "App"`), so `nsh run` and `nsh package` mount it on every platform, the browser included, without QuickJS: the app is Rust and native code from start to end (in the browser, with NSH's page script). Rust edits rebuild and restart the app.

```sh
nsh run App.tsx
```

This starts the TSX entry instead, which imports the same Rust component from `app/native`: write screens in TSX on top of the Rust components and modules, with hot reloading (TSX runs on QuickJS, in the browser too).

The `nsh` commands build the app's runners from NSH's crates on crates.io, at the CLI's version: `Cargo.toml` asks for that version, so keep its `nsh-*` dependencies at the version of the `nsh` you run. To build against an NSH checkout instead, set `NSH_SOURCE` to it. The first TSX build generates the `app/native` TypeScript declarations.

`cargo run` uses published NSH crates. UI state and event handlers execute directly in Rust. The native entry accepts scripted window checks through `NSH_SCRIPT`, for example:

```sh
NSH_SCRIPT='wait 800; shot window.png; quit' cargo run
```

# Rust application

The application has two entry points sharing the Rust UI in `src/lib.rs`:

```sh
cargo run
```

This starts the Rust root directly on the machine's native desktop backend: GTK 4 on Linux, AppKit on macOS, Win32 on Windows. Install the backend's development libraries before building.

```sh
nsh run
```

This starts `App.tsx`, which imports the same Rust component from `app/native`. Choose a development host with `--target ios`, `android`, `macos`, `linux`, `windows` or `gpui`.

The TSX Rust-component toolchain uses an NSH source checkout: either run the CLI built in that checkout or set `NSH_SOURCE` to a compatible checkout. Its first build generates the `app/native` TypeScript declarations. Rust edits rebuild and restart the development host; TSX edits hot-reload.

`cargo run` uses published NSH crates. UI state and event handlers execute directly in Rust. The native entry accepts scripted window checks through `NSH_SCRIPT`, for example:

```sh
NSH_SCRIPT='wait 800; shot window.png; quit' cargo run
```

# Rust application

A Rust application with two entry points sharing `src/lib.rs`:

- `cargo run` starts the Rust root directly on GTK 4 (Linux), AppKit (macOS) or Win32 (Windows).
- `nsh run` starts `App.tsx`, which imports that same Rust component from `app/native`. The TSX entry uses the existing development hosts, including iOS and Android.

The direct Rust executable uses the platform crate with its default features disabled. Its UI, state, event handlers and engine execute in Rust. The TSX entry activates the JS runtime in its development host.

```sh
cargo run --manifest-path examples/rust-app/Cargo.toml --target-dir target
./target/debug/nsh run examples/rust-app
```

The desktop backend's development libraries must be installed (see `docs/dev/linux.md`, `macos.md` and `windows.md`). The TSX entry builds a host with the app's Rust module; use the CLI built in this checkout, or set `NSH_SOURCE` to this checkout when using an installed CLI.

To create your own project with published dependencies:

```sh
nsh init my-app --rust
cd my-app
cargo run
```

Rust edits take effect on the next `cargo run`. For the TSX development session, `nsh run` rebuilds and restarts on Rust edits and hot-reloads TSX edits. The Rust module's TypeScript declarations are generated when `nsh build`, `nsh check` or `nsh run` builds the app's command line.

The example accepts scripted window checks through `NSH_SCRIPT`:

```sh
NSH_SCRIPT='wait 800; shot target/rust-app.png; quit' cargo run --manifest-path examples/rust-app/Cargo.toml --target-dir target
cargo test --manifest-path examples/rust-app/Cargo.toml --target-dir target
```

The engine test mounts the same Rust root, dispatches a button press and verifies that the displayed count changes.

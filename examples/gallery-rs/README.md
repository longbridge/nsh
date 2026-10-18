# Rust gallery

The gallery has a direct Rust entry and a TSX entry sharing `Gallery`. Everything it shows is written in Rust, in the
app's own crate `native/` (`nsh.json` `"rust": "native"`). It shows what Rust components can do, next to the TSX
gallery (`examples/gallery`). The crate's set is the `#[native_module("gallery/native")]` function in
`native/src/lib.rs`, which registers `Gallery` and the crate's `assets/`; `App.tsx` imports it by that specifier.

It has the TSX gallery's design (`ui.tsx`, `icons.tsx`, `theme.json`), following the system's appearance:
`native/src/ui/` is its Rust form (the palette for light and dark, finishes, icons, `Press`, `Appear`, `Toggle`,
`Segmented`, `Card`, `Section`, `Button`, ...), which the index and the demos are built with. As in the TSX gallery, a
wide window (700 pt on desktops, 900 pt elsewhere) shows a sidebar of the demos and the selected one beside it
(`native/src/shell.rs`), its cards in as many columns as fit (`DemoPage`, `native/src/ui/page.rs`).

```sh
cargo build -p nsh-cli
./target/debug/nsh run examples/gallery-rs --target ios   # or android, catalyst
```

`nsh` builds the app its own command line and hosts with the crate linked in (`docs/dev/tooling.md`, "Apps with Rust
components of their own"); editing the Rust code builds them again and restarts the app.

Run the gallery directly on the machine's native desktop backend (GTK 4 on Linux, AppKit on macOS, Win32 on Windows):

```sh
cargo run -p gallery-native
```

The executable mounts `Gallery` through `AppSource::Rust`, registers the gallery's assets and theme, and executes
UI state and event handlers in Rust. Its native dependencies disable default features, so this entry links the
Rust engine directly. The TSX entry uses the same component through the existing NSH development hosts.

Install the backend's development libraries before building (`docs/dev/linux.md`, `macos.md` and `windows.md`).
Rust changes take effect on the next `cargo run`; `nsh run` rebuilds and restarts its host on Rust changes.

```sh
NSH_SCRIPT='wait 800; shot target/rust-gallery.png; quit' cargo run -p gallery-native
cargo test -p gallery-native
```

The headless tests exercise the gallery's Rust root, demo navigation, state changes and desktop layouts. The
native entry is checked on Linux, macOS and Windows, including a dependency check for the Rust-only engine path.
Create an independent app with both entry points using `nsh init my-app --rust`, then `cd my-app` and `cargo run`.

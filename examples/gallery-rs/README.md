# Rust gallery

The app is one native component: `App.tsx` mounts `Gallery`, and everything it shows is written in Rust, in the
app's own crate `native/` (`nsh.json` `"rust": "native"`). It shows what Rust components can do, next to the TSX
gallery (`examples/gallery`). The crate's set is the `#[native_module("gallery/native")]` function in
`native/src/lib.rs`, which registers `Gallery` and the crate's `assets/`; `App.tsx` imports it by that specifier.

It has the TSX gallery's design (`ui.tsx`, `icons.tsx`, `theme.json`), following the system's appearance:
`native/src/ui/` is its Rust form (the palette for light and dark, finishes, icons, `Press`, `Appear`, `Toggle`,
`Segmented`, `Card`, `Section`, `Button`, ...), which the index and the demos are built with. As in the TSX gallery, a
wide window (700 pt on desktops, 900 pt elsewhere) shows a sidebar of the demos and the selected one beside it
(`native/src/shell.rs`).

```sh
cargo build -p nsh-cli
./target/debug/nsh run examples/gallery-rs --target ios   # or android, catalyst
```

`nsh` builds the app its own command line and hosts with the crate linked in (`docs/dev/tooling.md`, "Apps with Rust
components of their own"); editing the Rust code builds them again and restarts the app.

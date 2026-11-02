# Panel

An app with a host's second window: the main window (`PanelApp`) and a panel window (`PanelContent`) beside it. Both
are Rust components in `native/`; the panel runs its own app and engine and keeps its data apart
(`com.longbridge.nsh.panel.panel`).

```sh
cargo run -p panel-native
```

`nsh run examples/panel` shows only the main window. It mounts `nsh.json`'s `"root"` (`PanelApp`) on the engine
thread, and a component cannot open a window: a panel is created by the thread that runs the app's message loop, in
`run_app_with`'s setup hook (`native/src/main.rs`), which only a runner such as this binary has. The panel has a title
bar (the default is none), floats above the Dock or the taskbar (`WindowLevel::AboveDock`, which Linux leaves to the
window manager) and, on macOS, moves by its background.

`nsh.json` still names the crate and its root, so `nsh run` and the other targets mount `PanelApp`; only the panel
window needs the binary.

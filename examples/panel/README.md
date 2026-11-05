# Panel

An app with a host's second window: the main window (`PanelApp`) and a panel window (`PanelContent`) beside it. Both
are Rust components in `native/`; the panel runs its own app and engine and keeps its data apart
(`com.longbridge.nsh.panel.panel`).

```sh
cargo run -p panel-native
```

The panel is opened by the host, in `run_app_with`'s setup hook (`native/src/main.rs`): the hook runs on the thread
that runs the app's message loop and hands the host an `AppContext`, the only place a panel opens — a `Panel` is not
`Send`, so it is created there and kept there (dropping it closes its window). `AppContext::run` brings later work
back to the same thread, which is how a host opens a panel after startup, for a shortcut or a menu item.

`nsh run examples/panel` shows only the main window. It mounts `nsh.json`'s `"root"` (`PanelApp`) on the engine
thread, and a component cannot open a window: only a runner such as this binary calls `run_app_with` with a setup
hook. `nsh.json` still names the crate and its root, so `nsh run` and the other targets mount `PanelApp`.

The panel has a title bar (the default is none) and asks for `WindowLevel::AboveDock`, above the Dock or the taskbar;
Linux has no window levels, so the window manager places it like any other window.

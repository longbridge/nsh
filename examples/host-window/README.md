# Host window example

A host app with a second window of its own (`examples/host-window`): the main
window runs one app, and the host opens a second window — a panel-kind window
— with **an app of its own inside** (its own engine and its own host channel),
which is what `HostWindow` is for: windows the platform opens for the app
(`WindowRef`) share the app's engine; a host window keeps its isolation.

Run it with cargo:

```
cargo run -p host-window
```

`nsh run examples/host-window` shows only the main window: the dev host runs
`nsh.json`'s `root` (`App`) through the platform's `run_app`, and no nsh.json
key opens a host window — only the host's runner can, in `run_app_with`'s
setup hook on the thread that runs the app.

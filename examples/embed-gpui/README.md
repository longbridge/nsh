# embed-gpui

A GPUI Kit app embedding nsh apps (`nsh_platform_gpui::AppView`): a sidebar, an input, buttons and a dialog of GPUI
Kit around two surfaces in a resizable split, each running `app/` in its own engine. The host's theme and light or
dark mode reach the surfaces on every change (`src/theme.rs`), Tab moves through the host's widgets and the surfaces'
fields, GPUI Kit's dialogs and notifications draw above the surfaces, and closing a pane drops its view, which stops
its engine. "Violet theme" loads `themes/violet.json` (an nsh theme file) into GPUI Kit's theme registry.

```sh
cargo build -p nsh-cli
./target/debug/nsh build examples/embed-gpui/app -o examples/embed-gpui/build/app
cd examples/embed-gpui && CARGO_TARGET_DIR=../../target cargo run
```

`cargo run -- <bundle.nshc or its directory>` runs another bundle; `--alone` runs one surface in a window of its own
(no host widgets: Tab wraps around its fields and the switch). The example is its own Cargo workspace, so GPUI Kit
stays out of nsh's workspace builds; `CARGO_TARGET_DIR` shares what nsh's workspace built. It depends on `gpui-kit`
with feature `gpui-fast`, so GPUI Kit and nsh draw with the same GPUI (`cargo tree -i gpui-fast` shows one version).
See "GPUI and GPUI Kit apps" in `docs/dev/embedding.md`.

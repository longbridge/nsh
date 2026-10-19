# embed-gpui

A GPUI Kit app embedding NSH apps (`nsh_platform_gpui::AppView`): a sidebar, an input, buttons and a dialog of GPUI
Kit around two order tickets in a resizable split, each running `app/` in its own engine. The host's theme and light
or dark mode reach the surfaces on every change (`src/theme.rs`), Tab moves through the host's widgets and the
surfaces' fields, GPUI Kit's dialogs and notifications draw above the surfaces, and closing a pane drops its view,
which stops its engine. "Violet theme" loads `themes/violet.json` (an NSH theme file) into GPUI Kit's theme registry.

```sh
cargo build -p nsh-cli
./target/debug/nsh build examples/embed-gpui/app -o examples/embed-gpui/build/app
cargo run --manifest-path examples/embed-gpui/Cargo.toml --target-dir target
```

The app is built with the stock `nsh build`: everything the host provides, it connects by name when it runs the app.

- **The host's functions.** `src/trading.rs` is the host's paper-trading desk (sample prices; no order leaves the
  process), registered as the UI set `host/trading` with `set_ui_sets`, next to NSH's `nsh/ui/mobile`. The app
  declares its type and reaches it with `nativeModule<{ Trading: TradingApi }>("host/trading")`: `Trading.quote`,
  `Trading.select` and `Trading.placeOrder` run in Rust in the call (a wrong order throws, and the ticket shows the
  message), `Trading.subscribe` is `async`.
- **One desk, both ways.** The desk (the selected symbol, the orders, the positions) lives once, in the host, in a
  `tokio::sync::watch` channel. Each ticket subscribes: the function's task posts the desk to the ticket's callback
  now and after every change. The host's sidebar (the watchlist, the orders of all tickets) and toolbar (the
  positions) follow the same channel, and the sidebar's watchlist and the input (Enter) select a symbol on it. So an
  order placed in Ticket A shows in the host and in Ticket B, a symbol picked in the host shows in both tickets, and
  one picked in a ticket shows in the host and the other ticket.
- **GPUI widgets inside the app.** `host.watchlist` (`src/watchlist.rs`) and `host.switch` (`src/switch.rs`) are
  GPUI Kit widgets the host registers as native components and the app places with `nativeComponent`: the
  watchlist shows the desk's symbol and hands the one picked to `Trading.select`; the switch is the ticket's
  "Extended hours". Both take their own clicks and keys and are Tab stops among the app's fields.
- **Show layers** (on at the start, `src/layers.rs`) outlines each NSH surface and tags it "NSH · TSX app", and
  outlines and tags the GPUI widgets inside the apps "GPUI"; a legend under the tickets names the three layers. Off,
  the window is seamless.

`cargo run … -- <bundle.nshc or its directory>` runs another bundle; `--alone` runs one surface in a window of its
own (no host widgets: Tab wraps around its fields and the switch). The example is its own Cargo workspace, so GPUI
Kit stays out of NSH's workspace builds; `--target-dir target` shares what NSH's workspace built. It depends on
`gpui-kit` with feature `gpui-fast`, so GPUI Kit and NSH draw with the same GPUI (`cargo tree -i gpui-fast` shows
one version). See "GPUI and GPUI Kit apps" in `docs/dev/embedding.md`.

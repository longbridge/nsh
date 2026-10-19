# AGENTS.md

Native Shell (NSH for short in prose; crates, the CLI, the JS module and Java package all use `nsh`) is designed for
AI-generated, dynamically delivered native UI: native apps embed it (`NshAppView` on iOS, `NshView` on Android,
`AppView` on macOS, Windows and Linux) and load TSX modules at run time — signed, sandboxed by the host, rendered with
each platform's own UI. Embedding comes first; plugins for native apps and standalone apps use the same machinery.
Module code runs in QuickJS on a native runtime (the reactive graph, the renderer and every module apps import are
Rust); layout uses Taffy; embedding native components such as maps or web views has no layering problems. The
toolchain is pure Rust and does not need Node.js; app developers get a prebuilt distribution (`cargo xtask dist`) and
never need Rust either. Every platform has a native backend: iOS with UIKit views (`nsh-platform-ios`), Android with
Android views (`nsh-platform-android`), macOS with AppKit views (`nsh-platform-macos`), Windows in Win32 windows drawn
with Direct2D (`nsh-platform-windows`), Linux with GTK 4 widgets (`nsh-platform-linux`). The GPUI backend
(`nsh-platform-gpui`) draws on macOS, Windows and Linux and embeds in GPUI / GPUI Kit apps; the web platform
(`nsh-platform-web`) renders apps as HTML in a browser page, and `nsh package --target web` publishes them as static
sites. The only UI set is `nsh/ui/mobile` (it renders on every platform); a desktop set is planned.

This file is the short version. The details are in `docs/dev/` (read the page of the area you change first) and in
the module docs of the code (`//!`, Javadoc), which are the reference for how a module works inside.

## Layout

```
crates/
  nsh-core/            Platform-independent core: shadow tree, styles (RN semantics), Taffy layout (flex + grid), text merging, list virtualization,
                       mount items; CSS values (`css.rs`: colors, lengths, shadows, gradients, filters), effects (`effects.rs`), Tailwind (`tailwind/`);
                       the reactive graph both sides run on (`reactive.rs`, Solid's algorithm, generic over the host);
                       Rust components that run once (`ui/`: signals, elements, bindings, typed styles, the theme;
                       `core/components.rs`, `core/views.rs`), classes TS uses with `new` (`ui/class.rs`), JS's calls into
                       Rust and what comes back, typed (`ui/call.rs`: ops carrying closures, the event stream, posts); the runtime's Rust
                       modules (`runtime/`: `nsh/android`'s Toast, `nsh/image`, ...; their JS face is made from their specs),
                       the standard modules among them (`runtime/fs.rs`: `fs/promises`, `runtime/process.rs`, `runtime/net.rs`,
                       `runtime/websocket.rs`, `runtime/os.rs`) on the `System` their calls get (capabilities, data directory, quotas, per-VM state)
  nsh-macros/          `#[component]`, derives of Rust components (`Props`, `PropEnum`, `Event`, `JsObject`), `#[methods]`, `#[module]`, `#[class]`,
                       `#[native_module("…")]` (an app crate's UI set, which `rust_app.rs`'s generated code calls), `Styled`'s spacing methods;
                       `HostProps` (the host elements' props: decoding, defaults and TS), `JsOps` (recorded operations: canvas)
  nsh-ui-mobile/       The components and modules of `nsh/ui/mobile` (Button, SafeAreaView, StatusBar, Alert, ...), written in Rust; their JS
                       face is made from their specs (nsh-js `components.rs`), their TypeScript declarations generated
  nsh-animation/       Native animation graph (pure Rust): value / interpolate / math nodes, timing / spring / decay drivers
  nsh-js/              QuickJS host and NSH's runtime: console / timers, bytecode compilation and loading, limits (`sandbox.rs`);
                       the native modules apps import (`modules.rs`: module table and loader; docs/dev/js-modules.md), the
                       renderer of compiled JSX (`dom.rs`, `nsh:rt`), JS's reactive graph (`reactive.rs`: `nsh-core`'s
                       graph with JS values: signals, memos, effects, owners, context), Rust components' and modules' JS face (`components.rs`), navigation,
                       lists, canvas, animation (`navigation.rs`, `list.rs`, `canvas.rs`, `animated.rs`), touches and the
                       responder system (`touch.rs`), the environment (`env.rs`), hot module replacement (`hot.rs`),
                       Rust classes (`classes.rs`: on the web classes' machinery);
                       decoding JS values into core types (`protocol.rs`); the web APIs of TS apps (`web/`: fetch, streams,
                       Blob / FormData, URL, encoding, events), native classes; the standard modules' VM side (`standard/`:
                       the catalog and its loader, LLRT's `buffer` / `crypto` / `path` / `url` / `zlib`, `console`,
                       the globals `WebSocket`, `process` and `nextTick`); the TypeScript declarations of all of it
                       (`declarations.rs`, from the declaration next to each export and spec)
  nsh-engine/          Engine thread: drives nsh-js + nsh-core and hands one MountBatch per frame to the platform
  nsh-package/         Signed packages (`.nshpkg`): Ed25519 keys, release-key certificates, revocation lists, the trust store
  nsh-bundler/         Bundler: rolldown + Oxc (TS stripping) + NSH's JSX compiler (`jsx.rs`, docs/dev/native-pipeline.md) + source maps;
                       imports of the native modules stay imports (`plugin.rs`), so a bundle holds only the app's code;
                       the app's themes (nsh.json `tailwind` / `theme`, checked, handed to the renderer by the entry) and the
                       unknown-class check (`classes.rs`);
                       development bundles with hot module replacement (`hot.rs`); the bundler's part of `.nsh/types`
                       (`types.rs`: asset imports); feature `bundle` (default off in the workspace; the CLI turns it on) is
                       all of that, without it only `compile_module` (`compile.rs`: one module, no rolldown) is left, which
                       also builds for WebAssembly (the website's playground)
  nsh-dev/             Dev tooling: dev server protocol, server (CLI, feature `server`), client (host, feature `client`)
  nsh-platform-ios/    UIKit backend (iOS / simulator / Mac Catalyst): native view mounting, CoreText text engine, input, AppView, standalone app;
                       feature `dev`: dev server client, log forwarding, error overlay; feature `embed`: the C ABI (`embed.rs`,
                       `include/nsh.h`) under the `NativeShell` Swift module (`swift/NativeShell.swift`) iOS apps embed nsh with
  nsh-platform-android/ Android backend: `libnsh.so` (JNI glue, StaticLayout text engine, batch encoding, UI-thread animator) + the Java
                       runtime in `java/com/longbridge/nsh` (NshView, Mounter, views, images, networking); feature `dev` as above
  nsh-platform-web/    WebAssembly backend (published web apps, `nsh run --target web`, the website's live demos): the engine on
                       the page's main thread (`LocalEngine`), text laid out with browser font metrics, batches as JSON + CSS
                       for the DOM runtime in `web/nsh.js`, the animator on requestAnimationFrame, fetch over the browser's
                       (`net.rs`); feature `bytecode-only`: published apps' runtime (`nsh.wasm`, no QuickJS parser), default
                       features: the development host's (`nsh-dev.wasm`, source and hot updates); `web/index.html` is the demo
                       player (`cargo xtask web`); feature `compiler`: the playground's TSX compiler (`nsh-playground.wasm`)
                       (docs/dev/web.md)
  nsh-platform-linux/  GTK 4 backend (Linux): one `NshView` widget per node drawn with GSK, Pango text engine (typeset on the engine
                       thread, glyph runs drawn on the main thread), input, scroll views, navigation, video, native
                       components, AppView, standalone app; feature `dev`: dev server client, log forwarding, error overlay;
                       `--script` steps (screenshots, simulated input) for checking changes (docs/dev/linux.md)
  nsh-linux-host/      Development host on Linux (`nsh run --target linux`): connects to a dev server or runs a bundle
  nsh-linux-app/       Release runner on Linux: the executable of apps built by `nsh package --target linux`
  nsh-platform-macos/  AppKit backend (macOS): one flipped, layer-backed `NSView` per node decorated by Core Animation, CoreText text
                       (typeset on the engine thread), desktop input and scroll views as on Linux, navigation, video, web views,
                       native components, AppView, standalone app; feature `dev` as above; `--script` steps (docs/dev/macos.md)
  nsh-macos-host/      Development host on macOS (`nsh run --target macos`): connects to a dev server or runs a bundle
  nsh-macos-app/       Release runner on macOS: the executable of apps built by `nsh package --target macos`
  nsh-platform-windows/ Win32 backend (Windows): one retained `NshView` per node, the tree drawn with Direct2D into a swap chain,
                       DirectWrite text engine (typeset on the engine thread, glyph runs drawn on the main thread), input
                       (mouse, touch, pen, wheel, precision touchpads), scroll views, NSH's own text editor (IMM32 IME),
                       navigation, video (Media Foundation), web views (WebView2), native components, UI Automation,
                       AppView, standalone app; feature `dev` as above; `--script` steps (docs/dev/windows.md)
  nsh-windows-host/    Development host on Windows (`nsh run --target windows`): connects to a dev server or runs a bundle
  nsh-windows-app/     Release runner on Windows: the executable of apps built by `nsh package --target windows`
  gpui/                The GPUI backend: a workspace of its own (`crates/gpui/Cargo.toml`, its own `Cargo.lock` and GPUI's dependencies),
                       excluded from the main one, so the main workspace's builds, tests and CI never build GPUI:
  gpui/nsh-platform-gpui/ GPUI backend (macOS, Windows, Linux): the Windows backend's retained `NshView` tree, input,
                       scrolling and editing logic, painted by one GPUI element per surface (`render.rs`: `SurfaceElement`,
                       `DrawCx` with the transform and opacity GPUI lacks), text shaped by GPUI's text system on the engine
                       thread, the main-thread queue (`rt.rs`), GPUI events (`events.rs`, keys as Windows VK codes);
                       `AppView` is a GPUI view for GPUI / GPUI Kit apps; features `webview` (wry), `video` (GStreamer),
                       `dev` as above, `shot` (`--script` shots on macOS / Windows: GPUI's test support, never in the runner)
                       (docs/dev/gpui.md)
  gpui/nsh-gpui-host/  Development host on GPUI (`nsh run --target gpui`): connects to a dev server or runs a bundle
  gpui/nsh-gpui-app/   Release runner on GPUI: the executable of apps built by `nsh package --target gpui`
  nsh-ios-host/        Development host app (Catalyst / iOS simulator); connects to a dev server or runs a local bundle
  nsh-ios-app/         Release runner: the executable of iOS apps built by `nsh package`
  nsh-cli/             The `nsh` command line (a library: `main_with(ui_sets)`): init / build / trace / check / dev / run / package; keygen /
                       certify / publish / verify / revoke; apps with Rust components of their own (`rust_app.rs`: each crate
                       nsh.json `"rust"` names, one or a list, has a `#[native_module("…")]` fn; its hosts take nsh's crates where the app's crates do:
                       crates.io at the CLI's version, or the checkout they depend on by path; docs/dev/tooling.md); web packages and the
                       browser host's pages (`web.rs`, `web/`); what it prints (`ui.rs`:
                       one line per step, tools' output only on failure, `-v` for everything; docs/dev/tooling.md "Output")
  xtask/               Maintainer tasks: `cargo xtask android` (Android runtime), `ios-embed` (the iOS SDK), `dist` (the distribution),
                       `web` (the WebAssembly runtime and the website's demos)
docs/                 Documentation; `docs/dev/`: for developers of nsh (architecture, internals, decisions, pitfalls)
website/              The documentation site (Astro; guides, element / component / API reference in English and zh-CN) and its
                      runnable demos (`website/demos/*.tsx`, live on the site through the WebAssembly runtime); see its README
examples/             Example apps (gallery: a demo index, one module per demo; gallery-rs: the same in Rust, the app's crate in
                      `native/`; rust-exports: a TSX app using one of each export of its crate, the reference for exporting
                      components, modules and classes, with headless tests of the TSX entry; counter), embedding samples
                      (embed-ios, embed-android; embed-gpui: a GPUI Kit app, its own workspace; `run/` the root's
                      `cargo run -p example-embed-gpui`) and the app they run (embedded: uses a native component the samples
                      register); bench: component benchmarks
```

Dependency direction: `nsh-core` ← `nsh-js` ← `nsh-engine` (also ← `nsh-package`) ← `nsh-platform-*` ←
`nsh-ios-host` / `nsh-ios-app` / `nsh-linux-host` / `nsh-linux-app` / `nsh-macos-host` / `nsh-macos-app` / `nsh-windows-host` / `nsh-windows-app` / `nsh-gpui-host` / `nsh-gpui-app`; `nsh-cli` depends on `nsh-bundler`, `nsh-js` and `nsh-engine`; `nsh-ui-mobile` depends
on `nsh-core` only and is used by the bundler (declarations), the platforms and the CLI (registry). `nsh-core` must not depend
on any platform or JS crate.

## Documentation

| Changing | Read |
|---|---|
| threads, the JS → native data flow, text pipeline | `docs/dev/architecture.md` |
| how apps are compiled and run: the JSX compiler, the renderer | `docs/dev/native-pipeline.md` |
| the native modules, where a new API goes, platform props | `docs/dev/js-modules.md` |
| elements, touch, accessibility, the environment | `docs/dev/elements.md`, `text.md` |
| styles, Tailwind, effects | `docs/dev/styles.md` |
| scroll views, zooming, `List` | `docs/dev/scrolling-and-lists.md` |
| animation, canvas, images | `docs/dev/animation.md`, `canvas.md`, `images.md` |
| VM limits, capabilities, signed packages | `docs/dev/sandbox.md`, `signed-packages.md` |
| packaging, the iOS / Android SDKs, native components | `docs/dev/embedding.md` |
| `.nsh/types`, the dev server, hot reload | `docs/dev/tooling.md` |
| the web runtime (`nsh-platform-web`), the website's demos | `docs/dev/web.md`, `website/README.md` |
| the Linux backend (`nsh-platform-linux`, GTK 4) | `docs/dev/linux.md` |
| the macOS backend (`nsh-platform-macos`, AppKit) | `docs/dev/macos.md` |
| the Windows backend (`nsh-platform-windows`, Win32 / Direct2D) | `docs/dev/windows.md` |
| the GPUI backend (`nsh-platform-gpui`, macOS / Windows / Linux) | `docs/dev/gpui.md` |
| a hot path | `docs/dev/performance.md` (baselines) |
| anything platform-specific | `docs/dev/pitfalls.md` (verified constraints, by area) |
| components (`nsh/ui/mobile`) | `docs/dev/components.md`, `crates/nsh-ui-mobile/README.md` |
| Rust classes TS uses (`#[class]`), module functions' kinds, callbacks, how calls and Rust components' props cross (typed) | `docs/dev/rust-classes.md`, `docs/dev/components.md` |

Keep them current: a change that alters documented behavior updates its page (or the module doc) in the same commit,
including the user-facing pages in `website/` (English and `zh-CN`). User-facing pages and the README are written in
positive terms: what a platform or feature is and does, never what it is not or lacks, and name the product Native
Shell in formal places and NSH elsewhere (`nsh` only in code) ("Writing documentation" in `website/README.md`).

## Commands

```sh
cargo run [-- <app>] [-- --target catalyst|ios|android|gpui]                       # the gallery on this machine's backend (AppKit / GTK / Win32), hot reload; rebuilt and restarted on Rust changes
cargo xtask [<app>] [--target catalyst|ios|android|gpui] [-v]                      # the same without cargo's own two lines; -v (any nsh / xtask command): all output
cargo build -p nsh-cli                                                             # build the nsh CLI
./target/debug/nsh init my-app --template ts|rust|mixed                            # new app: TSX, Rust (a Cargo project, "root"), or TSX with a crate in native/; asks on a terminal without --template
cargo build -p nsh-ios-host --target aarch64-apple-ios-macabi                          # the Catalyst development host (nsh run builds it in a checkout)
cargo build -p nsh-ios-host --target aarch64-apple-ios-sim                             # the iOS simulator host (`nsh run --target ios`)
cargo build -p nsh-ios-app --target aarch64-apple-ios-sim                              # the release runner (`nsh package --target ios-simulator`)
cargo build -p nsh-linux-host                                                          # the Linux (GTK) development host (`nsh run` on Linux builds it in a checkout)
cargo build -p nsh-macos-host                                                          # the macOS (AppKit) development host (`nsh run --target macos` builds it in a checkout)
cargo build -p nsh-windows-host                                                        # the Windows development host (`nsh run` on Windows builds it in a checkout)
cargo build --manifest-path crates/gpui/Cargo.toml --target-dir target -p nsh-gpui-host                    # the GPUI development host (`nsh run --target gpui` builds it in a checkout)
cargo run -p example-embed-gpui                                                        # the GPUI Kit embedding example (a std-only runner that only runs `cargo run` in its own workspace)
./target/debug/nsh trace examples/counter                                  # headless run: prints mount items and the time of every commit
./target/release/nsh trace examples/gallery/Bench.tsx --scroll 3000 --taps 0       # headless scrolling (10pt per frame): per-frame list cost
./target/debug/nsh trace examples/hopper --tap label:Plan --wait 500 --safe-area 47,0,34,0   # tap by testID / label / text, run timers, insets
./target/debug/nsh check examples/hopper                                   # type check (TypeScript 7's native tsc, downloaded once; no Node)
./target/debug/nsh run examples/counter --trace                            # dev server + Catalyst host (390×844), hot reload on save
./target/debug/nsh run examples/gallery --target ios                       # in an iOS simulator (boots one; needs the nsh-ios-host sim build)
./target/debug/nsh run examples/gallery --target android                   # on an emulator / device (needs `cargo xtask android --dev`)
./target/debug/nsh run examples/gallery --target linux                     # in a GTK window (the default on Linux)
./target/debug/nsh-linux-host --script "wait 800; shot a.png; quit"        # Linux host, scripted: screenshots and simulated input
./target/debug/nsh run examples/gallery --target windows                   # in a Win32 window, 1200×800 (the default on Windows)
./target/debug/nsh-windows-host --bundle build/bundle.nshc --script "wait 800; shot a.png; quit"   # Windows host, scripted (connects to `nsh dev` without --bundle)
./target/debug/nsh run examples/gallery --target gpui                      # in a GPUI window (macOS, Windows, Linux)
./target/debug/nsh package examples/gallery --target gpui                  # packaged as this OS's apps are, with the GPUI runner
./target/debug/nsh run examples/gallery --target web                       # in the browser (the dev server serves the page)
./target/debug/nsh package examples/gallery --target web --run             # a static site (files named by content), served on this machine
./target/debug/nsh package examples/gallery --target ios-simulator --run   # installable app (.app / .ipa / .apk)
cargo xtask android [--release] [--dev]                                            # Android runtime into target/android/<profile>[-dev]
cargo xtask dist                                                                   # the distribution in dist/nsh
./target/debug/nsh dev examples/counter                                    # dev server only (for simulators / devices); prints local and LAN URLs
./target/debug/nsh build examples/counter --release                        # writes bundle.js / bundle.nshc
./target/debug/nsh keygen keys/root && ./target/debug/nsh keygen keys/release      # signing keys (.secret / .pub)
./target/debug/nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
./target/debug/nsh publish examples/counter --key keys/release.secret --cert keys/release.cert --build 2
./target/debug/nsh verify out/com.example.counter-2.nshpkg --root keys/root.pub     # as a device would check it
./target/debug/nsh revoke --root keys/root.secret --sequence 1 --package <sha256> -o revocations.json

cargo test --workspace
cargo test --release -p nsh-engine --test engine bench_gallery_scroll -- --ignored --nocapture   # gallery scroll benchmark (JS + core)
cargo test --release -p nsh-core bench_ -- --ignored --nocapture                # row / cell relayout benchmarks
cargo +nightly fmt --all                                                        # .rustfmt.toml uses nightly options
cargo clippy --workspace --all-targets
cargo clippy -p nsh-platform-ios -p nsh-ios-host -p nsh-ios-app --target aarch64-apple-ios-macabi --features nsh-platform-ios/embed,nsh-platform-ios/dev
cargo xtask ios-embed                                                           # NativeShell(Dev).xcframework into dist/nsh/embed/ios
cargo xtask web                                                                 # nsh.wasm + player + website demos into website/public/play
cargo clippy -p nsh-platform-web --target wasm32-unknown-unknown --features compiler   # the web backend (empty off WebAssembly)
cargo clippy -p nsh-platform-linux -p nsh-linux-host -p nsh-linux-app --all-targets      # the Linux backend (on Linux; empty elsewhere)
cargo clippy -p nsh-platform-macos -p nsh-macos-host -p nsh-macos-app --all-targets --features nsh-platform-macos/dev   # the macOS backend (on macOS)
cargo clippy -p nsh-platform-windows -p nsh-windows-host -p nsh-windows-app --all-targets   # the Windows backend (on Windows; empty elsewhere)
cargo test -p nsh-platform-windows -p nsh-windows-host -p nsh-windows-app               # its tests (on Windows; they open no window)
(cd crates/gpui && cargo clippy --all-targets --features nsh-platform-gpui/dev)        # the GPUI backend, its own workspace (empty off the desktops)
(cd crates/gpui && cargo test && cargo +nightly fmt --all)                              # its tests (they open no window) and formatting
cd website && bun run dev                                                       # the documentation site (builds the player too)
```

`nsh-platform-ios` only compiles for `target_os = "ios"` (iOS devices, the iOS simulator and Catalyst); on a macOS
host it is an empty crate, so always run clippy on platform code with the Catalyst target. Likewise
`nsh-platform-android` is empty off Android: check it with `cargo xtask android` (which sets up the NDK linker, `CC`,
`AR` and bindgen's sysroot) and `cargo xtask clippy-android`. `nsh-platform-linux` and its hosts are empty off Linux:
check them on Linux, with the GTK 4 (≥ 4.14) development packages installed. `nsh-platform-macos` and its hosts are
empty off macOS (`target_os = "macos"`, not Catalyst). `nsh-platform-windows` and its hosts are empty off Windows:
check them on Windows, with the MSVC Build Tools, the Windows SDK and LLVM (libclang for QuickJS' bindings);
`RUST_MIN_STACK=268435456` if rustc overflows its stack on the `windows` crate. The GPUI backend (`crates/gpui`) is
a workspace of its own: the main workspace's `cargo test --workspace` and clippy never build it, and its CI runs only when
it changes (and daily). Its crates build on all three desktops and are empty elsewhere; on Linux they need GPUI's X11 /
Wayland / xkbcommon / fontconfig packages plus GTK 3, WebKitGTK 4.1 (feature `webview`, on by default) and GStreamer
1.20+ with its base plug-ins (feature `video`, the host's and the runner's) (`docs/dev/gpui.md`). It builds into the
repository's `target/`; dependencies only it uses are declared in `crates/gpui/Cargo.toml`, not the root. `.cargo/config.toml` sets
`IPHONEOS_DEPLOYMENT_TARGET = 15.0` for Rust and C alike.

## Rules

- **Two module layers, and the standard catalog.** `nsh` / `nsh/<module>` (native, `crates/nsh-js`) are the primitives and the environment
  every UI set builds on; `nsh/ui/<set>` is a whole UI for one form factor (`nsh/ui/mobile`; a desktop set will be
  separate, never an abstraction over both). Shared vs one form factor decides the layer, not JS vs native. Platform-only APIs go to
  `nsh/ios` / `nsh/android`, platform-only props into `ios={{…}}` / `android={{…}}` / `desktop={{…}}`. `nsh` has no
  catch-all re-exports; moved APIs get no compatibility re-exports. Beside them sits a fixed catalog of eleven
  standard modules with bare specifiers (`nsh_core::js::STANDARD_MODULES`: `buffer`, `console`, `crypto`,
  `fs/promises`, `net`, `os`, `path`, `process`, `url`, `websocket`, `zlib`): Node-shaped subsets compatible with
  gpui-shell's, built on the same native machinery (typed registries, capability checks in Rust) with generated
  declarations. Exactly these names import; `node:` prefixes and other Node modules (`fs`, `events`, `util`, ...)
  are refused. Files and processes are these modules (`fs/promises`, `process`), never an `nsh/<module>`; the
  catalog grows only by a design decision.
- **Primitives only.** The runtime adds capabilities, not widgets: components are drawn with the primitives in a UI
  set and look the same on every platform; no platform-styled widgets. React Native is a reference for which
  capabilities exist, not an API to be compatible with.
- **Components are Rust.** A UI set's components live in its crate (`crates/nsh-ui-mobile`), run in the core and
  render primitives with typed styles (GPUI-style `Styled` methods, never class strings) and theme colors (gpui-kit's
  fields); props and events are derived types, their JS face is made from the specs (`nsh-js` `components.rs`) and
  their TypeScript declarations generated from them (`nsh_core::ui::declarations`), never hand-written per component. JS allocates even node ids,
  the core odd ones for what components expand (`docs/dev/components.md`).
- **The main thread never waits for JS.** Callbacks the system wants answered on the spot (may this text change, may
  the back button pop, may the sheet close, ...) are answered natively from props mounted beforehand; events tell JS
  afterwards (`docs/dev/elements.md`).
- **One surface / `NshView` is one engine**, with no global singletons (NSH is embedded with several instances).
- **Limits and checks are enforced in Rust, never in JS**: a bundle holds only the app's code, so nothing in JS is
  trusted. QuickJS bytecode is trusted input (`Bundle::bytecode` is `unsafe`): it only loads from the app's own
  resources, the dev server and verified packages (`docs/dev/signed-packages.md`).
- **Generated, never hand-copied**: the codes of encoded operations (`ANIMATION_*`, `CV_*`), the canvas's recording
  methods and properties (made natively from `CanvasOp`'s `#[derive(JsOps)]` specs, which also decode them), the style
  types, the host elements' props and events and the easings / layout animation presets come from
  `crates/nsh-core/src/js/`, and so do their declarations, as do those of the UI sets' components and the runtime's
  Rust modules. No `.d.ts` is checked in: a native module's export carries its declaration (`modules::Ts`), a web
  class's spec its members' (`web::ClassTs`), and tests load every module and global and check that what exists is
  what is declared (`nsh-js` `declarations.rs`). A new style property is one line of `style_keys!` (`style.rs`) with
  its TS type, a new host prop one variant of `Prop` with `#[prop(js = …)]`, a new event a row of `host::EVENTS` (or
  `INTERNAL_EVENTS`) (`docs/dev/elements.md`).
- **Keep in sync**: the handler types of `host::EVENTS` ↔ the payloads of `nsh-js`'s `dispatch_event`; the order of `EventKind` ↔ `Native.java`; `encode.rs`
  opcodes ↔ `Mounter.java`; new event payload keys ↔ the JNI event map's key list; `BUNDLE_NAME` /
  `BYTECODE_VERSION` ↔ the linked QuickJS.

## Code conventions

- Formatting: `cargo +nightly fmt --all`, configured in `.rustfmt.toml`.
- TSX tags are PascalCase, everywhere (examples, docs, tests): every tag names a binding in scope, only `<If>` and
  `<Each>` are globals that need no import. The primitives have one home each: `View`, `Text`, `Scroll`, `Input` from
  `nsh`, `Image` from `nsh/image`, `List` from `nsh/list`, `Canvas` from `nsh/canvas`, `WebView` from `nsh/webview`,
  `Video` from `nsh/video`, `Modal` from `nsh/ui/mobile`, `InputAccessoryView` from `nsh/ios`; the compiler makes
  their tags in place (`jsx::PRIMITIVES`). Lowercase tags are compile errors: TypeScript checks a lowercase tag
  against one global type per name, so generic components (`List`, `Each`) and local ones would lose their types.
- Workspace lints: `missing_docs = warn`, `unreachable_pub = deny`, `private_interfaces = deny`. New crates must set `[lints] workspace = true`.
  They cover NSH's own Rust crates only: not the Java / Swift / TS sources, and never the apps built with NSH.
  - Every public API has a doc comment.
  - Items in private modules are `pub(crate)`, never `pub`.
- Everything is written in English: code comments, docs, logs and this file. Comments explain why, not what the code says.
- Library crates only log through the `log` facade (no `eprintln!`); performance logs use target `nsh::perf`, JS logs target `js`. Binaries install the logger.
  The CLI prints through `nsh-cli`'s `ui.rs` (steps, captured tools, `error` reports), xtask through `term.rs`.
- Dependencies are declared once in the root `Cargo.toml` `[workspace.dependencies]` (the GPUI backend's in `crates/gpui/Cargo.toml`); crates only write `.workspace = true`.
- Every `unsafe` block has a `// SAFETY:` comment.
- A crate's integration tests are one test binary (`tests/<name>/main.rs` with a module per area, as
  `nsh-engine/tests/engine/`), never a new `tests/*.rs`: each binary links its own copy of the bundler, QuickJS and
  the core (about 1 GiB of debug objects apiece).

## Public Rust structs

- New `pub struct` types use `#[non_exhaustive]` and implement `Default`, deriving it when possible.
- Set only the fields needed and leave the rest at their defaults. Within the defining crate, use `..Default::default()`; callers in other crates start with `Type::default()` and assign fields, because non-exhaustive structs cannot be constructed with external struct literals.
- New fields have sensible defaults so extending a struct does not break downstream construction.

## Commits and pull requests

Details and examples: "Commits and pull requests" in `CONTRIBUTING.md`.

- **All changes go through a pull request. Never push commits directly to `main`.** This includes code,
  documentation, release / version updates and CI / workflow fixes. Create a topic branch, push it and open a PR
  targeting `main`; merge through the PR after required checks pass. Do not use administrator bypass to skip this
  workflow. A request to release or push changes does not authorize a direct push to `main`.

- Subjects and PR titles are `<area>: <What the change does>`: the area in lower case (a crate without `nsh-`:
  `core`, `js`, `engine`, `bundler`, `cli`, `ui-mobile`, ...; one platform: `ios`, `android`, `macos`, `windows`,
  `linux`; `desktop` for macOS, Windows and Linux together, `mobile` for iOS and Android together; `runtime`, a module;
  `tailwind`, `text`, `docs`, `examples`, `gallery`). One area only, never several separated by commas: for a change
  that spans parts, judge it as a whole and name the part it is mostly about (a feature the core adds and the platforms
  carry out is `core`); then an imperative sentence starting with a capital letter, no trailing period, about 72 characters in all. Say what it does for NSH's users, not which
  files changed. Never conventional prefixes (`fix:`, `feat:`); check `git log` and match it.
- The body explains why (wrapped at 72 columns); hot path changes include their before / after `nsh trace` numbers.
- One commit / PR does one thing; no unrelated refactors, cleanup or formatting.
- A PR that adds, changes or removes public API (Rust items, JS exports, elements, props, style keys, Tailwind
  utilities, `nsh.json` keys, CLI flags, SDK APIs) lists every item under `## Public API` with its signature and one
  line on its purpose; changes to existing items also go under `## Breaking Changes` with `diff` blocks
  (`.github/pull_request_template.md`).
- A PR review presents, in order: the PR's purpose (the problem and the scope of the changes), whether the requirement
  and approach are reasonable, then concrete findings, the validation performed and its limits.

## Performance rules (the project's first priority)

1. **QuickJS is the slowest link: JS does as little as possible.** Only the app's code is JS: the reactive graph, the renderer and
   the modules are native, and compiled JSX calls them directly. Parsing, diffing and string processing all happen in Rust. No JSON
   serialization on the JS side.
2. **Cross threads in batches.** One JS turn → its ops queued natively (`Shared.ops`) → one `Core::commit` → one `MountBatch` → one
   main-thread hop. Never cross FFI or threads per item.
3. **No needless allocation on hot paths.** Node ids are dense: index `Vec`s (`slots`), not hash maps. Internal links use slab keys. Reuse
   buffers (`take_ops`, `queue`).
4. **Do not send mount items that are not needed.** New views get no default style and no empty event mask; unchanged styles and frames are not
   sent. Check item counts with `nsh trace`.
5. **Text**: measurements are cached per constraint (`TextContext.cache`); platforms cache font objects (`FontCache`).
6. **Changes to hot paths come with before / after `nsh trace` timings and item counts**, compared with the baselines
   in `docs/dev/performance.md` (update them when they move).

## Verification after changes

1. `cargo test --workspace`, plus fmt and clippy as listed above (including the Catalyst target, the macOS crates, `cargo xtask clippy-android`, on Linux the Linux crates, on Windows the Windows crates; for changes in `crates/gpui` or to what it uses, its own tests and clippy there).
2. Changes to the runtime (`nsh-js`), engine or core: run `nsh trace` and check item counts and timings.
3. Changes to the platform layer: launch with `nsh run`, check screenshots and simulate input where needed (the terminal needs the Screen
   Recording and Accessibility permissions; while the screen is locked the frontmost app is `loginwindow` and clicks fail, but window
   screenshots still work). iOS simulator: `xcrun simctl io booted screenshot`, `xcrun simctl ui booted appearance dark`.
   Android: `adb exec-out screencap -p`, `adb shell input tap|swipe|text|keyevent` (all work with the screen locked).
   Linux: `nsh-linux-host --script "wait 800; shot a.png; tap 100 200; …; quit"` renders screenshots offscreen and
   simulates input (taps, drags, wheels, typing, keys, dark / light), without touching the screen (`docs/dev/linux.md`).
   Windows: `nsh-windows-host --script "…"` does the same (also `touch` drags, precision touchpad `pad` scrolls,
   `resize`); web views are child windows and missing from its screenshots (`docs/dev/windows.md`).
   GPUI on Linux: `nsh-gpui-host --bundle … --script "…"` (the same steps) on a rootful Xwayland display of its own
   started with `-shm` (`DISPLAY=:N WAYLAND_DISPLAY=`, `WEBKIT_DISABLE_DMABUF_RENDERER=1` for web views); shots are read
   from the X server (`docs/dev/gpui.md`).

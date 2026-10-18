# Tooling: TypeScript, the dev server, runtimes

## TypeScript

- Every command that builds an app writes `.nsh/types` next to its entry (only changed files; files NSH no longer writes
  are removed). None of it is checked in, and nothing in it depends on the app's code: `nsh_js::type_definitions(ui_sets)`
  makes the declarations of everything NSH serves (`nsh-js` `declarations.rs`), and `nsh_bundler::type_definitions()`
  the asset imports. The files: `nsh/index.d.ts` (`nsh`), one per module (`nsh/list.d.ts`, `nsh/ui/mobile.d.ts`,
  ...), `nsh/types.d.ts` (the styles, the elements' props and events, `YNode`, the JSX namespace; `nsh` exports them),
  `nsh/jsx-runtime.d.ts` (where `jsxImportSource` finds `JSX`: no intrinsic elements, so every tag is checked as the
  component it names and generic ones are inferred: `native-pipeline.md`), `nsh/globals.d.ts` (console, timers,
  frames, the web APIs and the control flow components `If` / `Each`), and from the bundler `nsh/assets.d.ts` (what
  image and font imports give). Each file declares its own names; the `import type` lines between them are worked out
  from the names each uses. `nsh init` writes a `tsconfig.json` mapping `nsh` and `nsh/*` there (`jsxImportSource:
  "nsh"`, no DOM library), so editors type-check apps without `node_modules`.
- Where each declaration comes from:
  - a native module's export: the `Ts` next to it in its table (`modules::Item`; the module's own interfaces in its
    `types`);
  - a registry's components and modules: their specs (`nsh_core::ui::declarations`; a module object's type is
    `<Name>Module`, which a native module can build on: `BackHandlerModule & BackListeners`);
  - the styles, host props, events, easings and canvas methods: the core's tables (`nsh_core::js::{style, host, animation,
    canvas}::declarations`; an event's handler type is its row in `host::EVENTS`, an event no element declares is in
    `INTERNAL_EVENTS`);
  - a web class: its spec's `ClassTs` (members as in a class body; the constants come from the spec's table), the
    types its file declares in `TYPES`, a global function's `web::Function`, the other globals `host::GLOBALS`.
- The text of declarations (documentation, TypeScript) is only in builds that can write them: the WebAssembly runtime
  (`nsh_core::ui::DECLARATIONS` is false there) leaves it out, as pages download it. The tables keep their shape and
  wrap the text: `Ts::function` / `value` / `class`, `ClassTs { … }.declared()`, `decl!("…")` (a constant's text is
  then not in the binary at all), and generated code calls `ts_of::<T>()` instead of `T::ts_type`. New tables do
  the same; nothing the runtime needs (names, lengths, whether a method returns) goes through them.
- The tests keep them true: every module is loaded and its exports compared with its declarations
  (`declarations_match_the_modules`), every web class's members with its spec (`web_classes_declare_their_members`),
  the globals with a host's (`globals_match_the_host`: what is declared exists, and every global the host adds to
  QuickJS-NG's is declared), every event has a type or is internal, and every imported name has one home.
- To check the declarations themselves, run `nsh check -- --skipLibCheck false` in an app.

### `nsh check`

The bundler strips types without checking them (Oxc), and no Rust type checker for TypeScript exists: checking means
running TypeScript itself. The options were:

- **Bundle the JS compiler** (`typescript.js`, ~9 MB) and run it in QuickJS: QuickJS is 10–50× slower than V8 at this
  workload (tens of seconds for a small app) and the compiler needs a host for files and `lib.*.d.ts`; no.
- **Require Node** (`npx tsc`, `bunx tsc`): what apps did before; the only Node dependency of the toolchain.
- **Run TypeScript's native compiler**: TypeScript 7 (July 2026) is the Go port, shipped on npm as one executable per
  platform (`@typescript/typescript-<os>-<arch>`, `package/lib/tsc` with its `lib.*.d.ts` next to it) behind the
  `typescript` package's optional dependencies. It needs no Node and checks the same as TypeScript 6.

`nsh check` does the last (`crates/nsh-cli/src/check.rs`): it writes `.nsh/types` as every command does, then runs
`tsc -p <app dir> --noEmit` (arguments after `--` are passed on). The compiler is a pinned release (`TYPESCRIPT`),
downloaded from the npm registry's tarball URL the first time (ureq, no npm client), checked against the
`dist.integrity` (SHA-512) recorded in the source for each host it supports (macOS, Linux and Windows, arm64 and x64; on Windows the compiler is
`package/lib/tsc.exe`), unpacked into NSH's cache (`NSH_CACHE`, else `~/Library/Caches/nsh` / `$XDG_CACHE_HOME/nsh` /
`%LOCALAPPDATA%\nsh`) and kept there. `--tsc <path>` /
`NSH_TSC` run another `tsc` (a TypeScript 5 / 6 one, an offline copy). Updating TypeScript is a change of the version
and the integrities (`npm view @typescript/typescript-<os>-<arch>@<version> dist.integrity`). The distribution
(`cargo xtask dist`) does not ship the compiler: it is 24 MB per platform and most runs of `nsh` never check types.

## Output

The CLI's terminal output is `crates/nsh-cli/src/ui.rs` (`website/docs/cli.md`, "Output", shows it): one line per
step, the details only when something fails. Every command prints through it:

- `✓` a step that finished (`ui::done`, or `ui::Step`, which shows a spinner while the step runs, on a terminal only,
  and adds its time), `→` where to find something, `•` what a device did, `warn` / `error` (wrapped under their first
  word on a terminal), plain lines for the app's own output. Paths are relative to the current directory when inside
  it. Colors on terminals unless `NO_COLOR` is set; redirected, only the result lines, no ANSI. All of it is stderr:
  stdout is the commands' results (`nsh trace`'s mount items, the signing commands' lines).
- Tools run captured: `ui::cargo` (every cargo build: the app's crate, the hosts, `cargo xtask android`) shows the
  crate being compiled in its spinner, counts cargo's warnings into the `✓` line and, when the build fails, returns a
  `ui::failure` whose details are cargo's whole output; `ui::report` prints `error <message>` and those details (the
  commands' errors all end there, `main_with`). `ui::quiet` keeps the desktop hosts' own output (AppKit's notes,
  GTK's) and shows it if the host fails. simctl, adb, codesign and the SDK's tools were captured already.
- Log lines (`logger.rs`): the library crates and the devices log through `log`; normally warnings and errors are shown
  without targets (a device's after its name: `warn iOS Simulator: …`), `js` lines and a device's lines from other
  crates (the app's Rust code) plainly (with the device's name when several are connected), `nsh::perf` as `perf …`;
  NSH's own info lines (a device's "loading build 3") are not shown: the CLI's steps say what happens. The dev server
  reports hosts connecting and leaving (`nsh_dev::ClientEvent`) rather than logging it, so the CLI prints
  `• Mac Catalyst connected, build 1`, and a host that cannot run the app as one `warn` line
  (`warn Android Emulator: its host has no gallery/native (it was built for another app): nsh run examples/gallery-rs
  --target android installs this app's host`; "Dev server" below).
- Verbose (`-v` / `--verbose` on any command, or `NSH_VERBOSE=1`; the app's command line inherits both): no spinner,
  and everything as it comes: every log line with its level and target, cargo's and the hosts' output.

`xtask` has the same style in `crates/xtask/src/term.rs` (it cannot depend on `nsh-cli`, which would build the bundler
before every `cargo run`): `✓` lines for its builds, cargo's and the tools' output only when they fail, `-v` /
`NSH_VERBOSE` to see it all (`cargo xtask clippy-android` streams clippy's output: it is what the task is for).

## Dev server (nsh dev / nsh run)

- The CLI watches sources with notify and pushes every rebuild over WebSocket (`/ws`, default port 8790, the next free port if taken) to all
  connected hosts; new clients subscribe before receiving the current state, so no event is missed. Protocol: `crates/nsh-dev/src/protocol.rs`
  (JSON text frames + binary frames).
- Bytecode version negotiation: the client reports `BYTECODE_VERSION` in its handshake; on a mismatch the server sends JS source instead
  (an empty version, the web host's, asks for source without a warning).
- The web host is a page the dev server serves itself (`DevServer::serve_web`, the fallback route: `/`, `/nsh.js`,
  `/nsh.wasm`) when a web runtime exists; browsers connect to `/ws` on the same port. `nsh run --target web` builds the
  runtime in a checkout and opens the page (`docs/dev/web.md`, "The CLI's web target").
- Watching ignores `Access` events: inotify (Linux) reports reads, FSEvents does not, so the build's own reads of the
  sources would start the next build, forever.
- `nsh run` defaults to the host that runs on this machine: `--target catalyst` on macOS, `--target linux` (the GTK
  host, `crates/nsh-linux-host`, run in place: it needs no app bundle) on Linux, `--target windows` (the Win32 host,
  `crates/nsh-windows-host`, run in place too) on Windows. `--target macos` runs the AppKit host
  (`crates/nsh-macos-host`) in place too: it makes itself a regular app without a bundle. `--target gpui` runs the
  GPUI host (`crates/gpui/nsh-gpui-host`) in place on any of the three; GPUI hosts report `HostPlatform::Gpui`, so the dev
  server shows them `nsh run --target gpui`. The hosts are stopped with the run
  (Ctrl-C, SIGTERM, the session ending); a host that exits with an error ends it with its kept output. On Windows the host starts in a
  process group of its own (`CREATE_NEW_PROCESS_GROUP`), out of the console's Ctrl-C, so a stop is not taken for the
  host failing, and in a job object that kills it when the job closes (`run::KillOnClose`): Windows ends a child with
  its parent only in a job, so the host goes with `nsh` however it ends (a closed console, a killed process). The
  command line an app with a Rust crate re-executes into is put in such a job too.
- `nsh run --target android` forwards the dev server's port to the device (`adb reverse tcp:<port> tcp:<port>`,
  `android::Reverse`) and removes that rule, and only that one (`adb reverse --remove tcp:<port>`), however the run
  ends: normally, by an error, by Ctrl-C or SIGTERM, or for a restart of an app with a Rust crate (exit code 75; the
  next run sets it again). Otherwise a host left on the device keeps reaching whatever dev server listens on that port
  next, of any app. Ctrl-C reaches the run through `ui::Stoppable`: while one is held, the signal handler records
  SIGINT / SIGTERM instead of ending the process, the dev session's watch loop returns, the command undoes what it set
  up, and `main_with` then ends the process by the same signal; a second signal ends it at once. The prebuilt `nsh`
  that runs an app's own command line holds one too while it waits, so the shell (and `cargo run`'s restart, which
  stops the process group) waits for the clean-up, and passes a stop sent to it alone on to the app's command line,
  which does not take that one as a second (the sender is its parent: `SA_SIGINFO`). `nsh dev` sets nothing up on
  devices.
- Hosts that cannot run the app: several hosts on one session is intended (a phone joining), but a host serves only the
  UI sets it was built with, and dev hosts keep reconnecting, so a host built for another app (an Android host left on
  the emulator, whose `adb reverse` still reaches this port) can connect. The hello reports the host's platform and
  the specifiers of the UI sets it serves (`HostPlatform`, `ui_sets`: iOS `set_ui_sets` or the mobile UI, Android
  `ui_sets!` or the mobile UI); each bundle is published with the UI sets it imports (`Built::ui_sets`: the bundle's
  native imports that are UI sets, `Bundle::natives` / `HotBundle::natives`). The server checks every host against
  the latest bundle, on connect and on each new one: a host missing any gets neither the bundle nor its patch (nor
  build errors, which would hide why) but `ServerMessage::CannotRun`, which it shows over the app ("This host cannot run
  the app", what is missing and the command that installs the app's host, from `DevServer::install_commands`: one per
  platform `nsh run` has a target for), and the CLI prints one `warn` line (`ClientEvent::CannotRun`), once per change
  of what is missing. A later bundle that needs only what the host has reaches it again. Hosts that do not report
  their UI sets (older ones) get every bundle.
- The host's `log` output (including JS `console.*`, target `js`) is forwarded to the CLI terminal with the device name while connected, and goes
  to stderr when disconnected (see "Output" for what the CLI shows of it).
- Build failures, bundle evaluation failures and uncaught exceptions show an overlay on the host (dark, the title in red; tap to dismiss; its Copy button copies the error, to paste into an AI chat or a report); it disappears on the next
  successful reload.
- **Hot module replacement** (`nsh dev` / `nsh run` without `--release`): development bundles keep one CommonJS chunk per
  module (rolldown `preserveModules`, tree shaking off), each registered with the native hot runtime
  (`crates/nsh-js/src/hot.rs`, the `__nsh_hot` global that importing `nsh:hot` makes; only development bundles import
  it). The script is an ES module importing `nsh:hot` and the namespaces of the native modules the app imports, for the
  factories' `require`; an edit importing a native module the running script did not makes the host reload. After a
  rebuild the CLI compares the chunks with the last good build and publishes a patch (`ServerMessage::Patch`, protocol
  v2) before the full bundle; hosts running the patch's base apply it in place (`EngineControl::hot_update`,
  ~0.2–0.5ms) and otherwise load the bundle. The runtime runs the changed modules again, plus the importers of modules
  that do not accept their own updates, up to the entry. The bundler's hot transform (`crates/nsh-bundler/src/hot.rs`,
  before the JSX is compiled) wraps top-level components (capitalized functions whose body has JSX) in stable proxies
  keyed by module and name, with a signature of their code plus the module's other code: a proxy is a memo over a
  version signal, so instances render again only when their signature changed (or a dependency module changed) and the
  rest of the app keeps its state. Top-level `context()` results keep their identity (taking the new default), and
  top-level `const`s made by `signal()` or `createNavigator()` (matched by the callee's name, as `context`) keep their
  value while their initializer's code is the same (`__nsh_hot.keep`, keyed by module and name; a changed initializer
  starts afresh). A function passed to a top-level `mount()` is a component too (`mount#<n>`). When a module runs
  again, a `mount` call with the same component as its previous run's next one (the same proxy: `mount(App)`, or the
  inline function's) and no props keeps that mount; from the first one that differs, the rest of the previous run's
  mounts are undone and mounted anew (`hot.rs` `reuse`). So what an update keeps:
  - an edit of a component (in any module, the entry too) renders its instances again: their own signals start
    afresh, and so does the state of the components they render (rendering a component again makes its children
    anew); everything outside them keeps its state;
  - an edit of a module's other code (a top-level constant, a helper) renders every component of that module again,
    and of the modules importing it up to the ones made only of components;
  - a navigator made at the top of a module keeps its routes, so a stack rendered again shows the same screens (the
    screens' contents render anew); a top-level signal keeps its value;
  - a changed `mount` call (another component, props) mounts the app again.
  `crates/nsh-engine/tests/engine/hot.rs` covers each case. A component that throws reports the error and renders
  nothing until the fix. A patch that throws shows the error and the next save recovers; a patch that cannot be applied (`false`)
  makes the host reload. Verified on the iOS simulator, the Android emulator and Mac Catalyst.
- Stacks are symbolicated with source maps: bundling produces a hidden source map (the TSX→JSX→JS maps are composed with
  `rolldown_sourcemap::collapse_sourcemaps`), sent along with the bundle; the engine rewrites `bundle.js:L:C` to `App.tsx:L:C` before reporting.

## `nsh build --split`

Writes a split bundle (`native-pipeline.md`, "Split bundles"): `bundle.js` / `.map` / `.nshc` and, per chunk,
`<id>`, `<id>.map` and `<stem>.nshc` (`build::compile_split`, `Built::chunks`). Without `--split` (and for `nsh
package`, `publish`, the dev server) the bundle holds the whole app.

## `nsh trace`

`crates/nsh-cli/src/trace.rs` runs the app in the CLI's process (`ApproxTextEngine`, a channel as the mount sink, 60 Hz
frames from a timer thread) and prints every batch. It keeps a model of the native tree from the mount items
(inserts, removals, `ViewProps`' `testID` and `accessibilityLabel`, texts, event masks) to find what to tap:
`--tap id:<testID>`, `label:<accessibilityLabel>`, `text:<text>`, `view:<id>` or a bare string (testID, then label,
then text), the first match in tree order below the root, or its nearest pressable ancestor; it is dispatched as
`Press`. Without `--tap`, `--taps <n>` taps the first pressable view. Waits are bounded so apps with timers or
animations end: before the taps the app must be quiet for `--idle-ms` (at most 10 s), after each tap for 100 ms (also
at most 10 s); `--wait <ms>` runs exactly that long instead, `--duration <ms>` keeps running after the last tap.
`--safe-area <top,right,bottom,left>` sets the environment's insets (`useSafeAreaInsets`, `SafeAreaView`, screens).

## Rust application projects (`nsh init --rust`)

`nsh init <dir> --rust` creates an independent Cargo workspace with published NSH dependencies,
`src/main.rs` (a native desktop entry using `AppSource::Rust(Root::of::<App>())`) and `src/lib.rs`
(the root component and its `#[native_module("app/native")]` registry). The platform dependencies
are target-specific and disable default features, so `cargo run` mounts the Rust root directly.
`App.tsx` imports that same component; `nsh.json` sets `"rust": "."`, so the existing Rust-component
build / development / packaging path serves it on the selected platform. The generated app accepts
`NSH_SCRIPT` for native desktop window checks. `examples/gallery-rs` exercises both entry points and
has a headless test of the Rust root's input handling.

The display name becomes a normalized Cargo package name prefixed with `nsh-app-`; the main entry
uses its Rust crate identifier. Rust creation checks for existing app and Cargo source files before
writing any of them. The ordinary TSX initializer continues preserving existing files.

Independent apps use crates.io dependencies. Generated host workspaces patch those NSH dependencies
to the selected source checkout, so the app and its hosts share the same core and registry types.
The app's Rust watcher excludes `target`, `.nsh` and `.git`, including generated sources under the
app root. Custom TypeScript declarations are generated when the app's command line is first built.

## Apps with Rust components of their own (`nsh.json` `"rust"`)

`"rust": "<crate dir>"` names a library crate of the app with a `#[native_module("<specifier>")]` function at its
root, which registers the crate's Rust components, modules and assets into its UI set, imported by the app's TSX by
that specifier (`examples/gallery-rs`: `import { Gallery } from "gallery/native"`):

```rust
use nsh_core::ui::prelude::*;

#[native_module("gallery/native")]
fn native(registry: &mut Registry) {
    registry.register::<Gallery>();
    registry.assets(assets!());
}
```

The attribute (`nsh-macros` `native_module.rs`) checks the specifier at compile time (a bare import specifier, not
`nsh` or `nsh/...`) and makes a hidden entry of a fixed name, `__nsh_native_module() -> Registry`
(`Registry::ui_set(specifier)`, then the function), which the generated code below calls. A second one in the crate
fails as that name defined twice; one outside the crate's root fails at the attribute (a glob of `crate::*` in a
block finds the entry or a stand-in whose bound reports where it goes: `#[diagnostic::on_unimplemented]`). A crate
without one fails the generated workspace's build with what to add, the same way: the generated code glob-imports
the app's crate in a block, which shadows its import of NSH's stand-in (`nsh_core::ui::__private`). The prebuilt
toolchain cannot know the set, so `nsh` builds the app its own (`crates/nsh-cli/src/rust_app.rs`):

- A Cargo workspace in `<app>/.nsh/rust`, written on every command (files only when they change): `cli` (the
  command line, `nsh_cli::main_with` of the mobile UI and the app's set), `ios-host` / `ios-app` (the development
  host and the release runner: `nsh_platform_ios::set_ui_sets`, then `nsh_ios_host::main` / `nsh_ios_app::main`),
  `linux-host` / `linux-app`, `macos-host` / `macos-app` and `windows-host` / `windows-app` (the same on Linux, macOS
  and Windows, `nsh_platform_linux` / `nsh_platform_macos` / `nsh_platform_windows::set_ui_sets`; `windows-app` is a
  windowed program, with no console) and `android` (the app's `libnsh.so`: `nsh::ui_sets!`, which the Android
  runtime looks up when an engine starts). `gpui-host` / `gpui-app` (`nsh_platform_gpui::set_ui_sets`, under
  `cfg(any(target_os = "linux", target_os = "macos", windows))`; `gpui-app` is a windowed program on Windows too) are
  a workspace of their own in `.nsh/rust/gpui` (excluded from the main one), written only when a GPUI host or runner is
  built (`RustApp::gpui_workspace`), so builds for the other targets never resolve GPUI's crates. They
  build against NSH's sources (the checkout the running `nsh` was built in, or `NSH_SOURCE`) with NSH's `Cargo.lock`
  and target directory, so only the app's code and what links it compile; the binaries are named after the app
  (`nsh-app-<package>-cli`).
- A command on the app (`build`, `trace`, `dev`, `run`, `package`, `publish`) builds `cli` and runs again in it
  (`NSH_RUST_APP` names the workspace). Everything then knows the app's set: the bundler keeps its imports, bytecode
  links against its exports, `.nsh/types` declares it (`.nsh/types/<specifier>.d.ts`, found through tsconfig's `"*"`
  path, which falls back to `node_modules`), `nsh trace` renders it. The hosts `run` and `package` need are built
  for the app instead of taken from the runtime (iOS `--target` triples; Linux's, macOS's, Windows' and GPUI's
  `*-host` / `*-app` for this machine; Android: the prebuilt `classes.dex` with
  the app's `libnsh.so` for each ABI of the prebuilt runtime, cross-compiled with the NDK).
- `nsh dev` / `nsh run` watch the crate too: a change to its `.rs` files, `Cargo.toml` or `assets/` ends the app's
  command line with exit code 75, and `nsh` builds it and runs the command again (the app restarts; JS and TSX edits
  stay hot).
- The files the sets' crates ship (`Registry::assets(assets!())`: a crate's `assets/` directory, published as
  `asset:/@<package>/<path>`) go with the app's imported assets: the dev server serves them
  (`DevServer::serve_files`), `nsh build` / `package` / `publish` copy them (`build::Built::assets`: each file's path
  and where it is), `nsh trace` reads them; the engine registers their fonts before the first render.

## Finding the runtimes

The CLI finds prebuilt runtimes next to itself (`bin/../runtime`, the distribution layout) or, in a checkout, in
`target/<triple>/<profile>` (Linux's, macOS's, Windows' and GPUI's, built for this machine, in `target/<profile>`,
Windows' `nsh-windows-host.exe` / `nsh-windows-app.exe`; the distribution has GPUI's in `runtime/gpui/` on each OS) and `target/android/<profile>[-dev]` with the CLI's own profile (`NSH_RUNTIME` overrides). In a checkout (`<root>/target/<profile>/nsh` next to
`crates/nsh-ios-host`), the CLI builds what it needs with cargo first (`cargo build -p nsh-ios-host --target ...`,
`cargo xtask android --dev`), so `nsh run` never uses a missing or stale host. The web runtimes (`runtime/web/nsh.wasm`
for published apps, `nsh-dev.wasm` for the development host, and `nsh.js`: `docs/dev/web.md`) are always built with
the `web` profile, into `target/wasm32-unknown-unknown/web/`.

## Releases

`.github/workflows/release.yml` runs `cargo xtask dist` on macOS (Apple silicon: the iOS, Mac Catalyst and macOS
runtimes and the iOS SDK), Linux and Windows (x86_64), each with the GPUI runtime of its OS and the Android runtime, checks that the packaged CLI
makes and bundles a new app, and archives `dist/nsh` as one directory `nsh/` in `nsh-<platform>.tar.gz` (`.zip` on
Windows), plus the embedding SDKs alone (`nsh-embed-ios.zip`, `nsh-embed-android.zip`), each with a `.sha256`. Asset
names carry no version, so `releases/latest/download/<asset>` always resolves; the installers and package managers
rely on that. To release, set `version` in the root `Cargo.toml`, push the tag `v<version>` (a tag that does not
match fails the run) and publish the draft release the run creates once every platform has built. Running the
workflow by hand without a tag, or a pull request that changes it or xtask, builds the archives only (the run's
artifacts). The binaries are not signed: macOS quarantines downloads from a browser (`xattr -dr
com.apple.quarantine nsh`), and the Linux runtime needs glibc 2.39 and GTK 4.14 or newer (Ubuntu 24.04 and later).

The same tag runs `.github/workflows/release-crate.yml`, which publishes the library crates to crates.io (secret
`CARGO_REGISTRY_TOKEN`): the main workspace's crates that do not inherit `publish = false` (they set `homepage`,
`readme` and `repository` instead), then `nsh-platform-gpui` from `crates/gpui`. The hosts, the runners,
`nsh-platform-web` and xtask are never published. A new version also sets the `version` of the `nsh-*` entries of
both workspaces' `[workspace.dependencies]` (the run fails otherwise). Crates already on crates.io at that version are
skipped, so a run stopped by crates.io's rate limit on new crates can simply be run again; running it by hand with
*dry run* packages and verifies without uploading.

How users get it (`website/docs/installation.md`):

- `website/public/install.sh` and `install.ps1` (served at `nsh.dev/install.sh`, `/install.ps1`, and
  attached to each release) download the latest archive or `NSH_VERSION`, check its `.sha256` and replace
  `~/.nsh` (`%LOCALAPPDATA%\nsh` on Windows) with it; `~/.local/bin/nsh` links to it, Windows adds `bin` to the
  user's `PATH`. The CLI resolves its own symlink (`runtime::cli_exe`) to find `bin/../runtime`: macOS reports
  `current_exe` as it was run.
- Homebrew: publishing the release runs `.github/workflows/packages.yml`, which fills in
  `.github/packages/nsh.rb` with the archives' checksums and pushes it to `longbridge/homebrew-tap` as
  `Formula/nsh.rb` (secret `HOMEBREW_TAP_TOKEN`). The formula keeps the distribution in `libexec`.
- Scoop: the release job fills in `.github/packages/nsh.json` and attaches it as `nsh.json`; Scoop installs from
  `releases/latest/download/nsh.json` and updates from the same URL, so nothing is committed anywhere.
- mise: its `github:` backend picks the archive by OS and architecture, strips `nsh/` and finds `bin/`.

## Entries and `cargo run`

Commands that build an app take its entry module or its directory (the current one by default), whose `App.tsx` is
the entry (matched in any case: `crates/nsh-cli/src/entry.rs`). `cargo run` is `xtask run` (the workspace's default
member): the gallery on the machine's own backend (the AppKit host on macOS, the GTK host on Linux, the Win32 host on Windows; `--target` picks another) through `nsh run`, in a process group of its own that is stopped and
started again when a source under `crates/` changes (`crates/xtask/src/run.rs`; it watches the canonical
`crates/`: inotify reports paths under the one watched as given, and the `..` of `CARGO_MANIFEST_DIR/../..` would make
every path look like a dot entry, which is skipped). On Windows the group is a `CREATE_NEW_PROCESS_GROUP` one, stopped
with Ctrl-Break (`nsh` takes it as Ctrl-C and stops its host) and then by closing the kill-on-close job `nsh run` was
put in. The first two lines (`Finished`,
`Running`) are the outer `cargo run`'s own, which only a global `term.quiet` would hide (it would hide `cargo build`'s
too); `cargo xtask [run] [<app>]` is the same task through the alias in `.cargo/config.toml` (`run -q`), without them. xtask
drops the variables `cargo run` sets for it (`CARGO_MANIFEST_DIR`, `CARGO_PKG_*`, ...) first: the builds it and `nsh`
start would inherit them, and since some crates' builds depend on them, every `cargo run` rebuilt ring, taffy and the
rest of what a plain `cargo build` had built.
`.cargo/config.toml` sets `IPHONEOS_DEPLOYMENT_TARGET = 15.0` for Rust and C alike.

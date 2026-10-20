---
title: CLI
description: Every NSH subcommand and flag - init, build, trace, check, dev, run, package, and the signing commands keygen, certify, publish, verify, symbolicate and revoke.
order: 6
group: start
---

# CLI

`nsh` is the only tool an NSH app needs. Every subcommand that builds an app takes `[ENTRY]`: the entry module, or the app's directory, whose `App.tsx` is the entry (default: the current directory, so `cd my-app && nsh run` works). It reads `nsh.json` next to the entry and refreshes the type declarations in `.nsh/types`.

```text
nsh <COMMAND>

  init         Create an app
  build        Bundle an app and compile it to QuickJS bytecode
  trace        Run an app headless and print the mount instructions and timings
  check        Type check an app with TypeScript's native compiler
  dev          Serve an app to dev hosts (simulators, devices) and push rebuilds
  run          Like dev, and also run the app in a development host
  package      Package an installable app from the prebuilt runtime
  keygen       Create a signing key
  certify      Certify a release key with a root key
  publish      Build an app in release mode and sign it into a package (.nshpkg)
  verify       Verify a package
  symbolicate  Map the positions in a reported error stack to the app's sources
  revoke       Sign a revocation list
```

`nsh <command> --help` prints the flags of a command; `nsh --version` prints the version.

## Output

Every command prints one line per step, and the details only when something fails:

```text
✓ built the Rust crate gallery-native (5.0s)
✓ built App.tsx (500 B js, 809 B bytecode, 21ms)
✓ built the development host for the iOS simulator (12.7s)
• iOS Simulator (iPhone 16 Plus) connected, build 1
warn iOS Simulator: image failed: An SSL error has occurred and a secure connection to the server cannot be made.
✓ rebuilt App.tsx (18ms), 1 device
```

`✓` is a finished step, `→` where to connect, `•` what a device did; `warn` and `error` lines are problems, and
plain lines are your app's own output (`console.log`). On a terminal a spinner shows the step that is running, and
lines are colored (unless `NO_COLOR` is set); redirected to a file, only the result lines are written, without colors.
The output of the tools NSH runs (cargo for an app's Rust crate, the development hosts) is printed below an `error`
line when one of them fails.

`-v, --verbose` (any command; also `NSH_VERBOSE=1`) prints everything as it comes: every log line with its level and
source, cargo's output and the hosts' own.

## nsh init

```sh
nsh init [DIR] [--name <NAME>] [--template <ts|rust|mixed>] [--nsh-source <DIR>]
```

Creates an app in `DIR` (default: the current directory, created if missing) from a template. Without `--template`, `nsh init` asks which one on a terminal (Enter picks `ts`) and takes `ts` elsewhere (a script, CI).

| Template | What it creates |
|---|---|
| `ts` | A TSX app: `App.tsx`, `nsh.json`, `tsconfig.json`, `.gitignore` and `.nsh/types`. Files that exist are kept; it fails only if all of them exist already. |
| `rust` | An app written in Rust: a Cargo project (`Cargo.toml`, `src/lib.rs` with the root component, `src/main.rs` for `cargo run`), `nsh.json` with `"rust": "."` and `"root": "App"`, `.gitignore` and a README, which also says how to add a TSX entry later. `nsh run`, `nsh trace` and `nsh package` mount the root with no QuickJS on every target, the browser included ([Building with Rust](./rust.md)); `cargo run` starts it on macOS, Windows or Linux directly. |
| `mixed` | A TSX app with a Rust crate of its own: `App.tsx`, `tsconfig.json`, `nsh.json` with `"rust": "native"`, `.gitignore`, `.nsh/types`, a README, and the crate in `native/`, whose `#[native_module("<name>/native")]` exports a component, a module and a class that `App.tsx` uses. |

| Flag | Description |
|---|---|
| `--name <NAME>` | Display name written to `nsh.json`. Default: the directory's name. The id becomes `com.example.<name>` (letters and digits only, lower case), and the crates' import specifier `<name>/native` (lower case, words joined by `-`). |
| `--template <TEMPLATE>` | `ts`, `rust` or `mixed` (above). `rust` and `mixed` need Rust; `cargo run` also needs the native desktop backend's development libraries. |
| `--nsh-source <DIR>` | With `rust` or `mixed`: the crates depend on the NSH crates of the checkout in `DIR`, by path, to build the app against NSH's sources. |

The crates of `rust` and `mixed` depend on NSH's crates on crates.io at the CLI's version (with `--nsh-source`, on a checkout's), and the `nsh` commands build the app's runners and hosts from the same crates. Before each command on such an app, `nsh` builds a small program of the app's crates and NSH's core that describes their components, modules and classes, and runs it when the crates changed; with that description the CLI itself bundles, compiles, type-checks, serves and packages the app. A Rust app's builds compile its crates and NSH's core alone. Create them in a new app directory: any of their files existing is reported as a conflict before anything is written.

## nsh run

```sh
nsh run [ENTRY] [--target catalyst|ios|android|macos|linux|windows|gpui|web] [--device <DEVICE>] [--port <PORT>] [--release] [--trace] [--fps] [--name <NAME>] [--window-size <WIDTHxHEIGHT>] [--script <STEPS>]
```

Starts the dev server (as `nsh dev`), launches a development host that loads the app from it, and rebuilds on every save. Hot module replacement patches the running app in place; logs and errors from the app are printed in the terminal. Stop it with Ctrl-C (or by closing the Catalyst, macOS, Linux, Windows or GPUI window). However it ends (Ctrl-C, `kill` / `pkill`, an error), it stops the host it started: the Catalyst app, the macOS, Linux, Windows or GPUI host, the development host in the simulator, or on the Android device.

A Rust app (`nsh.json` `"root"`, [Building with Rust](./rust.md)) runs with no dev server and no QuickJS: `nsh run` builds the app's runner for the target with its root mounted, starts it (on the desktops a debug build, its logs in the terminal; in the simulator and on Android, the app installed), and on a change to the crate's Rust code builds it again and restarts it. With `--target web` it prints the page's address, and the open page loads the new runtime by itself. Name the TSX entry (`nsh run App.tsx`) to run that instead.

| Flag | Default | Description |
|---|---|---|
| `--target <TARGET>` | `catalyst` on macOS, `linux` on Linux, `windows` on Windows | `catalyst`: the Mac Catalyst host, an iPhone-sized window (390×844). `ios`: the host in an iOS simulator. `android`: the host on an Android device or emulator. `macos`: the AppKit host, a 1200×800 window ([macOS](./macos.md)). `linux`: the GTK host, a 390×844 window ([Linux](./linux.md)). `windows`: the Win32 host, a 1200×800 window ([Windows](./windows.md)). `gpui`: the GPUI host, a 1200×800 window, on macOS, Windows or Linux ([GPUI](./gpui.md)). `web`: the dev server's page, at the address it prints ([Web](./web.md#develop-in-the-browser)). |
| `--device <DEVICE>` | | iOS simulator to use, by name (`"iPhone 16 Pro"`) or UDID. Default: a booted one, else the newest iPhone, which is booted. |
| `--port <PORT>` | `8790` | Dev server port. If it is taken, the next free one is used. |
| `--release` | | Minify and strip debug info. Release bundles have no hot module replacement: every save reloads the app. |
| `--trace` | | The host logs the timing of every commit (JS, layout, mount items) to the terminal. |
| `--fps` | | Shows the performance monitor over the app: frame rate, what frames and commits cost, CPU, GPU and memory ([macOS](./macos.md#performance-monitor); `--target macos` only for now). |
| `--name <NAME>` | `nsh.json` name | Application display name in the menu bar and initial window title (`--target macos`, TSX entries). |
| `--window-size <WIDTHxHEIGHT>` | Host default | Initial window size in points, such as `800x600` (`--target macos`, `linux`, `windows` and `gpui`, TSX entries). |
| `--script <STEPS>` | | Runs semicolon-separated automation steps, such as `"wait 800; shot a.png; quit"` (`--target macos`, `linux`, `windows` and `gpui`; an app written in Rust: `macos`, `linux` and `windows`). With `quit` the command ends with the script: screenshots of an app with a Rust crate of its own need no host started by hand. On macOS, screenshot-only scripts start in the background; scripted presses activate the app when needed. |

With `--target android`, the first device `adb devices` lists is used; the dev server's port is forwarded to it with `adb reverse`. See [Getting started](./getting-started.md#run-it) and [Dev server and debugging](./dev-server-and-debugging.md).

## nsh dev

```sh
nsh dev [ENTRY] [--port <PORT>] [--release]
```

Runs only the dev server: it builds the app, watches the entry's directory, and pushes every rebuild to the hosts connected to it. It prints the URL for hosts on this machine and one per LAN address for devices, and, when the web runtime is there, the page browsers open to run the app ([Web](./web.md#develop-in-the-browser)):

```text
✓ built App.tsx (3.2 KB js, 9.8 KB bytecode, 21ms)
→ dev server ws://127.0.0.1:8790/ws
  browsers: http://127.0.0.1:8790/
  devices on this network: ws://192.168.1.20:8790/ws
  browsers on this network: http://192.168.1.20:8790/
```

Use it with hosts you start yourself, for example an app that embeds the development SDK (`NativeShellDev.xcframework`, `nsh-dev.aar`). Flags as for `nsh run`.

## nsh trace

```sh
nsh trace [ENTRY] [--tap <SELECTOR>]... [--taps <N>] [--idle-ms <MS>] [--wait <MS>] [--duration <MS>]
          [--scroll <POINTS>] [--width <W>] [--height <H>] [--safe-area <INSETS>]
```

Runs the app headless, with the same core and runtime as a device but no views, and prints every mount instruction (create, insert, style, frame, text) and the time of each commit. Then it taps views (the ones `--tap` names, or the first pressable one) and prints what each tap changed. Use it to check that an app starts without errors, to count the work an interaction causes, and in CI ([Testing and trace](./testing-and-trace.md)). A TSX app runs in the CLI's own process. An app with Rust crates runs in a headless runner built with them, which the CLI hands the bytecode it compiled; a Rust app (`nsh.json` `"root"`, [Building with Rust](./rust.md)) runs in one with its root mounted, no bundle and no QuickJS. The output is the same either way.

```text
✓ bundled App.tsx (3255 B in 12.6 ms)
perf commit: 56 ops, 412µs (list js 6µs), 49 mount items, 21 nodes
── initial render: 49 items
   Create { id: 2, kind: View }
   ...
── tap #1 on view 20: 3 items
   Text { id: 14, text: "1", content: 71x77 @ 0,0 }
```

| Flag | Default | Description |
|---|---|---|
| `--tap <SELECTOR>` | | Tap a view; repeat it for several taps, in order. `id:<testID>`, `label:<accessibilityLabel>`, `text:<text>`, `view:<id>` (a view id from the output), or a bare string, matched as a `testID`, then a label, then a text. The first match on screen is tapped, or its nearest pressable ancestor (the button around a text). No match is an error. |
| `--taps <N>` | `2` | Without `--tap`: taps sent to the first pressable view. A tap that changes nothing is an error (exit code 1). |
| `--idle-ms <MS>` | `1200` | Before tapping, wait until the app renders nothing for this long (at most 10 s, for apps with timers or animations). The default lets the engine's idle garbage collection run first, so retention bugs show up. |
| `--wait <MS>` | | Run timers and animations exactly this long before the first tap and after each one, printing what they render, instead of waiting until the app is idle. |
| `--duration <MS>` | `0` | Keep running this long after the last tap. |
| `--safe-area <INSETS>` | `0` | Safe area insets in points, `top,right,bottom,left` (or one value for all four), e.g. `47,0,34,0` for a phone with a notch. |
| `--scroll <POINTS>` | | Scrolls the first scroll view or list by this many points, 10 points per frame, and prints each frame's batch. |
| `--width <W>` | `390` | Viewport width in points. |
| `--height <H>` | `844` | Viewport height in points. |

`nsh trace` behaves as iOS: `Platform.OS` is `"ios"`, and `ios:` classes and `ios={{…}}` props apply.

## nsh check

```sh
nsh check [ENTRY] [--tsc <TSC>] [-- <TSC ARGS>...]
```

Type checks the app: refreshes `.nsh/types` and `.nsh/tsconfig.json`, then runs `tsc -p <app directory> --noEmit` with the app's `tsconfig.json`. It uses TypeScript 7, the native compiler, which needs no Node: the first run downloads it from the npm registry (about 9 MB), checks it against the checksum NSH records, and keeps it in NSH's cache (`~/Library/Caches/nsh` on macOS, `~/.cache/nsh` on Linux, `%LOCALAPPDATA%\nsh` on Windows, or `NSH_CACHE`). Type errors make it exit with a non-zero code. It warns when `tsconfig.json` does not extend `./.nsh/tsconfig.json`, where NSH keeps its TypeScript settings ([Project structure](./project-structure.md#tsconfigjson-and-nshtypes)).

| Flag | Description |
|---|---|
| `--tsc <TSC>` | Run this `tsc` instead (also `NSH_TSC`), for example one from `node_modules` or a copy for machines without network. |
| `-- <TSC ARGS>` | Passed on to `tsc`, e.g. `nsh check -- --pretty false`. |

## nsh build

```sh
nsh build [ENTRY] [-o <OUT>] [--release] [--split]
```

Bundles the app and compiles it to QuickJS bytecode, for apps that embed NSH and ship the bundle themselves. For a Rust app (`nsh.json` `"root"`), it builds and describes the app's crates and writes `app.json` and the files they ship (`assets/`): its runners mount the root, so there is no bundle.

| Flag | Default | Description |
|---|---|---|
| `-o, --out <OUT>` | `out` | Output directory. |
| `--release` | | Minify and strip debug info. |
| `--split` | | Put what `import()` loads into chunk files of its own, downloaded when first imported (web apps, see [Split bundles](./web.md#split-bundles)). Without it, the bundle holds the whole app. |

It writes:

| File | Content |
|---|---|
| `bundle.js` | The bundle as JavaScript |
| `bundle.js.map` | Its source map (stack traces are mapped back to your sources) |
| `bundle.nshc` | The bundle as compiled bytecode: what apps load |
| `<Chunk>.js`, `<Chunk>.js.map`, `<Chunk>.nshc` | With `--split`: each chunk, its source map and its bytecode (`Screen.js` for `import("./Screen")`) |
| `capabilities.json` | The `capabilities` of `nsh.json`, if declared (removed otherwise) |
| `assets/` | Every imported image and font, at its path relative to the app root |

## nsh package

```sh
nsh package [ENTRY] --target ios|ios-simulator|android|macos|linux|windows|gpui|web [-o <OUT>] [flags]
```

Builds an installable app from the prebuilt runtime, in release mode. No Rust and no Xcode project or Gradle build are involved: NSH assembles the app bundle itself and uses Xcode's `actool` and `codesign` (iOS) or the SDK's `aapt2`, `zipalign` and `apksigner` (Android); for macOS it assembles an ad hoc signed `.app` with `codesign` (and `sips` for the icon); for Linux it copies the runner and writes a `.tar.gz` with `tar`; for Windows it copies the runner, embeds the icon and writes a `.zip`, with no tools; for GPUI it does what it does for this machine's OS, with the GPUI runner; for the web it writes a static site, with no tools. Metadata comes from `nsh.json`, overridden by the flags. A Rust app (`nsh.json` `"root"`) is packaged with its own runner, built with Rust from NSH's crates: no bundle and no QuickJS, its root mounted ([Building with Rust](./rust.md)).

| Flag | Description |
|---|---|
| `--target <TARGET>` | Required. `ios`: iOS devices, a signed `.ipa` (or an unsigned `.app` without `--sign`). `ios-simulator`: an ad hoc signed `.app`. `android`: a signed `.apk`. `macos`: an ad hoc signed `.app` ([macOS](./macos.md#packaging)). `linux`: a directory `<Name>/` (the executable, `nsh/`, a `.desktop` file) and `<Name>.tar.gz` ([Linux](./linux.md#packaging)). `windows`: a directory `<Name>\` (`<Name>.exe`, `nsh\`) and `<Name>.zip` ([Windows](./windows.md#packaging)). `gpui`: what `macos`, `windows` or `linux` makes on this machine, with the GPUI runner ([GPUI](./gpui.md#packaging)). `web`: a directory `<Name>/`, a static site (`index.html` and files named by their content) ([Web](./web.md#publish-an-app)). |
| `-o, --out <OUT>` | Output directory (default `out`). |
| `--name <NAME>` | Display name. |
| `--id <ID>` | Bundle identifier, e.g. `com.example.app`. |
| `--app-version <VERSION>` | Version, e.g. `1.2.0`. |
| `--build <BUILD>` | Build number. |
| `--icon <ICON>` | App icon: a square PNG, ideally 1024×1024. |
| `--sign <IDENTITY>` | iOS devices: code signing identity, e.g. `"Apple Development: …"`. |
| `--profile <FILE>` | iOS devices: provisioning profile (`.mobileprovision`). Entitlements come from it. |
| `--keystore <FILE>` | Android: release keystore. Default: the debug key in `~/.android/debug.keystore` (created if missing). |
| `--key-alias <ALIAS>` | Android: key alias in the keystore. |
| `--keystore-password <PASSWORD>` | Android: keystore password. Also read from `NSH_KEYSTORE_PASSWORD`. |
| `--run` | Install and launch the app afterwards: in the simulator (`ios-simulator`) or on the connected Android device; `macos`, `linux`, `windows` and `gpui` launch it; `web` serves the site on this machine at the address it prints. |

```sh
nsh package App.tsx --target ios-simulator --run
nsh package App.tsx --target ios --sign "Apple Development: Jane Doe (ABCDE12345)" --profile dev.mobileprovision
nsh package App.tsx --target android --keystore release.jks --key-alias upload
nsh package App.tsx --target macos
nsh package App.tsx --target linux
nsh package App.tsx --target windows
nsh package App.tsx --target gpui
nsh package App.tsx --target web
```

See [Packaging](./packaging.md).

## Signing commands

These commands make and check signed packages (`.nshpkg`): app bundles that apps embedding NSH download and verify before running them. [Signed packages](./signed-packages.md) explains the model (an offline root key certifies release keys for some app ids).

### nsh keygen

```sh
nsh keygen <NAME>
```

Creates an Ed25519 key pair: `<NAME>.secret` and `<NAME>.pub` (e.g. `nsh keygen keys/root`). Keep a root's `.secret` offline; apps trust its `.pub`.

### nsh certify

```sh
nsh certify --root <ROOT.secret> --key <KEY.pub> --apps <PATTERN>... [--days <DAYS>] -o <OUT>
```

Certifies a release key with a root key for some app ids and a time window; packages carry the certificate.

| Flag | Description |
|---|---|
| `--root <ROOT>` | The root's secret key. |
| `--key <KEY>` | The release key's public key. |
| `--apps <APPS>` | App ids it may sign for: `*`, `com.example.*` or an id. Repeatable. |
| `--days <DAYS>` | How long it may sign, from now (default 365). |
| `-o, --out <OUT>` | The certificate to write. |

### nsh publish

```sh
nsh publish [ENTRY] --key <KEY.secret> --cert <CERT> [-o <OUT>] [--id <ID>] [--app-version <V>] [--build <N>]
            [--source-map package|file|none] [--strip-debug]
```

Builds the app in release mode and signs it into a package. Metadata comes from `nsh.json`, overridden by the flags.

| Flag | Description |
|---|---|
| `--key <KEY>` | The release key's secret key. |
| `--cert <CERT>` | Its certificate (`nsh certify`). |
| `-o, --out <OUT>` | The package to write. Default: `out/<id>-<build>.nshpkg`. |
| `--id <ID>` | App id. |
| `--app-version <VERSION>` | Version. |
| `--build <BUILD>` | Build number: must grow with every release. |
| `--source-map <WHERE>` | Where the source map goes: `package` (default), `file` (`<package>.map` next to the package) or `none` ([Signed packages](./signed-packages.md#errors-from-published-packages)). |
| `--strip-debug` | Leave the line and column tables out of the bytecode: smaller, but error stacks have no positions. |

### nsh verify

```sh
nsh verify <PACKAGE> [--root <ROOT.pub>]... [--revocations <LIST>]
```

Verifies a package as an app trusting the given roots would. Without `--root`, only the package's own signature is checked. It prints the package's SHA-256, which `nsh revoke --package` takes.

### nsh symbolicate

```sh
nsh symbolicate <MAP> [INPUT]
```

Rewrites the bundle positions (`bundle.js:LINE:COL`) of an error a package reported to your sources and prints the
result. `MAP` is a source map (`<package>.map` from `nsh publish --source-map file`, `bundle.js.map`) or a package that
holds one; `INPUT` is the error text (default: standard input).

### nsh revoke

```sh
nsh revoke --root <ROOT.secret> [--extend <LIST>] [--sequence <N>] [--key <ID>]... [--package <SHA256>]... -o <OUT>
```

Signs a revocation list: release keys and packages that must not load any more.

| Flag | Description |
|---|---|
| `--root <ROOT>` | The root's secret key. |
| `--extend <EXTEND>` | Keep the entries of this list (the last one published). |
| `--sequence <N>` | The list's number, higher than any published before. Default with `--extend`: that list's plus one. |
| `--key <ID>` | A release key id to revoke. Repeatable. |
| `--package <SHA256>` | A package SHA-256 to revoke. Repeatable. |
| `-o, --out <OUT>` | The list to write. |

## Environment variables

| Variable | Used by | Description |
|---|---|---|
| `ANDROID_HOME`, `ANDROID_SDK_ROOT` | Android targets | The Android SDK. Fallback: `~/Library/Android/sdk`. |
| `CARGO_TARGET_DIR` | apps with Rust crates | Where the app's runners build. Default: the app's `target/`. |
| `NSH_KEYSTORE_PASSWORD` | `package --target android` | The keystore password. |
| `NSH_RUNTIME` | `run`, `package` | A runtime directory to use instead of the one next to the CLI. |
| `NSH_VERBOSE` | every command | `1`: as `--verbose`. |
| `NO_COLOR` | every command | Set: no colors on terminals. |

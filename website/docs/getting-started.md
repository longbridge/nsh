---
title: Getting started
description: "Create an NSH app with nsh init, run it on iOS, Android, macOS, Windows, Linux or the web, and ship it: as signed TSX packages inside your own app, or as a standalone app."
order: 4
group: start
demo: guide-getting-started
---

# Getting started

This guide creates an NSH app, runs it, and ships it.

You write NSH apps in TSX and get hot updates while you develop. Your code runs on QuickJS. The engine, layout,
rendering and modules under it are native. When you need native speed, system access or a crate, add Rust with
[nshm](./nshm.md). You can also write the whole app in Rust, with no JavaScript VM. Each example below shows the TSX
version with the Rust version beside it.

You need `nsh` on your `PATH` ([Installation](./installation.md)). In an NSH checkout, use `./target/debug/nsh`.

> [!TIP]
> You don't need an existing app. The first two sections run the app on its own: in a desktop window, the iOS
> simulator, an Android device or a browser. From there you can [package it as a standalone
> app](#or-ship-it-as-a-standalone-app).

## Create an app

`nsh init` asks which template to use: `ts` (the default), `rust`, or `mixed` (TSX plus Rust in `ext/`). Pass
`--template` to skip the question.

### In TSX

```sh
nsh init my-app --template ts
```

A TSX app is one or more `.tsx` files with one entry. `nsh run` runs the entry, and a host app loads it as a module.

| File | What it is |
|---|---|
| `App.tsx` | The entry module: renders the app with `mount` |
| `App.test.tsx` | Its test (`nsh test`) |
| `nsh.json` | The app's name, id, version and build number ([Project structure](./project-structure.md#nshjson)) |
| `tsconfig.json` | Extends `.nsh/tsconfig.json` (NSH's TypeScript settings), which resolves `nsh` to `.nsh/types` |
| `.gitignore` | Ignores `.nsh/` and `target/` |
| `.nsh/types/` | Generated declarations of every NSH module, for your editor |

### In Rust

```sh
nsh init my-app --template rust
```

This creates a new directory with a Cargo project:

| File | What it is |
|---|---|
| `src/lib.rs` | The app: its root component, state and handlers |
| `src/main.rs` | Mounts the same root on macOS, Windows or Linux for `cargo run` |
| `Cargo.toml` | The crate, depending on `nsh-core` and `nsh-base` at the CLI's version |
| `nsh.json` | The app's name, id, version and build number, its crate (`"rust": "."`) and root component (`"root": "App"`) ([Project structure](./project-structure.md#nshjson)) |
| `App.test.tsx` | The root's test, in TypeScript (`nsh test`), with `tsconfig.json` and `.nsh/types` for your editor |
| `README.md` | How to run, test and package the app, and how to add a TSX entry later |
| `.gitignore` | Ignores `.nsh/` and `target/` |

Rust apps need a Rust toolchain ([rustup](https://rustup.rs)). We recommend
[huacnlee/cargo](https://github.com/huacnlee/cargo) for building. `nsh` builds the app's runners from NSH's crates on
crates.io, at the CLI's version.

Notes for both templates:

- A TSX app can go into an existing directory. `nsh init` keeps files that already exist. A Rust app always gets a new
  directory.
- The display name defaults to the directory name. Use `--name "My App"` to change it.
- The id is `com.example.<name>`. Change it in `nsh.json` before you ship.

### TSX extended with Rust

```sh
nsh init my-app --template mixed
```

You get the TSX app plus an nshm crate in `ext/<name>-native/`. `App.tsx` imports its component, module and class.
Put the parts that are better in Rust there ([An app's own Rust: ext/](./nshm.md#an-apps-own-rust-ext)). To add this
to a `ts` app later, use `nsh add`.

### The root component

Both templates make the same counter app:

```tsx tabs
// App.tsx
import { mount, signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";

function App() {
  const count = signal(0);
  return (
    <View class="flex-1 items-center justify-center gap-4 bg-background pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]">
      <Text class="text-3xl font-semibold text-foreground">{count()}</Text>
      <View
        onPress={() => count.update((c) => c + 1)}
        role="button"
        class="rounded-(--radius) bg-primary px-5 py-3 active:bg-primary/90"
      >
        <Text class="text-lg text-primary-foreground">Tap</Text>
      </View>
    </View>
  );
}

// The desktops show the app in windows it declares; `App` is the content of
// the first one. Off the desktops it is the app's screen.
mount(() => <Window id="main">{App}</Window>);
```

```rust tabs
// src/lib.rs
use nsh_base::theme;
use nsh_core::{element::desktop::window as app_window, prelude::*};

/// The crate's UI set: what a TSX entry may import from `my-app/native`.
#[native_module("my-app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The application's root component.
#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    app_window().id("main").content(move || {
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(
            view()
                .flex_1()
                .p_safe_area()
                .child(
                    view()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .child(text(move || format!("Count: {}", count.get())).text_size(px(28.0)))
                        .child(
                            view()
                                .px_5()
                                .py_3()
                                .rounded(px(10.0))
                                .bg(move || theme().primary)
                                .active(|s| s.opacity(0.6))
                                .on_press(move || count.update(|value| *value += 1))
                                .child(
                                    text("Tap")
                                        .text_size(px(17.0))
                                        .text_color(move || theme().primary_foreground),
                                ),
                        ),
                ),
        )
    })
}
```

In TSX:

- **`from "nsh"`**: the core module. It has signals, `mount`, the other reactive primitives and the host elements.
  The native runtime provides it, so it is not bundled. **`nsh/base`** has the base components.
- **`function App()`** is a component. It runs **once**. **`{count()}`** reads the signal, and the compiler makes this
  text update when the signal changes.
- **`<View onPress>`** is a native view that handles presses. `active:` classes style it while pressed. Import `View`,
  `Text`, `Image`, `Scroll` and the other [host elements](../elements/index.md) from `nsh`, and `Input` from
  `nsh/base`. Only `If` and `Each` need no import.
- **`class`** takes [Tailwind](./tailwind.md) classes. `style={{ … }}` takes camelCase style properties, with numbers
  in points. `bg-background`, `bg-primary`, `text-primary-foreground` and `rounded-(--radius)` are theme tokens
  ([Theme tokens](./tailwind.md#theme-tokens)). They follow dark mode, and `"theme"` in `nsh.json` restyles them.
- **Layout** is flexbox. Children stack in a column by default ([Layout](./layout.md)).
- **`mount(App)`** renders the root. On desktop, the root declares its windows with `<Window>`.

In Rust:

- **`use nsh_core::prelude::*`** imports the primitives (`view()`, `text()`, `scroll()`, `input()`, `image()`, ...),
  [signals](./reactivity.md) and typed style methods. `.p_safe_area()` pads a view by the
  [safe area](../elements/safe-area.md).
- **`nsh_base`** has the unstyled base components (`button()`, the keyboard, dialogs) and `theme()`, the app's colors
  and radii ([Theme](./rust.md#theme)).
- **`#[component] pub fn App()`** is a component. It runs **once** and returns its elements. `"root": "App"` in
  `nsh.json` names it, and `nsh run` mounts it. `signal(0_u32)` holds a value that changes.
- **`text(move || format!(…, count.get()))`** binds the text to the signal. When `count` changes, NSH reruns the
  closure and updates only this text. Styles take closures too: `.bg(move || theme().primary)` follows dark mode.
- **`.on_press(..)`** handles presses. `.active(|s| s.opacity(0.6))` is the pressed style, applied natively. The button
  is the app's own: NSH provides primitives and capabilities, and the app draws its UI with them.
- **`.px_5()`, `.rounded(px(10.0))`, `.gap_4()`** are typed style methods named after Tailwind utilities, in points
  (`px`).
- **`#[native_module("my-app/native")]`** lists what a TSX entry may import from the crate, if you add TSX screens
  later ([Building with Rust](./rust.md#extending-a-rust-app-with-tsx)).

> [!IMPORTANT]
> Text must be inside a text element: `<Text>` in TSX, `text(..)` in Rust. In TSX, literal text in a `<View>`
> (`<View>Hello</View>`) is a build error. A string from an expression (`<View>{name}</View>`) logs a warning.

## Develop it

### Run it

`nsh run` builds the app, launches it and rebuilds when you edit. App logs appear in the terminal. Pass the app's
directory. It runs `App.tsx` for a TSX app, or the root for a Rust app. To run another entry, name it:
`nsh run my-app/App.tsx`.

| App | What `nsh run` does |
|---|---|
| TSX | Starts a dev server and a development host that loads the app. Patches the running app on every save. Builds Rust in `ext/` into the host and rebuilds it when it changes. |
| Rust | Builds the app's own runner for the target: your crate, the root mounted, no QuickJS. Rebuilds and restarts on every Rust change. |

In a Rust app, `cargo run` starts the same root directly on the desktop backend.

Without `--target`, `nsh run` targets your machine: AppKit on macOS, Win32 on Windows, GTK on Linux. Use `--target`
for other platforms:

| Target | Command | Runs from |
|---|---|---|
| iOS simulator | `nsh run my-app --target ios` | macOS with Xcode |
| Android | `nsh run my-app --target android` | macOS, Windows, Linux |
| macOS (AppKit) | `nsh run my-app` | macOS (the default) |
| Windows | `nsh run my-app` | Windows (the default) |
| Linux (GTK 4) | `nsh run my-app` | Linux (the default) |
| Web | `nsh run my-app --target web` | macOS, Windows, Linux |

#### iOS simulator

```sh
nsh run my-app --target ios
nsh run my-app --target ios --device "iPhone 16 Pro"
```

`nsh` uses the booted simulator, or boots the newest iPhone. It installs the Rust app's runner or the development
host. `--device` picks a simulator by name or UDID.

#### Android

```sh
nsh run my-app --target android
```

Start an emulator or connect a device first (`adb devices` lists them). NSH picks the device named by
`--device <serial>` or `ANDROID_SERIAL`, otherwise the first one. If several are connected, it prints which one it
uses.

`nsh` installs a Rust app's own `libnsh.so`, or a development host APK. The APK is rebuilt only when the runtime
changes. `adb reverse` forwards the dev server port, so USB is enough. No shared network is needed.

#### macOS (AppKit)

```sh
nsh run my-app --target macos
```

The app opens as a Mac app in a resizable AppKit window, with mouse, trackpad and keyboard input. Requires macOS 14
or later. See [macOS](./macos.md).

#### Windows (default on Windows)

```sh
nsh run my-app
```

The app opens in a 1200×800 point window, with mouse, touchpad, touch and keyboard input. Requires Windows 10 1809 or
later. See [Windows](./windows.md).

#### Linux (default on Linux)

```sh
nsh run my-app
```

The app opens in a 390×844 point GTK window. Requires GTK 4.14 or later. See [Linux](./linux.md).

#### Web

```sh
nsh run my-app --target web
```

`nsh` prints the page address. Open it in a browser. A Rust app runs as WebAssembly. After a change, the open page
loads the new build. See [Web](./web.md).

### Edit it

Leave `nsh run` running and change the button's label:

```tsx tabs
<Text style={{ color: "white", fontSize: 17 }}>Add one</Text>
```

```rust tabs
text("Add one")
    .text_size(px(17.0))
    .text_color(move || theme().primary_foreground)
```

Save. In a TSX app, the CLI sends a patch. The host reruns the changed module and re-renders only the components
whose code changed. Here that is `App`, which owns `count`, so the count resets. A Rust app rebuilds and restarts, so
its count resets too.

Move parts of the UI into their own components, or into plain functions that return elements:

```tsx tabs
function TapButton(props: { onPress: () => void }) {
  return (
    <View onPress={props.onPress} activeStyle={{ opacity: 0.6 }} style={{ padding: 12, borderRadius: 10, backgroundColor: "#0969da" }}>
      <Text style={{ color: "white", fontSize: 17 }}>Tap</Text>
    </View>
  );
}

// In App: <TapButton onPress={() => count.update((c) => c + 1)} />
```

```rust tabs
/// The app's button: a view that answers presses.
fn tap_button(on_press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .p_3()
        .rounded(px(10.0))
        .bg(move || theme().primary)
        .active(|s| s.opacity(0.6))
        .on_press(on_press)
        .child(text("Tap").text_size(px(17.0)).text_color(move || theme().primary_foreground))
}

// In App: .child(tap_button(move || count.update(|value| *value += 1)))
```

In TSX, state survives when you edit a component that does not own it. Tap a few times, then change `TapButton`'s
color. The button updates and the count stays.

How hot replacement works:

- It works per top-level component: a function with a capitalized name whose body contains JSX.
- A component re-renders when its own code changes, or when module code outside components changes (a constant or
  helper it uses).
- If an edit can't be applied in place, the host reloads the whole app. Examples: importing an NSH module the running
  app has not imported yet, or changing capabilities in `nsh.json`.

#### Errors

| Error | Where it shows |
|---|---|
| Rust compile error | Terminal, with cargo's output. `nsh run` rebuilds on the next save. |
| TSX build error | Terminal, and a red overlay on the device with file, line and column. The app keeps running the last good build. The overlay closes after the next good build, or when you tap it. |
| Runtime exception | Same overlay, with the stack mapped back to your `.tsx` source. |

### Development tools

#### Mac Catalyst

```sh
nsh run my-app --target catalyst
```

The app opens with the iOS (UIKit) backend in a 390×844 point window on your Mac. Use it only to check
Catalyst-specific behavior, or for a Mac Catalyst app that embeds NSH. For a Mac app, use the default `macos`. To see
the phone layout, use `--target ios`.

## Host it in your app

A native app shows NSH in a view:

| Platform | View |
|---|---|
| iOS | `NshAppView` (a `UIView`) |
| Android | `NshView` (an Android `View`) |
| macOS, Windows, Linux, GPUI | `AppView` |

Place it like any other view, on any screen, as many times as you need.

A Rust root is compiled into the app. The app links your crate and mounts the root with `AppSource::Rust` on the
`AppView` of macOS, Windows, Linux, GPUI and iOS ([Building with Rust](./rust.md#an-app-written-in-rust)).

A TSX app is a module the view loads. In development it comes from the dev server. In a release it comes from a built
bundle or a downloaded signed package. The sections below load TSX modules.

### iOS

Link `NativeShellDev.xcframework` in Debug and `NativeShell.xcframework` in Release ([Embedding in iOS
apps](./ios.md)), then:

```swift
import NativeShell

let app = NshAppView()
app.onError = { message in print("nsh error: \(message)") }
view.addSubview(app)   // and constrain it like any view

#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)   // `nsh dev my-app` on your Mac
#else
try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "nshc", subdirectory: "nsh")!)
#endif
```

`nsh dev my-app` serves the module and pushes every save, like `nsh run` does. For release,
`nsh build my-app --release --out MyApp/nsh` writes the bundle to a folder. Add that folder to the app's resources.

### Android

Add `nsh-dev.aar` for debug builds and `nsh.aar` for release builds ([Embedding in Android
apps](./android.md)), then:

```kotlin
val nsh = NshView(this)
nsh.setErrorListener { message -> Log.e("nsh", message) }
setContentView(nsh)   // or add it to any layout

if (BuildConfig.DEBUG) {
    nsh.connectDevServer("ws://127.0.0.1:8790/ws")   // with `adb reverse tcp:8790 tcp:8790`
} else {
    nsh.loadAssetBundle("nsh")   // assets/nsh, from `nsh build my-app --release --out app/src/main/assets/nsh`
}
```

### macOS, Windows and Linux

Rust desktop apps embed NSH with `AppView` from the `nsh-platform-macos`, `nsh-platform-windows`,
`nsh-platform-linux` and `nsh-platform-gpui` crates. It is an `NSView`, a child window, a GTK widget or a GPUI view.
It mounts a Rust root or loads a built bundle. See [macOS](./macos.md#embedding-in-a-mac-app),
[Windows](./windows.md#embedding-in-a-win32-app), [Linux](./linux.md#embedding-in-a-gtk-app) and [GPUI](./gpui.md),
and [Platforms](./platforms.md#embed-in-a-native-app) for every host side by side.

### What the module may do

On iOS and Android, the host sets a module's permissions before loading it. `grantCapabilities` lists the network
hosts and other capabilities it may use. Limits cap its memory, call time and node count ([Sandbox and
capabilities](./sandbox-and-capabilities.md)).

To give modules your own native views (a map, a chart, a design-system component), register them as
[native components](./native-components.md).

## Ship updates as signed packages

TSX modules that the app downloads after release are signed packages. Create a root key once and keep it offline.
Use it to certify a release key, then publish:

```sh
nsh keygen keys/root && nsh keygen keys/release
nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
nsh publish my-app --key keys/release.secret --cert keys/release.cert --build 2
```

The app trusts the root public key and loads the download. NSH checks the signature, certificate and revocation list
before running any code:

```swift
let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
let trust = try NshTrust(stateDirectory: support.appendingPathComponent("nsh"))
try trust.addRoot(rootPublicKey)   // the contents of keys/root.pub
let info = try await app.loadPackage(at: downloadedFile, trust: trust)
```

Android uses the same flow with `NshTrust` and `NshPackage`. For hosting, delivery and revocation, see [Signed
packages](./signed-packages.md).

## Or ship it as a standalone app

`nsh package` builds an installable TSX or Rust app. You don't need your own Xcode project or Gradle build:

```sh
nsh package my-app --target ios-simulator --run
nsh package my-app --target android
nsh package my-app --target macos
nsh package my-app --target linux
nsh package my-app --target windows
nsh package my-app --target web
```

A TSX app uses the prebuilt runtime. With Rust in `ext/`, it uses a runtime built with that Rust. A Rust app uses its
own runner, built from your crate and NSH's crates. For options, signing and icons, see [Packaging](./packaging.md).

## Try it in the terminal

`nsh test` runs an app headless, with no device or window. Pass a TSX entry or a Rust app's `nsh.json`. It opens the
app and prints what it mounts. `--trace` also prints native view operations and the time of every commit
([Testing](./testing-and-trace.md)):

```sh
nsh test my-app/App.tsx --trace      # a TSX app's entry
nsh test my-app/nsh.json --trace     # a Rust app's root
```

Pass a directory to run the app's tests: `nsh test my-app`. Each template's `App.test.tsx` opens the app, presses the
button and checks the result. In a Rust app, the test opens the root with `open(".")`. You can also test a Rust app in
`cargo test` with `nsh_testing::Headless` ([Building with Rust](./rust.md#testing)).

## Next steps

- [JSX and components](./jsx-and-components.md) and [Reactivity](./reactivity.md): components and signals, in TSX and
  Rust.
- [nshm](./nshm.md): add Rust to a TSX app, in `ext/` or from crates.io, and use published nshm.
- [Building with Rust](./rust.md): apps written in Rust from start to end, their theme, and TSX on top of a Rust crate.
- [Platforms](./platforms.md): what each platform supports, and its own page ([iOS](./ios.md),
  [Android](./android.md), [macOS](./macos.md), [Windows](./windows.md), [Linux](./linux.md), [Web](./web.md), [TUI](./tui.md)).
- [Signed packages](./signed-packages.md) and [Sandbox and capabilities](./sandbox-and-capabilities.md): delivery and
  what modules may do.
- [Project structure](./project-structure.md): what goes in `nsh.json`, assets, files.

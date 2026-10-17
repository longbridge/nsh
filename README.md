# Native Shell (nsh)

**AI-generated UI, native in your app.** Native Shell renders UI written in TSX and Tailwind with each platform's
own native views, on iOS, Android, macOS, Windows and Linux, not in a web view. Apps load these modules at run time,
as plugins: written by a model or by your team, delivered as signed packages and run in a sandbox the host
configures. The same module can also be a standalone app.

```tsx
import { mount, signal, Text, View } from "nsh";

function App() {
  const count = signal(0);
  return (
    <View class="flex-1 items-center justify-center gap-4 bg-white dark:bg-zinc-900">
      <Text class="text-3xl font-semibold text-zinc-900 dark:text-white">{count()}</Text>
      <View class="rounded-xl bg-blue-600 px-5 py-3 active:opacity-60" onPress={() => count.update((c) => c + 1)}>
        <Text class="text-white">Tap</Text>
      </View>
    </View>
  );
}

mount(App);
```

Documentation, guides and live demos: **[native-shell.com](https://native-shell.com)**.

## Features

- **Real native UI.** Every element is a platform view (UIKit, Android views, AppKit, Win32 drawn with Direct2D,
  GTK 4), laid out with flexbox and grid ([Taffy](https://github.com/DioxusLabs/taffy)). The system's text, input,
  scrolling and accessibility just work, and native components such as maps and web views mix with nsh's views without
  layering problems.
- **Plugins, or standalone apps.** Put an `NshAppView` (iOS), `NshView` (Android) or `AppView` (desktop) on any screen
  of an existing app and load modules into it at run time, from a bundle, the dev server or a downloaded signed
  package: features ship without an app release, next to the host's own native components. The same module also
  packages into an installable iOS, Android, macOS, Windows or Linux app with `nsh package`, with no Xcode project or
  Gradle build.
- **AI-friendly: TSX + Tailwind.** A module is one TSX file of components, signals and Tailwind classes, the format models
  write best. Every element, prop and module has TypeScript declarations generated from the Rust sources, the docs are
  served to models as [llms.txt](https://native-shell.com/llms.txt), and `nsh trace` runs a module headless to check
  it without a device.
- **Sandboxed and signed.** Each view runs its module in its own QuickJS VM, with memory, call-time and node limits,
  reaching only the capabilities the host grants. Packages are signed with Ed25519 by release keys an offline root
  certifies, and the device checks a revocation list.
- **Fast, with no Node.js.** Only the app's code is JavaScript: signals, the renderer, layout, animation and every
  module apps import are Rust, and each frame crosses to the UI thread as one batch. The single `nsh` binary bundles,
  type-checks, serves hot reload and packages; module authors need neither npm nor Rust.

| Platform | Drawn with | Embed with | Requires |
|---|---|---|---|
| iOS | UIKit, CoreText | `NshAppView` (Swift, `NativeShell.xcframework`) | iOS 15+ |
| Android | Android views, `StaticLayout` | `NshView` (Java / Kotlin, `nsh.aar`) | Android 9 (API 28)+ |
| macOS | AppKit, Core Animation, CoreText | `AppView` (Rust, `nsh-platform-macos`) | macOS 14+ |
| Windows | Win32, Direct2D, DirectWrite | `AppView` (Rust, `nsh-platform-windows`) | Windows 10 1809+ |
| Linux | GTK 4, Pango | `AppView` (Rust, `nsh-platform-linux`) | GTK 4.14+ |
| GPUI (macOS, Windows, Linux; in development) | GPUI (GPUI Fast) | `AppView` (Rust, a GPUI view, `nsh-platform-gpui`) | macOS, Windows; Linux with Vulkan |

A WebAssembly build runs the website's live demos. The GPUI backend (`nsh run --target gpui`, `nsh package --target gpui`)
draws apps with GPUI on all three desktops and embeds in GPUI and GPUI Kit apps; it is in development.

## Quick start

Install the `nsh` command ([other ways](https://native-shell.com/docs/installation)):

```sh
curl -fsSL https://native-shell.com/install.sh | sh     # macOS, Linux
irm https://native-shell.com/install.ps1 | iex          # Windows (PowerShell)
brew install longbridge/tap/nsh                         # Homebrew
```

Create an app and run it with hot reload:

```sh
nsh init my-app
nsh run my-app                     # a window on this machine (on macOS, an iPhone-sized Mac Catalyst window)
nsh run my-app --target ios        # iOS simulator (needs Xcode)
nsh run my-app --target android    # Android emulator or device (needs the Android SDK)
nsh run my-app --target macos      # an AppKit window
```

`nsh init` writes `App.tsx`, `nsh.json` and a `tsconfig.json`, and keeps type declarations in `.nsh/types`, so any
TypeScript editor checks the app without `node_modules`. On every save only the edited components render again; the
rest of the app keeps its state. Logs and errors show in the terminal. What each platform needs:
[Installation](https://native-shell.com/docs/installation#platform-tools).

## Embed it in your app

iOS (Swift):

```swift
import NativeShell

let app = NshAppView()
view.addSubview(app)                      // constrain it like any view
#if DEBUG
try app.connectDevServer(URL(string: "ws://127.0.0.1:8790/ws")!)   // `nsh dev my-app`
#else
try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "qbc", subdirectory: "nsh")!)
#endif
```

Android (Kotlin):

```kotlin
val nsh = NshView(this)
setContentView(nsh)                       // or add it to any layout
if (BuildConfig.DEBUG) nsh.connectDevServer("ws://127.0.0.1:8790/ws") else nsh.loadAssetBundle("nsh")
```

`nsh build my-app --release` writes the bundle a release build loads. More:
[iOS](https://native-shell.com/docs/embedding-ios), [Android](https://native-shell.com/docs/embedding-android),
[desktops](https://native-shell.com/docs/platforms), [native components](https://native-shell.com/docs/native-components),
[sandbox and capabilities](https://native-shell.com/docs/sandbox-and-capabilities).

## Ship updates

```sh
nsh keygen keys/root && nsh keygen keys/release        # a root key kept offline, and a release key
nsh certify --root keys/root.secret --key keys/release.pub --apps 'com.example.*' -o keys/release.cert
nsh publish my-app --key keys/release.secret --cert keys/release.cert --build 2   # a signed .nshpkg
```

The app trusts the root's public key and loads what it downloads with `loadPackage`
([Signed packages](https://native-shell.com/docs/signed-packages)). To ship the module as an app of its own instead:

```sh
nsh package my-app --target ios-simulator --run        # also ios, android, macos, windows, linux
```

## Examples

| Example | What it shows |
|---|---|
| [`examples/gallery`](examples/gallery) | A tour of what nsh supports: lists, navigation, forms, controls, gestures, animation, images, styles, text, web view and video, `fetch` |
| [`examples/counter`](examples/counter) | The smallest app |
| [`examples/hopper`](examples/hopper) | A dark, tabbed dashboard built only from primitives: interactive canvas charts, sliders, menus, animation |
| [`examples/desktop`](examples/desktop) | A desktop app with a title bar and menus of its own (`nsh/desktop`) |
| [`examples/gallery-rs`](examples/gallery-rs) | The gallery with its own components written in Rust |
| [`examples/embed-ios`](examples/embed-ios), [`examples/embed-android`](examples/embed-android) | nsh inside existing native apps, with a native component the app registers |
| [`examples/bench`](examples/bench) | Rust components against the same components in TSX |

```sh
nsh run examples/gallery
```

## Building from source

With Rust ([rustup](https://rustup.rs); `rust-toolchain.toml` picks the toolchain):

```sh
git clone https://github.com/longbridge/native-shell.git && cd native-shell
cargo run                              # the gallery on this machine's backend, with hot reload
cargo run -- examples/counter          # another app
cargo run -- --target ios              # or catalyst, android
cargo xtask dist                       # the distribution (CLI, runtimes, SDKs) in dist/nsh
```

[Installation](https://native-shell.com/docs/installation#building-from-a-checkout) lists the hosts and runtimes each
target needs.

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) covers pull requests, commits and testing; [AGENTS.md](AGENTS.md) the commands,
rules and conventions; [docs/dev](docs/dev/README.md) the architecture and internals. The website's sources are in
[website/](website/README.md).

[GPUI Kit](https://gpui-kit.com) is a sibling project: components and tools for desktop apps built with GPUI. nsh's
theme colors follow its fields, and nsh's GPUI backend builds on it.

## License

Licensed under the [Apache License, Version 2.0](LICENSE-APACHE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as
defined in the Apache-2.0 license, shall be licensed under Apache-2.0, without any additional terms or conditions.

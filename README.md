# Native Shell (NSH)

**Native UI in Rust and TSX, embedded in your app or shipped on its own.** Native Shell is a Rust engine for
cross-platform apps and AI-generated UI. Write your app in Rust, use TSX for dynamically delivered screens, or
combine both with shared Rust components. Each platform renders with its own UI: native views on iOS, Android,
macOS, Windows and Linux, GPUI in GPUI apps and HTML on the web.

Embed NSH in an existing app or package it as a standalone app. Hosts can load TSX modules at run time as plugins,
delivered as signed packages and run in a sandbox the host configures.

Documentation, guides and live demos: **[nsh.dev](https://nsh.dev)**.

## Features

- Write apps in Rust, TSX, or both, and render with each platform's own UI, sharing the same reactive engine,
  flexbox and grid layout, system text, input and accessibility.
- Give your app its own look: NSH provides the primitives, Tailwind classes, a theme and an unstyled UI foundation
  (`nsh/base`: navigation, safe areas, dialogs, the keyboard, the status bar), and your app draws its components from
  them in TSX or Rust.
- Embed in existing native apps alongside components such as maps and web views, or package as standalone apps
  and static websites.
- Deliver TSX screens at run time with signed packages and host-controlled sandbox capabilities.
- Build AI-generated screens with TSX and Tailwind, generated TypeScript declarations and
  [model-readable documentation](https://nsh.dev/llms.txt).
- Iterate with TSX hot updates, headless tracing and a single Rust-built `nsh` CLI for building and packaging.

### Platforms

| Platform | Drawn with | Embed with | Requires |
|---|---|---|---|
| iOS | UIKit, CoreText | `NshAppView` (Swift, `NativeShell.xcframework`) | iOS 15+ |
| Android | Android views, `StaticLayout` | `NshView` (Java / Kotlin, `nsh.aar`) | Android 9 (API 28)+ |
| macOS | AppKit, Core Animation, CoreText | `AppView` (Rust, `nsh-platform-macos`) | macOS 14+ |
| Windows | Win32, Direct2D, DirectWrite | `AppView` (Rust, `nsh-platform-windows`) | Windows 10 1809+ |
| Linux | GTK 4, Pango | `AppView` (Rust, `nsh-platform-linux`) | GTK 4.14+ |
| GPUI (macOS, Windows, Linux) | GPUI (GPUI Fast) | `AppView` (Rust, a GPUI view, `nsh-platform-gpui`) | macOS, Windows; Linux with Vulkan |
| Web | HTML, CSS | `createPlayer` (JavaScript) or an iframe | Modern browsers |

Each desktop runs and packages apps with its own backend (AppKit, Win32, GTK 4). The GPUI backend draws with GPUI on
all three desktops and embeds NSH in GPUI and GPUI Kit apps; `nsh run --target gpui` shows an app as they do. The web
platform (`nsh run --target web`, `nsh package --target web`) publishes apps as static sites and runs the website's
live demos.

## Quick start

Install the `nsh` command ([other ways](https://nsh.dev/docs/installation)):

<details open>
<summary>macOS / Linux</summary>

```sh
curl -fsSL https://nsh.dev/install.sh | sh
```

</details>

<details>
<summary>Homebrew</summary>

```sh
brew install longbridge/tap/nsh
```

</details>

<details>
<summary>Windows (PowerShell)</summary>

```powershell
irm https://nsh.dev/install.ps1 | iex
```

</details>

Choose how to write your app:

| | Rust | TSX |
|---|---|---|
| Create | `nsh init my-app --template rust` | `nsh init my-app --template ts` |
| Write | Rust components, typed styles and signals | TSX components, Tailwind classes and signals |
| Iterate | Rebuild and restart on Rust changes | Hot updates that preserve state |
| Toolchain | `nsh` and [Rust](https://rustup.rs) | `nsh` |

You can also combine them (`nsh init my-app --template mixed`): TSX screens import your Rust components and modules. See
[Building with Rust](https://nsh.dev/docs/rust) for Rust apps and mixed projects.

Run either app on your chosen platform:

<details>
<summary>iOS</summary>

```sh
nsh run my-app --target ios        # iOS simulator; requires Xcode
nsh run my-app --target catalyst   # Mac Catalyst window on macOS
```

</details>

<details>
<summary>Android</summary>

```sh
nsh run my-app --target android    # emulator or device; requires the Android SDK
```

</details>

<details>
<summary>macOS</summary>

```sh
nsh run my-app --target macos      # AppKit window
```

</details>

<details>
<summary>Windows</summary>

```sh
nsh run my-app --target windows    # Win32 window
```

</details>

<details>
<summary>Linux</summary>

```sh
nsh run my-app --target linux      # GTK 4 window
```

</details>

<details>
<summary>GPUI (macOS, Windows, Linux)</summary>

```sh
nsh run my-app --target gpui       # as GPUI apps that embed it show it
```

</details>

<details>
<summary>Web</summary>

```sh
nsh run my-app --target web        # open in the browser
```

</details>

The Rust template includes a Cargo project with its root component in `src/lib.rs`. The TSX template includes
`App.tsx`, `nsh.json` and `tsconfig.json`, with generated TypeScript declarations in `.nsh/types` for your editor.
Logs and errors show in the terminal. What each platform needs:
[Installation](https://nsh.dev/docs/installation#platform-tools).

## Examples

| Example | What it shows |
|---|---|
| [`examples/gallery`](examples/gallery) | A tour of what NSH supports: lists, navigation, forms, controls, gestures, animation, images, styles, text, web view and video, `fetch` |
| [`examples/counter`](examples/counter) | The smallest app |
| [`examples/hopper`](examples/hopper) | A dark, tabbed dashboard built only from primitives: interactive canvas charts, sliders, menus, animation |
| [`examples/desktop`](examples/desktop) | A desktop app with a title bar and menus of its own (`nsh/desktop`) |
| [`examples/gallery-rs`](examples/gallery-rs) | The gallery written in Rust: an app with no TSX and no QuickJS |
| [`examples/rust-exports`](examples/rust-exports) | A TSX app using a component, a module and a class of its own Rust crate |
| [`examples/embed-ios`](examples/embed-ios), [`examples/embed-android`](examples/embed-android) | NSH inside existing native apps, with a native component the app registers |
| [`examples/bench`](examples/bench) | Rust components against the same components in TSX |

```sh
cargo run
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE-APACHE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as
defined in the Apache-2.0 license, shall be licensed under Apache-2.0, without any additional terms or conditions.

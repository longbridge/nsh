---
title: Platform overview
description: "Where NSH runs: iOS, Android, macOS, Windows, Linux, GPUI, the terminal and the web. How to run, package and embed a TSX or Rust app on each, and what each needs."
order: 10
group: platforms
---

# Platform overview

NSH runs on iOS, Android, macOS, Windows, Linux, GPUI, the terminal and the web. Each platform has its own backend that draws with native UI: UIKit views on iPhone, AppKit views on a Mac, GTK 4 widgets on Linux, GPUI elements in a GPUI app, character Cells in a terminal and HTML in a browser. Text, input, accessibility and dark mode come from the platform.

Everything above the backend is shared Rust code: the reactive graph, the renderer, Taffy layout, the components and the native modules. So one app runs everywhere. A Rust app is that code plus your crate, with no JavaScript VM. A TSX app runs its modules on QuickJS on top ([Building with Rust](./rust.md)).

You can use an app in two ways on every platform:

- Package it as an installable app (`nsh package`).
- Embed it in your own native app. The host links a Rust root or loads TSX at run time.

During development, use `nsh run`. Rust edits rebuild and restart the app. TSX edits hot-update.

## At a glance

| Platform | Backend; text | Runs on | Develop | Package |
|---|---|---|---|---|
| [iOS](./ios.md) | UIKit; CoreText | iOS 15.0+, devices and the simulator | `nsh run --target ios` | `.app` (simulator), signed `.ipa` |
| [Android](./android.md) | Android views; `StaticLayout` | Android 8 (API 26)+, `arm64-v8a`, `armeabi-v7a` and `x86_64` | `nsh run --target android` | signed `.apk` |
| [macOS](./macos.md) | AppKit and Core Animation; CoreText | macOS 14+ | `nsh run --target macos` | ad hoc signed `.app` |
| [Windows](./windows.md) | Win32 windows drawn with Direct2D; DirectWrite | Windows 10 1809+, Windows 11 | `nsh run` (the default on Windows) | a directory, a `.zip` and an installer |
| [Linux](./linux.md) | GTK 4 widgets; Pango | GTK 4.14+ | `nsh run` (the default on Linux) | a directory and a `.tar.gz` |
| [GPUI](./gpui.md) | GPUI; GPUI's text system | macOS, Windows, Linux (Vulkan) | in your GPUI app; TSX also with `nsh run --target gpui` | inside the GPUI app that embeds it |
| [Web](./web.md) | HTML and CSS; the browser's fonts | modern browsers, desktop and phone | `nsh run --target web` | a static site |
| [TUI](./tui.md) | ratatui on the terminal's Cell grid; the terminal's font | terminals on macOS, Windows and Linux | `nsh run --target tui` | inside the terminal app that embeds it |

- **[GPUI](./gpui.md)** draws with [GPUI](https://gpui-kit.com), the framework behind GPUI Kit, on macOS, Windows and Linux. Use it to embed NSH in GPUI and GPUI Kit apps: they place `AppView` in their own element tree. Standalone desktop apps use each OS's own backend (AppKit, Win32, GTK 4).
- **[TUI](./tui.md)** draws on the terminal's grid of character Cells, in the terminal's colors. It is keyboard first and supports the mouse. `nsh run --target tui` runs an app in the current terminal. A ratatui app embeds one with `AppView`.
- **[Web](./web.md)** renders apps as HTML. `nsh package --target web` publishes a static site for any web server or CDN. A Rust app's engine and crate compile to WebAssembly. Your own page embeds an app with `createPlayer` or an iframe. The live demos and the [playground](/playground) on this site use it.

The community builds more platforms on the same engine, each in its own repository. The `nsh` CLI runs them by name. For example, [`nsh-platform-miniprogram`](https://github.com/huacnlee/nsh-platform-miniprogram) runs apps in WeChat Mini Programs (`--target miniprogram`). See [Building a platform](./platform-extensions.md).

**Development tool:** `--target catalyst` runs the iOS backend on a Mac in an iPhone-sized window (390×844). Use it only to check Catalyst-specific behavior or to embed NSH in a Mac Catalyst app. For a Mac app, use the default `macos`. To see the phone layout, use `--target ios`.

## Develop and package

Without `--target`, `nsh run` picks the machine's platform: AppKit on macOS, GTK on Linux, Win32 on Windows. Phones need a computer: iOS needs a Mac with Xcode. Android needs the Android SDK on macOS, Linux or Windows ([Installation](./installation.md#what-works-where)).

A Rust app names its `"root"` component in `nsh.json`. Run and package it from its directory. `nsh` builds a runner for the target with your crate and no QuickJS. A TSX app passes its entry file:

```sh
# A Rust app, from its directory
nsh run --target macos             # or ios, android, windows, linux, tui, web, catalyst
nsh package --target macos         # or ios, ios-simulator, android, windows, linux, web

# A TSX app
nsh run App.tsx --target macos     # or ios, android, windows, linux, gpui, tui, web, catalyst
nsh package App.tsx --target macos
```

Each desktop app is packaged on its own OS: a Mac app on a Mac, a Windows app on Windows, a Linux app on Linux. [Packaging](./packaging.md) covers output, signing and icons for every target.

## Embed in a native app

Step-by-step guides are in [Embedding in an existing app](./embedding.md).

| Platform | Host view | Runs | Ships as | Host language |
|---|---|---|---|---|
| macOS | `AppView` (an `NSView`) | a Rust root, TSX bundles | the `nsh-platform-macos` crate | Rust |
| Windows | `AppView` (a child window) | a Rust root, TSX bundles | the `nsh-platform-windows` crate | Rust |
| Linux | `AppView` (a GTK widget) | a Rust root, TSX bundles | the `nsh-platform-linux` crate | Rust |
| GPUI (macOS, Windows, Linux) | `AppView` (a GPUI view) | a Rust root, TSX bundles, signed packages | the `nsh-platform-gpui` crate | Rust |
| iOS | `AppView` (a `UIView`) | a Rust root, TSX bundles | the `nsh-platform-ios` crate | Rust |
| iOS | `NshAppView` (a `UIView`) | TSX bundles, signed packages | `NativeShell.xcframework` | Swift |
| Android | `NshView` (a `FrameLayout`) | TSX bundles, signed packages | `nsh.aar` | Java or Kotlin |
| Web | `createPlayer` (a page element) or an iframe | TSX bundles; a Rust app as its published site | the runtime files from `nsh package --target web` | JavaScript or HTML |
| TUI | `AppView` (an area of a ratatui frame) | a Rust root, TSX bundles | the `nsh-platform-tui` crate | Rust |

A Rust host mounts a Rust root (`Root::of::<App>()`) as `AppSource::Rust` or `App::Rust`. It passes the app's UI sets to its engines with `set_ui_sets`, so loaded TSX can import the app's own components and modules. Each view is one engine, and an app can show several. On every platform, the host can register its own native views for apps ([Native components](./native-components.md)).

> [!NOTE]
> Rust hosts use the platform crates from the NSH repository. On macOS, Windows and Linux, `AppView` loads bundles from the app's own files (or source code) with the default limits. The GPUI crate and the iOS and Android SDKs also load [signed packages](./signed-packages.md) and set limits and capabilities per view ([Sandbox and capabilities](./sandbox-and-capabilities.md)).

### Share a mobile host's Tokio runtime

A mobile host that links Rust can run NSH's async tasks on its existing Tokio runtime. Each view keeps a clone of the runtime's `Handle`. The app owns the runtime and must keep it running, with IO and time enabled, until its views stop. `None` uses NSH's shared runtime.

On iOS, set the runtime in the `AppView` (or `Surface`) options. Every load of the view uses it:

```rust
let mut options = SurfaceOptions::default();
options.runtime = Some(runtime.handle().clone());
let view = AppView::new(mtm, frame, options);
view.load(bundle);
```

On Android, call `nsh::set_runtime` with the `NshView`'s native handle (its `handle` field):

- Call it on the UI thread, before the view loads an app or connects to a dev server.
- It applies to that view from its next load. A running engine keeps the runtime it started with.

A JNI function in the app's own library reads the handle:

```rust
#[unsafe(no_mangle)]
extern "system" fn Java_com_example_app_RuntimeBridge_configure(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    view: JObject<'_>,
) {
    let Ok(handle) = env.get_field(&view, "handle", "J").and_then(|v| v.j()) else {
        return;
    };
    // SAFETY: called on the UI thread with a live NshView of this library.
    unsafe { nsh::set_runtime(handle, Some(app_runtime().clone())) };
}
```

In minified builds, keep the bridge class names (`-keep class com.example.app.RuntimeBridge { *; }`). The distributed AARs already keep the `com.longbridge.nsh` package, including `NshView.handle`.

## Writing for every platform

Most app code needs no platform checks. Elements, styles, layout and modules behave the same everywhere. Where platforms differ, NSH puts the difference in one place ([Platform-specific code](./platform-specific.md)):

- **Where the app runs**: Rust has `os()`, `idiom()` and `is_desktop()` (`nsh_core::prelude`). TSX has `platform.os`, `platform.idiom`, `platform.desktop` and `platform.select` from `nsh`. The OS is iOS, Android, macOS, Windows, Linux or the web. In a terminal, the idiom is `"terminal"` and the OS is the machine's own.
- **Platform-only props** go in `ios={{…}}`, `android={{…}}`, `macos={{…}}`, `windows={{…}}`, `web={{…}}` or `desktop={{…}}` in TSX. `desktop` covers macOS, Windows, Linux, desktop browsers and terminals.
- **Input follows the platform.** Phones get touch and the software keyboard. Desktops get the mouse, trackpad and keyboard.

Components are drawn with NSH primitives, so they run and look the same on desktops and phones. Build a desktop layout from the primitives (`view`, `text`, `scroll`, `virtual_list`, …; `View`, `Text`, `Scroll`, `List`, … in TSX) plus a desktop check, as the gallery does:

```tsx tabs
import { View } from "nsh";
import { platform } from "nsh";

export function Shell() {
  // macOS, Windows, Linux and desktop browsers get a sidebar; phones get tabs.
  return <View class="flex-1">{platform.desktop ? <SidebarLayout /> : <TabLayout />}</View>;
}

function SidebarLayout() {
  return <View class="flex-1 flex-row" />;
}

function TabLayout() {
  return <View class="flex-1 flex-col" />;
}
```

```rust tabs
use nsh_core::prelude::*;

#[component]
pub fn Shell() -> impl IntoElement {
    // macOS, Windows, Linux and desktop browsers get a sidebar; phones get tabs.
    view()
        .flex_1()
        .child(if is_desktop() { sidebar_layout() } else { tab_layout() })
}

fn sidebar_layout() -> AnyElement {
    view().flex_1().flex_row().into_element()
}

fn tab_layout() -> AnyElement {
    view().flex_1().flex_col().into_element()
}
```

Each backend lists what it does differently: [macOS](./macos.md#what-differs-on-macos), [Windows](./windows.md#what-differs-on-windows), [Linux](./linux.md#what-differs-on-linux), [GPUI](./gpui.md#differences), [TUI](./tui.md).

## See also

- [Getting started](./getting-started.md): a first app, run on each platform.
- [Building with Rust](./rust.md): apps in Rust, and Rust with TSX.
- [Installation](./installation.md): the tools each platform needs.
- [Architecture](./architecture.md): what is shared and what each backend does.
- [Building a platform](./platform-extensions.md): your own platform on NSH's engine.

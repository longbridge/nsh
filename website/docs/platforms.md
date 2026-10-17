---
title: Platform overview
description: "Where nsh runs: iOS, Android, macOS, Windows and Linux, each with its own native backend; how to run, package and embed on each, what each needs, and the web build of this site's demos."
order: 10
group: platforms
---

# Platform overview

nsh runs on phones and desktops: iOS, Android, macOS, Windows and Linux. Each has a backend of its own that draws with the platform's native UI, so a module is UIKit views on an iPhone, AppKit views on a Mac and GTK 4 widgets on Linux, with the platform's text, input, accessibility and dark mode. Everything above the backend is shared: QuickJS, the reactive graph, the renderer, Taffy layout and the components are the same Rust code everywhere, so one bundle runs on all of them.

On every platform, nsh's main use is the same: modules embedded in a native app of your own, which loads them at run time. You develop them with hot reload (`nsh run`), and the same modules can also be packaged as an installable app (`nsh package`), a supported second use.

## At a glance

| Platform | Backend; text | Runs on | Develop | Package |
|---|---|---|---|---|
| [iOS](./embedding-ios.md) | UIKit; CoreText | iOS 15.0+, devices and the simulator | `nsh run --target ios` | `.app` (simulator), signed `.ipa` |
| [Android](./embedding-android.md) | Android views; `StaticLayout` | Android 9 (API 28)+, `arm64-v8a` and `x86_64` | `nsh run --target android` | signed `.apk` |
| [macOS](./macos.md) | AppKit and Core Animation; CoreText | macOS 14+ | `nsh run --target macos` | ad hoc signed `.app` |
| [Windows](./windows.md) | Win32 windows drawn with Direct2D; DirectWrite | Windows 10 1809+, Windows 11 | `nsh run` (the default on Windows) | a directory and a `.zip` |
| [Linux](./linux.md) | GTK 4 widgets; Pango | GTK 4.14+ | `nsh run` (the default on Linux) | a directory and a `.tar.gz` |

Two more targets exist for development and for this site, not for shipping, and one is planned:

- **Mac Catalyst** runs the iOS backend in an iPhone-sized window (390×844) on a Mac. It is `nsh run`'s default target on macOS, because it is the quickest way to see how a module looks on a phone, without booting a simulator.
- **Web** ([WebAssembly](./web.md)) runs the same engine in a browser and draws with the DOM. It powers the live demos and the [playground](/playground) on this site; it is not a target to ship apps to.
- **GPUI** is a planned desktop backend drawn with [GPUI](https://gpui-kit.com), the framework of the sibling project GPUI Kit. It is in development and not part of nsh yet; the native macOS, Windows and Linux backends above are what desktops run today.

## Develop and package

`nsh run` picks the machine's own platform when you give no `--target`: Mac Catalyst on macOS, the GTK window on Linux and the Win32 window on Windows. Phones are reached from a computer: iOS from a Mac with Xcode, Android from macOS, Linux or Windows with the Android SDK ([Installation](./installation.md#what-works-where)).

```sh
nsh run my-app --target macos      # or ios, android, windows, linux, catalyst
nsh package my-app --target macos  # or ios, ios-simulator, android, windows, linux
```

Desktop backends build for the machine they run on: package a Mac app on a Mac, a Windows app on Windows and a Linux app on Linux. [Packaging](./packaging.md) covers every target's output, signing and icons.

## Embed in a native app

| Platform | Host view | Ships as | Host language |
|---|---|---|---|
| iOS | `NshAppView` (a `UIView`) | `NativeShell.xcframework` | Swift |
| Android | `NshView` (a `FrameLayout`) | `nsh.aar` | Java or Kotlin |
| macOS | `AppView` (an `NSView`) | the `nsh-platform-macos` crate | Rust |
| Windows | `AppView` (a child window) | the `nsh-platform-windows` crate | Rust |
| Linux | `AppView` (a GTK widget) | the `nsh-platform-linux` crate | Rust |

Each view is one engine with its own VM, and an app can show several. The host registers its own native views for modules to use on every platform ([Native components](./native-components.md)).

> [!NOTE]
> The desktop hosts are younger than the mobile SDKs. Their `AppView` loads bundles from the app's own files (or source code), with the default limits; loading [signed packages](./signed-packages.md) with `NshTrust`, and setting limits and capabilities per view ([Sandbox and capabilities](./sandbox-and-capabilities.md)), are part of the iOS and Android SDKs only so far. Desktop hosts are Rust crates from the nsh repository; there is no prebuilt desktop SDK yet.

## Writing for every platform

Most of an app needs no platform checks: elements, styles, layout and modules behave the same everywhere. Where platforms differ, nsh gives the difference one place ([Platform-specific code](./platform-specific.md)):

- **Platform-only props** go in `ios={{…}}`, `android={{…}}`, `web={{…}}` or `desktop={{…}}` (macOS, Windows, Linux and desktop browsers).
- **`Platform.select`** takes a value per OS, or for `mobile` and `desktop` as a whole; `Platform.OS` is `"ios"`, `"android"`, `"macos"`, `"windows"`, `"linux"` or `"web"`.
- **Input follows the platform.** Phones get touches and the software keyboard; desktops get the mouse, the trackpad and the keyboard.

The components of [`nsh/ui/mobile`](../components/index.md) are drawn by nsh, so they also run on desktops, looking as they do on phones. There is no desktop UI set yet: a desktop layout is built from the primitives (`View`, `Text`, `Scroll`, `List`, …) and `Platform.select({ desktop: … })`, as the gallery's desktop layout is.

Each backend lists what it does not support yet: [macOS](./macos.md#what-differs-on-macos), [Windows](./windows.md#what-differs-on-windows), [Linux](./linux.md#what-differs-on-linux).

## See also

- [Getting started](./getting-started.md): a first module, run on each platform.
- [Installation](./installation.md): the tools each platform needs.
- [Architecture](./architecture.md): what is shared and what each backend does.

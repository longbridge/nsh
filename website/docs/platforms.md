---
title: Platform overview
description: "Where NSH runs: iOS, Android, macOS, Windows, Linux, GPUI and the web, each with its own backend; how to run, package and embed on each, and what each needs."
order: 10
group: platforms
---

# Platform overview

NSH runs on phones, desktops and the web: iOS, Android, macOS, Windows, Linux, GPUI and web pages. Each has a backend of its own that draws with the platform's own UI, so a module is UIKit views on an iPhone, AppKit views on a Mac, GTK 4 widgets on Linux, GPUI elements in a GPUI app and HTML in a browser, with the platform's text, input, accessibility and dark mode. Everything above the backend is shared: QuickJS, the reactive graph, the renderer, Taffy layout and the components are the same Rust code everywhere, so one bundle runs on all of them.

On every platform, NSH's main use is the same: modules embedded in a native app of your own, which loads them at run time. You develop them with hot reload (`nsh run`), and the same modules can also be packaged as an installable app (`nsh package`), a supported second use.

## At a glance

| Platform | Backend; text | Runs on | Develop | Package |
|---|---|---|---|---|
| [iOS](./embedding-ios.md) | UIKit; CoreText | iOS 15.0+, devices and the simulator | `nsh run --target ios` | `.app` (simulator), signed `.ipa` |
| [Android](./embedding-android.md) | Android views; `StaticLayout` | Android 9 (API 28)+, `arm64-v8a` and `x86_64` | `nsh run --target android` | signed `.apk` |
| [macOS](./macos.md) | AppKit and Core Animation; CoreText | macOS 14+ | `nsh run --target macos` | ad hoc signed `.app` |
| [Windows](./windows.md) | Win32 windows drawn with Direct2D; DirectWrite | Windows 10 1809+, Windows 11 | `nsh run` (the default on Windows) | a directory and a `.zip` |
| [Linux](./linux.md) | GTK 4 widgets; Pango | GTK 4.14+ | `nsh run` (the default on Linux) | a directory and a `.tar.gz` |
| [GPUI](./gpui.md) | GPUI; GPUI's text system | macOS, Windows, Linux (Vulkan) | `nsh run --target gpui` | as the desktop it runs on |
| [Web](./web.md) | HTML and CSS; the browser's fonts | modern browsers, desktop and phone | `nsh run --target web` | a static site |

- **[GPUI](./gpui.md)** draws modules with [GPUI](https://gpui-kit.com), the framework of the sibling project GPUI Kit, on macOS, Windows and Linux alike: one renderer for all three desktops, and an `AppView` that GPUI and GPUI Kit apps place in their own element tree.
- **[Web](./web.md)** renders apps as HTML in a browser page: `nsh package --target web` publishes a static site for any web server or CDN, and a page of your own embeds an app with `createPlayer` or an iframe. It also runs the live demos and the [playground](/playground) on this site.
- **Mac Catalyst** runs the iOS backend in an iPhone-sized window (390×844) on a Mac, for development. It is `nsh run`'s default target on macOS: the quickest way to see how a module looks on a phone.

## Develop and package

`nsh run` picks the machine's own platform when you give no `--target`: Mac Catalyst on macOS, the GTK window on Linux and the Win32 window on Windows. Phones are reached from a computer: iOS from a Mac with Xcode, Android from macOS, Linux or Windows with the Android SDK ([Installation](./installation.md#what-works-where)).

```sh
nsh run my-app --target macos      # or ios, android, windows, linux, gpui, web, catalyst
nsh package my-app --target macos  # or ios, ios-simulator, android, windows, linux, gpui, web
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
| GPUI (macOS, Windows, Linux) | `AppView` (a GPUI view) | the `nsh-platform-gpui` crate | Rust |
| Web | `createPlayer` (a page element) or an iframe | the runtime files from `nsh package --target web` | JavaScript or HTML |

Each view is one engine with its own VM, and an app can show several. The host registers its own native views for modules to use on every platform ([Native components](./native-components.md)).

> [!NOTE]
> Desktop hosts embed NSH with Rust crates from the NSH repository; their `AppView` loads bundles from the app's own files (or source code), with the default limits. The iOS and Android SDKs also load [signed packages](./signed-packages.md) with `NshTrust` and set limits and capabilities per view ([Sandbox and capabilities](./sandbox-and-capabilities.md)).

## Writing for every platform

Most of an app needs no platform checks: elements, styles, layout and modules behave the same everywhere. Where platforms differ, NSH gives the difference one place ([Platform-specific code](./platform-specific.md)):

- **Platform-only props** go in `ios={{…}}`, `android={{…}}`, `web={{…}}` or `desktop={{…}}` (macOS, Windows, Linux and desktop browsers).
- **`Platform.select`** takes a value per OS, or for `mobile` and `desktop` as a whole; `Platform.OS` is `"ios"`, `"android"`, `"macos"`, `"windows"`, `"linux"` or `"web"`.
- **Input follows the platform.** Phones get touches and the software keyboard; desktops get the mouse, the trackpad and the keyboard.

The components of [`nsh/ui/mobile`](../components/index.md) are drawn by NSH, so they also run on desktops, looking as they do on phones. A desktop layout is built from the primitives (`View`, `Text`, `Scroll`, `List`, …) and `Platform.select({ desktop: … })`, as the gallery's desktop layout is; a desktop UI set is on the way.

Each backend lists what it does not support yet: [macOS](./macos.md#what-differs-on-macos), [Windows](./windows.md#what-differs-on-windows), [Linux](./linux.md#what-differs-on-linux), [GPUI](./gpui.md#differences).

## See also

- [Getting started](./getting-started.md): a first module, run on each platform.
- [Installation](./installation.md): the tools each platform needs.
- [Architecture](./architecture.md): what is shared and what each backend does.

---
title: Introduction
description: "Native Shell is a Rust-first UI framework for iOS, Android, macOS, Windows, Linux and the web. Each platform gets its own UI and interaction. You write apps in TSX and add Rust where needed."
order: 1
group: start
---

# Introduction

Native Shell (NSH) is a UI framework for **iOS, Android, macOS, Windows, Linux and the web**. You write one app, and on each platform it runs as a native app: built from that platform's views, with its text system and its interaction.

You write the app in **TSX**. Where it needs native speed, system access or a crate, you add **Rust** through [nshm](./nshm.md). You can also write the whole app in Rust. Both run on the same engine.

Apps you have already shipped can also load new UI at run time. Your team writes it, or a model generates it. It arrives as a signed module and runs in a sandbox that the host app configures.

> [!IMPORTANT]
> NSH provides native views and interaction. The app owns its visual design: colors, typography, spacing and shapes. Components, state and APIs are shared across platforms. Text input, focus and menus follow each device's conventions. See [Goals and design principles](./goals.md).

## What native means

Native in NSH has two layers: the platform's foundations and the platform's interaction. Your app still defines its own look. On supported platforms it can also use system materials such as Liquid Glass.

**The platform's foundations.** Each platform has its own backend, built on that platform's UI:

| Platform | Views | Text | Text input |
|---|---|---|---|
| iOS | UIKit | CoreText | The system's text fields and text views |
| Android | Android views | `StaticLayout` | `EditText` |
| macOS | AppKit, Core Animation | CoreText | `NSTextField`, `NSTextView` |
| Windows | Win32 windows drawn with Direct2D | DirectWrite | NSH's editor, with the system's input methods (IMM32) |
| Linux | GTK 4 | Pango | GTK's `Text` and `TextView` |
| Web | HTML elements styled with CSS | The browser's fonts | The browser's inputs |

Text selection and its menu, input methods, accessibility and scrolling come from the platform. When a new OS release improves them, your app gets the improvement too. Native components, such as a map, a web view, a video player or any view the host registers, sit in the same view hierarchy. They are laid out, clipped and scrolled like any other view ([Native components](./native-components.md)).

**The platform's interaction.** The components in [`nsh/base`](../modules/nsh-base.md) behave the way each device's users expect. You write them once:

| | Desktops (and desktop browsers) | Phones and tablets |
|---|---|---|
| `Menu` | A popup at its trigger; submenus cascade beside their item | A sheet from the bottom; a submenu replaces its menu's items |
| `ContextMenu` | A right click opens it at the pointer | A finger held on the trigger opens it as a sheet |
| `Tooltip` | Shown when the pointer rests on the trigger | Shown when the app opens it |
| `Popover` | A press outside closes it and reaches what is under the pointer | A press outside closes it |

Pages, dialogs, the keyboard and the status bar work the same way. Navigation stacks push and pop with each platform's motion. The back button, Escape and swipe-back do what users expect there ([Navigation](./navigation.md)).

## From phone to desktop to web

NSH does not scale one screen up or down. Each device gets the app it expects:

- **One app model.** Components, state, business logic and screens are shared. Each backend maps them to its platform.
- **Layouts that reshape.** [Taffy](https://github.com/DioxusLabs/taffy) lays out flexbox and CSS grid on every platform. Tailwind breakpoints (`md:`, `lg:`) and the window size (`window()` in Rust) change the structure, not only the size. A chat app shows sidebar, conversations and messages side by side in a wide window, and one screen at a time on a phone ([Layout](./layout.md), [Tailwind](./tailwind.md)).
- **The right input per device.** Desktops get pointer, hover, right click, shortcuts and focus. Phones get touch, gestures, the software keyboard and page navigation. For per-platform behavior, use `desktop={{…}}` props and `platform.select` ([Platform-specific code](./platform-specific.md)).
- **Tested on every target.** `nsh test` runs your tests headless. It also runs them with real platform input in macOS, Windows, Linux and GPUI windows, an iOS simulator, an Android device or a browser ([Testing and tracing](./testing-and-trace.md)).

For example, take a chat client. You build it on the desktop with `nsh run`. You check the phone layout with `nsh run --target ios`, and the web version with `nsh run --target web`. It is the same project each time. `nsh package` then builds the installable apps and the website. One engineer can do work that used to need a team per platform.

## Rust first

The engine, layout, rendering and every platform layer are written in Rust and compiled to native code. You write apps on top in TSX, and use Rust where it pays off:

- **Apps in TSX.** Screens, state, logic and styles are TSX, with Tailwind classes and signals. Each save hot-updates the app and keeps its state. Only your own code is JavaScript. The reactive graph, layout, rendering and every imported module are Rust ([JSX and components](./jsx-and-components.md)). `nsh init` starts here.
- **Rust where you need it.** Put your own crate in `ext/`, or use an [nshm](./nshm.md) from crates.io. It gives your TSX components, modules and classes. Use it for heavy computation, parsing, large data, system APIs and crates. It runs at native speed even when TSX calls it.
- **Whole apps in Rust.** If the core of your app is demanding business logic, write all of it in Rust. You get Rust's ownership, types and threads, and no JavaScript runtime ([Building with Rust](./rust.md)).
- **One toolchain.** The `nsh` executable creates, builds, type-checks, runs with hot reload, tests, packages and signs apps for every target. It includes a TSX bundler and a TypeScript checker, so a TSX app needs only `nsh`. You need the Rust toolchain only when your app has its own Rust code ([Installation](./installation.md), [CLI](./cli.md)).

## An app

The same counter, in TSX and in Rust:

```tsx tabs
import { mount, signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";

function Counter() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <Text style={{ fontSize: 48, fontWeight: "700" }}>{count()}</Text>
      <View
        onPress={() => count.update((n) => n + 1)}
        activeStyle={{ opacity: 0.6 }}
        class="rounded-lg bg-blue-600 px-5 py-3"
      >
        <Text class="text-white text-lg">Tap</Text>
      </View>
    </View>
  );
}

mount(() => <Window id="main"><Counter /></Window>);
```

```rust tabs
use nsh_core::prelude::*;

#[component]
pub fn Counter() -> impl IntoElement {
    let count = signal(0);
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_3()
        .child(text(move || count.get().to_string()).text_5xl().font_weight(FontWeight::BOLD))
        .child(
            view()
                .rounded_lg()
                .bg(rgb(0x2563eb))
                .px_5()
                .py_3()
                .active(|s| s.opacity(0.6))
                .on_press(move || count.update(|n| *n += 1))
                .child(text("Tap").text_lg().text_color(white())),
        )
}
```

Each view (`view()`, `<View>`) becomes a real platform view:

| Platform | View |
|---|---|
| iOS | `UIView` |
| Android | `ViewGroup` |
| macOS | `NSView` |
| Linux | GTK widget |
| Windows | Retained view drawn with Direct2D |
| Web | HTML element |

Text becomes a native text view. The press feedback (`.active(..)`, `activeStyle`) runs on the main thread. A tap calls `count.update`, which changes one signal. Only the text that reads it updates. The component function does not run again. In the TSX app, only your module's code is JavaScript. The Rust app is native code throughout.

## Retained views

NSH is retained. Every element is a view the platform keeps, and an update changes only what changed.

- **Fine-grained updates.** [Signals](./reactivity.md) work like Solid's, implemented natively. Components run once. A change re-runs only the expressions that read the changed signal.
- **Only changes cross threads.** NSH lays out one turn's changes together. It sends them to the main thread as one batch of view operations per frame. Unchanged styles and frames are not sent, and untouched views stay as they are.
- **Native animations.** Timing, spring and decay animations, gestures and scroll-linked values run on the platform's frame clock. They don't call into JS each frame ([Animation](./animation.md)).

This fits the screens most apps have: chats, mail, forms, lists, documents, settings, a streaming AI reply. A few views change at a time. Between updates the app sits near idle, and scrolling and animation hold 120 FPS on ProMotion displays.

In the [NSH performance benchmark](https://github.com/longbridge/nsh-perf-bench), a 500-row quote table updates 1,000 times a second. On Linux, NSH Rust uses 18% of a core. The same screen written directly on [GPUI Fast](https://github.com/longbridge/gpui-fast) uses 20%.

Put heavy computation in Rust. TSX runs in QuickJS, an interpreter. A Rust module runs at native speed even when TSX calls it: sorting 100,000 rows takes about 3 ms, the same as plain Rust ([Performance](./performance.md)).

## How it runs

The toolchain is one `nsh` binary. You write an app in TSX, Rust or both. Each path ends at the same Rust engine.

### TSX apps

A TSX app (a module) is a set of `.tsx` / `.ts` files with an entry, usually `App.tsx`:

1. **Build.** `nsh` bundles the files with rolldown and Oxc, and compiles JSX with its own compiler. Imports of `nsh` and `nsh/*` stay as imports, because the runtime provides them natively. The bundle holds only your code, compiled to QuickJS bytecode.
2. **Run.** The engine thread runs the bytecode in [QuickJS](https://github.com/quickjs-ng/quickjs), one VM per view. Compiled JSX calls the native renderer directly. JavaScript only describes the UI and handles events. Rust parses styles, resolves Tailwind classes, diffs, lays out and shapes text.

### Rust apps

A Rust app is a crate with no JavaScript runtime:

1. **Build.** `nsh` builds the runner for each target (iOS, Android, macOS, Windows, Linux or web) with your crate linked in. Components, state, handlers and the engine compile together into native code.
2. **Run.** The root component runs as Rust on the engine thread. The runner contains no JavaScript engine. QuickJS is not part of a Rust app on any platform.

### The difference

| | TSX app | Rust app |
|---|---|---|
| Written in | TSX (TypeScript and JSX) | Rust |
| Built into | QuickJS bytecode, loaded by the runtime | Native code, linked into each target's runner |
| JavaScript engine | QuickJS, one VM per view | None |
| While developing (`nsh run`) | Hot updates on each save | Rebuilt and restarted on each save |
| After release | Ships with the app; also delivered at run time as signed packages, in a sandbox | Ships with the app |
| Suits | Most apps, plugins, UI delivered at run time or generated by a model | Apps whose core is heavy computation or system work |

You can combine them. A TSX app imports Rust components, modules and classes from nshm, either its own in `ext/` or published ones. They run as native code ([nshm](./nshm.md)). A Rust app can host TSX screens the same way ([Building with Rust](./rust.md)).

### The shared engine

From the engine on, both paths are the same:

1. NSH collects each turn's changes.
2. [Taffy](https://github.com/DioxusLabs/taffy) lays them out (flexbox and CSS grid).
3. The main thread receives one batch of view operations per frame and applies it to native views.

The engine thread measures and lays out text once. It uses CoreText on Apple platforms, `StaticLayout` on Android, DirectWrite on Windows, Pango on Linux and the browser's font metrics on the web. The main thread only draws it.

On the web, the same engine runs in the page. The browser shows, scrolls and selects HTML elements styled with CSS, and handles input. `nsh package --target web` publishes the app as a static site with content-named files, ready for any CDN ([Web](./web.md)).

> [!NOTE]
> The main thread never waits for your code. The system sometimes asks synchronous questions: may this input take another character, may the user swipe back, may this sheet close. NSH answers them natively from props you set beforehand, such as `maxLength`, `popOnBack` or `closeOnSwipe`. Your event handlers run afterwards.

## Dynamic UI and AI-generated UI

NSH started as a way for native apps to load UI after release. That is still a main use. Your app embeds a view where a module should appear, and NSH runs TSX modules inside it:

| Platform | Embed with |
|---|---|
| iOS | `NshAppView` |
| Android | `NshView` |
| macOS, Windows, Linux, GPUI apps | `AppView` |
| Web page | `createPlayer` |

- **Delivered after release.** A module is a signed package (`.nshpkg`) that the app downloads at run time. Before running it, the app checks the signature against its trusted keys and a revocation list ([Signed packages](./signed-packages.md)). The host app ships the runtime. Teams build modules against its API and ship them on their own schedule.
- **The host stays in control.** Each module runs in its own QuickJS VM, with limits on memory, call time and node count. It reaches only what the host grants: network hosts, web pages, the status bar ([Sandbox and capabilities](./sandbox-and-capabilities.md)). Several modules can run side by side.
- **Modules match the app.** Modules can reuse the app's component styles. The host can also register its own native views for modules, such as a map, a player or a design-system component ([Native components](./native-components.md)).

The same mechanism lets an app **answer with an interface**. A model writes a small TSX module, such as a form, a chart or a booking card. Your server checks, builds and signs it. The app shows it natively:

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/ai-flow-light.svg" alt="From a prompt to native UI: a model writes a TSX module; your server builds, checks and signs it; across the trust boundary, the app verifies the package, runs it in a sandbox with the limits and capabilities it set, and shows it with the platform's own UI.">
<img class="diagram diagram-dark" src="/diagrams/ai-flow-dark.svg" alt="From a prompt to native UI: a model writes a TSX module; your server builds, checks and signs it; across the trust boundary, the app verifies the package, runs it in a sandbox with the limits and capabilities it set, and shows it with the platform's own UI.">
</div>

On iOS, the host needs three lines:

```swift
let app = NshAppView()                         // a UIView: place it anywhere
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
let info = try await app.loadPackage(at: downloadedFile, trust: trust)   // verified, then run
```

## AI-friendly

NSH is designed for software written with AI, from whole apps to generated screens:

- **One app model for every platform.** A model learns one way to write UI, and the result runs on phones, desktops and the web. NSH adapts the interaction per platform, so nothing is generated six times.
- **TSX, with Rust where it pays.** Use TSX for apps, screens, plugins and per-request UI. Use Rust for performance-critical modules or whole apps. Both run on one engine.
- **A precise, generated contract.** Every element, prop, style key and module has TypeScript declarations generated from the Rust sources (`.nsh/types`). Rust APIs are typed too. A compiler checks all generated code. These docs are also served as plain text for models ([`llms.txt`](/llms.txt), [`llms-full.txt`](/llms-full.txt)). `npx skills add longbridge/nsh` installs the Coding and Design Guides as skills for coding agents.
- **One command line for the whole loop.** Through `nsh`, an agent creates, builds, runs, tests, fixes and packages the app. `nsh test` runs headless, without a device, and reports errors and the resulting views. The compiler also runs in-process (`nsh_bundler::compile_module`) and in the browser ([Playground](/playground)).
- **Safe to run untrusted code.** Generated code delivered at run time is untrusted. It arrives signed, runs in a VM with limits and reaches only the capabilities the host grants.

## Where it runs

| Platform | Requires | Backend | Embedded with |
|---|---|---|---|
| [iOS](./ios.md) | iOS 15.0+ | UIKit | `NativeShell.xcframework` |
| [Android](./android.md) | Android 8+ | Android views | `nsh.aar` |
| [macOS](./macos.md) | macOS 14+ | AppKit | `AppView` |
| [Windows](./windows.md) | Windows 10 1809+ | Win32, Direct2D | `AppView` |
| [Linux](./linux.md) | GTK 4.14+ | GTK 4 | `AppView` |
| [Web](./web.md) | Modern browsers | HTML | `createPlayer`, iframe |
| [TUI](./tui.md) | Terminals on macOS, Windows and Linux | ratatui | `AppView` (ratatui area) |
| [GPUI](./gpui.md) | Desktops | GPUI | `AppView` (GPUI, GPUI Kit apps) |

For development, Mac Catalyst (`--target catalyst`) runs the iOS backend on a Mac for Catalyst-specific work.

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/platforms-light.svg" alt="One module, many hosts: the same bundle and Rust runtime on iOS, Android, Linux, macOS with AppKit and Windows with Win32 / Direct2D, GPUI on all three desktops, and web pages drawn with HTML.">
<img class="diagram diagram-dark" src="/diagrams/platforms-dark.svg" alt="One module, many hosts: the same bundle and Rust runtime on iOS, Android, Linux, macOS with AppKit and Windows with Win32 / Direct2D, GPUI on all three desktops, and web pages drawn with HTML.">
</div>

## Native Shell and GPUI Shell

[Longbridge](https://longbridge.com) builds NSH for its own clients. It started as a plugin layer for the mobile client, based on [GPUI Shell](https://gpui-kit.com/shell). GPUI Shell lets [GPUI](https://gpui-kit.com) desktop apps be extended in JavaScript.

On phones, drawing the UI with GPUI would mean rebuilding what the platform already does: text selection and its menu (Copy, Share, Translate, Look Up), input methods, accessibility, and the visual materials each iOS release adds. Every mobile team would also have to learn a new UI stack. So NSH renders with each platform's own UI. It then grew from a plugin layer into a framework for whole apps on every platform.

The two projects have different goals. GPUI Shell extends GPUI desktop apps. Native Shell builds native apps for phones, desktops and the web. Its GPUI backend can embed NSH apps in GPUI and GPUI Kit apps ([GPUI](./gpui.md)).

## How it compares

For whole apps, NSH competes with cross-platform frameworks such as React Native, Flutter, Qt, GPUI, Slint, Electron and Tauri. For UI delivered after release, it competes with web views, server-driven UI and mini-program containers. NSH uses each platform's own UI and interaction under one app model. You write in TSX on a Rust engine, and use Rust where you need it. [Comparing approaches](./comparison.md) covers each one.

## Next steps

- [Installation](./installation.md) and [Getting started](./getting-started.md): install `nsh`, write your first app and run it on each platform.
- [nshm](./nshm.md): extend a TSX app with Rust components, modules and classes, its own or published ones.
- [Building with Rust](./rust.md): apps written in Rust from start to end.
- [Platforms](./platforms.md): what runs where, and how to run, package and embed on each platform.
- [JSX and components](./jsx-and-components.md) and [Reactivity](./reactivity.md): the two core ideas, in TSX and Rust.
- [Signed packages](./signed-packages.md) and [Sandbox and capabilities](./sandbox-and-capabilities.md): deliver modules and decide what they may do.

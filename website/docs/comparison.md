---
title: Comparing approaches
description: "For Electron, React Native, Flutter and GPUI teams: where Native Shell comes from, how it compares in capabilities and measured numbers, what moving gains and costs, how to run a pilot, and how it compares with web views and server-driven UI for UI delivered after release."
order: 2
group: start
---

# Comparing approaches

NSH is a framework for whole apps on iOS, Android, macOS, Windows, Linux and the web, and a runtime for UI delivered to apps after release. This page compares it first with the cross-platform frameworks people write apps in, then with the ways apps take UI they did not ship with. Green means the capability is available, yellow that it needs more of the app's own work or a separate project, and red that it is not provided; the marks describe availability, not performance. Versions are each framework's documented minimums at the time of writing: check them for the release you use.

This page is written for teams on Electron, React Native, Flutter or GPUI: where NSH comes from, what each cell of the comparison rests on, what moving to NSH gains and costs, and when another option fits better.

## Where NSH comes from

[Longbridge](https://longbridge.com) needed a desktop client first. To get away from what Electron costs on the desktop — high memory, slow start, unsteady frame rates, a loose fit with the system — Longbridge built [Longbridge Pro](https://longbridge.com/desktop) on [GPUI](https://gpui-kit.com), the Rust GPU UI framework behind the [Zed](https://zed.dev) editor, and grew the [GPUI Kit](https://gpui-kit.com) components and the [GPUI Fast](https://github.com/longbridge/gpui-fast) fork on top of it. A trading client is demanding: high-frequency quotes, order books, candlesticks and linked charts stay open for hours without dropping frames or holding the CPU, and the desktop needs dependable windows, text input and system integration.

NSH started as a plugin layer for the mobile clients (the idea of [GPUI Shell](https://gpui-kit.com/shell), taken to phones) and then carried the same app model — components, signals, layout, modules — to iOS, Android, macOS, Windows, Linux and the web ([Introduction](./index.md#native-shell-and-gpui-shell)). So:

- **The desktop is not an add-on.** macOS (AppKit), Windows (Win32 with Direct2D) and Linux (GTK 4) are backends of the same framework as phones and the web, built from one repository, tested by one suite and shipped in one release.
- **Performance is a constraint of the architecture, not a later pass.** The renderer, layout, animation and list virtualization run in Rust; Rust apps link no JavaScript engine, and in TSX apps the main thread never waits on JS ([Architecture](./architecture.md)).
- **Delivered UI was required to be safe from day one.** A plugin layer runs code someone else wrote, so signed packages, revocation lists, a sandboxed virtual machine and capability grants were design goals from the start ([Signed packages](./signed-packages.md), [Sandbox and capabilities](./sandbox-and-capabilities.md)).

Legend: <span class="status status--yes" role="img" aria-label="Yes"></span> Yes · <span class="status status--partial" role="img" aria-label="Partial"></span> Partial · <span class="status status--no" role="img" aria-label="No"></span> No.

## Cross-platform frameworks

| Capability | NSH | Electron | React Native | Flutter | Qt 6 | GPUI / GPUI Fast | Slint |
|---|---|---|---|---|---|---|---|
| **Platforms** | | | | | | | |
| iOS | <span class="status status--yes" role="img" aria-label="Yes"></span> 15+ | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> 15.1+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 15+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 17+ | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Rust |
| Android | <span class="status status--yes" role="img" aria-label="Yes"></span> 9+ | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> 7+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 7+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 9+ | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> |
| macOS | <span class="status status--yes" role="img" aria-label="Yes"></span> 14+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 12+ | <span class="status status--partial" role="img" aria-label="Partial"></span> Separate project | <span class="status status--yes" role="img" aria-label="Yes"></span> 12+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 13+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 12+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 13+ |
| Windows | <span class="status status--yes" role="img" aria-label="Yes"></span> 10 1809+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 10+ | <span class="status status--partial" role="img" aria-label="Partial"></span> Separate project | <span class="status status--yes" role="img" aria-label="Yes"></span> 10+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 10 1809+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 10+ | <span class="status status--yes" role="img" aria-label="Yes"></span> 10+ |
| Linux | <span class="status status--yes" role="img" aria-label="Yes"></span> GTK 4.14+ | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> Vulkan | <span class="status status--yes" role="img" aria-label="Yes"></span> |
| Web | <span class="status status--yes" role="img" aria-label="Yes"></span> HTML | <span class="status status--partial" role="img" aria-label="Partial"></span> Separate web app | <span class="status status--partial" role="img" aria-label="Partial"></span> Library | <span class="status status--yes" role="img" aria-label="Yes"></span> Canvas | <span class="status status--yes" role="img" aria-label="Yes"></span> Canvas | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> Canvas |
| Terminal (TUI) | <span class="status status--yes" role="img" aria-label="Yes"></span> ratatui | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> |
| **Rendering and runtime** | | | | | | | |
| Drawn by | Platform UI | Chromium | Platform views | Own engine | Own (Widgets, Quick) | Own, GPU | Own renderer |
| Update model | Fine-grained signals, retained views | Web framework on the DOM | React re-render and diff | Widget rebuild and diff | Property bindings | Rebuilt every frame; Fast reuses unchanged views | Property bindings |
| Where layout, animation, lists run | Rust | JS and Chromium | Yoga (C++), list logic in JS | Dart | C++ | Rust | Rust |
| App language | Rust, TSX | JS, TS | JS, TS | Dart | C++, QML, Python | Rust | Slint + Rust, C++, JS, Python |
| No JS runtime | <span class="status status--yes" role="img" aria-label="Yes"></span> Rust apps | <span class="status status--no" role="img" aria-label="No"></span> Node + Chromium | <span class="status status--no" role="img" aria-label="No"></span> Hermes | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> C++ Widgets | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> |
| **Text, input and accessibility** | | | | | | | |
| System text editing | <span class="status status--yes" role="img" aria-label="Yes"></span> Platform editors | <span class="status status--partial" role="img" aria-label="Partial"></span> Chromium's | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Own | <span class="status status--partial" role="img" aria-label="Partial"></span> Own | <span class="status status--partial" role="img" aria-label="Partial"></span> Own | <span class="status status--partial" role="img" aria-label="Partial"></span> Own |
| System accessibility tree | <span class="status status--yes" role="img" aria-label="Yes"></span> Native views | <span class="status status--yes" role="img" aria-label="Yes"></span> Chromium | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> Semantics tree | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Limited | <span class="status status--partial" role="img" aria-label="Partial"></span> |
| Per-device interaction | <span class="status status--yes" role="img" aria-label="Yes"></span> Built into base components | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> |
| **Desktop and embedding** | | | | | | | |
| Windows, menus, tray | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Separate projects | <span class="status status--partial" role="img" aria-label="Partial"></span> Plugins | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> No tray | <span class="status status--partial" role="img" aria-label="Partial"></span> |
| Embed in an existing native app | <span class="status status--yes" role="img" aria-label="Yes"></span> One view | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> Brownfield | <span class="status status--yes" role="img" aria-label="Yes"></span> Add-to-app | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> |
| Embed in a GPUI app | <span class="status status--yes" role="img" aria-label="Yes"></span> GPUI backend | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> Native | <span class="status status--no" role="img" aria-label="No"></span> |
| **Tooling and delivery** | | | | | | | |
| Toolchain | One nsh executable, no Node | Node, npm, packager | Node, Metro, CocoaPods, Gradle | flutter CLI, Xcode, Gradle | CMake, Qt Creator | cargo | cargo, CMake |
| Hot reload | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> Fast Refresh | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> QML | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Live preview |
| Signed, sandboxed delivered UI | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> OTA of trusted code | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Loads QML | <span class="status status--partial" role="img" aria-label="Partial"></span> GPUI Shell | <span class="status status--no" role="img" aria-label="No"></span> |
| API compatibility registry | <span class="status status--yes" role="img" aria-label="Yes"></span> nsh-compat in CI | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> |
| License | Apache-2.0 | MIT | MIT | BSD-3-Clause | LGPL, GPL, commercial | Apache-2.0 | GPL, commercial |

GPUI and GPUI Fast share a column: they run on the same platforms, and GPUI Fast adds retained rendering on top of GPUI. "GPUI Shell" is [GPUI Shell](https://gpui-kit.com/shell).

Read down the NSH column: it is the only one green in every group. In particular, no other framework here combines

- **seven targets from one app** — iOS, Android, macOS, Windows, Linux, the web as HTML, and terminals — in one repository and one release;
- **the platform's own views, text editors and accessibility tree** with **no JavaScript runtime** for Rust apps;
- **interaction that adapts per device inside the base components**, instead of in each app;
- **embedding as a single view** into existing iOS, Android, desktop and GPUI apps;
- **signed, sandboxed UI delivered after release**, with revocation and rollback;
- **one executable for the whole toolchain** and **a CI-checked API compatibility registry**.

### What the rows mean

- **Electron.** Chromium plus Node in every app: the desktop's widest web-content path, with no phones. Minimums follow [Electron's supported platforms](https://www.electronjs.org/docs/latest/tutorial/support#supported-platforms); its web version is the same code deployed as a separate site.
- **Drawn by, update model.** NSH mounts each platform's own views and keeps them (retained mode): a change updates only the views it touched. React Native also mounts platform views, on iOS and Android. Flutter, Qt Quick, GPUI and Slint draw with their own renderers; GPUI builds the element tree each frame (immediate mode), and [GPUI Fast](https://github.com/longbridge/gpui-fast) reuses what did not change.
- **Platforms.** NSH's minimums are its own ([Platforms](./platforms.md)). The others come from [React Native 0.76](https://reactnative.dev/blog/2024/10/23/release-0.76-new-architecture) (iOS 15.1, Android 7 / API 24; macOS and Windows through the separate react-native-macos and react-native-windows projects, the web through the react-native-web library), [Flutter 3.47](https://docs.flutter.dev/reference/supported-platforms), [Qt 6.10](https://doc.qt.io/qt-6.10/supported-platforms.html), [Zed's requirements](https://zed.dev/docs/installation) for GPUI, and [Slint's platform guides](https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/general/) (iOS from Rust). "Canvas" means the framework draws into a canvas on the page; NSH's web apps are HTML elements styled with CSS, which the browser scrolls, selects and edits ([Web](./web.md)).
- **System text.** NSH's text fields are the platform's editors — `UITextView`, `EditText`, `NSTextView`, GTK's `TextView`, the browser's — with the system's input methods, selection handles and editing menus; on Windows, NSH's own editor works with DirectWrite and the system's input methods (IMM32). React Native also uses the platform's text inputs. The self-drawn frameworks implement text editing themselves and connect it to the system's input methods.
- **Per-device UX.** NSH's base components follow the device: the same `Menu` is a popup at its trigger with a pointer and a sheet from the bottom on a phone, and a `ContextMenu` opens on a right click or a held finger. In the others, adapting interaction per device is mostly the app's own work.
- **Where layout, animation and lists run.** In NSH they run in Rust off the JavaScript path: a busy TSX handler does not stall scrolling or an animation ([Architecture](./architecture.md)). React Native lays out with Yoga in C++ while list windowing and most animation logic stay in JS unless a library moves them.
- **Accessibility.** NSH's elements are native views, so VoiceOver, TalkBack and the desktop screen readers read the real hierarchy; apps add labels, roles and actions ([Accessibility](./accessibility.md)). GPUI exposes little to assistive technology today.
- **Windows, menus, tray.** `nsh/desktop` provides windows, menus, the tray and Dock badges on macOS, Windows and Linux ([nsh/desktop](../modules/nsh-desktop.md)). GPUI has windows and menus but no tray; Flutter takes them from plugins.
- **Embedding.** An NSH app embeds as one view in an existing iOS, Android or desktop app ([Embedding](./embedding.md)), and as a view in GPUI and GPUI Kit apps through its GPUI backend ([GPUI](./gpui.md)). React Native (brownfield) and Flutter (add-to-app) also embed, each bringing its own runtime.
- **Terminal.** The same component model renders terminal apps through ratatui ([TUI](./tui.md)).
- **Toolchain, hot reload.** One `nsh` executable creates, runs with hot reload, tests on real platforms, packages and signs for every target, and TSX needs no Node or npm ([CLI](./cli.md)).
- **API compatibility registry.** Every public API change is recorded in nsh-compat and checked by CI, so an app built for a compatible version keeps running unchanged ([Backward compatibility](./compatibility.md)). The others follow semantic versioning without a machine-checked registry.
- **No JS runtime.** A Rust app's runners link no JavaScript engine on any platform; TSX is the second path, on the same engine. Qt Quick's QML runs on Qt's JavaScript engine; Qt Widgets in C++ do not.
- **Signed delivery.** NSH verifies signed `.nshpkg` packages against the keys it trusts and a revocation list, and runs each in a VM with the limits and capabilities the host grants ([Signed packages](./signed-packages.md), [Sandbox and capabilities](./sandbox-and-capabilities.md)). A packaged NSH app takes its own TSX over the air the same way, iOS and Android included, and rolls back a release that fails to start ([OTA Updates](./ota-updates.md)). React Native's over-the-air updates ship trusted app code; Qt can load QML files; GPUI apps take JavaScript modules through [GPUI Shell](https://gpui-kit.com/shell).

### What sets NSH apart

- **Native on every platform, in both senses.** Each backend builds on its platform's own views, text and input methods — UIKit, Android views, AppKit, Win32 with Direct2D, GTK 4, HTML — and each platform's interaction comes with them. Text selection, input methods and accessibility are the system's, and what a new OS release brings to them reaches the app ([Introduction](./index.md#what-native-means)).
- **Phones, desktops and the web from one app.** Six platforms are backends of one framework, sharing one reactive graph, renderer, Taffy layout and module system, with layouts that reshape and input that follows the device; on the web, the app is HTML, published as a static site.
- **Rust first, with no JavaScript runtime.** A whole app can be Rust from start to end, compiled to native code with Rust's types, threads and crates. TSX is the second path, on the same engine, for screens written quickly ([Building with Rust](./rust.md)).
- **Retained views for everyday apps.** Components run once and fine-grained signals send only the properties that changed, so chats, forms, lists and documents sit near idle at rest ([Introduction](./index.md#retained-views), [Reactivity](./reactivity.md)).
- **UI delivered after release, signed and sandboxed.** Apps built with NSH — and native apps written with each platform's own tools — take TSX modules at run time, written by a team or generated by AI ([Dynamic UI](./index.md#dynamic-ui-and-ai-generated-ui)).
- **One toolchain, for people and agents.** One `nsh` executable creates, runs, hot-reloads, tests on real platforms, packages and signs apps for every target; TSX needs no Node or npm ([CLI](./cli.md), [Installation](./installation.md)).
- **A compatibility promise for shipped apps.** An app built for a compatible version keeps running unchanged on every later one; each API change is recorded in nsh-compat and checked by CI ([Backward compatibility](./compatibility.md)).

### Measured numbers

These come from the [performance report](./performance-report.md) and the [NSH performance benchmark](https://github.com/longbridge/nsh-perf-bench), all release builds; reproduce them on your own devices with `nsh`'s trace ([Measuring an app](./performance-report.md#measuring-an-app)).

| Measurement | Result |
|---|---|
| Engine start | ~75–155 µs |
| Counter app first frame (Mac Catalyst) | ~1.05–1.15 ms |
| One tap (JS plus commit) | ~0.12 ms |
| Rust app memory on macOS | ~19 MB, against ~17 MB for an AppKit window with one label |
| Sorting 100,000 rows in a Rust module called from TSX | ~3 ms, on par with pure Rust |
| 500-row quote table, 1,000 updates a second (Linux) | NSH Rust 18% of one core, GPUI Fast 20% |
| Stocks demo: 22 live sparklines, intraday chart, order book, trades (Linux) | 117–120 FPS, 25–29% CPU |

## Moving to NSH

### Electron and Tauri

Electron bundles Chromium and Tauri uses the system web view: both build desktop apps from web content, which is pleasant to develop, at the price of a browser engine in every app and text, scrolling and input that come from the page rather than the system. Longbridge built on GPUI precisely to leave this path.

- **What you gain.** The platform's own windows, menus, text fields and input methods; Rust apps carry no browser and no JS engine, with memory close to a plain system window (see [Measured numbers](#measured-numbers)); and the same code reaches phones. A web build remains, and it is real HTML.
- **What carries over.** TSX reads like JSX and styling reads like CSS and [Tailwind](./tailwind.md), so most of a web team's experience applies. When you need to show an existing page, embed it in a [web view](./web-view-and-video.md).
- **What it costs.** DOM code and npm component libraries that depend on a browser do not carry over; screens are rewritten with NSH elements.

Chat and AI apps gain the most. Many AI products render their conversations as web pages, and long sessions bring dropped frames while scrolling, awkward input methods and selection, and weak integration with system accessibility and gestures. NSH renders rich text, code blocks, tables, images and streamed output with the platform's text views and editors ([Text](./text.md), [Lists](./lists.md)), so selection, copying, input methods and accessibility are the system's.

### React Native

React Native is closest to NSH in intent: both aim to cut duplicated client work and both use platform views. The difference is architectural, and the New Architecture leaves these points in place:

| | React Native | NSH |
|---|---|---|
| Runtime | Always a JS engine (Hermes), with app logic and reconciliation in JS | Rust apps have no JS engine; TSX apps run only app code on QuickJS, while the reactive graph, renderer, layout, animation and list virtualization run in Rust |
| Update model | React reconciliation: re-render components after a state change, then diff | Fine-grained signals (Solid-style): components run once and push only changed properties |
| Desktop | Separate react-native-macos and react-native-windows projects, no Linux | AppKit, Win32 + Direct2D and GTK 4, in the same framework and release as phones |
| Web | Community react-native-web | A framework backend producing HTML and CSS |
| Per-device UX | Left to the app | Base components adapt: `Menu` pops at the pointer on desktop and rises from the bottom on phones |
| Updates after release | OTA delivery of a trusted JS bundle | Signed `.nshpkg`, revocation lists, a sandboxed VM with host-set capabilities and limits, automatic rollback of versions that fail to start |
| Toolchain | Node, Metro, CocoaPods, Gradle and more | One `nsh` executable |

- **What carries over.** TSX looks much like JSX, and the ideas of components, props and events stay; hooks become signals, with no dependency arrays or memoization.
- **What it costs.** NSH is not a compatibility layer for React Native, by design: it replaces React reconciliation with a more efficient model, so React Native packages do not run on it. The framework provides lists, navigation, animation, gestures, text, canvas, web views and video natively, and visual components are the app's own ([Design guides](./design-guides.md)); an existing native SDK becomes a [native component](./native-components.md) or a Rust [module](./platform-extensions.md).

### Flutter

Flutter draws every pixel with its own engine, so an app looks the same on every platform. NSH takes the other path: each platform's views, text and text input, and on the web, HTML elements with the browser's own scrolling, selection and input.

- **What you gain.** Text editing, input methods, the selection menu (copy, share, translate, look up) and accessibility are the platform's, and the materials and interaction a new OS release brings reach the app without waiting for an engine to reimplement them; web output is HTML that can be selected, indexed and read by screen readers, not a canvas.
- **Language.** Dart becomes Rust and TSX: Rust brings crates and native threads, TSX quick screens.
- **What it costs.** The look differs between platforms, which is the point of using platform UI; for pixel-identical branded UI, Flutter is the natural fit.

### GPUI and GPUI Fast

[GPUI](https://gpui-kit.com) is a Rust UI framework drawn on the GPU for demanding desktop apps; [GPUI Fast](https://github.com/longbridge/gpui-fast) is Longbridge's fork with retained rendering — reusing unchanged views, layout and text measurement — scroll layers and native window compositing. NSH comes from the same team and suits a different load.

- **What you gain.** Phones and the web; the platform's own text input, selection, input methods and system integration; a TSX path and signed delivery. NSH's GPUI backend embeds NSH apps in existing GPUI and GPUI Kit apps, so screens can move one at a time instead of in a rewrite ([GPUI](./gpui.md)).
- **The numbers.** In the [NSH performance benchmark](https://github.com/longbridge/nsh-perf-bench) (Linux, a 500-row quote table, NSH on its GTK 4 backend), everyday load costs the same: at 1,000 quotes a second NSH Rust uses 18% of one core and GPUI Fast 20%. Under sustained heavy updates GPUI Fast leads: at 20,000 quotes a second it uses 25% against NSH's 73–78%, and on an animation of 1,000 boxes computed every frame it uses about a third of NSH's CPU.
- **How to choose.** For a desktop-only trading terminal or custom visualization dominated by thousands of views updated every frame, stay on GPUI Fast; to reach phones and the web with everyday screens, or to deliver UI after release, use NSH.

### Qt

Qt is a mature toolkit with two UI technologies, Qt Widgets and Qt Quick, and long-built tooling and platform support. NSH differs in foundation and model: Rust at the core with a TSX path, a toolchain designed for automated (and AI-driven) development, HTML on the web, and one component model that adapts interaction across mobile, desktop and web; the license is Apache-2.0.

### Slint

Slint is a declarative UI toolkit with its own language and a light renderer, aimed at desktop and embedded devices and usable from Rust, C++, JavaScript or Python. Both value native performance; NSH's path is each platform's own UI and interaction, HTML on the web, Rust and TSX as app languages, and AI-driven development across six platforms.

## Evaluating it: start with a pilot

No rewrite is needed to find out. An NSH app can ship on its own or embed as one view in an existing iOS, Android or desktop app ([Embedding](./embedding.md)), so start with one screen:

1. **Pick the scenario that matters most to you**: live data and charts, a rich-text or AI chat screen, a module you need to deliver after release, or one module of a desktop client.
2. **Build it in NSH alongside the current one**: screens in TSX to begin with, key logic and computation in Rust.
3. **Compare with your own data**: start time and memory; CPU and frame rate under frequent updates; scrolling of long content and whether selection and input methods feel natural; whether menus, windows and text input are consistent across platforms; and, for delivered UI, signature checks, capability limits and rollback.

`nsh`'s trace and the methods in the [performance report](./performance-report.md) apply directly to these comparisons.

## UI delivered after release

An app that wants UI it did not ship with — answers generated by a model, features from partners or other teams, screens that change without a release — has a handful of established options, on phones and desktops alike. They differ in what draws the UI, what the delivered code may do, and how much of the app has to change to host them.

| Capability | NSH | Web view | Server-driven UI | Mini-programs | OTA updates |
|---|---|---|---|---|---|
| Drawn by | Platform UI | Browser engine | The app's views | Web engine + layer | Framework renderer |
| Native look and behavior | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> |
| Own state, logic and layout | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> |
| Host's native components | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Bridge | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--yes" role="img" aria-label="Yes"></span> |
| Sandbox the host configures | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Same-origin | <span class="status status--yes" role="img" aria-label="Yes"></span> Schema | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--no" role="img" aria-label="No"></span> |
| Signed, revocable packages | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--no" role="img" aria-label="No"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> |
| Fit for AI-generated UI | <span class="status status--yes" role="img" aria-label="Yes"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> Web pages | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--partial" role="img" aria-label="Partial"></span> | <span class="status status--no" role="img" aria-label="No"></span> |
| Cost to the host app | One view and a library | A browser engine per view | A renderer per component | A container SDK | The whole app in the framework |

- **Web view (H5).** A page in a browser engine: anything HTML can express, with the browser's look and a hand-written bridge to native code.
- **Server-driven UI.** JSON that recombines the components and actions the app already ships: easy to generate, with no local state or logic beyond what the schema provides.
- **Mini-program containers.** A platform-specific framework and API on a web engine, inside a large container SDK.
- **Over-the-air updates** (React Native with a code-push service, for example). The whole framework, but updates are trusted app code, and the app has to be written in that framework.
- **NSH.** A TSX module with its own state, logic, animation and layout, rendered with the platform's own UI, that can use the host's registered native views. It arrives as a signed package the app verifies, runs in its own VM with memory, call-time and node limits, and reaches only the network hosts, web pages and system settings the host grants ([Sandbox and capabilities](./sandbox-and-capabilities.md)). The host adds one view where the module appears ([Platforms](./platforms.md#embed-in-a-native-app)), and the same bundle runs on every platform. This is the reason NSH exists next to [GPUI Shell](./index.md#native-shell-and-gpui-shell).

## When another option fits better

- The screen is dominated by sustained, dense updates of thousands of views — a market-data terminal streaming tens of thousands of quotes a second, a custom visualization redrawn every frame: GPUI Fast's retained GPU rendering uses a fraction of the CPU (see [GPUI and GPUI Fast](#gpui-and-gpui-fast)).
- The app must look pixel-identical on every platform: a framework that draws everything itself, such as Flutter or Slint, gives that by design.
- You need a large ecosystem of third-party packages today: React Native, Flutter and Qt have years of them.
- The UI is a document or an existing web page: a web view shows it as it is.
- The set of screens is fixed and small, and only content and order change: server-driven UI needs no runtime at all.

## See also

- [Introduction](./index.md): what NSH is, and what native means in it.
- [Architecture](./architecture.md): what runs where inside NSH.
- [Performance](./performance.md): the rules NSH follows, and how to measure an app.

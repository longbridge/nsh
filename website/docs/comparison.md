---
title: Comparing approaches
description: "How Native Shell compares with React Native, Flutter, Qt, GPUI, Slint, Electron and Tauri, and with web views and server-driven UI for UI delivered after release."
order: 2
group: start
---

# Comparing approaches

NSH is a framework for whole apps on iOS, Android, macOS, Windows, Linux and the web, and a runtime for UI delivered to apps after release. This page compares it first with the cross-platform frameworks people write apps in, then with the ways apps take UI they did not ship with. The comparison is about architecture — what draws the UI, where it runs, what the app is written in. Several of these frameworks also bring large, long-standing ecosystems.

## Cross-platform frameworks

| | What draws it | Platforms | App code | Code delivered after release |
|---|---|---|---|---|
| **React Native** | The platform's views on iOS and Android; desktops through separate projects | iOS, Android; Windows and macOS out of tree | JavaScript or TypeScript, with React | Over-the-air updates of trusted app code |
| **Flutter** | Its own engine draws every pixel | iOS, Android, macOS, Windows, Linux, web | Dart | Not part of the framework |
| **Qt** | Qt Widgets and Qt Quick, drawn by Qt | Desktops, mobile, embedded systems, web | C++, QML, Python | QML the app loads |
| **GPUI** ([GPUI Fast](https://github.com/longbridge/gpui-fast)) | Its own GPU renderer | macOS, Windows, Linux | Rust | JavaScript modules with GPUI Shell |
| **Slint** | Its own renderer | Desktops, embedded systems, Android, web | The Slint language, with Rust, C++, JavaScript or Python | Not part of the framework |
| **Electron** | Chromium, bundled with the app | macOS, Windows, Linux | HTML, CSS, JavaScript | Web content the app loads |
| **Tauri** | The system's web view | macOS, Windows, Linux, iOS, Android | HTML, CSS, JavaScript; Rust for the native side | Web content the app loads |
| **NSH** | Each platform's own UI: UIKit, Android views, AppKit, Win32 with Direct2D, GTK 4, HTML | iOS, Android, macOS, Windows, Linux, web | Rust apps, TSX, or both; the runtime is Rust | Signed TSX packages, verified and sandboxed per view |

What sets NSH apart:

- **Each platform's own UI, under one app model.** NSH unifies the components, state, logic and API, and leaves the drawing to each platform: text selection, input methods, accessibility and the system's look come from the platform, and what a new OS release brings to them reaches the app with it ([Introduction](./index.md#what-native-means)).
- **Interaction that follows the device.** The same `Menu` is a popup at its trigger with a pointer and a sheet from the bottom on a phone; a `ContextMenu` opens on a right click or a held finger. Cross-platform covers interaction, not only code reuse.
- **Rust first, TSX beside it.** A whole app can be Rust from start to end, with Rust's types, threads and crates and native-speed computation; TSX serves the screens written quickly and the UI delivered after release, on the same engine. One `nsh` executable builds, tests and packages both, and brings its own TSX tooling.
- **The web as a platform of its own.** On the web, NSH renders HTML elements styled with CSS, and the browser scrolls, selects text and handles input; a published app is a static site, cached and served like any other ([Web](./web.md)).
- **Plugins and generated UI on the same engine.** The apps NSH builds — and native apps written with each platform's own tools — can host signed TSX modules in a sandbox ([Dynamic UI](./index.md#dynamic-ui-and-ai-generated-ui)).

### React Native

React Native is the closest in intent: both reduce duplicated client work and use the platform's views. React Native's center is JavaScript or TypeScript with React and its package ecosystem, on iOS and Android, with desktop support in separate projects. NSH's center is Rust: its runtime, renderer and modules are Rust, Rust is an app language of its own, and TSX is one path among two; its six platforms, the desktops and the web included, are backends of the same framework.

### Flutter

Flutter draws every pixel with its own engine, so an app looks the same on every platform. NSH takes the other path: the views, text and text input of each platform, and on the web HTML elements and the browser's own scrolling, selection and input. Text editing, input methods and selection are the platform's, rather than reimplemented per platform.

### Qt

Qt is a mature toolkit with two UI technologies, Qt Widgets and Qt Quick, and long-standing tools and platform support. NSH differs in its base and its model: Rust as the core language with a TSX path, a toolchain designed for automated (and AI-driven) development, the web rendered as HTML, and mobile, desktop and web interaction adapted inside one component model.

### GPUI and GPUI Fast

[GPUI](https://gpui-kit.com) is a Rust UI framework that draws with the GPU, built for demanding desktop apps; [GPUI Fast](https://github.com/longbridge/gpui-fast) is Longbridge's fork with retained rendering — it reuses unchanged views, layout and text measurements — scroll layers and native window composition. NSH builds on each platform's views instead, so text input, selection, input methods and system interaction are the platform's own, and it covers phones and the web as well; NSH's GPUI backend embeds NSH apps in GPUI and GPUI Kit apps ([GPUI](./gpui.md)).

The two suit different loads. In the [NSH performance benchmark](https://github.com/longbridge/nsh-perf-bench) (Linux, a 500-row quote table, NSH on its GTK 4 backend), everyday loads cost the same: at 1,000 quotes a second, NSH Rust uses 18% of a core and GPUI Fast 20%. Under sustained, dense updates GPUI Fast is ahead: at 20,000 quotes a second it uses 25% of a core where NSH uses 73–78%, and frame-by-frame animation of 1,000 boxes costs it about a third of NSH's CPU.

### Slint

Slint is a declarative UI toolkit with its own language and light renderers, for desktops and embedded devices, used from Rust, C++, JavaScript or Python. Both value native performance; NSH's route is each platform's own UI and interaction, HTML on the web, Rust and TSX as app languages, and AI-driven development across six platforms.

## UI delivered after release

An app that wants UI it did not ship with — answers generated by a model, features from partners or other teams, screens that change without a release — has a handful of established options, on phones and desktops alike. They differ in what draws the UI, what the delivered code may do, and how much of the app has to change to host them.

| | What draws it | What it can express | Confinement | Fit for generated UI | Cost to the host app |
|---|---|---|---|---|---|
| **Web view (H5)** | A browser engine | Anything HTML, CSS and JS can | Same-origin rules; native access through a hand-written bridge | Models write HTML well; the result looks and behaves like a web page | A browser engine per view; a bridge to maintain |
| **Server-driven UI** | The app's own native views | Only the components and actions the app built in, described in JSON | Total: the schema is the limit | Easy to generate, little to express: no local state or logic beyond what the schema provides | A renderer for every component and its versions, in each app |
| **Mini-program containers** | Usually a web engine with a proprietary layer | A platform-specific framework and API | The container's permission model | Proprietary dialect, less known to models | A large container SDK |
| **Over-the-air updates of a cross-platform app** (React Native with a code-push service, for example) | That framework's renderer | The whole framework | None: updates are trusted app code | Code generated per request is not what these updates are for | The app has to be written in that framework |
| **NSH** | The platform's own UI: UIKit, Android views, AppKit, GTK 4, Win32 with Direct2D, HTML | Components with state, logic, animation and layout; host-registered native views | Signed packages, a VM per view with host-set limits, capabilities granted per view | Designed for it: one TSX module, typed API, headless checks | One view and a library in an existing native app |

How NSH differs here:

- **The platform's own UI.** A module is made of the platform's views, so text selection and its menu, input methods, accessibility and scrolling are the system's own — the reason NSH exists next to [GPUI Shell](./index.md#native-shell-and-gpui-shell).
- **More than a schema.** Server-driven UI only recombines what the app already ships. An NSH module has its own state, logic, animations and layout, so a model can produce an interface the app's developers never anticipated.
- **Confined by the host.** Delivered code is untrusted. NSH checks signatures and revocations before running anything, gives each view its own VM with memory, call-time and node limits, and lets the host decide which network hosts, web pages and system settings a module may reach ([Sandbox and capabilities](./sandbox-and-capabilities.md)).
- **Embedded in the app.** The app keeps its toolchain; NSH is a view placed where a module should appear ([Platforms](./platforms.md#embed-in-a-native-app)).
- **One module, every platform.** The same bundle runs on iOS, Android, macOS, Windows, Linux and the web, each time with that platform's own UI ([Platforms](./platforms.md)).

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

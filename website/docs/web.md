---
title: Web (WebAssembly)
description: Run NSH apps in a browser with the WebAssembly build of the runtime, which renders to the DOM; it powers the live demos on this site.
order: 16
group: platforms
demo: counter
---

# Web (WebAssembly)

The NSH runtime also compiles to WebAssembly. The same engine runs in a browser page and renders to the DOM instead of UIKit or Android views: the core, the reactive graph, the renderer, Taffy layout, the `nsh/ui/mobile` components and QuickJS itself. Every live demo on this site is an app bundle from `nsh build` running on this runtime.

## What runs where

On every native platform the engine has its own thread and hands one `MountBatch` per commit to the main thread. A browser page has one thread, so the web build drives the same engine step by step from the page's event loop (`nsh_engine::LocalEngine`):

```text
pointer / keyboard / scroll events ─┐
setTimeout (the engine's deadline) ─┼─▶ nsh.wasm: engine turn (JS, core, layout) ─▶ one JSON batch ─▶ nsh.js: DOM
requestAnimationFrame ──────────────┘                                                 ▲
                                                          native animation outputs ───┘
```

- **Layout is the core's.** Every view is an absolutely positioned element at the frame Taffy computed, so flexbox, grid, `aspectRatio` and percentages behave exactly as on devices. The browser's own layout is not involved.
- **Styles are turned into CSS in Rust.** Background, borders, radii, shadows (`shadow*` and `boxShadow`), gradients, filters, `backdropFilter`, transforms, `transformOrigin`, `outline`, `mixBlendMode`, transitions and the built-in keyframe animations become CSS properties. Tailwind classes, `active:`, `hover:` and `focus:` states and `dark:` all resolve natively, as they do on devices.
- **Text is laid out in Rust** with font measurements from the browser (`CanvasRenderingContext2D.measureText`) and drawn run by run at the positions the layout chose, so what was measured is what is drawn. Lines break after spaces, around CJK characters and at `\n`; `numberOfLines` truncates with an ellipsis.
- **Native animations run in Rust** on `requestAnimationFrame`: `Animated` timing, spring and decay, `interpolate`, native values bound to `opacity`, `transform` and `backgroundColor`, drags bound with `pan`, scroll-linked values and canvas arguments. JS is not involved per frame, as on devices.
- **Input:** presses with `activeOpacity` and long presses, the responder system (`onTouch*`, `PanResponder`), native drags and two-finger pinch and rotation, scroll views (the browser's scrolling, plus dragging with a mouse), `<Input>` with controlled values, focus and submit, `Canvas`, `Stack` and `Screen` with a header, `Modal`, `Alert`.

## Build it

```sh
cargo xtask web                       # nsh.wasm, nsh.js, the player and website/demos/*.tsx
cargo xtask web --out target/play     # somewhere else
```

The output directory holds:

| File | What it is |
|---|---|
| `nsh.wasm` | The engine, the core, the mobile components and QuickJS (about 4.6 MB, 1.5 MB gzipped) |
| `nsh-playground.wasm` | The same plus NSH's TSX compiler, for the [playground](#the-playground) (about 8 MB, 2.4 MB gzipped) |
| `nsh.js` | The DOM runtime: loads `nsh.wasm`, applies batches, turns input into events |
| `index.html` | A player page: runs `demos/<name>.js` for `#<name>` |
| `demos/<name>.js` | Each demo bundled by `nsh build --release` |

Serve the directory with any static server (it needs the `application/wasm` MIME type for streaming compilation) and open `index.html#counter`.

## The playground

The [Playground](/playground) edits and runs an app in the browser: the page compiles the TSX with NSH's own JSX compiler (built into `nsh-playground.wasm`, without rolldown) and runs it on every pause in typing. Every demo on this site has an **Edit in Playground** link, and **Share** puts the code in the link itself, so nothing is stored on a server.

A playground app is one module: it imports NSH's modules (`nsh`, `nsh/*`, `nsh/ui/mobile`) but no other files or packages. `console.log` output and errors appear below the phone.

The compiler is also a Rust function, `nsh_bundler::compile_module(name, source, ui_sets)`, available without the crate's `bundle` feature.

## Embed it in a page

`nsh.js` is an ES module. `createPlayer` loads the runtime into a container; `run` starts a bundle (the `bundle.js` that `nsh build` writes):

```js
import { createPlayer } from "./nsh.js";

const player = await createPlayer(document.querySelector("#phone"), {
  wasm: "./nsh.wasm",
  assetBase: "./assets/",   // where `asset:/…` URIs (image imports) are served from
  // dark: true,            // force an appearance; otherwise it follows the system
});
player.run(await (await fetch("./bundle.js")).text());
```

With `nsh-playground.wasm`, `player.runSource(tsx)` compiles TSX first and returns the compile error, or `null`; `player.compile(tsx)` returns `{ code }` or `{ error }`.

The container's size is the app's window (`windowSize()`); resizing the container resizes the app. `player.setDark(true)` switches the appearance at run time, which apps observe through `colorScheme()` and `dark:` classes.

> [!NOTE]
> The web build reports `Platform.OS` as `"ios"`, so apps render their iOS look. It exists to show apps, not as a third target platform.

## Limits

The web runtime is for demos and documentation. It does not have:

- **Networking**: `fetch` fails with "this host has no network stack".
- **Web views, videos and native components**: `WebView` maps to an `<iframe>` and `Video` to `<video>` only in the simplest form; registered native components render an empty box.
- **Platform services**: the status bar, toasts and the Android back button do nothing, and there is no software keyboard to avoid (`keyboard()` stays 0).
- **Custom fonts loaded before first use**: imported fonts are registered with `FontFace`, which loads asynchronously, so the first frame may be measured with a fallback font.
- **Text shaping details**: line breaking is a subset of UAX #14 (spaces, CJK, hyphens), with no hyphenation and no bidirectional reordering beyond the paragraph direction.
- **Pull to refresh and zooming scroll views.**

The QuickJS VM runs with development settings (no hardening) and the default [limits](./sandbox-and-capabilities.md).

## See also

- [Architecture](./architecture.md): the engine, the core and the mount batches the web build reuses.
- [Testing and trace](./testing-and-trace.md): `nsh trace` runs the same engine headless, with an approximate text engine.

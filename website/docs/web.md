---
title: Web (WebAssembly)
description: Run nsh apps in a browser with the WebAssembly build of the runtime, which renders to the DOM; publish them as static sites with nsh package --target web, develop them with nsh run --target web.
order: 16
group: platforms
demo: counter
---

# Web (WebAssembly)

The nsh runtime also compiles to WebAssembly. The same engine runs in a browser page and renders to the DOM instead of UIKit or Android views: the core, the reactive graph, the renderer, Taffy layout, the `nsh/ui/mobile` components and QuickJS itself. `nsh package --target web` publishes an app as a static site for any web server or CDN ([Publish an app](#publish-an-app)), `nsh run --target web` runs it in the browser while you edit it ([Develop in the browser](#develop-in-the-browser)), and every live demo on this site is an app bundle from `nsh build` running on this runtime.

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

## Publish an app

```sh
nsh package App.tsx --target web           # out/<Name>/: a static site
nsh package App.tsx --target web --run     # and serve it on this machine, opened in the browser
```

`nsh package --target web` writes `out/<Name>/` (`--out` for another directory), laid out like a frontend build: one
page and files named after their content.

| File | What it is |
|---|---|
| `index.html` | The app's page: its name as the title, the icons, the colors of its `appearance`, a loading indicator, and a message when the app cannot load or start |
| `manifest.webmanifest` | The name, id, icons and colors browsers install the app with |
| `nsh.<digest>.js` | The DOM runtime, minified (42 KB, 12.6 KB with brotli) |
| `nsh.<digest>.wasm` | The engine, the core, the mobile components and QuickJS (about 3.7 MB, 1.3 MB gzipped, 1.0 MB with brotli) |
| `app.<digest>.js` | The app, built as `nsh build --release --split` builds it |
| `chunks/<name>.<digest>.js` | What `import()` loads, downloaded the first time it is imported ([split bundles](#split-bundles)) |
| `assets/<path>.<digest>.<ext>` | Every asset the app imports, loaded from its URL when it is used (fonts before the app starts: text is measured with them) |
| `icons/<size>.<digest>.png` | The icon of `nsh.json` at 32 (favicon), 180 (`apple-touch-icon`), 192 and 512 pixels |

`<digest>` is the start of the SHA-256 of the file's content: a file's name changes exactly when its content does, so a
new release changes only `index.html` and the files that really changed. Everything uses relative URLs, so the site
works from any directory of any host (`https://example.com/apps/counter/`).

### Deploy it

Upload the directory to any static host: an object store behind a CDN, Cloudflare Pages, Netlify, GitHub Pages,
nginx. Two rules make it load fast and update reliably:

| Files | `Cache-Control` |
|---|---|
| `index.html`, `manifest.webmanifest` | `no-cache` (revalidated on every load, so a release shows at once) |
| everything else | `public, max-age=31536000, immutable` |

Serve `.wasm` as `application/wasm` (browsers then compile it while it downloads) and let the host or CDN compress
text and WebAssembly (gzip or brotli). `nsh package --target web --run` serves the site this way on
`http://127.0.0.1:4173/`.

An app published for the web runs like a packaged native app: as a release, [hardened](./sandbox-and-capabilities.md)
(no `eval`), with the capabilities its `nsh.json` declares. When they restrict `network`, the page also carries a
Content-Security-Policy `connect-src` with the declared hosts (and the page's own origin), so the browser itself
refuses requests and redirect hops to any other host. An error that keeps it from starting is shown on the page;
later errors go to the browser's console.

### Embed it with an iframe

There is no embedding API on the web: embed the published app with an `<iframe>`. Its size is the app's window:

```html
<iframe src="https://apps.example.com/counter/" title="Counter"
        style="width: 390px; height: 844px; border: 0"></iframe>
```

## Develop in the browser

```sh
nsh run App.tsx --target web
```

starts the dev server and opens its page, `http://127.0.0.1:8790/`, in the browser. The page is a development host
like the others: it runs each build of the app, applies edits in place (hot updates), shows build errors and runtime
errors over the app, and sends the app's `console` output to the terminal. Phones on the same network open the
address `nsh run` prints (`browsers on this network: http://192.168.…:8790/`). `nsh dev` serves the same page once the
web runtime is there (in the distribution, always).

## Build the website's player

```sh
cargo xtask web                       # nsh.wasm, nsh.js, the player and website/demos/*.tsx
cargo xtask web --out target/play     # somewhere else
```

The output directory holds:

| File | What it is |
|---|---|
| `gallery/nsh.<digest>.wasm` | The engine, the core, the mobile components and QuickJS (about 3.7 MB, 1.3 MB gzipped, 1.0 MB with brotli; built with the `web` profile), shared by the player and the gallery |
| `nsh-playground.wasm` | The same plus nsh's TSX compiler, for the [playground](#the-playground) (about 8 MB, 2.4 MB gzipped) |
| `gallery/nsh.<digest>.js` | The DOM runtime (minified): loads the wasm, applies batches, turns input into events |
| `index.html` | A player page: runs `demos/<name>.js` for `#<name>` |
| `demos/<name>.js` | Each demo bundled by `nsh build --release` |
| `gallery/` | The gallery app published with `nsh package --target web`, shown on the [Gallery](/gallery) page |

Serve the directory with any static server (it needs the `application/wasm` MIME type for streaming compilation) and open `index.html#counter`.

## The playground

The [Playground](/playground) edits and runs an app in the browser: the page compiles the TSX with nsh's own JSX compiler (built into `nsh-playground.wasm`, without rolldown) and runs it on every pause in typing. Every demo on this site has an **Edit in Playground** link, and **Share** puts the code in the link itself, so nothing is stored on a server.

A playground app is one module: it imports nsh's modules (`nsh`, `nsh/*`, `nsh/ui/mobile`) but no other files or packages. `console.log` output and errors appear below the phone.

The compiler is also a Rust function, `nsh_bundler::compile_module(name, source, ui_sets)`, available without the crate's `bundle` feature.

## Embed it in a page

`nsh.js` is an ES module. `createPlayer` loads the runtime into a container; `run` starts a bundle (the `bundle.js` that `nsh build` writes):

```js
import { createPlayer } from "./nsh.js";

const player = await createPlayer(document.querySelector("#phone"), {
  wasm: "./nsh.wasm",
  assetBase: "./assets/",   // where `asset:/…` URIs (image imports) are served from
  // dark: true,            // force an appearance; otherwise it follows the system
  // idiom: "phone",        // force "phone", "pad" or "desktop"; otherwise the pointer and the size decide
});
player.run(await (await fetch("./bundle.js")).text());
```

`assets` (a map from `asset:/` paths to URLs) replaces `assetBase`, and `readAsset(uri)` returns the bytes of an asset the engine needs at once (a font) or `null`; `player.run(code, { release: true, capabilities })` runs a bundle as a release under the capabilities it declares (the default is development settings). The page `nsh package --target web` writes uses all three.

`run(code, { chunks, chunkBase })` also takes where the chunks of a [split bundle](#split-bundles) are.

With `nsh-playground.wasm`, `player.runSource(tsx)` compiles TSX first and returns the compile error, or `null`; `player.compile(tsx)` returns `{ code }` or `{ error }`.

The container's size is the app's window (`windowSize()`); resizing the container resizes the app. `player.setDark(true)` switches the appearance at run time, which apps observe through `colorScheme()` and `dark:` classes.

## The browser as a platform

`Platform.OS` is `"web"`. What kind of device the browser is on decides how the app behaves, as `Platform.constants.interfaceIdiom`:

- `"desktop"`: a mouse (a fine pointer that hovers) and a container at least 600 points wide. `Platform.isDesktop` is `true`, `Platform.select` picks `desktop`, `desktop={{…}}` props apply and headers are as tall as on the desktop backends.
- `"pad"`: a touch screen whose short side is at least 600 points; `"phone"` otherwise. These are mobile: `Platform.select` picks `mobile`.

The idiom is decided when the app starts and stays the same while it runs, as on every platform; a narrow desktop window is still a desktop, laid out for its `windowSize()`. `web={{…}}` props, `Platform.select({ web })` and `web:` classes apply in browsers only; `ios={{…}}`, `native` and `ios:` do not apply there.

The page reports the rest of the environment, and keeps it current:

| Value | From the browser |
|---|---|
| Locale (`Intl`'s default) | `navigator.languages[0]` |
| `I18nManager.isRTL` | `<html dir>` when set, else the locale's script |
| `reduceMotion()` | `prefers-reduced-motion` |
| `fontScale()` | the root element's font size / 16 px (the browser's text size setting) |
| `PixelRatio.get()` | `devicePixelRatio` (it changes with zoom and screens) |
| `colorScheme()` | `prefers-color-scheme` |
| `safeArea()` | `env(safe-area-inset-*)` where it overlaps the container (`viewport-fit=cover` pages on phones) |
| `keyboard()` | how much of the container the on-screen keyboard covers (`visualViewport`) |

`Platform.Version` is `"0.0"`, and `screenReaderEnabled()` stays `false` (browsers do not tell). [`StatusBar`](../components/status-bar.md)'s `web={{ themeColor }}` sets the page's `<meta name="theme-color">`, the color browsers give their bars around the page.

## Networking

`fetch` (with streams, `AbortController` and `FormData`) and `<Image>` URLs go through the browser's own network stack, after nsh has checked the app's [capabilities](./sandbox-and-capabilities.md). Since the requests are the browser's, the browser's rules apply:

- **Relative URLs** resolve against the page's URL, as in the browser: a published app reaches its own API with `fetch("/api/items")`, and `<Image src="/photos/a.jpg" />` loads from its own server. (Native platforms have no page, so they need absolute URLs.)
- **CORS.** A request to another origin than the page's only succeeds if that server allows the page's origin (`Access-Control-Allow-Origin`); otherwise `fetch` rejects with a network error. Only the response headers the server exposes are visible.
- **Cookies** (and HTTP authentication) are sent to the page's own origin only (`credentials: "same-origin"`), and headers browsers forbid (`Cookie`, `User-Agent`, `Host`, ...) are dropped.
- **Redirects** are followed by the browser. A redirected request that must not follow it fails: `redirect: "manual"` and `"error"`, and requests of an app whose capabilities restrict `network` (nsh checks every hop, and browsers do not reveal a redirect before following it).
- **Request bodies** that are a `ReadableStream` are collected first and sent in one piece; response bodies stream as they arrive.

## Split bundles

A web app does not need to download all of its code before the first screen shows. `nsh package --target web` publishes apps split, with each chunk as `chunks/<name>.<digest>.js` and the page mapping chunks to those files. To split a bundle yourself, `nsh build --split` puts the modules that `import()` loads into chunks of their own, next to `bundle.js`:

```sh
nsh build --release --split -o site/app
# bundle.js        the app up to its first screen
# Screen.js        import("./Screen") and what only it uses
# detail.js        what two chunks share, when they do (made by the bundler)
```

The first `import("./Screen")` downloads `Screen.js` and the chunks it imports, all at once, then evaluates them; later imports of it resolve at once. The page says where the chunks are: by default next to the page (`chunkBase`), or one URL per chunk, for files renamed with a content digest so a CDN can cache them forever:

```js
player.run(code, {
  chunks: { "Screen.js": "/app.3f2a9c.js", "detail.js": "/app.81bb07.js" },
});
```

A chunk that cannot be downloaded rejects the `import()` with a `TypeError` naming it; the next `import()` tries again. Chunk names are fixed file names from the bundle: the app cannot make the page download anything else.

## Limits

The web runtime is for demos and documentation. It does not have:

- **Web views, videos and native components**: `WebView` maps to an `<iframe>` and `Video` to `<video>` only in the simplest form; registered native components render an empty box.
- **Platform services**: toasts and the Android back button do nothing.
- **Custom fonts loaded before first use**: imported fonts are registered with `FontFace`, which loads asynchronously, so the first frame may be measured with a fallback font.
- **Text shaping details**: line breaking is a subset of UAX #14 (spaces, CJK, hyphens), with no hyphenation and no bidirectional reordering beyond the paragraph direction.
- **Pull to refresh and zooming scroll views.**

The player and the dev server run apps with development settings (no hardening); published apps run hardened. Both use the default [limits](./sandbox-and-capabilities.md).

## See also

- [Packaging](./packaging.md): the other targets of `nsh package`.
- [Architecture](./architecture.md): the engine, the core and the mount batches the web build reuses.
- [Testing and trace](./testing-and-trace.md): `nsh trace` runs the same engine headless, with an approximate text engine.

---
title: Web (WebAssembly)
description: Run NSH apps in a browser with the WebAssembly build of the runtime, which renders to the DOM; publish them as static sites with NSH package --target web, develop them with NSH run --target web.
order: 16
group: platforms
demo: counter
---

# Web (WebAssembly)

The NSH runtime also compiles to WebAssembly. The same engine runs in a browser page and renders to the DOM instead of UIKit or Android views: the core, the reactive graph, the renderer, Taffy layout, the `nsh/ui/mobile` components and QuickJS itself. `nsh package --target web` publishes an app as a static site for any web server or CDN ([Publish an app](#publish-an-app)), `nsh run --target web` runs it in the browser while you edit it ([Develop in the browser](#develop-in-the-browser)), and every live demo on this site is an app bundle from `nsh build` running on this runtime.

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
- **Native animations run in Rust** on `requestAnimationFrame`: `Animated` timing, spring and decay, `interpolate`, native values bound to `opacity`, `transform` and `backgroundColor`, drags bound with `pan`, scroll-linked values and canvas arguments. JS is not involved per frame, as on devices. `LayoutAnimation`, style transitions, the `animate-*` keyframes and screen transitions move as on the desktop backends, and `prefers-reduced-motion` turns them off (screens fade).
- **Input:** presses with `activeOpacity` and long presses, the responder system (`onTouch*`, `PanResponder`), native drags and two-finger pinch and rotation, scroll views (the browser's scrolling), pull to refresh, zooming scroll views (pinch, ctrl+wheel and touchpad pinches, `zoomTo`), `<Input>` with controlled values, focus and submit, `Canvas`, `Stack` and `Screen` with a header, `Modal`.
- **System UI is drawn in the page:** `Alert` and `ActionSheet` look like the desktop backends' dialogs on desktop browsers and like iOS's on phones (with the `desktop` idiom, see below), and `Toast.show` shows a toast at the bottom. Custom fonts are measured again once the browser has loaded them, and an app waits for the fonts it imports before its first frame shows.
- **Modals** are presented as on the platform the browser is like: `pageSheet` and `formSheet` (`desktop={{ presentationStyle }}` or `web={{ presentationStyle }}`) are cards centered over the dimmed page in desktop and tablet browsers, the sizes the desktop backends use, closed by Escape, the browser's back button or a click outside (each asks through `onRequestClose`); in phone browsers they rise from the bottom to just below the top, as iPhone sheets do. `fullScreen` and transparent modals cover the app; `animationType`, `onShow` and `onDismiss` work as on devices.

## Keyboard, mouse and touch

Published apps behave as desktop apps in desktop browsers and as phone apps in phone browsers:

- **Wheels and touchpads** scroll the innermost scroll view along their axis that can still move (a vertical wheel scrolls a horizontal list when nothing vertical can), and stay on it until the scroll ends; wheel notches glide, paging and snapping views move one page per scroll. Ctrl + wheel and touchpad pinches pinch views with native pinch values instead of zooming the page. Dragging with the mouse does not scroll, as on the desktop platforms: mouse drags go to pans, the responder system and text selection. Scroll views report `onScrollBeginDrag` / `onScrollEndDrag`, `onMomentumScrollBegin` / `End` and `onScrollEnd` as on the desktop platforms.
- **Keyboard**: Tab and Shift+Tab move between pressables and inputs (a focus ring and `focus:` styles show the keyboard focus); Return presses the focused pressable, Space presses it when released; arrows, Page Up / Page Down, Space, Home and End scroll the scroll view around the focus or under the mouse; Escape leaves a field, closes the dialog, then goes back. `nsh/keymap` shortcuts work in the browser, matched on the spot (`mod` is Command in Safari and Chrome on a Mac, Control elsewhere).
- **Going back**: Escape, Alt+Left (Cmd+[ on a Mac) and the mouse's back button pop the innermost stack, or ask the newest modal to close, as the Android back button does; `preventBack` and `onBackPress` work as there. When the app is the whole page, every pushed screen and presented modal is a browser history entry, so the browser's back button goes back in the app.
- **Touch**: only scroll views pan; the page itself never scrolls, zooms or pulls to refresh under the app, and taps have no delay. In an iframe, swipes the app does not take scroll the embedding page. Drags on views with native pans, pinches and the responder system are the app's.
- **Accessibility**: `accessibilityRole`, `accessibilityLabel`, `accessibilityHint`, `accessibilityState`, `accessibilityValue`, `accessibilityLiveRegion` and `accessibilityElementsHidden` become ARIA; `AccessibilityInfo.announceForAccessibility` speaks through a live region and `setAccessibilityFocus` moves the focus. Views under a modal or alert are inert.
- **Selection and menus**: in desktop browsers text is selectable by default, as on a web page, except inside interactive elements (views with press or touch handlers: buttons, pressable rows, tabs) and text with `selectable={false}` or `select-none`; dragging the mouse over it selects it, Ctrl/Cmd+C copies it line by line and Ctrl/Cmd+A selects all of the text the selection is in. In phone and tablet browsers only `selectable` text is selectable, as in native apps. The browser's context menu appears only in selectable text and inputs.


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
| `nsh.<digest>.js` | The DOM runtime, minified (79 KB, 22.5 KB with brotli) |
| `nsh.<digest>.wasm` | The engine, the core, the mobile components and QuickJS without its compiler (about 3.5 MB, 1.2 MB gzipped, 0.93 MB with brotli) |
| `app.<digest>.nshc` | The app, built as `nsh build --release --split` builds it, compiled to QuickJS bytecode: nothing is compiled in the browser, so it starts sooner |
| `harden.<digest>.nshc` | The [sandbox's](./sandbox-and-capabilities.md) hardening, compiled likewise (1.4 KB) |
| `chunks/<name>.<digest>.nshc` | What `import()` loads, compiled likewise, downloaded the first time it is imported ([split bundles](#split-bundles)) |
| `assets/<path>.<digest>.<ext>` | Every asset the app imports, loaded from its URL when it is used (fonts before the code that registers them runs, as text is measured with them: before the app starts, or with the chunk that imports them) |
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
| `gallery/nsh.<digest>.wasm` | The runtime of published apps (about 3.5 MB, 1.2 MB gzipped, 0.93 MB with brotli; built with the `web` profile), shared by the player and the gallery |
| `nsh-playground.wasm` | The same plus NSH's TSX compiler, for the [playground](#the-playground) (about 8 MB, 2.4 MB gzipped) |
| `gallery/nsh.<digest>.js` | The DOM runtime (minified): loads the wasm, applies batches, turns input into events |
| `index.html` | A player page: runs `demos/<name>.nshc` for `#<name>` |
| `demos/<name>.nshc` | Each demo compiled by `nsh build --release` |
| `gallery/` | The gallery app published with `nsh package --target web`, shown on the [Gallery](/gallery) page |

Serve the directory with any static server (it needs the `application/wasm` MIME type for streaming compilation) and open `index.html#counter`.

## The playground

The [Playground](/playground) edits and runs an app in the browser: the page compiles the TSX with NSH's own JSX compiler (built into `nsh-playground.wasm`, without rolldown) and runs it on every pause in typing. Every demo on this site has an **Edit in Playground** link, and **Share** puts the code in the link itself, so nothing is stored on a server.

A playground app is one module: it imports NSH's modules (`nsh`, `nsh/*`, `nsh/ui/mobile`) but no other files or packages. `console.log` output and errors appear below the phone.

The compiler is also a Rust function, `nsh_bundler::compile_module(name, source, ui_sets)`, available without the crate's `bundle` feature.

## Embed it in a page

`nsh.js` is an ES module. `createPlayer` loads the runtime into a container; `run` starts a bundle that `nsh build` writes: `bundle.nshc`, its bytecode, as bytes, or `bundle.js` as a string. The runtime `nsh package --target web` ships (`nsh.wasm`) has no JavaScript compiler and runs bytecode only; the development runtime (`nsh-dev.wasm` in the distribution) runs both:

```js
import { createPlayer } from "./nsh.js";

const player = await createPlayer(document.querySelector("#phone"), {
  wasm: "./nsh.wasm",
  assetBase: "./assets/",   // where `asset:/…` URIs (image imports) are served from
  // dark: true,            // force an appearance; otherwise it follows the system
  // idiom: "phone",        // force "phone", "pad" or "desktop"; otherwise the pointer and the size decide
  // history: false,        // screens and modals as browser history entries; by default only when the app is the page, not in an iframe
});
player.run(await (await fetch("./bundle.nshc")).arrayBuffer());
```

`assets` (a map from `asset:/` paths to URLs) replaces `assetBase`, and `readAsset(uri)` returns the bytes of an asset the engine needs at once (a font) or `null`; `player.run(code, { release: true, capabilities })` runs a bundle as a release under the capabilities it declares (the default is development settings); with bytecode, a release also takes the hardening's bytecode (`harden`, the bytes of `harden.<digest>.nshc`). The page `nsh package --target web` writes uses all of them.

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

`fetch` (with streams, `AbortController` and `FormData`) and `<Image>` URLs go through the browser's own network stack, after NSH has checked the app's [capabilities](./sandbox-and-capabilities.md). Since the requests are the browser's, the browser's rules apply:

- **Relative URLs** resolve against the page's URL, as in the browser: a published app reaches its own API with `fetch("/api/items")`, and `<Image src="/photos/a.jpg" />` loads from its own server. (Native platforms have no page, so they need absolute URLs.)
- **CORS.** A request to another origin than the page's only succeeds if that server allows the page's origin (`Access-Control-Allow-Origin`); otherwise `fetch` rejects with a network error. Only the response headers the server exposes are visible.
- **Cookies** (and HTTP authentication) are sent to the page's own origin only (`credentials: "same-origin"`), and headers browsers forbid (`Cookie`, `User-Agent`, `Host`, ...) are dropped.
- **Redirects** are followed by the browser. A redirected request that must not follow it fails: `redirect: "manual"` and `"error"`, and requests of an app whose capabilities restrict `network` (NSH checks every hop, and browsers do not reveal a redirect before following it).
- **Request bodies** that are a `ReadableStream` are collected first and sent in one piece; response bodies stream as they arrive.

## Split bundles

A web app does not need to download all of its code before the first screen shows. `nsh package --target web` publishes apps split, with each chunk as `chunks/<name>.<digest>.nshc` and the page mapping chunks to those files (a bundle run as bytecode loads its chunks as bytecode: `Screen.nshc` next to `Screen.js`). To split a bundle yourself, `nsh build --split` puts the modules that `import()` loads into chunks of their own, next to `bundle.js`:

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

The [gallery](/gallery) loads each demo this way: `lazy` (in `examples/gallery/ui.tsx`) makes a component that starts the import the first time it renders, shows a placeholder until it resolves, then renders what it loaded. Its first load is the index and the demo shown first; a font a demo imports comes with that demo's chunk. Native builds hold every module, so there the import resolves at once:

```tsx
const Stocks = lazy(() => import("./demos/Stocks").then((m) => m.Stocks));
```

## Limits

The web runtime is for demos and documentation. It does not have:

- **Web views of other sites**: `WebView` is an `<iframe>`. Inline HTML (`source={{ html }}`) gets everything: `onMessage`, `postMessage`, `injectJavaScript`, the injected scripts and load events (it runs with an origin of its own, apart from the page). A page of another site only gets `postMessage` and load events with the URL it was given: browsers hide its navigations, title, history and errors from the page around it, and `userAgent` cannot be set.
- **Videos with sound that start on their own**: `Video` is a `<video>` with every prop and event, but browsers only start a video with sound after a user gesture (muted videos play).
- **Native components**: registered native components belong to the native apps that embed NSH; on the web their view stays empty, with a warning in the console.
- **Platform services**: reading the clipboard (`Clipboard.getString`, also `hasString`) needs a secure context (HTTPS) and may ask the user. Images do not report `onProgress`.
- **Text shaping details**: line breaking is a subset of UAX #14 (spaces, CJK, hyphens), with no hyphenation and no bidirectional reordering beyond the paragraph direction.

The player and the dev server run apps with development settings (no hardening); published apps run hardened. Both use the default [limits](./sandbox-and-capabilities.md).

## See also

- [Packaging](./packaging.md): the other targets of `nsh package`.
- [Architecture](./architecture.md): the engine, the core and the mount batches the web build reuses.
- [Testing and trace](./testing-and-trace.md): `nsh trace` runs the same engine headless, with an approximate text engine.

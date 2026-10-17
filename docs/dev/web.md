# The web runtime (WebAssembly)

`crates/nsh-platform-web` runs nsh apps in a browser page, rendering to the DOM: apps published with `nsh package
--target web`, the web development host (`nsh run --target web`) and the website's live demos (`website/demos/*.tsx`).
`Platform.OS` is `"web"` (`Os::Web`).
User-facing page: `website/docs/web.md`.

## Building

`cargo xtask web` builds `nsh.wasm` (`--target wasm32-unknown-unknown --profile web`: the release profile optimized for size, QuickJS still at `-O3`), writes
`web/index.html` (the player, loading the gallery's runtime files: see "What a published app downloads"), `nsh-playground.wasm` (feature `compiler`, built in `target/web-playground` so the two
builds keep their caches), and bundles each demo with `nsh build --release` into `website/public/play/` (generated,
ignored by git), and publishes `examples/gallery` with `nsh package --target web` into `play/gallery/` (the website's
Gallery page frames it). Check the crate with `cargo clippy -p nsh-platform-web --target wasm32-unknown-unknown`: off
WebAssembly it is empty. `.cargo/config.toml` gives wasm32 an 8 MiB stack (QuickJS recursion).

QuickJS is C, so the build needs a clang that targets wasm32 and an `llvm-ar` (CI: Ubuntu's `clang` and `llvm`, with
`AR_wasm32_unknown_unknown=llvm-ar`). Apple's clang has no WebAssembly target: on macOS point
`CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown` at an LLVM toolchain's `clang` and `llvm-ar` (Homebrew's
`llvm`, or the Android NDK's `toolchains/llvm/prebuilt/*/bin`).

## The CLI's web target

`crates/nsh-cli/src/web.rs`. The CLI finds the runtime as it finds the others (`runtime::web`): `runtime/web/nsh.wasm`
and `runtime/web/nsh.js` in the distribution (`cargo xtask dist` puts them there); in a checkout it builds
`nsh-platform-web` with `--profile web --target wasm32-unknown-unknown` whatever its own profile (a debug build is
several times larger and slower; `AR_wasm32_unknown_unknown` defaults to `llvm-ar`) and takes `nsh.js` from the crate.

- **`nsh package --target web`** writes a static site. Every file but `index.html` and `manifest.webmanifest` is
  content-addressed (`<stem>.<10 hex digits of SHA-256>.<ext>`, `web::hashed_name`), so hosts cache them for good
  (`immutable`) and only the page is revalidated. The page (template `crates/nsh-cli/src/web/index.html`) imports the
  hashed `nsh.js`, preloads the wasm, the app's code and its fonts, and passes `createPlayer` the wasm URL, `assets`
  (each `asset:/<path>` to its hashed URL: images and videos load from there on demand) and `readAsset` (the fonts'
  bytes: the engine reads fonts synchronously, `Assets::read`, to measure text with them). Fonts the entry or the UI
  sets register are fetched before the app starts; one only chunks register (`font("asset:/…")` in their code) comes
  with the first of them, through `createPlayer`'s `prepareChunk(id)`, which `nsh.js` awaits with the chunk's download
  (`web::font_loads`). It runs the bundle with `run(code, { release: true, capabilities, chunks })`; an app whose capabilities restrict `network`
  gets a CSP `connect-src` meta with `'self'` and the declared hosts (`web::connect_src`, a host without a port as
  `host:*`), as the browser follows redirects before nsh could check them. Relative URLs resolve against the page
  (`document.baseURI`, `nsh_set_base_url`). The app's code ships as JS source
  (`web::CODE`; the web build's QuickJS is 32-bit, so the CLI's bytecode would not load there). The app is built split
  (`build::compile_split`): each chunk is written as `chunks/<name>.<digest>.js` and the page's `chunks` maps its id to
  that URL; nothing preloads them, the runtime fetches one the first time `import()` needs it. Apps with a Rust crate of their own are refused: the prebuilt wasm has only
  `nsh/ui/mobile`. `--run` serves the directory with `nsh_dev::SiteServer` (the cache headers a CDN should send) and
  opens the browser.
- **The development host** is a page the dev server serves (`DevServer::serve_web`: the page at `/`, `nsh.js` and
  `nsh.wasm` next to it, all `no-cache`), from `crates/nsh-cli/src/web/dev.html`. `nsh dev` serves it whenever a web
  runtime exists (`runtime::web(false)`), `nsh run --target web` builds one first and opens the browser. The page
  speaks the dev protocol over the same port's WebSocket: its hello has platform `web` and an empty
  `bytecode_version` (the server then sends JS source, without the warning a mismatched version gets), it applies
  patches with `player.hotUpdate` (`nsh_hot_update`; when it reports a failure, `nsh_hot_failed`, the bundle that
  follows loads from scratch), shows build errors and `CannotRun` over the app, sends the app's log lines back, and
  reads fonts with a synchronous `XMLHttpRequest` from `/assets/`.

`nsh_bundle_options` sets how the next `nsh_start` runs: development settings by default (the player, the
playground, the dev host), hardened (`Policy::default()`) for published apps, the declared capabilities and a source
map.

### What a published app downloads

`examples/gallery` as `nsh package --target web` publishes it (2026-10; `gzip -9`, `brotli -q 11`). Each demo is a
module the index loads with `import()` (the gallery's `lazy`), so it is a chunk of its own: the first load is the page,
the runtime, the app's shell and the demo shown first (Stocks, in the desktop layout; none in the phone layout's
index). The other demos, 18 chunks of 265 KB (72 KB gzipped) in all, and the fonts they register (Pacifico, with
the Text demo: `web::font_loads`, the page's `prepareChunk`) come when opened; images when shown, icons when the
browser wants them.

| File | Raw | gzip | brotli |
|---|---:|---:|---:|
| `index.html` (with the chunk map) | 5.7 KB | 2.6 KB | 2.1 KB |
| `nsh.<digest>.js` (minified) | 57.5 KB | 18.9 KB | 17.1 KB |
| `nsh.<digest>.wasm` | 3779 KB | 1311 KB | 1001 KB |
| `app.<digest>.js`: the shell (minified by rolldown) | 34.9 KB | 9.7 KB | 8.5 KB |
| `chunks/Stocks.<digest>.js` | 55.9 KB | 17.3 KB | 15.4 KB |
| **first load** (with the 32 px favicon) | **3934 KB** | **1361 KB** | **1046 KB** |
| before the demos were split: `app.<digest>.js` 347 / 92.5 / 74.9 KB and the preloaded font 329 / 145 / 121 KB | 4518 KB | 1571 KB | 1217 KB |
| `manifest.webmanifest` | 0.4 KB | 0.3 KB | 0.2 KB |
| images (`bounce.gif`, `logo@2x.png`, `spinner.webp`) | 16.1 KB | 6.0 KB | 5.6 KB |
| icons 180 / 192 / 512 px | 114 KB | 89.4 KB | 86.9 KB |

The app's own share of the first load went from 676 KB (195 KB with brotli) to 91 KB (24 KB). The wasm is now nearly all of the first load. `nsh.js` is minified at package time
(`nsh_bundler::minify_module`: oxc's minifier, local names mangled, property names kept, since the mounter looks its
`op_<name>` methods up by name); the dev host serves it as it is. Published sites carry no source maps.

The website's player (`cargo xtask web`) loads the published gallery's `nsh.<digest>.js` and `nsh.<digest>.wasm`
(`play/gallery/`) rather than copies of its own, so the site deploys one runtime; only the playground has a second
wasm (`nsh-playground.wasm`, with the compiler).

## Size

`nsh.wasm` is downloaded before anything shows, so the `web` profile builds Rust for size (`opt-level = "z"`, fat LTO,
`panic = "abort"`, stripped) while QuickJS keeps `-O3`: at `-Os` it is 0.18 MB smaller (50 KB with brotli) but its
interpreter is about 10% slower, and QuickJS is the slowest link. `nsh-platform-web` turns on rquickjs'
`disable-assertions` (QuickJS' internal `assert`s, which check its own invariants: 37 KB, and a little faster); the
native platforms keep them.

Measure with names kept (`CARGO_PROFILE_WEB_STRIP=false`) and `twiggy top` / `twiggy dominators`, or a linker map
(`RUSTFLAGS="-C link-arg=--Map=map.txt"`, which also sizes the data segment by symbol). Compare raw, `gzip -9` and
`brotli -q 11` sizes: pages are served compressed, and a change can shrink the raw module and not the compressed one.

## What WebAssembly changes in the shared crates

- **No threads.** `nsh_engine::LocalEngine` drives the engine from the host's event loop: `pump()` handles what was
  sent through its `EngineControl`, runs timers and tasks, commits, and returns its next deadline. The threaded
  `Engine` and `LocalEngine` share `Driver` (setup, `deadline`, `handle`, `turn`, idle collection); `Engine` and the
  shared tokio runtime are `cfg(not(target_arch = "wasm32"))`.
- **No clock.** `std::time::Instant::now` panics on `wasm32-unknown-unknown`. Shared code uses
  `nsh_core::time::Instant` (std's elsewhere; `performance.now()` through the `nsh.now_ms` import there) and
  `nsh_core::time::unix_ms`. Rust components' `cx.timer()` returns `nsh_core::time::Sleep` there, whose wakers the
  driver fires (`next_timer`, `fire_timers`); `background` / `blocking` do not exist.
- **QuickJS** uses rquickjs' pregenerated bindings (its bindgen run misses `JS_ToCStringLen2` for wasm32) and its
  vendored wasi-libc; the only libc import is `env.__rquickjs_host_now_us`.
- **32-bit pointers.** The reactive graph packs node handles into QuickJS opaque pointers: 32 bits of id and 32 of
  generation on 64-bit targets, 20 and 11 (under a set top bit) on 32-bit ones (`reactive.rs`, `pack`). Anything that
  stores more than a pointer's worth of bits in an opaque must do the same.
- `nsh-package` cannot generate keys there (no `getrandom`); verifying works.

## The playground's compiler

Feature `compiler` adds `nsh_compile` (and `nsh_compiled_ptr` / `_len`): `nsh_bundler::compile_module`, the one-module
path of the bundler (`crates/nsh-bundler/src/compile.rs`: oxc parse, TypeScript removal, nsh's JSX compiler,
codegen). Rolldown does not build for wasm32, so `nsh-bundler` puts bundling behind its default feature `bundle`
(rolldown, tokio, assets, `.nsh/types`); the workspace depends on it without, and the CLI and the engine's tests
turn it on. The compiler grows the module by about 3 MB, so the docs pages load `nsh.wasm` and only the playground
`nsh-playground.wasm`.

oxc's transformer links `oxc-browserslist`, whose wasm32 build depends on `js-sys`: the module imports wasm-bindgen
glue (`__wbindgen_placeholder__`, `__wbindgen_externref_xform__`) that nsh never calls (it only reads dates for
browserslist queries). `nsh.js` gives every import it does not know a stub that throws if called.

## The environment

`nsh.js` reports what the page knows with `nsh_environment(json)` before `nsh_start` and whenever it changes (only
when the JSON differs): `idiom`, `locale` (`navigator.languages[0]`), `rtl` (`<html dir>`, else the locale's script),
`reduceMotion`, `fontScale` (the root font size / 16), `pixelRatio`, `safeArea` (`env(safe-area-inset-*)` read
through a hidden probe element, clipped to the container) and `keyboard` (the container's part below
`visualViewport`). The Rust side keeps it (`PAGE`) for the next start; the appearance and the window keep their own
calls (`nsh_set_dark`, `nsh_resize`). Changes come from media query listeners, `languagechange`, `visualViewport`,
`resize` and a mutation observer on `<html dir lang>`.

The idiom is fixed at start, as `Environment::idiom` is everywhere: `desktop` for a fine, hovering pointer and a
container at least 600 wide (headless Chromium has no pointer: it measures as a tablet or phone), `pad` for a screen
whose short side is at least 600, else `phone`; the player's `idiom` option overrides it. `Environment::desktop()`
(desktop OS, or the web in the desktop idiom) decides `desktop={{…}}` props, `Platform.select`'s `desktop` key and
`Platform.isDesktop`; `Os::is_desktop()` stays the desktop backends' OSes (`nsh/desktop`'s windows and menus). A
desktop-idiom page gets the desktop backends' header heights (47 / 94). `Command::StatusBar`'s `background` (from
`StatusBar`'s `web={{ themeColor }}`) becomes `<meta name="theme-color">`.

## The protocol

Exports are `nsh_*` functions (`src/lib.rs`); imports are module `nsh` (`src/ffi.rs`). Every export that reaches
the engine pumps it and returns the next delay in milliseconds (-1: none), which `nsh.js` turns into a `setTimeout`.
The engine is not reentrant: `nsh.js` routes every call through `Player.call`, which defers calls made while a
batch is being applied (a batch arrives inside a wasm call).

A commit becomes one JSON array (`src/encode.rs`): `["c", id, kind]`, `["f", id, x, y, w, h]`, `["s", id, css,
borderCss, tint]`, `["t", id, ...content, runs, styles, selectable]`, and so on; `nsh.js` has one `op_<name>` method per
item. Views are absolutely positioned at the core's frames; borders are an overlay element (as UIKit draws layer
borders) so they never move the content the core placed. Native animation items stay in Rust (`src/animation.rs`, a port
of the Android animator); bound properties go out as `["a", id, prop, value]` after each batch and each
`requestAnimationFrame` tick.

A split bundle's chunks (`native-pipeline.md`) are fetched by the page: the engine asks through the `load_chunk`
import (`Sink::load_chunk`), `nsh.js` fetches `chunks[id]` or `id` relative to `chunkBase` (`player.run(code,
{ chunks, chunkBase })`) and hands the source to `nsh_chunk(id, code, ok)` (`ok` 0: `code` is the error), which
pumps. A chunk that arrives after `run` started another app is dropped. Chunks are source; the engine also takes
bytecode chunks (`Chunk::bytecode`), which the page does not send yet.

Text (`src/text.rs`) is broken into lines here, with widths and font metrics from the `measure_text`,
`font_ascent` and `font_descent` imports (canvas `measureText`, cached per font and string); runs are drawn at the
positions computed here with a line height equal to the font's ascent + descent, so the baseline lands where layout
put it.

## Networking

`fetch` runs on the browser's `fetch` (`src/net.rs`, the `fetch_*` imports and `fetchStart` in `nsh.js`), behind the
engine's `Net` like every platform's stack, so the app's capabilities are checked in Rust before a request reaches
the browser. JS reports each request's head, body chunks (read from the response's `ReadableStream`), end or failure
through the `nsh_fetch_*` exports, which hand them to the request's `FetchEvents` and pump; `pause` stops reading the
reader (back pressure), dropping or cancelling the task aborts its `AbortController`. Streamed request bodies are
collected and sent as one `Blob`: Chromium streams uploads only over HTTP/2 (`duplex: "half"`), other browsers not at
all.

The browser stays in charge of what it enforces: CORS (cross-origin servers must allow the page's origin, and only
CORS-exposed response headers are visible), cookies (`credentials: "same-origin"`: sent to the page's own origin
only), forbidden request headers (`Cookie`, `User-Agent`, ... are dropped). Browsers do not expose a redirect they
do not follow (`redirect: "manual"` gives an opaque response without status or `Location`), so a hop with
`follow_redirects` off fails when it is redirected: JS `redirect: "manual"` / `"error"`, and the hops the engine
follows itself to check each one against restricted capabilities (`access.rs`).

Relative request URLs (`fetch("/api")`, `new Request("x")`) resolve against the page's `document.baseURI`, which
`nsh.js` hands over once with `nsh_set_base_url` and `nsh_start` puts in `Bundle::base_url` (→ `HostOptions::base_url`,
used by nsh-js `fetch.rs`); native hosts have no base and keep rejecting them. `new URL("/x")` without a base throws,
as in browsers. Relative `<Image>` URLs are the `<img>` element's, resolved by the browser.

`Image.getSize` / `prefetch` (`Command::ImageInfo`) load an `Image` in the page and answer with `onImageInfo` on
the root; `<Image>` elements are `<img>` elements, loaded by the browser.

## Input

`nsh.js` turns browser input into what the desktop backends report (`docs/dev/linux.md` is the reference), and phone
browsers' touches into what phones report:

- **Touches** pan only scroll views: the root is `touch-action: none` (`pan-x pan-y` in an iframe, `nsh-embedded`,
  so that swipes the app does not take scroll the embedding page), scroll views `pan-x pan-y`, views with native
  pans or pinches `none` (a `panActivation` gate along one axis leaves the other axis to the scroll views around), so
  the page neither scrolls, zooms nor refreshes under the app and presses and pans get no `pointercancel`. Touch
  scrolling is the browser's: a `pointercancel` in a scroll view starts its drag, `touchend` ends it, a scroll within
  150 ms after it begins the glide, `scrollend` ends it (`touchScroll`, `scrolled`, `scrollEnded`).
- **Wheels and touchpads** (`wheel`) go to the innermost scroll view along their axis that can move that way (a
  vertical wheel scrolls a horizontal view when no vertical one can) and stay on it while events come within 200 ms;
  notches glide, touchpads follow the fingers; snapping views move one snap point per scroll. Nothing to scroll: the
  event goes on to the page around (an embedding page scrolls). Ctrl + wheel (a touchpad's pinch) pinches the view
  under the pointer and never zooms the page. Mouse drags scroll as before, with momentum, and snapping views settle
  on the next snap point along a flick. Scroll events follow the desktop backends: drags send `onScrollBeginDrag` /
  `EndDrag`, glides and snaps `onMomentumScrollBegin` / `End`, every scroll ends with one `onScrollEnd` (the browser's
  `scrollend` after a scroll driven here, `drive` / `ownEnd`, is not another).
- **Keys** (`keyDown`), in this order: the app's shortcuts (`nsh/keymap`: the mounted `MountItem::Keymap` stays in
  Rust, `nsh_key` matches a key on the spot with `Keymap::find` and `nsh_key_command` reports the binding; keys are
  passed as the character they type without modifiers, else the US layout's character of their `code`; `mod` was
  resolved by the browser's platform, `appleKeyboard` in the environment JSON); an open dialog (Escape presses its
  cancel button); a focused field (Escape leaves it); selectable text with a selection (select all, Escape); a focused
  pressable (Return presses it, Space when released); going back (Escape, Alt+Left, Cmd+[, BrowserBack); scrolling the
  scroll view around the focus or under the mouse (arrows by 40 points, Page Up / Down and Space by 0.875 of a page,
  Home / End).
- **Focus**: pressables are focusable (`tabindex=0`, role `button` unless they have one) unless disabled or hidden,
  so Tab and Shift+Tab reach them in DOM order; `focus:` states follow the keyboard focus (`:focus-visible`), which
  draws a ring inside the view; a popped screen gives the focus back to what had it when the screen was pushed
  (`screenGone`). A click never moves the focus (`mousedown` is prevented outside fields, selectable
  text and hosted pages), as in native apps; it focuses the app's window in an iframe. The views under the newest
  modal or dialog are `inert`; the focus a modal or dialog took comes back when it closes.
- **Accessibility**: `op_vp` sets ARIA from the accessibility props (roles mapped by `ARIA_ROLES`, `aria-disabled`,
  `-selected`, `-checked`, `-busy`, `-expanded`, `-value*`, `-live`, `-hidden`, the hint as `aria-description`);
  `Command::Announce` writes into a polite live region, `Command::AccessibilityFocus` focuses the view.
- **Going back** (`back`) is decided from what is mounted, as on Android and the desktops: a stack inside the newest
  modal pops, else the modal asks to close (`onRequestClose`; a `dismissible` one closes and reports `onDismiss`);
  without a modal the root's `onBackPress` hears it when the app listens, else the innermost stack pops (a screen
  with `preventBack` gets `onBackRequest`). `Command::Back` goes back as without the app's listeners.
- **History**: when the app is the page (`window.top === window`, or `createPlayer`'s `history` option) every screen
  above a stack's first and every modal is a history entry (`pushState`, state `{ nsh: depth }`), so the browser's
  back button and the mouse's go back in the app; entries the app takes away itself are gone back over with
  `history.go`, whose `popstate` is ignored, and a back the app refuses gets its entry again; a new `run` goes back
  to the first entry. Entries carry the page load's id (`nshSession`): a reload keeps the earlier load's entries,
  which going back passes over (`history.back()` again) as if the app had been left from its first screen. In an
  iframe (the website's demos) the app pushes nothing: each entry would be one of the embedding page's history.
- **Elsewhere**: no page context menu or iOS callout outside fields and selectable text; a mouse drag selects only in
  selectable text and fields; overflow is `clip`, never `hidden`, so that the browser cannot scroll a box the core
  laid out to show a focused field.

## Fonts

`register_font` adds a `FontFace`, which loads asynchronously even from bytes: until it has, `measureText` measures
with a fallback font. When the faces settle, `nsh.js` calls `nsh_fonts_loaded`, which clears `WebTextEngine`'s caches
(the core's text engine is an `Rc` the app keeps) and `EngineControl::fonts_changed` (`Core::fonts_changed`: every
text is measured and laid out again, as after `register_font`). Faces registered while the app starts (its imports'
fonts, in `nsh_start`) keep the root hidden until they load (at most 3 s), so the first frame shows the right fonts.

## Scroll views: pull to refresh and zooming

A zoomable (`maximumZoomScale` above `minimumZoomScale`) or refreshable scroll view moves its children into a layer
(`ensureLayer`) that is translated (the pull, `centerContent`) and scaled (the zoom), so the core's frames stay the
unzoomed ones; the spacer that gives the browser its scroll range is the zoomed content size plus the pull. Offsets
reported to the core are the zoomed ones (as UIKit's), minus the pull (negative while pulled, as on the native
backends); zoom changes go to `nsh_zoom` (`EngineControl::zoom`) before the offset.

- **Pull to refresh** (vertical views with `onRefresh` or `refreshing`): past the top, a drag moves the content down
  at half speed over an 8-spoke spinner; released past 64 points it fires `onRefresh` and keeps 56 points for the
  turning spinner until `refreshing` turns false (the Linux and macOS backends' numbers). Mouse drags go through the
  pointer handling (which scrolls mouse drags anyway); touches through non-passive touch events on the view, since
  the browser scrolls touches itself: a move past the top is prevented while it can be, and a gesture the browser
  already scrolls stays the browser's.
- **Zooming**: a zoomable view is `touch-action: none` and pans (both axes, with a glide) and pinches itself, keeping
  the content point between the fingers between them (rubber band past the limits with `bouncesZoom`, a spring back
  after). Ctrl+wheel over it zooms around the pointer instead of pinching (Chromium and Firefox send touchpad pinches
  as that; a mouse notch is 1.25×), Safari's touchpad pinches come as `gesture*` events. `zoomTo` and `zoomScale` animate or jump with the
  content point to the middle. Canvases inside are drawn again at the zoomed resolution when a zoom ends (up to 4×).

## Platform services

- **Dialogs** (`Alert`, `ActionSheet`) are drawn in the root: with the `desktop` idiom as the Linux and macOS
  backends' cards (up to three buttons in a row, the last regular one the default), otherwise as iOS's alert and
  action sheet; their keys and focus are under Input. Only clicks pressed inside the dialog count: a tap's
  compatibility click, which lands after the press that opened it, must not dismiss it.
- **Toasts** (`Toast.show` from `nsh/android`, which the desktop backends also show) are a pill at the bottom of the
  root for 2 or 3.5 s, also read out through the live region `Command::Announce` writes into.
- **The clipboard**: `navigator.clipboard`, with a copy command as the fallback for writes. Reading (also for
  `hasString`, since browsers cannot tell without reading) needs a secure context and may ask the user.
- **Images**: `<img>` with `object-fit` (`center` is `scale-down`), `defaultSource` as a second image until the load
  (faded over `fadeDuration`), `blurRadius` as a CSS blur, a failed image hidden (`Image.getSize` / `prefetch`:
  under Networking). `onProgress` is not reported.
- **Web views** are `<iframe>`s. HTML loads as `srcdoc` in a sandbox without `allow-same-origin` (an origin of its
  own: it cannot reach the player's page), after a bridge script (`webBridge`): the first message from the player
  carrying a random token hands it a `MessageChannel` port, over which `ReactNativeWebView.postMessage` reaches
  `onMessage` and `injectJavaScript` and `goBack` / `goForward` run; the page's own `message` listeners never see that
  message. The injected scripts are `<script>`s at the start and end (after a doctype). A URL loads as it is, keeping
  its origin unless it is the player's own (which must not reach it either); the browser hides what such a page
  does, so it only gets `postMessage` (`window.onmessage`) and load events with the URL it was given. `reload` loads
  the frame again. Navigations inside a frame are not checked against the `webview` capability (the core checks the
  source it is given).
- **Videos** are `<video>`s: `onLoadStart`, `onLoad` (`loadedmetadata`), `onProgress` on a timer at
  `progressUpdateInterval` while playing, `onBuffer` (`waiting` / `playing` / `canplay`), `onEnd`, `onError`; a
  deleted video is stopped (a detached media element plays on). Browsers only start a video with sound after a user
  gesture (a warning says so).
- **Registered native components** are the native apps' own: their view stays empty, with a warning once per name.

## Not implemented

Platform services other than the theme color, toasts, dialogs and the clipboard; image download progress; a web
view's events and commands for pages of other sites (beyond `postMessage` and loads), user agents. A published app
ships JS source, not bytecode, and no source map.

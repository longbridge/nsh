# The web runtime (WebAssembly)

`crates/nsh-platform-web` runs nsh apps in a browser page, rendering to the DOM. It exists for the website's live
demos (`website/demos/*.tsx`), not as a target platform: it reports `Platform.OS` as `"ios"`.
User-facing page: `website/docs/web.md`.

## Building

`cargo xtask web` builds `nsh.wasm` (`--target wasm32-unknown-unknown --profile web`: the release profile optimized for size, QuickJS still at `-O3`), copies `web/nsh.js` and
`web/index.html` (the player), `nsh-playground.wasm` (feature `compiler`, built in `target/web-playground` so the two
builds keep their caches), and bundles each demo with `nsh build --release` into `website/public/play/` (generated,
ignored by git). Check the crate with `cargo clippy -p nsh-platform-web --target wasm32-unknown-unknown`: off
WebAssembly it is empty. `.cargo/config.toml` gives wasm32 an 8 MiB stack (QuickJS recursion).

QuickJS is C, so the build needs a clang that targets wasm32 and an `llvm-ar` (CI: Ubuntu's `clang` and `llvm`, with
`AR_wasm32_unknown_unknown=llvm-ar`). Apple's clang has no WebAssembly target: on macOS point
`CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown` at an LLVM toolchain's `clang` and `llvm-ar` (Homebrew's
`llvm`, or the Android NDK's `toolchains/llvm/prebuilt/*/bin`).

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

`Image.getSize` / `prefetch` (`Command::ImageInfo`) load an `Image` in the page and answer with `onImageInfo` on
the root; `<Image>` elements are `<img>` elements, loaded by the browser.

## Not implemented

Refresh controls, zooming scroll views, registered native components, the status bar and other
platform services, fonts that are usable before `FontFace` finishes loading.

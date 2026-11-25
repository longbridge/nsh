---
title: Architecture
description: "How an NSH app runs: the build pipeline, the engine and main threads, the path from a signal change to native views, the native reactive graph, text layout and the crates."
order: 63
group: tools
---

# Architecture

This page explains what happens between `count.set(1)` and a label changing on screen, in a Rust or TSX app.
You don't need it to write apps. It explains why components run once, why animations stay smooth while JS is busy,
and why the main thread never waits for your code.

## The big picture

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/architecture-light.svg" alt="How an nsh app runs: the nsh CLI builds App.tsx into bytecode; on the engine thread, QuickJS runs a handler, the reactive graph in nsh-js queues ops, and nsh-core commits one MountBatch; it crosses to the main thread in one hop, which applies it to UIKit or Android views; touches go back as events. Native animations run on the main thread without JavaScript.">
<img class="diagram diagram-dark" src="/diagrams/architecture-dark.svg" alt="How an nsh app runs: the nsh CLI builds App.tsx into bytecode; on the engine thread, QuickJS runs a handler, the reactive graph in nsh-js queues ops, and nsh-core commits one MountBatch; it crosses to the main thread in one hop, which applies it to UIKit or Android views; touches go back as events. Native animations run on the main thread without JavaScript.">
</div>

## Building: only your code becomes JS

`nsh build`, `nsh dev` and the other commands bundle your app with a toolchain written in Rust (no Node.js):

1. **Oxc** removes the TypeScript types.
2. **NSH's JSX compiler** turns JSX into direct calls to the native renderer, in the same pass. Static props and
   children are set once. Expressions that can change (they call a function or read a property) become bindings,
   which the renderer updates when their inputs change. Components become plain function calls. Their props object
   uses getters for props that can change.
3. **rolldown** bundles your modules into one ES module. Imports of NSH's modules (`nsh`, `nsh/mobile`,
   `nsh/base`, ...) stay imports, because the runtime serves those modules natively. **The bundle contains only your
   app's code.** The counter example is under 1 KB of JS.
4. The bundle is compiled to **bytecode** (`bundle.nshc`), which loads faster than source. A **source map** covers
   all steps, so stack traces point at your `.tsx` lines.

Images and fonts you import are collected as assets, and type declarations for your editor are written to
`.nsh/types`.

A Rust app has no bundle. Cargo builds each target's runner with your crate linked in. The root component named in
`nsh.json` runs natively on the engine. The rest of this page applies to Rust apps too. The only difference is that
your components run as Rust code instead of in QuickJS.

## Running: two threads per app

Each running app is one **engine**. On iOS it is an `NshAppView`, on Android an `NshView`, and in a packaged app a
window. Each engine has its own **engine thread** and, for TSX, its own QuickJS VM. There are no global singletons, so
several apps can run side by side in one host app.

### The engine thread

Everything about your app's logic happens here:

- **Your code** runs here: components, event handlers, timers, effects. Rust components run natively; TSX runs in
  **QuickJS**.
- **The reactive graph** is native. It holds signals, computed values, effects, owners and context. Its nodes are
  QuickJS objects of native classes, so the garbage collector can see references through them. Dependency tracking
  and update scheduling run in Rust and follow the algorithm of Solid 1.9.
- **The renderer** that your compiled JSX calls is native. It creates nodes and turns every change into an **op** (one
  small native instruction), pushed straight into a native queue. There is no JSON and no message per prop.
- **The core** keeps a shadow tree of all nodes. When a JS turn ends, it takes all queued ops at once and **commits**.
  A commit:
  - renders the Rust components (those of `nsh/base` and the app's own);
  - resolves `env()` insets in padding and margin;
  - rebuilds changed texts;
  - lays out with **Taffy** (flexbox and grid, column direction by default, no shrinking);
  - diffs frames and styles;
  - produces one **MountBatch**: the list of operations the native views need.

### The main thread

The platform's main (UI) thread applies each MountBatch in one hop. It creates and inserts views, sets styles and
frames, and hands over laid-out text. Apart from that, it does only the work that must happen on the main thread:

- **drawing text** that was already laid out on the engine thread;
- **native animations**: animated values from `nsh` are driven frame by frame (a `CADisplayLink` on iOS, a UI-thread
  animator on Android) and written to the views directly;
- **gestures and scrolling**: touches become events sent to the engine thread; `scrollValue` and `pan` feed animated
  values in the same frame.

**The main thread never waits for JS.** Sometimes the system needs an answer at once: may this text change, may the
back button pop this screen, may this sheet close. NSH mounts that answer beforehand as a prop. JS learns what
happened afterwards through an event. A busy engine thread can delay updates. It cannot freeze scrolling, animations
or system gestures.

## From a tap to the screen

```text
 main thread               engine thread                                              main thread
 ───────────               ─────────────────────────────────────────────────────      ───────────
 tap on a view ─ event ─▶  handler runs: count.set(count() + 1)
                           reactive graph: the bindings that read count() run again
                           renderer: one op per changed binding → native queue
                           JS turn ends → core commit: render, text, layout, diff
                           → MountBatch (e.g. 1 Text item)                 ─ hop ─▶   apply: draw the new text
```

In the counter example, a tap costs about 0.12 ms for JS and commit together, in a release build on a Mac. It
produces a handful of mount items (see [Testing](./testing-and-trace.md)).

In a Rust app the handler is a Rust closure (`count.set(count.get() + 1)`). The same graph, commit and batch follow,
with no interpreter involved.

Three properties follow from this design:

- **One turn is one commit.** However many signals a handler sets, the views change once, together.
- **Components run once.** Only bindings re-run. Components never re-render and there is no virtual DOM diff. The
  only diffing left is the core's diff of frames and styles, in Rust.
- **Only changes travel.** Unchanged styles and frames are not sent. New views get no default values.

## Text: laid out once, off the main thread

Text is often the most expensive part of a UI. NSH lays it out on the engine thread with the platform's own text
engine, so it looks exactly like native text. Layout has three steps:

1. **prepare**: a `<Text>` and its nested spans become one attributed string (CoreText on Apple platforms,
   `PrecomputedText` / `StaticLayout` on Android).
2. **measure** at the widths layout asks for. Measurements are cached by content and width, so a color-only change does
   not measure again.
3. **layout** at the final size.

The main thread receives the immutable result and only draws it. A `<Text>` nested in another `<Text>` is a span, not
a view. Strings outside `<Text>` render nothing.

## Lists

`List` is virtualized natively: only rows near the viewport exist as views. The core picks those rows, computes
positions, columns and sticky headers, and recycles cells. When a cell needs a different item, the runtime rebinds it,
so your row function's `item()` and `index()` accessors change. Frames that reveal no new row call no JS at all. See
[Lists](./lists.md).

## Everything else the runtime does

- **Networking**: `fetch` and the other global APIs are native classes. Requests use the platform's HTTP stack
  (`NSURLSession`, `HttpURLConnection`) and stream in both directions.
- **Images** download and decode off the main thread, at the size of the view, with a memory cache.
- **Limits and capabilities**: the runtime enforces them in Rust. See [Sandbox and capabilities](./sandbox-and-capabilities.md).
- **Hot module replacement** is native too. The runtime applies a patch in well under a millisecond (see
  [Dev server and debugging](./dev-server-and-debugging.md)).

## Three layers

NSH's UI has three layers, like GPUI Kit's:

| Layer | NSH | GPUI Kit | What it is |
|---|---|---|---|
| Primitives | `nsh-core` and the runtime (`nsh`, `nsh/mobile`, `nsh/desktop`) | `gpui` | The host elements, styles, layout, rendering, reactivity, the environment |
| Base components | `nsh-base` (`nsh/base`) | `gpui-base` | Unstyled components that own behavior, state, focus, the keyboard and accessibility, and take their look from the app |
| Component library | your app's, or the community's | `gpui-component` | Styled components: buttons, cards, rows with a look |

The first two layers are Native Shell. The third is yours. You can build your own component library on `nsh/base`
and the primitives, in TSX or as Rust components in [your own crate](./rust.md). You can also use a community
library, or style the UI directly in the app.

## The crates

NSH is a Rust workspace. You never build it to write apps, because the distribution ships the CLI and prebuilt
runtimes. The crate names still show up in logs and docs:

| Crate | What it is |
|---|---|
| `nsh-core` | The platform-independent core: shadow tree, styles, Taffy layout, text merging, list virtualization, mount items; CSS values and Tailwind; the native animation graph (values, interpolation, timing / spring / decay); the Rust components' framework and theme; the runtime's Rust modules (`nsh/mobile`'s `BackHandler`, ...). |
| `nsh-base` | `nsh/base`, the unstyled base components (NSH's `gpui-base`): navigation, dialogs, controls and the status bar; Rust apps use it as `nsh_base`. |
| `nsh-js` | The QuickJS host and NSH's runtime: the reactive graph, the renderer, every native module, the global APIs, hot module replacement, limits; and the TypeScript declarations of all of it. |
| `nsh-engine` | The engine thread: drives `nsh-js` and `nsh-core` and hands one batch per frame to the platform. |
| `nsh-package` | Signed packages: keys, certificates, revocation, the trust store. |
| `nsh-bundler` | The bundler: rolldown, Oxc, NSH's JSX compiler, source maps, Tailwind class checks. |
| `nsh-dev` | The dev server and its client. |
| `nsh-platform-ios` | The UIKit backend (iOS, simulator, Mac Catalyst): view mounting, the CoreText text engine, input; the embedding SDK. |
| `nsh-platform-android` | The Android backend: `libnsh.so` and the Java runtime (`NshView`, the views, images, networking). |
| `nsh-cli` | The `nsh` command. |

The dependency direction is `nsh-core` ← `nsh-js` ← `nsh-engine` ← platforms. `nsh-base` builds on `nsh-core`. The
core depends on no platform and no JS engine. This is why `nsh test` can run the real runtime headless on any machine.

## See also

- [Performance](./performance.md)
- [Reactivity](./reactivity.md)

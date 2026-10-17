---
title: Performance
description: How NSH keeps apps fast (native runtime, batched crossings, native animations, virtualized lists), what that means for your code, and how to measure with nsh trace.
order: 62
group: tools
---

# Performance

Performance is NSH's first priority, and most of it is decided by the framework rather than by your app: the runtime
is native, each update crosses threads once, animations and list virtualization run without JS. This page explains the
rules NSH follows, what they mean for the code you write, and how to measure.

## The rules

### 1. JS does as little as possible

QuickJS is an interpreter: small, quick to start, and much slower than native code for heavy work. So in NSH, **only
your app's code is JS**. The reactive graph (signals, computed values, effects), the renderer your compiled JSX calls,
the layout engine, the components of `nsh/ui/mobile`, list virtualization, animation and every module you import are
Rust. Parsing, diffing, layout and string processing happen natively.

The effect is measurable. The same counter app on NSH's earlier JS runtime and on the native runtime (release builds):

| | Former JS runtime | Native runtime |
|---|---|---|
| Bundle size | 14 KB JS / 19.6 KB bytecode | 886 B JS / 1.7 KB bytecode |
| Evaluating the bundle | ~390 µs | ~95 µs |
| JS time of one tap (median) | ~14 µs | ~5 µs |
| Commit of one tap | ~22 µs | ~15 µs |
| Scrolling a 1000-row list, JS per rebinding frame | ~150 µs | ~77 µs |

The same holds for components: a `Button` from `nsh/ui/mobile` (Rust) against an equivalent TSX button, 200 of them:

| | Rust `Button`: JS / core | TSX button: JS / core |
|---|---|---|
| First frame (ops) | 1.32 ms / 0.77 ms (1015) | 2.32 ms / 0.70 ms (2415) |
| Updating all 200 titles (ops) | 0.42 ms / 0.42 ms (200) | 0.77 ms / 0.36 ms (600) |

(Measured on Apple silicon; the second table on the former runtime. Both from NSH's performance baselines.)

**For your code:** keep event handlers and computed values light. Heavy computation (parsing large payloads, sorting
thousands of items on every keystroke) costs more in QuickJS than you may be used to in V8 or JavaScriptCore with a JIT.
Do it once, cache it in a `computed`, or move it off the interaction path.

### 2. Cross threads in batches

Your app's JS runs on an engine thread, never on the main thread. One JS turn, for example one event handler with
everything it changes, becomes **one commit**: the runtime renders components, lays out, diffs and produces one batch
of native view operations, which reaches the main thread in **one hop**. There is no bridge message per view and no
serialization per prop.

**For your code:** you do not need to coalesce writes for performance; ten `set` calls in one handler still produce one
commit. Use `batch` when you want effects and computed values to see all the writes at once instead of running after
each one.

### 3. Fine-grained updates

Components run **once**. Afterwards, only the bindings that read a signal run again when it changes: a text, a style
property, a branch of `<If>`, the rows of `<Each>`. Nothing re-renders a whole subtree to find out what changed.

```tsx
import type { Accessor } from "nsh";
import { Text } from "nsh";

type Quote = { price: number; change: number };

function Price(props: { quote: Accessor<Quote> }) {
  // Runs once. When quote() changes, only the text and the color update.
  return (
    <Text style={{ color: props.quote().change < 0 ? "#cf222e" : "#1a7f37" }}>
      {props.quote().price.toFixed(2)}
    </Text>
  );
}
```

**For your code:**

- Read signals where they are used (inside JSX expressions, `computed`, `effect`), not at the top of a component, so
  the narrowest binding updates.
- Use `<Each>` (rows kept by key) or [`List`](./lists.md) for collections; a plain `.map()` in JSX creates every row
  again on each change.
- Derive values with `computed` when several bindings use them, so they are computed once per change.

Appending a fourth item to a three-item list, as `nsh trace` shows it:

```tsx
{items().map((n) => <Text>{`row ${n}`}</Text>)}                   // 22 mount items: 3 views removed, 4 created
<Each of={items()}>{(n) => <Text>{`row ${n()}`}</Text>}</Each>      // 4 mount items: 1 view created
```

See [Reactivity](./reactivity.md).

### 4. Do not send what is not needed

The core sends only what changed: new views get no default style and no empty event mask, unchanged styles and frames
are not sent, a change of text color does not lay the text out again. A tap that changes one number sends one text
item. You can see this in `nsh trace` output (below).

### 5. Animate natively

An animation driven from JS (a signal written in `requestAnimationFrame`) costs a JS call and a commit per frame, and
stutters when the engine thread is busy. [`nsh/animated`](./animation.md) runs animations natively instead:

```tsx
import { View } from "nsh";
import { createNativeValue } from "nsh/animated";
import { List } from "nsh/list";

const scrollY = createNativeValue(0);

<View
  style={{
    opacity: scrollY.interpolate({ inputRange: [0, 120], outputRange: [1, 0.35] }),
    transform: [{ translateY: scrollY.interpolate({ inputRange: [0, 120], outputRange: [0, -6] }) }],
  }}
/>;
<List each={photos()} scrollValue={scrollY} estimatedItemSize={64} style={{ flex: 1 }}>
  {(photo) => <Row photo={photo()} />}
</List>;
```

- `timing`, `spring` and `decay` run on the platform's UI thread, frame by frame, with no JS per frame.
- `scrollValue` writes a scroll offset into a native value in the same frame; `pan={{ x, y }}` lets drags move a view
  directly. JS hears about the end (the promise resolves, `onPanEnd` gets the velocity).
- `Animated.sequence` / `parallel` / `stagger` / `loop` run as one native tree: each step starts the moment the
  previous one ends, with no gap waiting for JS.
- `LayoutAnimation.configureNext` animates the next commit's frames, insertions and removals natively.

### 6. Virtualize long lists

[`List`](./lists.md) keeps only the rows near the viewport, recycles their cells, and computes positions, columns,
sticky headers and viewability natively. Scrolling frames that reveal no new row call no JS and produce no batch; a
frame that binds one new row of the gallery's photo list is 4 operations and 4 mount items.

**For your code:**

- Give `estimatedItemSize` close to the real row height.
- Keep per-item state in your data (keyed by id), not in signals created inside the row: cells are reused for other
  items, and `item()` / `index()` change when they are.
- Keep rows cheap: a row binds once per recycle, so its JS runs while the user scrolls.
- Use `scrollValue` instead of `onScroll` for effects that follow the scroll position. `onScroll` delivers at most one
  event per engine turn, but each is still a call into JS.

## Measuring

### Headless: `nsh trace`

```sh
nsh trace App.tsx                                   # first frame + 2 taps
nsh trace App.tsx --scroll 3000 --taps 0            # scrolling, 10 pt per frame
```

```text
perf commit: 56 ops, 395µs (list js 6µs), 49 mount items, 21 nodes
perf first frame: eval 271µs, total 1235µs
perf event js: 88µs
perf commit: 10 ops, 151µs (list js 2µs), 8 mount items, 27 nodes
```

What to look at:

- **ops** (operations your JS queued) and **mount items** (native view operations): these are deterministic. If a tap
  that should change one label produces 40 items, something re-creates views it should update.
- **event js**: the JS cost of a handler and what it triggered.
- **commit**: the core's cost of rendering components, layout and diffing.
- **list js**: JS spent binding list rows during that commit.

Full guide: [Testing with nsh trace](./testing-and-trace.md).

### On a device: `--trace`

```sh
nsh run App.tsx --trace
nsh run App.tsx --target android --trace --release
```

logs the same commit lines from the real host, with the platform's own text engine. `NshAppView(trace: true)` and
`new NshView(context, true)` do the same in an embedding app.

### On screen: `--fps`

```sh
nsh run App.tsx --target macos --fps --release
```

shows the frame rate and what frames and commits cost over the running app, with CPU, GPU and memory
([macOS: performance monitor](./macos.md#performance-monitor)).

> [!TIP]
> Single measurements are dominated by the CPU's clock state: a thread that just slept runs slower for a while.
> Compare the minimum of several runs, on the same machine, and prefer release builds for timing.

## Reference numbers

From NSH's own baselines (Apple silicon unless noted):

| Scenario | Numbers |
|---|---|
| Counter app, headless (debug) | First frame ~0.5–0.65 ms (58 ops / 49 mount items); one tap ~0.15 ms |
| Counter app, Mac Catalyst (release) | First frame ~1.05–1.15 ms (about 0.7 ms of it lays out 10 texts with CoreText for the first time); one tap ~0.12 ms |
| Gallery photo list, headless (debug) | First frame 653 ops / 224 nodes, ~2.7 ms |
| Scrolling the photo list, Catalyst (release) | Rebinding one row while scrolling slowly ~0.4 ms; a whole screen of ~32 rows on a fast fling 0.9–1.6 ms |
| Scrolling the photo list, Android emulator (arm64, release) | Rebinding one row ~0.3–0.4 ms (JS ~60 µs) |
| Engine startup (release) | ~75–155 µs; the global APIs (`fetch`, `URL`, ...) cost nothing until first used, then ~90 µs |
| Hot reload | ~120 ms from saving a file to the new version running; applying the update 0.2–0.5 ms |

## Checklist

- Interactions: a tap changes a handful of mount items, not dozens.
- Lists: `List` for anything long, with `estimatedItemSize`; per-item state in data.
- Animations: native values, not signals written every frame.
- Scroll effects: `scrollValue`, not `onScroll`.
- Handlers: no heavy work per keystroke or per scroll event; cache in `computed`.
- Components: prefer the Rust components of `nsh/ui/mobile` where they fit; they cost less JS than equivalents written
  in TSX.

## See also

- [Architecture](./architecture.md)
- [Testing with nsh trace](./testing-and-trace.md)
- [Lists](./lists.md), [Animation](./animation.md), [Reactivity](./reactivity.md)

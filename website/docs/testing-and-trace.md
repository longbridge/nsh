---
title: Testing with nsh trace
description: Run an app headless with nsh trace, read the native view operations and timings it prints, simulate taps and scrolling, and use it in CI; testID for UI tests.
order: 61
group: tools
---

# Testing with nsh trace

`nsh trace` runs an app **headless**: the real runtime (the same QuickJS VM, reactive graph, renderer, components,
layout and list virtualization as on a device), but no simulator, no device and no window. Instead of drawing, it
prints every operation the runtime would send to the native views, with timings. It runs anywhere the CLI runs,
including Linux CI machines.

```sh
nsh trace App.tsx
```

Use it to:

- check that an app (or a demo) builds, evaluates and renders without errors;
- see exactly which native views a change creates, moves or updates, and how many;
- measure the cost of the first frame, of a tap and of each scrolling frame;
- catch regressions in CI.

## What it does

1. Bundles the entry (development settings, not minified) and compiles it to bytecode.
2. Starts an engine with a 390×844 viewport and renders the first frame.
3. Optionally scrolls the first scroll view or list.
4. Waits while the app is idle (by default 1.2 s, so the engine's idle garbage collection runs first and retention bugs
   show up), printing whatever the app renders meanwhile (timers, fetches, animations). An app that never stops
   rendering (a clock, a looping animation) is waited for at most 10 s; `--wait` sets the time instead.
5. Taps the views `--tap` names, or the first pressable view (the first view with `onPress`) a number of times, and
   prints what each tap changes, then what the app renders after it.

| Flag | Default | Description |
|---|---|---|
| `--tap <selector>` | none | A view to tap (repeatable, in order): `id:<testID>`, `label:<accessibilityLabel>`, `text:<text>`, `view:<id>`, or a bare string matched as each of the first three. |
| `--taps <n>` | 2 | Without `--tap`: taps to send to the first pressable view. |
| `--idle-ms <ms>` | 1200 | Idle time before tapping. |
| `--wait <ms>` | none | Run exactly this long before the first tap and after each one, instead of waiting until the app is idle. |
| `--duration <ms>` | 0 | Keep running this long after the last tap. |
| `--safe-area <insets>` | 0 | Safe area insets: `top,right,bottom,left` in points, e.g. `47,0,34,0`. |
| `--scroll <points>` | none | Scroll the first scroll view or list by this many points, in 10 pt frames, printing each frame's batch. |
| `--width <points>` | 390 | Viewport width. |
| `--height <points>` | 844 | Viewport height. |

## Reading the output

Here is the counter example (`examples/counter/App.tsx`), shortened:

```text
✓ bundled App.tsx (3255 B in 14.3 ms)
perf commit: 56 ops, 395µs (list js 6µs), 49 mount items, 21 nodes
perf first frame: eval 271µs, total 1235µs
── initial render: 49 items
   LayoutDirection { rtl: false }
   Create { id: 2, kind: View }
   Create { id: 4, kind: Text }
   Insert { parent: 2, child: 4, before: None }
   ...
   Events { id: 20, events: EventMask(1) }
   Frame { id: 2, x: 0, y: 0, w: 390, h: 844 }
   Frame { id: 4, x: 24, y: 64, w: 342, h: 34 }
   ...
   Text { id: 4, text: "nsh counter", content: 342x34 @ 0,0 }
   Text { id: 14, text: "0", content: 36x77 @ 0,0 }
perf event Press on view 20
perf event js: 196µs
perf commit: 10 ops, 248µs (list js 6µs), 8 mount items, 24 nodes
── tap #1 on view 20: 8 items
   Create { id: 42, kind: Text }
   Insert { parent: 32, child: 42, before: None }
   Frame { id: 14, x: 136, y: 49, w: 71, h: 77 }
   Frame { id: 32, x: 24, y: 366, w: 342, h: 43 }
   Frame { id: 42, x: 0, y: 23, w: 342, h: 20 }
   Text { id: 14, text: "-1", content: 71x77 @ 0,0 }
   Text { id: 42, text: "• -1", content: 342x20 @ 0,0 }
   Text { id: 34, text: "Recent values: 1", content: 342x17 @ 0,0 }
```

### Batches and mount items

Each `── title: N items` block is one **mount batch**: everything the platform applies to native views for one frame,
in one hop to the main thread. The lines in it are **mount items**:

| Item | Meaning |
|---|---|
| `Create { id, kind }` | Create a native view (`View`, `Text`, `Image`, `Scroll`, ...). |
| `Insert { parent, child, before }` | Put a view into its parent, before a sibling (`None`: at the end). |
| `Remove` / `Delete` | Detach a view from its parent / free it. |
| `ViewStyle { id, style }` | The view's decoration: background, borders, radius, shadow, opacity, transform, ... |
| `ViewProps { id, props }` | Touch and accessibility settings, `testID`. |
| `Events { id, events }` | Which events the view reports (`EventMask(1)` is press). |
| `Frame { id, x, y, w, h }` | Its position and size in its parent, in points. |
| `Text { id, text, content }` | Laid-out text for a text view. |
| `Image { id, source, ... }` | An image to load. |

Only what changed is sent: a tap that changes one number sends one `Text` item; views get no default style and no empty
event mask; unchanged frames are not sent. A frame of scrolling that reveals no new row produces **no batch at all**.

### The `perf` lines

| Line | Meaning |
|---|---|
| `✓ bundled App.tsx (3255 B in 14.3 ms)` | Bundling time and bundle size (development settings). |
| `commit: 56 ops, 395µs (list js 6µs), 49 mount items, 21 nodes` | One commit: the operations JS queued, the time the core spent (rendering components, laying out, diffing), JS time spent binding list rows, the mount items produced, and the nodes alive. |
| `turn: 2640µs (timers and tasks 1964µs)` | A turn that committed: its whole time, and what its JS timers and Rust component tasks took before the commit (a Rust component renders in the commit). |
| `first frame: eval 271µs, total 1235µs` | Time to evaluate the bundle, and from start to the first batch. |
| `event Press on view 20` / `event js: 196µs` | An event dispatched, and the time its handler (and what it caused) took in JS. |

> [!NOTE]
> Text is measured with an approximate text engine headless, so text sizes differ a little from a device, which uses
> CoreText or Android's text layout. Layout logic, view counts and operation counts are the same.

Single-run timings depend a lot on the CPU's clock state (a machine that just woke from idle is slow). Compare runs on
the same machine, look at the minimum of several runs, and trust the op and item counts more than the microseconds.

## Scrolling

```sh
nsh trace examples/gallery/Bench.tsx --scroll 3000 --taps 0
```

Scrolls the first `<Scroll>` or `List` 10 points per frame and prints each frame's batch:

```text
perf commit: 26 ops, 297µs (list js 111µs), 22 mount items, 233 nodes
── scroll to 20: 22 items
   Create { id: 448, kind: View }
   ...
   Text { id: 456, text: "Photo #25", content: 298x20 @ 0,0 }
```

For a [list](./lists.md), this shows the virtualization at work: which frames bind new rows, how many operations a row
costs, and that frames revealing no new row produce nothing. Use it with a release build of the CLI
(`./target/release/nsh` in a checkout) for meaningful timings.

## Taps

Name the views to tap with `--tap`, by `testID`, accessibility label or text, in the order to tap them. A match that is
not pressable itself (a `Text` inside a button) taps its nearest pressable ancestor, and the views a tap brings in (a
pushed screen, an opened menu) can be tapped next:

```sh
nsh trace --tap label:Plan --tap "text:Add stage" --tap id:save --wait 300
```

Without `--tap`, `nsh trace` taps the **first** view with `onPress` on screen. You can also write a small entry file
that renders only the screen under test:

```tsx
// trace/AddItem.tsx: renders the screen and puts the button under test first
import { mount } from "nsh";
import { Cart } from "../src/Cart";

mount(() => <Cart initialItems={[]} />);
```

```sh
nsh trace trace/AddItem.tsx --taps 5
```

If a tap produces no update, `nsh trace` stops with `error tap #1 produced no update` and a non-zero exit code: that
is how it catches handlers that stopped working, for example because the app's nodes were garbage-collected.

## In CI

`nsh trace` needs nothing but the CLI, so it runs on Linux runners. Errors in the app (a failed bundle evaluation, an
uncaught exception, an unhandled promise rejection) are printed as `error` lines; a build failure, an app that renders nothing, or a tap that changes
nothing make it exit with a non-zero code. Check both:

```sh
#!/bin/sh
set -e
for entry in App.tsx trace/*.tsx; do
  out=$(nsh trace "$entry" --idle-ms 300 2>&1) || { echo "$out"; exit 1; }
  if echo "$out" | grep -E '^ *error' >/dev/null; then
    echo "$out"
    echo "errors in $entry"
    exit 1
  fi
done
```

```yaml
# .github/workflows/trace.yml
jobs:
  trace:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: ./scripts/trace.sh
```

(Install the `nsh` CLI on the runner the way you install it locally; see [Installation](./installation.md).)

To guard performance, record the item counts of a few interactions (the `── … : N items` lines and the `ops` /
`mount items` numbers) and fail when they grow. Counts are deterministic; timings on shared CI machines are not.

> [!NOTE]
> `nsh trace` runs with every capability: it does not apply the `capabilities` of `nsh.json`, and its `fetch` really
> goes to the network. Test capability rules on a device or in a packaged build.

## `testID` for UI tests

`nsh trace` checks what the runtime renders. To drive the real native UI with the platform's test tools (XCUITest,
Espresso / UI Automator, Appium, Maestro), give views a `testID`:

```tsx
<View testID="add-to-cart" onPress={add} style={{ padding: 12 }}>
  <Text testID="cart-count">{`${count()} items`}</Text>
</View>
```

| Platform | Where `testID` goes |
|---|---|
| iOS | The view's `accessibilityIdentifier` (`app.otherElements["add-to-cart"]` in XCUITest). |
| Android | The accessibility node's view id resource name (`By.res("add-to-cart")` in UI Automator). |

`testID` is a prop of every element and of the components in `nsh/ui/mobile`. It does not change what screen readers
say; use `accessibilityLabel` for that (see [Accessibility](./accessibility.md)). In `nsh trace` output it appears in
the view's `ViewProps` item as `test_id: Some("add-to-cart")`.

## See also

- [Performance](./performance.md): what to look for in these numbers
- [Dev server and debugging](./dev-server-and-debugging.md)
- [Architecture](./architecture.md): ops, commits and mount batches

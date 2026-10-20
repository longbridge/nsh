---
title: Testing
description: Write tests in TypeScript and run them headless with nsh test - open the app, press, type, scroll and wait like a user, stub the network and storage - and read the native view operations and timings --trace prints; testID for UI tests.
order: 61
group: tools
---

# Testing

`nsh test` runs apps **headless**: the real runtime (the same QuickJS VM, reactive graph, renderer, components,
layout and list virtualization as on a device), but no simulator, no device and no window. It runs anywhere the CLI
runs, including Linux CI machines, and does two jobs:

- it runs tests written in TypeScript: each opens the app, takes the steps a user would (press, type, scroll), and
  checks what is on screen;
- with `--trace`, it prints every operation the runtime sends to the native views, with timings: what the first
  frame, a tap or a frame of scrolling costs.

## Tests with nsh test

A test file is a `*.test.tsx` or `*.test.ts` file next to your code. It imports `nsh/test`:

```tsx
// App.test.tsx
import { expect, open, test } from "nsh/test";

test("adds a todo", async () => {
  const app = await open("./App.tsx", { storage: { todos: "[]" } });
  await app.type("id:new-todo", "Buy milk");
  await app.press("text:Add");
  await app.waitFor("text:Buy milk");
  expect(app.texts()).toContain("1 item left");
});
```

```sh
nsh test                      # every test file of the app in the current directory
nsh test App.test.tsx         # one file
nsh test --filter "adds"      # the tests whose name contains "adds"
```

```text
✓ App.test.tsx › adds a todo (84ms)
✗ App.test.tsx › keeps todos after a restart (312ms)
    expected ["Todos","0 items left"] to contain "1 item left"
    at <anonymous> (App.test.tsx:21:22)
    on screen: "Todos", "0 items left"
error 2 tests: 1 passed, 1 failed (0.9s)
```

A failing test shows what was thrown, where in the test file, and the texts on screen of the app it opened. Any failure
makes `nsh test` exit with a non-zero code. `nsh init` creates an `App.test.tsx` with the app, and `nsh check` type
checks test files with the rest (`.nsh/types` declares `nsh/test`).

Each test file runs on an engine of its own, and its tests run one after the other, in order. `open` starts the app on
another engine, a fresh one each time, so tests never share state; the apps a test opened are closed when it ends. A
test fails when it throws, when the promise it returns rejects, and after 30 seconds. An app with Rust crates of its own
(`nsh.json` `"rust"`) runs its tests in a headless runner NSH builds with them, the same way; in a Rust app
(`nsh.json` `"root"`, [Building with Rust](./rust.md)), `open(".")` starts its root component.

### Opening the app

`open(entry, options?)` builds the app at `entry` (relative to the test file; each entry is built once per run),
starts it on a 390×844 viewport and resolves once it rendered and is quiet.

| Option | Default | Description |
|---|---|---|
| `storage` | empty | What `nsh/storage` holds at the start: `{ todos: "[]" }`. |
| `network` | none | The responses the app's requests get, by `"<METHOD> <URL>"` (a bare URL is a `GET`): `{ "GET https://api.example.com/todos": { status: 200, body: [] } }`. `body` is a string, or a value sent as JSON. A request not listed fails, so tests never reach the network unasked. `"live"`: the real network. |
| `width`, `height` | 390, 844 | The viewport in points. |
| `safeArea` | 0 | The safe area insets in points: `{ top: 47, bottom: 34 }`. |
| `clipboard` | empty | The text on the clipboard at the start. |

### Steps

Steps name views with **selectors**, written as Playwright's:

| Selector | Matches |
|---|---|
| `id:<testID>` | The view with that `testID`. |
| `label:<label>` | The view with that `accessibilityLabel`. |
| `text:<text>` | A text that reads it (spaces around it aside), or an input holding it or, while empty, with that placeholder. |
| `role:<role>`, `role:<role>[<name>]` | A view with that `accessibilityRole` (`role:button`), and that name: its label, else the texts in it (`role:button[Save]`). |
| `view:<id>` | The view with that id in `--trace` output. |
| anything else | A `testID`, else a label, else a text, as written. |

`label:`, `text:` and a role's name also take a regex: `text:/^Buy/i`. ` >> ` looks inside what the part before matched
(`id:cart >> role:button[Remove]`), and `nth=<n>` keeps one of the matches, from 0 (`text:Delete >> nth=1`). A
selector finds the views a user would look at first: in the top modal, then on screen, and of a navigation stack only
the top screen. A step that acts on a view (`press`, `type`, `clear`, `submit`, `scroll`, `longPress`) needs exactly
one: a selector matching several fails the step, and the error says how to narrow it.

| Step | Description |
|---|---|
| `await app.press(selector)` | Presses the view, or the nearest view around it that hears presses (the button around a text): `onPressIn`, `onPressOut`, `onPress`. Rejects when it is disabled. |
| `await app.longPress(selector)` | Holds a finger on it for its `delayLongPress` (500 ms): `onPressIn`, `onLongPress`, `onPressOut`; no `onPress` follows. |
| `await app.clear(selector)` | Empties the input, as selecting all and deleting does. |
| `await app.type(selector, text)` | Types into the input (or the first input inside the view; `text:` also matches an input's text or placeholder) at the end of what it holds, key by key: it takes the focus, then each character is an `onKeyPress` and an `onChangeText`, within `maxLength`. A `\n` in a single-line input is the return key. |
| `await app.submit(selector)` | The return key in the input, as its `submitBehavior` says: `onSubmitEditing` (and the focus leaves a single-line input), or a line break in a multiline one. |
| `await app.scroll(selector, distance)` | Drags the scroll view or list (the view, the one around it, or the first inside it) by `distance` points along its axis, 10 points per frame, within its content. Negative scrolls back. |
| `await app.wait(ms)` | Lets the app run for `ms` milliseconds: timers, animations, requests. |
| `await app.waitFor(selector, options?)` | Resolves once a view matches, or with `{ state: "gone" }` once none does; rejects after `timeout` ms (5000). |
| `await app.waitUntil(condition, options?)` | Resolves once `condition()` returns true, looking again after each change of the app: for what no selector says (`() => app.storage().note === "Milk"`). |
| `app.shows(selector)` | Whether a view matches now. |
| `app.count(selector)` | How many views match now. |
| `app.enabled(selector)` | Whether the view takes input: neither it nor a view around it is disabled, and an input is editable. |
| `app.texts()` | The texts on screen now, in order. |
| `app.value(selector)` | The text of an input, `null` when none matches. |
| `app.storage()` | What the app's storage holds now. |
| `await app.back()` | Goes back as the user does (a header's back button, swiping a sheet down): the innermost stack of the newest modal, else of the screen, pops its top screen (`onDismiss`), or asks it (`onBackRequest`) when it has `preventBack`; without one, the newest modal asks to close (`onRequestClose`). Rejects when there is nothing to go back from. |
| `app.onDialog(handler)` | Answers the dialogs the app shows from now on (`Alert.alert`, action sheets): `handler` gets the dialog (`title`, `message`, `buttons`, `cancelable`, `sheet`) and returns the text of the button to press, or `null` to close it without one. |
| `app.dialogs()` | The dialogs the app showed, in order. |
| `app.clipboard()` | The text on the clipboard now, `null` when it holds none. |

A step resolves once the app rendered nothing for 100 ms and no native animation runs (2 s at most, for apps that
render all the time), so what it changed is on screen when the next line runs; use `waitFor` for what comes later (a
response, a timer). A step rejects, failing the test, when nothing matches its selector (the error lists the texts on
screen), when the app threw an exception meanwhile, and when the app asked for what only a real platform answers.

### What headless runs play of the platform

A headless run has the real runtime and no platform. It plays the platform's part NSH's protocol defines, and nothing
that only a real platform can do:

- **Native animations run** frame by frame, to their end: `start(callback)` callbacks run, and so does what waits for
  them.
- **Going back** pops screens and closes modals as a platform does (`app.back()`).
- **Dialogs are answered**: by the test's `onDialog`, else by their cancel button, else they are closed when they can
  be; a dialog that only its other buttons answer fails the step that showed it until the test says which to press.
- **The clipboard holds text**: what the app copies, read back when it pastes (`clipboard` to start with some).
- **Modals show at once**: `onShow` comes when one is presented.
- **Images are not loaded**: no `onLoad` or `onError` comes.
- **What only a real platform answers fails the step**: an image's size (`Image.getSize`, `Image.prefetch`), an image
  or files on the clipboard.

### Expectations

`expect(value)` compares values as JSON does (`undefined` is `null`, numbers by value), and each check throws when it
does not hold. `.not` turns them around: `expect(app.texts()).not.toContain("Error")`.

| Check | Holds when |
|---|---|
| `toBe(expected)`, `toEqual(expected)` | The value equals `expected`, member by member. |
| `toContain(item)` | An array holds `item`, or a string contains it. |
| `toBeTruthy()`, `toBeFalsy()` | The value is truthy / falsy, as JS sees it. |
| `toMatch(pattern)` | A string contains `pattern` (a string), or matches it (a `RegExp`). |
| `toHaveLength(n)` | An array has `n` items, or a string `n` UTF-16 units. |
| `toBeNull()` | The value is `null` (or `undefined`). |
| `toBeGreaterThan(n)`, `toBeGreaterThanOrEqual(n)`, `toBeLessThan(n)`, `toBeLessThanOrEqual(n)` | A number compares so with `n`. |
| `toThrow(message?)` | `expect(fn)`: calling `fn` throws, with a message matching `message` when given. |
| `rejects.toThrow(message?)` | `expect(fn)`: the promise `fn` returns rejects: `await expect(() => app.press("Pay")).rejects.toThrow("is disabled")`. |

### Steps from standard input

Given the app's entry instead of tests, `nsh test` opens the app on the real network, lets it run for 1.2 s, then takes
the steps standard input gives, with `app` the app:

```sh
echo 'await app.press("text:+1"); expect(app.texts()).toContain("1");' | nsh test App.tsx
```

The 1.2 s let the engine's idle garbage collection run before the first step, so an app that lets go of what it still
uses shows it: its handlers stop working.

The examples have tests to start from: `examples/counter` and `examples/gallery/tests` (storage, inputs, a long list,
stubbed requests) for TSX apps, `examples/rust-exports` for an app with a Rust crate, `examples/gallery-rs` for a Rust
app.

## Tracing with --trace

`--trace` prints every mount batch of the apps the tests open, under the step it came in, and the time of every
commit. Run it on a test file, or on the app's entry with the steps on standard input:

```sh
nsh test App.test.tsx --trace
nsh test App.tsx --trace                                   # the first frame, then 1.2 s idle
echo 'await app.press("Add")' | nsh test App.tsx --trace    # and a press
```

Use it to:

- check that an app (or a demo) builds, evaluates and renders without errors;
- see exactly which native views a change creates, moves or updates, and how many;
- measure the cost of the first frame, of a tap and of each scrolling frame;
- catch regressions in CI.

### Reading the output

Here is the counter example with one press (`echo 'await app.press("+1")' | nsh test examples/counter/App.tsx
--trace`), shortened:

```text
✓ bundled App.tsx (3255 B in 6.2 ms)
perf commit: 56 ops, 271µs (list js 5µs), 49 mount items, 21 nodes
perf first frame: eval 187µs, total 820µs
── open(App.tsx): 49 items
   LayoutDirection { rtl: false }
   Create { id: 2, kind: View }
   Create { id: 4, kind: Text }
   Insert { parent: 2, child: 4, before: None }
   ...
   Frame { id: 2, x: 0, y: 0, w: 390, h: 844 }
   Frame { id: 4, x: 24, y: 64, w: 342, h: 34 }
   ...
   Text { id: 4, text: "nsh counter", content: 342x34 @ 0,0 }
   Text { id: 14, text: "0", content: 36x77 @ 0,0 }
perf event Press on view 26
perf event js: 413µs
perf commit: 10 ops, 672µs (list js 7µs), 7 mount items, 24 nodes
── press(+1): 7 items
   Create { id: 42, kind: Text }
   Insert { parent: 32, child: 42, before: None }
   Frame { id: 32, x: 24, y: 366, w: 342, h: 43 }
   Frame { id: 42, x: 0, y: 23, w: 342, h: 20 }
   Text { id: 14, text: "1", content: 36x77 @ 0,0 }
   Text { id: 42, text: "• 1", content: 342x20 @ 0,0 }
   Text { id: 34, text: "Recent values: 1", content: 342x17 @ 0,0 }
✓ App.tsx › steps (1.4s)
```

#### Batches and mount items

Each `── step: N items` block is one **mount batch**, under the step it came in: everything the platform applies to native views for one frame,
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

#### The `perf` lines

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

### Scrolling

```sh
echo 'await app.scroll("id:photos", 3000)' | nsh test examples/gallery/Bench.tsx --trace
```

`scroll` moves the list 10 points per frame, at 60 frames a second, and each frame's batch is printed:

```text
perf commit: 55 ops, 1667µs (list js 771µs), 46 mount items, 414 nodes
── scroll(id:photos, 3000): 46 items
   Create { id: 792, kind: View }
   Insert { parent: 94, child: 792, before: None }
   ...
```

For a [list](./lists.md), this shows the virtualization at work: which frames bind new rows, how many operations a row
costs, and that frames revealing no new row produce nothing. Use it with a release build of the CLI
(`./target/release/nsh` in a checkout) for meaningful timings.

### In CI

`nsh test` needs nothing but the CLI, so it runs on Linux runners, and any failing test makes it exit with a non-zero
code: a test that fails, an app that does not build or renders nothing, an exception the app throws during a step.

```yaml
# .github/workflows/test.yml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: nsh test
```

(Install the `nsh` CLI on the runner the way you install it locally; see [Installation](./installation.md).)

To guard performance, record the item counts of a few interactions (the `── … : N items` lines and the `ops` /
`mount items` numbers of `--trace`) and fail when they grow. Counts are deterministic; timings on shared CI machines
are not.

> [!NOTE]
> Tests run with every capability: `nsh.json`'s `capabilities` are not applied. Test capability rules on a device or in
> a packaged build.

## `testID` for UI tests

`nsh test` checks what the runtime renders, and its `id:` selectors find views by `testID`. To drive the real native UI with the platform's test tools (XCUITest,
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
say; use `accessibilityLabel` for that (see [Accessibility](./accessibility.md)). In `--trace` output it appears in
the view's `ViewProps` item as `test_id: Some("add-to-cart")`.

## See also

- [Performance](./performance.md): what to look for in these numbers
- [Dev server and debugging](./dev-server-and-debugging.md)
- [Architecture](./architecture.md): ops, commits and mount batches

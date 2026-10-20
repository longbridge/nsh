---
title: Reactivity
description: Signals, computed values, effects, batch, untracked, cleanup and context - how NSH updates exactly what changed, and the mistakes to avoid.
order: 31
group: ui
demo: guide-reactivity
---

# Reactivity

NSH updates the UI with **signals**: values that know who reads them. When a signal changes, NSH runs again exactly the computations that read it (a text, a style, an effect) and nothing else. Components do not re-render. The model is Solid's, and the reactive graph itself is native: reading a signal is a call into Rust, and the scheduling, dependency tracking and cleanup all happen outside the interpreter.

Everything on this page is imported from `nsh`:

```tsx
import { batch, cleanup, computed, context, effect, signal, untracked } from "nsh";
```

## Signals

`signal(value)` makes a signal. Call it to read the value; `set` and `update` write it:

```tsx
const count = signal(0);

count();                     // 0, and tracks the read
count.set(5);
count.update((n) => n + 1);  // 6
```

A read inside a tracking scope (JSX, `computed`, `effect`) subscribes that scope to the signal. A write notifies the subscribers when the value changed, compared with `===` by default.

```tsx
function Counter() {
  const count = signal(0);
  return (
    <View onPress={() => count.update((n) => n + 1)}>
      <Text>Tapped {count()} times</Text>
    </View>
  );
}
```

Tapping updates the one text node that reads `count()`. `Counter` itself runs once.

`signal<T>()` without a value starts as `undefined` and is typed `Signal<T | undefined>`. The second argument sets when a write counts as a change:

```tsx
const position = signal({ x: 0, y: 0 }, { equals: (a, b) => a.x === b.x && a.y === b.y });
const tick = signal(0, { equals: false });   // every write notifies
```

### Objects and arrays

Signals compare with `===`, so a value mutated in place is not a change. Write a new object or array:

```tsx
const todos = signal<Todo[]>([]);

todos.update((list) => [...list, { id: 3, title: "New" }]);                 // notifies
todos.update((list) => list.map((t) => (t.id === id ? { ...t, done: true } : t)));

todos().push(item);    // does not notify anyone
todos.set(todos());    // still the same array: no change
```

`<Each>` keys rows by item, so replacing one item in a new array rebuilds only that row ([JSX and components](./jsx-and-components.md#control-flow)).

## computed

`computed(fn)` derives a value from other signals. It runs `fn`, remembers the result, and runs it again when a signal it read changes. Reading it is like reading a signal:

```tsx
const price = signal(12);
const quantity = signal(2);
const total = computed(() => price() * quantity());

<Text>Total: {total()}</Text>
```

Use `computed` when a derived value is read in several places or is expensive: it runs once per change however many readers it has, and readers are notified only when its result changes (`===`, or the `equals` option). For a cheap value read once, a plain function is enough:

```tsx
const label = () => `${quantity()} items`;
<Text>{label()}</Text>
```

`fn` receives the previous value (`undefined` the first time), for values that accumulate:

```tsx
const max = computed((prev: number | undefined) => Math.max(prev ?? 0, speed()));
```

## effect

`effect(fn)` runs `fn` after the current rendering, and again whenever a signal it read changes. Effects are for side effects: logging, timers, network requests, imperative calls on [refs](./jsx-and-components.md#refs).

```tsx
function Search(props: { query: string }) {
  const results = signal<string[]>([]);
  effect(() => {
    const q = props.query;          // tracked: runs again when the prop changes
    if (!q) return;
    fetch(`https://api.example.com/search?q=${encodeURIComponent(q)}`)
      .then((r) => r.json())
      .then((data) => results.set(data.items));
  });
  return <Each of={results()}>{(item) => <Text>{item()}</Text>}</Each>;
}
```

An effect tracks the reads made while it runs synchronously. Reads in a `.then` callback or after an `await` are not tracked.

Effects run after the elements created in the same pass exist, so refs are set:

```tsx
let input: YNode | undefined;
effect(() => {
  if (editing()) input?.focus();
});
```

> [!TIP]
> Do not use effects to keep one signal in sync with another (`effect(() => b.set(a() * 2))`). Use `computed`: it is never out of date and does not cause a second pass. An effect that writes a signal it also reads runs itself again and again until NSH stops it with an error.

## resource

`resource(source, fetcher)` loads a value asynchronously and keeps where the load is, so a component shows data, a spinner or an error without managing them itself:

```tsx
import { resource, signal, Text } from "nsh";
import { List } from "nsh/list";

type Post = { id: number; title: string };

const url = signal("https://jsonplaceholder.typicode.com/posts?_limit=20");
const posts = resource(url, async (u, { signal }) => {
  const response = await fetch(u, { signal });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return (await response.json()) as Post[];
});

<If when={posts.error()}>
  <Text>Failed: {String(posts.error())}</Text>
</If>
<List
  each={posts() ?? []}
  refreshing={posts.state() === "refreshing"}
  onRefresh={posts.refetch}
>
  {(post) => <Text>{post().title}</Text>}
</List>
```

`source` is an accessor (a signal, a function) or a value. It is tracked: whenever it gives a new value, `fetcher(value, info)` loads again. A source of `undefined`, `null` or `false` loads nothing, for a user not signed in yet or nothing picked. With no source, `resource(fetcher)` loads once, calling `fetcher(info)`.

| Member | What it is |
|---|---|
| `posts()` | The latest value: `undefined` before the first load ends, kept while loading again and after a failure. |
| `posts.loading()` | Whether a load runs. |
| `posts.error()` | The last load's error, `undefined` once a load gives a value. |
| `posts.state()` | `"unresolved"` (nothing to load), `"pending"` (the first load), `"ready"`, `"refreshing"` (loading again, the value kept), `"errored"`. |
| `posts.refetch()` | Loads again from the same source: a retry, a pull to refresh. |
| `posts.mutate(value)` | Sets the value here with no load, an optimistic update, and clears the error. |

The members are bound, so `onRefresh={posts.refetch}` and `const loading = posts.loading` work as they are.

Only the latest load counts. A new one aborts the one running through `info.signal`, which `fetch` takes, and so does the component going away; a load that ends after it was superseded changes nothing. `info.refetching` says that `refetch()` started the load. `fetcher` may return a plain value as well as a promise; a throw is a failure. With `{ initialValue }` the resource has a value from the start, and its type has no `undefined`.

Rust components have the same: `resource(move || source(), |value, info| async move { .. })` with a `Result` for the outcome ([Building with Rust](./rust.md)).

## cleanup

`cleanup(fn)` registers `fn` to run before the current effect runs again, or when the component or effect it belongs to goes away:

```tsx
function Clock() {
  const now = signal(Date.now());
  const id = setInterval(() => now.set(Date.now()), 1000);
  cleanup(() => clearInterval(id));       // when Clock is removed
  return <Text>{new Date(now()).toLocaleTimeString()}</Text>;
}

effect(() => {
  if (!running()) return;
  const id = setInterval(tick, 100);
  cleanup(() => clearInterval(id));       // before the next run, or on removal
});
```

A component goes away when it is no longer rendered: its `<If>` branch is hidden, its `<Each>` row is removed, or its screen is popped. Everything it created (effects, computed values, cleanups, nested components) is disposed with it.

## batch

Each write notifies its subscribers right away. To make several writes count as one change, wrap them in `batch`: dependants run once, after the last write.

```tsx
const save = () =>
  batch(() => {
    firstName.set(form.first);
    lastName.set(form.last);
    dirty.set(false);
  });
```

Event handlers are not batched for you: two writes in an `onPress` run an effect that reads both signals twice. `batch` returns what its function returns.

## untracked

`untracked(fn)` runs `fn` without subscribing to what it reads. Use it in a tracking scope to read a value without depending on it:

```tsx
effect(() => {
  const q = query();                     // runs again when query changes
  const filters = untracked(activeFilters); // read, but not a reason to run again
  search(q, filters);
});
```

A component body is already untracked: reading a signal there reads it once.

## context

`context(defaultValue)` makes a context, a value a component provides to everything it renders, however deep. Call the context to read the nearest provided value (or the default), and render `<Ctx.Provider value>` to provide one:

```tsx
const Theme = context<"light" | "dark">("light");

function Label(props: { children: string }) {
  const theme = Theme();
  return <Text style={{ color: theme === "dark" ? "#ffffff" : "#1f2328" }}>{props.children}</Text>;
}

mount(() => (
  <Theme.Provider value="dark">
    <Label>Hello</Label>
  </Theme.Provider>
));
```

The provider reads `value` once, when it is created. To provide something that changes, provide a signal (or an object of signals) and read it in JSX:

```tsx
const Accent = context(signal("#0969da"));

function App() {
  const accent = signal("#0969da");
  return (
    <Accent.Provider value={accent}>
      <Swatch />
    </Accent.Provider>
  );
}

function Swatch() {
  const accent = Accent();
  return <View onPress={() => accent.set("#8250df")} style={{ width: 40, height: 40, backgroundColor: accent() }} />;
}
```

Read a context in a component body (or in code that runs while components are being created), not in an event handler or a timer, where there is no component to look it up from.

## What runs, and when

| You write | Runs | Runs again when |
|---|---|---|
| Component body | Once, when it is created | Never (hot reload aside) |
| `{expr}` or `prop={expr}` that calls or reads a property | Once, when the element is created | A signal it read changes; only that prop or child is updated |
| `computed(fn)` | When created | A signal it read changes |
| `effect(fn)` | After the elements of the current pass exist | A signal it read changes |
| `<If when>` | When created | The truthiness of `when` changes |
| `<Each of>` row | When its key first appears | Never; its `item()` / `index()` accessors change instead |

The environment is reactive the same way: `windowSize()`, `colorScheme()` and `fontScale()` from [`nsh/device`](../modules/nsh-device.md) and `safeArea()` / `keyboard()` from `nsh/ui/mobile` are accessors, so JSX that reads them follows rotation, dark mode or the keyboard.

## Common mistakes

**Destructuring props.** `const { title } = props` reads `title` once. Read `props.title` where you use it, or split props with [`split`](./jsx-and-components.md#spreading-props).

**Reading a signal in the component body and expecting updates.**

```tsx
function Bad() {
  const n = count();           // read once
  return <Text>{n}</Text>;     // never changes
}

function Good() {
  return <Text>{count()}</Text>;
}
```

**Mutating instead of setting.** `list().push(x)` changes the array but notifies nobody. Write a new array.

**Deriving state with effects.** `effect(() => total.set(price() * qty()))` works, but leaves `total` stale between the write and the effect, and costs a second pass. Use `computed`.

**Creating effects outside components.** Effects and computed values belong to the component that created them and are disposed with it. Created in an event handler or a timer callback, they have no owner and live forever. Create them in a component body and let them read the signals that change.

**Expecting `async` code to be tracked.** Only reads made synchronously while an effect or computed value runs are tracked. Read the signals first, then start the asynchronous work.

## Related

- [JSX and components](./jsx-and-components.md): props as getters, `<If>`, `<Each>`.
- [`nsh` module reference](../modules/nsh.md): every function's signature.
- [Animation](./animation.md): animated values that animate on the UI thread without signals.

---
title: nsh
description: "The core module: signals, computed values, effects, context, mount and the element and style types."
order: 2
---

# nsh

`nsh` is the core module: the reactive API (signals, computed values, effects, context), `mount`, the host elements `View`, `Text`, `Scroll` and `Input`, and the TypeScript types of elements, styles and events. The reactive graph itself runs natively in Rust; reading a signal is a call into the runtime that costs a few nanoseconds, and only your own closures run in the JavaScript interpreter.

```ts
import { batch, cleanup, computed, context, effect, Input, mount, Scroll, signal, split, Text, untracked, View } from "nsh";
```

NSH's reactivity follows the same model as SolidJS: a component function runs **once**, and the parts of the UI that read signals are updated in place when those signals change. There is no re-render of the component and no virtual DOM. See [Reactivity](../docs/reactivity.md) for the guide.

## signal

```ts
function signal<T>(value: T, options?: SignalOptions<T>): Signal<T>;
function signal<T = undefined>(): Signal<T | undefined>;

interface Signal<T> {
  (): T;                              // read (tracked)
  set(value: T): void;                // write
  update(fn: (value: T) => T): void;  // write from the current value
}

interface SignalOptions<T> {
  /** When a new value counts as a change (default `===`); `false`: always. */
  equals?: false | ((previous: T, next: T) => boolean);
}
```

A signal holds a value. Calling it reads the value and, inside a tracking scope (JSX expressions, `computed`, `effect`), subscribes that scope to it. `set` and `update` write it; everything that read it updates.

```tsx
import { mount, signal, Text, View } from "nsh";

function Counter() {
  const count = signal(0);
  return (
    <View style={{ padding: 24, gap: 12 }}>
      <Text>Count: {count()}</Text>
      <Text onPress={() => count.update((n) => n + 1)}>Add one</Text>
    </View>
  );
}

mount(Counter);
```

By default a write that is `===` to the current value does nothing. Pass `equals: false` to notify on every write (useful when you mutate an object in place and set the same object again), or your own comparison:

```ts
const items = signal<string[]>([], { equals: false });
const point = signal({ x: 0, y: 0 }, { equals: (a, b) => a.x === b.x && a.y === b.y });
```

## computed

```ts
function computed<T>(fn: (previous: T | undefined) => T, options?: SignalOptions<T>): Accessor<T>;

interface Accessor<T> {
  (): T;
}
```

A value derived from signals. `fn` runs again when a signal it read changes; readers of the computed value only update when its result changes (by `equals`, `===` by default). `fn` receives the previous result.

```ts
const first = signal("Ada");
const last = signal("Lovelace");
const full = computed(() => `${first()} ${last()}`);
full(); // "Ada Lovelace"
```

Use `computed` when a derivation is expensive or read in several places. For a cheap expression, a plain function (`const full = () => first() + " " + last()`) is just as reactive.

## effect

```ts
function effect(fn: () => void): void;
```

Runs `fn` after rendering, and again whenever a signal it read changes. Use effects for side effects: logging, timers, talking to native modules. An effect belongs to the component (or effect) it was created in and is disposed with it.

```ts
const query = signal("");
effect(() => {
  console.log("query is now", query());
});
```

To undo something before the effect runs again (or when its component goes away), call [`cleanup`](#cleanup) inside it.

## batch

```ts
function batch<T>(fn: () => T): T;
```

Runs `fn` and applies all its writes together at the end, so readers update once instead of once per write. Returns what `fn` returns.

```ts
batch(() => {
  first.set("Grace");
  last.set("Hopper");
}); // `full` recomputes once
```

## untracked

```ts
function untracked<T>(fn: () => T): T;
```

Runs `fn` without subscribing the current scope to what it reads. Use it to read a signal inside an effect or JSX expression without depending on it.

```ts
effect(() => {
  // Runs again when `query` changes, not when `page` changes.
  search(query(), untracked(() => page()));
});
```

## cleanup

```ts
function cleanup(fn: () => void): void;
```

Registers `fn` to run before the current effect runs again, or when the current component or effect goes away.

```tsx
function Clock() {
  const now = signal(new Date());
  const timer = setInterval(() => now.set(new Date()), 1000);
  cleanup(() => clearInterval(timer));
  return <Text>{now().toLocaleTimeString()}</Text>;
}
```

## context

```ts
function context<T>(defaultValue: T): Context<T>;

interface Context<T> extends Accessor<T> {
  Provider: (props: { value: T; children?: JSX.Element }) => JSX.Element;
}
```

A value a component provides to everything it renders, however deep. Calling the context (`Theme()`) reads the nearest provider's value, or `defaultValue` without one.

```tsx
import { context, mount, signal, Text, View } from "nsh";

const Theme = context<"light" | "dark">("light");

function Label(props: { text: string }) {
  return <Text style={{ color: Theme() === "dark" ? "#fff" : "#000" }}>{props.text}</Text>;
}

function App() {
  const dark = signal(false);
  return (
    <Theme.Provider value={dark() ? "dark" : "light"}>
      <View style={{ flex: 1, padding: 24, backgroundColor: dark() ? "#000" : "#fff" }}>
        <Label text="Hello" />
        <Text onPress={() => dark.update((d) => !d)}>Toggle</Text>
      </View>
    </Theme.Provider>
  );
}

mount(App);
```

## mount

```ts
function mount<P extends object>(app: Component<P>, props?: P): () => void;
```

Renders `app` into the surface (the window, or the `NshAppView` / `NshView` the app is embedded in) and returns a function that removes it. Every app's entry file ends with `mount(App)`.

```tsx
mount(App);
```

## split

```ts
function split<T extends object, K extends readonly (keyof T)[]>(
  props: T,
  keys: K,
): [Pick<T, K[number]>, Omit<T, K[number]>];
```

Splits a props object into the props named in `keys` and the rest. Both halves stay reactive: props whose values change are getters, and `split` keeps them as getters. Use it in a component that handles some props itself and passes the others on. Destructuring props (`const { label } = props`) would read them once and lose updates.

```tsx
import { Input, type InputProps, split, Text, View } from "nsh";

function LabeledInput(props: InputProps & { label: string }) {
  const [own, rest] = split(props, ["label"]);
  return (
    <View style={{ gap: 4 }}>
      <Text>{own.label}</Text>
      <Input {...rest} />
    </View>
  );
}
```

## View, Text, Scroll, Input

```ts
function View(props: ViewProps): YNode;
function Text(props: TextProps): YNode;
function Scroll(props: ScrollProps): YNode;
function Input(props: InputProps): YNode;
```

The host elements `nsh` exports, written as tags: `<View>`, `<Text>`, `<Scroll>`, `<Input>`. The compiler makes the
native view in place for a tag that names one of them through its import (an alias too), so importing them costs
nothing at run time. They are components like any other, so they can also be passed around (`const Tag = inline ? Text : View`).
The other elements come from their own modules: `Image` from [`nsh/image`](./nsh-image.md), `List` from
[`nsh/list`](./nsh-list.md), `Canvas`, `WebView` and `Video` from theirs. Their props are documented under
[Elements](../elements/index.md).

## Types

`nsh` also exports the types of the reactive API and of the host elements. The most used ones:

| Type | What it is |
|---|---|
| `Accessor<T>` | A readable value: `() => T` |
| `Signal<T>` | An `Accessor<T>` with `set` and `update` |
| `SignalOptions<T>` | Options of `signal` and `computed` (`equals`) |
| `Context<T>` | What `context` returns |
| `Component<P>` | `(props: P) => JSX.Element` |
| `JSX` | The JSX namespace (`JSX.Element`) |
| `YNode` | What a `ref` receives: focus, scroll and other commands. See [Elements](../elements/index.md) |
| `ViewProps`, `TextProps`, `ImageProps`, `ScrollProps`, `InputProps`, `CommonProps`, `TouchProps` | Props of host elements |
| `ViewStyle`, `TextStyle`, `ImageStyle`, `LayoutStyle`, `StyleProp<T>` | Style objects; see the [style reference](./style-reference.md) |
| `ColorValue`, `DimensionValue`, `LengthValue`, `TransformStep` | Values inside styles |
| `GestureResponderEvent`, `LayoutEvent`, `ScrollEvent`, `PanEvent`, `TouchPoint` | Event payloads |
| `ImageAsset` | What importing an image file gives (`{ uri, width, height }`) |
| `OS` | `"ios" \| "android" \| "macos" \| "windows" \| "linux"` |
| `AccessibilityRole`, `AccessibilityState` | Accessibility props |

```tsx
import { type GestureResponderEvent, type YNode, signal } from "nsh";

let field: YNode | undefined;
const onPress = (e: GestureResponderEvent) => field?.focus();
```

`StyleProp<T>` accepts one style or a list merged left to right, skipping falsy entries:

```tsx
<View style={[styles.card, selected() && styles.selected]} />
```

---
title: JSX and components
description: How NSH compiles JSX, what a component is, how props, children, control flow, refs and spreads work, and how tags name elements and components.
order: 30
group: ui
demo: guide-components
---

# JSX and components

NSH apps describe their UI with JSX, but there is no React and no virtual DOM. NSH's compiler turns each JSX element into direct calls of the native renderer, and a component is a plain function that runs once to build its part of the tree. What keeps the UI up to date afterwards is [reactivity](./reactivity.md): expressions in JSX that read signals are followed natively, and only they run again.

## Elements

Every tag is PascalCase and names something in scope. The **host elements**, each backed by a native view, are components you import from NSH's modules; only `If` and `Each` are globals that need no import:

| Tag | Import from | Native view | Page |
|---|---|---|---|
| `<View>` | `nsh` | `UIView` / `ViewGroup`: a box for layout, styling and touches | [View](../elements/view.md) |
| `<Text>` | `nsh` | A native text view; nested `<Text>` elements are spans | [Text](../elements/text.md) |
| `<Image>` | `nsh/image` | An image view (local assets and URLs) | [Image](../elements/image.md) |
| `<Scroll>` | `nsh` | `UIScrollView` / `ScrollView` | [Scroll](../elements/scroll.md) |
| `<Input>` | `nsh` | `UITextField` / `UITextView` / `EditText` | [Input](../elements/input.md) |
| `<List>` | `nsh/list` | A virtualized, recycling list | [List](../elements/list.md) |
| `<Canvas>`, `<WebView>`, `<Video>`, `<Modal>` | `nsh/canvas`, `nsh/webview`, `nsh/video`, `nsh/ui/mobile` | A drawing, a web page, a video player, a presented layer | [Elements](../elements/index.md) |
| `<If>`, `<Each>` | nothing: globals | Control flow, no view of its own | [Control flow](../elements/control-flow.md) |

```tsx
import { mount, Text, View } from "nsh";
import { Image } from "nsh/image";
```

Tags are PascalCase so that TypeScript checks each one against the binding it names: a lowercase tag is checked against one global type per name, so a generic component such as `List` or `Each` could not infer its rows' type from `each` / `of`, and an app's own components would not be typed at all. The imports cost nothing at run time: the compiler recognizes the primitives through their imports (aliases too) and makes the native element in place, so `<View>` is a direct call of the renderer and the import is dropped from the bundle.

Props keep their camelCase names (`onPress`, `numberOfLines`, `backgroundColor`). Every element accepts `style`, `class` / `classList` ([Tailwind](./tailwind.md)), `ref`, touch handlers, accessibility props and `testID`; platform-only props go in `ios={{…}}` and `android={{…}}`, and only the current platform's object is sent:

```tsx
<View android={{ elevation: 4, ripple: { color: "#0001" } }} style={{ borderRadius: 12 }} />
```

A lowercase tag is a build error that says what to write:

```text
x lowercase tag <view>: tags are PascalCase components: write <View> and
| `import { View } from "nsh"`
```

> [!IMPORTANT]
> Text is only displayed inside `<Text>`. A string directly in a `<View>` creates no native view and is not shown.

## Components

A component is a function that takes a props object and returns JSX:

```tsx
function Badge(props: { label: string; color?: string }) {
  return (
    <View style={{ borderRadius: 999, paddingHorizontal: 10, paddingVertical: 4, backgroundColor: props.color ?? "#ddf4ff" }}>
      <Text style={{ fontSize: 13, fontWeight: "600" }}>{props.label}</Text>
    </View>
  );
}

<Badge label={`${unread()} new`} color="#fff8c5" />
```

The function runs **once**, when the component is created, and does not track what it reads: there is no re-rendering. Code in its body (creating signals, starting effects, computing constants) runs one time. The JSX it returns stays connected to the props and signals it reads, so `Badge` above updates its text whenever `unread()` changes, without running `Badge` again.

A component may return an element, a string or number, an array, `null`, or a function returning any of these (which the renderer follows). The `Component<P>` type from `nsh` describes one:

```tsx
import type { Component, JSX } from "nsh";
import { Text, View } from "nsh";

const Row: Component<{ title: string; children?: JSX.Element }> = (props) => (
  <View style={{ flexDirection: "row", gap: 8 }}>
    <Text>{props.title}</Text>
    {props.children}
  </View>
);
```

### What runs again

The compiler decides for each expression in JSX whether it can change. An expression changes when it **calls a function or reads a property** (outside nested functions); everything else is set once:

```tsx
<Text>{count()}</Text>              // follows count
<Text>{props.title}</Text>          // follows the prop
<Text>{user.name}</Text>            // re-read when something it depends on changes
<Text>{label}</Text>                // a plain variable: set once
<View style={{ opacity: busy() ? 0.5 : 1 }} />   // the style object is rebuilt when busy changes
```

A changing prop or child is a small native effect that sets that one prop or replaces that one child. Style objects are diffed in Rust, so rebuilding a style only sends the properties that changed.

Event handlers are set once when they are a function or a name (`onPress={() => …}`, `onPress={save}`), and followed otherwise (`onPress={props.onPress}` follows the prop).

### Props are getters

A prop given a changing expression is passed as a getter: reading `props.title` calls it, which is what lets the JSX that reads it follow it. Static props are plain values. This is why props must be read where they are used:

```tsx
// Wrong: reads the prop once, when the component runs.
function Title(props: { text: string }) {
  const { text } = props;
  return <Text>{text}</Text>;
}

// Right: reads it in JSX, which follows it.
function Title(props: { text: string }) {
  return <Text>{props.text}</Text>;
}
```

The same applies to default values: write `props.color ?? "#ddf4ff"` in the JSX rather than destructuring with a default. To compute something from props, use a function or [`computed`](./reactivity.md#computed):

```tsx
function Price(props: { cents: number }) {
  const formatted = () => `$${(props.cents / 100).toFixed(2)}`;
  return <Text>{formatted()}</Text>;
}
```

### Children

What you put between a component's tags arrives as `props.children`, a getter that builds those elements when read:

```tsx
function Card(props: { title: string; children?: JSX.Element }) {
  return (
    <View style={{ padding: 14, borderRadius: 12, backgroundColor: "#ffffff", gap: 8 }}>
      <Text style={{ fontWeight: "600" }}>{props.title}</Text>
      {props.children}
    </View>
  );
}

<Card title="Account">
  <Text>Signed in as {user().name}</Text>
</Card>
```

Each read of `props.children` builds the children again (new native views). Read it once, in the place where the children go. Children can also be a function, a render callback the component calls with arguments:

```tsx
function Repeat(props: { times: number; children: (i: number) => JSX.Element }) {
  return <View>{Array.from({ length: props.times }, (_, i) => props.children(i))}</View>;
}

<Repeat times={3}>{(i) => <Text>Row {i}</Text>}</Repeat>
```

Fragments (`<>…</>`) group elements without a view.

## Control flow

`<If>` and `<Each>` are built-in control flow. They render no view of their own.

```tsx
<If when={user()} else={<Text>Signed out</Text>}>
  <Text>Hello, {user()!.name}</Text>
</If>

<Each of={todos()} key={(t) => t.id}>
  {(todo, index) => (
    <Text>
      {index() + 1}. {todo().title}
    </Text>
  )}
</Each>
```

- **`<If when else>`** renders its children while `when` is truthy, else the `else` element. It renders a branch again only when the condition's truthiness changes, not every time `when` is re-evaluated.
- **`<Each of key>`** renders its only child, a function, once per item. Rows are kept by `key(item)` (default: the item itself; objects by identity, primitives by value): when the array changes, kept rows are moved rather than rebuilt, and new rows are made only for new keys. The function receives **accessors**, `todo()` and `index()`, because a kept row's item and position can change. `key="index"` (or `key={(item, i) => i}`; the key function gets the index too) keeps rows by position instead: a list of strings that changes in place, such as text streamed in, keeps its rows and only their `item()` changes.

A ternary or `&&` in JSX also works (`{busy() ? <Spinner /> : <Content />}`), but it builds its branch again every time anything it reads changes. Prefer `<If>` when a branch is more than a few elements.

For long or infinite lists, use [`List`](./lists.md): it creates views only for the rows on screen and recycles them. See [Control flow](../elements/control-flow.md) for the details.

## Refs

`ref` gives you the native node of a host element, a `YNode`, for imperative calls: `focus()`, `blur()`, `scrollTo()`, `scrollToIndex()`, `measure()` ([where a view is](./touch-and-gestures.md#where-a-view-is-measure)), ... A variable is assigned, a function is called:

```tsx
import type { YNode } from "nsh";
import { Input, Scroll, Text, View } from "nsh";

function Search() {
  let input: YNode | undefined;
  return (
    <View>
      <Input ref={input} placeholder="Search" />
      <View onPress={() => input?.focus()}>
        <Text>Focus</Text>
      </View>
    </View>
  );
}

<Scroll ref={(node) => (scroller = node)} />
```

The ref is set when the element is created, before the component returns, so it is available in [effects](./reactivity.md#effect) and event handlers. Calls are sent to the view in order with the changes made before them. Components from `nsh/ui/mobile` accept `ref` too.

## Spreading props

Spread an object of props onto an element or a component with `{...props}`. Getters stay getters, so spread props keep following what they read. To take some props for yourself and pass the rest on, use `split`:

```tsx
import { Input, type InputProps, split, Text, View } from "nsh";

function Field(props: InputProps & { label: string }) {
  const [own, rest] = split(props, ["label"]);
  return (
    <View style={{ gap: 4 }}>
      <Text>{own.label}</Text>
      <Input style={{ borderWidth: 1, borderRadius: 8, padding: 8 }} {...rest} />
    </View>
  );
}

<Field label="Email" value={email()} onChangeText={(t) => email.set(t)} keyboardType="email-address" />
```

`split(props, keys)` returns `[picked, rest]`, both still reading through to the original props (destructuring `{ label, ...rest } = props` would read every prop once). Element props that come later in the JSX win over earlier ones.

`<If>` and `<Each>` take no spread props, and spread children (`{...items}`) are not supported: use `<Each>` or an array expression.

## Tags and names

A tag names a binding in scope, as usual: `<Badge />`, `<Theme.Provider>`, an imported `<Button>`. A tag that names nothing in scope is a build error, except `<If>` and `<Each>`, which are globals.

```tsx
import { Button } from "nsh/ui/mobile";

<Button title="Save" onPress={save} />
```

Your bindings win over the globals: a component named `Each` in scope makes `<Each>` that component. A primitive is recognized by its import, so `import { View as Box } from "nsh"` makes `<Box>` a view, and a component of your own named `Text` is just your component. The primitives are real components too: `const Tag = inline ? Text : View;` and then `<Tag>…</Tag>` works.

Components are typed by their own declarations, like any TypeScript function: the editor checks a component's props wherever it is used, with nothing to generate.

## Mounting and the root

`mount(App)` renders the root component; `mount(App, props)` passes it props. The root view fills the surface (the window, or the view an app embeds NSH in). Give the root `flex: 1` so it fills the screen, and a background color: the surface's own background depends on the platform.

```tsx
mount(() => (
  <View style={{ flex: 1, backgroundColor: "#ffffff" }}>
    <App />
  </View>
));
```

## Related

- [Reactivity](./reactivity.md): signals, effects, context.
- [Elements](../elements/index.md): every host element and its props.
- [Components](../components/index.md): the ready-made components of `nsh/ui/mobile`.

---
title: nsh
description: 核心模块：signal、computed、effect、context、mount，以及元素和样式的类型。
order: 2
---

# nsh

`nsh` 是核心模块：响应式 API（signal、computed、effect、context）、`mount`、宿主元素 `View`、`Text`、`Scroll` 和 `Input`，以及元素、样式和事件的 TypeScript 类型。响应式图本身以 Rust 原生运行；读取一个 signal 是一次对运行时的调用，只需几纳秒，只有你自己写的闭包会在 JavaScript 解释器中执行。

```ts
import { batch, cleanup, computed, context, effect, Input, mount, Scroll, signal, split, Text, untracked, View } from "nsh";
```

NSH 的响应式模型与 SolidJS 相同：组件函数只运行**一次**，界面中读取了 signal 的部分会在 signal 变化时就地更新。组件不会重新渲染，也没有虚拟 DOM。教程见[响应式](../docs/reactivity.md)。

## signal

```ts
function signal<T>(value: T, options?: SignalOptions<T>): Signal<T>;
function signal<T = undefined>(): Signal<T | undefined>;

interface Signal<T> {
  (): T;                              // 读取（会被追踪）
  set(value: T): void;                // 写入
  update(fn: (value: T) => T): void;  // 基于当前值写入
}

interface SignalOptions<T> {
  /** 新值何时算作变化（默认 `===`）；`false`：总是算作变化。 */
  equals?: false | ((previous: T, next: T) => boolean);
}
```

signal 保存一个值。调用它会读取该值；如果处在追踪作用域内（JSX 表达式、`computed`、`effect`），该作用域会订阅它。`set` 和 `update` 写入新值，所有读过它的地方随之更新。

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

默认情况下，写入与当前值 `===` 相等的值不会产生任何效果。传入 `equals: false` 可以让每次写入都通知订阅者（适合原地修改对象后再次写入同一个对象的场景），也可以传入自定义比较函数：

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

由 signal 派生出的值。`fn` 读过的 signal 变化时它会重新运行；只有当结果真的变化（按 `equals` 判断，默认 `===`）时，读取它的地方才会更新。`fn` 会收到上一次的结果。

```ts
const first = signal("Ada");
const last = signal("Lovelace");
const full = computed(() => `${first()} ${last()}`);
full(); // "Ada Lovelace"
```

派生计算开销较大或在多处读取时使用 `computed`。对于简单的表达式，普通函数（`const full = () => first() + " " + last()`）同样是响应式的。

## effect

```ts
function effect(fn: () => void): void;
```

在渲染之后运行 `fn`，并在它读过的任何 signal 变化时再次运行。effect 用于副作用：打日志、定时器、调用原生模块等。effect 归属于创建它的组件（或 effect），并随之一起销毁。

```ts
const query = signal("");
effect(() => {
  console.log("query is now", query());
});
```

如需在 effect 再次运行前（或组件销毁时）撤销某些操作，在 effect 内部调用 [`cleanup`](#cleanup)。

## batch

```ts
function batch<T>(fn: () => T): T;
```

运行 `fn`，并在结束时一次性应用其中的所有写入，这样读取方只更新一次，而不是每次写入都更新一次。返回 `fn` 的返回值。

```ts
batch(() => {
  first.set("Grace");
  last.set("Hopper");
}); // `full` 只重新计算一次
```

## untracked

```ts
function untracked<T>(fn: () => T): T;
```

运行 `fn`，但当前作用域不会订阅其中读取的内容。用于在 effect 或 JSX 表达式中读取某个 signal 而不依赖它。

```ts
effect(() => {
  // `query` 变化时重新运行，`page` 变化时不会。
  search(query(), untracked(() => page()));
});
```

## cleanup

```ts
function cleanup(fn: () => void): void;
```

注册 `fn`，在当前 effect 再次运行之前，或当前组件、effect 被销毁时运行。

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

组件向其渲染的所有后代（无论多深）提供的值。调用 context（`Theme()`）读取最近的 Provider 提供的值，没有 Provider 时得到 `defaultValue`。

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

把 `app` 渲染到界面上（窗口，或嵌入应用时的 `NshAppView` / `NshView`），返回一个用于移除它的函数。每个应用的入口文件都以 `mount(App)` 结尾。

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

把 props 对象拆成 `keys` 中列出的属性和其余属性。两部分都保持响应式：会变化的属性是 getter，`split` 会保留这些 getter。适用于组件自己处理一部分 props、把其余部分透传下去的场景。直接解构 props（`const { label } = props`）只会读取一次，之后的更新就丢失了。

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

## View、Text、Scroll、Input

```ts
function View(props: ViewProps): YNode;
function Text(props: TextProps): YNode;
function Scroll(props: ScrollProps): YNode;
function Input(props: InputProps): YNode;
```

`nsh` 导出的宿主元素，写作标签：`<View>`、`<Text>`、`<Scroll>`、`<Input>`。当标签通过导入（包括别名）指向它们之一时，编译器会就地创建原生视图，所以导入它们在运行时没有任何开销。它们也是普通的组件，可以作为值传递（`const Tag = inline ? Text : View`）。其他元素来自各自的模块：`Image` 来自 [`nsh/image`](./nsh-image.md)，`List` 来自 [`nsh/list`](./nsh-list.md)，`Canvas`、`WebView` 和 `Video` 来自各自的模块。它们的属性见[元素](../elements/index.md)。

## 类型

`nsh` 还导出响应式 API 和宿主元素的类型。最常用的有：

| 类型 | 说明 |
|---|---|
| `Accessor<T>` | 可读取的值：`() => T` |
| `Signal<T>` | 带有 `set` 和 `update` 的 `Accessor<T>` |
| `SignalOptions<T>` | `signal` 和 `computed` 的选项（`equals`） |
| `Context<T>` | `context` 的返回值 |
| `Component<P>` | `(props: P) => JSX.Element` |
| `JSX` | JSX 命名空间（`JSX.Element`） |
| `YNode` | `ref` 拿到的对象：聚焦、滚动等命令。见[元素](../elements/index.md) |
| `ViewProps`、`TextProps`、`ImageProps`、`ScrollProps`、`InputProps`、`CommonProps`、`TouchProps` | 宿主元素的属性 |
| `ViewStyle`、`TextStyle`、`ImageStyle`、`LayoutStyle`、`StyleProp<T>` | 样式对象；见[样式参考](./style-reference.md) |
| `ColorValue`、`DimensionValue`、`LengthValue`、`TransformStep` | 样式中的值 |
| `GestureResponderEvent`、`LayoutEvent`、`ScrollEvent`、`PanEvent`、`TouchPoint` | 事件数据 |
| `ImageAsset` | 导入图片文件得到的对象（`{ uri, width, height }`） |
| `OS` | `"ios" \| "android" \| "macos" \| "windows" \| "linux"` |
| `AccessibilityRole`、`AccessibilityState` | 无障碍属性 |

```tsx
import { type GestureResponderEvent, type YNode, signal } from "nsh";

let field: YNode | undefined;
const onPress = (e: GestureResponderEvent) => field?.focus();
```

`StyleProp<T>` 接受单个样式，或一个从左到右合并的样式列表（假值会被跳过）：

```tsx
<View style={[styles.card, selected() && styles.selected]} />
```

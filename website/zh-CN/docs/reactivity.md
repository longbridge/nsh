---
title: 响应式
description: signal、computed、effect、batch、untracked、cleanup 和 context：NSH 如何只更新真正变化的部分，以及需要避开的常见错误。
order: 31
group: ui
demo: guide-reactivity
---

# 响应式

NSH 用 **signal** 更新界面：signal 是知道谁在读取自己的值。signal 变化时，NSH 只重新运行读取了它的那些计算（一段文本、一个样式、一个 effect），其他一概不动。组件不会重新渲染。这套模型来自 Solid，而响应式图本身是原生实现的：读取 signal 是一次对 Rust 的调用，调度、依赖跟踪和清理都在解释器之外完成。

本页的所有内容都从 `nsh` 导入：

```tsx
import { batch, cleanup, computed, context, effect, signal, untracked } from "nsh";
```

## signal

`signal(value)` 创建一个 signal。调用它读取值；`set` 和 `update` 写入值：

```tsx
const count = signal(0);

count();                     // 0, and tracks the read
count.set(5);
count.update((n) => n + 1);  // 6
```

在跟踪作用域（JSX、`computed`、`effect`）内读取 signal，会让该作用域订阅这个 signal。写入时如果值发生了变化（默认用 `===` 比较），就会通知订阅者。

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

点击只会更新读取 `count()` 的那一个文本节点。`Counter` 本身只运行一次。

不带初始值的 `signal<T>()` 从 `undefined` 开始，类型为 `Signal<T | undefined>`。第二个参数决定什么样的写入算作变化：

```tsx
const position = signal({ x: 0, y: 0 }, { equals: (a, b) => a.x === b.x && a.y === b.y });
const tick = signal(0, { equals: false });   // every write notifies
```

### 对象与数组

signal 用 `===` 比较，因此原地修改的值不算变化。请写入新的对象或数组：

```tsx
const todos = signal<Todo[]>([]);

todos.update((list) => [...list, { id: 3, title: "New" }]);                 // notifies
todos.update((list) => list.map((t) => (t.id === id ? { ...t, done: true } : t)));

todos().push(item);    // does not notify anyone
todos.set(todos());    // still the same array: no change
```

`<Each>` 按项为行设置 key，所以在新数组中替换某一项时，只会重建那一行（[JSX 与组件](./jsx-and-components.md)）。

## computed

`computed(fn)` 从其他 signal 派生出一个值。它运行 `fn` 并记住结果，当它读取过的 signal 变化时再重新运行。读取它和读取 signal 一样：

```tsx
const price = signal(12);
const quantity = signal(2);
const total = computed(() => price() * quantity());

<Text>Total: {total()}</Text>
```

当派生值在多处被读取，或计算开销较大时，使用 `computed`：无论有多少读取者，每次变化它只运行一次，而且只有结果真正变化时（`===`，或 `equals` 选项）才会通知读取者。对于只读一次、计算很便宜的值，普通函数就够了：

```tsx
const label = () => `${quantity()} items`;
<Text>{label()}</Text>
```

`fn` 会收到上一次的值（第一次为 `undefined`），适合需要累积的值：

```tsx
const max = computed((prev: number | undefined) => Math.max(prev ?? 0, speed()));
```

## effect

`effect(fn)` 在当前渲染完成后运行 `fn`，之后每当它读取过的 signal 变化时再次运行。effect 用于副作用：日志、定时器、网络请求、对 [ref](./jsx-and-components.md) 的命令式调用。

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

effect 只跟踪它同步运行期间的读取。`.then` 回调中或 `await` 之后的读取不会被跟踪。

effect 在同一轮创建的元素都已存在之后才运行，因此 ref 已经被设置：

```tsx
let input: YNode | undefined;
effect(() => {
  if (editing()) input?.focus();
});
```

> [!TIP]
> 不要用 effect 让一个 signal 与另一个保持同步（`effect(() => b.set(a() * 2))`）。请使用 `computed`：它永远不会过期，也不会引起第二轮更新。一个 effect 如果写入了它自己读取的 signal，就会反复触发自身，直到 NSH 以错误终止它。

## resource

`resource(source, fetcher)` 异步加载一个值，并记录加载进行到哪一步，这样组件不用自己管理，就能显示数据、加载中或错误：

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

`source` 是一个 accessor（signal 或函数）或者一个普通值，它会被追踪：每当它给出新值，就调用 `fetcher(value, info)` 重新加载。`source` 为 `undefined`、`null` 或 `false` 时不加载，适合"用户还没登录""还没选中"这类情况。不传 source 时，`resource(fetcher)` 只加载一次，调用 `fetcher(info)`。

| 成员 | 含义 |
|---|---|
| `posts()` | 最新的值：第一次加载结束前为 `undefined`；重新加载时和加载失败后保留原值。 |
| `posts.loading()` | 是否正在加载。 |
| `posts.error()` | 上一次加载的错误；某次加载成功后变回 `undefined`。 |
| `posts.state()` | `"unresolved"`（没有可加载的）、`"pending"`（首次加载）、`"ready"`、`"refreshing"`（重新加载，原值保留）、`"errored"`。 |
| `posts.refetch()` | 用同一个 source 重新加载：重试、下拉刷新。 |
| `posts.mutate(value)` | 不发请求，直接在本地设置值（乐观更新），并清除错误。 |

这些成员都已绑定到各自的 resource，所以 `onRefresh={posts.refetch}`、`const loading = posts.loading` 都可以直接用。

只有最新一次加载有效：新的加载会通过 `info.signal`（可直接传给 `fetch`）取消正在进行的那次，组件被移除时也会取消；被取代后才结束的加载不会改变任何东西。`info.refetching` 表示这次加载由 `refetch()` 发起。`fetcher` 可以返回 promise，也可以直接返回值；抛出异常即视为失败。传入 `{ initialValue }` 时，resource 从一开始就有值，它的类型里也就不含 `undefined`。

Rust 组件有同样的 API：`resource(move || source(), |value, info| async move { .. })`，用 `Result` 表示结果（[用 Rust 开发](./rust.md)）。

## cleanup

`cleanup(fn)` 注册 `fn`，在当前 effect 再次运行之前，或者在它所属的组件或 effect 被销毁时运行：

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

effect 也可以像 React 那样直接返回清理函数：`fn` 返回的函数和传给 `cleanup` 的函数在同样的时机运行。

```tsx
effect(() => {
  const id = setInterval(tick, 100);
  return () => clearInterval(id);
});
```

组件不再被渲染时就会被销毁：它所在的 `<If>` 分支被隐藏、所在的 `<Each>` 行被移除，或者它的页面被弹出。它创建的一切（effect、computed 值、cleanup、嵌套组件）都会随之释放。

## batch

每次写入都会立即通知订阅者。要让多次写入算作一次变化，用 `batch` 包起来：依赖方会在最后一次写入之后只运行一次。

```tsx
const save = () =>
  batch(() => {
    firstName.set(form.first);
    lastName.set(form.last);
    dirty.set(false);
  });
```

事件处理函数不会自动批处理：在一个 `onPress` 中写入两次，读取这两个 signal 的 effect 就会运行两次。`batch` 返回其函数的返回值。

## untracked

`untracked(fn)` 运行 `fn`，但不订阅它读取的内容。在跟踪作用域中，用它读取一个值而不对它产生依赖：

```tsx
effect(() => {
  const q = query();                     // runs again when query changes
  const filters = untracked(activeFilters); // read, but not a reason to run again
  search(q, filters);
});
```

组件函数体本身就不被跟踪：在那里读取 signal 只读一次。

## context

`context(defaultValue)` 创建一个 context：组件向它渲染的所有内容（无论多深）提供的值。调用 context 读取最近一层提供的值（或默认值），渲染 `<Ctx.Provider value>` 来提供一个值：

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

provider 只在创建时读取一次 `value`。要提供会变化的内容，就提供一个 signal（或由 signal 组成的对象），并在 JSX 中读取它：

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

请在组件函数体中（或在组件创建期间运行的代码中）读取 context，而不是在事件处理函数或定时器中读取：那里没有可供查找的组件。

## 什么会运行，何时运行

| 你写的 | 何时运行 | 何时再次运行 |
|---|---|---|
| 组件函数体 | 创建时运行一次 | 从不（热重载除外） |
| 调用函数或读取属性的 `{expr}` 或 `prop={expr}` | 元素创建时运行一次 | 它读取的 signal 变化时；只更新那一个 prop 或子节点 |
| `computed(fn)` | 创建时 | 它读取的 signal 变化时 |
| `effect(fn)` | 当前这一轮的元素都已存在之后 | 它读取的 signal 变化时 |
| `<If when>` | 创建时 | `when` 的真假变化时 |
| `<Each of>` 的行 | 它的 key 第一次出现时 | 从不；改为由 `item()` / `index()` 访问器变化 |

环境同样是响应式的：[`nsh/device`](../modules/nsh-device.md) 中的 `windowSize()`、`colorScheme()`、`fontScale()`，以及 `nsh/ui/mobile` 中的 `safeArea()` / `keyboard()` 都是访问器，读取它们的 JSX 会跟随屏幕旋转、深色模式或键盘变化。

## 常见错误

**解构 props。** `const { title } = props` 只会读取一次 `title`。请在使用的地方读取 `props.title`，或用 [`split`](./jsx-and-components.md) 拆分 props。

**在组件函数体中读取 signal 并期待它更新。**

```tsx
function Bad() {
  const n = count();           // read once
  return <Text>{n}</Text>;     // never changes
}

function Good() {
  return <Text>{count()}</Text>;
}
```

**原地修改而不是重新设置。** `list().push(x)` 改变了数组，却不会通知任何人。请写入一个新数组。

**用 effect 派生状态。** `effect(() => total.set(price() * qty()))` 能工作，但在写入和 effect 运行之间 `total` 是过期的，还要多付出一轮更新的代价。请使用 `computed`。

**在组件之外创建 effect。** effect 和 computed 值属于创建它们的组件，并随组件一起释放。如果在事件处理函数或定时器回调中创建，它们没有所有者，会永远存在。请在组件函数体中创建，让它们去读取会变化的 signal。

**期待 `async` 代码被跟踪。** 只有 effect 或 computed 值运行期间同步发生的读取才会被跟踪。先读取 signal，再开始异步工作。

## 相关内容

- [JSX 与组件](./jsx-and-components.md)：作为 getter 的 props、`<If>`、`<Each>`。
- [`nsh` 模块参考](../modules/nsh.md)：每个函数的签名。
- [动画](./animation.md)：不经过 signal、在 UI 线程上运行动画的动画值。

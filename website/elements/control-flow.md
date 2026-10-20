---
title: If / Each
description: "The control flow components: <If> renders a branch by a condition, <Each> renders a keyed row per item of an array."
order: 12
demo: el-control-flow
---

# If / Each

`<If>` and `<Each>` are the components for conditional and repeated content. They create no native view; they decide which
elements exist. Both are compiled into calls to the native renderer, which updates only what changed: `<If>` swaps a
branch when the condition's truthiness changes, `<Each>` adds, moves and removes rows by key instead of rebuilding the
list.

```tsx
<View>
  <If when={user()} else={<Text>Signed out</Text>}>
    {(u) => <Text>Hello, {u().name}</Text>}
  </If>
  <Each of={todos()}>{(todo) => <Text>{todo().title}</Text>}</Each>
</View>
```

They are globals: they need no import (a binding of the same name in scope takes their place). Inside JSX expressions you can still use plain JavaScript (`cond ? a : b`, `items.map(…)`), but those
rebuild their elements every time what they read changes; `<If>` and `<Each>` keep the elements that stay.

## `<If>`

Renders its children while `when` is truthy, else the `else` element (or nothing).

| Prop | Type | Description |
|---|---|---|
| `when` | `T` | The condition. Only its truthiness matters. |
| `else` | `JSX.Element` | Rendered while `when` is falsy. |
| `children` | `JSX.Element` \| `(value: () => NonNullable<T>) => JSX.Element` | Rendered while `when` is truthy. A function gets an accessor of `when`'s value. |

The branch is created again only when the truthiness of `when` changes. A `when` that goes from one truthy value to
another (`user()` changing from one object to another) keeps the branch, and the expressions inside it update on their
own.

```tsx
const loading = signal(true);

<If when={!loading()} else={<Text style={{ color: "#6e7781" }}>Loading…</Text>}>
  <Profile />
</If>;
```

A function child reads the value `when` gave, without `null` or `undefined` in its type, so the branch needs no `!`:

```tsx
const user = signal<User | null>(null);

<If when={user()}>{(u) => <Text>{u().name}</Text>}</If>;
```

## `<Each>`

Renders `children(item, index)` for each item of `of`.

| Prop | Type | Description |
|---|---|---|
| `of` | `readonly any[] \| null \| undefined` | The items. `null` and `undefined` render nothing. |
| `key` | `(item: any) => unknown` | The identity of an item. Default: the item itself (objects by identity, primitives by value). |
| `children` | `(item: () => any, index: () => number) => JSX.Element` | Renders one row. |

The row function gets **accessors**, not values: call `item()` and `index()`. When `of` changes, rows whose key is still
there are kept (and moved if their position changed); their `item()` and `index()` update, and only the expressions
reading them run again. New keys get new rows; rows whose key is gone are removed.

```tsx
interface Todo {
  id: number;
  title: string;
  done: boolean;
}

const todos = signal<Todo[]>([]);

<Each of={todos()} key={(t: Todo) => t.id}>
  {(todo, index) => (
    <View style={{ flexDirection: "row", gap: 8 }}>
      <Text>{index() + 1}.</Text>
      <Text style={{ textDecorationLine: todo().done ? "line-through" : "none" }}>{todo().title}</Text>
    </View>
  )}
</Each>;
```

### Choosing a key

- Items replaced by new objects on every update (immutable updates, `{ ...t, done: true }`) need a `key`, usually an id.
  Without it each new object is a new row and its elements are created again.
- Arrays of strings or numbers work without a key: they are compared by value.
- Each row has its own owner: effects and cleanups created in a row end when the row is removed.

> [!NOTE]
> `Each` is generic: TypeScript infers the item type from `of`, so the row function's `todo()` and the `key` parameter
> are typed without annotations.

## `<Each>` or a list

`<Each>` creates every row. That is right for tens or a few hundred items, in a `<View>` or a [`<Scroll>`](./scroll.md).
For long or unbounded data, use a [list](./list.md): it creates only the rows near the viewport and recycles them.

See [JSX and components](../docs/jsx-and-components.md) and [reactivity](../docs/reactivity.md) for how expressions
update.

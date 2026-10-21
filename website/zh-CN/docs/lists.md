---
title: 列表
description: 用 nsh/list 的 List 渲染长列表和无限列表：复用、条目标识、多列、吸顶标题、倒置的聊天列表、横向列表、加载更多、可见性，以及滚动到指定条目。
order: 40
group: ui
demo: guide-lists
---

# 列表

`nsh/list` 中的 `List` 在原生滚动视图上渲染长数据，但只创建屏幕附近的行。哪些行存在、放在哪里、多列布局、吸顶标题以及哪些条目可见，全部由原生计算；只有当原生端要一个单元格时，你的 JS 才去填充内容。一百万条的列表，开销和三十条的差不多。

```tsx
import { mount, Text, View } from "nsh";
import { List } from "nsh/list";

const items = Array.from({ length: 10_000 }, (_, i) => ({ id: i, title: `Item ${i + 1}` }));

function App() {
  return (
    <List each={items} estimatedItemSize={52} style={{ flex: 1, backgroundColor: "#ffffff" }}>
      {(item) => (
        <View style={{ paddingHorizontal: 16, paddingVertical: 16, borderBottomWidth: 1, borderBottomColor: "#eaeef2" }}>
          <Text style={{ fontSize: 16 }}>{item().title}</Text>
        </View>
      )}
    </List>
  );
}

mount(App);
```

和 [`<Scroll>`](./scrolling.md) 一样，列表本身是视口，需要父元素给出尺寸（`flex: 1` 或一个高度）。

## 复用是如何工作的

行函数不是每个条目调用一次，而是**每个单元格**调用一次，并且单元格会被复用：某一行滚出屏幕足够远后，它的单元格会被分配给另一个条目，`item()` 和 `index()` 随之改变。行中读取它们的地方都会原地更新，就像其他任何 [signal](./reactivity.md) 一样。

由此得出两条规则：

**在用到的地方读取 `item()`。** 在行函数开头复制出来的值，只属于这个单元格第一次显示的那个条目。

```tsx
// 错误：单元格被复用后，`title` 仍是第一个条目的标题。
{(item) => {
  const title = item().title;
  return <Text>{title}</Text>;
}}

// 正确：文本跟随单元格当前的条目。
{(item) => <Text>{item().title}</Text>}
```

**把每个条目的状态放在数据里，而不是行里。** 在行内创建的 signal 属于单元格，会跟着它去到下一个条目。"这一行是否展开 / 选中 / 点赞" 应该存放在条目中，或存放在以 id 为键的映射里：

```tsx
const liked = signal<Set<number>>(new Set());
const toggle = (id: number) =>
  liked.update((s) => {
    const next = new Set(s);
    next.has(id) ? next.delete(id) : next.add(id);
    return next;
  });

<List each={posts()} estimatedItemSize={64} style={{ flex: 1 }}>
  {(post) => (
    <View style={{ flexDirection: "row", padding: 16 }}>
      <Text style={{ flex: 1 }}>{post().title}</Text>
      <Text onPress={() => toggle(post().id)}>{liked().has(post().id) ? "♥" : "♡"}</Text>
    </View>
  )}
</List>
```

各行可以高度不同。在测量之前，列表假定行高为 `estimatedItemSize`（默认 50 点）；传入接近典型行高的值，滚动条和跳转到远处条目从一开始就会比较准确。

## 修改数据

传入新数组即可改变条目：`posts.set([...posts(), ...more])`。列表会**按条目标识**（`===`）比较新旧数组，保留相同的开头和结尾，中间部分视为删除和插入。测量结果跟随各自的条目；在屏幕上可见的行之上插入条目时，可见行保持原位，不会跳动。

- 用不可变的方式更新：新数组，未改变的条目对象原样复用。原地修改数组（`push`）不会被察觉。
- 改动过的条目应是一个新对象（`{ ...post, title }`），这样它的行才会显示变化。
- 在前面插入（新消息、刷新后的信息流）会保留用户的阅读位置。

`keyExtractor` 为可见性回调中的条目命名（见下文）；默认依次取 `item.key`、`item.id`、索引。它不影响改动的匹配方式，匹配总是按标识进行。

## 属性

`List` 接受 `<Scroll>` 中对列表有意义的所有滚动属性（`pagingEnabled`、`snapToInterval`、`snapToOffsets`、`snapToAlignment`、`decelerationRate`、`disableIntervalMomentum`、`bounces`、`scrollEnabled`、`contentInset`、`contentOffset`、`keyboardShouldPersistTaps`、`keyboardDismissMode`、`scrollEventThrottle`、`onScroll`、`onScrollBeginDrag`、`onScrollEndDrag`、`onMomentumScrollBegin`、`onMomentumScrollEnd`、`onScrollEnd`、`scrollValue`），说明见[滚动](./scrolling.md)。列表不能缩放。它自己的属性：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `each` | `readonly T[]` | | 条目。 |
| `children` | `(item: () => T, index: () => number) => JSX.Element` | | 渲染一个单元格；每个单元格调用一次，见[复用](#复用是如何工作的)。 |
| `estimatedItemSize` | `number` | `50` | 尚未测量的行的尺寸，单位为点。 |
| `horizontal` | `boolean` | `false` | 横向滚动。 |
| `numColumns` | `number` | `1` | 每行的条目数（纵向列表）。 |
| `inverted` | `boolean` | `false` | 第 0 项在底部（聊天）；初始滚动到末尾，并保持与末尾的距离。 |
| `stickyHeaderIndices` | `number[]` | | 滚过时吸附在顶部的条目（单列且非倒置）。 |
| `contentStyle` | `{ padding*, gap, rowGap, columnGap }` | | 内容的内边距和间距，只接受数字。 |
| `contentClass` | `string` | | 同上，写成类名（`p-*`、`gap-*`）；`contentStyle` 优先。 |
| `header` | 组件或元素 | | 在条目之前。 |
| `footer` | 组件或元素 | | 在条目之后。 |
| `empty` | 组件或元素 | | `each` 为空时代替条目显示。 |
| `separator` | 组件或元素 | | 条目之间（单列）。 |
| `initialScrollIndex` | `number` | | 第一帧之前滚动到该条目。 |
| `onEndReached` | `() => void` | | 接近末尾（见下文）。 |
| `onEndReachedThreshold` | `number` | `1` | 距离末尾多少个视口时触发 `onEndReached`。 |
| `onViewableItemsChanged` | `(info) => void` | | 有条目变为可见或不再可见。 |
| `viewabilityConfig` | `ViewabilityConfig` | | 什么情况下条目算作可见。 |
| `keyExtractor` | `(item, index) => string` | | 可见性回调中的键。 |
| `refreshing` / `onRefresh` | `boolean` / `() => void` | | 下拉刷新。 |
| `showsIndicator` | `boolean` | `true` | 显示滚动条。在桌面和桌面浏览器中，横向视图始终保留滚动条：它是鼠标的滚动条。 |
| `ref` | `YNode` | | `scrollToIndex`、`scrollToOffset`、`scrollToEnd`、`scrollTo`。 |

头部、尾部和空状态插槽接受组件（`header={Header}`）或元素（`header={<Header />}`）。它们是独立的单元格，从不被复用，并且和其他 JSX 一样是响应式的。

## 多列与网格

`numColumns` 把条目排成每行若干列；`contentStyle` 为内容四周添加内边距，并设置行间距（`rowGap`）、列间距（`columnGap`）或两者（`gap`）。网格中各行的高度可以不同：每行与其中最高的条目一样高。

```tsx
<List
  each={photos}
  numColumns={3}
  estimatedItemSize={120}
  style={{ flex: 1 }}
  contentStyle={{ padding: 12, gap: 8 }}
>
  {(photo) => <Image src={photo().thumb} style={{ aspectRatio: 1, borderRadius: 8 }} />}
</List>
```

## 吸顶标题

`stickyHeaderIndices` 列出的条目在内容滚过时吸附在视口顶部，直到下一个吸顶条目把它推出去。分组列表就是一个由标题条目和普通行组成的列表，标题的索引设为吸顶。吸附在每一帧滚动时由原生完成。

```tsx
type Row = { header: string } | { name: string };

const rows: Row[] = [];
const headers: number[] = [];
for (const [letter, names] of groups) {
  headers.push(rows.length);
  rows.push({ header: letter });
  for (const name of names) rows.push({ name });
}

<List each={rows} stickyHeaderIndices={headers} estimatedItemSize={44} style={{ flex: 1 }}>
  {(row) => (
    <If
      when={"header" in row()}
      else={<Text style={{ padding: 12 }}>{(row() as { name: string }).name}</Text>}
    >
      <Text style={{ padding: 6, backgroundColor: "#eaeef2", fontWeight: "700" }}>{(row() as { header: string }).header}</Text>
    </If>
  )}
</List>
```

同一个单元格可能这会儿显示标题、过一会儿显示名字，所以行用 [`<If>`](../elements/control-flow.md) 决定渲染什么，它会在条目改变时切换。吸顶标题要求单列且列表不倒置。

## 聊天：倒置列表

`inverted` 让列表从末尾开始排版：第 0 项在底部，列表初始滚动到末尾，并在条目变化时保持与末尾的距离。把最新的消息放在数组最前面；添加消息时插到前面，用 `onEndReached` 加载更早的消息时追加到后面（倒置列表的"末尾"是顶部）。

```tsx
import { Input, mount, signal, Text, View } from "nsh";
import { List } from "nsh/list";
import { KeyboardAvoidingView } from "nsh/base";

type Message = { id: number; text: string; mine: boolean };

function Chat() {
  let next = 0;
  const message = (text: string, mine: boolean): Message => ({ id: next++, text, mine });
  const messages = signal(Array.from({ length: 30 }, (_, i) => message(`Message ${30 - i}`, i % 3 === 0)));
  const draft = signal("");
  const send = () => {
    if (!draft().trim()) return;
    messages.update((m) => [message(draft(), true), ...m]); // 最新的在最前
    draft.set("");
  };
  const loadOlder = () => messages.update((m) => [...m, ...Array.from({ length: 20 }, (_, i) => message(`Older ${i}`, false))]);
  return (
    <KeyboardAvoidingView style={{ flex: 1 }}>
      <List
        each={messages()}
        inverted
        estimatedItemSize={44}
        style={{ flex: 1 }}
        contentStyle={{ padding: 12, gap: 6 }}
        keyboardDismissMode="interactive"
        onEndReached={loadOlder}
        onEndReachedThreshold={0.5}
      >
        {(m) => (
          <View
            style={{
              alignSelf: m().mine ? "flex-end" : "flex-start",
              maxWidth: "75%",
              paddingHorizontal: 12,
              paddingVertical: 8,
              borderRadius: 14,
              backgroundColor: m().mine ? "#0969da" : "#eaeef2",
            }}
          >
            <Text style={{ color: m().mine ? "#ffffff" : "#1f2328" }}>{m().text}</Text>
          </View>
        )}
      </List>
      <View style={{ flexDirection: "row", gap: 8, padding: 8 }}>
        <Input
          value={draft()}
          onChange={(t) => draft.set(t)}
          onSubmit={send}
          multiline
          submitBehavior="submit"
          returnKeyType="send"
          placeholder="Message"
          style={{ flex: 1, maxHeight: 120, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 18, paddingHorizontal: 12, paddingVertical: 8 }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}

mount(Chat);
```

在倒置列表上调用 `scrollToEnd()` 会滚动到它的开头，也就是最新的消息。

## 横向列表

`horizontal` 让列表横向排列；单元格按列表的高度排版。配合吸附可以做出任意长度的轮播：

```tsx
<List
  each={cards}
  horizontal
  estimatedItemSize={232}
  snapToInterval={232}
  decelerationRate="fast"
  showsIndicator={false}
  style={{ height: 140 }}
  contentStyle={{ paddingHorizontal: 16, gap: 12 }}
>
  {(card) => <View style={{ width: 220, height: 140, borderRadius: 14, backgroundColor: card().color }} />}
</List>
```

在从右到左布局中，横向列表从右侧开始向左排列。

## 加载更多：onEndReached

当视口距离末尾不到 `onEndReachedThreshold` 个视口时（默认 1：提前一屏），触发 `onEndReached`。追加下一页数据；新的末尾临近时会再次触发。

```tsx
const posts = signal<Post[]>([]);
let loading = false;
let page = 0;

const loadMore = async () => {
  if (loading) return;
  loading = true;
  try {
    const next: Post[] = await (await fetch(`https://api.example.com/posts?page=${++page}`)).json();
    posts.set([...posts(), ...next]);
  } finally {
    loading = false;
  }
};
loadMore();

<List
  each={posts()}
  estimatedItemSize={72}
  style={{ flex: 1 }}
  onEndReached={loadMore}
  onEndReachedThreshold={0.5}
  footer={<Text style={{ padding: 16, textAlign: "center", color: "#8c959f" }}>Loading…</Text>}
>
  {(post) => <PostRow post={post()} />}
</List>
```

像上面那样防止重复加载：在一页数据返回之前，末尾可能会被再次报告。

## 可见性

`onViewableItemsChanged` 报告哪些条目在屏幕上，可用于统计曝光、自动播放可见的视频或把消息标记为已读。它收到 `{ viewableItems, changed }`，两者都是由 `{ item, key, index, isViewable }` 组成的数组。可见性由原生计算，JS 只在变化时收到通知。

| `viewabilityConfig` | 说明 |
|---|---|
| `itemVisiblePercentThreshold` | 条目必须可见的百分比。 |
| `viewAreaCoveragePercentThreshold` | 条目必须覆盖视口的百分比。 |
| `minimumViewTime` | 条目保持可见多少毫秒后才被报告。 |

不设阈值时，只要有一个像素可见就算可见。

```tsx
<List
  each={videos}
  estimatedItemSize={300}
  style={{ flex: 1 }}
  keyExtractor={(v) => v.slug}
  viewabilityConfig={{ itemVisiblePercentThreshold: 60, minimumViewTime: 250 }}
  onViewableItemsChanged={({ viewableItems }) => playing.set(viewableItems[0]?.key ?? null)}
>
  {(v) => <VideoCard video={v()} />}
</List>
```

## 从代码滚动

给列表设置 `ref`：

| 方法 | 说明 |
|---|---|
| `scrollToIndex({ index, animated?, viewPosition?, viewOffset? })` | 把第 `index` 项滚动到可见。`viewPosition` 决定位置：0 在视口开头（默认），0.5 在中间，1 在末尾；`viewOffset` 再额外偏移。 |
| `scrollToOffset({ offset, animated? })` | 沿列表主轴滚动到内容偏移处。 |
| `scrollToEnd({ animated? })` | 滚动到内容末尾（倒置列表则是开头）。 |
| `scrollTo({ x?, y?, animated? })` | 滚动到某个偏移，与 `<Scroll>` 相同。 |

```tsx
import { type YNode } from "nsh";
import { List } from "nsh/list";

let list!: YNode;

<List ref={list} each={items} estimatedItemSize={48} style={{ flex: 1 }}>…</List>

list.scrollToIndex({ index: 120, viewPosition: 0.5 });
```

`scrollToIndex`（以及 `scrollToEnd`）由原生根据当时已知的尺寸计算：已测量的行用实测值，其余用 `estimatedItemSize`。之后在目标附近的行被测量时，列表会持续跟随目标，因此能准确落位，直到用户自己滚动或数据变化为止。要让列表一打开就停在某个条目，请用 `initialScrollIndex`，而不是在第一帧之后再滚动。

<a id="a-settings-style-list"></a>

## 设置风格的列表

列表的外观属于应用自己：`List` 负责摆放和复用行，行、分隔线和表面都是你的视图。分组的设置风格列表就是一个带内边距、分隔线、首尾行圆角的 `List`。把它写成自己的组件，在应用中需要列出设置的地方使用：

```tsx
import { Text, View } from "nsh";
import { List } from "nsh/list";

type Setting = { id: string; label: string };

function SettingsList(props: { each: Setting[]; onSelect: (s: Setting) => void }) {
  return (
    <View class="flex-1 bg-neutral-100 dark:bg-neutral-950">
      <List
        each={props.each}
        estimatedItemSize={48}
        style={{ flex: 1 }}
        contentStyle={{ padding: 16 }}
        separator={() => (
          <View class="bg-white dark:bg-neutral-900">
            <View class="ml-4 h-px bg-neutral-200 dark:bg-neutral-800" />
          </View>
        )}
      >
        {(s, i) => (
          <View
            onPress={() => props.onSelect(s())}
            role="button"
            class="h-12 justify-center bg-white px-4 active:bg-neutral-200 dark:bg-neutral-900"
            classList={{ "rounded-t-xl": i() === 0, "rounded-b-xl": i() === props.each.length - 1 }}
          >
            <Text>{s().label}</Text>
          </View>
        )}
      </List>
    </View>
  );
}
```

## 性能提示

- 每个单元格单独排版，滚动时只重新排版有变化的单元格。行的层级保持浅一些，避免在行里读取每帧都会变化的 signal。
- 原生端分批请求单元格，每帧可以进行多轮，合并成一次更新；快速滑动时一帧内绑定许多单元格，也不会每个单元格调用一次 JS。
- 用 [`nsh test --trace` 加上 `scroll` 步骤](./testing-and-trace.md#滚动)测量行在每一帧的开销。

## 相关内容

- [`List` 元素](../elements/list.md) 与 [`nsh/list`](../modules/nsh-list.md)：参考。
- [滚动](./scrolling.md)：列表与 `<Scroll>` 共用的滚动属性。
- [响应式](./reactivity.md)：为什么 `item()` 是一个函数。
- [性能](./performance.md)：测量列表。

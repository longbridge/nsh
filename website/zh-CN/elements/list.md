---
title: List
description: 虚拟化、可复用单元格的列表：行、多列、吸顶标题、倒序聊天列表、可见性回调和滚动到指定项。
order: 7
---

# List

`<List>` 用来展示很长或无上限的数据。只有视口附近的行才真正存在：用户滚动时，离开视口的行会被回收，用来显示新进入的行。哪些行存在、它们的位置、多列布局、吸顶标题和可见性都在原生侧计算，所以滚动一个上千项的列表，开销和二十项的列表差不多。

```tsx
import { mount, Text, View } from "nsh";
import { List } from "nsh/list";

const contacts = Array.from({ length: 1000 }, (_, i) => ({ id: i, name: `Contact ${i + 1}` }));

function App() {
  return (
    <List each={contacts} estimatedItemSize={56} style={{ flex: 1 }}>
      {(contact, index) => (
        <View style={{ height: 56, paddingHorizontal: 16, justifyContent: "center" }}>
          <Text style={{ fontSize: 16 }}>
            {index() + 1}. {contact().name}
          </Text>
        </View>
      )}
    </List>
  );
}

mount(App);
```

`List` 从 [`nsh/list`](../modules/nsh-list.md) 导入。它是泛型组件：`List<T>` 会检查 `each` 与行函数的条目类型是否一致，TypeScript 会从 `each` 推断出条目类型。

如果列表很短、所有行都应该存在（设置页、表单），在 [`<Scroll>`](./scroll.md) 里用 [`<Each>`](./control-flow.md) 更简单。分组表格样式的列表见 [`ListView`](../components/list-view.md)。[列表指南](../docs/lists.md) 介绍了常见模式：无限加载、聊天、分组。

## 行会被复用

行函数对每个单元格执行一次，而不是对每个条目执行一次。单元格被复用来显示另一个条目时，它的 `item()` 和 `index()` 访问器会变化，读取它们的地方随之更新：

```tsx
<List each={messages()}>
  {(message) => <Text>{message().body}</Text>}
</List>
```

因此，每个条目自己的状态要放在数据里（按 id 区分），不要放在行函数内部创建的 signal 里：那样的 signal 会跟着单元格走，出现在别的条目上。

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `each` | `readonly T[]` | | 条目。 |
| `children` | `(item: () => T, index: () => number) => JSX.Element` | | 渲染一行。 |
| `estimatedItemSize` | `number` | `50` | 尚未测量的条目尺寸（点）。估值越接近，滚动条越稳定。 |
| `horizontal` | `boolean` | `false` | 横向滚动。 |
| `numColumns` | `number` | `1` | 每行的条目数（纵向列表）。 |
| `inverted` | `boolean` | `false` | 从末端开始布局，第 0 项在最后（聊天样式）：初始滚动到末端，内容变化时保持与末端的距离。 |
| `stickyHeaderIndices` | `number[]` | | 滚动经过时吸附在起始端的条目（单列、非倒序）。 |
| `contentContainerStyle` | `ListContentStyle` | | 内容的内边距和间距（点）：`padding`、`paddingHorizontal`、`paddingVertical`、`paddingTop`、`paddingRight`、`paddingBottom`、`paddingLeft`、`gap`、`rowGap`、`columnGap`。 |
| `ListHeaderComponent` | `ListSlot` | | 条目之前的内容：一个组件（`Header`）或一个元素（`<Header />`）。 |
| `ListFooterComponent` | `ListSlot` | | 条目之后的内容。 |
| `ListEmptyComponent` | `ListSlot` | | `each` 为空时代替条目显示。 |
| `ItemSeparatorComponent` | `ListSlot` | | 条目之间的分隔（单列）。 |
| `initialScrollIndex` | `number` | | 第一帧之前先滚动到该条目。 |
| `keyExtractor` | `(item: T, index: number) => string` | `item.key`、`item.id` 或索引 | 条目在可见性 token 中的 key。 |
| `onEndReached` | `() => void` | | 快到末端了：加载更多。 |
| `onEndReachedThreshold` | `number` | `1` | 距末端多近（以视口为单位）时触发 `onEndReached`。 |
| `onViewableItemsChanged` | `(info: { viewableItems: ViewToken<T>[]; changed: ViewToken<T>[] }) => void` | | 有条目变为可见或不再可见。 |
| `viewabilityConfig` | `ViewabilityConfig` | | 条目何时算作可见（见下文）。 |
| `refreshing` | `boolean` | | 显示下拉刷新的转圈。 |
| `onRefresh` | `() => void` | | 用户下拉刷新。 |
| `showsIndicator` | `boolean` | `true` | 显示滚动条。在桌面和桌面浏览器中，横向视图始终保留滚动条：它是鼠标的滚动条。 |
| `scrollValue` | `AnimatedValue` | | 接收滚动偏移的动画值，用于随滚动变化的动画。 |
| `keyboardDismissMode` | `"none" \| "on-drag" \| "interactive"` | `"none"` | 拖动时是否收起键盘。 |
| `style` | `StyleProp<ViewStyle>` | | 列表自身的样式（需要有确定的尺寸：`flex: 1` 或高度）。 |
| `ref` | `YNode \| (node: YNode) => void` | | 列表的节点（见 [Ref 方法](#ref-方法)）。 |

列表同样支持 [`<Scroll>`](./scroll.md) 的滚动属性：`pagingEnabled`、`snapToInterval`、`snapToOffsets`、`snapToAlignment`、`disableIntervalMomentum`、`decelerationRate`、`bounces`、`contentInset`、`contentOffset`、`keyboardShouldPersistTaps`、`scrollEventThrottle` 和 `scrollEnabled`。列表不支持缩放。

所有元素通用的属性（[通用属性](./index.md)）原样传给列表的滚动视图：`testID`、`accessibilityLabel` 等无障碍属性、`class` 和 `classList`、`onLayout`、触摸和响应者属性。UI 测试按 `testID` 找到列表，和找其他视图一样。

`ViewabilityConfig`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `itemVisiblePercentThreshold` | `number` | 条目可见部分须达到的百分比。 |
| `viewAreaCoveragePercentThreshold` | `number` | 条目须覆盖视口的百分比。两个阈值都不设时，露出任何一个像素都算可见。 |
| `minimumViewTime` | `number` | 条目须保持可见多少毫秒才会上报。 |

`ViewToken<T>` 为 `{ item, key, index, isViewable }`。

## 事件

| 事件 | 载荷 | 时机 |
|---|---|---|
| `onScroll` | `ScrollEvent` | 偏移发生变化。 |
| `onScrollBeginDrag` / `onScrollEndDrag` | `{ x, y }` | 开始 / 结束拖动。 |
| `onMomentumScrollBegin` / `onMomentumScrollEnd` | `{ x, y }` | 惯性滑动开始 / 结束。 |
| `onScrollEnd` | `{ x, y }` | 滚动停止。 |
| `onEndReached` | 无 | 距末端不足 `onEndReachedThreshold` 个视口。 |
| `onViewableItemsChanged` | `{ viewableItems, changed }` | 可见性变化。 |
| `onRefresh` | 无 | 下拉刷新。 |

## Ref 方法

| 方法 | 说明 |
|---|---|
| `scrollToIndex({ index, animated?, viewPosition?, viewOffset? })` | 把第 `index` 项滚动到视口内。`viewPosition` 决定位置：0 在视口起始处（默认），0.5 居中，1 在末端；`viewOffset` 再额外偏移。在原生侧按当前（已测量或估计的）条目尺寸计算。 |
| `scrollToOffset({ offset, animated? })` | 沿列表方向滚动到某个偏移。 |
| `scrollToEnd({ animated? })` | 滚动到内容末端（倒序列表则是起始端）。 |
| `scrollTo({ x?, y?, animated? })` | 滚动到某个偏移。 |

## 示例

带标题和可见性回调的网格：

```tsx
const visible = signal("");
let list!: YNode;

<List
  ref={list}
  each={photos}
  numColumns={3}
  estimatedItemSize={120}
  contentContainerStyle={{ padding: 12, gap: 8 }}
  viewabilityConfig={{ itemVisiblePercentThreshold: 50 }}
  onViewableItemsChanged={({ viewableItems }) => visible.set(`${viewableItems.length} visible`)}
  ListHeaderComponent={<Text onPress={() => list.scrollToIndex({ index: 60, viewPosition: 0.5 })}>Jump to 60</Text>}
  style={{ flex: 1 }}
>
  {(photo) => <Image src={photo().url} style={{ height: 120, borderRadius: 8 }} />}
</List>;
```

聊天：

```tsx
<List each={messages()} inverted estimatedItemSize={64} keyboardDismissMode="interactive" style={{ flex: 1 }}>
  {(m) => <Bubble message={m()} />}
</List>
```

第 0 项是最新的消息，画在最底部；新消息插到 `each` 开头时，用户正在看的内容不会跳动。

## 平台说明

- 在可见行之上插入条目时，两个平台都会保持可见行不动。
- 从右到左布局中的横向列表在原生侧镜像。
- 吸顶标题在每一帧滚动时于原生侧固定。

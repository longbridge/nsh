---
title: List
description: "The virtualized, recycling list: rows, columns, sticky headers, inverted chat lists, viewability and scrolling to items."
order: 7
---

# List

`<List>` shows long or unbounded data. Only the rows near the viewport exist: as the user scrolls, rows that leave are
recycled for the rows that come in. Which rows exist, their positions, columns, sticky headers and viewability are all
computed natively, so scrolling a list of thousands of items costs no more than a list of twenty.

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

`List` comes from [`nsh/list`](../modules/nsh-list.md). It is generic: `List<T>` checks that `each` and the row
function agree on the item type, which TypeScript infers from `each`.

For a short list whose rows should all exist (a settings page, a form), an [`<Each>`](./control-flow.md) inside a
[`<Scroll>`](./scroll.md) is simpler. For a list styled as a grouped table, see
[`ListView`](../components/list-view.md). The [lists guide](../docs/lists.md) covers patterns: infinite loading, chat,
sections.

## Rows are recycled

The row function runs once per cell, not once per item. When a cell is reused for another item, its `item()` and
`index()` accessors change and everything that reads them updates:

```tsx
<List each={messages()}>
  {(message) => <Text>{message().body}</Text>}
</List>
```

So keep per-item state in your data (keyed by id), not in signals created inside the row function: such a signal would
stay with the cell and show up on another item.

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `each` | `readonly T[]` | | The items. |
| `children` | `(item: () => T, index: () => number) => JSX.Element` | | Renders one row. |
| `estimatedItemSize` | `number` | `50` | The size of items not measured yet, in points. Close estimates keep the scroll bar steady. |
| `horizontal` | `boolean` | `false` | Scrolls sideways. |
| `numColumns` | `number` | `1` | Items per row (vertical lists). |
| `inverted` | `boolean` | `false` | Laid out from the end, item 0 last (chat style): starts scrolled to the end and keeps its distance from the end as content changes. |
| `maintainVisibleContentPosition` | `{ autoscrollToBottomThreshold?: number }` | | Not inverted: at the end of the content, or within that many points of it, the list stays at the end as the content grows or the viewport shrinks; scrolled further up, the visible content stays. |
| `stickyHeaderIndices` | `number[]` | | Items that stick to the start while scrolled past (one column, not inverted). |
| `contentContainerStyle` | `ListContentStyle` | | Padding and gaps of the content, in points: `padding`, `paddingHorizontal`, `paddingVertical`, `paddingTop`, `paddingRight`, `paddingBottom`, `paddingLeft`, `gap`, `rowGap`, `columnGap`. |
| `ListHeaderComponent` | `ListSlot` | | Before the items: a component (`Header`) or an element (`<Header />`). |
| `ListFooterComponent` | `ListSlot` | | After the items. |
| `ListEmptyComponent` | `ListSlot` | | Shown instead of the items while `each` is empty. |
| `ItemSeparatorComponent` | `ListSlot` | | Between items (one column). |
| `initialScrollIndex` | `number` | | Scrolls to this item before the first frame. |
| `keyExtractor` | `(item: T, index: number) => string` | `item.key`, `item.id` or the index | The key of an item in viewability tokens. |
| `onEndReached` | `() => void` | | The end is near: load more. |
| `onEndReachedThreshold` | `number` | `1` | How close to the end (in viewports) `onEndReached` fires. |
| `onViewableItemsChanged` | `(info: { viewableItems: ViewToken<T>[]; changed: ViewToken<T>[] }) => void` | | Items became viewable or stopped being viewable. |
| `viewabilityConfig` | `ViewabilityConfig` | | When items count as viewable (below). |
| `refreshing` | `boolean` | | The pull-to-refresh spinner shows. |
| `onRefresh` | `() => void` | | The user pulled to refresh. |
| `showsIndicator` | `boolean` | `true` | The scroll indicator shows. On the desktop and in desktop browsers a horizontal view keeps it: it is the mouse's scroll bar. |
| `scrollValue` | `NativeValue` | | A native value fed with the scroll offset, for scroll-linked animations. |
| `keyboardDismissMode` | `"none" \| "on-drag" \| "interactive"` | `"none"` | Whether dragging dismisses the keyboard. |
| `style` | `StyleProp<ViewStyle>` | | The list's own style (it needs a bounded size: `flex: 1` or a height). |
| `ref` | `YNode \| (node: YNode) => void` | | The list's node (see [ref methods](#ref-methods)). |

Lists also take the scrolling props of [`<Scroll>`](./scroll.md#scrolling): `pagingEnabled`, `snapToInterval`,
`snapToOffsets`, `snapToAlignment`, `disableIntervalMomentum`, `decelerationRate`, `bounces`, `contentInset`,
`contentOffset`, `keyboardShouldPersistTaps`, `scrollEventThrottle` and `scrollEnabled`. Lists do not zoom.

`ViewabilityConfig`:

| Field | Type | Description |
|---|---|---|
| `itemVisiblePercentThreshold` | `number` | Percent of an item that must be visible. |
| `viewAreaCoveragePercentThreshold` | `number` | Percent of the viewport an item must cover. Without either threshold, any visible pixel counts. |
| `minimumViewTime` | `number` | Milliseconds items must stay viewable before they are reported. |

`ViewToken<T>` is `{ item, key, index, isViewable }`.

## Events

| Event | Payload | When |
|---|---|---|
| `onScroll` | `ScrollEvent` | The offset changed. |
| `onScrollBeginDrag` / `onScrollEndDrag` | `{ x, y }` | Dragging started / stopped. |
| `onMomentumScrollBegin` / `onMomentumScrollEnd` | `{ x, y }` | A glide started / stopped. |
| `onScrollEnd` | `{ x, y }` | Scrolling came to rest. |
| `onEndReached` | none | Within `onEndReachedThreshold` viewports of the end. |
| `onViewableItemsChanged` | `{ viewableItems, changed }` | Viewability changed. |
| `onRefresh` | none | Pulled to refresh. |

## Ref methods

| Method | Description |
|---|---|
| `scrollToIndex({ index, animated?, viewPosition?, viewOffset? })` | Scrolls item `index` into view. `viewPosition` places it: 0 at the start of the viewport (default), 0.5 in the middle, 1 at the end; `viewOffset` shifts it further. Resolved natively with the current (measured or estimated) item sizes. |
| `scrollToOffset({ offset, animated? })` | Scrolls to an offset along the list's axis. |
| `scrollToEnd({ animated? })` | Scrolls to the end of the content (the start of an inverted list). |
| `scrollTo({ x?, y?, animated? })` | Scrolls to an offset. |

## Examples

A grid with a header and viewability:

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

A chat:

```tsx
<List each={messages()} inverted estimatedItemSize={64} keyboardDismissMode="interactive" style={{ flex: 1 }}>
  {(m) => <Bubble message={m()} />}
</List>
```

Item 0 is the newest message, drawn at the bottom; new messages prepended to `each` appear without moving what the
user is reading.

## Platform notes

- Inserting items above the visible rows keeps the visible rows in place on both platforms.
- Horizontal lists in right-to-left layouts are mirrored natively.
- Sticky headers are pinned natively on every scroll frame.

---
title: Lists
description: Long and endless lists with List from nsh/list - recycling, item identity, columns, sticky headers, inverted chat lists, horizontal lists, loading more, viewability and scrolling to an item.
order: 40
group: ui
demo: guide-lists
---

# Lists

`List` from `nsh/list` renders long data on a native scroll view while only creating the rows near the screen. Which rows exist, where they go, the columns, sticky headers and which items are visible are all computed natively; your JS only fills cells with content when the native side asks for one. A list of a million items costs about as much as a list of thirty.

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

Like a [`<Scroll>`](./scrolling.md), the list is the viewport and needs a size from its parent (`flex: 1` or a height).

## How recycling works

The row function is not called once per item. It is called **once per cell**, and cells are reused: when a row scrolls far off screen, its cell is given another item, and `item()` and `index()` change. Everything in the row that reads them updates in place, like any other [signal](./reactivity.md).

Two rules follow from that:

**Read `item()` where you use it.** A value copied out at the top of the row function belongs to whichever item the cell showed first.

```tsx
// Wrong: `title` stays the first item's title when the cell is reused.
{(item) => {
  const title = item().title;
  return <Text>{title}</Text>;
}}

// Right: the text follows the cell's current item.
{(item) => <Text>{item().title}</Text>}
```

**Keep per-item state in your data, not in the row.** A signal created inside the row belongs to the cell and travels with it to the next item. Store "is this row expanded / selected / liked" in the items or in a map keyed by id:

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

Rows can have different heights. Until a row has been measured, the list assumes `estimatedItemSize` (default 50 points); pass a value close to your typical row so the scroll indicator and jumps to far items are accurate from the start.

## Changing the data

Pass a new array to change the items: `posts.set([...posts(), ...more])`. The list compares the old and the new array **by item identity** (`===`), keeps the common start and end, and treats what is between as removed and inserted. Measurements stay with their items, and when items are inserted above the rows on screen, the visible rows stay where they are instead of jumping.

- Update immutably: a new array, with the unchanged item objects reused. Mutating the array in place (`push`) is not seen.
- A changed item should be a new object (`{ ...post, title }`), so its row shows the change.
- Prepending (new messages, a refreshed feed) keeps the user's place.

`keyExtractor` names items in viewability tokens (below); by default the key is `item.key`, then `item.id`, then the index. It does not affect how changes are matched, which is always by identity.

## Props

`List` takes every scrolling prop of `<Scroll>` that makes sense for a list (`pagingEnabled`, `snapToInterval`, `snapToOffsets`, `snapToAlignment`, `decelerationRate`, `disableIntervalMomentum`, `bounces`, `scrollEnabled`, `contentInset`, `contentOffset`, `keyboardShouldPersistTaps`, `keyboardDismissMode`, `scrollEventThrottle`, `onScroll`, `onScrollBeginDrag`, `onScrollEndDrag`, `onMomentumScrollBegin`, `onMomentumScrollEnd`, `onScrollEnd`, `scrollValue`), described in [Scrolling](./scrolling.md). Lists do not zoom. Its own props:

| Prop | Type | Default | Description |
|---|---|---|---|
| `each` | `readonly T[]` | | The items. |
| `children` | `(item: () => T, index: () => number) => JSX.Element` | | Renders a cell; called once per cell, see [recycling](#how-recycling-works). |
| `estimatedItemSize` | `number` | `50` | The size of rows not measured yet, in points. |
| `horizontal` | `boolean` | `false` | Scrolls sideways. |
| `numColumns` | `number` | `1` | Items per row (vertical lists). |
| `inverted` | `boolean` | `false` | Item 0 at the bottom (chat); starts at the end and keeps its distance from the end. |
| `maintainVisibleContentPosition` | `{ autoscrollToBottomThreshold?: number }` | | A list that is not inverted stays at the end while it is within that many points of it, as content grows (see below). |
| `stickyHeaderIndices` | `number[]` | | Items that stick to the top while scrolled past (one column, not inverted). |
| `contentContainerStyle` | `{ padding*, gap, rowGap, columnGap }` | | Padding and gaps of the content, numbers only. |
| `ListHeaderComponent` | component or element | | Before the items. |
| `ListFooterComponent` | component or element | | After the items. |
| `ListEmptyComponent` | component or element | | Shown instead of the items while `each` is empty. |
| `ItemSeparatorComponent` | component or element | | Between items (one column). |
| `initialScrollIndex` | `number` | | Scrolls to this item before the first frame. |
| `onEndReached` | `() => void` | | The end is near (see below). |
| `onEndReachedThreshold` | `number` | `1` | How close to the end, in viewports, `onEndReached` fires. |
| `onViewableItemsChanged` | `(info) => void` | | Items became or stopped being viewable. |
| `viewabilityConfig` | `ViewabilityConfig` | | When items count as viewable. |
| `keyExtractor` | `(item, index) => string` | | The key in viewability tokens. |
| `refreshing` / `onRefresh` | `boolean` / `() => void` | | Pull to refresh. |
| `showsIndicator` | `boolean` | `true` | The scroll indicator shows. On the desktop and in desktop browsers a horizontal view keeps it: it is the mouse's scroll bar. |
| `ref` | `YNode` | | `scrollToIndex`, `scrollToOffset`, `scrollToEnd`, `scrollTo`. |

The header, footer and empty slots take a component (`ListHeaderComponent={Header}`) or an element (`ListHeaderComponent={<Header />}`). They are cells of their own, never recycled, and they are reactive like any other JSX.

## Columns and grids

`numColumns` lays items out in rows of that many columns; `contentContainerStyle` adds padding around the content and gaps between rows (`rowGap`) and columns (`columnGap`), or both (`gap`). Rows of a grid may have different heights: each row is as tall as its tallest item.

```tsx
<List
  each={photos}
  numColumns={3}
  estimatedItemSize={120}
  style={{ flex: 1 }}
  contentContainerStyle={{ padding: 12, gap: 8 }}
>
  {(photo) => <Image src={photo().thumb} style={{ aspectRatio: 1, borderRadius: 8 }} />}
</List>
```

## Sticky headers

`stickyHeaderIndices` lists items that stick to the top of the viewport while the content scrolls past them, until the next sticky item pushes them out. Sections are one list of header and row items; the headers' indices stick. The pinning is done natively on every scroll frame.

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

Because a cell may show a header now and a name later, the row decides what to render with [`<If>`](../elements/control-flow.md), which switches when the item changes. Sticky headers need one column and a list that is not inverted.

## Chat: inverted lists

`inverted` lays the list out from the end: item 0 is at the bottom, the list starts scrolled to the end, and it keeps its distance from the end as items change. Keep the newest message first in the array; prepend to add one, append to load older ones with `onEndReached` (the "end" of an inverted list is the top).

```tsx
import { Input, mount, signal, Text, View } from "nsh";
import { List } from "nsh/list";
import { KeyboardAvoidingView } from "nsh/ui/mobile";

type Message = { id: number; text: string; mine: boolean };

function Chat() {
  let next = 0;
  const message = (text: string, mine: boolean): Message => ({ id: next++, text, mine });
  const messages = signal(Array.from({ length: 30 }, (_, i) => message(`Message ${30 - i}`, i % 3 === 0)));
  const draft = signal("");
  const send = () => {
    if (!draft().trim()) return;
    messages.update((m) => [message(draft(), true), ...m]); // newest first
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
        contentContainerStyle={{ padding: 12, gap: 6 }}
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
          onChangeText={(t) => draft.set(t)}
          onSubmitEditing={send}
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

`scrollToEnd()` on an inverted list scrolls to its start: the newest message.

### Following the end of a list that is not inverted

A list in reading order (oldest first) can follow its end too: with `maintainVisibleContentPosition={{ autoscrollToBottomThreshold: 40 }}`, while the user is at the end (or within 40 points of it), the list stays at the end as the content grows: a reply streaming into the last item, a new message appended, the keyboard shrinking the list. Scrolled further up, the visible content stays where it is and the new content grows below it. Items inserted above the visible ones never move them, with or without the option.

```tsx
<List each={messages()} keyExtractor={(m) => m.id} maintainVisibleContentPosition={{ autoscrollToBottomThreshold: 40 }} style={{ flex: 1 }}>
  {(m) => <Bubble message={m()} />}
</List>
```

## Horizontal lists

`horizontal` turns the list sideways; cells are laid out at the list's height. Combined with snapping it makes a carousel of any length:

```tsx
<List
  each={cards}
  horizontal
  estimatedItemSize={232}
  snapToInterval={232}
  decelerationRate="fast"
  showsIndicator={false}
  style={{ height: 140 }}
  contentContainerStyle={{ paddingHorizontal: 16, gap: 12 }}
>
  {(card) => <View style={{ width: 220, height: 140, borderRadius: 14, backgroundColor: card().color }} />}
</List>
```

Right to left, horizontal lists start at the right and run leftwards.

## Loading more: onEndReached

`onEndReached` fires when the viewport comes within `onEndReachedThreshold` viewports of the end (1 by default: one screen ahead). Append the next page; the list fires again when the new end comes near.

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
  ListFooterComponent={<Text style={{ padding: 16, textAlign: "center", color: "#8c959f" }}>Loading…</Text>}
>
  {(post) => <PostRow post={post()} />}
</List>
```

Guard against overlapping loads as above: the end may be reported again while a page is on its way.

## Viewability

`onViewableItemsChanged` reports which items are on screen, for analytics, autoplaying the visible video, or marking messages read. It gets `{ viewableItems, changed }`, arrays of tokens `{ item, key, index, isViewable }`. Viewability is computed natively; JS only hears about changes.

| `viewabilityConfig` | Description |
|---|---|
| `itemVisiblePercentThreshold` | Percent of an item that must be visible. |
| `viewAreaCoveragePercentThreshold` | Percent of the viewport an item must cover. |
| `minimumViewTime` | Milliseconds an item must stay viewable before it is reported. |

Without a threshold, any visible pixel counts.

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

## Scrolling from code

Give the list a `ref`:

| Method | Description |
|---|---|
| `scrollToIndex({ index, animated?, viewPosition?, viewOffset? })` | Scrolls item `index` into view. `viewPosition` places it: 0 at the start of the viewport (default), 0.5 in the middle, 1 at the end; `viewOffset` shifts it further. |
| `scrollToOffset({ offset, animated? })` | Scrolls to a content offset along the list's axis. |
| `scrollToEnd({ animated? })` | Scrolls to the end of the content (the start of an inverted list). |
| `scrollTo({ x?, y?, animated? })` | Scrolls to an offset, as on `<Scroll>`. |

```tsx
import { type YNode } from "nsh";
import { List } from "nsh/list";

let list!: YNode;

<List ref={list} each={items} estimatedItemSize={48} style={{ flex: 1 }}>…</List>

list.scrollToIndex({ index: 120, viewPosition: 0.5 });
```

`scrollToIndex` (and `scrollToEnd`) is resolved natively with the sizes known at that moment: measured rows where they have been measured, `estimatedItemSize` elsewhere. The list then follows its target while the rows around it are measured, so it lands exactly, until the user scrolls or the items change. To open a list at an item, use `initialScrollIndex` rather than scrolling after the first frame.

## ListView from nsh/ui/mobile

`ListView` from `nsh/ui/mobile` is a ready-made list built on the same machinery: you pass `each` and render a row, and it adds the padding, dividers and an `inset` grouped look. Use it for settings-style and plain lists that should look like the rest of the mobile UI set; use `List` when you control the whole look. See [ListView](../components/list-view.md).

```tsx
import { Text } from "nsh";
import { ListView } from "nsh/ui/mobile";

<ListView variant="inset" dividers each={settings} keyExtractor={(s) => s.id}>
  {(s) => <Text>{s().label}</Text>}
</ListView>
```

## Performance notes

- Each cell is laid out on its own, so scrolling only lays out the cells that changed. Keep rows shallow and avoid reading signals in rows that change on every frame.
- The native side asks for cells in batches, up to several rounds per frame, merged into one update; a fast fling binds many cells in one frame without a JS call per cell.
- Use [`nsh trace --scroll`](./testing-and-trace.md) to measure the per-frame cost of your rows.

## Related

- [`List` element](../elements/list.md) and [`nsh/list`](../modules/nsh-list.md): the reference.
- [Scrolling](./scrolling.md): the scrolling props lists share with `<Scroll>`.
- [Reactivity](./reactivity.md): why `item()` is a function.
- [Performance](./performance.md): measuring lists.

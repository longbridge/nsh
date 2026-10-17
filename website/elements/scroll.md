---
title: Scroll
description: "The native scroll view: vertical and horizontal scrolling, paging and snapping, pull to refresh, zooming and scroll events."
order: 5
demo: el-scroll
---

# Scroll

`<Scroll>` is a native scroll view (UIScrollView on iOS and Mac Catalyst, a ScrollView on Android). It lays out all its
children as its content and scrolls them natively: scrolling, flinging, bouncing and snapping never wait for JS.

```tsx
<Scroll style={{ flex: 1 }}>
  <View style={{ padding: 16, gap: 8 }}>
    <Each of={rows}>{(row) => <Text>{row().title}</Text>}</Each>
  </View>
</Scroll>
```

`<Scroll>` creates every child, which suits pages of moderate size. For long or unbounded data use a
[list](./list.md): it creates only the rows near the viewport and recycles them.

A scroll view must have a bounded size along its axis: give it `flex: 1` (or a `height`) inside a container that has
one. Its content is laid out at the scroll view's width (height when `horizontal`) and as tall (wide) as it needs.

## Props

Besides the [common props](./index.md#common-props) (with `style` typed as `ViewStyle`):

### Scrolling

| Prop | Type | Default | Description |
|---|---|---|---|
| `horizontal` | `boolean` | `false` | Scrolls sideways. |
| `scrollEnabled` | `boolean` | `true` | The user can scroll it. |
| `showsIndicator` | `boolean` | `true` | The scroll indicator shows. |
| `bounces` | `boolean` | `true` | Overscroll past the edges. |
| `pagingEnabled` | `boolean` | `false` | Stops at multiples of the viewport. |
| `snapToInterval` | `number` | `0` | Stops at multiples of this length (0: off). |
| `snapToOffsets` | `number[]` | | Stops at these offsets. |
| `snapToAlignment` | `"start" \| "center" \| "end"` | `"start"` | Which point of an item aligns with the viewport with `snapToInterval`. |
| `disableIntervalMomentum` | `boolean` | `false` | A fling moves one snap point at most. |
| `decelerationRate` | `"normal" \| "fast" \| number` | platform | How fast a fling slows down: the velocity kept per millisecond (`normal` 0.998, `fast` 0.99). |
| `contentInset` | `{ top?, right?, bottom?, left? }` | | Room around the content. |
| `contentOffset` | `{ x?, y? }` | | Where it starts. |
| `keyboardDismissMode` | `"none" \| "on-drag" \| "interactive"` | `"none"` | Whether dragging dismisses the keyboard. |
| `keyboardShouldPersistTaps` | `"never" \| "always" \| "handled" \| boolean` | `"never"` | Whether a tap dismisses the keyboard: `never` (the tap only dismisses it), `always` (it stays up), `handled` (it stays up when something handles the tap). |
| `scrollValue` | `NativeValue` | | A native value fed with the scroll offset (main axis), for scroll-linked animations. |
| `scrollEventThrottle` | `number` | | Minimum milliseconds between `onScroll` events (at most one per frame anyway). |

### Pull to refresh

| Prop | Type | Description |
|---|---|---|
| `refreshing` | `boolean` | The refresh spinner shows. |
| `onRefresh` | `() => void` | The user pulled to refresh: start loading and set `refreshing` until it ends. |

```tsx
const refreshing = signal(false);
async function reload() {
  refreshing.set(true);
  await load();
  refreshing.set(false);
}

<Scroll style={{ flex: 1 }} refreshing={refreshing()} onRefresh={reload}>
  …
</Scroll>;
```

### Zooming

Setting `maximumZoomScale` above `minimumZoomScale` makes the content zoomable with a pinch; zoomed in, it scrolls both
ways. Lists do not zoom.

| Prop | Type | Default | Description |
|---|---|---|---|
| `minimumZoomScale` | `number` | `1` | The smallest zoom. |
| `maximumZoomScale` | `number` | `1` | The largest zoom. |
| `zoomScale` | `number` | | Zooms to this scale when it changes; otherwise the user's zoom is kept. |
| `bouncesZoom` | `boolean` | `true` | Pinching may go past the limits and springs back. |
| `pinchGestureEnabled` | `boolean` | `true` | The user can pinch to zoom. |
| `centerContent` | `boolean` | `false` | Content smaller than the viewport (zoomed out) is centered. |

```tsx
let photo!: YNode;
<Scroll ref={photo} maximumZoomScale={4} centerContent style={{ flex: 1, backgroundColor: "#000" }}>
  <Image src={picture} style={{ width: 390, height: 520 }} onPress={() => photo.zoomTo({ scale: 1, animated: true })} />
</Scroll>;
```

## Events

| Event | Payload | When |
|---|---|---|
| `onScroll` | `ScrollEvent` | The offset changed (at most once per frame, and no more often than `scrollEventThrottle`). |
| `onScrollBeginDrag` / `onScrollEndDrag` | `{ x, y }` | The user started / stopped dragging. |
| `onMomentumScrollBegin` / `onMomentumScrollEnd` | `{ x, y }` | A glide after the drag started / stopped. A glide caught by a new drag still ends with `onMomentumScrollEnd`. |
| `onScrollEnd` | `{ x, y }` | Scrolling came to rest. |
| `onRefresh` | none | Pulled to refresh. |

`ScrollEvent` is `{ x, y, contentWidth, contentHeight, width, height, zoomScale }`: the offset, the (zoomed) content
size, the viewport size and the zoom scale (1 unless the view zooms).

> [!TIP]
> To move something with the scroll position (a shrinking header, a parallax image), bind `scrollValue` to a native
> value and interpolate it in a style. It runs on the UI thread at the display's rate, which `onScroll` (a JS event per
> frame) cannot guarantee. See [animation](../docs/animation.md).

## Ref methods

| Method | Description |
|---|---|
| `scrollTo({ x?, y?, animated? })` | Scrolls to an offset. |
| `zoomTo({ scale, x?, y?, animated? })` | Zooms to `scale` (clamped to the limits) with the content point (`x`, `y`, unzoomed points) in the middle of the viewport, or around the middle. E.g. a double tap zooming in on the tapped point. |

```tsx
let scroller!: YNode;
<Scroll ref={scroller}>…</Scroll>;
scroller.scrollTo({ y: 0, animated: true });
```

## Keyboard

Inputs inside a scroll view are kept clear of the keyboard natively: the scroll view gets a bottom inset and scrolls to
the focused input, with no JS involved. For content outside scroll views use `KeyboardAvoidingView` from
[`nsh/ui/mobile`](../components/keyboard-avoiding-view.md). See [forms and the keyboard](../docs/forms-and-keyboard.md).

## Right to left

Horizontal scroll views start at the right end in right-to-left layouts and keep their distance from it as the content
changes; their children are mirrored.

## Platform notes

- Android: pull to refresh is drawn by NSH (overscroll stays native), `contentInset` is applied as padding, and
  `decelerationRate` only shapes snapping glides.
- Android has no zooming scroll view: NSH switches to its own, which scales the content around its top left and scrolls
  both ways. Pinches start after Android's minimum span between the fingers (about 27 mm).
- Mac Catalyst (Mac idiom) has no system refresh control; NSH detects the pull and shows a spinner.
- iOS redraws texts and canvases at the zoomed resolution when a zoom ends.

---
title: Scrolling
description: The scroll element, paging and snapping, zooming, pull to refresh, scroll events, scroll-linked effects with scrollValue, the keyboard in scroll views, and scrolling from code.
order: 39
group: ui
demo: guide-scrolling
---

# Scrolling

`<Scroll>` is the platform's own scroll view (UIScrollView on iOS, a ScrollView on Android): flings, bounces, the scroll indicator and pull to refresh are native, and so is snapping. JS hears about scrolling through events and can bind the offset to a [native value](./animation.md#native-values) for effects that follow the finger frame by frame.

Use `<Scroll>` for content of a known, moderate size: a settings page, a form, an article, a row of chips. For long or unbounded data use [`List`](./lists.md), which recycles rows and accepts the same scrolling props.

## A scroll view

A `<Scroll>` is the viewport; its children are the content. The viewport needs a size from its parent (usually `flex: 1`), and the content is laid out at the viewport's width (vertical) or height (horizontal) and as long as it needs to be.

```tsx
import { mount, Scroll, Text, View } from "nsh";

function App() {
  return (
    <Scroll style={{ flex: 1, backgroundColor: "#f6f8fa" }}>
      <View style={{ padding: 16, gap: 12 }}>
        <Each of={Array.from({ length: 50 }, (_, i) => i + 1)}>
          {(n) => (
            <View style={{ padding: 16, borderRadius: 10, backgroundColor: "#ffffff" }}>
              <Text>Row {n()}</Text>
            </View>
          )}
        </Each>
      </View>
    </Scroll>
  );
}

mount(App);
```

The child view above can also be the scroll view's own content container: `contentContainerStyle` (or `contentContainerClass`) wraps the children in a view with that style, which scrolls with them. `flexGrow: 1` makes it at least as tall as the viewport, so a short content can be centered and a long one still scrolls:

```tsx
<Scroll style={{ flex: 1 }} contentContainerStyle={{ flexGrow: 1, justifyContent: "center", padding: 16 }}>
  <If when={messages().length > 0} else={<Text>No messages yet</Text>}>
    <Each of={messages()}>{(m) => <Text>{m().text}</Text>}</Each>
  </If>
</Scroll>
```

A horizontal scroll view takes `horizontal` and a row as its child:

```tsx
<Scroll horizontal showsIndicator={false}>
  <View style={{ flexDirection: "row", gap: 8, paddingHorizontal: 16 }}>
    <Each of={["Today", "This week", "This month", "This year"]}>
      {(label) => <Text style={{ paddingHorizontal: 14, paddingVertical: 8, borderRadius: 16, overflow: "hidden", backgroundColor: "#ffffff" }}>{label()}</Text>}
    </Each>
  </View>
</Scroll>
```

Right to left (see [Device and environment](./device-and-environment.md#right-to-left)), a horizontal scroll view starts at its right end and keeps its distance from it.

### Behavior props

| Prop | Type | Default | Description |
|---|---|---|---|
| `horizontal` | `boolean` | `false` | Scrolls sideways. |
| `scrollEnabled` | `boolean` | `true` | The user can scroll it. Scrolling from code still works. |
| `bounces` | `boolean` | `true` | Overscroll past the edges. |
| `showsIndicator` | `boolean` | `true` | The scroll indicator shows. |
| `contentInset` | `{ top?, right?, bottom?, left? }` | | Room around the content, scrollable into. |
| `contentOffset` | `{ x?, y? }` | | Where it starts. |
| `decelerationRate` | `"normal" \| "fast" \| number` | the platform's | How fast a fling slows down: `"normal"`, `"fast"`, or the velocity kept per millisecond (`0.998` is normal, `0.99` fast; `0` is the platform's default). |

## Paging and snapping

Snapping is decided natively when the finger lifts, from the release offset, the projected glide and the velocity; the scroll view then glides to the chosen stop. No JS is involved.

| Prop | Type | Description |
|---|---|---|
| `pagingEnabled` | `boolean` | Stops at multiples of the viewport's size. |
| `snapToInterval` | `number` | Stops at multiples of this length (0: off). |
| `snapToOffsets` | `number[]` | Stops at these offsets. |
| `snapToAlignment` | `"start" \| "center" \| "end"` | With `snapToInterval`: which point of an item aligns with the viewport. |
| `disableIntervalMomentum` | `boolean` | A fling moves one stop at most. |

A pager: pages as wide as the scroll view, and an indicator updated when the glide ends.

```tsx
import { mount, Scroll, signal, Text, View } from "nsh";

const colors = ["#0969da", "#8250df", "#bf3989", "#1a7f37"];

function Pager() {
  const width = signal(0);
  const page = signal(0);
  const settle = (e: { x: number }) => page.set(Math.round(e.x / width()));
  return (
    <View style={{ gap: 8, padding: 16 }}>
      <Scroll
        horizontal
        pagingEnabled
        showsIndicator={false}
        style={{ height: 160, borderRadius: 12 }}
        onLayout={(e) => width.set(e.width)}
        onScrollEnd={settle}
      >
        <View style={{ flexDirection: "row" }}>
          <Each of={colors}>
            {(color, i) => (
              <View style={{ width: width(), height: 160, backgroundColor: color(), alignItems: "center", justifyContent: "center" }}>
                <Text style={{ color: "#ffffff", fontSize: 24, fontWeight: "700" }}>Page {i() + 1}</Text>
              </View>
            )}
          </Each>
        </View>
      </Scroll>
      <View style={{ flexDirection: "row", justifyContent: "center", gap: 6 }}>
        <Each of={colors}>
          {(_, i) => <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: page() === i() ? "#0969da" : "#d0d7de" }} />}
        </Each>
      </View>
    </View>
  );
}

mount(Pager);
```

A carousel of cards that snap to the center, one card per fling:

```tsx
<Scroll
  horizontal
  snapToInterval={232}          // card width (220) + gap (12)
  snapToAlignment="center"
  decelerationRate="fast"
  disableIntervalMomentum
  showsIndicator={false}
>
  <View style={{ flexDirection: "row", gap: 12, paddingHorizontal: 16 }}>…</View>
</Scroll>
```

`List` takes the same snapping props, which is the better choice for a long carousel.

## Scroll events

| Prop | Payload | When |
|---|---|---|
| `onScroll` | `ScrollEvent` | The offset changed. At most once per frame; `scrollEventThrottle` spaces them further. |
| `scrollEventThrottle` | `number` | Minimum milliseconds between `onScroll` events. |
| `onScrollBeginDrag` | `{ x, y }` | The user started dragging. |
| `onScrollEndDrag` | `{ x, y }` | The user lifted the finger. |
| `onMomentumScrollBegin` | `{ x, y }` | The glide after a fling started. |
| `onMomentumScrollEnd` | `{ x, y }` | The glide ended (also when a new drag catches it). |
| `onScrollEnd` | `{ x, y }` | A user scroll came to rest: after the glide, or at the end of a drag without one. |

`ScrollEvent` carries `x`, `y` (the offset), `contentWidth`, `contentHeight` (the content's size, zoomed), `width`, `height` (the viewport) and `zoomScale`.

```tsx
const atBottom = signal(false);

<Scroll
  style={{ flex: 1 }}
  scrollEventThrottle={100}
  onScroll={(e) => atBottom.set(e.y + e.height >= e.contentHeight - 20)}
>
  …
</Scroll>
```

> [!TIP]
> `onScroll` runs JS on every frame of a scroll (subject to the throttle). For something that only has to *look* scroll-linked, such as a header that fades or a parallax image, use [`scrollValue`](#scroll-linked-effects-scrollvalue) instead: it costs no JS at all.

## Scroll-linked effects: scrollValue

`scrollValue` takes a [native value](./animation.md#native-values) and writes the scroll offset (along the main axis) into it natively, in the same frame the content moves. Bind that value, or nodes derived from it with `interpolate`, `diffClamp` or arithmetic, to styles.

```tsx
import { mount, Scroll, Text, View } from "nsh";
import { createNativeValue } from "nsh/animated";

function App() {
  const offset = createNativeValue(0);
  return (
    <View style={{ flex: 1 }}>
      <View
        style={{
          height: 56,
          justifyContent: "center",
          paddingHorizontal: 16,
          // Transparent at the top, white after 80 points.
          backgroundColor: offset.interpolate({ inputRange: [0, 80], outputRange: ["rgba(255,255,255,0)", "#ffffff"] }),
        }}
      >
        <Text
          style={{
            fontSize: 22,
            fontWeight: "700",
            transformOrigin: "left center",
            transform: [{ scale: offset.interpolate({ inputRange: [0, 80], outputRange: [1, 0.8] }) }],
          }}
        >
          Inbox
        </Text>
      </View>
      <Scroll style={{ flex: 1 }} scrollValue={offset}>
        <View style={{ padding: 16, gap: 8 }}>…</View>
      </Scroll>
    </View>
  );
}

mount(App);
```

A header that slides away while the content scrolls up and comes back as soon as it scrolls down is `diffClamp` of the offset:

```tsx
const offset = createNativeValue(0);
const header = offset.diffClamp(0, 56);

<View style={{ flex: 1 }}>
  <Scroll style={{ flex: 1 }} scrollValue={offset}>
    <View style={{ paddingTop: 56 }}>…</View>
  </Scroll>
  <View style={{ position: "absolute", top: 0, left: 0, right: 0, height: 56, transform: [{ translateY: header.mul(-1) }] }}>…</View>
</View>
```

Interpolations clamp outside their input range by default, so pulling past the top (a negative offset) does not overshoot the effect; pass `extrapolate: "extend"` for a stretchy header that grows as you pull.

## Pull to refresh

`refreshing` and `onRefresh` add the platform's pull-to-refresh control. The control shows while `refreshing` is true; set it in `onRefresh` and clear it when the work is done.

```tsx
const refreshing = signal(false);
const reload = async () => {
  refreshing.set(true);
  try {
    items.set(await (await fetch("https://api.example.com/items")).json());
  } finally {
    refreshing.set(false);
  }
};

<Scroll style={{ flex: 1 }} refreshing={refreshing()} onRefresh={reload}>…</Scroll>
```

On iOS the control is UIRefreshControl; in the Mac idiom of Mac Catalyst, which has no UIRefreshControl, NSH shows its own spinner with the same behavior. `List` takes the same two props.

## Zooming

A `<Scroll>` (not a `List`) zooms when `maximumZoomScale` is above `minimumZoomScale`: pinching scales the content, and while zoomed in it scrolls both ways.

| Prop | Type | Default | Description |
|---|---|---|---|
| `minimumZoomScale` | `number` | `1` | The smallest zoom. |
| `maximumZoomScale` | `number` | `1` | The largest zoom; above the minimum, pinching zooms. |
| `zoomScale` | `number` | | Zooms to this scale when the prop changes; the user's zoom is kept otherwise. |
| `bouncesZoom` | `boolean` | `true` | Pinching may go past the limits and springs back. |
| `pinchGestureEnabled` | `boolean` | `true` | The user can pinch to zoom. |
| `centerContent` | `boolean` | `false` | Content smaller than the viewport (zoomed out) is centered. |

`onScroll` reports `zoomScale` and the zoomed content size. From code, `ref.zoomTo({ scale, x, y, animated })` zooms so that the content point (`x`, `y`, in unzoomed points) lands in the middle of the viewport; double tap to zoom is a press handler calling it:

```tsx
import { mount, Scroll, signal, type YNode } from "nsh";
import { Image } from "nsh/image";

function Photo() {
  let scroll!: YNode;
  const scale = signal(1);
  return (
    <Scroll
      ref={scroll}
      style={{ flex: 1, backgroundColor: "#000000" }}
      minimumZoomScale={1}
      maximumZoomScale={4}
      centerContent
      onScroll={(e) => scale.set(e.zoomScale)}
    >
      <Image
        src="https://picsum.photos/id/1015/1200/800"
        style={{ width: 390, height: 260 }}
        onPress={() => scroll.zoomTo({ scale: scale() > 1.5 ? 1 : 3 })}
      />
    </Scroll>
  );
}

mount(Photo);
```

Text and canvases inside a zoomed scroll view are drawn again at the new resolution when the zoom ends, so they stay sharp on iOS. Android has no zooming scroll view of its own: NSH switches the scroll view to its own implementation, which scales around the pinch's focal point, pans with any number of fingers, flings and rubber-bands at the edges and zoom limits. Android only starts a pinch once the fingers are about 27 mm apart, as the system's gesture detector does.

## Scrolling from code

Give the scroll view a [`ref`](./jsx-and-components.md) and call its methods. Commands run after the rest of the changes in the same update, so scrolling to content you just added works.

| Method | Description |
|---|---|
| `scrollTo({ x?, y?, animated? })` | Scrolls to an offset. Animated by default. |
| `zoomTo({ scale, x?, y?, animated? })` | Zoomable scroll views: zooms to `scale` (clamped to the limits), around the content point (`x`, `y`) or the middle. |

```tsx
let scroll!: YNode;

<Scroll ref={scroll} style={{ flex: 1 }}>…</Scroll>
<View onPress={() => scroll.scrollTo({ y: 0 })}>…</View>
```

Lists add `scrollToIndex`, `scrollToOffset` and `scrollToEnd`; see [Lists](./lists.md#scrolling-from-code).

## Scroll views and the keyboard

NSH keeps a focused input visible on its own: when the keyboard shows, the scroll view around the input gets a bottom inset as tall as the keyboard and scrolls the input into view. This happens natively, with no code in your app. Use [`KeyboardAvoidingView`](./forms-and-keyboard.md#content-outside-scroll-views-keyboardavoidingview) only for content outside scroll views, such as a message bar pinned to the bottom.

Two props decide how scrolling and taps interact with the keyboard:

| Prop | Type | Default | Description |
|---|---|---|---|
| `keyboardDismissMode` | `"none" \| "on-drag" \| "interactive"` | `"none"` | Whether dragging the content hides the keyboard. `"interactive"` lets the keyboard follow the finger down on iOS; on Android it behaves like `"on-drag"`. |
| `keyboardShouldPersistTaps` | `"never" \| "always" \| "handled"` | `"never"` | What a tap does while the keyboard is up. |

`keyboardShouldPersistTaps`:

- `"never"`: a tap outside the focused input only hides the keyboard; buttons do not get it. The user taps once to dismiss, again to press.
- `"handled"`: a tap on something that handles it (a button) goes through and the keyboard stays; a tap on empty space hides it. Usually the right choice for forms.
- `"always"`: taps never hide the keyboard.

Both are decided natively when the tap or drag happens; nothing waits for JS. To hide the keyboard from code, call `Keyboard.dismiss()` from `nsh/ui/mobile`.

## Nested scroll views

A horizontal scroll view inside a vertical one works as expected: each takes the drags along its own axis. Two scroll views on the same axis nest too (the inner one scrolls first). JS responders that need the finger, such as a drawing canvas inside a scroll view, return `true` from `onResponderGrant` to keep the scroll views around them from taking the touches; see [Touch and gestures](./touch-and-gestures.md#the-responder-system).

## Related

- [`Scroll`](../elements/scroll.md): the element reference, every prop and event.
- [Lists](./lists.md): virtualized lists with the same scrolling props.
- [Animation](./animation.md): native values, `interpolate` and `diffClamp` for scroll-linked effects.
- [Forms and the keyboard](./forms-and-keyboard.md): inputs, focus and the keyboard.

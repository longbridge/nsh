---
title: Layout
description: Flexbox with a column default, CSS grid, sizes and percentages, aspect ratio, absolute positioning, measuring with onLayout, responsive layouts and right-to-left.
order: 34
group: ui
demo: guide-layout
---

# Layout

NSH lays out every element with [Taffy](https://github.com/DioxusLabs/taffy), a Rust implementation of CSS flexbox, grid and block layout, configured so that children stack vertically and do not shrink by default. Layout runs in the core, after each JavaScript turn, and only the frames that changed are sent to the native views. Text is measured by the platform's text engine on the same thread, so layout never waits for the main thread.

The layout properties below are style properties. Each has a [Tailwind](./tailwind.md) equivalent (`flex-row`, `gap-4`, `grid-cols-3`, `absolute inset-0`, ...).

## Flexbox

A view lays out its children with flexbox. The defaults differ from the web's:

| Property | NSH default | Web default |
|---|---|---|
| `flexDirection` | `"column"` | `row` |
| `flexShrink` | `0` | `1` |
| `alignContent` | `"flex-start"` | `stretch` |
| `alignItems` | `"stretch"` | `stretch` |
| `minWidth` / `minHeight` | `0` | `auto` |
| `position` | `"relative"` | `static` |

```tsx
<View style={{ flexDirection: "row", alignItems: "center", gap: 12, padding: 16 }}>
  <Image src={avatar} style={{ width: 40, height: 40, borderRadius: 20 }} />
  <View style={{ flex: 1, gap: 2 }}>
    <Text style={{ fontWeight: "600" }}>{props.name}</Text>
    <Text style={{ color: "#6e7781" }} numberOfLines={1}>{props.message}</Text>
  </View>
  <Text style={{ color: "#6e7781" }}>{props.time}</Text>
</View>
```

| Property | Values |
|---|---|
| `flexDirection` | `"column"`, `"row"`, `"column-reverse"`, `"row-reverse"` |
| `justifyContent` | Main axis: `"flex-start"`, `"flex-end"`, `"center"`, `"space-between"`, `"space-around"`, `"space-evenly"`, `"stretch"` |
| `alignItems` | Cross axis: `"stretch"`, `"flex-start"`, `"flex-end"`, `"center"`, `"baseline"` |
| `alignSelf` | One child's cross-axis alignment; `"auto"` follows the parent's `alignItems` |
| `alignContent` | Lines of a wrapping container |
| `flexWrap` | `"nowrap"`, `"wrap"`, `"wrap-reverse"` |
| `flex` | A single-number shorthand (below) |
| `flexGrow`, `flexShrink`, `flexBasis` | As in CSS |
| `gap`, `rowGap`, `columnGap` | Space between children |

`flex` takes one number and is not the CSS shorthand: a positive number `n` means grow by `n`, shrink by 1 and start from a basis of 0, so `flex: 1` siblings share the space equally whatever their content; `flex: 0` (the default) sizes the view to its content; a negative number shrinks it by that much down to its minimum.

```tsx
<View style={{ flexDirection: "row", gap: 8 }}>
  <View style={{ flex: 1, height: 36, backgroundColor: "#c8e1ff" }} />   {/* one share */}
  <View style={{ flex: 2, height: 36, backgroundColor: "#79b8ff" }} />   {/* two shares */}
  <View style={{ width: 36, height: 36, backgroundColor: "#0366d6" }} /> {/* fixed */}
</View>
```

Because minimum sizes default to 0, a `flex: 1` text in a row shrinks and wraps instead of pushing its siblings out.

> [!NOTE]
> `display: "flex"` in a style keeps the column default. Only Tailwind's `flex` class means a row, as in CSS ([Tailwind](./tailwind.md#what-differs-from-tailwind-on-the-web)).

## Sizes

| Property | Values |
|---|---|
| `width`, `height` | Points, a percentage of the parent (`"50%"`), or `"auto"` |
| `minWidth`, `maxWidth`, `minHeight`, `maxHeight` | Points or percentages |
| `aspectRatio` | Width divided by height: `1`, `16 / 9` |
| `padding`, `paddingHorizontal`, `paddingVertical`, `paddingTop`, ... | Points or percentages |
| `margin`, `marginHorizontal`, `marginVertical`, `marginTop`, ... | Points, percentages, or `"auto"` |

Without a size, a view is as big as its content (plus padding), stretched across its parent's cross axis by the default `alignItems: "stretch"`. Percentages are relative to the parent's size (for padding and margin, as in CSS, to its width).

`aspectRatio` sizes the other dimension from the one that is known:

```tsx
<View style={{ width: "100%", aspectRatio: 16 / 9, backgroundColor: "#000000" }} />
```

Images imported from files have a natural size and lay out at it when no size is given ([Images and assets](./images-and-assets.md)). Remote images have no size until they load: give them one.

## Grid

`display: "grid"` lays out children in CSS grid tracks:

```tsx
<View style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 8 }}>
  <View style={{ gridColumn: "span 2", height: 60, backgroundColor: "#d8b9ff" }} />
  <View style={{ height: 60, backgroundColor: "#b083f0" }} />
  <View style={{ gridRow: "span 2", backgroundColor: "#8250df" }} />
  <View style={{ gridColumn: "2 / 4", height: 60, backgroundColor: "#6639ba" }} />
</View>
```

| Property | Values |
|---|---|
| `gridTemplateColumns`, `gridTemplateRows` | CSS track lists: `"repeat(3, 1fr)"`, `"200px 1fr"`, `minmax()`, `auto-fill` / `auto-fit`, line names |
| `gridAutoColumns`, `gridAutoRows` | Tracks of implicitly created columns and rows |
| `gridAutoFlow` | `"row"`, `"column"`, `"dense"`, `"row dense"`, `"column dense"` |
| `gridColumn`, `gridRow` | `"1 / 3"`, `"span 2"`, a line number or name |
| `gridColumnStart`, `gridColumnEnd`, `gridRowStart`, `gridRowEnd` | Line numbers or names |
| `justifyItems`, `justifySelf` | How items sit in their areas along the row |

`display: "block"` uses CSS block layout (children stacked, each as wide as the container), and `display: "none"` removes the view from layout.

## Absolute positioning

`position: "absolute"` takes a view out of the flow and places it by `top`, `right`, `bottom` and `left`, relative to its parent:

```tsx
<View style={{ height: 200 }}>
  <Image src={cover} style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0 }} />
  <View style={{ position: "absolute", left: 12, right: 12, bottom: 12 }}>
    <Text style={{ color: "#ffffff", fontWeight: "700" }}>Caption</Text>
  </View>
</View>
```

Offsets accept points and percentages. A view with `left` and `right` (or `top` and `bottom`) and no size stretches between them. Later siblings are drawn above earlier ones; `zIndex` changes the order.

## Measuring

`onLayout` reports a view's frame, relative to its parent, after each layout in which it changed:

```tsx
const width = signal(0);

<View onLayout={(e) => width.set(e.width)}>
  <Text>{width()} points wide</Text>
</View>
```

The event is `{ x, y, width, height }` in points. For the whole window, read `windowSize()` from [`nsh/device`](../modules/nsh-device.md), an accessor that follows rotation and window resizing:

```tsx
import { View } from "nsh";
import { windowSize } from "nsh/device";

<View style={{ flexDirection: windowSize().width > 600 ? "row" : "column" }} />
```

Tailwind's breakpoints (`sm:`, `md:`, ...) and `portrait:` / `landscape:` give the same result without code.

## Safe areas

The root view covers the whole screen, under the status bar and the home indicator. Wrap screens in [`SafeAreaView`](../components/safe-area-view.md) from `nsh/ui/mobile`, which pads its content by the safe area insets, or read the insets with `safeArea()` / `useSafeAreaInsets()` and apply them yourself. Screens of a [navigation stack](./navigation.md) already sit below their header.

## Right to left

`I18nManager` from `nsh/device` controls the layout direction of the whole app. When the app lays out right to left (the system language is right-to-left, or `I18nManager.forceRTL(true)`):

- rows run from the right;
- `left` and `right` properties (margins, paddings, positions, borders, corners) swap sides;
- `start` and `end` properties always mean the leading and trailing side: `marginStart`, `marginEnd`, `paddingStart`, `paddingEnd`, `start`, `end`, `borderStartWidth`, `borderTopStartRadius`, ...;
- text with `textAlign: "auto"` aligns right, and native controls mirror.

```tsx
import { Text, View } from "nsh";
import { I18nManager } from "nsh/device";

<View style={{ flexDirection: "row", gap: 8 }}>
  <View style={{ marginStart: 16, borderTopStartRadius: 12, width: 44, height: 44, backgroundColor: "#54aeff" }} />
</View>

<View onPress={() => I18nManager.forceRTL(!I18nManager.isRTL)}>
  <Text>{I18nManager.isRTL ? "Right to left" : "Left to right"}</Text>
</View>
```

`I18nManager.isRTL` is reactive and changes apply at once, without a restart. The `direction` style (`"inherit"`, `"ltr"`, `"rtl"`) overrides the direction for one view and its children. On Android, apps that embed NSH need `android:supportsRtl="true"` in their manifest for native controls to mirror.

## Related

- [Styling](./styling.md): how styles combine.
- [Scrolling](./scrolling.md) and [Lists](./lists.md): content larger than the screen.
- [Style reference](../modules/style-reference.md): every layout property and its type.

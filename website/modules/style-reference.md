---
title: Style reference
description: Every style key NSH accepts, grouped by LayoutStyle, ViewStyle, TextStyle and ImageStyle, with its type and meaning.
order: 16
---

# Style reference

This page lists every key of the `style` prop. The keys and their types come from the generated declarations in `.nsh/types/nsh/types.d.ts`, so your editor checks them as you type. Layout is flexbox with a column default, numbers are points, and the CSS features are there too: grid, gradients, box shadows, filters, blend modes, outlines, transitions and keyframe animations.

For how to apply styles, combine them with classes and inherit text styles, see [Styling](../docs/styling.md). For the class syntax, see the [Tailwind reference](./tailwind-reference.md).

## The four interfaces

Each interface extends the one before it:

| Interface | Extends | Used by |
|---|---|---|
| `LayoutStyle` | | Layout of every element (flexbox, grid, sizes, spacing, position) |
| `ViewStyle` | `LayoutStyle` | `<View>`, `<Scroll>`, `<List>`, controls and every other element |
| `TextStyle` | `ViewStyle` | `<Text>` and `<Input>` (and views, whose text styles pass to the text inside them) |
| `ImageStyle` | `ViewStyle` | `<Image>` |

```tsx
import type { ViewStyle, TextStyle } from "nsh";
import { Text, View } from "nsh";

const card: ViewStyle = {
  padding: 16,
  borderRadius: 12,
  backgroundColor: "#fff",
  boxShadow: "0 4px 12px rgb(0 0 0 / 0.08)",
};

const title: TextStyle = { fontSize: 17, fontWeight: "600", color: "#111" };

<View style={card}>
  <Text style={title}>Hello</Text>
</View>;
```

`style` also takes an array; entries merge left to right and falsy entries are skipped, so conditional styles need no spreading:

```tsx
<View style={[card, selected() && { borderColor: "#2563eb", borderWidth: 2 }]} />
```

> [!NOTE]
> The inline `style` always wins over `class`. Setting a key to `null` or `undefined` resets it to its default.

## Value types

| Type | Accepts |
|---|---|
| `number` | Points (density-independent). `1` is one point on iOS and one dp on Android. |
| `DimensionValue` | `number` (points), `` `${number}%` `` (of the parent), or `"auto"`. |
| `LengthValue` | `number` (points) or `` `${number}%` ``. No `"auto"`. |
| `ColorValue` | A CSS color string: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, `hsl()`, `hsla()`, `oklch()`, `oklab()`, or a named CSS color (`black`, `white`, `transparent`, `rebeccapurple`, ...). |
| `NativeNode` | A native animated value or a node derived from it (see [nsh/animated](./nsh-animated.md)). Accepted by `backgroundColor`, `opacity` and the steps of `transform`; the value then animates on the UI thread without JS. |

Strings with CSS syntax (`boxShadow`, `backgroundImage`, `filter`, `gridTemplateColumns`, ...) take lengths in `px` and bare numbers (both points), `pt`, or `rem` (16 points).

## LayoutStyle

Layout is computed in Rust (flexbox and CSS grid). The defaults differ from CSS's: `display: "flex"`, `flexDirection: "column"`, `flexShrink: 0`, `alignContent: "flex-start"`, `alignItems: "stretch"`, and no automatic minimum size. See [Layout](../docs/layout.md).

### Display and position

| Key | Type | Description |
|---|---|---|
| `display` | `"flex" \| "grid" \| "block" \| "none"` | The layout of the children. `"none"` removes the element from layout and hides it. `"block"` stacks children as CSS block layout does. Default `"flex"`. |
| `position` | `"relative" \| "absolute"` | `"absolute"` takes the element out of the flow and places it by `top` / `right` / `bottom` / `left` relative to its parent. Default `"relative"`. |
| `top` | `DimensionValue` | Offset from the parent's top edge (absolute) or from the element's own position (relative). |
| `right` | `DimensionValue` | Offset from the right edge. In right-to-left layouts it means the end edge. |
| `bottom` | `DimensionValue` | Offset from the bottom edge. |
| `left` | `DimensionValue` | Offset from the left edge. In right-to-left layouts it means the start edge. |
| `start` | `DimensionValue` | Left in left-to-right layouts, right in right-to-left ones. |
| `end` | `DimensionValue` | Right in left-to-right layouts, left in right-to-left ones. |
| `zIndex` | `number` | Order among siblings (stable for equal values); also the native stacking order. |
| `direction` | `"inherit" \| "ltr" \| "rtl"` | Layout direction, inherited by default. Right to left, `left` and `right` properties mean start and end. |

### Flexbox

| Key | Type | Description |
|---|---|---|
| `flexDirection` | `"row" \| "column" \| "row-reverse" \| "column-reverse"` | The main axis. Default `"column"`. |
| `flexWrap` | `"wrap" \| "nowrap" \| "wrap-reverse"` | Whether children wrap onto several lines. Default `"nowrap"`. |
| `flex` | `number` | One number, not the CSS shorthand: a positive number sets `flexGrow` to it, `flexShrink: 1` and `flexBasis: 0`; `0` makes the element inflexible; a negative number shrinks it down to its minimum size. |
| `flexGrow` | `number` | How much of the free space the element takes. Default `0`. |
| `flexShrink` | `number` | How much the element shrinks when space runs out. Default `0`, not `1` as in CSS. |
| `flexBasis` | `DimensionValue` | The main-axis size before free space is distributed. |
| `justifyContent` | `"flex-start" \| "flex-end" \| "center" \| "space-between" \| "space-around" \| "space-evenly" \| "stretch"` | Distribution of children along the main axis. Default `"flex-start"`. |
| `alignItems` | `"flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | Alignment of children on the cross axis. Default `"stretch"`. |
| `alignSelf` | `"auto" \| "flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | Overrides the parent's `alignItems` for this element. |
| `alignContent` | `"flex-start" \| "flex-end" \| "center" \| "stretch" \| "space-between" \| "space-around" \| "space-evenly"` | Distribution of wrapped lines (and grid tracks) on the cross axis. Default `"flex-start"`. |
| `gap` | `LengthValue` | Space between rows and columns of children. |
| `rowGap` | `LengthValue` | Space between rows. |
| `columnGap` | `LengthValue` | Space between columns. |

### Grid

Set `display: "grid"` to lay out children in a CSS grid. Track lists use CSS syntax.

| Key | Type | Description |
|---|---|---|
| `gridTemplateColumns` | `string` | Column tracks: `"repeat(3, 1fr)"`, `"200px 1fr"`, `minmax()`, `auto-fill` / `auto-fit`, line names. |
| `gridTemplateRows` | `string` | Row tracks, same syntax. |
| `gridAutoColumns` | `string` | Size of implicitly created columns. |
| `gridAutoRows` | `string` | Size of implicitly created rows. |
| `gridAutoFlow` | `"row" \| "column" \| "dense" \| "row dense" \| "column dense"` | How items without a placement fill the grid. |
| `gridColumn` | `number \| string` | Column placement: `"1 / 3"`, `"span 2"`, a line number or a line name. |
| `gridColumnStart` | `number \| string` | Start column line. |
| `gridColumnEnd` | `number \| string` | End column line. |
| `gridRow` | `number \| string` | Row placement, same syntax as `gridColumn`. |
| `gridRowStart` | `number \| string` | Start row line. |
| `gridRowEnd` | `number \| string` | End row line. |
| `justifyItems` | `"flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | How grid items sit in their areas along the row. |
| `justifySelf` | `"auto" \| "flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | Overrides `justifyItems` for this item. |

```tsx
<View style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 8 }}>
  <View style={{ gridColumn: "span 2", height: 60, backgroundColor: "#c7d2fe" }} />
  <View style={{ height: 60, backgroundColor: "#a5b4fc" }} />
</View>
```

### Size

| Key | Type | Description |
|---|---|---|
| `width` | `DimensionValue` | Width. |
| `height` | `DimensionValue` | Height. |
| `minWidth` | `LengthValue` | Minimum width. |
| `minHeight` | `LengthValue` | Minimum height. |
| `maxWidth` | `LengthValue` | Maximum width. |
| `maxHeight` | `LengthValue` | Maximum height. |
| `aspectRatio` | `number` | Width divided by height; the missing dimension is computed from the other one. |

### Margin

Margins accept `"auto"` (absorbs free space, as in flexbox) and negative values.

| Key | Type | Description |
|---|---|---|
| `margin` | `DimensionValue` | All four sides. |
| `marginHorizontal` | `DimensionValue` | Left and right. |
| `marginVertical` | `DimensionValue` | Top and bottom. |
| `marginTop` | `DimensionValue` | Top. |
| `marginRight` | `DimensionValue` | Right. |
| `marginBottom` | `DimensionValue` | Bottom. |
| `marginLeft` | `DimensionValue` | Left. |
| `marginStart` | `DimensionValue` | Left in left-to-right layouts, right in right-to-left ones. |
| `marginEnd` | `DimensionValue` | The opposite of `marginStart`. |

### Padding

| Key | Type | Description |
|---|---|---|
| `padding` | `LengthValue` | All four sides. |
| `paddingHorizontal` | `LengthValue` | Left and right. |
| `paddingVertical` | `LengthValue` | Top and bottom. |
| `paddingTop` | `LengthValue` | Top. |
| `paddingRight` | `LengthValue` | Right. |
| `paddingBottom` | `LengthValue` | Bottom. |
| `paddingLeft` | `LengthValue` | Left. |
| `paddingStart` | `LengthValue` | Left in left-to-right layouts, right in right-to-left ones. |
| `paddingEnd` | `LengthValue` | The opposite of `paddingStart`. |

## ViewStyle

Everything drawn by a view. All keys of `LayoutStyle` apply too.

### Background

| Key | Type | Description |
|---|---|---|
| `backgroundColor` | `ColorValue \| NativeNode` | The fill color. Takes a native animated color. |
| `backgroundImage` | `string` | CSS gradients drawn over the background color: `linear-gradient(...)`, `radial-gradient(...)`, `conic-gradient(...)`, several separated by commas. |
| `experimental_backgroundImage` | `string` | Alias of `backgroundImage`. |

```tsx
<View style={{ height: 120, borderRadius: 16, backgroundImage: "linear-gradient(135deg, #6366f1, #ec4899)" }} />
```

### Borders

| Key | Type | Description |
|---|---|---|
| `borderWidth` | `number` | Width of all four sides. |
| `borderTopWidth` | `number` | Top side. |
| `borderRightWidth` | `number` | Right side. |
| `borderBottomWidth` | `number` | Bottom side. |
| `borderLeftWidth` | `number` | Left side. |
| `borderStartWidth` | `number` | Start side (left in left-to-right layouts). |
| `borderEndWidth` | `number` | End side. |
| `borderColor` | `ColorValue` | Color of all four sides. |
| `borderTopColor` | `ColorValue` | Top side. |
| `borderRightColor` | `ColorValue` | Right side. |
| `borderBottomColor` | `ColorValue` | Bottom side. |
| `borderLeftColor` | `ColorValue` | Left side. |
| `borderStartColor` | `ColorValue` | Start side. |
| `borderEndColor` | `ColorValue` | End side. |
| `borderStyle` | `"solid" \| "dashed" \| "dotted"` | Line style. Default `"solid"`. |

### Corners

| Key | Type | Description |
|---|---|---|
| `borderRadius` | `number` | Radius of all four corners. |
| `borderTopLeftRadius` | `number` | Top-left corner. |
| `borderTopRightRadius` | `number` | Top-right corner. |
| `borderBottomRightRadius` | `number` | Bottom-right corner. |
| `borderBottomLeftRadius` | `number` | Bottom-left corner. |
| `borderTopStartRadius` | `number` | Top corner on the start side. |
| `borderTopEndRadius` | `number` | Top corner on the end side. |
| `borderBottomStartRadius` | `number` | Bottom corner on the start side. |
| `borderBottomEndRadius` | `number` | Bottom corner on the end side. |
| `borderCurve` | `"circular" \| "continuous"` | `"continuous"`: corners whose curvature eases into the edges, like iOS app icons. Where the box is too small the curve falls back to circular, so pills stay pills. Works on both platforms. |

### Outline

An outline is drawn outside the border and takes no space in layout.

| Key | Type | Description |
|---|---|---|
| `outlineWidth` | `number` | Line width. |
| `outlineColor` | `ColorValue` | Line color. |
| `outlineStyle` | `"solid" \| "double" \| "dashed" \| "dotted" \| "none" \| "hidden"` | Line style. |
| `outlineOffset` | `number` | Distance from the border edge; negative values move it inside. |

### Shadows

Two ways to cast a shadow: the CSS `boxShadow` (several layers, spread, inset) or the four `shadow*` keys.

| Key | Type | Description |
|---|---|---|
| `boxShadow` | `string` | CSS `box-shadow`: offsets, blur, spread and color, several layers separated by commas, `inset` for inner shadows. |
| `shadowColor` | `ColorValue` | Shadow color. |
| `shadowOffset` | `{ width: number; height: number }` | Offset. Default `{ width: 0, height: -3 }`. |
| `shadowOpacity` | `number` | Opacity, 0 to 1. Default `0` (no shadow). |
| `shadowRadius` | `number` | Blur radius. Default `3`. |

```tsx
<View style={{ padding: 16, borderRadius: 12, backgroundColor: "#fff",
  boxShadow: "0 1px 2px rgb(0 0 0 / 0.06), 0 8px 24px -4px rgb(0 0 0 / 0.12)" }} />
```

> [!NOTE]
> On Android, shadows need a background: a view with a transparent background (text on nothing, for example) casts no shadow. For Android's material shadow use the `android={{ elevation }}` prop instead of a style.

### Opacity, visibility and overflow

| Key | Type | Description |
|---|---|---|
| `opacity` | `number \| NativeNode` | 0 (invisible) to 1. Takes a native animated value. |
| `visibility` | `"visible" \| "hidden" \| "collapse"` | Hidden views keep their space in layout (unlike `display: "none"`). |
| `overflow` | `"visible" \| "hidden"` | `"hidden"` clips children to the view's bounds (and corners). |
| `backfaceVisibility` | `"visible" \| "hidden"` | `"hidden"`: invisible while a 3D rotation turns it away. |

### Transforms

| Key | Type | Description |
|---|---|---|
| `transform` | `TransformStep[]` | A list of steps, applied right to left like CSS. |
| `transformOrigin` | `string \| (number \| string)[]` | The point `transform` acts around: keywords, percentages and lengths (`"top left"`, `"50% 0"`, `[10, 20]`). Default the center. |

A `TransformStep` is an object with one key:

| Step | Value | Native animated |
|---|---|---|
| `translateX`, `translateY` | `number` (points) | yes |
| `scale`, `scaleX`, `scaleY` | `number` (factor) | yes |
| `rotate`, `rotateZ`, `rotateX`, `rotateY` | `number` (degrees) or `string` (`"45deg"`, `"0.5rad"`) | yes |
| `skewX`, `skewY` | `number` (degrees) or `string` | no |
| `perspective` | `number` | no |

```tsx
<View style={{ transform: [{ perspective: 600 }, { rotateY: "20deg" }, { scale: 0.95 }] }} />
```

> [!NOTE]
> Android has no skew: `skewX` and `skewY` are ignored there.

### Filters and blending

| Key | Type | Description |
|---|---|---|
| `filter` | `string` | CSS filters of the view and its content: `brightness()`, `contrast()`, `grayscale()`, `hue-rotate()`, `invert()`, `saturate()`, `sepia()`, `opacity()`, `blur()`, `drop-shadow()`. |
| `backdropFilter` | `string` | CSS filters of what is behind the view, such as `"blur(10px)"` for frosted glass. |
| `mixBlendMode` | `"normal" \| "multiply" \| "screen" \| "overlay" \| "darken" \| "lighten" \| "color-dodge" \| "color-burn" \| "hard-light" \| "soft-light" \| "difference" \| "exclusion" \| "hue" \| "saturation" \| "color" \| "luminosity" \| "plus-darker" \| "plus-lighter"` | How the view blends with what is behind it. |

> [!WARNING]
> Platform limits:
> - iOS applies only `opacity()`, `brightness()` up to 1 and `drop-shadow()` to views. `<Image>` takes every color filter and `blur()`, applied while the image is decoded.
> - iOS `backdropFilter` blur uses a system material at a fraction, an approximation of the exact radius.
> - Android needs API 31 for `blur()`, `drop-shadow()` and `backdropFilter`; below that only color filters apply. Blend modes need API 29.

### Transitions

When a transitioned property changes (from a new style, a class or a variant such as `active:`), the platform animates it natively. No JS runs during the animation.

| Key | Type | Description |
|---|---|---|
| `transitionProperty` | `string` | Comma-separated properties that animate: `"all"`, `"colors"` (`color`, `backgroundColor`, `borderColor`, ...), `"opacity"`, `"transform"`, `"boxShadow"`, `"borderRadius"`, `"borderWidth"`, or `"none"`. |
| `transitionDuration` | `number \| "${number}ms" \| "${number}s"` | Milliseconds as a number, or `"150ms"` / `"0.2s"`. |
| `transitionDelay` | `number \| "${number}ms" \| "${number}s"` | Delay before the transition starts. |
| `transitionTimingFunction` | `"linear" \| "ease" \| "ease-in" \| "ease-out" \| "ease-in-out" \| "cubic-bezier(...)"` | The easing curve. |

```tsx
<View
  onPress={() => open.set(!open())}
  style={{ height: open() ? 160 : 56, opacity: open() ? 1 : 0.8,
    transitionProperty: "opacity, transform", transitionDuration: 200 }}
/>
```

### Keyframe animations

Built-in keyframe animations, run natively (Tailwind's `animate-*`).

| Key | Type | Description |
|---|---|---|
| `animationName` | `"none" \| "spin" \| "ping" \| "pulse" \| "bounce"` | The animation. |
| `animationDuration` | `number \| "${number}ms" \| "${number}s"` | Length of one iteration. |
| `animationDelay` | `number \| "${number}ms" \| "${number}s"` | Delay before the first iteration. |
| `animationTimingFunction` | `"linear" \| "ease" \| "ease-in" \| "ease-out" \| "ease-in-out" \| "cubic-bezier(...)"` | The easing curve. |
| `animationIterationCount` | `number \| "infinite"` | How many times it runs. |

For animations driven by values, gestures or springs, use [nsh/animated](./nsh-animated.md) instead.

### Pointer

| Key | Type | Description |
|---|---|---|
| `cursor` | `"auto" \| "default" \| "pointer" \| "text" \| "grab" \| "grabbing" \| "not-allowed" \| ...` (every CSS cursor keyword, and `"none"`) | The pointer shape over the view, for pointers (iPad trackpads, Mac Catalyst, desktops). |

## TextStyle

Text styles apply to `<Text>` and `<Input>`. They are also inherited, as in CSS: text properties set on a view pass to the texts and inputs inside it, and nested `<Text>` spans inherit from their parent text. All keys of `ViewStyle` apply too. See [Text](../docs/text.md).

### Font

| Key | Type | Description |
|---|---|---|
| `color` | `ColorValue` | Text color. |
| `fontSize` | `number` | Size in points. |
| `fontWeight` | `"normal" \| "bold" \| number \| "${number}"` | `"normal"` (400), `"bold"` (700) or 100–900. |
| `fontFamily` | `string` | A system family, or the name an imported font evaluates to (`import Inter from "./Inter.ttf"`). See [Fonts](../docs/fonts.md). |
| `fontStyle` | `"normal" \| "italic"` | Italic. |
| `fontVariant` | `("small-caps" \| "oldstyle-nums" \| "lining-nums" \| "tabular-nums" \| "proportional-nums")[]` | OpenType number and letter forms. `tabular-nums` keeps digits the same width, useful for counters and timers. |

### Spacing and alignment

| Key | Type | Description |
|---|---|---|
| `lineHeight` | `number` | Line height in points. |
| `letterSpacing` | `number` | Extra space between characters, in points. |
| `textAlign` | `"auto" \| "left" \| "center" \| "right" \| "justify"` | Horizontal alignment. |
| `textAlignVertical` | `"auto" \| "top" \| "center" \| "bottom"` | Where text shorter than its box sits (a `<Text>` with a height). |
| `writingDirection` | `"auto" \| "ltr" \| "rtl"` | Base direction of the text; `"auto"` takes it from the content. |
| `textTransform` | `"none" \| "uppercase" \| "lowercase" \| "capitalize"` | Case transformation. |

> [!TIP]
> Tailwind's `leading-*` and `tracking-*` classes set line heights as multiples of the font size and letter spacing in `em`, so they scale with `text-*` sizes.

### Decoration

| Key | Type | Description |
|---|---|---|
| `textDecorationLine` | `"none" \| "underline" \| "line-through" \| "underline line-through"` | Lines drawn with the text. |
| `textDecorationColor` | `ColorValue` | Color of the lines (default the text color). |
| `textDecorationStyle` | `"solid" \| "double" \| "dotted" \| "dashed" \| "wavy"` | Line style. |
| `textDecorationThickness` | `number \| "auto" \| "from-font"` | Line thickness. |
| `textUnderlineOffset` | `number \| "auto"` | Distance of the underline from the baseline. |

### Text shadow

| Key | Type | Description |
|---|---|---|
| `textShadowColor` | `ColorValue` | Shadow color. |
| `textShadowOffset` | `{ width: number; height: number }` | Shadow offset. |
| `textShadowRadius` | `number` | Blur radius. |

> [!NOTE]
> iOS draws no gradients (`backgroundImage`) or inset box shadows on `<Text>`.

## ImageStyle

All keys of `ViewStyle`, plus:

| Key | Type | Description |
|---|---|---|
| `tintColor` | `ColorValue` | Recolors a template image (every opaque pixel takes this color), for monochrome icons. |

How an image fills its box is the `resizeMode` prop of `<Image>`, not a style; see [image](../elements/image.md).

## Props that look like styles

Some settings are props in NSH, not style keys. Their Tailwind classes exist, but an explicit prop wins over the class:

| Setting | Prop | Class |
|---|---|---|
| Line limit | `numberOfLines` on `<Text>` | `line-clamp-*`, `truncate` |
| Ellipsis position | `ellipsizeMode` on `<Text>` | `text-ellipsis`, `text-clip` |
| Touch pass-through | `pointerEvents` | `pointer-events-none`, `pointer-events-auto` |
| Image fit | `resizeMode` on `<Image>` | `object-cover`, `object-contain`, ... |
| Selectable text | `selectable` on `<Text>` | `select-text`, `select-none` |
| Placeholder color | `placeholderTextColor` on `<Input>` | `placeholder:text-*` |
| Material shadow (Android) | `android={{ elevation }}` | |

## Related

- [Styling](../docs/styling.md): `style`, `class`, inheritance and precedence.
- [Tailwind reference](./tailwind-reference.md): every supported utility and variant.
- [Layout](../docs/layout.md): flexbox and grid in practice.
- [view](../elements/view.md), [text](../elements/text.md), [image](../elements/image.md): the elements these styles apply to.

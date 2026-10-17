---
title: Styling
description: The style prop - camelCase properties with flexbox, grid and CSS effects - colors, borders, corners, shadows, gradients, filters, transforms and transitions, with platform differences.
order: 32
group: ui
demo: guide-styling
---

# Styling

Every element takes a `style` prop: an object of camelCase properties, including CSS features such as box shadows, gradients, filters, outlines, transitions and grid. Styles are decoded in Rust and only the properties that changed are sent to the native view. You can also style with [Tailwind classes](./tailwind.md); both resolve to the same native styles.

```tsx
<View
  style={{
    padding: 16,
    borderRadius: 12,
    backgroundColor: "#ffffff",
    borderWidth: 1,
    borderColor: "#d0d7de",
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.12)",
  }}
>
  <Text style={{ fontSize: 17, fontWeight: "600", color: "#1f2328" }}>Card</Text>
</View>
```

This page covers how styles combine and the visual properties. Layout properties (flexbox, grid, sizes, positions) are on [Layout](./layout.md), text properties on [Text](./text.md), and every property with its type is listed in the [style reference](../modules/style-reference.md).

## The style prop

`style` takes an object, or an array of objects merged left to right. Falsy entries (`false`, `null`, `undefined`) are skipped, which makes conditional styles short:

```tsx
const styles = {
  button: { paddingHorizontal: 16, paddingVertical: 10, borderRadius: 10, backgroundColor: "#0969da" },
  disabled: { opacity: 0.4 },
  danger: { backgroundColor: "#cf222e" },
} as const;

<View style={[styles.button, props.danger && styles.danger, busy() && styles.disabled]} />
```

Define shared styles as constants outside components (with `as const`, so TypeScript keeps literal types such as `"row"`). There is no `StyleSheet.create`: a plain object is all it takes, and styles are parsed in Rust each time they are set, from objects nobody needs to register.

A style expression that reads signals is rebuilt when they change, and NSH sends only the properties whose values differ:

```tsx
<View style={{ backgroundColor: selected() ? "#ddf4ff" : "#ffffff", padding: 12 }} />
```

When an element has both, `style` wins over `class`: classes are applied first, then the inline style on top. A key set to `undefined` sets nothing (`backgroundColor: active ? accent : undefined` keeps the class color when inactive); `null` sets the default.

## Units and values

- **Numbers are points** (density-independent pixels): `width: 100` is 100pt, which is 200 or 300 physical pixels on a 2× or 3× screen.
- **Percentages** (`"50%"`) are relative to the parent, for sizes, positions, margins and paddings ([Layout](./layout.md)).
- **Angles** are degrees as numbers or strings with a unit: `45`, `"45deg"`, `"0.5rad"`.
- **Durations** are milliseconds as numbers, or strings: `"150ms"`, `"0.2s"`.

## Defaults

NSH's defaults differ from the web's:

| | Default |
|---|---|
| `flexDirection` | `"column"` |
| `flexShrink` | `0` (children do not shrink unless asked) |
| `alignContent` | `"flex-start"` |
| `position` | `"relative"` |
| `minWidth` / `minHeight` | `0` |
| `backgroundColor` | transparent |
| Text | black, 14pt, weight 400 |
| `shadowOffset` / `shadowRadius` / `shadowOpacity` | `{ width: 0, height: -3 }` / `3` / `0` |

New views carry no style at all until you give them one, so unstyled wrappers cost nothing on the native side.

## Colors

Color properties accept CSS color strings:

| Form | Example |
|---|---|
| Hex | `"#09f"`, `"#09fc"`, `"#0099ff"`, `"#0099ffcc"` |
| `rgb()` / `rgba()` | `"rgb(0, 153, 255)"`, `"rgba(0, 0, 0, 0.5)"`, `"rgb(0 0 0 / 10%)"` |
| `hsl()` / `hsla()` | `"hsl(210 100% 50%)"` |
| `oklch()` / `oklab()` | `"oklch(62.3% 0.214 259.8)"` |
| Named | `"white"`, `"black"`, `"transparent"`, `"rebeccapurple"`, ... |

For dark mode, read [`colorScheme()`](../modules/nsh-device.md) from `nsh/device`, an accessor that follows the system appearance, or use Tailwind's `dark:` variant:

```tsx
import { Text, View } from "nsh";
import { colorScheme } from "nsh/device";

const palette = () =>
  colorScheme() === "dark"
    ? { background: "#0d1117", text: "#e6edf3" }
    : { background: "#ffffff", text: "#1f2328" };

<View style={{ flex: 1, backgroundColor: palette().background }}>
  <Text style={{ color: palette().text }}>Follows the system appearance</Text>
</View>
```

The default text color is black in both appearances: set colors explicitly in apps that support dark mode.

## Borders and corners

```tsx
<View
  style={{
    borderWidth: 1,
    borderColor: "#d0d7de",
    borderStyle: "dashed",          // "solid" | "dashed" | "dotted"
    borderRadius: 12,
    borderTopLeftRadius: 24,        // per corner, overrides borderRadius
    borderBottomWidth: 3,           // per side
    borderBottomColor: "#0969da",
  }}
/>
```

- **Widths and colors** per side: `borderTopWidth`, `borderRightWidth`, ..., `borderTopColor`, ...; and the direction-aware `borderStartWidth`, `borderEndWidth`, `borderStartColor`, `borderEndColor`.
- **Radii** per corner: `borderTopLeftRadius`, `borderTopRightRadius`, `borderBottomRightRadius`, `borderBottomLeftRadius`, and `borderTopStartRadius` / `borderTopEndRadius` / `borderBottomStartRadius` / `borderBottomEndRadius`, which follow the [layout direction](./layout.md#right-to-left).
- **`borderCurve: "continuous"`** draws corners whose curvature eases into the edges, like iOS app icons, on both platforms. A pill (`borderRadius` of half the height) stays a pill.
- **`overflow: "hidden"`** clips children to the rounded shape.

Radii larger than half the side are clamped, so `borderRadius: 9999` makes a circle or a pill.

## Shadows

Three ways to cast a shadow:

```tsx
// The shadow* properties
<View style={{ shadowColor: "#000", shadowOpacity: 0.2, shadowRadius: 8, shadowOffset: { width: 0, height: 4 } }} />

// CSS box-shadow: several, comma separated, with spread and inset
<View style={{ boxShadow: "0 1px 2px rgba(0,0,0,0.08), 0 8px 24px rgba(0,0,0,0.12)" }} />
<View style={{ boxShadow: "inset 0 2px 4px rgba(0,0,0,0.15)" }} />

// Android's material elevation (a prop, not a style)
<View android={{ elevation: 6 }} style={{ backgroundColor: "#ffffff", borderRadius: 12 }} />
```

`boxShadow` follows CSS: `offsetX offsetY blur spread color`, `inset` for inner shadows. Shadows follow the view's rounded shape.

> [!NOTE]
> On Android, shadows need a background: a view with a transparent background (a text without one, for example) casts no shadow. `elevation` takes its color from `shadowColor`.

## Gradients

`backgroundImage` takes CSS gradients, drawn over the background color. Several gradients are separated by commas.

```tsx
<View style={{ height: 120, borderRadius: 16, backgroundImage: "linear-gradient(135deg, #54aeff, #8250df)" }} />
<View style={{ backgroundImage: "radial-gradient(circle at top left, #fff8c5, #fd8c73)" }} />
<View style={{ backgroundImage: "conic-gradient(red, yellow, lime, aqua, blue, magenta, red)" }} />
```

`experimental_backgroundImage` is accepted too, as an alias of `backgroundImage`. Text elements do not draw gradients on iOS.

## Filters and blending

| Property | Value |
|---|---|
| `filter` | CSS filters of the view and its content: `brightness()`, `contrast()`, `grayscale()`, `hue-rotate()`, `invert()`, `saturate()`, `sepia()`, `opacity()`, `blur()`, `drop-shadow()`; several separated by spaces |
| `backdropFilter` | Filters of what is behind the view: `"blur(10px)"` (frosted glass) |
| `mixBlendMode` | How the view blends with what is behind it: `"multiply"`, `"screen"`, `"overlay"`, ... |

```tsx
<Image src={photo} style={{ width: 120, height: 120, filter: "grayscale(1) contrast(1.2)" }} />
<View style={{ position: "absolute", bottom: 0, left: 0, right: 0, padding: 12, backgroundColor: "rgba(255,255,255,0.4)", backdropFilter: "blur(12px)" }} />
```

Platform support differs:

| | iOS | Android |
|---|---|---|
| `filter` on views | `opacity`, `brightness` up to 1 and `drop-shadow` only | All, from Android 12 (API 31); color filters only below |
| `filter` on images | Every color filter and `blur` | As views |
| `backdropFilter` | A system blur material, approximating the radius | From Android 12 (API 31) |
| `mixBlendMode` | Yes | From Android 10 (API 29) |

## Outlines, opacity and visibility

- `outlineWidth`, `outlineColor`, `outlineStyle`, `outlineOffset`: a line outside the border that takes no space (negative offsets draw inside).
- `opacity`: 0 to 1, applied to the view and its children.
- `visibility: "hidden"`: the view is not drawn but keeps its space. `display: "none"` removes it from layout.
- `zIndex`: the order among siblings, also the native stacking order (stable for equal values).
- `cursor`: the pointer shape over the view, for pointers and desktops.

## Transforms

`transform` is a list of steps applied right to left, like CSS:

```tsx
<View style={{ transform: [{ translateX: 20 }, { rotate: "-8deg" }, { scale: 0.9 }] }} />
<View style={{ transform: [{ perspective: 600 }, { rotateY: "35deg" }], backfaceVisibility: "hidden" }} />
<View style={{ transformOrigin: "top left", transform: [{ rotate: "30deg" }] }} />
```

Steps: `translateX`, `translateY`, `scale`, `scaleX`, `scaleY`, `rotate` (`rotateZ`), `rotateX`, `rotateY`, `skewX`, `skewY`, `perspective`. `transformOrigin` takes keywords, percentages and lengths (`"top left"`, `"50% 0"`, `[10, 20]`); the default is the center. Transforms do not affect layout: siblings are placed as if the view were untransformed.

Android has no skew transforms.

## Transitions

A view animates changes of its own style when you name what to animate. The animation runs natively; no JavaScript runs per frame.

```tsx
<View
  onPress={() => open.set(!open())}
  style={{
    width: 150,
    height: 72,
    borderRadius: open() ? 36 : 12,
    backgroundColor: open() ? "#8250df" : "#54aeff",
    transform: [{ translateX: open() ? 120 : 0 }],
    transitionProperty: "all",
    transitionDuration: 300,
    transitionTimingFunction: "ease-in-out",
  }}
/>
```

| Property | Value |
|---|---|
| `transitionProperty` | What animates: `"all"`, or names separated by commas: colors (`"colors"`, `"backgroundColor"`, `"borderColor"`, ...), `"opacity"`, `"transform"`, `"boxShadow"`, `"borderRadius"` / `"borderWidth"` |
| `transitionDuration` | `300`, `"300ms"`, `"0.3s"` |
| `transitionDelay` | As the duration |
| `transitionTimingFunction` | `"linear"`, `"ease"`, `"ease-in"`, `"ease-out"`, `"ease-in-out"`, `"cubic-bezier(x1, y1, x2, y2)"` |

Sizes and positions are layout, not style, and do not transition; animate layout changes with `LayoutAnimation` or drive values with native animations ([Animation](./animation.md)).

### Keyframe animations

`animationName` runs one of Tailwind's built-in keyframe animations natively: `"spin"`, `"ping"`, `"pulse"` or `"bounce"`, with `animationDuration`, `animationDelay`, `animationTimingFunction` and `animationIterationCount` (a number or `"infinite"`):

```tsx
<View style={{ width: 24, height: 24, borderRadius: 4, backgroundColor: "#0969da", animationName: "spin", animationDuration: 1000, animationIterationCount: "infinite", animationTimingFunction: "linear" }} />
```

## Press feedback

Pressed states are handled natively, without a round trip to JavaScript:

| Prop | Effect while pressed |
|---|---|
| `activeOpacity={0.6}` | The view fades to this opacity |
| `underlayColor="#eaeef2"` | The background becomes this color and the children fade to `underlayOpacity` (default 0.85) |
| `android={{ ripple: { color: "#0002" } }}` | Android's material ripple |
| `class="active:bg-zinc-100 active:scale-95"` | Any style, through Tailwind's `active:` variant |

See [Touch and gestures](./touch-and-gestures.md).

## Related

- [Tailwind](./tailwind.md): the same styles as classes, with variants for dark mode, pressed states and breakpoints.
- [Layout](./layout.md) and [Text](./text.md).
- [Style reference](../modules/style-reference.md): every style property and its type.

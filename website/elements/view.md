---
title: View
description: "The basic container element: layout, backgrounds, borders, shadows, press handling and gestures."
order: 2
demo: el-view
---

# View

`<View>` is the building block of every screen: a native container (a UIView on iOS and Mac Catalyst, a ViewGroup on
Android) that lays out its children with flexbox or grid, draws a background, borders, shadows and gradients, and handles
presses. Buttons, cards, rows and screens are all views.

```tsx
import { mount, signal, Text, View } from "nsh";

function App() {
  const liked = signal(false);
  return (
    <View style={{ flex: 1, padding: 24, gap: 16, backgroundColor: "#f6f8fa" }}>
      <View
        style={{
          backgroundColor: "#ffffff",
          borderRadius: 16,
          padding: 16,
          boxShadow: "0 4px 12px rgba(0, 0, 0, 0.08)",
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
        }}
      >
        <View style={{ width: 40, height: 40, borderRadius: 20, backgroundColor: "#8250df" }} />
        <Text style={{ flex: 1, fontSize: 16 }}>A card</Text>
        <View onPress={() => liked.set(!liked())} activeOpacity={0.5} hitSlop={8}>
          <Text style={{ fontSize: 22 }}>{liked() ? "♥" : "♡"}</Text>
        </View>
      </View>
    </View>
  );
}

mount(App);
```

## Layout

A view lays out its children with flexbox, with these defaults: `flexDirection: "column"`, `flexShrink: 0`,
`alignContent: "flex-start"`, `position: "relative"`. `display: "grid"` switches to CSS grid and `display: "block"` to
block layout. Sizes are in points. See [layout](../docs/layout.md) and the [style reference](../modules/style-reference.md).

The Tailwind `flex` class follows CSS rather than the style default: `class="flex"` is a row. Use `flex-col` for a column.

## Props

`<View>` has no props of its own beyond the [common props](./index.md#common-props): `style` (`ViewStyle`), `class`,
`classList`, `disabled`, the press props (`onPress`, `onLongPress`, `onPressIn`, `onPressOut`, `delayLongPress`,
`activeOpacity`, `underlayColor`, `underlayOpacity`, `hitSlop`, `pointerEvents`), `onHoverIn` / `onHoverOut`,
`onLayout`, `ref`, `nativeID`,
`testID`, the gesture props (`pan`, `pinch`, `rotation` and their events), the touch and responder props, the
accessibility props and `ios` / `android` / `desktop`.

## Press feedback

A pressable view is any view with a press handler. Three kinds of feedback run natively while the finger is down, with no
JS round trip:

```tsx
// Fade the whole view.
<View onPress={open} activeOpacity={0.6}>…</View>

// Show a background and fade the children (default 0.85).
<View onPress={open} underlayColor="#e5e7eb" underlayOpacity={0.7}>…</View>

// Change any style while pressed with active: classes.
<View onPress={open} class="bg-blue-600 active:bg-blue-800 active:scale-95">…</View>
```

On Android, `android={{ ripple: { color: "#0000001f" } }}` adds the material ripple (`borderless`, `radius` and
`foreground` adjust it).

`disabled` removes the press listeners from the native view (no press, no feedback) and applies `disabled:` classes:

```tsx
<View onPress={submit} disabled={!valid()} class="bg-green-600 disabled:opacity-40">…</View>
```

## Events

| Event | Payload | When |
|---|---|---|
| `onPress` | none | A tap ended inside the view. |
| `onLongPress` | none | The finger stayed down for `delayLongPress` (500 ms). |
| `onPressIn` / `onPressOut` | none | A finger went down / lifted or left. |
| `onHoverIn` / `onHoverOut` | none | The mouse came over the view (or a view inside it) / left it or the window; macOS, Linux and GPUI ([hover](./index.md#hover)). |
| `onLayout` | `{ x, y, width, height }` | The frame relative to the parent changed. |
| `onPanStart` / `onPanEnd` | `PanEvent` | A drag bound with `pan` started / ended. |
| `onPinchStart` / `onPinchEnd` | `{ scale, velocity }` | A pinch bound with `pinch` started / ended. |
| `onRotationStart` / `onRotationEnd` | `{ rotation, velocity }` | A rotation bound with `rotation` started / ended. |
| `onAccessibilityAction` | `{ actionName }` | A custom accessibility action ran. |

Touch and responder events: see [common props](./index.md#raw-touches-and-the-responder-system).

## Ref methods

A plain view has no commands of its own. `ref` gives you the `YNode`, e.g. to pass to
`AccessibilityInfo.setAccessibilityFocus` ([`nsh/accessibility`](../modules/nsh-accessibility.md)).

## Views inside text

A `<View>` placed inside a `<Text>` is an inline attachment: it sits on the baseline and wraps with the words, and it
takes presses of its own. See [text](./text.md#inline-views-and-images).

## Platform notes

- `boxShadow` and the shadow props (`shadowColor`, `shadowOpacity`, `shadowRadius`, `shadowOffset`) draw on
  both platforms; `android={{ elevation }}` gives Android's material shadow instead. On Android a shadow needs a
  background color (a view with a transparent background casts none).
- `filter` on views: iOS supports only `opacity()`, `brightness()` up to 1 and `drop-shadow()` (images take every color
  filter and `blur()`); Android supports them all from API 31, and only the color filters below it.
- `backdropFilter: "blur(…)"` is a system material on iOS (an approximation) and needs API 31 on Android.
- Android has no `skewX` / `skewY` transforms.
- `borderCurve: "continuous"` draws corners that ease into the edges (as iOS app icons), on both platforms.
- The `cursor` style applies where there is a pointer (Mac Catalyst, iPad with a pointer, desktops).

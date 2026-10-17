---
title: SafeAreaView
description: A view padded by the insets of system UI, a screen's header or a modal, plus safeArea() and useSafeAreaInsets().
order: 6
---

# SafeAreaView

`SafeAreaView` adds padding so that its content is not covered by the notch, the status bar, the home indicator, a
navigation header or the Mac Catalyst title bar. The padding is the insets **where the view renders**. Inside a
[Screen](./navigation.md) these are the insets below the screen's header. Inside a [Modal](./navigation.md#modal)
they are the modal's insets. Elsewhere they are the surface's safe area.

```tsx
import { SafeAreaView } from "nsh/ui/mobile";
```

## Usage

```tsx
import { mount, Text } from "nsh";
import { SafeAreaView } from "nsh/ui/mobile";

function App() {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: "#fff8c5", padding: 16 }}>
      <Text style={{ fontSize: 24, fontWeight: "700" }}>Never under the notch</Text>
    </SafeAreaView>
  );
}

mount(App);
```

The insets are **added** to the view's own padding. In the example, the content starts 16 pt below the top inset, and
the background color fills the whole area, including the area behind the status bar.

## Choosing edges

By default all four edges are padded. Pass `edges` to pad only some of them, for example when a tab bar or a custom
bottom bar handles the bottom inset:

```tsx
<SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1 }}>
  …
</SafeAreaView>
```

## Screens and modals

A [Screen](./navigation.md) with a visible, opaque header already places its content below the header, so the top
inset there is 0. SafeAreaView is mainly useful in these cases:

- the root of an app without a navigation stack,
- a screen with `headerShown={false}` or `headerTransparent`,
- the content of a full-screen [Modal](./navigation.md#modal).

```tsx
<Screen headerShown={false}>
  <SafeAreaView style={{ flex: 1, padding: 16 }}>
    <Button title="Back" onPress={() => nav.pop()} />
  </SafeAreaView>
</Screen>
```

## Reading the insets

When you need the numbers, for example to pad a bottom bar yourself, use one of the two accessors. Both return an
`Insets` object (`{ top, right, bottom, left }` in points) and are reactive: code that reads them runs again when they
change, for example on rotation.

| Export | Returns |
|---|---|
| `safeArea()` | The surface's safe area: the parts not covered by system UI (notch, home indicator, title bar). |
| `useSafeAreaInsets()` | Call it in a component. It returns an accessor of the insets where that component renders: the nearest screen's (below its header), the nearest modal's, or else the surface's safe area. These are the same insets SafeAreaView uses. |

```tsx
import { Text, View } from "nsh";
import { useSafeAreaInsets } from "nsh/ui/mobile";

function BottomBar() {
  const insets = useSafeAreaInsets();
  return (
    <View style={{ flexDirection: "row", padding: 8, paddingBottom: 8 + insets().bottom }}>
      <Text>Bottom bar</Text>
    </View>
  );
}
```

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `edges` | `("top" \| "right" \| "bottom" \| "left")[] \| null` | all four | The edges to pad. |
| `children` | `JSX.Element` | none | The content. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | | | Applied to the view. Its padding is added to the insets. |

`SafeAreaView` has no events and uses no theme colors.

## Platform notes

- **iOS**: the insets come from UIKit's safe area (status bar, notch or Dynamic Island, home indicator).
- **Android**: NSH reports only the parts of the system bars and the display cutout that overlap the NSH view. When
  NSH is [embedded](../docs/embedding-android.md) below a native toolbar, the view gets no top inset.
- **Mac Catalyst**: the top inset is the window's title bar.

## Related

- [Keyboard](./keyboard.md) and [KeyboardAvoidingView](./keyboard-avoiding-view.md): avoiding the software keyboard.
- [Navigation](./navigation.md): screens and modals provide their insets.
- [Device and environment](../docs/device-and-environment.md).

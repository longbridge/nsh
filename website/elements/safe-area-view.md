---
title: SafeAreaView
description: "The SafeAreaView component and the safeArea and safeAreaInsets accessors: content kept clear of the notch, the status bar, the home indicator and headers."
order: 23
group: base
demo: el-safe-area-view
---

# SafeAreaView

`SafeAreaView` keeps its content clear of the notch or Dynamic Island, the status bar, the home indicator, a
navigation header and the Mac Catalyst title bar. It pads itself by the insets **where it renders**: inside a
[NavPage](./nav-stack.md) these are the insets below the page's header, inside a [Dialog](./dialog.md) the dialog's
insets, and elsewhere the surface's safe area. It has no look of its own; its background and padding are the app's.

```rust tabs
use nsh_base::safe_area_view;
use nsh_core::ui::prelude::*;

fn app() -> impl IntoElement {
    safe_area_view()
        .flex_1()
        .bg(rgb(0xfff8c5))
        .p_4()
        .child(text("Clear of the notch").text_2xl().font_weight(FontWeight::BOLD))
}
```

```tsx tabs
import { mount, Text } from "nsh";
import { SafeAreaView } from "nsh/base";

function App() {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: "#fff8c5", padding: 16 }}>
      <Text style={{ fontSize: 24, fontWeight: "700" }}>Clear of the notch</Text>
    </SafeAreaView>
  );
}

mount(App);
```

The insets are **added** to the view's own padding. In the example, the content starts 16 pt below the top inset, and
the background color fills the whole area, including the area behind the status bar.

## Props

| Prop | Rust | Type | Default | Description |
|---|---|---|---|---|
| `edges` | `.edges(vec![Edge::Top, ..])` | `Edge[] \| null` (`"top" \| "right" \| "bottom" \| "left"`) | all four | The edges to pad. |
| `children` | `.child(..)` | `JSX.Element` | none | The content. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | style methods, `.test_id(..)` | | | Applied to the view. Its padding is added to the insets. |

## Choosing the edges

By default all four edges are padded. Pass `edges` to pad only some of them, for example when a tab bar or a bottom
bar of your own handles the bottom inset:

```rust tabs
use nsh_base::{Edge, safe_area_view};

safe_area_view()
    .edges(vec![Edge::Top, Edge::Left, Edge::Right])
    .flex_1()
```

```tsx tabs
<SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1 }}>
  …
</SafeAreaView>
```

## Where to use it

A [NavPage](./nav-stack.md) with a visible, opaque header already places its content below the header, so the top
inset there is 0. SafeAreaView is for:

- the root of an app without a navigation stack,
- a page with `headerVisible={false}` or `headerTransparent`,
- the content of a full-screen [Dialog](./dialog.md).

## safeArea and safeAreaInsets

When you need the numbers, for example to pad a bottom bar yourself, read them directly. Both TSX accessors return an
`Edges` object (`{ top, right, bottom, left }` in points) and are reactive: code that reads them runs again when they
change, for example on rotation.

| TSX | Rust | Returns |
|---|---|---|
| `safeArea()` | `safe_area()` | The surface's safe area: the parts clear of system UI (notch, home indicator, title bar). |
| `safeAreaInsets()` | `safe_area_view()`, `cx.insets()` | The insets where the component renders: the nearest page's (below its header), the nearest dialog's, or else the surface's safe area. These are the insets SafeAreaView uses. In TSX, call it in a component; it returns an accessor. In Rust, `safe_area_view()` applies them to its content and a component of your crate reads them with `cx.insets()`. |

```rust tabs
use nsh_core::ui::prelude::*;

fn bottom_bar() -> impl IntoElement {
    view()
        .flex_row()
        .p_2()
        .pb(move || px(8.0) + safe_area().bottom)
        .child(text("Bottom bar"))
}
```

```tsx tabs
import { Text, View } from "nsh";
import { safeArea } from "nsh/base";

function BottomBar() {
  return (
    <View style={{ flexDirection: "row", padding: 8, paddingBottom: 8 + safeArea().bottom }}>
      <Text>Bottom bar</Text>
    </View>
  );
}
```

Reading `safe_area()` (TSX: `safeArea()`) inside a closure, as above, subscribes the style to them: it updates when the
insets change.

## Platform notes

- **iOS**: the insets come from UIKit's safe area: the status bar, the notch or Dynamic Island, and the home
  indicator.
- **Android**: NSH reports the parts of the system bars and the display cutout that overlap the NSH view. When NSH is
  [embedded](../docs/android.md) below a native toolbar, the view's top inset is 0.
- **Mac Catalyst**: the top inset is the window's title bar.
- **Desktops (macOS, Windows, Linux, GPUI) and the web**: a page's content starts below its `header`, and a dialog
  gives its content insets of its own, as on the phones.

---
title: KeyboardAvoidingView
description: "The KeyboardAvoidingView component, Keyboard.dismiss and the keyboard's height: content kept above the software keyboard, moving with it."
order: 24
group: base
demo: el-keyboard-avoiding-view
---

# KeyboardAvoidingView

`KeyboardAvoidingView` keeps its content above the software keyboard. When the keyboard shows or hides, the view makes
room by the keyboard's height, and the change animates with the keyboard natively. Use it for content outside scroll
views, such as a message bar at the bottom of a chat screen; inputs inside a [scroll view](./scroll.md) or a
[List](./list.md) are scrolled into view natively. With it come `Keyboard.dismiss()`, which hides the keyboard, and
the keyboard's height, for layouts of your own.

```rust tabs
use nsh_base::keyboard_avoiding_view;
use nsh_core::ui::prelude::*;

fn chat() -> impl IntoElement {
    let draft = signal(String::new());
    keyboard_avoiding_view()
        .flex_1()
        .bg(white())
        .child(scroll().flex_1().child(text("Messages…").p_4()))
        .child(
            view()
                .flex_row()
                .gap_2()
                .p_2()
                .border_t_1()
                .border_color(rgb(0xe5e5e5))
                .child(
                    input()
                        .value(draft)
                        .on_change(move |v: &str| draft.set(v.to_owned()))
                        .placeholder("Message")
                        .flex_1()
                        .p_2(),
                ),
        )
}
```

```tsx tabs
import { Input, mount, Scroll, signal, Text, View } from "nsh";
import { KeyboardAvoidingView } from "nsh/base";

function App() {
  const draft = signal("");
  return (
    <KeyboardAvoidingView style={{ flex: 1, backgroundColor: "#ffffff" }}>
      <Scroll style={{ flex: 1 }} keyboardDismissMode="interactive">
        <Text style={{ padding: 16, paddingTop: 48 }}>Messages…</Text>
      </Scroll>
      <View style={{ flexDirection: "row", gap: 8, padding: 8, borderTopWidth: 1, borderTopColor: "#e5e5e5" }}>
        <Input
          value={draft()}
          onChange={(v) => draft.set(v)}
          placeholder="Message"
          style={{ flex: 1, fontSize: 16, padding: 8 }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}

mount(App);
```

The view assumes that it reaches the bottom of the screen, as a screen's content does: it makes room by the full
keyboard height. When something is below it, such as a tab bar, subtract that with `offset` (a negative value) or
add to it with a positive value.

## Props

| Prop | Rust | Type | Default | Description |
|---|---|---|---|---|
| `enabled` | `.enabled(..)` | `boolean` | `true` | Avoid the keyboard. While false, the view keeps its own layout. |
| `behavior` | `.behavior(AvoidBehavior::Height)` | `"padding" \| "height"` | `"padding"` | How the view makes room (see [Behaviors](#behaviors)). |
| `offset` | `.offset(px(..))` | `number` | `0` | Added to the keyboard's height while it shows (for example, minus the height of a bar below the view). The total is at least 0. |
| `children` | `.child(..)` | `JSX.Element` | none | The content. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | style methods, `.test_id(..)` | | | Applied to the outer view. |

## Behaviors

| `behavior` | How it makes room |
|---|---|
| `"padding"` (default) | Adds the keyboard's height to the view's bottom padding. The content above shrinks; the background still reaches the bottom of the screen. |
| `"height"` | Adds the keyboard's height as a bottom margin. The whole view, including its background, ends at the top of the keyboard. |

The padding or margin is added to what the view's own style sets, so your padding stays.

```rust tabs
use nsh_base::{AvoidBehavior, keyboard_avoiding_view};
use nsh_core::ui::prelude::*;

fn sign_in() -> impl IntoElement {
    keyboard_avoiding_view()
        .behavior(AvoidBehavior::Height)
        .offset(px(-49.0))
        .flex_1()
        .p_4()
        .gap_3()
        .child(input().placeholder("E-mail").keyboard_type(KeyboardType::Email))
}
```

```tsx tabs
<KeyboardAvoidingView behavior="height" offset={-49} style={{ flex: 1, padding: 16, gap: 12 }}>
  <Input placeholder="E-mail" keyboardType="email-address" class="rounded-lg border border-neutral-300 px-3 py-2" />
</KeyboardAvoidingView>
```

Here a 49 pt tab bar below the view already covers part of the keyboard's height.

## Hiding the keyboard

`Keyboard.dismiss()` unfocuses the focused input, which hides the keyboard. With no input focused, it leaves
everything as it is. Rust components call `dismiss_keyboard()`.

```rust tabs
use nsh_base::button;
use nsh_core::ui::prelude::*;

fn done() -> impl IntoElement {
    button()
        .on_press(|| dismiss_keyboard())
        .rounded_lg()
        .bg(rgb(0x2563eb))
        .px_4()
        .py_3()
        .child(text("Done").text_color(white()))
}
```

```tsx tabs
import { Text } from "nsh";
import { Button, Keyboard } from "nsh/base";

function Done() {
  return (
    <Button onPress={() => Keyboard.dismiss()} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
      <Text class="text-center font-semibold text-white">Done</Text>
    </Button>
  );
}
```

> [!TIP]
> To hide the keyboard when the user drags a scroll view, set `keyboardDismissMode="on-drag"` or `"interactive"` on
> the [scroll view](./scroll.md) or [List](./list.md).

## The keyboard's height

| TSX | Rust | Returns |
|---|---|---|
| `keyboardFrame()` | `keyboard_height()`, `keyboard_duration()` | The height of the software keyboard over the NSH surface, in points (0 while it is hidden), and the duration of its show or hide animation, in seconds. |

Both are reactive: an effect, a JSX expression or a style closure that reads them runs again when the keyboard shows,
hides or changes height.

```rust tabs
use nsh_core::ui::prelude::*;

fn message_bar() -> impl IntoElement {
    view()
        .flex_row()
        .p_2()
        .pb(move || {
            // The home indicator's inset only matters while the keyboard is hidden.
            let home = if keyboard_height() > Points::ZERO {
                Points::ZERO
            } else {
                safe_area().bottom
            };
            px(8.0) + home
        })
        .child(input().placeholder("Message").flex_1())
}
```

```tsx tabs
import { Input, View } from "nsh";
import { keyboardFrame, safeArea } from "nsh/base";

function MessageBar() {
  const insets = safeArea();
  return (
    <View
      style={{
        flexDirection: "row",
        padding: 8,
        // The home indicator's inset only matters while the keyboard is hidden.
        paddingBottom: 8 + (keyboardFrame().height > 0 ? 0 : insets().bottom),
      }}
    >
      <Input placeholder="Message" style={{ flex: 1 }} />
    </View>
  );
}
```

The height is the part of the keyboard that overlaps the NSH surface. When the window itself shrinks for the keyboard
(an Android activity that resizes), or when NSH is embedded in a view that ends above the keyboard, the height is 0.

> [!NOTE]
> To keep content above the keyboard, KeyboardAvoidingView is usually simpler than reading the height yourself: it
> animates with the keyboard natively.

## Platform notes

- **iOS**: the room animates with the keyboard's own duration and an ease-in-ease-out curve.
- **Android**: where the window shrinks for the keyboard (an activity that resizes), the content already ends above
  the keyboard, and KeyboardAvoidingView keeps its layout. Where the keyboard overlaps the view, it makes room as on
  iOS.
- **Desktops and Mac Catalyst** type with a hardware keyboard: the keyboard's height stays 0 and the view keeps its
  layout.

See [Forms and keyboard](../docs/forms-and-keyboard.md) for focus order, return keys and input accessories.

---
title: nsh/accessibility
description: Screen reader and reduced motion settings, announcements and accessibility focus.
order: 10
---

# nsh/accessibility

The accessibility API reads the user's accessibility settings (is a screen reader running, did they ask for less motion) and talks to the screen reader. Rust code calls `screen_reader()`, `reduce_motion()` and `announce_for_accessibility()` from `nsh_core::ui::prelude` and moves the focus with an `ElementRef`; TSX imports `screenReaderEnabled()`, `reduceMotion()`, `announce()` and `setAccessibilityFocus()` from `nsh/accessibility`. The settings are reactive: read them in a binding, `computed` or `effect`, and what reads them updates when the user changes them. The accessibility props of elements (`accessibilityLabel`, `role`, ...) are documented under [Elements](../elements/index.md); the guide is [Accessibility](../docs/accessibility.md).

```rust tabs
use nsh_core::ui::prelude::*; // screen_reader, reduce_motion, announce_for_accessibility, element_ref
```

```tsx tabs
import { announce, reduceMotion, screenReaderEnabled, setAccessibilityFocus } from "nsh/accessibility";
```

## screenReaderEnabled

```ts
const screenReaderEnabled: Accessor<boolean>;
```

Whether a screen reader (VoiceOver, TalkBack) is running. Reactive: what reads it updates when the user turns the screen reader on or off. Rust: `screen_reader() -> bool`, reactive in the same way.

```rust tabs
when(move || !screen_reader(), || text("Swipe for the next photo"))
    .otherwise(move || text("Next photo").on_press(next))
```

```tsx tabs
<If when={!screenReaderEnabled()} else={<Text onPress={next}>Next photo</Text>}>
  <Text>Swipe for the next photo</Text>
</If>
```

## reduceMotion

```ts
const reduceMotion: Accessor<boolean>;
```

Whether the user asked for less motion (Reduce Motion on iOS, Remove animations on Android). Reactive. Rust: `reduce_motion() -> bool`. Tailwind classes follow it too with the `motion-reduce:` variant.

```rust tabs
let open = move || {
    if reduce_motion() {
        panel.set(0.0);
    } else {
        let _ = panel.spring(0.0, Spring::default());
    }
};
```

```tsx tabs
const open = () => {
  if (reduceMotion()) panel.set(0);
  else panel.spring(0);
};
```

## announce

```ts
function announce(message: string): void;
```

The screen reader reads `message` (a VoiceOver announcement, TalkBack). Use it for results that appear without focus moving, such as "Saved". Rust: `announce_for_accessibility(message)`.

## setAccessibilityFocus

```ts
function setAccessibilityFocus(node: YNode): void;
```

Moves the screen reader's focus to `node` (an element's `ref`). Rust: `ElementRef::set_accessibility_focus()`.

```rust tabs
use nsh_core::ui::prelude::*;

#[component]
fn Saved() -> impl IntoElement {
    let note = element_ref();
    let save = move || {
        announce_for_accessibility("Saved");
        note.set_accessibility_focus();
    };
    // Runs again when the setting changes, until the component goes.
    effect(|| log::info!("screen reader {}", screen_reader()));
    view()
        .gap_2()
        .child(text("Save").role(AccessibilityRole::Button).on_press(save))
        .child(text("Your changes are kept on this device.").element_ref(&note))
}
```

```tsx tabs
import { effect, Text, View, type YNode } from "nsh";
import { announce, screenReaderEnabled, setAccessibilityFocus } from "nsh/accessibility";

function Saved() {
  let note!: YNode;
  const save = () => {
    announce("Saved");
    setAccessibilityFocus(note);
  };
  // Runs again when the setting changes, until the component goes.
  effect(() => console.log("screen reader", screenReaderEnabled()));
  return (
    <View style={{ gap: 8 }}>
      <Text onPress={save} role="button">Save</Text>
      <Text ref={note}>Your changes are kept on this device.</Text>
    </View>
  );
}
```

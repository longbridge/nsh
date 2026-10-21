---
title: nsh/accessibility
description: Screen reader and reduced motion settings, announcements and accessibility focus.
order: 10
---

# nsh/accessibility

The accessibility API reads the user's accessibility settings (is a screen reader running, did they ask for less motion) and talks to the screen reader. Rust code calls `screen_reader()`, `reduce_motion()` and `announce_for_accessibility()` from `nsh_core::ui::prelude` and moves the focus with an `ElementRef`; TSX imports them from `nsh/accessibility`. The accessibility props of elements (`accessibilityLabel`, `role`, ...) are documented under [Elements](../elements/index.md); the guide is [Accessibility](../docs/accessibility.md).

```rust tabs
use nsh_core::ui::prelude::*; // screen_reader, reduce_motion, announce_for_accessibility, element_ref
```

```tsx tabs
import { AccessibilityInfo, reduceMotion, screenReaderEnabled } from "nsh/accessibility";
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

## AccessibilityInfo

```ts
const AccessibilityInfo: {
  announceForAccessibility(message: string): void;
  setAccessibilityFocus(node: YNode): void;
  isScreenReaderEnabled(): Promise<boolean>;
  isReduceMotionEnabled(): Promise<boolean>;
  addEventListener(
    event: "screenReaderChanged" | "reduceMotionChanged",
    listener: (enabled: boolean) => void,
  ): { remove(): void };
};
```

The system's accessibility settings, read once or followed with listeners. `screenReaderEnabled()` and `reduceMotion()` are their reactive forms; prefer them in components.

| Member | Rust | Description |
|---|---|---|
| `announceForAccessibility(message)` | `announce_for_accessibility(message)` | The screen reader reads `message` (a VoiceOver announcement, TalkBack). Use it for results that appear without focus moving, such as "Saved". |
| `setAccessibilityFocus(node)` | `ElementRef::set_accessibility_focus()` | Moves the screen reader's focus to `node` (an element's `ref`). |
| `isScreenReaderEnabled()` | `screen_reader()` | Resolves with whether a screen reader runs. |
| `isReduceMotionEnabled()` | `reduce_motion()` | Resolves with whether reduced motion is on. |
| `addEventListener(event, listener)` | an `effect` reading `screen_reader()` or `reduce_motion()` | Calls `listener` with the new value when the setting changes; returns a subscription to `remove()`. |

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
import { cleanup, Text, View, type YNode } from "nsh";
import { AccessibilityInfo } from "nsh/accessibility";

function Saved() {
  let note!: YNode;
  const save = () => {
    AccessibilityInfo.announceForAccessibility("Saved");
    AccessibilityInfo.setAccessibilityFocus(note);
  };
  const sub = AccessibilityInfo.addEventListener("screenReaderChanged", (on) => console.log("screen reader", on));
  cleanup(() => sub.remove());
  return (
    <View style={{ gap: 8 }}>
      <Text onPress={save} role="button">Save</Text>
      <Text ref={note}>Your changes are kept on this device.</Text>
    </View>
  );
}
```

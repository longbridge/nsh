---
title: Tooltip
description: "The Tooltip component: a hint next to its trigger, shown when the pointer rests on it on desktops or a finger is held on it on touch screens, styled by the app."
order: 28
group: base
---

# Tooltip

A tooltip is a short hint next to what it is about: what an icon button does, the full text of a truncated label.

- On the desktops and in desktop browsers, it shows once the pointer has rested on the trigger for `delay`
  (500 ms), and hides when the pointer leaves or presses.
- On phones and tablets, it shows while a finger is held on the trigger and stays a moment after it lifts.

It never takes the pointer or presses, so the trigger works exactly as it does without it.

```rust tabs
use nsh_base::tooltip;
use nsh_core::ui::prelude::*;

fn save_button(on_save: impl Fn() + 'static) -> impl IntoElement {
    tooltip()
        .offset(px(6.0))
        .content(
            view()
                .rounded_md()
                .bg(rgb(0x111827))
                .px_2()
                .py_1()
                .child(text("Save the document").text_color(white())),
        )
        .child(view().on_press(on_save).p_2().child(text("💾")))
}
```

```tsx tabs
import { Text, View } from "nsh";
import { Tooltip } from "nsh/base";

function SaveButton(props: { onSave: () => void }) {
  return (
    <Tooltip
      offset={6}
      popupClass="rounded-md bg-gray-900 px-2 py-1"
      content={<Text class="text-white">Save the document</Text>}
    >
      <View onPress={props.onSave} accessibilityLabel="Save" class="p-2">
        <Text>💾</Text>
      </View>
    </Tooltip>
  );
}
```

`Tooltip` comes from [`nsh/base`](../modules/nsh-base.md). It goes above its trigger by default (`placement`), on the
other side when there is no room, and stays inside the safe area.

## Props

| Prop | Rust | Type | Default | Description |
|---|---|---|---|---|
| `content` | `.content(..)` | `JSX.Element` | none | What it says. |
| `children` | `.child(..)` | `JSX.Element` | none | The trigger. |
| `open` | `.open(..)` | `boolean` | none | Shown, controlled by the app. |
| `defaultOpen` | `.default_open(..)` | `boolean` | `false` | Shown at first, without `open`. |
| `delay` | `.delay(..)` | `number` | `500` | Desktops: milliseconds the pointer rests on the trigger before it shows. |
| `disabled` | `.disabled(..)` | `boolean` | `false` | It never shows. |
| `placement` | `.placement(..)` | `"top" \| "bottom" \| "left" \| "right"` | `"top"` | The side of the trigger it goes on. |
| `align` | `.align(..)` | `"start" \| "center" \| "end"` | `"center"` | How it lines up with the trigger along that side. |
| `offset` | `.offset(..)` | `number` | `0` | The gap between the trigger and the tooltip. |
| `margin` | `.margin(..)` | `number` | `0` | The distance it keeps from the edges of the safe area. |
| `popupClass`, `popupStyle` | `.popup_class(..)`, `.popup_style(..)` | `string`, `StyleProp<ViewStyle>` | none | The look of the popup the content is in. |

## Events

| Event | Rust | Description |
|---|---|---|
| `onOpenChange` | `.on_open_change(\|e\| ..)` | It showed or hid: `"hover"`, `"longPress"` or `"triggerPress"`. |

## Accessibility

A tooltip repeats what its trigger says to screen readers: give the trigger an `accessibilityLabel` too, so the
hint is there for everyone.

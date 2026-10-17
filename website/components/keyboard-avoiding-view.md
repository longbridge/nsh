---
title: KeyboardAvoidingView
description: Keep content outside scroll views, such as a message bar, above the software keyboard, moving with it.
order: 12
---

# KeyboardAvoidingView

`KeyboardAvoidingView` keeps its content above the software keyboard. When the keyboard shows or hides, the view makes
room by the keyboard's height, and the change animates with the keyboard. Use it for content that is **not** in a
scroll view, such as a message bar at the bottom of a chat screen.

```tsx
import { KeyboardAvoidingView } from "nsh/ui/mobile";
```

> [!TIP]
> You do not need it for inputs inside a [scroll view](../elements/scroll.md) or a [List](../elements/list.md): NSH
> scrolls the focused input into view natively.

## Usage

```tsx
import { Input, mount, Scroll, signal, Text, View } from "nsh";
import { KeyboardAvoidingView } from "nsh/ui/mobile";

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
          onChangeText={(v) => draft.set(v)}
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
keyboard height. If something is below it, such as a tab bar, subtract that with `keyboardVerticalOffset` (a negative
value) or add to it with a positive value.

## Behaviors

| `behavior` | How it makes room |
|---|---|
| `"padding"` (default) | Adds the keyboard's height to the view's bottom padding. The content above shrinks; the background still reaches the bottom of the screen. |
| `"height"` | Adds the keyboard's height as a bottom margin. The whole view, including its background, ends at the top of the keyboard. |
| `"position"` | Moves an inner content view up by the keyboard's height without changing its size. Style that inner view with `contentContainerStyle`. |

```tsx
<KeyboardAvoidingView behavior="position" contentContainerStyle={{ padding: 16, gap: 12 }} style={{ flex: 1 }}>
  <TextField label="E-mail" />
  <Button title="Sign in" onPress={signIn} />
</KeyboardAvoidingView>
```

The padding or margin is added to what the view's own style sets, so your padding stays.

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `enabled` | `boolean` | `true` | Avoid the keyboard. While false, the view does nothing. |
| `behavior` | `"padding" \| "height" \| "position"` | `"padding"` | How the view makes room (see [Behaviors](#behaviors)). |
| `keyboardVerticalOffset` | `number` | `0` | Added to the keyboard's height while it shows (for example, the height of a bar below the view). The total is never less than 0. |
| `contentContainerStyle` | `StyleProp<ViewStyle>` | none | The inner view's style with `behavior="position"`. |
| `children` | `JSX.Element` | none | The content. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | | | Applied to the outer view. |

`KeyboardAvoidingView` has no events and uses no theme colors.

## Platform notes

- **iOS**: the room animates with the keyboard's own duration and an ease-in-ease-out curve.
- **Android**: where the window shrinks for the keyboard (an activity that resizes), the content already ends above
  the keyboard, the keyboard does not overlap the NSH view, and KeyboardAvoidingView adds nothing. Where the keyboard
  overlaps the view, it makes room as on iOS.
- **Mac Catalyst** has no software keyboard; the view never moves.

## Related

- [Keyboard](./keyboard.md): `Keyboard.dismiss()` and the `keyboard()` height.
- [Forms and keyboard](../docs/forms-and-keyboard.md).
- [SafeAreaView](./safe-area-view.md).

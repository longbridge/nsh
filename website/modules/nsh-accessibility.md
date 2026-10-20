---
title: nsh/accessibility
description: Screen reader and reduced motion settings, announcements and accessibility focus.
order: 10
---

# nsh/accessibility

`nsh/accessibility` reads the user's accessibility settings (is a screen reader running, did they ask for less motion) and talks to the screen reader. The accessibility props of elements (`accessibilityLabel`, `accessibilityRole`, ...) are documented under [Elements](../elements/index.md); the guide is [Accessibility](../docs/accessibility.md).

```ts
import { AccessibilityInfo, reduceMotion, screenReaderEnabled } from "nsh/accessibility";
```

## screenReaderEnabled

```ts
const screenReaderEnabled: Accessor<boolean>;
```

Whether a screen reader (VoiceOver, TalkBack) is running. Reactive: what reads it updates when the user turns the screen reader on or off.

```tsx
<If when={!screenReaderEnabled()} else={<Text onPress={next}>Next photo</Text>}>
  <Text>Swipe for the next photo</Text>
</If>
```

## reduceMotion

```ts
const reduceMotion: Accessor<boolean>;
```

Whether the user asked for less motion (Reduce Motion on iOS, Remove animations on Android). Reactive. Tailwind classes follow it too with the `motion-reduce:` variant.

```ts
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

| Member | Description |
|---|---|
| `announceForAccessibility(message)` | The screen reader reads `message` (a VoiceOver announcement, TalkBack). Use it for results that appear without focus moving, such as "Saved". |
| `setAccessibilityFocus(node)` | Moves the screen reader's focus to `node` (an element's `ref`). |
| `isScreenReaderEnabled()` | Resolves with whether a screen reader runs. |
| `isReduceMotionEnabled()` | Resolves with whether reduced motion is on. |
| `addEventListener(event, listener)` | Calls `listener` with the new value when the setting changes; returns a subscription to `remove()`. |

```tsx
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
      <Text onPress={save} accessibilityRole="button">Save</Text>
      <Text ref={note}>Your changes are kept on this device.</Text>
    </View>
  );
}
```

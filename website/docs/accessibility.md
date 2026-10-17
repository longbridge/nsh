---
title: Accessibility
description: Making apps work with VoiceOver and TalkBack - labels, roles, states, values, custom actions, grouping and hiding, announcements and focus, text size, and reduced motion.
order: 48
group: ui
---

# Accessibility

NSH renders with native views, so the platforms' assistive technologies (VoiceOver on iOS and Mac Catalyst, TalkBack on Android, Switch Control, Voice Control, ...) see your app's real view hierarchy. What you add are the descriptions those technologies cannot infer: what an element is, what it says, what state it is in and what it can do. Every element accepts the props below.

## What you get without doing anything

- **Text is read.** A `<Text>` is an accessibility element whose label is its text. A view without a label is read as the texts inside it.
- **Pressable views are elements.** A view with `onPress` or `onLongPress` is an accessibility element by default, and activating it (double tap with VoiceOver or TalkBack) presses it.
- **Inputs are text fields**, with their placeholder, value and keyboard.
- **Text follows the user's size setting.** See [Text size](#text-size).

## Labels, hints and roles

```tsx
<View
  onPress={play}
  accessibilityRole="button"
  accessibilityLabel="Play"
  accessibilityHint="Starts the song"
  style={{ width: 44, height: 44, alignItems: "center", justifyContent: "center" }}
>
  <Text>▶</Text>
</View>
```

| Prop | Type | Description |
|---|---|---|
| `accessible` | `boolean` | The view is one element for assistive technologies; its children are not exposed separately. |
| `accessibilityLabel` | `string` | What screen readers read, instead of the texts inside. |
| `accessibilityHint` | `string` | What activating the element does, read after a pause. |
| `accessibilityRole` (or `role`) | `AccessibilityRole` | What the element is. |
| `accessibilityState` | `AccessibilityState` | Its state: disabled, selected, checked, busy, expanded. |
| `accessibilityValue` | `{ min?, max?, now?, text? }` | The value of a range or textual element. |
| `accessibilityActions` | `{ name, label? }[]` | Actions offered to assistive technologies. |
| `onAccessibilityAction` | `(e: { actionName }) => void` | An action was performed. |
| `accessibilityElementsHidden` | `boolean` | Hides the view and its subtree from assistive technologies. |
| `importantForAccessibility` | `"auto" \| "yes" \| "no" \| "no-hide-descendants"` | Whether the view (and its subtree) is exposed. |
| `android={{ accessibilityLiveRegion }}` | `"none" \| "polite" \| "assertive"` | Android: how changes of the view are announced to TalkBack. |
| `testID` | `string` | An identifier for UI tests (the accessibility identifier on iOS, the resource id on Android); not read aloud. |

Give every icon-only control a label: a screen reader cannot read a glyph or an image. Keep labels short and do not include the role ("Play", not "Play button"): the role is announced on its own.

### Roles

`accessibilityRole` tells the user what kind of element this is and how to operate it. The values are `"none"`, `"button"`, `"link"`, `"search"`, `"image"`, `"imagebutton"`, `"keyboardkey"`, `"text"`, `"adjustable"`, `"header"`, `"summary"`, `"alert"`, `"checkbox"`, `"combobox"`, `"menu"`, `"menubar"`, `"menuitem"`, `"progressbar"`, `"radio"`, `"radiogroup"`, `"scrollbar"`, `"spinbutton"`, `"switch"`, `"tab"`, `"tablist"`, `"timer"`, `"toolbar"`, `"list"` and `"grid"`.

The most useful ones:

- `"button"` for anything pressable that is not a link; `"link"` for navigation to content.
- `"header"` for section titles: screen reader users jump between headers to skim a screen.
- `"switch"`, `"checkbox"`, `"radio"` with `accessibilityState.checked`.
- `"adjustable"` for sliders and steppers, with `accessibilityValue` and the `increment` / `decrement` actions.
- `"image"` for meaningful images, with a label; decorative images should be hidden instead.

```tsx
<Text accessibilityRole="header" style={{ fontSize: 22, fontWeight: "700" }}>Settings</Text>
```

### States and values

```tsx
// A toggle drawn with views.
<View
  onPress={() => on.set(!on())}
  accessibilityRole="switch"
  accessibilityLabel="Wi-Fi"
  accessibilityState={{ checked: on() }}
  hitSlop={8}
  class="h-[31px] w-[51px] rounded-full p-0.5 transition-colors"
  classList={{ "bg-green-500": on(), "bg-zinc-300": !on() }}
>
  <View class="size-[27px] rounded-full bg-white transition-transform" classList={{ "translate-x-5": on() }} />
</View>
```

`accessibilityState` is `{ disabled?, selected?, checked?: boolean | "mixed", busy?, expanded? }`. `disabled` is also implied by the `disabled` prop. `accessibilityValue` with `min`, `max` and `now` is read as a percentage; with `text` it is read as given ("3 of 10", "Medium").

## Actions

A pressable view supports activation already. For anything else, declare actions and handle them:

```tsx
function Quantity() {
  const n = signal(1);
  return (
    <View
      accessible
      accessibilityRole="adjustable"
      accessibilityLabel="Quantity"
      accessibilityValue={{ text: String(n()) }}
      accessibilityActions={[{ name: "increment" }, { name: "decrement" }]}
      onAccessibilityAction={(e) => {
        if (e.actionName === "increment") n.update((v) => v + 1);
        if (e.actionName === "decrement") n.update((v) => Math.max(0, v - 1));
      }}
      style={{ flexDirection: "row", alignItems: "center", gap: 12 }}
    >
      <View onPress={() => n.update((v) => Math.max(0, v - 1))}><Text>−</Text></View>
      <Text>{n()}</Text>
      <View onPress={() => n.update((v) => v + 1)}><Text>+</Text></View>
    </View>
  );
}
```

Some action names are standard and map to the platforms' own gestures:

| Name | iOS (VoiceOver) | Android (TalkBack) |
|---|---|---|
| `activate` | Double tap | Double tap (click) |
| `increment`, `decrement` | Swipe up / down on an adjustable element | Scroll forward / backward |
| `longpress` | | Double tap and hold |
| `expand`, `collapse` | | Expand / collapse |
| `magicTap` | Two-finger double tap | |
| `escape` | Two-finger Z scrub | |

Any other name becomes a custom action, listed in VoiceOver's actions rotor and TalkBack's actions menu with its `label` (or its name). Custom actions are the accessible form of swipe-to-delete and similar gestures:

```tsx
<View
  accessibilityActions={[{ name: "archive", label: "Archive" }, { name: "delete", label: "Delete" }]}
  onAccessibilityAction={(e) => (e.actionName === "archive" ? archive(mail.id) : remove(mail.id))}
>
  …
</View>
```

## Grouping and hiding

`accessible` makes a view one element: a card with a title, a subtitle and a price is read in one go, instead of the user swiping through three texts.

```tsx
<View accessible onPress={open} accessibilityRole="button" style={{ padding: 16 }}>
  <Text>Blue jacket</Text>
  <Text>Size M</Text>
  <Text>$120</Text>
</View>
```

Hide what adds nothing: decorative images, dividers, an icon next to a text that already says the same.

```tsx
<Image src={decoration} accessibilityElementsHidden importantForAccessibility="no-hide-descendants" />
```

`accessibilityElementsHidden` (iOS's name) and `importantForAccessibility="no-hide-descendants"` (Android's) both hide the subtree on both platforms; setting both is harmless.

## AccessibilityInfo

`nsh/accessibility` reports the user's settings and lets you talk to the screen reader.

| Member | Description |
|---|---|
| `screenReaderEnabled()` | Whether VoiceOver or TalkBack runs (reactive). |
| `reduceMotion()` | Whether the user asked for less motion (reactive). |
| `AccessibilityInfo.isScreenReaderEnabled()` | The same, as a promise. |
| `AccessibilityInfo.isReduceMotionEnabled()` | The same, as a promise. |
| `AccessibilityInfo.addEventListener(event, listener)` | `"screenReaderChanged"` or `"reduceMotionChanged"`; returns `{ remove() }`. |
| `AccessibilityInfo.announceForAccessibility(message)` | The screen reader reads `message`. |
| `AccessibilityInfo.setAccessibilityFocus(ref)` | Moves the screen reader's focus to a view. |

Prefer the reactive accessors in components: they update what reads them, and need no cleanup.

```tsx
import { signal, Text, View, type YNode } from "nsh";
import { AccessibilityInfo, screenReaderEnabled } from "nsh/accessibility";

function Upload() {
  const done = signal(false);
  let result!: YNode;
  const finish = () => {
    done.set(true);
    AccessibilityInfo.announceForAccessibility("Upload complete");
    AccessibilityInfo.setAccessibilityFocus(result);
  };
  return (
    <View style={{ gap: 12 }}>
      {/* Autoplaying carousels are hard to use with a screen reader: show a list instead. */}
      <If when={!screenReaderEnabled()} else={<PhotoList />}>
        <PhotoCarousel />
      </If>
      <Text ref={result}>{done() ? "Uploaded 3 photos" : "Uploading…"}</Text>
    </View>
  );
}
```

Announce changes the user would otherwise miss: a background save finishing, an error appearing away from the focus, a timer ending. On Android, `android={{ accessibilityLiveRegion: "polite" }}` on a view makes TalkBack announce its changes by itself.

## Text size

Text and inputs follow the system text size (Dynamic Type on iOS, font size on Android) unless you set `allowFontScaling={false}`. Rather than turning it off, design for it:

- Let text wrap: avoid fixed heights on views that contain text.
- Cap very large headings with `maxFontSizeMultiplier` instead of disabling scaling.
- Switch layouts when the text is large: read `fontScale()` from `nsh/device` (for example, stack a label and its value vertically above 1.3).

See [Device and environment](./device-and-environment.md#text-size).

## Reduced motion

`reduceMotion()` is `true` when the user turned on iOS's Reduce Motion or Android's remove animations setting. NSH does not disable your animations by itself. Use it, or the `motion-reduce:` and `motion-safe:` Tailwind variants, to replace large movement with a fade or an instant change:

```tsx
import { View } from "nsh";
import { reduceMotion } from "nsh/accessibility";
import { LayoutAnimation } from "nsh/animated";

const toggle = () => {
  if (!reduceMotion()) LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
  expanded.set(!expanded());
};

<View onPress={open} class="transition active:scale-95 motion-reduce:transition-none" />
<View class="motion-safe:animate-bounce" />
```

## Touch targets

Make pressable elements at least 44 × 44 points. When a control must look smaller, extend its touch area with `hitSlop` instead of its size; see [Touch and gestures](./touch-and-gestures.md#hit-areas-hitslop).

## Checking your app

- Turn on VoiceOver (iOS Settings, Accessibility, or triple-click the side button if set up) or TalkBack, and use the app without looking: every control should say what it is and what it does.
- On iOS simulators, Xcode's Accessibility Inspector shows each element's label, traits and value.
- Try the largest text size and dark mode.
- `testID` gives UI tests a stable handle that does not change with the label's language.

## Related

- [`nsh/accessibility`](../modules/nsh-accessibility.md): the reference.
- [`View`](../elements/view.md): every accessibility prop.
- [Animation](./animation.md#reduced-motion) and [Device and environment](./device-and-environment.md).

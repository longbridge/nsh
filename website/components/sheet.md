---
title: Sheet
description: A bottom sheet over a dimmed backdrop that the user drags down, taps outside or backs out of to close.
order: 8
demo: ui-sheet
---

# Sheet

`Sheet` is a panel that slides up from the bottom of the screen over a dimmed backdrop. The user closes it by dragging
it down, tapping the backdrop or pressing the Android back button. The drag and the animations run natively: the panel
follows the finger on the UI thread, with no JS and no render per frame.

```tsx
import { Sheet } from "nsh/ui/mobile";
```

## Usage

`Sheet` is controlled. It is shown while `open` is true. When the user closes it, `onClose` is called, and you set
`open` to false:

```tsx
import { mount, signal, Text, View } from "nsh";
import { Button, Sheet } from "nsh/ui/mobile";

function App() {
  const open = signal(false);
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48 }}>
      <Button title="Share" onPress={() => open.set(true)} />
      <Sheet open={open()} onClose={() => open.set(false)}>
        <Text style={{ fontSize: 17, fontWeight: "600" }}>Share the photo</Text>
        <Button title="Copy link" variant="secondary" onPress={() => open.set(false)} />
        <Button title="Done" onPress={() => open.set(false)} />
      </Sheet>
    </View>
  );
}

mount(App);
```

## Behavior

- **Opening**: when `open` becomes true, the panel slides in from below the screen with a spring, and the backdrop
  fades in as the panel rises.
- **Dragging**: the panel follows a vertical drag. When the user releases it more than 120 pt below its resting
  position, or flicks it down faster than 900 pt/s, it closes. Otherwise it springs back, continuing at the speed of
  the flick.
- **Closing by the user** (drag, backdrop tap, Android back button): the panel slides down in 220 ms, then `onClose`
  is called. Always set `open` to false in `onClose`: while `open` stays true, the sheet's modal stays presented
  after the panel has slid out of view, and it keeps covering the screen.
- **Closing from your code**: setting `open` to false removes the sheet at once, without the slide-down animation, and
  does not call `onClose`.
- **Content**: the children are made when the sheet opens and released once it is closed, so they start fresh each
  time (a form is empty again, a component reads the current state when it is made). Keep what must survive a close
  in a signal outside the sheet.

The sheet is presented in a transparent [modal](../elements/modal.md), so it covers everything, including a navigation
header and a tab bar. It can be declared anywhere in the tree.

## Layout

The panel is as tall as its content, and the content is stacked in a column with 12 pt gaps. The panel has a grabber
at the top, 16 pt horizontal padding, rounded top corners, and bottom padding of the safe area's bottom inset plus
16 pt, so the content stays above the home indicator.

For a list in a sheet, give the list's parent a height:

```tsx
<Sheet open={picking()} onClose={() => picking.set(false)}>
  <Text style={{ fontSize: 17, fontWeight: "600" }}>Choose a city</Text>
  <View style={{ height: 320 }}>
    <SelectList
      options={cities}
      value={city()}
      onSelect={(i) => {
        city.set(i);
        picking.set(false);
      }}
    />
  </View>
</Sheet>
```

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `open` | `boolean` | `false` | Whether the sheet is shown. |
| `children` | `JSX.Element` | none | The panel's content. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | | | Applied to the sheet's host view in your tree, not to the panel. |

## Events

| Event | Payload | When |
|---|---|---|
| `onClose` | none | The user closed the sheet (drag, backdrop tap or back button), after it slid away. Set `open` to false. |

## Theming

| Part | Theme key |
|---|---|
| Panel | `background` |
| Backdrop | `overlay` |
| Grabber | `muted.background` |
| Top corners | `radius.lg`, at least 16 pt |

See [Theming](./index.md#theming). The default `overlay` color is light (5% black in light mode), so the backdrop is
only slightly dimmed. Set `overlay` in your theme for a darker backdrop.

## Accessibility

The backdrop is labeled "Close", so screen reader users can close the sheet by activating it. On Android, the back
button closes the sheet.

## Related

- [Modal](./navigation.md#modal): full-screen and page-sheet modals.
- [Animation](../docs/animation.md): animated values and springs.
- [ActionSheet](./action-sheet.md): the system's action sheet.

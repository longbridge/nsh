---
title: Dialog
description: "The Dialog component: a modal surface over everything else, for full-screen flows, sheets and dialogs the app styles."
order: 11
---

# Dialog

A dialog presents content over the rest of the app in its own native window layer: a presented view controller on iOS
and Mac Catalyst, a Dialog on Android, a layer over the whole window on the desktops and the web. It is used for
full-screen flows, sheets and dialogs. The system's own gestures (swiping a sheet down, the Android back button,
Escape) work, and are decided natively.

The dialog owns its behavior and leaves its look to the app: `class` and `style` style the popup (the view holding the
children), and `backdropClass` / `backdropStyle` add a backdrop behind it.

```tsx
import { signal, Text, View } from "nsh";
import { Button, Dialog, SafeAreaView } from "nsh/base";

function Example() {
  const open = signal(false);
  return (
    <View style={{ flex: 1, justifyContent: "center", padding: 24 }}>
      <Button onPress={() => open.set(true)} class="items-center rounded-lg bg-blue-600 px-4 py-3 active:opacity-70">
        <Text class="font-semibold text-white">Open</Text>
      </Button>
      <Dialog open={open()} presentation="pageSheet" onOpenChange={(next) => open.set(next)} class="flex-1 bg-white">
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12 }}>
          <Text style={{ fontSize: 24, fontWeight: "700" }}>A sheet</Text>
          <Button onPress={() => open.set(false)} class="items-center rounded-lg bg-blue-600 px-4 py-3 active:opacity-70">
            <Text class="font-semibold text-white">Close</Text>
          </Button>
        </SafeAreaView>
      </Dialog>
    </View>
  );
}
```

`Dialog` comes from [`nsh/base`](../modules/nsh-base.md#dialog). It is shown while `open` is not `false` and closed
when it becomes `false`, and it gives its content the dialog's safe-area insets, which `SafeAreaView` and
`safeAreaInsets()` inside it read. `Button`, also from `nsh/base`, is a press target styled by the app.

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `open` | `boolean` | `true` | Shown while true. |
| `animation` | `"slide" \| "fade" \| "none"` | `"slide"` | How it comes in and goes. |
| `presentation` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | Its form on iOS: `fullScreen` covers the screen with a clear background; `pageSheet` and `formSheet` are system sheets that keep what is behind visible (`formSheet` is a smaller centered sheet on large screens). |
| `closeOnEscape` | `boolean` | `true` | Escape and the Android back button close it natively. |
| `closeOnBackdropPress` | `boolean` | `true` | A press on the backdrop closes it. |
| `closeOnSwipe` | `boolean` | `true` | Swiping a sheet down (iOS, phone browsers) closes it natively. |
| `statusBar` | `{ foreground?, hidden?, animated? }` | none | iOS and Android: the status bar while the dialog is open, over the pages' ([Status bar](../modules/nsh-base.md#status-bar)). |
| `onOpen` | `() => void` | | It finished appearing. |
| `onClose` | `() => void` | | It went away: the user closed it, or the app removed it. |
| `onCancel` | `() => void` | | The user tried to close it (Escape, back, a backdrop press, a swipe down), whether or not a `closeOn*` flag let it close. |
| `onOpenChange` | `(open: boolean, reason: "cancel" \| "backdropPress" \| "swipe") => void` | | The user closed it: keep `open` in sync here. |
| `class`, `style` | `string`, `StyleProp<ViewStyle>` | | The popup: the view holding the children. |
| `backdropClass`, `backdropStyle` | `string`, `StyleProp<ViewStyle>` | | A backdrop filling the dialog behind the popup, present when either is set. |
| `children` | `JSX.Element` | | The content. |

| `ios` key | Type | Default | Description |
|---|---|---|---|
| `supportedOrientations` | `("portrait" \| "portrait-upside-down" \| "landscape" \| "landscape-left" \| "landscape-right")[]` | the presenter's | The orientations the dialog allows. |

| `android` key | Type | Default | Description |
|---|---|---|---|
| `statusBarTranslucent` | `boolean` | `false` | The content reaches under the status bar; otherwise it starts below it. |

## Opening and closing

`open` is the app's state. When the user closes the dialog (Escape, the Android back button, a press on the backdrop,
a sheet swiped down), it closes natively and `onOpenChange(false, reason)` tells the app, which sets its state to
match. While `open` stays `true` the dialog stays closed, and it opens again when `open` goes from `false` to `true`.

Each `closeOn*` flag is read natively when the user acts, since the main thread never waits for JS. To confirm before
closing, set the flags to `false`: the attempt then calls `onCancel`, and the app closes the dialog itself by setting
`open` to `false`:

```tsx
<Dialog
  open={open()}
  presentation="pageSheet"
  closeOnEscape={false}
  closeOnBackdropPress={false}
  closeOnSwipe={false}
  onCancel={() => askToDiscard.set(true)}
  class="flex-1 bg-white"
>
  …
</Dialog>
```

`onCancel` is called on every attempt, whether the flags let the dialog close or not.

## A dialog with a backdrop

A dialog with the default `fullScreen` presentation shows what the app draws: here a dimmed backdrop and a card centered
over it. A press on the backdrop closes it (`closeOnBackdropPress`):

```tsx
<Dialog
  open={confirm()}
  animation="fade"
  onOpenChange={(next) => confirm.set(next)}
  backdropClass="bg-black/40"
  class="m-auto w-80 gap-3 rounded-2xl bg-white p-5"
>
  <Text style={{ fontSize: 20, fontWeight: "700" }}>Delete this item?</Text>
  <Button onPress={remove} class="items-center rounded-lg bg-red-600 px-4 py-3 active:opacity-70">
    <Text class="font-semibold text-white">Delete</Text>
  </Button>
</Dialog>
```

For the system's alert or action sheet, use [`AlertDialog`](../modules/nsh-base.md#alertdialog) from `nsh/base`.

## Ref methods

None.

## Platform notes

- iOS and Mac Catalyst: `presentation` picks the form. `fullScreen` presents over the full screen with a clear
  background; `pageSheet` and `formSheet` are system sheets with the system background, which follow the finger when
  swiped down (`closeOnSwipe`).
- Android: the dialog fills the window and shows the app's popup and backdrop. The back button goes to the topmost
  dialog first, before stacks and `BackHandler` listeners.
- Desktops (macOS, Windows, Linux, GPUI) and the web: the dialog is a transparent layer over the whole window that
  shows the app's popup and backdrop. Escape is a close attempt.
- `supportedOrientations` is iOS's; `statusBarTranslucent` is Android's. Without `statusBarTranslucent`, an Android
  dialog's content starts below the status bar.

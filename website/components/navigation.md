---
title: Navigation
description: Native navigation stacks with createNavigator, Stack and Screen, header options, back handling and Modal.
order: 16
demo: ui-navigation
---

# Navigation

`nsh/ui/mobile` provides native navigation: a `Stack` of `Screen`s driven by a navigator you create with
`createNavigator`, and `Modal` for content presented over everything else. A stack is `UINavigationController` on iOS,
with the native header and swipe back. On Android it is a stack of screens with a toolbar and predictive back.

```tsx
import { createNavigator, Modal, Screen, Stack } from "nsh/ui/mobile";
```

This page is the API reference. For a walkthrough, see the [Navigation guide](../docs/navigation.md).

## Usage

```tsx
import { mount, View } from "nsh";
import { Button, createNavigator, Screen, Stack } from "nsh/ui/mobile";

type Route = { name: "home" } | { name: "detail"; id: number };

const nav = createNavigator<Route>({ name: "home" });

function Home() {
  return (
    <Screen title="Inbox">
      <View style={{ flex: 1, padding: 16 }}>
        <Button title="Open message 1" onPress={() => nav.push({ name: "detail", id: 1 })} />
      </View>
    </Screen>
  );
}

function Detail(props: { id: number }) {
  return (
    <Screen title={`Message ${props.id}`}>
      <View style={{ flex: 1, padding: 16 }}>
        <Button title="Back" variant="secondary" onPress={() => nav.pop()} />
      </View>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) => (route.name === "home" ? <Home /> : <Detail id={route.id} />)}
    </Stack>
  );
}

mount(App);
```

## How a stack works

- The navigator holds the **routes**: plain values of your choosing, from the bottom of the stack to the top. A route
  is usually an object that names the screen and carries its parameters.
- `Stack` calls its child function once for each route and shows the result as a screen. It does not call the
  function again for the same route, so a screen keeps its state (signals, scroll position, inputs) while it is in the
  stack, also while other screens cover it.
- When the user leaves a screen natively (the back button, swipe back, Android back), the stack removes its route from
  the navigator, so `nav.routes()` always matches what the user sees.
- Routes are compared by **identity** (`===`). Each route in a stack must be a distinct value: push a new object
  (`{ name: "detail", id }`) rather than reusing one, and avoid string routes for screens that can appear twice.
- If the child function returns something other than a `<Screen>`, the stack wraps it in a screen with default
  options. Return a `<Screen>` to set a title and other header options.

## createNavigator

```ts
function createNavigator<R>(initial: R | R[]): Navigator<R>
```

Creates a navigator whose stack starts with `initial` (one route or several). Create it once, for example at module
level, and pass it to `<Stack navigator>`. Any code can import it to navigate.

| Member | Description |
|---|---|
| `routes: Accessor<readonly R[]>` | The routes, bottom to top. Reactive. |
| `push(route: R): void` | Pushes a screen. |
| `pop(count?: number): void` | Pops `count` screens (default 1). At least one screen always stays. |
| `popToTop(): void` | Pops back to the first screen. |
| `replace(route: R): void` | Replaces the top screen. |
| `reset(routes: R[]): void` | Replaces all screens. |
| `remove(route: R): void` | Removes a route wherever it is in the stack. |

```tsx
nav.push({ name: "detail", id: 7 });
nav.pop(2);
nav.reset([{ name: "home" }, { name: "detail", id: 1 }]);
const depth = () => nav.routes().length;
```

## Stack

```tsx
<Stack navigator={nav} style={{ flex: 1 }}>
  {(route, index) => …}
</Stack>
```

| Prop | Type | Description |
|---|---|---|
| `navigator` | `Navigator<R>` | The navigator that holds the routes. Required. |
| `children` | `(route: R, index: Accessor<number>) => JSX.Element` | Renders a route. Return a `<Screen>` for header options. Required. |
| `style` | `StyleProp<ViewStyle>` | The stack's style; usually `{ flex: 1 }`. |

The child function receives the route value itself and an accessor of its position in the stack.

## Screen

`Screen` is one screen of a stack and configures its header:

```tsx
<Screen
  title="Profile"
  headerTintColor="#ffffff"
  headerBackgroundColor="#1a7f37"
  headerTitleColor="#ffffff"
  headerRight={<Text onPress={edit} style={{ color: "#ffffff", fontSize: 17 }}>Edit</Text>}
>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</Screen>
```

| Prop | Type | Default | Description |
|---|---|---|---|
| `title` | `string` | none | The header's title. |
| `headerShown` | `boolean` | `true` | Whether the header is shown. |
| `headerTransparent` | `boolean` | `false` | The content extends under a transparent header. |
| `headerTintColor` | `ColorValue` | platform | The color of the back button and the bar buttons. |
| `headerBackgroundColor` | `ColorValue` | platform | The header's background color. |
| `headerTitleColor` | `ColorValue` | platform | The title's color. |
| `headerBackVisible` | `boolean` | `true` | Whether the back button is shown. |
| `headerLeft` | `JSX.Element` | none | Content at the left of the header, in place of the back button. |
| `headerRight` | `JSX.Element` | none | Content at the right of the header. |
| `headerTitle` | `JSX.Element` | none | Content that replaces the title. |
| `gestureEnabled` | `boolean` | `true` | The interactive back gesture: iOS swipe back, Android predictive back that follows the finger. The back buttons work either way. |
| `preventBack` | `boolean` | `false` | The user cannot leave the screen. See [Leaving a screen](#leaving-a-screen). |
| `animation` | `"default" \| "fade" \| "none"` | `"default"` | How the screen enters and leaves. |
| `ios` | `{ headerLargeTitle?: boolean; headerBackTitle?: string }` | none | iOS only: a large title that shrinks into the bar as the content scrolls; the label of the back button that leads back to this screen. |
| `desktop` | `{ headerLargeTitle?: boolean; headerBackTitle?: string }` | none | Desktop only (macOS, Windows, Linux): the same options on a desktop header. |
| `style` | `StyleProp<ViewStyle>` | none | The screen's style. |
| `children` | `JSX.Element` | none | The content. |
| `onBackRequest` | `() => void` | none | The user tried to leave a screen that has `preventBack`. |
| `onDismiss` | `() => void` | none | The user left the screen natively and its route was removed. |

The header items (`headerLeft`, `headerRight`, `headerTitle`) are ordinary JSX: they can read signals, handle presses
and update while the screen is shown.

The screen's content starts below an opaque header. With `headerTransparent` or `headerShown={false}`, it starts at
the top of the screen; use [SafeAreaView](./safe-area-view.md) or `useSafeAreaInsets()` to keep it clear of the header
and the status bar.

```tsx
<Screen title="Large title" ios={{ headerLargeTitle: true }}>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</Screen>
```

### Leaving a screen

`preventBack` keeps the user on a screen, for example while a form has unsaved changes. While it is true, swipe back is
off, and the header's back button and the Android back button call `onBackRequest` instead of popping. Ask the user,
then pop yourself:

```tsx
import { Input, signal } from "nsh";
import { Alert, Screen } from "nsh/ui/mobile";

function EditNote() {
  const text = signal("");
  const confirm = async () => {
    const i = await Alert.alert("Discard your changes?", undefined, [
      { text: "Keep editing", style: "cancel" },
      { text: "Discard", style: "destructive" },
    ]);
    if (i === 1) nav.pop();
  };
  return (
    <Screen title="Edit" preventBack={text().length > 0} onBackRequest={confirm}>
      <Input value={text()} onChangeText={(v) => text.set(v)} placeholder="A note" />
    </Screen>
  );
}
```

> [!IMPORTANT]
> The system asks whether the user may go back on the main thread, at the moment the user acts, and the main thread
> never waits for JS. So NSH answers from the `preventBack` value already mounted. Set `preventBack` as soon as there
> is something to lose, not in a handler that runs when the user goes back.

To turn off only the swipe gesture and keep the back buttons, use `gestureEnabled={false}`.

## Modal

`Modal` presents content over everything else, including the stack's header, in its own native window layer. It is
shown while `visible` is true (the default), and it is controlled: when the user asks to close it, `onRequestClose` is
called and you set `visible` to false.

```tsx
import { signal, Text, View } from "nsh";
import { Button, Modal, SafeAreaView } from "nsh/ui/mobile";

function Settings() {
  const open = signal(false);
  return (
    <View>
      <Button title="About" onPress={() => open.set(true)} />
      <Modal visible={open()} ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12 }}>
          <Text style={{ fontSize: 24, fontWeight: "700" }}>About</Text>
          <Button title="Close" onPress={() => open.set(false)} />
        </SafeAreaView>
      </Modal>
    </View>
  );
}
```

| Prop | Type | Default | Description |
|---|---|---|---|
| `visible` | `boolean` | `true` | Shown while true. |
| `animationType` | `"slide" \| "fade" \| "none"` | `"slide"` | How the modal appears. |
| `transparent` | `boolean` | `false` | The background shows what is behind, for dialogs and overlays. |
| `dismissible` | `boolean` | `false` | The user closes it natively (iOS sheet swiped down, Android back button); then `onRequestClose` and `onDismiss` are called. Without it, those gestures only ask to close. |
| `ios` | `{ presentationStyle?: "fullScreen" \| "pageSheet" \| "formSheet"; supportedOrientations?: (…)[] }` | none | iOS only: `pageSheet` and `formSheet` keep what is behind visible; the orientations the modal allows (default: the presenter's). |
| `desktop` | `{ presentationStyle?: "fullScreen" \| "pageSheet" \| "formSheet" }` | none | Desktop only (macOS, Windows, Linux): `pageSheet` and `formSheet` are cards centered in the window over a dimmed backdrop; clicking the backdrop or pressing Escape asks to close. |
| `android` | `{ statusBarTranslucent?: boolean }` | none | Android only: the content reaches under the status bar (by default it starts below it). |
| `style` | `StyleProp<ViewStyle>` | none | The modal's style. |
| `children` | `JSX.Element` | none | The content. |
| `onRequestClose` | `() => void` | none | The user asked to close it (Android back button, iOS sheet swipe). Set `visible` to false. |
| `onShow` | `() => void` | none | The modal was shown. |
| `onDismiss` | `() => void` | none | The modal was dismissed. |

A modal provides its own insets to its content, so [SafeAreaView](./safe-area-view.md) and `useSafeAreaInsets()`
inside it use the modal's insets. A modal with `transparent` and `animationType="fade"` works as a custom dialog:

```tsx
<Modal visible={open()} transparent animationType="fade" onRequestClose={close}>
  <View style={{ flex: 1, backgroundColor: "rgba(0,0,0,0.4)", alignItems: "center", justifyContent: "center", padding: 32 }}>
    <View style={{ alignSelf: "stretch", backgroundColor: "#ffffff", borderRadius: 14, padding: 20, gap: 12 }}>
      <Text style={{ fontSize: 20, fontWeight: "700" }}>A dialog</Text>
      <Button title="OK" onPress={close} />
    </View>
  </View>
</Modal>
```

See [Modal](../elements/modal.md) for more about the element.

## Platform notes

- **iOS**: `UINavigationController` with the native header, back button and swipe back. `ios={{ headerLargeTitle }}`
  and `headerBackTitle` are iOS only. Stack changes that arrive during a running transition are applied when it ends.
- **Android**: a stack with a toolbar. The system back button pops the top screen, with predictive back on Android
  versions that have it. See `BackHandler` in [nsh/android](../modules/nsh-android.md) to handle the back button
  yourself.
- **Mac Catalyst**: as on iOS. Page and form sheets are presented as macOS sheets the size of the window.

## Related

- [Navigation guide](../docs/navigation.md).
- [SafeAreaView](./safe-area-view.md): insets below headers and in modals.
- [Sheet](./sheet.md): a bottom sheet drawn by NSH, the same on both platforms.
- [Alert](./alert.md): confirming before leaving a screen.

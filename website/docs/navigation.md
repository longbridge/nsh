---
title: Navigation
description: Native navigation stacks with createNavigator, Stack and Screen, header options, keeping the user on a screen with preventBack, modals, and the Android back button.
order: 41
group: ui
---

# Navigation

`nsh/ui/mobile` provides a native navigation stack and native modals. A `Stack` is a UINavigationController on iOS, with its header, swipe back and large titles, and a stack with a toolbar and predictive back on Android. You keep the list of routes in a navigator; the stack renders each route as a screen and keeps it mounted, with its state, while it is in the stack.

```tsx
import { mount, Scroll, Text, View } from "nsh";
import { createNavigator, Screen, Stack } from "nsh/ui/mobile";

type Route = { name: "inbox" } | { name: "message"; id: number };

const nav = createNavigator<Route>({ name: "inbox" });

function Inbox() {
  return (
    <Screen title="Inbox">
      <Scroll style={{ flex: 1 }}>
        <Each of={[1, 2, 3, 4, 5]}>
          {(id) => (
            <View onPress={() => nav.push({ name: "message", id: id() })} underlayColor="#eaeef2" style={{ padding: 16 }}>
              <Text>Message {id()}</Text>
            </View>
          )}
        </Each>
      </Scroll>
    </Screen>
  );
}

function Message(props: { id: number }) {
  return (
    <Screen title={`Message ${props.id}`}>
      <View style={{ flex: 1, padding: 16 }}>
        <Text>The body of message {props.id}.</Text>
      </View>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) => {
        switch (route.name) {
          case "inbox":
            return <Inbox />;
          case "message":
            return <Message id={route.id} />;
        }
      }}
    </Stack>
  );
}

mount(App);
```

## The navigator

`createNavigator(initial)` creates the stack of routes. A route is any value you like (a string, an object, a discriminated union as above); routes are compared by identity, so push a new object for each screen. Create the navigator once, at module level or in your root component, and import it wherever screens navigate.

| Member | Description |
|---|---|
| `routes()` | The routes, bottom to top (reactive). |
| `push(route)` | Pushes a screen. |
| `pop(count?)` | Pops `count` screens (default 1); at least one stays. |
| `popToTop()` | Pops back to the first screen. |
| `replace(route)` | Replaces the top screen. |
| `reset(routes)` | Replaces all screens, e.g. after signing in. |
| `remove(route)` | Removes a route wherever it is. |

`createNavigator` also takes an array, to start several screens deep: `createNavigator<Route>([{ name: "inbox" }, { name: "message", id: 3 }])`.

When the user leaves a screen natively (the back button in the header, a swipe back on iOS, Android's back button or gesture), the stack removes that route from the navigator for you, so `routes()` always matches what is on screen.

A route carries what the screen needs. The gallery app pushes routes that hold the screen component itself, which keeps the route type small when there are many screens:

```ts
import { type JSX } from "nsh";
import { createNavigator } from "nsh/ui/mobile";

export type Route = { name: "home" } | { name: "page"; page: () => JSX.Element };
export const nav = createNavigator<Route>({ name: "home" });

// Elsewhere: nav.push({ name: "page", page: Settings });
```

## Stack

| Prop | Type | Description |
|---|---|---|
| `navigator` | `Navigator<R>` | The routes to show. |
| `children` | `(route: R, index: Accessor<number>) => JSX.Element` | Renders a route; return a `<Screen>` for header options. |
| `style` | `StyleProp<ViewStyle>` | The stack's own style (usually `flex: 1`). |

The render function runs once per route, when it is pushed; `route` is the plain value, `index()` its current position. What it returns becomes the screen's content. Return a `<Screen>` to set the header; anything else is wrapped in a screen with default options.

Screens stay mounted while their route is in the stack: going back shows the previous screen exactly as it was left, with its scroll position, inputs and signals. A screen's [`cleanup`](./reactivity.md) handlers run when its route leaves the stack.

## Screen and the header

`Screen` wraps a screen's content and carries its header options. All options are reactive: change a signal and the header follows.

| Prop | Type | Default | Description |
|---|---|---|---|
| `title` | `string` | | The header's title. |
| `headerShown` | `boolean` | `true` | The header shows. |
| `headerTransparent` | `boolean` | `false` | The content extends under the header. |
| `headerTintColor` | `ColorValue` | | The back button and bar buttons. |
| `headerBackgroundColor` | `ColorValue` | | The header's background. |
| `headerTitleColor` | `ColorValue` | | The title. |
| `headerBackVisible` | `boolean` | `true` | The back button shows. |
| `headerLeft` | `JSX.Element` | | Views at the start of the header (replace the back button). |
| `headerRight` | `JSX.Element` | | Views at the end of the header. |
| `headerTitle` | `JSX.Element` | | Views replacing the title. |
| `animation` | `"default" \| "fade" \| "none"` | `"default"` | How the screen enters and leaves. |
| `gestureEnabled` | `boolean` | `true` | The interactive back gesture (iOS swipe back, Android predictive back). The back buttons pop either way. |
| `preventBack` | `boolean` | `false` | The user cannot leave; back calls `onBackRequest` instead. |
| `onBackRequest` | `() => void` | | The user tried to leave a screen with `preventBack`. |
| `onDismiss` | `() => void` | | The user left the screen natively; its route is already removed. |
| `ios={{ headerLargeTitle }}` | `boolean` | | iOS: a large title that shrinks into the bar as the content scrolls. |
| `ios={{ headerBackTitle }}` | `string` | | iOS: the label of the back button that leads back to this screen. |
| `style` | `StyleProp<ViewStyle>` | | The screen's content view. |

Header items are ordinary views, so they can be pressable and reactive:

```tsx
function Compose() {
  const text = signal("");
  return (
    <Screen
      title="New message"
      headerLeft={<Text onPress={() => nav.pop()} style={{ fontSize: 17, color: "#0969da" }}>Cancel</Text>}
      headerRight={
        <Text
          onPress={() => send(text())}
          style={{ fontSize: 17, fontWeight: "600", color: text() ? "#0969da" : "#8c959f" }}
        >
          Send
        </Text>
      }
    >
      <Input multiline value={text()} onChangeText={(t) => text.set(t)} style={{ flex: 1, padding: 16, fontSize: 17 }} />
    </Screen>
  );
}
```

A colored header:

```tsx
<Screen title="Profile" headerBackgroundColor="#1a7f37" headerTintColor="#ffffff" headerTitleColor="#ffffff">
  …
</Screen>
```

A screen without a header still has the system bars around it; pad its content with [`SafeAreaView`](../components/safe-area-view.md):

```tsx
<Screen headerShown={false}>
  <SafeAreaView style={{ flex: 1, padding: 16 }}>…</SafeAreaView>
</Screen>
```

### Safe areas under the header

Inside a screen, `useSafeAreaInsets()` and `SafeAreaView` report the insets below the header, not the whole surface's. With `headerTransparent`, the content starts at the top of the screen and the top inset is the header's height, so `SafeAreaView` keeps text clear of it while a background image fills the whole screen. With `ios={{ headerLargeTitle: true }}`, put a scroll view directly in the screen so the title can collapse as it scrolls.

## Keeping the user on a screen: preventBack

To ask before discarding unsaved changes, set `preventBack` while there is something to lose and handle `onBackRequest`:

```tsx
import { Input, signal } from "nsh";
import { Alert, Screen } from "nsh/ui/mobile";

function EditNote() {
  const text = signal("");
  const confirm = async () => {
    const choice = await Alert.alert("Discard your changes?", undefined, [
      { text: "Keep editing", style: "cancel" },
      { text: "Discard", style: "destructive" },
    ]);
    if (choice === 1) nav.pop();
  };
  return (
    <Screen title="Edit" preventBack={text().length > 0} onBackRequest={confirm}>
      <Input value={text()} onChangeText={(t) => text.set(t)} placeholder="A note" style={{ margin: 16, fontSize: 17 }} />
    </Screen>
  );
}
```

With `preventBack` on, the swipe back is disabled and the back buttons (in the header, Android's back button and gesture) call `onBackRequest` instead of popping. `nav.pop()` from code still pops.

> [!IMPORTANT]
> `preventBack` is a prop, not a callback that returns true or false, because the main thread never waits for JS. When the user starts a swipe or presses back, the system asks right away whether the screen may go, and NSH answers natively from the props it already has. Set `preventBack` as soon as there is something to lose (as above, from the text), not when the user tries to leave.

`gestureEnabled={false}` is the lighter option: it only turns off the interactive gesture (iOS swipe back, Android's predictive back animation), and the back buttons still pop.

## Modals

`Modal` presents content over everything else, in its own native window layer: a presented view controller on iOS, a dialog on Android. Render it where its state lives and control it with `visible`:

```tsx
import { signal, Text, View } from "nsh";
import { Modal, SafeAreaView } from "nsh/ui/mobile";

function Settings() {
  const open = signal(false);
  return (
    <View style={{ flex: 1 }}>
      <View onPress={() => open.set(true)} style={{ padding: 16 }}>
        <Text>Edit profile</Text>
      </View>
      <Modal visible={open()} animationType="slide" ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12 }}>
          <Text style={{ fontSize: 22, fontWeight: "700" }}>Profile</Text>
          <View onPress={() => open.set(false)}>
            <Text style={{ color: "#0969da" }}>Done</Text>
          </View>
        </SafeAreaView>
      </Modal>
    </View>
  );
}
```

| Prop | Type | Default | Description |
|---|---|---|---|
| `visible` | `boolean` | `true` | Shown while true. |
| `animationType` | `"slide" \| "fade" \| "none"` | | How it appears. |
| `transparent` | `boolean` | `false` | The background shows what is behind (dialogs, custom sheets). |
| `dismissible` | `boolean` | `false` | The user closes it natively (iOS sheet swiped down, Android back), then `onRequestClose` and `onDismiss` follow. |
| `onRequestClose` | `() => void` | | The user asked to close it: set `visible` to false. |
| `onShow` | `() => void` | | It has appeared. |
| `onDismiss` | `() => void` | | It has gone. |
| `ios={{ presentationStyle }}` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | iOS: sheets keep what is behind visible. |
| `ios={{ supportedOrientations }}` | `("portrait" \| "landscape" \| …)[]` | the presenter's | iOS: the orientations it allows. |
| `android={{ statusBarTranslucent }}` | `boolean` | `false` | Android: the content reaches under the status bar. |

Modals are **controlled** by default: a swipe down on an iOS sheet or Android's back button only asks, through `onRequestClose`, and the modal stays until you set `visible` to false. That lets you confirm first. With `dismissible`, the platform closes it right away and then tells you; use it for sheets that hold nothing to lose. As with `preventBack`, the choice is a prop because the system asks whether the sheet may close on the spot.

A transparent modal makes a custom dialog:

```tsx
<Modal visible={open()} transparent animationType="fade" onRequestClose={() => open.set(false)}>
  <View style={{ flex: 1, backgroundColor: "rgba(0,0,0,0.4)", alignItems: "center", justifyContent: "center", padding: 32 }}>
    <View style={{ alignSelf: "stretch", backgroundColor: "#ffffff", borderRadius: 14, padding: 20, gap: 12 }}>
      <Text style={{ fontSize: 20, fontWeight: "700" }}>Delete the file?</Text>
      <View onPress={() => open.set(false)}><Text style={{ color: "#cf222e" }}>Delete</Text></View>
    </View>
  </View>
</Modal>
```

Inside a modal, `SafeAreaView` and `useSafeAreaInsets()` give the modal's own insets. For system dialogs (an alert with buttons, an action sheet) use [`Alert`](../components/alert.md) and [`ActionSheet`](../components/action-sheet.md); for a panel that slides up over a dimmed backdrop and follows the finger, use [`Sheet`](../components/sheet.md).

## The Android back button

On Android, the back button (or gesture) is handled in this order:

1. The top modal: `onRequestClose` (or it closes, with `dismissible`).
2. Your `BackHandler` listeners from `nsh/android`, most recently added first.
3. The top stack: it pops its top screen, or calls `onBackRequest` with `preventBack`.
4. Nothing handled it: the app exits.

A listener returns `true` when it handled the press, which keeps the stack from popping. While a listener is registered,
Android's back gesture does not preview the pop (the app answers only once the gesture ends), so add listeners only
while they may take the press:

```tsx
import { cleanup, signal } from "nsh";
import { BackHandler } from "nsh/android";

function Editor() {
  const selecting = signal(false);
  const sub = BackHandler.addEventListener("hardwareBackPress", () => {
    if (!selecting()) return false; // let the stack pop
    selecting.set(false);
    return true;
  });
  cleanup(() => sub.remove());
  // …
}
```

`BackHandler.exitApp()` leaves the app. On iOS, `nsh/android` can be imported and its calls do nothing, so the same code runs on both platforms. See [Platform-specific code](./platform-specific.md).

## Tabs and other layouts

`nsh/ui/mobile` has no tab bar component. A tab bar is a row of pressable views and a signal holding the selected tab; render the selected tab's content with a function child or [`<If>`](../elements/control-flow.md). To keep each tab's state while switching, give each tab its own `Stack` and navigator and keep all of them mounted, hiding the inactive ones with `display: "none"`.

## Related

- [Navigation components](../components/navigation.md): the `createNavigator`, `Stack`, `Screen` and `Modal` reference.
- [`SafeAreaView`](../components/safe-area-view.md) and [`StatusBar`](../components/status-bar.md).
- [`Alert`](../components/alert.md), [`ActionSheet`](../components/action-sheet.md) and [`Sheet`](../components/sheet.md).
- [`nsh/android`](../modules/nsh-android.md): `BackHandler`.

---
title: nsh/base
description: "Unstyled base components for an app's UI: buttons, navigation stacks and dialogs, safe areas, system alert dialogs, the software keyboard and the status bar of pages and dialogs."
order: 2.5
demo: ui-navigation
---

# nsh/base

`nsh/base` is the base component layer of an app's UI: unstyled components that own behavior, state, focus and
accessibility, on every platform. It sits between the primitives (`nsh` and the `nsh/<module>` modules) and the
app's own component library, as `gpui-base` sits between `gpui` and a styled library in GPUI Kit (see
[Modules](./index.md#three-layers)). It provides controls (`Button`), navigation (a `NavStack` of `NavPage`s driven
by the state you create with `createNavStack`, and `Dialog` for content presented over everything else), safe areas
(`SafeAreaView`, `safeArea()` and `safeAreaInsets()`), the system's alert dialogs (`AlertDialog`), the software
keyboard (`Keyboard`, `keyboardFrame()` and `KeyboardAvoidingView`). Pages and dialogs also decide the status bar
(`statusBar`).

The look is the app's: what goes inside a page, a dialog or a header is your own JSX, drawn from the primitives with
styles, [Tailwind](../docs/tailwind.md) classes and your theme, Rust components from [your own crate](../docs/rust.md),
or a component library the community builds on `nsh/base`. Every host serves `nsh/base`; Rust apps use the same API
from the `nsh-base` crate (`nsh_base::{SafeAreaView, …}`).

```tsx
import {
  AlertDialog, Button, createNavStack, Dialog, Keyboard, keyboardFrame, KeyboardAvoidingView, NavPage, NavStack,
  SafeAreaView, safeArea, safeAreaInsets,
} from "nsh/base";
import type {
  AlertDialogAction, AlertDialogOptions, AlertDialogPresentation, AlertDialogRole, AvoidBehavior, ButtonProps,
  DialogChangeReason, DialogProps, Edge, HeaderStyle, Insets, NavPageProps, NavStackProps, NavStackState, StatusBar,
  StatusBarForeground,
} from "nsh/base";
```

This page is the API reference. For walkthroughs, see the [Navigation guide](../docs/navigation.md) and
[Forms and keyboard](../docs/forms-and-keyboard.md).

## Controls

### Button

A press target that owns its behavior and has no look: presses, the disabled and selected states and its role for
screen readers. It has no colors, sizes or label of its own. Style it with `class` or `style`, its states with the
`active:`, `hover:`, `focus:`, `selected:` and `disabled:` variants (or `activeStyle`, `hoverStyle`, `focusStyle`,
`selectedStyle` and `disabledStyle`), and put its content inside, so one `Button` becomes your primary button, an icon
button, a tab or a row of a list.

```tsx
import { mount, signal, Text } from "nsh";
import { Button } from "nsh/base";

function App() {
  const count = signal(0);
  return (
    <Button
      onPress={() => count.update((n) => n + 1)}
      class="self-center px-5 py-3 rounded-xl bg-blue-600 active:bg-blue-700 disabled:opacity-40"
    >
      <Text class="text-white text-base font-semibold">{`Pressed ${count()}`}</Text>
    </Button>
  );
}

mount(App);
```

| Prop | Type | |
|---|---|---|
| `disabled` | `boolean` | No presses and no press feedback; `disabled:` styles apply and screen readers say it is dimmed. |
| `selected` | `boolean` | Selected (a toggled trigger, the current tab); screen readers say so. The app owns the value and styles it. |
| `selectedStyle` | `StyleProp<ViewStyle>` | The style while `selected` (and not disabled), as `selected:` classes. |
| `accessibilityLabel` | `string` | What screen readers say, when the content is not enough (an icon). |
| `role` | `AccessibilityRole` | What it is for screen readers (default `"button"`): `"link"`, `"tab"`, `"menuitem"`, ... |
| `onPress` | `() => void` | It was pressed. |
| `onPressDown`, `onPressUp` | `() => void` | A touch went down on it, and ended or left: for your own press feedback (a spring, a highlight). |
| `class`, `style` | | Its look, all of it. |
| `activeStyle`, `hoverStyle`, `focusStyle`, `disabledStyle` | `StyleProp<ViewStyle>` | The style while pressed, hovered, focused or disabled, as the `<state>:` classes. Later states win; `disabled` wins over all. |

A row of tabs is a `Button` per tab with `role="tab"`, `selected` on the current one and its look in `selected:`
classes:

```tsx
<Button role="tab" selected={tab() === "inbox"} onPress={() => tab.set("inbox")}
  class="rounded-full px-4 py-2 selected:bg-blue-600 active:opacity-70">
  <Text class="text-base">Inbox</Text>
</Button>
```

## Navigation

A navigation stack is `UINavigationController` on iOS, with the system header and swipe back, and a stack of pages
with a toolbar and predictive back on Android. On the desktops (macOS, Windows, Linux, GPUI) and the web, each page
shows the app's own `header` at its top, full width, with the content below it; the back keys (Escape, Alt+Left, a
mouse's back button) and the browser's back button pop the page.

The pages' content and the desktop header are your own JSX. Here a small `Action` component draws the buttons:

```tsx
import { mount, Text, View } from "nsh";
import { Button, createNavStack, NavPage, NavStack } from "nsh/base";

type Route = { name: "home" } | { name: "detail"; id: number };

const nav = createNavStack<Route>([{ name: "home" }]);

function Action(props: { title: string; onPress: () => void }) {
  return (
    <Button onPress={props.onPress} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
      <Text class="text-center text-base font-semibold text-white">{props.title}</Text>
    </Button>
  );
}

function Bar(props: { title: string }) {
  return (
    <View class="flex-row items-center gap-3 border-b border-neutral-200 px-4 py-3">
      {nav.routes().length > 1 && <Action title="Back" onPress={() => nav.pop()} />}
      <Text class="text-lg font-semibold">{props.title}</Text>
    </View>
  );
}

function Home() {
  return (
    <NavPage title="Inbox" header={<Bar title="Inbox" />}>
      <View style={{ flex: 1, padding: 16 }}>
        <Action title="Open message 1" onPress={() => nav.push({ name: "detail", id: 1 })} />
      </View>
    </NavPage>
  );
}

function Detail(props: { id: number }) {
  const title = `Message ${props.id}`;
  return (
    <NavPage title={title} header={<Bar title={title} />}>
      <View style={{ flex: 1, padding: 16 }}>
        <Text>The message.</Text>
      </View>
    </NavPage>
  );
}

function App() {
  return (
    <NavStack state={nav} style={{ flex: 1 }}>
      {(route) => (route.name === "home" ? <Home /> : <Detail id={route.id} />)}
    </NavStack>
  );
}

mount(App);
```

On iOS and Android the page shows the system header with `title` and a back button; on the desktops and the web it
shows `header`. An app that targets only phones leaves `header` out; one that targets only desktops leaves out the
system header's options.

### How a stack works

- The stack's state holds the **routes**: plain values of your choosing, from the bottom of the stack to the top. A
  route is usually an object that names the page and carries its parameters.
- `NavStack` calls its child function once for each push and shows the result as a page. It does not call the
  function again for that page, so a page keeps its state (signals, scroll position, inputs) while it is in the stack,
  also while other pages cover it.
- Each push is a page of its own, the same route value pushed twice included.
- When the user leaves a page natively (the back button, swipe back, Android back, the desktops' back keys, the
  browser's back), the stack removes its route from the state, so `nav.routes()` always matches what the user sees.
- If the child function returns something other than a `<NavPage>`, the stack wraps it in a page with default
  options. Return a `<NavPage>` to set its header and other options.

### createNavStack

```ts
function createNavStack<R>(routes: R[]): NavStackState<R>
```

Creates the state of a stack that starts with `routes`, bottom to top. Create it once, for example at module level,
and pass it to `<NavStack state>`. Any code can import it to navigate.

| Member | Description |
|---|---|
| `routes: Accessor<readonly R[]>` | The routes, bottom to top. Reactive. |
| `push(route: R): void` | Pushes a page. |
| `pop(count?: number): void` | Pops `count` pages (default 1). At least one page always stays. |
| `popToRoot(): void` | Pops back to the first page. |
| `replace(route: R): void` | Replaces the top page. |
| `setRoutes(routes: R[]): void` | Replaces the pages; a route that stays (the same value) keeps its page and its state. |
| `remove(route: R): void` | Removes the topmost page of `route` (compared with `===`), wherever it is in the stack. |

```tsx
nav.push({ name: "detail", id: 7 });
nav.pop(2);
nav.setRoutes([{ name: "home" }, { name: "detail", id: 1 }]);
const depth = () => nav.routes().length;
```

### NavStack

```tsx
<NavStack state={nav} style={{ flex: 1 }}>
  {(route, index) => …}
</NavStack>
```

| Prop | Type | Description |
|---|---|---|
| `state` | `NavStackState<R>` | The state that holds the routes (`createNavStack`). Required. |
| `children` | `(route: R, index: Accessor<number>) => JSX.Element` | Renders a route. Return a `<NavPage>` for its options. Required. |
| `style`, `class` | | The stack's style; usually `flex: 1`. |

The child function receives the route value itself and an accessor of its position in the stack.

### NavPage

`NavPage` is one page of a stack. It holds the page's content, its `header` on the desktops and the web, and the
options of the system header on iOS and Android:

```tsx
<NavPage
  title="Profile"
  header={<ProfileBar />}
  headerStyle={{ backgroundColor: "#1a7f37", color: "#ffffff", tintColor: "#ffffff" }}
  headerRight={<Text onPress={edit} style={{ color: "#ffffff", fontSize: 17 }}>Edit</Text>}
>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</NavPage>
```

| Prop | Type | Default | Description |
|---|---|---|---|
| `header` | `JSX.Element` | none | Desktops and the web: the page's header, placed at the top of the page, as wide as the page and as tall as it lays out, with the content below it. |
| `title` | `string` | none | iOS and Android: the system header's title. |
| `headerShown` | `boolean` | `true` | iOS and Android: the system header shows. |
| `headerTransparent` | `boolean` | `false` | iOS and Android: the content extends under a transparent system header. |
| `headerLargeTitle` | `boolean` | `false` | iOS: a large title that shrinks into the bar as the content scrolls; the content scrolls under the header. |
| `headerBackTitle` | `string` | none | iOS and Android: the label of the back button that leads back to this page (on Android, its description for screen readers). |
| `headerBackVisible` | `boolean` | `true` | iOS and Android: the back button shows. |
| `headerLeft` | `JSX.Element` | none | iOS and Android: content at the leading edge of the system header, in place of the back button. |
| `headerRight` | `JSX.Element` | none | iOS and Android: content at the trailing edge of the system header. |
| `headerTitle` | `JSX.Element` | none | iOS and Android: content that replaces the title. |
| `headerStyle` | `{ backgroundColor?, color?, tintColor? }` | platform | iOS and Android: the system header's colors: its background, the title's (`color`) and the buttons' (`tintColor`). |
| `headerClass` | `string` | none | The same colors as classes: `bg-*`, `text-*` for the title, `tint-*` for the buttons. `headerStyle` wins over them. |
| `popOnBack` | `boolean` | `true` | The back buttons and the swipe back pop the page. See [Leaving a page](#leaving-a-page). |
| `popOnSwipe` | `boolean` | `true` | The interactive back gesture pops the page: iOS swipe back, Android predictive back that follows the finger. The back buttons pop either way. |
| `animation` | `"default" \| "slide" \| "fade" \| "none"` | `"default"` | How the page enters and leaves. |
| `statusBar` | `{ foreground?, hidden?, animated? }` | none | iOS and Android: the status bar while the page is on top. See [Status bar](#status-bar). |
| `style`, `class` | | none | The page's style. |
| `children` | `JSX.Element` | none | The content. |
| `onBackPress` | `() => void` | none | The user pressed back on the page (the header's back button, Android back, Escape, Alt+Left or a mouse's back button on the desktops, the browser's back), whether or not `popOnBack` let it pop. |
| `onDismiss` | `() => void` | none | The user popped the page (back button, swipe); its route is gone. |

The header items (`header`, `headerLeft`, `headerRight`, `headerTitle`) are ordinary JSX: they can read signals,
handle presses and update while the page is shown.

The page's content starts below the header: below an opaque system header on iOS and Android, below `header` on the
desktops and the web. With `headerTransparent`, `headerLargeTitle` or `headerShown={false}`, it starts at the top of
the page on iOS and Android; use [SafeAreaView](#safeareaview) or `safeAreaInsets()` to keep it clear of the header
and the status bar.

```tsx
<NavPage title="Large title" headerLargeTitle>
  <Scroll style={{ flex: 1 }}>…</Scroll>
</NavPage>
```

#### Leaving a page

`popOnBack={false}` keeps the user on a page, for example while a form has unsaved changes. While it is false, swipe
back is off and the back buttons and keys call `onBackPress` without popping. Ask the user, then pop yourself:

```tsx
import { Input, signal } from "nsh";
import { AlertDialog, NavPage } from "nsh/base";

function EditNote() {
  const text = signal("");
  const confirm = async () => {
    const choice = await AlertDialog.show({
      title: "Discard your changes?",
      actions: [
        { label: "Keep editing", role: "cancel" },
        { label: "Discard", role: "destructive", id: "discard" },
      ],
    });
    if (choice === "discard") nav.pop();
  };
  return (
    <NavPage title="Edit" popOnBack={text().length === 0} onBackPress={() => text().length > 0 && confirm()}>
      <Input value={text()} onChange={(v) => text.set(v)} placeholder="A note" />
    </NavPage>
  );
}
```

> [!IMPORTANT]
> The system asks whether the user may go back on the main thread, at the moment the user acts, and the main thread
> never waits for JS. So NSH answers from the `popOnBack` value already mounted. Set `popOnBack={false}` as soon as
> there is something to lose, before the user goes back.

`onBackPress` is called on every back press, also when the page pops, so it also tells you that the user went back.
To turn off only the swipe gesture and keep the back buttons, use `popOnSwipe={false}`.

### Dialog

`Dialog` presents content over everything else, including the stack's header, in its own native window layer. It has
no look of its own: `class` and `style` style the popup (the view that holds its children), and `backdropClass` or
`backdropStyle` add a backdrop behind it, whose presses close the dialog. It is shown while `open` is true (the
default). The user closes it with Escape, the Android back button, a press on the backdrop or a swipe down on an iOS
sheet; then `onOpenChange(false, reason)` is called and you set `open` to false.

```tsx
import { signal, Text, View } from "nsh";
import { Button, Dialog } from "nsh/base";

function Settings() {
  const open = signal(false);
  return (
    <View>
      <Button onPress={() => open.set(true)} class="self-start active:opacity-60">
        <Text class="text-blue-600">About</Text>
      </Button>
      <Dialog
        open={open()}
        onOpenChange={(next) => open.set(next)}
        animation="fade"
        backdropClass="bg-black/40"
        class="m-auto w-80 gap-3 rounded-2xl bg-white p-5"
      >
        <Text class="text-xl font-bold">About</Text>
        <Button onPress={() => open.set(false)} class="self-end active:opacity-60">
          <Text class="font-semibold text-blue-600">Close</Text>
        </Button>
      </Dialog>
    </View>
  );
}
```

| Prop | Type | Default | Description |
|---|---|---|---|
| `open` | `boolean` | `true` | Shown while true. |
| `animation` | `"slide" \| "fade" \| "none"` | `"slide"` | How it comes in and goes. |
| `presentation` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | iOS: `pageSheet` and `formSheet` keep what is behind visible. |
| `closeOnEscape` | `boolean` | `true` | Escape and the Android back button close it. |
| `closeOnBackdropPress` | `boolean` | `true` | A press on the backdrop closes it. |
| `closeOnSwipe` | `boolean` | `true` | Swiping a sheet down (iOS, phone browsers) closes it. |
| `statusBar` | `{ foreground?, hidden?, animated? }` | none | iOS and Android: the status bar while the dialog is open. See [Status bar](#status-bar). |
| `class`, `style` | | none | The popup's style. |
| `backdropClass`, `backdropStyle` | | none | With either, a backdrop fills the dialog behind the popup. |
| `ios` | `{ supportedOrientations?: (…)[] }` | none | iOS: the orientations it allows (default: the presenter's). |
| `android` | `{ statusBarTranslucent?: boolean }` | none | Android: the content reaches under the status bar (by default it starts below it). |
| `onOpenChange` | `(open: boolean, reason: "cancel" \| "backdropPress" \| "swipe") => void` | none | The user closed it. Set `open` to false. |
| `onCancel` | `() => void` | none | The user tried to close it, whether or not a `closeOn*` flag let it close. |
| `onOpen` | `() => void` | none | It finished appearing. |
| `onClose` | `() => void` | none | It went away: the user closed it, or the app removed it. |

A dialog provides its own insets to its content, so [SafeAreaView](#safeareaview) and `safeAreaInsets()` inside it
use the dialog's insets. On the desktops and the web a dialog is a transparent layer over the whole window, and the
popup and backdrop you style are all it shows. See [Dialog](../elements/dialog.md) for sheets, full-screen flows and
dialogs that ask before they close.

## Safe areas

### SafeAreaView

`SafeAreaView` adds padding so that its content stays clear of the notch, the status bar, the home indicator, a
navigation header or the Mac Catalyst title bar. The padding is the insets **where the view renders**. Inside a
[NavPage](#navpage) these are the insets below the page's header. Inside a [Dialog](#dialog) they are the dialog's
insets. Elsewhere they are the surface's safe area.

```tsx
import { mount, Text } from "nsh";
import { SafeAreaView } from "nsh/base";

function App() {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: "#fff8c5", padding: 16 }}>
      <Text style={{ fontSize: 24, fontWeight: "700" }}>Clear of the notch</Text>
    </SafeAreaView>
  );
}

mount(App);
```

The insets are **added** to the view's own padding. In the example, the content starts 16 pt below the top inset, and
the background color fills the whole area, including the area behind the status bar.

By default all four edges are padded. Pass `edges` to pad only some of them, for example when a tab bar or a bottom
bar of your own handles the bottom inset:

```tsx
<SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1 }}>
  …
</SafeAreaView>
```

A [NavPage](#navpage) with a visible, opaque header already places its content below the header, so the top inset
there is 0. SafeAreaView is for:

- the root of an app without a navigation stack,
- a page with `headerShown={false}` or `headerTransparent`,
- the content of a full-screen [Dialog](#dialog).

| Prop | Type | Default | Description |
|---|---|---|---|
| `edges` | `Edge[] \| null` (`"top" \| "right" \| "bottom" \| "left"`) | all four | The edges to pad. |
| `children` | `JSX.Element` | none | The content. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | | | Applied to the view. Its padding is added to the insets. |

### safeArea and safeAreaInsets

When you need the numbers, for example to pad a bottom bar yourself, use one of the two accessors. Both return an
`Insets` object (`{ top, right, bottom, left }` in points) and are reactive: code that reads them runs again when they
change, for example on rotation.

| Export | Returns |
|---|---|
| `safeArea()` | The surface's safe area: the parts clear of system UI (notch, home indicator, title bar). |
| `safeAreaInsets()` | Call it in a component. It returns an accessor of the insets where that component renders: the nearest page's (below its header), the nearest dialog's, or else the surface's safe area. These are the insets SafeAreaView uses. |

```tsx
import { Text, View } from "nsh";
import { safeAreaInsets } from "nsh/base";

function BottomBar() {
  const insets = safeAreaInsets();
  return (
    <View style={{ flexDirection: "row", padding: 8, paddingBottom: 8 + insets().bottom }}>
      <Text>Bottom bar</Text>
    </View>
  );
}
```

### Navigation and safe areas on each platform

- **iOS**: `UINavigationController` with the system header, back button and swipe back; `headerLargeTitle` is
  UIKit's large title. Stack changes that arrive during a running transition are applied when it ends. The insets come
  from UIKit's safe area (status bar, notch or Dynamic Island, home indicator).
- **Android**: a stack with a toolbar. The system back button pops the top page, with predictive back on Android
  versions that have it; the up arrow's description for screen readers is the lower page's `headerBackTitle`, or the
  system's. See `BackHandler` in [nsh/android](./nsh-android.md) to handle the back button yourself. NSH reports the
  parts of the system bars and the display cutout that overlap the NSH view; when NSH is
  [embedded](../docs/embedding-android.md) below a native toolbar, the view's top inset is 0.
- **Mac Catalyst**: as on iOS. Page and form sheets are presented as macOS sheets the size of the window. The top
  inset is the window's title bar.
- **Desktops (macOS, Windows, Linux, GPUI) and the web**: each page shows its `header` at the top and its content
  below it. Escape, Alt+Left and a mouse's back button go back on the desktops, the browser's back button on the web.
  A dialog is a transparent layer over the whole window.

## Alert dialogs

`AlertDialog` shows the platform's own alert or action sheet: `UIAlertController` on iOS, `AlertDialog` (or a list
dialog for an action sheet) on Android. It returns a promise that resolves with the action the user chose.

### AlertDialog

```ts
AlertDialog.show(options: AlertDialogOptions): Promise<string | null>
```

The promise resolves with the chosen action's `id` (its `label` when it has no `id`), or `null` when the dialog closed
without an action (a press outside it, Escape or the back button, where `closeOnBackdropPress` and `closeOnEscape`
allow it). It rejects when `actions` is empty.

```tsx
import { mount, signal, Text, View } from "nsh";
import { AlertDialog, Button } from "nsh/base";

function App() {
  const answer = signal("–");
  const confirm = async () => {
    const choice = await AlertDialog.show({
      title: "Delete the photo?",
      description: "This cannot be undone.",
      actions: [
        { label: "Cancel", role: "cancel" },
        { label: "Delete", role: "destructive", id: "delete" },
      ],
    });
    answer.set(choice === "delete" ? "deleted" : "kept");
  };
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48, gap: 12 }}>
      <Button onPress={confirm} class="rounded-lg bg-red-600 px-4 py-3 active:bg-red-700">
        <Text class="text-center font-semibold text-white">Delete…</Text>
      </Button>
      <Text>{answer()}</Text>
    </View>
  );
}

mount(App);
```

An action sheet takes the same options with `presentation: "actionSheet"`:

```tsx
const choice = await AlertDialog.show({
  title: "Share the photo",
  presentation: "actionSheet",
  actions: [
    { label: "Copy link", id: "copy" },
    { label: "Save", id: "save" },
    { label: "Delete", role: "destructive", id: "delete" },
    { label: "Cancel", role: "cancel" },
  ],
});
```

Every label comes from the app, so an alert says what your app says, in its language.

#### AlertDialogOptions

| Field | Type | Default | Description |
|---|---|---|---|
| `title` | `string \| null` | none | Above the description. |
| `description` | `string \| null` | none | Below the title. |
| `actions` | `AlertDialogAction[]` | required | The actions, in order (at least one; at most three in an Android alert). |
| `presentation` | `"alert" \| "actionSheet"` | `"alert"` | An alert in the middle, or a sheet of actions from the bottom (a list dialog on Android). |
| `closeOnEscape` | `boolean \| null` | `false` for an alert, `true` for an action sheet | Escape and the Android back button close it without an action. |
| `closeOnBackdropPress` | `boolean \| null` | `false` for an alert, `true` for an action sheet | A press outside it closes it without an action. |

#### AlertDialogAction

| Field | Type | Default | Description |
|---|---|---|---|
| `label` | `string` | required | What it says. |
| `role` | `"default" \| "cancel" \| "destructive"` | `"default"` | What it means: the platform places and draws it by its role (a destructive action is red on iOS). |
| `id` | `string \| null` | its `label` | What `show` resolves with when it is chosen. |

> [!TIP]
> Because the main thread never waits for JS, the dialog is shown and answered natively and your code continues when
> the promise resolves. You can `await` an alert dialog inside any event handler, for example to confirm leaving a
> page with [`popOnBack`](#leaving-a-page).

### Alert dialogs on each platform

- **iOS**: `UIAlertController` lays out the actions. An action with `role: "cancel"` is placed where iOS puts cancel
  actions, and an action sheet shows it separately at its bottom. On iPad and Mac Catalyst the action sheet is a
  popover, with an untitled cancel action: a tap outside it closes it, and the promise resolves with `null`.
- **Android**: an `AlertDialog` shows up to three actions, mapped by position: the last action is the positive
  button, the one before it the negative button, and the one before that the neutral button. An action sheet is a list
  of the actions in a dialog, with a tap outside or the back button in place of its cancel action (the promise then resolves with `null`). The list dialog shows the
  `title`, or the `description` when the sheet has no title.
- **Windows, Linux, GPUI and the web**: NSH draws the alert or action sheet in the platform's dialog look.

## Keyboard

`Keyboard` hides the software keyboard, the `keyboardFrame()` accessor tells you how much of the screen it covers, and
`KeyboardAvoidingView` keeps content above it, moving with it.

### Keyboard.dismiss

```ts
Keyboard.dismiss(): void
```

Unfocuses the focused input, which hides the keyboard. With no input focused, it leaves everything as it is.

```tsx
import { Input, mount, signal, Text, View } from "nsh";
import { Button, Keyboard } from "nsh/base";

function App() {
  const name = signal("");
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48, gap: 12 }}>
      <Input
        value={name()}
        onChange={(v) => name.set(v)}
        placeholder="Name"
        class="rounded-lg border border-neutral-300 px-3 py-2 text-base"
      />
      <Button onPress={() => Keyboard.dismiss()} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
        <Text class="text-center font-semibold text-white">Done</Text>
      </Button>
    </View>
  );
}

mount(App);
```

> [!TIP]
> To hide the keyboard when the user drags a scroll view, set `keyboardDismissMode="on-drag"` or `"interactive"` on
> the [scroll view](../elements/scroll.md) or [List](../elements/list.md).

### Keyboard height

```ts
keyboardFrame: Accessor<{ height: number; duration: number }>
```

`keyboardFrame()` returns the height of the software keyboard over the NSH surface, in points (0 while it is hidden), and
the duration of the keyboard's show or hide animation, in seconds. It is reactive: an effect or JSX expression that
reads it runs again when the keyboard shows, hides or changes height.

```tsx
import { Input, View } from "nsh";
import { keyboardFrame, safeAreaInsets } from "nsh/base";

function MessageBar() {
  const insets = safeAreaInsets();
  return (
    <View
      style={{
        flexDirection: "row",
        padding: 8,
        // The home indicator's inset only matters while the keyboard is hidden.
        paddingBottom: 8 + (keyboardFrame().height > 0 ? 0 : insets().bottom),
      }}
    >
      <Input placeholder="Message" style={{ flex: 1 }} />
    </View>
  );
}
```

The height is the part of the keyboard that overlaps the NSH surface. When the window itself shrinks for the keyboard
(an Android activity that resizes), or when NSH is embedded in a view that ends above the keyboard, the height is 0.

> [!NOTE]
> To keep content above the keyboard, [KeyboardAvoidingView](#keyboardavoidingview) is usually simpler than reading
> `keyboardFrame()` yourself: it animates with the keyboard natively.

### KeyboardAvoidingView

`KeyboardAvoidingView` keeps its content above the software keyboard. When the keyboard shows or hides, the view makes
room by the keyboard's height, and the change animates with the keyboard. Use it for content outside scroll views,
such as a message bar at the bottom of a chat screen. Inputs inside a [scroll view](../elements/scroll.md) or a
[List](../elements/list.md) are scrolled into view natively.

```tsx
import { Input, mount, Scroll, signal, Text, View } from "nsh";
import { KeyboardAvoidingView } from "nsh/base";

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
          onChange={(v) => draft.set(v)}
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
keyboard height. When something is below it, such as a tab bar, subtract that with `offset` (a negative value) or
add to it with a positive value.

#### Behaviors

| `behavior` | How it makes room |
|---|---|
| `"padding"` (default) | Adds the keyboard's height to the view's bottom padding. The content above shrinks; the background still reaches the bottom of the screen. |
| `"height"` | Adds the keyboard's height as a bottom margin. The whole view, including its background, ends at the top of the keyboard. |

```tsx
<KeyboardAvoidingView behavior="height" offset={-49} style={{ flex: 1, padding: 16, gap: 12 }}>
  <Input placeholder="E-mail" keyboardType="email-address" class="rounded-lg border border-neutral-300 px-3 py-2" />
  <Button onPress={signIn} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
    <Text class="text-center font-semibold text-white">Sign in</Text>
  </Button>
</KeyboardAvoidingView>
```

Here a 49 pt tab bar below the view already covers part of the keyboard's height.

The padding or margin is added to what the view's own style sets, so your padding stays.

#### KeyboardAvoidingView props

| Prop | Type | Default | Description |
|---|---|---|---|
| `enabled` | `boolean` | `true` | Avoid the keyboard. While false, the view keeps its own layout. |
| `behavior` | `"padding" \| "height"` | `"padding"` | How the view makes room (see [Behaviors](#behaviors)). |
| `offset` | `number` | `0` | Added to the keyboard's height while it shows (for example, minus the height of a bar below the view). The total is at least 0. |
| `children` | `JSX.Element` | none | The content. |
| `class`, `classList`, `style`, `testID`, `onLayout`, `ref` | | | Applied to the outer view. |

### The keyboard on each platform

- **iOS**: the room animates with the keyboard's own duration and an ease-in-ease-out curve.
- **Android**: where the window shrinks for the keyboard (an activity that resizes), the content already ends above
  the keyboard, and KeyboardAvoidingView keeps its layout. Where the keyboard overlaps the view, it makes room as on
  iOS.
- **Desktops and Mac Catalyst** type with a hardware keyboard: `keyboardFrame().height` stays 0 and the view keeps its
  layout.

## Status bar

The status bar belongs to what is on screen, as in UIKit, where each view controller declares its
`preferredStatusBarStyle` and the topmost one decides. In NSH the pages and dialogs are those view controllers: a
`NavPage` or a `Dialog` declares the status bar it wants with `statusBar`, and the app declares its default in
`nsh.json`.

```tsx
import { NavPage } from "nsh/base";

function PhotoViewer() {
  return (
    <NavPage headerShown={false} statusBar={{ foreground: "light", hidden: true, animated: true }}>
      …
    </NavPage>
  );
}
```

`statusBar` takes these fields, each optional:

| Field | Type | Description |
|---|---|---|
| `foreground` | `"auto" \| "light" \| "dark"` | The color of the content (time, battery, signal): `"light"` for dark backgrounds, `"dark"` for light ones, `"auto"` the platform's for the current appearance. |
| `hidden` | `boolean` | Hides the status bar. |
| `animated` | `boolean` | Animates the change when the page or dialog comes and goes. |

The value is reactive: `statusBar={{ foreground: dark() ? "light" : "dark" }}` follows the signal.

### Which one applies

The page or dialog shown **most recently** decides, **field by field**: a page pushed onto a stack is over the pages
below it, and an open dialog is over the pages. A field it leaves out comes from the one below it, and in the end
from the app's default. When the page is popped or the dialog closes, the one below it applies again.

```tsx
<NavPage title="Inbox" statusBar={{ foreground: "dark" }}>…</NavPage>

// Pushed over it: hidden, with the inbox's dark content when it shows again.
<NavPage title="Photo" statusBar={{ hidden: true }}>…</NavPage>
```

### The app's default

`nsh.json`'s `"statusBar"` is the setting under every page and dialog, as Info.plist's `UIStatusBarStyle` and
`UIStatusBarHidden` are in a UIKit app:

```json
{
  "statusBar": { "foreground": "light", "hidden": false }
}
```

Both fields are optional. Without it the content is `"auto"` and the bar shows. Packaged iOS apps also show it on the
launch screen.

### The status bar on each platform

- **iOS**: hiding and showing animate with a fade. In an app [embedded](../docs/embedding-ios.md) in a native one,
  the host app's view controller decides how the status bar looks. The host applies NSH's setting by returning
  `NshAppView.preferredStatusBarStyle`, `NshAppView.prefersStatusBarHidden` and
  `NshAppView.preferredStatusBarUpdateAnimation` from its view controller.
- **Android**: apps draw edge to edge, behind the status bar (Android 15, API 35): the app's own background is the
  bar's, and `foreground` keeps its content readable on it.
- **Desktops and the web**: the window's title bar and the browser's bars are the system's and the browser's;
  `statusBar` and `nsh.json`'s `"statusBar"` are for the phones.

> [!IMPORTANT]
> When your `nsh.json` declares `"capabilities"`, the status bar changes only when they include
> `"statusBar": true`; other changes are refused and logged. See
> [Sandbox and capabilities](../docs/sandbox-and-capabilities.md).

```json
{
  "capabilities": {
    "statusBar": true
  }
}
```

## Related

- [Navigation guide](../docs/navigation.md).
- [Forms and keyboard](../docs/forms-and-keyboard.md).
- [Dialog](../elements/dialog.md) and [Input](../elements/input.md).
- [Device and environment](../docs/device-and-environment.md).
- [nsh/android](./nsh-android.md): `BackHandler`, `Toast.show` for short messages and other Android-only APIs.

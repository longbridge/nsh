---
title: Navigation
description: Native navigation stacks with createNavStack, NavStack and NavPage, headers on every platform, keeping the user on a page with popOnBack, dialogs, and the Android back button.
order: 41
group: ui
---

# Navigation

`nsh/base` provides a native navigation stack and native dialogs. A `NavStack` is a UINavigationController on iOS, with its header, swipe back and large titles, and a stack with a toolbar and predictive back on Android; on the desktops and the web, each page shows the header your app draws. You keep the routes in the stack's state; the stack renders each route as a page and keeps it mounted, with its state, while it is in the stack.

```tsx
import { mount, Scroll, Text, View } from "nsh";
import { createNavStack, NavPage, NavStack } from "nsh/base";

type Route = { name: "inbox" } | { name: "message"; id: number };

const nav = createNavStack<Route>([{ name: "inbox" }]);

// The page's header on the desktops and the web; iOS and Android show the system's, from `title`.
function Header(props: { title: string }) {
  return (
    <View class="h-12 flex-row items-center gap-3 border-b border-gray-200 px-4">
      <If when={nav.routes().length > 1}>
        <Text role="button" onPress={() => nav.pop()} class="text-blue-600 active:opacity-60">
          Back
        </Text>
      </If>
      <Text style={{ fontSize: 17, fontWeight: "600" }}>{props.title}</Text>
    </View>
  );
}

function Inbox() {
  return (
    <NavPage title="Inbox" header={<Header title="Inbox" />}>
      <Scroll style={{ flex: 1 }}>
        <Each of={[1, 2, 3, 4, 5]}>
          {(id) => (
            <View onPress={() => nav.push({ name: "message", id: id() })} class="p-4 active:bg-gray-100">
              <Text>Message {id()}</Text>
            </View>
          )}
        </Each>
      </Scroll>
    </NavPage>
  );
}

function Message(props: { id: number }) {
  return (
    <NavPage title={`Message ${props.id}`} header={<Header title={`Message ${props.id}`} />}>
      <View style={{ flex: 1, padding: 16 }}>
        <Text>The body of message {props.id}.</Text>
      </View>
    </NavPage>
  );
}

function App() {
  return (
    <NavStack state={nav} style={{ flex: 1 }}>
      {(route) => {
        switch (route.name) {
          case "inbox":
            return <Inbox />;
          case "message":
            return <Message id={route.id} />;
        }
      }}
    </NavStack>
  );
}

mount(App);
```

## The stack's state

`createNavStack(routes)` creates the stack's state from an array of routes, bottom to top. A route is any value you like (a string, an object, a discriminated union as above), and each push is a page of its own, the same route pushed twice included. Create the state once, at module level or in your root component, and import it wherever pages navigate.

| Member | Description |
|---|---|
| `routes()` | The routes, bottom to top (reactive). |
| `push(route)` | Pushes a page. |
| `pop(count?)` | Pops `count` pages (default 1); at least one stays. |
| `popToRoot()` | Pops back to the first page. |
| `replace(route)` | Replaces the top page. |
| `setRoutes(routes)` | Replaces the pages, e.g. after signing in; a route that stays (the same value) keeps its page and its state. |
| `remove(route)` | Removes the topmost page of `route`, wherever it is. |

To start several pages deep, pass more routes: `createNavStack<Route>([{ name: "inbox" }, { name: "message", id: 3 }])`.

When the user leaves a page natively (the back button in the header, a swipe back on iOS, Android's back button or gesture, the desktops' back keys and mouse button, the browser's back button), the stack removes that route from its state for you, so `routes()` always matches what is on screen.

A route carries what the page needs. The gallery app pushes routes that hold the page component itself, which keeps the route type small when there are many pages:

```ts
import { type JSX } from "nsh";
import { createNavStack } from "nsh/base";

export type Route = { name: "home" } | { name: "page"; page: () => JSX.Element };
export const nav = createNavStack<Route>([{ name: "home" }]);

// Elsewhere: nav.push({ name: "page", page: Settings });
```

## NavStack

| Prop | Type | Description |
|---|---|---|
| `state` | `NavStackState<R>` | The routes to show. |
| `children` | `(route: R, index: Accessor<number>) => JSX.Element` | Renders a route; return a `<NavPage>` for its header. |
| `style`, `class` | | The stack's own style (usually `flex: 1`). |

The render function runs once per route, when it is pushed; `route` is the plain value, `index()` its current position. What it returns becomes the page's content. Return a `<NavPage>` to set the header; anything else is wrapped in a page with default options.

Pages stay mounted while their route is in the stack: going back shows the previous page exactly as it was left, with its scroll position, inputs and signals. A page's [`cleanup`](./reactivity.md) handlers run when its route leaves the stack.

## NavPage and the header

`NavPage` wraps a page's content and carries its header. iOS and Android show their system header, set up by `title` and the `header*` props. The desktops and the web show the page's `header` slot, your own JSX, at the top of the page: as wide as the page, as tall as it lays out, with the content below it. Give a page both, as the example above does, and it looks at home everywhere. All props are reactive: change a signal and the header follows.

| Prop | Type | Default | Description |
|---|---|---|---|
| `header` | `JSX.Element` | | The desktops and the web: the page's header, drawn by the app (a title, a back button, toolbar buttons). |
| `title` | `string` | | iOS and Android: the system header's title. |
| `headerShown` | `boolean` | `true` | iOS and Android: the system header shows. |
| `headerTransparent` | `boolean` | `false` | iOS and Android: the content extends under the header. |
| `headerStyle` | `{ backgroundColor?, color?, tintColor? }` | | iOS and Android: the header's background, the title's color (`color`) and the buttons' (`tintColor`). |
| `headerClass` | `string` | | The same as classes: `bg-*`, `text-*` for the title, `tint-*` for the buttons; `headerStyle` wins over them. |
| `headerBackVisible` | `boolean` | `true` | iOS and Android: the back button shows. |
| `headerLeft` | `JSX.Element` | | iOS and Android: views at the start of the header (they replace the back button). |
| `headerRight` | `JSX.Element` | | iOS and Android: views at the end of the header. |
| `headerTitle` | `JSX.Element` | | iOS and Android: views replacing the title. |
| `headerLargeTitle` | `boolean` | `false` | iOS: a large title that shrinks into the bar as the content scrolls. |
| `headerBackTitle` | `string` | | The label of the back button that leads back to this page (iOS; Android's up arrow says it to screen readers). |
| `animation` | `"default" \| "slide" \| "fade" \| "none"` | `"default"` | How the page enters and leaves. |
| `statusBar` | `{ foreground?, hidden?, animated? }` | | iOS and Android: the status bar while the page is on top ([Status bar](../modules/nsh-base.md#status-bar)). |
| `popOnSwipe` | `boolean` | `true` | The interactive back gesture (iOS swipe back, Android predictive back) pops. The back buttons pop either way. |
| `popOnBack` | `boolean` | `true` | Every way back pops the page: the back buttons, Android's back, the desktops' back keys, the browser's back button and the swipe. |
| `onBackPress` | `() => void` | | The user pressed back on the page, whether or not `popOnBack` let it pop. |
| `onDismiss` | `() => void` | | The user popped the page; its route is already removed. |
| `style`, `class` | | | The page's content view. |

Header items are ordinary views, so they can be pressable and reactive:

```tsx
function Compose() {
  const text = signal("");
  const cancel = () => (
    <Text onPress={() => nav.pop()} style={{ fontSize: 17, color: "#0969da" }}>
      Cancel
    </Text>
  );
  const sendButton = () => (
    <Text
      onPress={() => send(text())}
      style={{ fontSize: 17, fontWeight: "600", color: text() ? "#0969da" : "#8c959f" }}
    >
      Send
    </Text>
  );
  return (
    <NavPage
      title="New message"
      headerLeft={cancel()}
      headerRight={sendButton()}
      header={
        <View class="h-12 flex-row items-center justify-between px-4">
          {cancel()}
          <Text style={{ fontSize: 17, fontWeight: "600" }}>New message</Text>
          {sendButton()}
        </View>
      }
    >
      <Input multiline value={text()} onChange={(t) => text.set(t)} style={{ flex: 1, padding: 16, fontSize: 17 }} />
    </NavPage>
  );
}
```

A colored system header:

```tsx
<NavPage title="Profile" headerStyle={{ backgroundColor: "#1a7f37", color: "#ffffff", tintColor: "#ffffff" }}>
  …
</NavPage>
```

A page that shows no header still has the system bars around it; pad its content with [`SafeAreaView`](../modules/nsh-base.md#safeareaview):

```tsx
<NavPage headerShown={false}>
  <SafeAreaView style={{ flex: 1, padding: 16 }}>…</SafeAreaView>
</NavPage>
```

### Safe areas under the header

Inside a page, `safeAreaInsets()` and `SafeAreaView` report the insets below the system header, not the whole surface's. With `headerTransparent`, the content starts at the top of the page and the top inset is the header's height, so `SafeAreaView` keeps text clear of it while a background image fills the whole page. With `headerLargeTitle`, put a scroll view directly in the page so the title can collapse as it scrolls. On the desktops and the web, the content starts below the page's `header`.

## Keeping the user on a page: popOnBack

To ask before discarding unsaved changes, set `popOnBack` to `false` while there is something to lose, handle `onBackPress`, and pop once the user agrees:

```tsx
import { Input, signal } from "nsh";
import { AlertDialog, NavPage } from "nsh/base";

function EditNote() {
  const text = signal("");
  const dirty = () => text().length > 0;
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
    <NavPage title="Edit" popOnBack={!dirty()} onBackPress={() => dirty() && confirm()}>
      <Input value={text()} onChange={(t) => text.set(t)} placeholder="A note" style={{ margin: 16, fontSize: 17 }} />
    </NavPage>
  );
}
```

With `popOnBack={false}`, the page stays on every way back (the back buttons, Android's back button and gesture, the desktops' back keys, the browser's back button, the swipe), and `onBackPress` tells you the user tried. `nav.pop()` from code pops as always.

> [!IMPORTANT]
> `popOnBack` is a prop, not a callback that returns true or false, because the main thread never waits for JS. When the user starts a swipe or presses back, the system asks right away whether the page may go, and NSH answers natively from the props it already has. Set `popOnBack` as soon as there is something to lose (as above, from the text), before the user tries to leave.

`popOnSwipe={false}` is the lighter option: it turns off only the interactive gesture (iOS swipe back, Android's predictive back animation), and the back buttons still pop.

## Dialogs

`Dialog` presents content over everything else, in its own native window layer: a presented view controller on iOS, a dialog window on Android, a layer over the whole window on the desktops and the web. Its look is yours: `class` and `style` style the popup that holds its children, and `backdropClass` or `backdropStyle` add a backdrop behind it. Render it where its state lives and control it with `open`:

```tsx
import { signal, Text, View } from "nsh";
import { Dialog } from "nsh/base";

function Files() {
  const open = signal(false);
  return (
    <View style={{ flex: 1 }}>
      <View onPress={() => open.set(true)} style={{ padding: 16 }}>
        <Text>Delete the file</Text>
      </View>
      <Dialog
        open={open()}
        animation="fade"
        onOpenChange={(next) => open.set(next)}
        backdropClass="bg-black/40"
        class="m-auto w-80 gap-3 rounded-2xl bg-white p-5"
      >
        <Text style={{ fontSize: 20, fontWeight: "700" }}>Delete the file?</Text>
        <View onPress={() => open.set(false)}>
          <Text style={{ color: "#cf222e" }}>Delete</Text>
        </View>
      </Dialog>
    </View>
  );
}
```

Escape, Android's back button, a press on the backdrop and a swipe down on a sheet close it natively (`closeOnEscape`, `closeOnBackdropPress` and `closeOnSwipe`, each `true` by default); `onOpenChange(false, reason)` then tells you, so set `open` to `false` there. To confirm first, set a flag to `false` and handle `onCancel`, which reports every attempt, then close the dialog yourself. On iOS, `presentation="pageSheet"` or `"formSheet"` presents a sheet that keeps what is behind visible.

See [Dialog](../elements/dialog.md) for every prop. Inside a dialog, `SafeAreaView` and `safeAreaInsets()` give the dialog's own insets. For system dialogs (an alert with actions, an action sheet) use [`AlertDialog`](../modules/nsh-base.md#alertdialog) from `nsh/base`.

## The Android back button

On Android, the back button (or gesture) is handled in this order:

1. The top dialog: it calls `onCancel`, and closes with `closeOnEscape` (the default).
2. Your `BackHandler` listeners from `nsh/android`, most recently added first.
3. The top stack: its top page calls `onBackPress`, and pops while `popOnBack` is `true`.
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

A tab bar is your own UI, drawn from primitives: a row of pressable views and a signal holding the selected tab; render the selected tab's content with a function child or [`<If>`](../elements/control-flow.md). To keep each tab's state while switching, give each tab its own `NavStack` and state and keep all of them mounted, hiding the inactive ones with `display: "none"`.

## Related

- [`nsh/base`](../modules/nsh-base.md): the `createNavStack`, `NavStack`, `NavPage`, `Dialog` and `SafeAreaView` reference.
- [The status bar](../modules/nsh-base.md#status-bar) of pages and dialogs.
- [`AlertDialog`](../modules/nsh-base.md#alertdialog).
- [`nsh/android`](../modules/nsh-android.md): `BackHandler`.

---
title: Platform-specific code
description: Writing one app for phones, desktops and browsers - platform-only props in ios={{}} / android={{}} / desktop={{}} / web={{}}, the nsh/ios and nsh/android modules, Platform.OS and Platform.select, and platform Tailwind variants.
order: 17
group: platforms
---

# Platform-specific code

One NSH bundle runs on iOS, Android, macOS, Windows and Linux (and in Mac Catalyst), and in browsers through the [web runtime](./web.md). Most of an app needs no platform checks at all: elements, styles, layout and modules behave the same everywhere, and the components you draw from them look the same everywhere. When something really is specific to one platform, NSH gives it a clear place, so platform code stays visible and contained:

| Where | For |
|---|---|
| [`ios={{…}}`, `android={{…}}`, `desktop={{…}}`, `web={{…}}`](#platform-only-props) | Props only one platform has. |
| [`nsh/ios`, `nsh/android`](#platform-modules) | APIs only one platform has. |
| [`Platform.OS`, `Platform.select`](#platformos-and-platformselect) | Choosing values or code per platform. |
| [`ios:` / `android:` / `web:` classes](#platform-variants-in-tailwind) | Styles per platform. |

## Platform-only props

Props that exist on one platform only are grouped in an object named after it. Each element accepts `ios`, `android`, `desktop` (macOS, Windows, Linux, and browsers in the desktop idiom) and `web` (browsers); on other platforms the object is ignored, and on its own platform only its keys are applied. A desktop browser applies both `desktop` and `web`.

```tsx
<View
  onPress={open}
  activeStyle={{ opacity: 0.6 }}
  android={{ ripple: { color: "rgba(0,0,0,0.12)" }, elevation: 4 }}
  style={{ padding: 16, borderRadius: 12, backgroundColor: "#ffffff" }}
>
  <Text>Open</Text>
</View>

<Input
  placeholder="Code"
  ios={{ textContentType: "oneTimeCode" }}
/>

<Dialog open={open()} ios={{ supportedOrientations: ["portrait"] }} android={{ statusBarTranslucent: true }}>
  …
</Dialog>
```

The objects are reactive like any prop; a key removed from the object is cleared.

What exists today:

| Prop | On | Description |
|---|---|---|
| `android={{ ripple }}` | every element | The material ripple while pressed: `{ color?, borderless?, radius?, foreground? }`. |
| `android={{ elevation }}` | every element | The material shadow, in points, from the view's shape. |
| `android={{ accessibilityLiveRegion }}` | every element | `"none"`, `"polite"` or `"assertive"`: TalkBack announces the view's changes. |
| `android={{ statusBarTranslucent }}` | `Dialog` | The dialog's content reaches under the status bar. |
| `ios={{ inputAccessoryViewID }}` | `<Input>` | The `InputAccessoryView` shown over the keyboard. |
| `ios={{ textContentType }}` | `<Input>` | iOS's names for autofill types. |
| `ios={{ supportedOrientations }}` | `Dialog` | The orientations it allows. |
| `ios={{ allowsBackForwardNavigationGestures }}` | `WebView` | Swipe back and forward in the page history. |
| `desktop={{ allowsBackForwardNavigationGestures }}` | `WebView` | macOS: two-finger trackpad swipes go back and forward in the page history; Windows: touchscreen swipes; Linux: touchpad swipes. |
| `desktop={{ windowDrag }}` | every element | Pressing it drags the window, as the system's title bar would (see [nsh/desktop](../modules/nsh-desktop.md#a-title-bar-of-your-own)). |

`desktop={{…}}` and `web={{…}}` are accepted everywhere. The types (`IOSProps`, `AndroidProps`, `DesktopProps` and their per-element forms) tell your editor what each element takes.

Something two or more platforms do is a top-level prop, applied where it means something: `NavPage`'s `title`, `headerStyle`, `headerLargeTitle` and `headerBackTitle` set up the system header of iOS and Android, its `header` slot is the header of the desktops and the web, and `Dialog`'s `presentation` picks iOS's sheets. `AlertDialog.show`'s `closeOnBackdropPress`, `pagingEnabled`, `refreshing`, `hitSlop` and the rest work on every platform.

## Platform modules

`nsh/ios` and `nsh/android` hold the APIs only one platform has, `nsh/desktop` those only desktops have. All can be imported on every platform, so one bundle runs everywhere: elsewhere their functions do nothing and their components render nothing. Each exports `isAvailable`, which is `true` only on its own platforms.

### nsh/android

| Export | Description |
|---|---|
| `Toast.show(message, duration?)` | A toast at the bottom of the screen; `"short"` (default) or `"long"`. |
| `BackHandler.addEventListener("hardwareBackPress", listener)` | Listens to the back button after dialogs, before stacks; return `true` when handled. Returns `{ remove() }`. |
| `BackHandler.exitApp()` | Leaves the app. |
| `isAvailable` | `true` on Android. |

```tsx
import { cleanup } from "nsh";
import { BackHandler, Toast } from "nsh/android";
import { Platform } from "nsh/device";
import { AlertDialog } from "nsh/base";

function saved() {
  if (Platform.OS === "android") Toast.show("Saved");
  else AlertDialog.show({ title: "Saved", actions: [{ label: "OK" }] });
}

function Player() {
  // Back leaves full screen before it leaves the page.
  const sub = BackHandler.addEventListener("hardwareBackPress", () => {
    if (!fullScreen()) return false;
    fullScreen.set(false);
    return true;
  });
  cleanup(() => sub.remove());
  // …
}
```

The back button is handled by the top dialog first, then `BackHandler` listeners (most recent first), then the top navigation stack, and the app exits when nothing handled it. See [Navigation](./navigation.md#the-android-back-button).

### nsh/ios

| Export | Description |
|---|---|
| `InputAccessoryView` | A bar over the keyboard of the inputs naming its `nativeID` in `ios={{ inputAccessoryViewID }}`. |
| `isAvailable` | `true` on iOS and Mac Catalyst. |

See [Forms and the keyboard](./forms-and-keyboard.md#a-bar-over-the-keyboard-ios-inputaccessoryview).

### nsh/desktop

| Export | Description |
|---|---|
| `Window` | The window: `setTitle`, `setTitleBar("hidden")` for a title bar of the app's own, `minimize`, `toggleMaximize`, `toggleFullScreen`, `close`. |
| `titleBar` | The hidden title bar's height and the room the system's window controls take (reactive). |
| `Menu` | `setApplicationMenu(items)` (the menu bar, with shortcuts) and `popup(items, at?)` (context menus). |
| `isAvailable` | `true` on macOS, Windows and Linux. |

See [nsh/desktop](../modules/nsh-desktop.md).

### The status bar

On both platforms the status bar belongs to the page or dialog on top, as in UIKit, where the topmost view controller decides it: [`NavPage` and `Dialog` take `statusBar`](../modules/nsh-base.md#status-bar) with `foreground` (`"auto"`, `"light"` for dark backgrounds, `"dark"` for light ones), `hidden` and `animated`. The page or dialog shown most recently wins, field by field, and the one below it applies again when it goes away. `nsh.json`'s `"statusBar"` is the app's default under them.

```tsx
import { View } from "nsh";
import { NavPage } from "nsh/base";

function PhotoViewer() {
  return (
    <NavPage headerShown={false} statusBar={{ foreground: "light", animated: true }}>
      <View style={{ flex: 1, backgroundColor: "#000000" }}>…</View>
    </NavPage>
  );
}
```

In an app embedded in a native one, the host app's view controller decides the status bar on iOS. An app that declares `"capabilities"` in `nsh.json` needs `"statusBar": true` to change it.

## Platform.OS and Platform.select

`Platform` from `nsh/device` tells you where the app runs: `Platform.OS` is `"ios"`, `"android"`, `"macos"`, `"windows"`, `"linux"` or `"web"` (Mac Catalyst is `"ios"`, with `Platform.isMacCatalyst`; the [macOS backend](./macos.md) is `"macos"`, the [Linux backend](./linux.md) `"linux"` and the [Windows backend](./windows.md) `"windows"`, and their elements apply their `desktop` props; [browsers](./web.md) are `"web"`, and apply `desktop` props too when the idiom is `"desktop"`). See [Device and environment](./device-and-environment.md#the-platform) for `Version`, `isPad` and the rest.

```tsx
import { Platform } from "nsh/device";

if (Platform.OS === "android") {
  // …
}
```

### Platform.select

`Platform.select(spec)` returns the value for this platform. It looks for the OS's own key, then `mobile` (iOS, Android, phone and tablet browsers) or `desktop` (macOS, Windows, Linux, desktop browsers), then `native` (every platform but the web, as in React Native), then `default`:

```tsx
const styles = {
  card: {
    borderRadius: 12,
    backgroundColor: "#ffffff",
    ...Platform.select({
      ios: { shadowColor: "#000000", shadowOpacity: 0.12, shadowRadius: 8, shadowOffset: { width: 0, height: 2 } },
      android: { borderWidth: 1, borderColor: "#e5e7eb" },
    }),
  },
};

const hint = Platform.select({ ios: "Swipe down to close", android: "Press back to close", default: "Close" });
```

It returns `undefined` when nothing matches and there is no `default`.

### Rendering different components

Choose components the same way, at module level when the choice never changes:

```tsx
const Spinner = Platform.select({ ios: IosSpinner, default: MaterialSpinner })!;
```

or inline with [`<If>`](../elements/control-flow.md):

```tsx
<If when={Platform.OS === "ios"} else={<Text>Android has no input accessories.</Text>}>
  <InputAccessoryView nativeID="bar">…</InputAccessoryView>
</If>
```

## Platform variants in Tailwind

The `ios:`, `android:` and `web:` variants apply classes on one platform:

```tsx
<View class="rounded-xl bg-white p-4 ios:shadow-md android:border android:border-zinc-200">…</View>
<Text class="text-base android:font-medium">Title</Text>
```

## Prefer shared behavior

Before reaching for a platform check, consider whether the difference matters to your users. Elements and styles already map to each platform's native behavior where it should differ (scroll physics, text rendering, keyboards, accessibility), while staying the same where users expect consistency (your app's own look). Platform checks are best kept for platform conventions users rely on: Android's back button and toasts, iOS's sheets and large titles, autofill names, Android's ripple.

## Related

- [`nsh/ios`](../modules/nsh-ios.md) and [`nsh/android`](../modules/nsh-android.md): the references.
- [`nsh/device`](../modules/nsh-device.md): `Platform`.
- [Native components](./native-components.md): using views your host app provides on each platform.
- [Tailwind](./tailwind.md): every variant.

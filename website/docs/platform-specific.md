---
title: Platform-specific code
description: Writing one app for phones and desktops - platform-only props in ios={{}} / android={{}} / desktop={{}}, the nsh/ios and nsh/android modules, Platform.OS and Platform.select, and platform Tailwind variants.
order: 17
group: platforms
---

# Platform-specific code

One NSH bundle runs on iOS, Android, macOS, Windows and Linux (and in Mac Catalyst). Most of an app needs no platform checks at all: elements, styles, layout, components and modules behave the same everywhere, and the components of `nsh/ui/mobile` are drawn by NSH rather than borrowed from each platform, so they look the same everywhere. When something really is specific to one platform, NSH gives it a clear place, so platform code stays visible and contained:

| Where | For |
|---|---|
| [`ios={{…}}`, `android={{…}}`, `desktop={{…}}`](#platform-only-props) | Props only one platform has. |
| [`nsh/ios`, `nsh/android`](#platform-modules) | APIs only one platform has. |
| [`Platform.OS`, `Platform.select`](#platformos-and-platformselect) | Choosing values or code per platform. |
| [`ios:` / `android:` classes](#platform-variants-in-tailwind) | Styles per platform. |

## Platform-only props

Props that exist on one platform only are grouped in an object named after it. Each element accepts `ios`, `android` and `desktop` (macOS, Windows, Linux); on other platforms the object is ignored, and on its own platform only its keys are applied.

```tsx
<View
  onPress={open}
  activeOpacity={0.6}
  android={{ ripple: { color: "rgba(0,0,0,0.12)" }, elevation: 4 }}
  style={{ padding: 16, borderRadius: 12, backgroundColor: "#ffffff" }}
>
  <Text>Open</Text>
</View>

<Input
  placeholder="Code"
  ios={{ textContentType: "oneTimeCode" }}
/>

<Modal visible={open()} ios={{ presentationStyle: "pageSheet" }} android={{ statusBarTranslucent: true }}>
  …
</Modal>
```

The objects are reactive like any prop; a key removed from the object is cleared.

What exists today:

| Prop | On | Description |
|---|---|---|
| `android={{ ripple }}` | every element | The material ripple while pressed: `{ color?, borderless?, radius?, foreground? }`. |
| `android={{ elevation }}` | every element | The material shadow, in points, from the view's shape. |
| `android={{ accessibilityLiveRegion }}` | every element | `"none"`, `"polite"` or `"assertive"`: TalkBack announces the view's changes. |
| `android={{ statusBarTranslucent }}` | `Modal` | The modal's content reaches under the status bar. |
| `android={{ backgroundColor }}` | `StatusBar` | The status bar's color (before Android 15's edge-to-edge). |
| `ios={{ inputAccessoryViewID }}` | `<Input>` | The `InputAccessoryView` shown over the keyboard. |
| `ios={{ textContentType }}` | `<Input>` | iOS's names for autofill types. |
| `ios={{ headerLargeTitle }}`, `ios={{ headerBackTitle }}` | `Screen` | A large title; the back button's label. |
| `ios={{ presentationStyle }}`, `ios={{ supportedOrientations }}` | `Modal` | `"pageSheet"` / `"formSheet"` sheets; allowed orientations. |
| `ios={{ allowsBackForwardNavigationGestures }}` | `WebView` | Swipe back and forward in the page history. |
| `desktop={{ headerLargeTitle }}`, `desktop={{ headerBackTitle }}` | `Screen` | A large title; the back button's label. |
| `desktop={{ presentationStyle }}` | `Modal` | `"pageSheet"` / `"formSheet"`: a card centered in the window. |
| `desktop={{ allowsBackForwardNavigationGestures }}` | `WebView` | macOS: two-finger trackpad swipes go back and forward in the page history; Windows: touchscreen swipes; Linux: touchpad swipes. |
| `desktop={{ windowDrag }}` | every element | Pressing it drags the window, as the system's title bar would (see [nsh/desktop](../modules/nsh-desktop.md#a-title-bar-of-your-own)). |

`desktop={{…}}` is accepted everywhere; the props above are the desktop-only ones so far. The types (`IOSProps`, `AndroidProps`, `DesktopProps` and their per-element forms) tell your editor what each element takes.

Cross-platform props whose effect differs slightly are not grouped: `Alert.alert`'s `cancelable` option exists on both (iOS alerts are simply never dismissed by a tap outside), and `pagingEnabled`, `refreshing`, `hitSlop` and the rest work on every platform.

## Platform modules

`nsh/ios` and `nsh/android` hold the APIs only one platform has, `nsh/desktop` those only desktops have. All can be imported on every platform, so one bundle runs everywhere: elsewhere their functions do nothing and their components render nothing. Each exports `isAvailable`, which is `true` only on its own platforms.

### nsh/android

| Export | Description |
|---|---|
| `Toast.show(message, duration?)` | A toast at the bottom of the screen; `"short"` (default) or `"long"`. |
| `BackHandler.addEventListener("hardwareBackPress", listener)` | Listens to the back button after modals, before stacks; return `true` when handled. Returns `{ remove() }`. |
| `BackHandler.exitApp()` | Leaves the app. |
| `setStatusBarBackgroundColor(color, animated?)` | The status bar's color under what `StatusBar` components set (before edge-to-edge, API 35). |
| `isAvailable` | `true` on Android. |

```tsx
import { cleanup } from "nsh";
import { BackHandler, Toast } from "nsh/android";
import { Platform } from "nsh/device";
import { Alert } from "nsh/ui/mobile";

function saved() {
  if (Platform.OS === "android") Toast.show("Saved");
  else Alert.alert("Saved");
}

function Player() {
  // Back leaves full screen before it leaves the screen.
  const sub = BackHandler.addEventListener("hardwareBackPress", () => {
    if (!fullScreen()) return false;
    fullScreen.set(false);
    return true;
  });
  cleanup(() => sub.remove());
  // …
}
```

The back button is handled by the top modal first, then `BackHandler` listeners (most recent first), then the top navigation stack, and the app exits when nothing handled it. See [Navigation](./navigation.md#the-android-back-button).

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

The status bar is set with [`StatusBar`](../components/status-bar.md) from `nsh/ui/mobile`, on both platforms: `barStyle` (`"default"`, `"light-content"`, `"dark-content"`), `hidden` and `animated`, with Android's color in `android={{ backgroundColor }}`. The most recently mounted `StatusBar` wins, prop by prop, so a screen can render one for itself and the previous settings come back when it goes away. `StatusBar.setBarStyle()` and `StatusBar.setHidden()` set the base values imperatively.

```tsx
import { View } from "nsh";
import { StatusBar } from "nsh/ui/mobile";

function PhotoViewer() {
  return (
    <View style={{ flex: 1, backgroundColor: "#000000" }}>
      <StatusBar barStyle="light-content" animated />
      …
    </View>
  );
}
```

In an app embedded in a native one, the host app's view controller decides the status bar on iOS. An app that declares `"capabilities"` in `nsh.json` needs `"statusBar": true` to change it.

## Platform.OS and Platform.select

`Platform` from `nsh/device` tells you where the app runs: `Platform.OS` is `"ios"`, `"android"`, `"macos"`, `"windows"` or `"linux"` (Mac Catalyst is `"ios"`, with `Platform.isMacCatalyst`; the [macOS backend](./macos.md) is `"macos"`, the [Linux backend](./linux.md) `"linux"` and the [Windows backend](./windows.md) `"windows"`, and their elements apply their `desktop` props). See [Device and environment](./device-and-environment.md#the-platform) for `Version`, `isPad` and the rest.

```tsx
import { Platform } from "nsh/device";

if (Platform.OS === "android") {
  // …
}
```

### Platform.select

`Platform.select(spec)` returns the value for this platform. It looks for the OS's own key, then `mobile` (iOS and Android) or `desktop` (macOS, Windows, Linux), then `native` (every platform), then `default`:

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

The `ios:` and `android:` variants apply classes on one platform:

```tsx
<View class="rounded-xl bg-white p-4 ios:shadow-md android:border android:border-zinc-200">…</View>
<Text class="text-base android:font-medium">Title</Text>
```

## Prefer shared behavior

Before reaching for a platform check, consider whether the difference matters to your users. Elements and styles already map to each platform's native behavior where it should differ (scroll physics, text rendering, keyboards, accessibility), while staying the same where users expect consistency (your app's own look). Platform checks are best kept for platform conventions users rely on: Android's back button and toasts, iOS's sheets and large titles, autofill names, ripple versus opacity feedback.

## Related

- [`nsh/ios`](../modules/nsh-ios.md) and [`nsh/android`](../modules/nsh-android.md): the references.
- [`nsh/device`](../modules/nsh-device.md): `Platform`.
- [Native components](./native-components.md): using views your host app provides on each platform.
- [Tailwind](./tailwind.md): every variant.

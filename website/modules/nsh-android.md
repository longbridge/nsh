---
title: nsh/android
description: "Android-only APIs: isAvailable, Toast, BackHandler and setStatusBarBackgroundColor."
order: 12
---

# nsh/android

`nsh/android` holds APIs that only Android has. The module can be imported on every platform, so one bundle runs everywhere: elsewhere its functions do nothing, and `isAvailable` tells you whether you are on Android.

```ts
import { BackHandler, isAvailable, setStatusBarBackgroundColor, Toast } from "nsh/android";
```

Android-only *props* of elements and components are not here: they go in the `android={{…}}` prop (for example `<View android={{ elevation: 4, ripple: … }} />`). See [Platform-specific code](../docs/platform-specific.md).

## isAvailable

```ts
const isAvailable: boolean;
```

`true` on Android; `false` elsewhere.

## Toast

```ts
const Toast: {
  show(message: string, duration?: "short" | "long" | null): void;
};
```

Shows a short message at the bottom of the screen (Android's toast). `duration` is `"short"` by default.

```ts
import { Platform } from "nsh/device";
import { Toast } from "nsh/android";
import { Alert } from "nsh/ui/mobile";

const notify = (message: string) =>
  Platform.OS === "android" ? Toast.show(message) : Alert.alert("Done", message);
```

## BackHandler

```ts
const BackHandler: {
  addEventListener(event: "hardwareBackPress", listener: () => boolean | void): { remove(): void };
  exitApp(): void;
};
```

The system back button and back gesture. Modals and navigation stacks handle the back press first (a modal closes, a stack pops); only when none of them takes it do `BackHandler`'s listeners run.

| Member | Description |
|---|---|
| `addEventListener("hardwareBackPress", listener)` | `listener` returns `true` when it handled the press. The most recently added listener runs first. If no listener handles it, the app exits. Returns a subscription to `remove()`. |
| `exitApp()` | Leaves the app. |

```tsx
import { cleanup, signal } from "nsh";
import { BackHandler } from "nsh/android";
import { Alert } from "nsh/ui/mobile";

function Editor() {
  const dirty = signal(false);
  const sub = BackHandler.addEventListener("hardwareBackPress", () => {
    if (!dirty()) return false; // let the app go back or exit
    Alert.alert("Discard changes?", undefined, [{ text: "Keep" }, { text: "Discard", style: "destructive" }]).then(
      (i) => i === 1 && BackHandler.exitApp(),
    );
    return true;
  });
  cleanup(() => sub.remove());
  // ...
}
```

> [!NOTE]
> The listener runs after the system has delivered the press: the main thread never waits for JS. While at least one listener is registered, NSH takes back presses that modals and stacks leave, and exits the app itself when no listener returns `true`.

## setStatusBarBackgroundColor

```ts
function setStatusBarBackgroundColor(backgroundColor: string, animated?: boolean | null): void;
```

Sets the status bar's color, below what rendered `<StatusBar android={{ backgroundColor }}>` components set (the last mounted `StatusBar` wins over this base value). It has an effect on Android versions before edge-to-edge became mandatory (API 35); from API 35 the status bar is transparent and your content draws behind it.

```ts
setStatusBarBackgroundColor("#1e293b", true);
```

See [`StatusBar`](../components/status-bar.md) for the component.

---
title: nsh/android
description: "Android-only APIs: isAvailable, Toast and BackHandler."
order: 12
---

# nsh/android

`nsh/android` holds APIs that only Android has. Rust code calls their counterparts in `nsh_core::ui::prelude` (`toast`, `on_back_press`) and checks `os() == Os::Android`; TSX imports the module, which can be imported on every platform, so one bundle runs everywhere: elsewhere its functions do nothing, and `isAvailable` tells you whether you are on Android.

```rust tabs
use nsh_core::{platform::Os, ui::prelude::*}; // os, toast, ToastDuration, on_back_press
```

```tsx tabs
import { BackHandler, isAvailable, Toast } from "nsh/android";
```

Android-only *props* of elements and components are not here: they go in the `android={{…}}` prop in TSX (for example `<View android={{ elevation: 4, ripple: … }} />`). See [Platform-specific code](../docs/platform-specific.md).

## isAvailable

```ts
const isAvailable: boolean;
```

`true` on Android; `false` elsewhere. Rust: `os() == Os::Android`.

## Toast

```ts
const Toast: {
  show(message: string, duration?: "short" | "long" | null): void;
};
```

Shows a short message at the bottom of the screen (Android's toast). `duration` is `"short"` by default. Rust: `toast(message, ToastDuration::Short)` (or `ToastDuration::Long`), which does nothing off Android.

```rust tabs
use nsh_base::alert_dialog;
use nsh_core::{platform::Os, ui::prelude::*};

fn notify(message: &str) {
    if os() == Os::Android {
        toast(message, ToastDuration::Short);
    } else {
        let message = message.to_string();
        spawn(async move {
            let _ = alert_dialog().title("Done").description(message).action("OK").show().await;
        });
    }
}
```

```tsx tabs
import { platform } from "nsh/device";
import { Toast } from "nsh/android";
import { AlertDialog } from "nsh/base";

const notify = (message: string) =>
  platform.os === "android"
    ? Toast.show(message)
    : AlertDialog.show({ title: "Done", description: message, actions: [{ label: "OK" }] });
```

## BackHandler

```ts
const BackHandler: {
  onBackPress(listener: () => boolean | void): () => void;
  exitApp(): void;
};
```

The system back button and back gesture. Dialogs handle the back press first (a dialog closes); then `BackHandler`'s listeners run, the most recently added first; when none of them handles it, the top navigation stack pops, and with no page to go back to the app exits.

| Member | Rust | Description |
|---|---|---|
| `onBackPress(listener)` | `on_back_press(listener)` | `listener` returns `true` when it handled the press. Returns a function that stops listening; a listener added while a component renders (or an effect runs) stops with it, in Rust and TSX alike. |
| `exitApp()` | the listener returns `false` | Leaves the app. A Rust listener that returns `false` lets the press go on, so the stack goes back or the app exits. |

```rust tabs
use nsh_base::alert_dialog;
use nsh_core::ui::prelude::*;

#[component]
fn Editor() -> impl IntoElement {
    let dirty = signal(false);
    on_back_press(move || {
        if !dirty.get_untracked() {
            return false; // let the app go back or exit
        }
        spawn(async move {
            let answer = alert_dialog().title("Discard changes?").cancel("Keep").destructive("Discard").show().await;
            if answer == Ok(Some("Discard".into())) {
                dirty.set(false); // the next back press leaves
            }
        });
        true
    });
    // ...
}
```

```tsx tabs
import { signal } from "nsh";
import { BackHandler } from "nsh/android";
import { AlertDialog } from "nsh/base";

function Editor() {
  const dirty = signal(false);
  // Stops listening when Editor goes.
  BackHandler.onBackPress(() => {
    if (!dirty()) return false; // let the app go back or exit
    AlertDialog.show({
      title: "Discard changes?",
      actions: [{ label: "Keep", role: "cancel" }, { label: "Discard", role: "destructive", id: "discard" }],
    }).then((id) => id === "discard" && BackHandler.exitApp());
    return true;
  });
  // ...
}
```

> [!NOTE]
> The listener runs after the system has delivered the press: the main thread never waits for the app's code. While at least one listener is registered, NSH takes back presses that dialogs leave, and goes back in the top stack or exits the app itself when no listener returns `true`.

The status bar is set by the `statusBar` of [pages and dialogs](./nsh-base.md#status-bar) from `nsh/base`, on iOS and Android alike.

---
title: nsh/android
description: Android 专有 API：isAvailable、Toast 和 BackHandler。
order: 12
---

# nsh/android

`nsh/android` 包含只有 Android 才有的 API。Rust 代码调用 `nsh_core::ui::prelude` 中对应的函数（`toast`、`on_back_press`），并用 `os() == Os::Android` 判断平台；TSX 导入这个模块，它在所有平台上都可以导入，因此同一个 bundle 可以在所有平台运行：在其他平台上它的函数什么也不做，`isAvailable` 告诉你当前是否在 Android 上。

```rust tabs
use nsh_core::{platform::Os, ui::prelude::*}; // os、toast、ToastDuration、on_back_press
```

```tsx tabs
import { BackHandler, isAvailable, Toast } from "nsh/android";
```

元素和组件的 Android 专有*属性*不在这里：在 TSX 中它们放在 `android={{…}}` 属性中（例如 `<View android={{ elevation: 4, ripple: … }} />`）。见[平台专有代码](../docs/platform-specific.md)。

## isAvailable

```ts
const isAvailable: boolean;
```

在 Android 上为 `true`，其他平台为 `false`。Rust：`os() == Os::Android`。

## Toast

```ts
const Toast: {
  show(message: string, duration?: "short" | "long" | null): void;
};
```

在屏幕底部显示一条简短消息（Android 的 toast）。`duration` 默认为 `"short"`。Rust：`toast(message, ToastDuration::Short)`（或 `ToastDuration::Long`），在其他平台上什么也不做。

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
import { Platform } from "nsh/device";
import { Toast } from "nsh/android";
import { AlertDialog } from "nsh/base";

const notify = (message: string) =>
  Platform.OS === "android"
    ? Toast.show(message)
    : AlertDialog.show({ title: "Done", description: message, actions: [{ label: "OK" }] });
```

## BackHandler

```ts
const BackHandler: {
  addEventListener(event: "hardwareBackPress", listener: () => boolean | void): { remove(): void };
  exitApp(): void;
};
```

系统返回键和返回手势。对话框和导航栈先处理返回操作（关闭对话框、导航栈出栈）；只有它们都不处理时，`BackHandler` 的监听器才会运行。

| 成员 | Rust | 说明 |
|---|---|---|
| `addEventListener("hardwareBackPress", listener)` | `on_back_press(listener)` | `listener` 处理了这次返回时返回 `true`。最后添加的监听器最先运行。没有监听器处理时，应用退出。返回一个可 `remove()` 的订阅；在 Rust 中，监听器与添加它的组件同生命周期。 |
| `exitApp()` | 监听器返回 `false` | 退出应用。Rust 监听器返回 `false` 时，返回操作继续传递，由导航栈返回或应用退出。 |

```rust tabs
use nsh_base::alert_dialog;
use nsh_core::ui::prelude::*;

#[component]
fn Editor() -> impl IntoElement {
    let dirty = signal(false);
    on_back_press(move || {
        if !dirty.get_untracked() {
            return false; // 交给应用返回或退出
        }
        spawn(async move {
            let answer = alert_dialog().title("Discard changes?").cancel("Keep").destructive("Discard").show().await;
            if answer == Ok(Some("Discard".into())) {
                dirty.set(false); // 下一次返回即离开
            }
        });
        true
    });
    // ...
}
```

```tsx tabs
import { cleanup, signal } from "nsh";
import { BackHandler } from "nsh/android";
import { AlertDialog } from "nsh/base";

function Editor() {
  const dirty = signal(false);
  const sub = BackHandler.addEventListener("hardwareBackPress", () => {
    if (!dirty()) return false; // 交给应用返回或退出
    AlertDialog.show({
      title: "Discard changes?",
      actions: [{ label: "Keep", role: "cancel" }, { label: "Discard", role: "destructive", id: "discard" }],
    }).then((id) => id === "discard" && BackHandler.exitApp());
    return true;
  });
  cleanup(() => sub.remove());
  // ...
}
```

> [!NOTE]
> 监听器在系统已经派发了返回操作之后才运行：主线程从不等待应用的代码。只要注册了至少一个监听器，对话框和导航栈不处理的返回操作就由 NSH 接管；没有监听器返回 `true` 时，由 NSH 自己退出应用。

状态栏在 iOS 和 Android 上都由 `nsh/base` 中[页面和对话框](./nsh-base.md#status-bar)的 `statusBar` 设置。

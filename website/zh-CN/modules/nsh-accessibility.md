---
title: nsh/accessibility
description: 屏幕阅读器和减弱动态效果设置、无障碍播报与无障碍焦点。
order: 10
---

# nsh/accessibility

无障碍 API 读取用户的无障碍设置（屏幕阅读器是否在运行、是否要求减弱动态效果），并与屏幕阅读器交互。Rust 代码调用 `nsh_core::ui::prelude` 中的 `screen_reader()`、`reduce_motion()` 和 `announce_for_accessibility()`，用 `ElementRef` 移动焦点；TSX 从 `nsh/accessibility` 导入它们。元素的无障碍属性（`accessibilityLabel`、`role` 等）见[元素](../elements/index.md)；教程见[无障碍](../docs/accessibility.md)。

```rust tabs
use nsh_core::ui::prelude::*; // screen_reader、reduce_motion、announce_for_accessibility、element_ref
```

```tsx tabs
import { AccessibilityInfo, reduceMotion, screenReaderEnabled } from "nsh/accessibility";
```

## screenReaderEnabled

```ts
const screenReaderEnabled: Accessor<boolean>;
```

屏幕阅读器（VoiceOver、TalkBack）是否正在运行。响应式：用户开启或关闭屏幕阅读器时，读取它的地方会更新。Rust：`screen_reader() -> bool`，同样是响应式的。

```rust tabs
when(move || !screen_reader(), || text("Swipe for the next photo"))
    .otherwise(move || text("Next photo").on_press(next))
```

```tsx tabs
<If when={!screenReaderEnabled()} else={<Text onPress={next}>Next photo</Text>}>
  <Text>Swipe for the next photo</Text>
</If>
```

## reduceMotion

```ts
const reduceMotion: Accessor<boolean>;
```

用户是否要求减少动画（iOS 的“减弱动态效果”，Android 的“移除动画”）。响应式。Rust：`reduce_motion() -> bool`。Tailwind 的 `motion-reduce:` 变体也会跟随它。

```rust tabs
let open = move || {
    if reduce_motion() {
        panel.set(0.0);
    } else {
        let _ = panel.spring(0.0, Spring::default());
    }
};
```

```tsx tabs
const open = () => {
  if (reduceMotion()) panel.set(0);
  else panel.spring(0);
};
```

## AccessibilityInfo

```ts
const AccessibilityInfo: {
  announceForAccessibility(message: string): void;
  setAccessibilityFocus(node: YNode): void;
  isScreenReaderEnabled(): Promise<boolean>;
  isReduceMotionEnabled(): Promise<boolean>;
  addEventListener(
    event: "screenReaderChanged" | "reduceMotionChanged",
    listener: (enabled: boolean) => void,
  ): { remove(): void };
};
```

系统的无障碍设置，可以读取一次，也可以用监听器跟踪变化。`screenReaderEnabled()` 和 `reduceMotion()` 是它们的响应式版本，在组件中优先使用它们。

| 成员 | Rust | 说明 |
|---|---|---|
| `announceForAccessibility(message)` | `announce_for_accessibility(message)` | 让屏幕阅读器读出 `message`（VoiceOver 播报、TalkBack）。适用于焦点没有移动就出现的结果，比如“已保存”。 |
| `setAccessibilityFocus(node)` | `ElementRef::set_accessibility_focus()` | 把屏幕阅读器的焦点移到 `node`（某个元素的 `ref`）。 |
| `isScreenReaderEnabled()` | `screen_reader()` | resolve 为屏幕阅读器是否在运行。 |
| `isReduceMotionEnabled()` | `reduce_motion()` | resolve 为是否开启了减弱动态效果。 |
| `addEventListener(event, listener)` | 读取 `screen_reader()` 或 `reduce_motion()` 的 `effect` | 设置变化时以新值调用 `listener`；返回一个可 `remove()` 的订阅。 |

```rust tabs
use nsh_core::ui::prelude::*;

#[component]
fn Saved() -> impl IntoElement {
    let note = element_ref();
    let save = move || {
        announce_for_accessibility("Saved");
        note.set_accessibility_focus();
    };
    // 设置变化时再次运行，直到组件移除。
    effect(|| log::info!("screen reader {}", screen_reader()));
    view()
        .gap_2()
        .child(text("Save").role(AccessibilityRole::Button).on_press(save))
        .child(text("Your changes are kept on this device.").element_ref(&note))
}
```

```tsx tabs
import { cleanup, Text, View, type YNode } from "nsh";
import { AccessibilityInfo } from "nsh/accessibility";

function Saved() {
  let note!: YNode;
  const save = () => {
    AccessibilityInfo.announceForAccessibility("Saved");
    AccessibilityInfo.setAccessibilityFocus(note);
  };
  const sub = AccessibilityInfo.addEventListener("screenReaderChanged", (on) => console.log("screen reader", on));
  cleanup(() => sub.remove());
  return (
    <View style={{ gap: 8 }}>
      <Text onPress={save} role="button">Save</Text>
      <Text ref={note}>Your changes are kept on this device.</Text>
    </View>
  );
}
```

---
title: KeyboardAvoidingView
description: KeyboardAvoidingView 组件、Keyboard.dismiss 与键盘高度：让内容保持在软键盘上方，并随键盘移动。
order: 24
group: base
demo: el-keyboard-avoiding-view
---

# KeyboardAvoidingView

`KeyboardAvoidingView` 让它的内容保持在软键盘上方。键盘显示或隐藏时，视图按键盘的高度让出空间，变化在原生侧随键盘一起以动画呈现。它用于滚动视图之外的内容，比如聊天界面底部的消息栏；[滚动视图](./scroll.md)或 [List](./list.md) 中的输入框会在原生侧被滚动到可见位置。与它一起的还有隐藏键盘的 `Keyboard.dismiss()`，以及供你自己布局使用的键盘高度。

```rust tabs
use nsh_base::keyboard_avoiding_view;
use nsh_core::ui::prelude::*;

fn chat() -> impl IntoElement {
    let draft = signal(String::new());
    keyboard_avoiding_view()
        .flex_1()
        .bg(white())
        .child(scroll().flex_1().child(text("Messages…").p_4()))
        .child(
            view()
                .flex_row()
                .gap_2()
                .p_2()
                .border_t_1()
                .border_color(rgb(0xe5e5e5))
                .child(
                    input()
                        .value(draft)
                        .on_change(move |v: &str| draft.set(v.to_owned()))
                        .placeholder("Message")
                        .flex_1()
                        .p_2(),
                ),
        )
}
```

```tsx tabs
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

视图假定自己延伸到屏幕底部，就像一个页面的内容那样：它按完整的键盘高度让出空间。下面还有其他内容时，比如标签栏，用 `offset` 减去它（负值），或用正值增加。

<a id="props"></a>

## 属性

| 属性 | Rust | 类型 | 默认值 | 说明 |
|---|---|---|---|---|
| `enabled` | `.enabled(..)` | `boolean` | `true` | 避让键盘。为 false 时，视图保持自己的布局。 |
| `behavior` | `.behavior(AvoidBehavior::Height)` | `"padding" \| "height"` | `"padding"` | 视图让出空间的方式（见[让位方式](#behaviors)）。 |
| `offset` | `.offset(px(..))` | `number` | `0` | 键盘显示时加到键盘高度上的值（比如减去视图下方一个栏的高度）。总和至少为 0。 |
| `children` | `.child(..)` | `JSX.Element` | 无 | 内容。 |
| `class`、`classList`、`style`、`testID`、`onLayout`、`ref` | 样式方法、`.test_id(..)` | | | 应用到外层视图上。 |

<a id="behaviors"></a>

## 让位方式

| `behavior` | 如何让出空间 |
|---|---|
| `"padding"`（默认） | 把键盘高度加到视图的底部内边距上。上方的内容缩小；背景仍然延伸到屏幕底部。 |
| `"height"` | 把键盘高度作为底部外边距。整个视图（包括背景）止于键盘顶部。 |

内边距或外边距会加到视图自己的样式上，所以你的内边距保持不变。

```rust tabs
use nsh_base::{AvoidBehavior, keyboard_avoiding_view};
use nsh_core::ui::prelude::*;

fn sign_in() -> impl IntoElement {
    keyboard_avoiding_view()
        .behavior(AvoidBehavior::Height)
        .offset(px(-49.0))
        .flex_1()
        .p_4()
        .gap_3()
        .child(input().placeholder("E-mail").keyboard_type(KeyboardType::Email))
}
```

```tsx tabs
<KeyboardAvoidingView behavior="height" offset={-49} style={{ flex: 1, padding: 16, gap: 12 }}>
  <Input placeholder="E-mail" keyboardType="email-address" class="rounded-lg border border-neutral-300 px-3 py-2" />
</KeyboardAvoidingView>
```

这里视图下方 49 pt 的标签栏已经覆盖了键盘高度的一部分。

<a id="hiding-the-keyboard"></a>

## 隐藏键盘

`Keyboard.dismiss()` 让当前获得焦点的输入框失去焦点，从而隐藏键盘。没有输入框获得焦点时，什么也不改变。Rust 组件调用 `dismiss_keyboard()`。

```rust tabs
use nsh_base::button;
use nsh_core::ui::prelude::*;

fn done() -> impl IntoElement {
    button()
        .on_press(|| dismiss_keyboard())
        .rounded_lg()
        .bg(rgb(0x2563eb))
        .px_4()
        .py_3()
        .child(text("Done").text_color(white()))
}
```

```tsx tabs
import { Text } from "nsh";
import { Button, Keyboard } from "nsh/base";

function Done() {
  return (
    <Button onPress={() => Keyboard.dismiss()} class="rounded-lg bg-blue-600 px-4 py-3 active:bg-blue-700">
      <Text class="text-center font-semibold text-white">Done</Text>
    </Button>
  );
}
```

> [!TIP]
> 要在用户拖动滚动视图时隐藏键盘，在[滚动视图](./scroll.md)或 [List](./list.md) 上设置 `keyboardDismissMode="on-drag"` 或 `"interactive"`。

<a id="the-keyboards-height"></a>

## 键盘高度

| TSX | Rust | 返回 |
|---|---|---|
| `keyboardFrame()` | `keyboard_height()`、`keyboard_duration()` | 软键盘覆盖在 NSH 界面上的高度，单位为点（隐藏时为 0），以及键盘显示或隐藏动画的时长，单位为秒。 |

两者都是响应式的：读取它们的 effect、JSX 表达式或样式闭包会在键盘显示、隐藏或改变高度时重新运行。

```rust tabs
use nsh_core::ui::prelude::*;

fn message_bar() -> impl IntoElement {
    view()
        .flex_row()
        .p_2()
        .pb(move || {
            // The home indicator's inset only matters while the keyboard is hidden.
            let home = if keyboard_height() > Points::ZERO {
                Points::ZERO
            } else {
                safe_area().bottom
            };
            px(8.0) + home
        })
        .child(input().placeholder("Message").flex_1())
}
```

```tsx tabs
import { Input, View } from "nsh";
import { keyboardFrame, safeArea } from "nsh/base";

function MessageBar() {
  const insets = safeArea();
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

高度是键盘与 NSH 界面重叠的部分。窗口本身为键盘缩小时（会调整大小的 Android activity），或 NSH 嵌入在一个止于键盘上方的视图中时，高度为 0。

> [!NOTE]
> 要让内容保持在键盘上方，KeyboardAvoidingView 通常比自己读取高度更简单：它在原生侧随键盘一起动画。

<a id="platform-notes"></a>

## 平台说明

- **iOS**：让出的空间以键盘自己的时长和 ease-in-ease-out 曲线动画呈现。
- **Android**：窗口为键盘缩小时（会调整大小的 activity），内容本来就止于键盘上方，KeyboardAvoidingView 保持它的布局。键盘覆盖视图时，它与 iOS 一样让出空间。
- **桌面平台和 Mac Catalyst** 使用硬件键盘输入：键盘高度保持为 0，视图保持它的布局。

焦点顺序、回车键和输入辅助视图见[表单与键盘](../docs/forms-and-keyboard.md)。

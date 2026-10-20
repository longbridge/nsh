---
title: Keyboard
description: 收起软键盘，并以响应式方式读取键盘的高度和动画时长。
order: 15
---

# Keyboard

`nsh/ui/mobile` 为软键盘提供了两个导出：用于收起键盘的 `Keyboard` 模块，以及告诉你键盘遮挡了多少屏幕的 `keyboard()` 访问器。

```tsx
import { Keyboard, keyboard } from "nsh/ui/mobile";
```

## Keyboard.dismiss

```ts
Keyboard.dismiss(): void
```

让当前获得焦点的输入框失去焦点，从而收起键盘。没有输入框获得焦点时不做任何事。

```tsx
import { mount, signal, View } from "nsh";
import { Button, Keyboard, TextField } from "nsh/ui/mobile";

function App() {
  const name = signal("");
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48, gap: 12 }}>
      <TextField label="Name" value={name()} onChange={(v) => name.set(v)} />
      <Button title="Done" onPress={() => Keyboard.dismiss()} />
    </View>
  );
}

mount(App);
```

> [!TIP]
> 如果想在用户拖动滚动视图时收起键盘，在[滚动视图](../elements/scroll.md)或 [List](../elements/list.md) 上设置 `keyboardDismissMode="on-drag"` 或 `"interactive"`，不必调用 `Keyboard.dismiss()`。

## keyboard()

```ts
keyboard: Accessor<{ height: number; duration: number }>
```

`keyboard()` 返回软键盘覆盖在 NSH 界面上的高度（单位为 pt，键盘隐藏时为 0），以及键盘弹出或收起动画的时长（单位为秒）。它是响应式的：读取它的 effect 或 JSX 表达式会在键盘弹出、收起或高度变化时重新运行。

```tsx
import { Input, View } from "nsh";
import { keyboard, useSafeAreaInsets } from "nsh/ui/mobile";

function MessageBar() {
  const insets = useSafeAreaInsets();
  return (
    <View
      style={{
        flexDirection: "row",
        padding: 8,
        // The home indicator's inset only matters while the keyboard is hidden.
        paddingBottom: 8 + (keyboard().height > 0 ? 0 : insets().bottom),
      }}
    >
      <Input placeholder="Message" style={{ flex: 1 }} />
    </View>
  );
}
```

这个高度是键盘与 NSH 界面重叠的部分。如果窗口本身会为键盘缩小（会调整窗口大小的 Android Activity），或者 NSH 嵌入的视图本来就在键盘上方结束，高度为 0。

> [!NOTE]
> 要让内容保持在键盘上方，使用 [KeyboardAvoidingView](./keyboard-avoiding-view.md) 通常比自己读取 `keyboard()` 更简单：它会以原生方式随键盘同步动画。

## 相关内容

- [KeyboardAvoidingView](./keyboard-avoiding-view.md)。
- [TextField](./text-field.md) 与 [Input](../elements/input.md)。
- [表单与键盘](../docs/forms-and-keyboard.md)。

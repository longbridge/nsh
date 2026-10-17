---
title: KeyboardAvoidingView
description: 让滚动视图之外的内容（例如消息输入栏）始终位于软键盘上方，并随键盘一起移动。
order: 12
---

# KeyboardAvoidingView

`KeyboardAvoidingView` 让其内容保持在软键盘上方。键盘弹出或收起时，视图会让出与键盘等高的空间，并与键盘同步执行动画。它适用于**不在**滚动视图中的内容，例如聊天界面底部的消息输入栏。

```tsx
import { KeyboardAvoidingView } from "nsh/ui/mobile";
```

> [!TIP]
> [滚动视图](../elements/scroll.md)或 [List](../elements/list.md) 中的输入框不需要它：NSH 会以原生方式把获得焦点的输入框滚动到可见区域。

## 用法

```tsx
import { Input, mount, Scroll, signal, Text, View } from "nsh";
import { KeyboardAvoidingView } from "nsh/ui/mobile";

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
          onChangeText={(v) => draft.set(v)}
          placeholder="Message"
          style={{ flex: 1, fontSize: 16, padding: 8 }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}

mount(App);
```

该视图假定自己延伸到屏幕底部（屏幕内容通常如此），因此会让出完整的键盘高度。如果它下方还有其他内容（例如标签栏），可以用 `keyboardVerticalOffset` 传入负值减去这部分高度，或传入正值增加额外空间。

<a id="behaviors"></a>

## 让位方式

| `behavior` | 如何让出空间 |
|---|---|
| `"padding"`（默认） | 把键盘高度加到视图的底部内边距上。上方内容随之缩小，背景仍延伸到屏幕底部。 |
| `"height"` | 把键盘高度作为底部外边距。整个视图（包括背景）在键盘顶部截止。 |
| `"position"` | 把内部的内容视图向上移动键盘的高度，不改变其尺寸。用 `contentContainerStyle` 设置这个内部视图的样式。 |

```tsx
<KeyboardAvoidingView behavior="position" contentContainerStyle={{ padding: 16, gap: 12 }} style={{ flex: 1 }}>
  <TextField label="E-mail" />
  <Button title="Sign in" onPress={signIn} />
</KeyboardAvoidingView>
```

这里的内边距或外边距会叠加在视图自身样式设置的值之上，你自己的 padding 不受影响。

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `enabled` | `boolean` | `true` | 是否避让键盘。为 false 时视图不做任何处理。 |
| `behavior` | `"padding" \| "height" \| "position"` | `"padding"` | 视图如何让出空间（见[让位方式](#behaviors)）。 |
| `keyboardVerticalOffset` | `number` | `0` | 键盘显示时加到键盘高度上的值（例如视图下方某个栏的高度）。总和不会小于 0。 |
| `contentContainerStyle` | `StyleProp<ViewStyle>` | 无 | `behavior="position"` 时内部视图的样式。 |
| `children` | `JSX.Element` | 无 | 内容。 |
| `class`、`classList`、`style`、`testID`、`onLayout`、`ref` | | | 作用于外层视图。 |

`KeyboardAvoidingView` 没有事件，也不使用主题颜色。

## 平台说明

- **iOS**：让位动画使用键盘自身的时长和 ease-in-ease-out 曲线。
- **Android**：如果窗口会为键盘缩小（Activity 会调整窗口大小），内容本来就在键盘上方结束，键盘不会遮挡 NSH 视图，KeyboardAvoidingView 也就不会额外让出空间。键盘遮挡视图时，它的表现与 iOS 相同。
- **Mac Catalyst** 没有软键盘，该视图不会移动。

## 相关内容

- [Keyboard](./keyboard.md)：`Keyboard.dismiss()` 与 `keyboard()` 高度。
- [表单与键盘](../docs/forms-and-keyboard.md)。
- [SafeAreaView](./safe-area-view.md)。

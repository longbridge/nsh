---
title: Modal
description: Modal 组件：在所有内容之上呈现内容，可以是全屏、底部弹出的 sheet 或对话框。
order: 11
---

# Modal

modal 在应用其余内容之上、在它自己的原生窗口层中呈现内容：iOS 和 Mac Catalyst 上是一个被 present 的 view controller，Android 上是一个 Dialog。它用于全屏流程、sheet 和对话框。系统自带的手势（下滑关闭 sheet、Android 返回键）都能正常工作，并在原生侧决定。

```tsx
import { signal, Text, View } from "nsh";
import { Button, Modal, SafeAreaView } from "nsh/ui/mobile";

function Example() {
  const open = signal(false);
  return (
    <View style={{ flex: 1, justifyContent: "center", padding: 24 }}>
      <Button title="Open" onPress={() => open.set(true)} />
      <Modal visible={open()} ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12, backgroundColor: "#ffffff" }}>
          <Text style={{ fontSize: 24, fontWeight: "700" }}>A sheet</Text>
          <Button title="Close" onPress={() => open.set(false)} />
        </SafeAreaView>
      </Modal>
    </View>
  );
}
```

`Modal` 从 [`nsh/ui/mobile`](../components/navigation.md) 导入。`visible` 不为 `false` 时呈现，变为 `false` 时关闭；它还会给内容提供 modal 的安全区域 inset，供其内部的 `SafeAreaView` 和 `useSafeAreaInsets()` 读取。

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `visible` | `boolean` | `true` | 为 true 时呈现。 |
| `animationType` | `"slide" \| "fade" \| "none"` | `"slide"` | modal 的出现方式。 |
| `transparent` | `boolean` | `false` | 背景透出下面的内容（用于应用自己绘制的对话框）。 |
| `dismissible` | `boolean` | `false` | 用户可在原生侧直接关闭（iOS 下滑 sheet、Android 返回键），随后触发 `onRequestClose` 和 `onDismiss`。不设置时，这些手势只是请求关闭。 |
| `onRequestClose` | `() => void` | | 用户请求关闭：把 `visible` 设为 false 即可关闭。 |
| `onShow` | `() => void` | | modal 已完全出现。 |
| `onDismiss` | `() => void` | | modal 已完全消失。 |
| `style` | `StyleProp<ViewStyle>` | | modal 的根视图样式。 |
| `children` | `JSX.Element` | | 内容。 |

| `ios` 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `presentationStyle` | `"fullScreen" \| "pageSheet" \| "formSheet"` | `"fullScreen"` | `pageSheet` 和 `formSheet` 会让下面的内容保持可见；在大屏上 `formSheet` 是一个更小的居中 sheet。 |
| `supportedOrientations` | `("portrait" \| "portrait-upside-down" \| "landscape" \| "landscape-left" \| "landscape-right")[]` | 与呈现者相同 | modal 允许的方向。 |

| `android` 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `statusBarTranslucent` | `boolean` | `false` | 内容延伸到状态栏下方；否则从状态栏下方开始。 |

## 默认是受控的

modal 是受控的：用户下滑 sheet 或按返回键时，NSH 通过 `onRequestClose` 询问，modal 会一直保留，直到你把 `visible` 设为 `false`。这样你可以在关闭前先确认。由于主线程从不等待 JS，这个决定无法在事件处理函数里做出；`dismissible` 是事先告诉平台它可以自行关闭 modal：

```tsx
<Modal visible={open()} dismissible ios={{ presentationStyle: "pageSheet" }} onRequestClose={() => open.set(false)}>
  …
</Modal>
```

设置 `dismissible` 后，sheet 会跟随手指并在原生侧关闭；之后 `onRequestClose` 通知你更新状态。

## 对话框

一个透明、淡入的 modal 可以在变暗的背景上绘制自己的对话框：

```tsx
<Modal visible={confirm()} transparent animationType="fade" onRequestClose={() => confirm.set(false)}>
  <View style={{ flex: 1, backgroundColor: "rgba(0,0,0,0.4)", alignItems: "center", justifyContent: "center", padding: 32 }}>
    <View style={{ alignSelf: "stretch", backgroundColor: "#ffffff", borderRadius: 14, padding: 20, gap: 12 }}>
      <Text style={{ fontSize: 20, fontWeight: "700" }}>Delete this item?</Text>
      <Button title="Delete" onPress={remove} />
    </View>
  </View>
</Modal>
```

如果需要系统提示框或操作表，请改用 `nsh/ui/mobile` 的 `Alert` 和 `ActionSheet`（[alert](../components/alert.md)、[action sheet](../components/action-sheet.md)）。应用自绘的 sheet 见 [`Sheet`](../components/sheet.md)。

## Ref 方法

无。

## 平台说明

- Android：返回键首先交给最上层的 modal，然后才是导航栈和 `BackHandler` 监听器。
- `presentationStyle` 和 `supportedOrientations` 仅适用于 iOS，在 Android 上会被忽略。在桌面上，`desktop={{ presentationStyle }}`
  让 `pageSheet` / `formSheet` 成为窗口中居中的卡片，背后的内容变暗。
- 未设置 `statusBarTranslucent` 时，Android modal 的内容从状态栏下方开始。

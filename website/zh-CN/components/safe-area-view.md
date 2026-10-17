---
title: SafeAreaView
description: 按系统 UI、页面标题栏或模态框的 inset 添加内边距的视图，以及 safeArea() 和 useSafeAreaInsets()。
order: 6
---

# SafeAreaView

`SafeAreaView` 通过添加内边距，避免内容被刘海、状态栏、Home 指示条、导航标题栏或 Mac Catalyst 的窗口标题栏遮挡。内边距取的是**视图所在位置**的 inset：在 [Screen](./navigation.md) 内，是该页面标题栏下方的 inset；在 [Modal](./navigation.md#modal) 内，是模态框的 inset；其他情况下，是整个 surface 的安全区域。

```tsx
import { SafeAreaView } from "nsh/ui/mobile";
```

## 用法

```tsx
import { mount, Text } from "nsh";
import { SafeAreaView } from "nsh/ui/mobile";

function App() {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: "#fff8c5", padding: 16 }}>
      <Text style={{ fontSize: 24, fontWeight: "700" }}>Never under the notch</Text>
    </SafeAreaView>
  );
}

mount(App);
```

inset 会**叠加**在视图自身的内边距上。在上面的例子里，内容从顶部 inset 再往下 16 pt 处开始，而背景色铺满整个区域，包括状态栏后面的部分。

## 选择边

默认四条边都会加内边距。传入 `edges` 可以只处理其中几条，比如底部 inset 已经由标签栏或自定义底栏处理时：

```tsx
<SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1 }}>
  …
</SafeAreaView>
```

## 页面与模态框

标题栏可见且不透明的 [Screen](./navigation.md) 已经把内容放在标题栏下方，所以那里的顶部 inset 为 0。SafeAreaView 主要用于以下场景：

- 没有导航栈的应用的根视图；
- 设置了 `headerShown={false}` 或 `headerTransparent` 的页面；
- 全屏 [Modal](./navigation.md#modal) 的内容。

```tsx
<Screen headerShown={false}>
  <SafeAreaView style={{ flex: 1, padding: 16 }}>
    <Button title="Back" onPress={() => nav.pop()} />
  </SafeAreaView>
</Screen>
```

## 读取 inset

需要具体数值时（比如自己给底栏加内边距），使用下面两个 accessor 之一。它们都返回 `Insets` 对象（`{ top, right, bottom, left }`，单位 pt），并且是响应式的：读取它们的代码会在值变化时（例如旋转屏幕）重新运行。

| 导出 | 返回值 |
|---|---|
| `safeArea()` | surface 的安全区域：未被系统 UI（刘海、Home 指示条、标题栏）遮挡的部分。 |
| `useSafeAreaInsets()` | 在组件中调用。返回该组件所在位置的 inset 的 accessor：最近的页面（标题栏下方）的、最近的模态框的，否则为 surface 的安全区域。与 SafeAreaView 使用的 inset 相同。 |

```tsx
import { Text, View } from "nsh";
import { useSafeAreaInsets } from "nsh/ui/mobile";

function BottomBar() {
  const insets = useSafeAreaInsets();
  return (
    <View style={{ flexDirection: "row", padding: 8, paddingBottom: 8 + insets().bottom }}>
      <Text>Bottom bar</Text>
    </View>
  );
}
```

## Props

| Prop | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `edges` | `("top" \| "right" \| "bottom" \| "left")[] \| null` | 全部四条边 | 要加内边距的边。 |
| `children` | `JSX.Element` | 无 | 内容。 |
| `class`、`classList`、`style`、`testID`、`onLayout`、`ref` | | | 作用于该视图。其内边距会与 inset 相加。 |

`SafeAreaView` 没有事件，也不使用主题颜色。

## 平台说明

- **iOS**：inset 来自 UIKit 的安全区域（状态栏、刘海或灵动岛、Home 指示条）。
- **Android**：NSH 只报告系统栏和屏幕挖孔中与 NSH 视图重叠的部分。当 NSH [嵌入](../docs/embedding-android.md)在原生工具栏下方时，视图不会得到顶部 inset。
- **Mac Catalyst**：顶部 inset 为窗口的标题栏。

## 相关内容

- [Keyboard](./keyboard.md) 和 [KeyboardAvoidingView](./keyboard-avoiding-view.md)：避开软键盘。
- [Navigation](./navigation.md)：页面和模态框会提供各自的 inset。
- [设备与环境](../docs/device-and-environment.md)。

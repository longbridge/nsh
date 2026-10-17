---
title: View
description: 最基础的容器元素：布局、背景、边框、阴影、按压处理和手势。
order: 2
demo: el-view
---

# View

`<View>` 是每个界面的基本构件：一个原生容器（iOS 和 Mac Catalyst 上是 UIView，Android 上是 ViewGroup），用 flexbox 或 grid 布局子元素，绘制背景、边框、阴影和渐变，并处理按压。按钮、卡片、列表行和整个界面都是 view。

```tsx
import { mount, signal, Text, View } from "nsh";

function App() {
  const liked = signal(false);
  return (
    <View style={{ flex: 1, padding: 24, gap: 16, backgroundColor: "#f6f8fa" }}>
      <View
        style={{
          backgroundColor: "#ffffff",
          borderRadius: 16,
          padding: 16,
          boxShadow: "0 4px 12px rgba(0, 0, 0, 0.08)",
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
        }}
      >
        <View style={{ width: 40, height: 40, borderRadius: 20, backgroundColor: "#8250df" }} />
        <Text style={{ flex: 1, fontSize: 16 }}>一张卡片</Text>
        <View onPress={() => liked.set(!liked())} activeOpacity={0.5} hitSlop={8}>
          <Text style={{ fontSize: 22 }}>{liked() ? "♥" : "♡"}</Text>
        </View>
      </View>
    </View>
  );
}

mount(App);
```

## 布局

view 用 flexbox 布局子元素，默认值为：`flexDirection: "column"`、`flexShrink: 0`、`alignContent: "flex-start"`、`position: "relative"`。`display: "grid"` 切换为 CSS grid，`display: "block"` 切换为块布局。尺寸单位是点。参见[布局](../docs/layout.md)和[样式参考](../modules/style-reference.md)。

Tailwind 的 `flex` 类遵循 CSS 而不是样式的默认值：`class="flex"` 是横向排列。需要纵向排列时用 `flex-col`。

## 属性

除[通用属性](./index.md)外，`<View>` 没有自己的属性：`style`（`ViewStyle`）、`class`、`classList`、`disabled`，按压属性（`onPress`、`onLongPress`、`onPressIn`、`onPressOut`、`delayLongPress`、`activeOpacity`、`underlayColor`、`underlayOpacity`、`hitSlop`、`pointerEvents`），`onHoverIn` / `onHoverOut`，`onLayout`、`ref`、`nativeID`、`testID`，手势属性（`pan`、`pinch`、`rotation` 及其事件），触摸与响应者属性，无障碍属性，以及 `ios` / `android` / `desktop`。

## 按压反馈

带有按压处理函数的 view 就是可按压的。手指按下期间，有三种反馈在原生侧运行，不经过 JS：

```tsx
// 整个视图淡化。
<View onPress={open} activeOpacity={0.6}>…</View>

// 显示背景色并淡化子元素（默认 0.85）。
<View onPress={open} underlayColor="#e5e7eb" underlayOpacity={0.7}>…</View>

// 用 active: 类在按下时改变任意样式。
<View onPress={open} class="bg-blue-600 active:bg-blue-800 active:scale-95">…</View>
```

在 Android 上，`android={{ ripple: { color: "#0000001f" } }}` 会加上 Material 水波纹（可用 `borderless`、`radius` 和 `foreground` 调整）。

`disabled` 会从原生视图上移除按压监听（既不响应按压，也没有反馈），并应用 `disabled:` 类：

```tsx
<View onPress={submit} disabled={!valid()} class="bg-green-600 disabled:opacity-40">…</View>
```

## 事件

| 事件 | 载荷 | 触发时机 |
|---|---|---|
| `onPress` | 无 | 在视图内完成一次点击。 |
| `onLongPress` | 无 | 手指按住达到 `delayLongPress`（500 ms）。 |
| `onPressIn` / `onPressOut` | 无 | 手指按下 / 抬起或移出。 |
| `onHoverIn` / `onHoverOut` | 无 | 鼠标移到视图（或其内部的视图）上 / 离开视图或窗口；macOS、Linux 和 GPUI（见[悬停](./index.md#悬停)）。 |
| `onLayout` | `{ x, y, width, height }` | 相对父视图的 frame 发生变化。 |
| `onPanStart` / `onPanEnd` | `PanEvent` | 通过 `pan` 绑定的拖动开始 / 结束。 |
| `onPinchStart` / `onPinchEnd` | `{ scale, velocity }` | 通过 `pinch` 绑定的捏合开始 / 结束。 |
| `onRotationStart` / `onRotationEnd` | `{ rotation, velocity }` | 通过 `rotation` 绑定的旋转开始 / 结束。 |
| `onAccessibilityAction` | `{ actionName }` | 执行了一个自定义无障碍操作。 |

触摸与响应者事件：参见[通用属性](./index.md)。

## ref 方法

普通 view 没有自己的命令。`ref` 拿到的是 `YNode`，例如可以传给 `AccessibilityInfo.setAccessibilityFocus`（[`nsh/accessibility`](../modules/nsh-accessibility.md)）。

## 文本中的视图

放在 `<Text>` 里的 `<View>` 是行内附件：它坐落在基线上、随文字一起换行，并且可以单独响应按压。参见 [text](./text.md)。

## 平台说明

- `boxShadow` 以及阴影属性（`shadowColor`、`shadowOpacity`、`shadowRadius`、`shadowOffset`）在两个平台上都会绘制；`android={{ elevation }}` 则改用 Android 的 Material 阴影。在 Android 上阴影需要背景色（背景透明的视图不会投下阴影）。
- view 上的 `filter`：iOS 只支持 `opacity()`、不超过 1 的 `brightness()` 和 `drop-shadow()`（图片支持所有颜色滤镜和 `blur()`）；Android 从 API 31 起全部支持，低于该版本只支持颜色滤镜。
- `backdropFilter: "blur(…)"` 在 iOS 上是系统材质（近似效果），在 Android 上需要 API 31。
- Android 不支持 `skewX` / `skewY` 变换。
- `borderCurve: "continuous"` 在两个平台上都会绘制平滑过渡到边的圆角（如同 iOS 应用图标）。
- `cursor` 样式在有指针的场合生效（Mac Catalyst、连接指针设备的 iPad、桌面平台）。

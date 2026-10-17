---
title: StatusBar
description: 通过渲染 StatusBar 或调用其函数，控制状态栏内容的颜色以及是否显示。
order: 9
---

# StatusBar

`StatusBar` 决定系统状态栏的外观：内容（时间、电量、信号）的颜色，以及状态栏是否隐藏。你可以把它当作组件渲染，让设置只在这部分 UI 显示期间生效；也可以调用它的静态函数。

```tsx
import { StatusBar } from "nsh/ui/mobile";
```

## 渲染 StatusBar

渲染出来的 `StatusBar` 不占任何空间，它的设置在挂载期间生效：

```tsx
import { mount, Text, View } from "nsh";
import { StatusBar } from "nsh/ui/mobile";

function App() {
  return (
    <View style={{ flex: 1, backgroundColor: "#0b0b0f" }}>
      <StatusBar barStyle="light-content" />
      <Text style={{ color: "#ffffff", padding: 16, paddingTop: 64 }}>Dark screen, light status bar</Text>
    </View>
  );
}

mount(App);
```

可以同时渲染多个 `StatusBar`，比如每个页面一个。**最后挂载**的那个生效，并且是**逐个属性**生效：只设置了 `hidden` 的 `StatusBar` 会保留下层那个的 `barStyle`。某个 `StatusBar` 卸载时（例如它所在的页面被弹出），之前的设置会重新生效。

```tsx
function PhotoViewer() {
  return (
    <Screen headerShown={false}>
      <StatusBar hidden animated />
      …
    </Screen>
  );
}
```

## 用函数设置

静态函数设置的是基础设置，任何渲染出来的 `StatusBar` 都会覆盖它。

```tsx
StatusBar.setBarStyle("dark-content", true);
StatusBar.setHidden(true, "fade");
StatusBar.setHidden(false);
```

| 函数 | 说明 |
|---|---|
| `StatusBar.setBarStyle(barStyle: BarStyle, animated?: boolean \| null): void` | 设置内容的颜色。`animated` 默认为 `false`。 |
| `StatusBar.setHidden(hidden: boolean, animation?: "none" \| "fade" \| "slide" \| null): void` | 隐藏或显示状态栏。除 `"none"` 以外的任何动画值都会让变化以动画呈现。 |

## 状态栏样式

| `barStyle` | 内容 |
|---|---|
| `"default"` | 平台针对当前外观（浅色或深色模式）的默认样式。 |
| `"light-content"` | 浅色内容，用于深色背景。 |
| `"dark-content"` | 深色内容，用于浅色背景。 |

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `barStyle` | `"default" \| "light-content" \| "dark-content" \| null` | 不设置 | 内容的颜色。 |
| `hidden` | `boolean \| null` | 不设置 | 隐藏状态栏。 |
| `animated` | `boolean \| null` | 不设置 | 以动画呈现 `barStyle` 和 `hidden` 的变化。 |
| `android` | `{ backgroundColor?: string \| null }` | 不设置 | 仅 Android：状态栏的背景色。 |

未设置的属性，会把状态栏的这一部分交给下层的 `StatusBar` 或函数决定。`StatusBar` 没有事件。

## 平台说明

- **iOS**：隐藏和显示以淡入淡出呈现，`"slide"` 同样是淡入淡出。在[嵌入](../docs/embedding-ios.md)原生应用的场景下，状态栏的外观由宿主应用的视图控制器决定。宿主可以在其视图控制器中返回 `NshAppView.preferredStatusBarStyle`、`NshAppView.prefersStatusBarHidden` 和 `NshAppView.preferredStatusBarUpdateAnimation`，以应用 NSH 的设置。
- **Android**：`android={{ backgroundColor }}` 为状态栏着色。它只在 edge-to-edge 之前生效（Android 15，即 API 35 起，应用会绘制到状态栏之下）。
- **Mac Catalyst** 没有状态栏。

> [!IMPORTANT]
> 如果你的 `nsh.json` 声明了 `"capabilities"`，只有其中包含 `"statusBar": true` 时才能更改状态栏，否则更改会被拒绝并记录日志。参见[沙箱与权限](../docs/sandbox-and-capabilities.md)。

```json
{
  "capabilities": {
    "statusBar": true
  }
}
```

## 相关

- [nsh/android](../modules/nsh-android.md)：`setStatusBarBackgroundColor` 及其他 Android 专用 API。
- [SafeAreaView](./safe-area-view.md)：让内容避开状态栏。

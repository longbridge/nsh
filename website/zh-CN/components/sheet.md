---
title: Sheet
description: 覆盖在半透明遮罩上的底部面板，用户可以下拉、点按外部或按返回键关闭。
order: 8
demo: ui-sheet
---

# Sheet

`Sheet` 是一个从屏幕底部滑入、覆盖在半透明遮罩之上的面板。用户可以向下拖动、点按遮罩或按 Android 返回键来关闭它。拖动和动画都在原生端运行：面板在 UI 线程上跟随手指移动，不经过 JS，也不需要每帧渲染。

```tsx
import { Sheet } from "nsh/ui/mobile";
```

## 用法

`Sheet` 是受控组件：`open` 为 true 时显示。用户关闭它时会调用 `onClose`，由你把 `open` 设为 false：

```tsx
import { mount, signal, Text, View } from "nsh";
import { Button, Sheet } from "nsh/ui/mobile";

function App() {
  const open = signal(false);
  return (
    <View style={{ flex: 1, padding: 16, paddingTop: 48 }}>
      <Button title="Share" onPress={() => open.set(true)} />
      <Sheet open={open()} onClose={() => open.set(false)}>
        <Text style={{ fontSize: 17, fontWeight: "600" }}>Share the photo</Text>
        <Button title="Copy link" variant="secondary" onPress={() => open.set(false)} />
        <Button title="Done" onPress={() => open.set(false)} />
      </Sheet>
    </View>
  );
}

mount(App);
```

## 行为

- **打开**：`open` 变为 true 时，面板以弹簧动画从屏幕下方滑入，遮罩随面板上升逐渐显现。
- **拖动**：面板跟随竖直方向的拖动。松手时如果面板低于停靠位置超过 120 pt，或向下轻扫的速度超过 900 pt/s，就会关闭；否则弹回原位，并延续轻扫的速度。
- **由用户关闭**（拖动、点按遮罩、Android 返回键）：面板在 220 ms 内滑出，然后调用 `onClose`。务必在 `onClose` 中把 `open` 设为 false：只要 `open` 仍为 true，即使面板已经滑出视野，面板所在的模态层依然存在，并会继续盖住整个屏幕。
- **由代码关闭**：把 `open` 设为 false 会立即移除面板，没有下滑动画，也不会调用 `onClose`。

面板呈现在一个透明的[模态层](../elements/modal.md)中，因此会盖住所有内容，包括导航栏和标签栏。你可以在组件树的任意位置声明它。

## 布局

面板的高度由内容决定，内容按列排列，间距 12 pt。面板顶部有一个拖动条，左右内边距 16 pt，上方两角为圆角；底部内边距为安全区域的底部 inset 再加 16 pt，使内容始终位于 Home 指示条之上。

如果要在面板中放列表，请给列表的父视图一个高度：

```tsx
<Sheet open={picking()} onClose={() => picking.set(false)}>
  <Text style={{ fontSize: 17, fontWeight: "600" }}>Choose a city</Text>
  <View style={{ height: 320 }}>
    <SelectList
      options={cities}
      value={city()}
      onSelect={(i) => {
        city.set(i);
        picking.set(false);
      }}
    />
  </View>
</Sheet>
```

## 属性

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `open` | `boolean` | `false` | 是否显示面板。 |
| `children` | `JSX.Element` | 无 | 面板的内容。 |
| `class`、`classList`、`style`、`testID`、`onLayout`、`ref` | | | 作用于 Sheet 在你的组件树中的宿主视图，而不是面板本身。 |

## 事件

| 事件 | 参数 | 触发时机 |
|---|---|---|
| `onClose` | 无 | 用户关闭了面板（拖动、点按遮罩或返回键），在面板滑出之后触发。请把 `open` 设为 false。 |

## 主题

| 部位 | 主题键 |
|---|---|
| 面板 | `background` |
| 遮罩 | `overlay` |
| 拖动条 | `muted.background` |
| 上方圆角 | `radius.lg`，至少 16 pt |

参见[主题](./index.md#theming)。默认的 `overlay` 颜色很浅（浅色模式下为 5% 的黑色），所以遮罩只会让背景稍稍变暗。如需更深的遮罩，请在主题中设置 `overlay`。

## 无障碍

遮罩带有标签 "Close"，使用屏幕阅读器的用户激活它即可关闭面板。在 Android 上，返回键也会关闭面板。

## 相关

- [Modal](./navigation.md#modal)：全屏和页面表单样式的模态层。
- [动画](../docs/animation.md)：动画值与弹簧动画。
- [ActionSheet](./action-sheet.md)：系统的操作表。

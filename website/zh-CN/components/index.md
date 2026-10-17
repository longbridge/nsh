---
title: 移动端 UI
description: nsh/ui/mobile UI 套件：其中的组件与模块，以及如何通过 nsh.json 设置主题。
order: 1
---

# 移动端 UI

`nsh/ui/mobile` 是 NSH 面向手机和平板的 UI 套件，包含按钮、卡片、列表、文本输入框、底部面板、安全区域、键盘、状态栏、系统对话框和导航。按需导入即可：

```tsx
import { Button, Card, Group, Sheet, Stack, Screen, createNavigator } from "nsh/ui/mobile";
```

它构建在[宿主元素](../elements/index.md)（`<View>`、`<Text>`、`<Input>` 等）之上。宿主元素是所有 UI 套件共用的基础构件，而这里的组件是用这些元素搭出来的、开箱即用的移动端外观。

## 组件的工作方式

这些组件用 Rust 编写，而不是 JavaScript。在应用里，`<Button>` 依然只是一个 JSX 节点：它的 props 传给原生核心，由核心用宿主元素渲染出按钮，并与界面其余部分在同一轮布局中完成排版；它的事件再回到你的处理函数。由此带来几点实际影响：

- **iOS 和 Android 外观一致。** 组件用 NSH 自己的视图绘制，而不是 UIKit 或 Android 控件，所以 `Button` 在两个平台上看起来一样。例外是 [Alert](./alert.md) 和 [ActionSheet](./action-sheet.md)，它们显示的是平台的系统对话框。
- **交互保持原生。** 按压反馈、[Sheet](./sheet.md) 的拖动、长按 [Stepper](./stepper.md) 按钮时的连续步进，都在核心或 UI 线程上运行。JS 只接收事件（`onPress`、`onChange`、`onClose` 等）。
- **类型由源码生成。** `.nsh/types/nsh/ui/mobile.d.ts` 中的 TypeScript 声明是从 Rust 的 props 生成的，编辑器能显示每个 prop 及其文档。
- **你的内容仍归你所有。** 插槽（`header`、`footer`）、子元素和列表行都是你的 JSX。组件只负责摆放，从不创建或修改它们，因此其中的 signal、事件处理函数和 ref 都照常工作。

每个组件的根视图都接受 `class`、`classList`、`style`、`testID`、`onLayout` 和 `ref`（`ComponentHostProps`）。你的 `class` 在组件自身样式之后应用，因此会覆盖它们：

```tsx
<Button title="Continue" class="rounded-full" onPress={next} />
<Card class="mx-4" variant="outlined">…</Card>
```

## 组件

| 组件 | 说明 |
|---|---|
| [Button](./button.md) | 带文字标签的按压目标：`primary`、`secondary`、`danger` 或 `plain` |
| [Card](./card.md) | 圆角卡片，可带页眉和页脚 |
| [Group](./group.md) | 设置页风格的行，放在圆角框内，以细线分隔 |
| [ListView](./list-view.md) | 渲染你的行的虚拟化列表，plain 或 inset 样式 |
| [SafeAreaView](./safe-area-view.md) | 按安全区域、页面标题栏或模态框的 inset 添加内边距的视图 |
| [SelectList](./select-list.md) | 可滚动的选项列表，选中项带勾号 |
| [Sheet](./sheet.md) | 覆盖在变暗背景上的底部面板，向下拖动即可关闭 |
| [StatusBar](./status-bar.md) | 状态栏内容颜色与显隐，可通过渲染组件或调用函数设置 |
| [Stepper](./stepper.md) | 带 − 和 + 按钮的数字；长按按钮会连续步进 |
| [TextField](./text-field.md) | 带标签和清除按钮的文本输入框 |
| [KeyboardAvoidingView](./keyboard-avoiding-view.md) | 让滚动视图之外的内容保持在软键盘上方 |

模块与导航：

| 导出 | 说明 |
|---|---|
| [Alert](./alert.md) | `Alert.alert(title, message?, buttons?, options?)`：系统警告框，返回被按下按钮的索引 |
| [ActionSheet](./action-sheet.md) | `ActionSheet.show(options)`：系统操作表，返回所选操作的索引 |
| [Keyboard](./keyboard.md) | `Keyboard.dismiss()` 以及响应式的 `keyboard()` accessor |
| [Navigation](./navigation.md) | `createNavigator`、`Stack`、`Screen` 和 `Modal`：原生导航栈与模态框 |

该套件还从运行时重新导出了三个环境 accessor：`safeArea()`、`keyboard()` 和 `useSafeAreaInsets()`（见 [SafeAreaView](./safe-area-view.md) 和 [Keyboard](./keyboard.md)）。

<a id="theming"></a>

## 主题

所有组件的颜色都取自同一个主题，从不使用写死的颜色值。每种颜色都有浅色和深色两个值。系统在浅色与深色外观之间切换时，组件会在原生层直接换色，无需重新渲染，也不经过 JS。

默认主题是 gpui-kit 的默认主题，一套中性的黑白配色，圆角半径针对移动端做了调整。要使用自己的主题，在 `nsh.json` 中设置 `"theme"`，值为主题文件的路径（相对于 `nsh.json`）：

```json
{
  "name": "My App",
  "id": "com.example.myapp",
  "theme": "brand-theme.json"
}
```

也可以直接把主题内容内联写成这个值。格式与 gpui-kit 的主题文件相同。`themes` 中的每一项为一种外观设置颜色（`mode`：`"light"` 或 `"dark"`；不写 `mode` 时视为浅色）：

```json
{
  "themes": [
    {
      "mode": "light",
      "radius": 8,
      "radius.lg": 14,
      "font.size": 16,
      "colors": {
        "primary.background": "#7c3aed",
        "primary.foreground": "white",
        "border": "neutral-200"
      }
    },
    {
      "mode": "dark",
      "colors": {
        "primary.background": "#a78bfa",
        "primary.foreground": "neutral-950"
      }
    }
  ]
}
```

- 你的主题叠加在默认主题之上，只需列出要改的部分。
- 颜色可以是 CSS 颜色（`#7c3aed`、`rgb(…)`、`white`），也可以是 Tailwind 默认调色板中的名称（`neutral-900`、`red-500`）。`nsh.json` 中 `tailwind` 键定义的自定义颜色在这里不可用。
- `radius` 是控件（按钮、输入框、步进器按钮）的圆角半径，`radius.lg` 是大面积表面（卡片、分组、inset 列表、底部面板）的圆角半径，`font.size` 是按钮文字、卡片标题和步进器的基础字号。单位均为 pt。
- 如果省略了按下状态的颜色（`primary.active.background`、`secondary.active.background`、`danger.active.background` 等），NSH 会从基础颜色推导：浅色模式下加深 12%，深色模式下提亮 12%。
- 只有 gpui-kit 桌面组件使用的键（侧边栏、标题栏等）会被忽略，因此 gpui-kit 的主题文件可以直接拿来用。
- 打包器在构建时检查主题。文件缺失、JSON 无效、`mode` 未知或某个值不是颜色，都会以 `nsh.json: theme: …` 错误使构建失败。

下表列出移动端组件用到的主题键及其默认的浅色和深色值：

| 键 | 浅色 | 深色 | 使用者 |
|---|---|---|---|
| `background` | `white` | `neutral-950` | ListView、SelectList 和 TextField 的背景，Sheet 面板 |
| `foreground` | `neutral-950` | `neutral-50` | ListView、SelectList 和 Stepper 中的文字；Plain 按钮 |
| `border` | `neutral-200` | `neutral-800` | 分隔线、Card 边框 |
| `primary.background` / `.active.background` / `.foreground` | `neutral-900` / `neutral-950` / `neutral-50` | `neutral-50` / `neutral-200` / `neutral-900` | Primary 按钮；SelectList 勾号、TextField 选区（`primary.background`） |
| `secondary.background` / `.active.background` / `.foreground` | `neutral-200` / `neutral-300` / `neutral-900` | `neutral-800` / `#212121` / `neutral-50` | Secondary 按钮、Stepper 按钮 |
| `danger.background` / `.active.background` / `.foreground` | `red-500` / `red-600` / `neutral-50` | `red-400` / `red-500` / `red-600` | Danger 按钮 |
| `accent.background` | `neutral-100` | `neutral-800` | 按下时的 Plain 按钮 |
| `muted.background` | `neutral-100` | `neutral-800` | inset ListView 的页面背景、Sheet 拖动条、TextField 清除按钮 |
| `muted.foreground` | `neutral-500` | `neutral-400` | Group 标题说明、TextField 标签和占位文字 |
| `group_box.background` / `.foreground` | `#f5f5f5` / `#171717` | `neutral-950` / `neutral-50` | Card 和 Group 的表面 |
| `input.border` | `neutral-200` | `#2f2f2f` | TextField 边框 |
| `ring` | `neutral-400` | `neutral-500` | 聚焦时的 TextField 边框 |
| `list.active.background` | `#bfdbfe33` | `#1e40af33` | 按下时的 SelectList 行 |
| `overlay` | `#0000000d` | `#00000033` | Sheet 背景遮罩 |

> [!TIP]
> 只想调整某一个组件实例时，传入 `class` 或 `style`：它们作用于组件的根视图，并覆盖组件自身的样式。想在所有地方改变某个组件的外观，就修改主题。

## 相关内容

- [样式](../docs/styling.md)和 [Tailwind](../docs/tailwind.md)：为你自己的视图设置样式。
- [导航指南](../docs/navigation.md)和[表单与键盘](../docs/forms-and-keyboard.md)。
- [平台相关代码](../docs/platform-specific.md)：`ios={{…}}` / `android={{…}}` props。

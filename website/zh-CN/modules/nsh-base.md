---
title: nsh/base
description: 应用界面的无样式基础组件：按钮、Markdown 与 HTML 文档、跨文字的选区、导航栈与对话框、安全区、系统警告对话框、软键盘，以及页面和对话框的状态栏。
order: 2.5
demo: ui-navigation
---

# nsh/base

`nsh/base` 是应用界面的基础组件层：不带样式、负责行为、状态、焦点和无障碍的组件，各平台通用。它位于基础元素（`nsh` 和各个 `nsh/<module>` 模块）与应用自己的组件库之间，就像 GPUI Kit 中 `gpui-base` 位于 `gpui` 与带样式的组件库之间（见[模块](./index.md#three-layers)）。它提供控件（`Button`）、Markdown 或 HTML 文档（`TextView`）、跨文字的选区（`SelectionArea`）、导航（由 `createNavStack` 创建的状态驱动、由多个 `NavPage` 组成的 `NavStack`，以及在所有内容之上展示内容的 `Dialog`）、安全区（`SafeAreaView`、`safeArea()` 和 `safeAreaInsets()`）、系统警告对话框（`AlertDialog`）、软键盘（`Keyboard`、`keyboardFrame()` 和 `KeyboardAvoidingView`）。页面和对话框还决定状态栏（`statusBar`）。

外观由应用决定：页面、对话框或标题栏里的内容都是你自己的 JSX，用基础元素配合样式、[Tailwind](../docs/tailwind.md) 类和主题绘制；或者是[你自己的 crate](../docs/rust.md) 中的 Rust 组件；也可以是社区基于 `nsh/base` 构建的组件库。每个宿主都提供 `nsh/base`；Rust 应用通过 `nsh-base` crate 使用同一套 API（`nsh_base::{button, safe_area_view, …}`）。

```rust tabs
use nsh_base::{
    AvoidBehavior, Edge, alert_dialog, button, dialog, keyboard_avoiding_view, nav_page, nav_stack, safe_area_view,
    theme,
};
```

```tsx tabs
import {
  AlertDialog, Button, createNavStack, Dialog, Keyboard, keyboardFrame, KeyboardAvoidingView, NavPage, NavStack,
  SafeAreaView, safeArea, safeAreaInsets, SelectionArea, TextView,
} from "nsh/base";
import type {
  AlertDialogAction, AlertDialogOptions, AlertDialogPresentation, AlertDialogRole, AvoidBehavior, ButtonProps,
  DialogChangeReason, DialogProps, Edge, HeaderStyle, Insets, NavPageProps, NavStackProps, NavStackState, StatusBar,
  SelectionAreaProps, StatusBarForeground, TextViewProps, TextViewDocument, MarkdownExtension, MarkdownOptions,
} from "nsh/base";
```

本页是模块的索引：每个组件都有自己的元素页面，包含示例、属性和平台说明。完整的上手讲解见[导航指南](../docs/navigation.md)和[表单与键盘](../docs/forms-and-keyboard.md)。

| TSX | Rust | 是什么 |
|---|---|---|
| [`Button`](../elements/button.md) | `button()` | 带禁用和选中状态、面向屏幕阅读器有角色的按压目标。 |
| [`NavStack`、`NavPage`、`createNavStack`](../elements/nav-stack.md) | `nav_stack()`、`nav_page()` | 原生导航栈及其页面。 |
| [`Dialog`](../elements/dialog.md) | `dialog()` | 在所有内容之上呈现的内容：sheet、全屏流程、对话框。 |
| [`SafeAreaView`](../elements/safe-area-view.md) | `safe_area_view()` | 避开系统界面和标题栏的视图。 |
| [`safeArea()`、`safeAreaInsets()`](../elements/safe-area-view.md#safearea-and-safeareainsets) | `safe_area()`、`cx.insets()` | 边距本身，响应式。 |
| [`KeyboardAvoidingView`](../elements/keyboard-avoiding-view.md) | `keyboard_avoiding_view()` | 让内容保持在软键盘上方的视图。 |
| [`Keyboard.dismiss()`、`keyboardFrame()`](../elements/keyboard-avoiding-view.md#hiding-the-keyboard) | `dismiss_keyboard()`、`keyboard_height()` | 隐藏键盘；键盘在界面上的高度。 |
| [`AlertDialog`](../elements/alert-dialog.md) | `alert_dialog()` | 平台的警告框或操作表。 |
| 页面和对话框的 `statusBar` | `.status_bar(..)` | 页面或对话框显示期间的状态栏。见[状态栏](#status-bar)。 |

在 Rust 中，`theme()` 以设计 token 的形式读取应用的主题（`theme().primary()`、`theme().radius()`），与 TSX 的[主题类](../docs/tailwind.md#主题-token)使用同样的 token。

<a id="controls"></a>

## 控件

<a id="button"></a>

### Button

一个只负责行为、没有外观的按压目标：处理按压、禁用和选中状态，并向屏幕阅读器报告它的角色。它的外观是应用的 `class` 和 `style` 加上状态变体，内容是它的子元素。见 [Button](../elements/button.md)。

<a id="documents"></a>

## 文档

### TextView

`TextView` 用 [`Text`](../elements/text.md) 元素和视图显示 Markdown、HTML 或应用自己构建的文档块：标题、列表、代码、表格、图片、公式、脚注以及应用自己的语法。它为流式输出的 AI 聊天回复而设计（`options={{ streaming: true }}` 加上 `caret`），自带一套采用主题颜色的阅读排版，应用可按部件重新设定样式（`classes`、`styles`）。点击链接会通知应用（`onLinkPress`），渲染函数则把应用自己的内容放进文档：代码块及其操作、表格的操作、公式、图片和自定义节点。

```tsx
<TextView
  markdown={reply()}
  options={{ streaming: true }}
  classes={{ heading: "font-semibold", code: "rounded bg-neutral-100 px-1", link: "text-blue-600" }}
  onLinkPress={(url) => openLink(url)}
/>
```

部件、流式输出、渲染函数、Markdown 扩展、文档模型以及在 Rust 中的用法见 [TextView](../elements/text-view.md)。

<a id="selection"></a>

## 选择

<a id="selectionarea"></a>

### SelectionArea

`SelectionArea` 让其中的文字一起选择，就像网页上的文字一样：一个选区可以从一段文字中的某处延伸到另一段文字中的某处，跨越视图、[`Text`](../elements/text.md) 元素和 [`TextView`](../elements/text-view.md)，按阅读顺序排列。把聊天记录包在它里面，选区就能从一条消息开始、在下一条消息结束；复制会把选中的文字放进剪贴板，并排的文字之间用制表符连接，其他文字之间用换行连接。

```tsx
import { Text, View } from "nsh";
import { SelectionArea, TextView } from "nsh/base";

<SelectionArea class="gap-3 p-4">
  <Text class="font-semibold">Release notes</Text>
  <TextView markdown={notes} />
  <View class="flex-row gap-2">
    <Text selectable={false} class="text-neutral-500">Updated</Text>
    <Text>October 21</Text>
  </View>
</SelectionArea>
```

| 平台 | 如何选择 |
|---|---|
| macOS、Windows、Linux、GPUI | 在文字上拖动；双击选中一个词，三击选中一个段落，之后的拖动按词或段落继续。Shift+点击扩展选区。Ctrl / Cmd+A 选中区域内的全部文字，Ctrl / Cmd+C 复制，Esc 或在区域外按下会清除选区。 |
| iOS、Android | 长按选中一个词；用平台的选择手柄扩展选区，平台的编辑菜单提供“拷贝”和“全选”。 |
| Web | 浏览器自己的选择，使用浏览器的手势、菜单和快捷键。 |

| 属性 | 类型 | |
|---|---|---|
| `children` | `JSX.Element` | 内容：任意视图，文字可以在任意层级。 |
| `class`、`style` | | 它的布局和外观，与 [View](../elements/view.md) 相同。 |

- **不参与选择的文字**：设置了 `selectable={false}` 的文字（标签、按钮上的文字）会被选区及其手势跳过。
- **嵌套的区域**：一个区域在另一个区域里面时属于外层区域，一个选区覆盖两者。
- **TextView**：设置了 `selectable` 的 `TextView` 本身就是一个选择区域：它的段落、列表项、单元格和代码块一起选择。放在 `SelectionArea` 里时，它加入外层的选区。
- **区域之外**：不在任何区域里的 `selectable` 文字单独选择，见 [Text](../elements/text.md#属性)。
- **外观**是平台的高亮，使用每段文字的 `selectionColor`（`selection:text-*`）；区域本身不绘制任何东西。
- **列表**：在虚拟化的 [List](../elements/list.md) 中，选区覆盖已挂载的行；滚出视野被回收的行会离开选区。

<a id="navigation"></a>

## 导航

由 `NavPage` 组成的 `NavStack` 在 iOS 上是 `UINavigationController`，在 Android 上是带工具栏和预测性返回的页面栈；在桌面平台和 Web 上，每个页面显示应用自己的 `header`。见 [NavStack](../elements/nav-stack.md)，其中也介绍了 `createNavStack` 和离开页面。

<a id="dialog"></a>

### Dialog

`Dialog` 在所有内容之上（包括栈的标题栏）、在它自己的原生窗口层中呈现内容。它的弹出层和背景层由应用设定样式。见 [Dialog](../elements/dialog.md)。

<a id="safe-areas"></a>

## 安全区

<a id="safeareaview"></a>

### SafeAreaView

`SafeAreaView` 按自己渲染位置的边距为内容添加内边距：最近的页面的（标题栏下方）、最近的对话框的，否则为界面的安全区。见 [SafeAreaView](../elements/safe-area-view.md)。

<a id="safearea-and-safeareainsets"></a>

### safeArea 与 safeAreaInsets

`safeArea()` 是界面的安全区；`safeAreaInsets()` 是组件渲染位置的边距。两者都是响应式的 `Insets`（`{ top, right, bottom, left }`，单位为点）。见 [safeArea 与 safeAreaInsets](../elements/safe-area-view.md#safearea-and-safeareainsets)。

<a id="alert-dialogs"></a>

## 警告对话框

<a id="alertdialog"></a>

### AlertDialog

`AlertDialog.show(options)` 显示平台自己的警告框或操作表，并以用户选择的操作作为结果。见 [AlertDialog](../elements/alert-dialog.md)。

<a id="keyboard"></a>

## 键盘

`Keyboard.dismiss()` 隐藏软键盘，`keyboardFrame()` 告诉你它覆盖了屏幕的多少。见[隐藏键盘](../elements/keyboard-avoiding-view.md#hiding-the-keyboard)和[键盘高度](../elements/keyboard-avoiding-view.md#the-keyboards-height)。

<a id="keyboardavoidingview"></a>

### KeyboardAvoidingView

`KeyboardAvoidingView` 让它的内容保持在软键盘上方，并随键盘移动。见 [KeyboardAvoidingView](../elements/keyboard-avoiding-view.md)。

<a id="status-bar"></a>

## 状态栏

状态栏属于屏幕上正在显示的内容，和 UIKit 一样：在 UIKit 中，每个视图控制器声明自己的 `preferredStatusBarStyle`，由最上层的那个决定。在 NSH 中，页面和对话框就是这些视图控制器：`NavPage` 或 `Dialog` 用 `statusBar` 声明它需要的状态栏，应用则在 `nsh.json` 中声明默认值。

```rust tabs
use nsh_base::nav_page;
use nsh_core::ui::prelude::*;

fn photo_viewer() -> Screen {
    nav_page().header_shown(false).status_bar(
        StatusBarSetting::default()
            .foreground(StatusBarStyle::Light)
            .hidden(true)
            .animated(true),
    )
}
```

```tsx tabs
import { NavPage } from "nsh/base";

function PhotoViewer() {
  return (
    <NavPage headerShown={false} statusBar={{ foreground: "light", hidden: true, animated: true }}>
      …
    </NavPage>
  );
}
```

`statusBar` 接受以下字段，每个都可省略（在 Rust 中可以只传一个 `StatusBarStyle`，或用 `StatusBarSetting` 的同名构建方法）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `foreground` | `"auto" \| "light" \| "dark"`（Rust：`StatusBarStyle::Default`、`Light`、`Dark`） | 内容（时间、电量、信号）的颜色：`"light"` 用于深色背景，`"dark"` 用于浅色背景，`"auto"` 为平台针对当前外观的颜色。 |
| `hidden` | `boolean` | 隐藏状态栏。 |
| `animated` | `boolean` | 页面或对话框出现和离开时，以动画呈现变化。 |

这个值是响应式的：`statusBar={{ foreground: dark() ? "light" : "dark" }}` 会跟随信号变化，读取信号的 Rust 闭包也一样。

<a id="which-one-applies"></a>

### 哪个生效

**最近显示**的页面或对话框决定状态栏，并且**逐个字段**生效：推入栈的页面位于它下面的页面之上，打开的对话框位于页面之上。它省略的字段取自下面那个，最终取自应用的默认值。页面被弹出或对话框关闭时，下面那个重新生效。

```rust tabs
nav_page()
    .title("Inbox")
    .status_bar(StatusBarStyle::Dark);

// Pushed over it: hidden, with the inbox's dark content when it shows again.
nav_page()
    .title("Photo")
    .status_bar(StatusBarSetting::default().hidden(true));
```

```tsx tabs
<NavPage title="Inbox" statusBar={{ foreground: "dark" }}>…</NavPage>

// 推入在它之上：隐藏状态栏；再次显示时沿用收件箱的深色内容。
<NavPage title="Photo" statusBar={{ hidden: true }}>…</NavPage>
```

<a id="the-apps-default"></a>

### 应用的默认值

`nsh.json` 的 `"statusBar"` 是所有页面和对话框之下的设置，就像 UIKit 应用中 Info.plist 的 `UIStatusBarStyle` 和 `UIStatusBarHidden`：

```json
{
  "statusBar": { "foreground": "light", "hidden": false }
}
```

两个字段都可省略。不设置时，内容为 `"auto"`，状态栏显示。打包的 iOS 应用在启动屏上也会使用它。

<a id="the-status-bar-on-each-platform"></a>

### 各平台上的状态栏

- **iOS**：隐藏和显示以淡入淡出呈现。在[嵌入](../docs/embedding-ios.md)原生应用的场景下，状态栏的外观由宿主应用的视图控制器决定。宿主可以在其视图控制器中返回 `NshAppView.preferredStatusBarStyle`、`NshAppView.prefersStatusBarHidden` 和 `NshAppView.preferredStatusBarUpdateAnimation`，以应用 NSH 的设置。
- **Android**：应用以 edge-to-edge 方式绘制到状态栏之下（Android 15，即 API 35）：应用自己的背景就是状态栏的背景，`foreground` 让状态栏的内容在其上保持清晰。
- **桌面平台与 Web**：窗口标题栏和浏览器的栏属于系统和浏览器；`statusBar` 和 `nsh.json` 的 `"statusBar"` 用于手机。

> [!IMPORTANT]
> 如果你的 `nsh.json` 声明了 `"capabilities"`，只有其中包含 `"statusBar": true` 时状态栏才会改变，其他更改会被拒绝并记录日志。参见[沙箱与权限](../docs/sandbox-and-capabilities.md)。

```json
{
  "capabilities": {
    "statusBar": true
  }
}
```

<a id="related"></a>

## 相关内容

- [导航指南](../docs/navigation.md)。
- [表单与键盘](../docs/forms-and-keyboard.md)。
- [Button](../elements/button.md)、[NavStack](../elements/nav-stack.md)、[Dialog](../elements/dialog.md)、[SafeAreaView](../elements/safe-area-view.md)、[KeyboardAvoidingView](../elements/keyboard-avoiding-view.md)、[AlertDialog](../elements/alert-dialog.md)、[TextView](../elements/text-view.md)、[Text](../elements/text.md) 与 [Input](../elements/input.md)。
- [设备与环境](../docs/device-and-environment.md)。
- [nsh/android](./nsh-android.md)：`BackHandler`、用于简短消息的 `Toast.show` 及其他 Android 专用 API。

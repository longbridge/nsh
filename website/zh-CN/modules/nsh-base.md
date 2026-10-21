---
title: nsh/base
description: 应用界面的无样式基础组件：按钮、导航栈与对话框、安全区、系统警告对话框、软键盘，以及页面和对话框的状态栏。
order: 2.5
demo: ui-navigation
---

# nsh/base

`nsh/base` 是应用界面的基础组件层：不带样式、负责行为、状态、焦点和无障碍的组件，各平台通用。它位于基础元素（`nsh` 和各个 `nsh/<module>` 模块）与应用自己的组件库之间，就像 GPUI Kit 中 `gpui-base` 位于 `gpui` 与带样式的组件库之间（见[模块](./index.md#three-layers)）。它提供控件（`Button`）、导航（由 `createNavStack` 创建的状态驱动、由多个 `NavPage` 组成的 `NavStack`，以及在所有内容之上展示内容的 `Dialog`）、安全区（`SafeAreaView`、`safeArea()` 和 `safeAreaInsets()`）、系统警告对话框（`AlertDialog`）、软键盘（`Keyboard`、`keyboardFrame()` 和 `KeyboardAvoidingView`）。页面和对话框还决定状态栏（`statusBar`）。

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
  SafeAreaView, safeArea, safeAreaInsets,
} from "nsh/base";
import type {
  AlertDialogAction, AlertDialogOptions, AlertDialogPresentation, AlertDialogRole, AvoidBehavior, ButtonProps,
  DialogChangeReason, DialogProps, Edge, HeaderStyle, Insets, NavPageProps, NavStackProps, NavStackState, StatusBar,
  StatusBarForeground,
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
- [Button](../elements/button.md)、[NavStack](../elements/nav-stack.md)、[Dialog](../elements/dialog.md)、[SafeAreaView](../elements/safe-area-view.md)、[KeyboardAvoidingView](../elements/keyboard-avoiding-view.md)、[AlertDialog](../elements/alert-dialog.md) 与 [Input](../elements/input.md)。
- [设备与环境](../docs/device-and-environment.md)。
- [nsh/android](./nsh-android.md)：`BackHandler`、用于简短消息的 `Toast.show` 及其他 Android 专用 API。

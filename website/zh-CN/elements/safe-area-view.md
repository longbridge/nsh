---
title: SafeAreaView
description: SafeAreaView 组件以及 safeArea 和 safeAreaInsets 访问器：让内容避开刘海、状态栏、主屏幕指示条和标题栏。
order: 23
group: base
demo: el-safe-area-view
---

# SafeAreaView

`SafeAreaView` 让它的内容避开刘海或灵动岛、状态栏、主屏幕指示条、导航标题栏以及 Mac Catalyst 的窗口标题栏。它按**自己渲染位置**的边距为自己添加内边距：在 [NavPage](./nav-stack.md) 中是页面标题栏下方的边距，在 [Dialog](./dialog.md) 中是对话框的边距，其他地方是界面的安全区。它没有自己的外观；背景和内边距由应用决定。

```rust tabs
use nsh_base::safe_area_view;
use nsh_core::ui::prelude::*;

fn app() -> impl IntoElement {
    safe_area_view()
        .flex_1()
        .bg(rgb(0xfff8c5))
        .p_4()
        .child(text("Clear of the notch").text_2xl().font_weight(FontWeight::BOLD))
}
```

```tsx tabs
import { mount, Text } from "nsh";
import { SafeAreaView } from "nsh/base";

function App() {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: "#fff8c5", padding: 16 }}>
      <Text style={{ fontSize: 24, fontWeight: "700" }}>Clear of the notch</Text>
    </SafeAreaView>
  );
}

mount(App);
```

边距会**加到**视图自己的内边距上。在示例中，内容从顶部边距再往下 16 pt 开始，背景色铺满整个区域，包括状态栏后面的区域。

<a id="props"></a>

## 属性

| 属性 | Rust | 类型 | 默认值 | 说明 |
|---|---|---|---|---|
| `edges` | `.edges(vec![Edge::Top, ..])` | `Edge[] \| null`（`"top" \| "right" \| "bottom" \| "left"`） | 全部四边 | 要添加内边距的边。 |
| `children` | `.child(..)` | `JSX.Element` | 无 | 内容。 |
| `class`、`classList`、`style`、`testID`、`onLayout`、`ref` | 样式方法、`.test_id(..)` | | | 应用到视图上。它的内边距与边距相加。 |

<a id="choosing-the-edges"></a>

## 选择边

默认四条边都添加内边距。传入 `edges` 只为其中一部分添加，比如由标签栏或你自己的底栏处理底部边距时：

```rust tabs
use nsh_base::{Edge, safe_area_view};

safe_area_view()
    .edges(vec![Edge::Top, Edge::Left, Edge::Right])
    .flex_1()
```

```tsx tabs
<SafeAreaView edges={["top", "left", "right"]} style={{ flex: 1 }}>
  …
</SafeAreaView>
```

<a id="where-to-use-it"></a>

## 使用场景

标题栏可见且不透明的 [NavPage](./nav-stack.md) 已经把内容放在标题栏下方，所以那里的顶部边距为 0。SafeAreaView 用于：

- 没有导航栈的应用的根视图，
- 设置了 `headerVisible={false}` 或 `headerTransparent` 的页面，
- 全屏 [Dialog](./dialog.md) 的内容。

<a id="safearea-and-safeareainsets"></a>

## safeArea 与 safeAreaInsets

需要具体数值时，比如自己为底栏添加内边距，可以直接读取它们。两个 TSX 访问器都返回一个 `Edges` 对象（`{ top, right, bottom, left }`，单位为点），并且是响应式的：读取它们的代码会在它们变化时重新运行，比如旋转屏幕时。

| TSX | Rust | 返回 |
|---|---|---|
| `safeArea()` | `safe_area()` | 界面的安全区：避开系统界面（刘海、主屏幕指示条、标题栏）的部分。 |
| `safeAreaInsets()` | `safe_area_view()`、`cx.insets()` | 组件渲染位置的边距：最近的页面的（标题栏下方）、最近的对话框的，否则为界面的安全区。SafeAreaView 使用的就是这些边距。在 TSX 中，在组件里调用它，它返回一个 accessor。在 Rust 中，`safe_area_view()` 会把它们应用到内容上，你的 crate 中的组件用 `cx.insets()` 读取它们。 |

```rust tabs
use nsh_core::ui::prelude::*;

fn bottom_bar() -> impl IntoElement {
    view()
        .flex_row()
        .p_2()
        .pb(move || px(8.0) + safe_area().bottom)
        .child(text("Bottom bar"))
}
```

```tsx tabs
import { Text, View } from "nsh";
import { safeArea } from "nsh/base";

function BottomBar() {
  return (
    <View style={{ flexDirection: "row", padding: 8, paddingBottom: 8 + safeArea().bottom }}>
      <Text>Bottom bar</Text>
    </View>
  );
}
```

像上面这样在闭包中读取 `safe_area()`（TSX：`safeArea()`），样式就订阅了它们：边距变化时样式随之更新。

<a id="platform-notes"></a>

## 平台说明

- **iOS**：边距来自 UIKit 的安全区：状态栏、刘海或灵动岛，以及主屏幕指示条。
- **Android**：NSH 报告系统栏和显示屏挖孔与 NSH 视图重叠的部分。NSH [嵌入](../docs/android.md)在原生工具栏下方时，视图的顶部边距为 0。
- **Mac Catalyst**：顶部边距是窗口的标题栏。
- **桌面平台（macOS、Windows、Linux、GPUI）和 Web**：页面的内容从它的 `header` 下方开始，对话框为它的内容提供自己的边距，与手机上一样。

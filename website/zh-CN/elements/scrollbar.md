---
title: Scrollbar
description: "Scrollbar 组件：为滚动视图或列表提供应用自己外观的滚动条，放在应用想放的位置；滑块跟随内容，拖动和按压会滚动内容，可在滚动时、悬停时或始终显示。"
order: 20.86
group: base
demo: el-scrollbar
---

# Scrollbar

滚动条为滚动视图或列表提供应用自己外观的滚动条：鼠标靠近时变宽的细胶囊、品牌色的轨道、固定表头旁边或横跨两个面板的滚动条。它指明
自己滚动的滚动视图，放在应用放置的任何位置。它负责行为：滑块的长度和位置跟随内容、视口和偏移（列表变长、窗口缩放时也一样）；拖动滑块
会滚动内容，按压轨道会让滑块中点移到那里；它在内容滚动时、指针悬停时或始终显示。外观属于应用：`Scrollbar` 没有自己的颜色、尺寸、
圆角或动效。

```rust tabs
use nsh_base::{ScrollbarMode, scrollbar, theme};
use nsh_core::prelude::*;

fn inbox(rows: impl IntoElement) -> impl IntoElement {
    let list = element_ref();
    view()
        .relative()
        .flex_1()
        // 滚动视图自带的指示条让位给应用的滚动条。
        .child(scroll().element_ref(&list).flex_1().scrollbar(false).child(rows))
        .child(
            scrollbar()
                .scroll(list)
                .mode(ScrollbarMode::Hover)
                .accessibility_label("消息")
                // 轨道覆盖在右边缘，滑块在其中靠右。
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(12.0))
                .items_end()
                .px(px(2.0))
                .py(px(4.0))
                .thumb_style(|s| {
                    s.w(px(6.0))
                        .rounded_full()
                        .bg(move || theme().muted_foreground)
                        .opacity(0.6)
                        .group_hover(|s| s.w(px(8.0)).opacity(0.9))
                }),
        )
}
```

```tsx tabs
import { mount, Scroll, Text, View, type YNode } from "nsh";
import { Scrollbar } from "nsh/base";

function App() {
  let list: YNode | undefined;
  return (
    <View class="relative flex-1">
      <Scroll ref={list} scrollbar={false} class="flex-1">
        {Array.from({ length: 60 }, (_, i) => (
          <Text class="px-4 py-3">{`消息 ${i + 1}`}</Text>
        ))}
      </Scroll>
      <Scrollbar
        scroll={list}
        mode="hover"
        accessibilityLabel="消息"
        class="absolute inset-y-0 right-0 w-3 items-end px-0.5 py-1 transition-opacity duration-200"
        thumbClass="w-1.5 rounded-full bg-muted-foreground opacity-60 group-hover:w-2 group-hover:opacity-90 active:w-2 active:opacity-100"
      />
    </View>
  );
}

mount(App);
```

`Scrollbar` 来自 [`nsh/base`](../modules/nsh-base.md)，在 Rust 中是 `nsh-base` crate。在 Rust 中，`scrollbar()` 创建一个滚动条，每个
prop 一个 setter（`.scroll(..)`、`.thumb_style(..)`），滚动条本身用类型化的样式方法；它的 `scroll` 接受 `ElementRef`（或
`ScrollbarTarget`）。

## 它滚动的滚动视图

`scroll` 指明滚动条跟随的滚动视图或列表：它的 `ref`。滚动视图出现在屏幕上后滚动条就会找到它，所以 `ref` 可以由同一次渲染中后面的元素
设置；把滚动条写在它的滚动视图之后（或把 `ref` 放在 signal 里）。用 `scrollbar={false}` 关闭滚动视图自带的指示条，用 `class` 放置
滚动条：放在包住两者的 `relative` 容器里，覆盖在滚动视图的边缘，或放在屏幕上任何其他位置。

- 滑块的长度是视口显示的内容所占的比例，沿轨道的内容区（在内边距之内）计算，不短于 `minThumbLength`。它的位置是内容滚动的程度；内容
  越过一端回弹时，滑块停在那一端。
- 拖动滑块时，内容滚动的量与滑块移动的量相对应。按压轨道会让滑块中点移到指针下方，之后的拖动从那里继续。
- 纵向滚动条跟随纵向滚动视图；`axis="horizontal"` 为横向滚动视图提供滚动条（沿底边放置）。每个方向一个滚动条。
- 列表同样适用：随着行被测量和加入，滑块跟随列表的总长度。

## 何时显示

| `mode` | 显示 |
|---|---|
| `"scrolling"`（默认） | 内容滚动时，以及停止后的 `hideDelay` 内。 |
| `"hover"` | 指针在滚动视图上时，以及 `"scrolling"` 显示的时候。 |
| `"always"` | 始终。 |

在任何模式下，指针在滚动条上、拖动滑块时它都显示，之后再保持 `hideDelay`。内容能完整放进视口时，它保持隐藏。隐藏时滚动条的
`opacity` 为 0，按压穿过它交给下面的视图：在它的 `class` 上加 `transition-opacity` 即可淡入淡出。

## Props

| Prop | Rust | 类型 | 默认值 | 说明 |
|---|---|---|---|---|
| `scroll` | `.scroll(..)` | `YNode` | 无 | 滚动条跟随并滚动的滚动视图或列表（它的 `ref`）。 |
| `axis` | `.axis(..)` | `"horizontal" \| "vertical"` | `"vertical"` | 滚动视图的方向：轨道沿它延伸。创建滚动条时读取。 |
| `mode` | `.mode(..)` | `"scrolling" \| "hover" \| "always"` | `"scrolling"` | 滚动条何时显示。 |
| `hideDelay` | `.hide_delay(..)` | `number` | `1000` | 内容停止滚动、指针离开或拖动结束后，滚动条保持显示的时长，单位毫秒。 |
| `minThumbLength` | `.min_thumb_length(..)` | `number` | `24` | 内容很长时滑块的最短长度，让它保持容易抓取。 |
| `thumbClass`、`thumbStyle` | `.thumb_style(\|s\| ..)` | `string`、`StyleProp<ViewStyle>` | 无 | 滑块。 |
| `accessibilityLabel` | `.accessibility_label(..)` | `string` | 无 | 屏幕阅读器对滚动条读出的内容。 |
| `class`、`style` | 样式方法 | `string`、`StyleProp<ViewStyle>` | 无 | 滚动条本身：它的轨道，放在应用指定的位置。 |

## 部件

| 部件 | 位于 | 样式 |
|---|---|---|
| 轨道 | 应用放置的位置 | `class`、`style` |
| 滑块 | 轨道的内容区 | `thumbClass`、`thumbStyle` |

滚动条沿方向设置滑块的长度并移动它；滑块的粗细和它在轨道横向上的位置由应用决定（轨道上的 `w-1.5` 和 `items-end` 让 6 点宽的滑块靠
右）。轨道的内边距让滑块与两端和两侧保持距离。

部件呈现的状态：

- 轨道：指针下方时 `hover:`；它是一个 `group`，所以滑块可以从它获得 `group-hover:`（指针靠近时变宽的滑块）。
- 滑块：指针下方时 `hover:`，拖动时 `active:`。

滚动条的可见性就是它的不透明度：颜色的透明度请放在轨道背景或滑块上。动效由应用选择（淡入淡出用 `transition-opacity`，变宽的滑块用
`transition-all`），`motion-reduce:` 为希望减少动效的人保持静止。

## 无障碍

- 屏幕阅读器滚动的仍是滚动视图：它们用自己的手势移动它并读出其中的内容。
- 滚动条由 `accessibilityLabel` 命名，供能到达它的屏幕阅读器读出。

标签、角色和操作的一般说明见[无障碍](../docs/accessibility.md)。

## 平台说明

- 每个平台都绘制应用设置的样式，滑块跟随各平台为滚动视图报告的偏移。拖动滑块和按压轨道支持鼠标、触控笔和手指。
- `"hover"` 跟随鼠标或触控板的指针（桌面平台和 Web）；在触摸屏上，滚动条在内容滚动时显示。

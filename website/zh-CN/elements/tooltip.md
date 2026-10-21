---
title: Tooltip
description: Tooltip 组件：显示在触发元素旁的提示，在桌面上指针停留时出现，在触摸屏上手指按住时出现，样式由应用决定。
order: 28
group: base
---

# Tooltip

工具提示是显示在所说明内容旁边的简短提示：图标按钮的作用、被截断的标签的完整文字。

- 在桌面平台和桌面浏览器中，指针在触发元素上停留 `delay`（500 毫秒）后显示，指针离开或按下时隐藏。
- 在手机和平板上，手指按住触发元素时显示，抬起后还会停留片刻。

它从不接收指针或按压，所以触发元素的行为与没有它时完全一样。

```rust tabs
use nsh_base::{Tooltip, TooltipProps};
use nsh_core::ui::prelude::*;

fn save_button(on_save: impl Fn() + 'static) -> impl IntoElement {
    Tooltip::element(TooltipProps {
        offset: px(6.0).into(),
        ..TooltipProps::default()
    })
    .fill(
        "content",
        view().rounded_md().bg(rgb(0x111827)).px_2().py_1().child(text("Save the document").text_color(white())),
    )
    .child(view().on_press(on_save).p_2().child(text("💾")))
}
```

```tsx tabs
import { Text, View } from "nsh";
import { Tooltip } from "nsh/base";

function SaveButton(props: { onSave: () => void }) {
  return (
    <Tooltip
      offset={6}
      popupClass="rounded-md bg-gray-900 px-2 py-1"
      content={<Text class="text-white">Save the document</Text>}
    >
      <View onPress={props.onSave} accessibilityLabel="Save" class="p-2">
        <Text>💾</Text>
      </View>
    </Tooltip>
  );
}
```

`Tooltip` 来自 [`nsh/base`](../modules/nsh-base.md)。它默认显示在触发元素上方（`placement`），空间不够时换到另一侧，并始终位于安全区之内。

<a id="props"></a>

## 属性

| 属性 | Rust | 类型 | 默认值 | 说明 |
|---|---|---|---|---|
| `content` | `.fill("content", ..)` | `JSX.Element` | 无 | 提示的内容。 |
| `children` | `.child(..)` | `JSX.Element` | 无 | 触发元素。 |
| `open` | `open` | `boolean` | 无 | 是否显示，由应用控制。 |
| `defaultOpen` | `default_open` | `boolean` | `false` | 没有 `open` 时，初始是否显示。 |
| `delay` | `delay` | `number` | `500` | 桌面：指针在触发元素上停留多少毫秒后显示。 |
| `disabled` | `disabled` | `boolean` | `false` | 它从不显示。 |
| `placement` | `placement` | `"top" \| "bottom" \| "left" \| "right"` | `"top"` | 它位于触发元素的哪一侧。 |
| `align` | `align` | `"start" \| "center" \| "end"` | `"center"` | 它沿这一侧如何与触发元素对齐。 |
| `offset` | `offset` | `number` | `0` | 触发元素与工具提示之间的间距。 |
| `margin` | `margin` | `number` | `0` | 它与安全区边缘保持的距离。 |
| `popupClass`、`popupStyle` | `popup_class`、`popup_style` | `string`、`StyleProp<ViewStyle>` | 无 | 装着内容的弹出层的外观。 |

<a id="events"></a>

## 事件

| 事件 | Rust | 说明 |
|---|---|---|
| `onOpenChange` | `TooltipEvent::OpenChange(open, reason)` | 它显示或隐藏了：`"hover"`、`"longPress"` 或 `"triggerPress"`。 |

<a id="accessibility"></a>

## 无障碍

工具提示把触发元素的说明重复给屏幕阅读器：也给触发元素设置 `accessibilityLabel`，让每个人都能得到这条提示。

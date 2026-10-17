---
title: 无障碍
description: 让应用配合 VoiceOver 和 TalkBack 使用：标签、角色、状态、值、自定义操作、分组与隐藏、播报与焦点、文字大小以及减少动态效果。
order: 48
group: ui
---

# 无障碍

NSH 用原生视图渲染，所以平台的辅助技术（iOS 和 Mac Catalyst 上的 VoiceOver、Android 上的 TalkBack、切换控制、语音控制等）看到的是应用真实的视图层级。你需要补充的是这些技术无法推断的描述：元素是什么、说什么、处于什么状态、能做什么。所有元素都接受下面的属性。

## 无需任何操作就能得到的

- **文本会被朗读。** `<Text>` 是一个无障碍元素，标签就是它的文字。没有标签的视图会朗读其内部的文字。
- **可按压视图是元素。** 带 `onPress` 或 `onLongPress` 的视图默认是无障碍元素，激活它（在 VoiceOver 或 TalkBack 下双击）就会按下它。
- **输入框是文本框**，带有占位文字、值和键盘。
- **文字跟随用户的大小设置。** 见[文字大小](#文字大小)。

## 标签、提示与角色

```tsx
<View
  onPress={play}
  accessibilityRole="button"
  accessibilityLabel="Play"
  accessibilityHint="Starts the song"
  style={{ width: 44, height: 44, alignItems: "center", justifyContent: "center" }}
>
  <Text>▶</Text>
</View>
```

| 属性 | 类型 | 说明 |
|---|---|---|
| `accessible` | `boolean` | 视图作为一个整体元素提供给辅助技术；子元素不单独暴露。 |
| `accessibilityLabel` | `string` | 屏幕阅读器朗读的内容，代替内部的文字。 |
| `accessibilityHint` | `string` | 激活元素会发生什么，在停顿后朗读。 |
| `accessibilityRole`（或 `role`） | `AccessibilityRole` | 元素是什么。 |
| `accessibilityState` | `AccessibilityState` | 它的状态：禁用、选中、勾选、忙碌、展开。 |
| `accessibilityValue` | `{ min?, max?, now?, text? }` | 范围类或文本类元素的值。 |
| `accessibilityActions` | `{ name, label? }[]` | 提供给辅助技术的操作。 |
| `onAccessibilityAction` | `(e: { actionName }) => void` | 执行了某个操作。 |
| `accessibilityElementsHidden` | `boolean` | 对辅助技术隐藏该视图及其子树。 |
| `importantForAccessibility` | `"auto" \| "yes" \| "no" \| "no-hide-descendants"` | 视图（及其子树）是否暴露。 |
| `android={{ accessibilityLiveRegion }}` | `"none" \| "polite" \| "assertive"` | Android：视图的变化如何播报给 TalkBack。 |
| `testID` | `string` | UI 测试用的标识（iOS 上的 accessibility identifier，Android 上的 resource id）；不会被朗读。 |

给每个只有图标的控件加上标签：屏幕阅读器无法朗读一个符号或一张图片。标签保持简短，不要包含角色（写 "Play"，不要写 "Play button"）：角色会单独播报。

### 角色

`accessibilityRole` 告诉用户这是哪一类元素、如何操作。取值有 `"none"`、`"button"`、`"link"`、`"search"`、`"image"`、`"imagebutton"`、`"keyboardkey"`、`"text"`、`"adjustable"`、`"header"`、`"summary"`、`"alert"`、`"checkbox"`、`"combobox"`、`"menu"`、`"menubar"`、`"menuitem"`、`"progressbar"`、`"radio"`、`"radiogroup"`、`"scrollbar"`、`"spinbutton"`、`"switch"`、`"tab"`、`"tablist"`、`"timer"`、`"toolbar"`、`"list"` 和 `"grid"`。

最常用的几个：

- `"button"` 用于不是链接的可按压元素；`"link"` 用于跳转到内容。
- `"header"` 用于分区标题：屏幕阅读器用户会在标题之间跳转来快速浏览页面。
- `"switch"`、`"checkbox"`、`"radio"` 配合 `accessibilityState.checked`。
- `"adjustable"` 用于滑块和步进器，配合 `accessibilityValue` 以及 `increment` / `decrement` 操作。
- `"image"` 用于有意义的图片，并加上标签；装饰性图片应当隐藏。

```tsx
<Text accessibilityRole="header" style={{ fontSize: 22, fontWeight: "700" }}>Settings</Text>
```

### 状态与值

```tsx
// 用视图画出来的开关。
<View
  onPress={() => on.set(!on())}
  accessibilityRole="switch"
  accessibilityLabel="Wi-Fi"
  accessibilityState={{ checked: on() }}
  hitSlop={8}
  class="h-[31px] w-[51px] rounded-full p-0.5 transition-colors"
  classList={{ "bg-green-500": on(), "bg-zinc-300": !on() }}
>
  <View class="size-[27px] rounded-full bg-white transition-transform" classList={{ "translate-x-5": on() }} />
</View>
```

`accessibilityState` 为 `{ disabled?, selected?, checked?: boolean | "mixed", busy?, expanded? }`。`disabled` 属性也意味着禁用状态。带 `min`、`max` 和 `now` 的 `accessibilityValue` 以百分比朗读；带 `text` 时按原文朗读（"3 of 10"、"Medium"）。

## 操作

可按压视图已经支持激活。其他操作需要声明并处理：

```tsx
function Quantity() {
  const n = signal(1);
  return (
    <View
      accessible
      accessibilityRole="adjustable"
      accessibilityLabel="Quantity"
      accessibilityValue={{ text: String(n()) }}
      accessibilityActions={[{ name: "increment" }, { name: "decrement" }]}
      onAccessibilityAction={(e) => {
        if (e.actionName === "increment") n.update((v) => v + 1);
        if (e.actionName === "decrement") n.update((v) => Math.max(0, v - 1));
      }}
      style={{ flexDirection: "row", alignItems: "center", gap: 12 }}
    >
      <View onPress={() => n.update((v) => Math.max(0, v - 1))}><Text>−</Text></View>
      <Text>{n()}</Text>
      <View onPress={() => n.update((v) => v + 1)}><Text>+</Text></View>
    </View>
  );
}
```

有些操作名称是标准的，对应平台自己的手势：

| 名称 | iOS（VoiceOver） | Android（TalkBack） |
|---|---|---|
| `activate` | 双击 | 双击（点击） |
| `increment`、`decrement` | 在可调整元素上向上 / 向下轻扫 | 向前 / 向后滚动 |
| `longpress` | | 双击并按住 |
| `expand`、`collapse` | | 展开 / 收起 |
| `magicTap` | 双指双击 | |
| `escape` | 双指 Z 字形擦除 | |

其他名称都成为自定义操作，以 `label`（或名称）列在 VoiceOver 的操作转子和 TalkBack 的操作菜单中。自定义操作是滑动删除之类手势的无障碍替代：

```tsx
<View
  accessibilityActions={[{ name: "archive", label: "Archive" }, { name: "delete", label: "Delete" }]}
  onAccessibilityAction={(e) => (e.actionName === "archive" ? archive(mail.id) : remove(mail.id))}
>
  …
</View>
```

## 分组与隐藏

`accessible` 让一个视图成为一个整体元素：带标题、副标题和价格的卡片会一次读完，而不需要用户在三段文字之间来回轻扫。

```tsx
<View accessible onPress={open} accessibilityRole="button" style={{ padding: 16 }}>
  <Text>Blue jacket</Text>
  <Text>Size M</Text>
  <Text>$120</Text>
</View>
```

隐藏没有信息量的东西：装饰性图片、分隔线、旁边文字已经表达了同样意思的图标。

```tsx
<Image src={decoration} accessibilityElementsHidden importantForAccessibility="no-hide-descendants" />
```

`accessibilityElementsHidden`（iOS 的叫法）和 `importantForAccessibility="no-hide-descendants"`（Android 的叫法）在两个平台上都会隐藏子树；两者同时设置也没有坏处。

## AccessibilityInfo

`nsh/accessibility` 报告用户的设置，并让你与屏幕阅读器交互。

| 成员 | 说明 |
|---|---|
| `screenReaderEnabled()` | VoiceOver 或 TalkBack 是否在运行（响应式）。 |
| `reduceMotion()` | 用户是否要求减少动态效果（响应式）。 |
| `AccessibilityInfo.isScreenReaderEnabled()` | 同上，以 promise 形式返回。 |
| `AccessibilityInfo.isReduceMotionEnabled()` | 同上，以 promise 形式返回。 |
| `AccessibilityInfo.addEventListener(event, listener)` | `"screenReaderChanged"` 或 `"reduceMotionChanged"`；返回 `{ remove() }`。 |
| `AccessibilityInfo.announceForAccessibility(message)` | 屏幕阅读器朗读 `message`。 |
| `AccessibilityInfo.setAccessibilityFocus(ref)` | 把屏幕阅读器的焦点移到某个视图。 |

在组件中优先使用响应式访问器：它们会更新读取它们的地方，而且不需要清理。

```tsx
import { signal, Text, View, type YNode } from "nsh";
import { AccessibilityInfo, screenReaderEnabled } from "nsh/accessibility";

function Upload() {
  const done = signal(false);
  let result!: YNode;
  const finish = () => {
    done.set(true);
    AccessibilityInfo.announceForAccessibility("Upload complete");
    AccessibilityInfo.setAccessibilityFocus(result);
  };
  return (
    <View style={{ gap: 12 }}>
      {/* 自动轮播在屏幕阅读器下很难使用：改为显示列表。 */}
      <If when={!screenReaderEnabled()} else={<PhotoList />}>
        <PhotoCarousel />
      </If>
      <Text ref={result}>{done() ? "Uploaded 3 photos" : "Uploading…"}</Text>
    </View>
  );
}
```

播报那些用户否则会错过的变化：后台保存完成、在焦点之外出现的错误、计时结束。在 Android 上，给视图设置 `android={{ accessibilityLiveRegion: "polite" }}`，TalkBack 就会自动播报它的变化。

## 文字大小

除非设置 `allowFontScaling={false}`，文本和输入框都会跟随系统文字大小（iOS 的动态字体，Android 的字体大小）。与其关闭它，不如为它设计：

- 让文字可以换行：包含文字的视图不要设置固定高度。
- 对特别大的标题用 `maxFontSizeMultiplier` 设置上限，而不是禁用缩放。
- 文字很大时切换布局：读取 `nsh/device` 中的 `fontScale()`（例如超过 1.3 时把标签和值上下排列）。

见[设备与环境](./device-and-environment.md#文字大小)。

## 减少动态效果

当用户打开 iOS 的"减弱动态效果"或 Android 的"移除动画"时，`reduceMotion()` 为 `true`。NSH 不会自行关闭你的动画。用它，或 Tailwind 的 `motion-reduce:` 和 `motion-safe:` 变体，把大幅度的运动替换为淡入淡出或直接切换：

```tsx
import { View } from "nsh";
import { reduceMotion } from "nsh/accessibility";
import { LayoutAnimation } from "nsh/animated";

const toggle = () => {
  if (!reduceMotion()) LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
  expanded.set(!expanded());
};

<View onPress={open} class="transition active:scale-95 motion-reduce:transition-none" />
<View class="motion-safe:animate-bounce" />
```

## 触摸目标

可按压元素至少要有 44 × 44 点。控件必须看起来更小时，用 `hitSlop` 扩大触摸区域，而不是放大它的尺寸；见[触摸与手势](./touch-and-gestures.md#点击区域hitslop)。

## 检查你的应用

- 打开 VoiceOver（iOS 设置中的辅助功能，或设置好后三击侧边按钮）或 TalkBack，不看屏幕使用应用：每个控件都应该说明它是什么、做什么。
- 在 iOS 模拟器上，Xcode 的 Accessibility Inspector 可以查看每个元素的标签、特征和值。
- 试试最大的文字大小和深色模式。
- `testID` 为 UI 测试提供一个稳定的标识，不会随标签的语言变化。

## 相关内容

- [`nsh/accessibility`](../modules/nsh-accessibility.md)：参考。
- [`View`](../elements/view.md)：所有无障碍属性。
- [动画](./animation.md#减少动态效果) 与 [设备与环境](./device-and-environment.md)。

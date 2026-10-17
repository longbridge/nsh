---
title: 布局
description: 默认纵向排列的 Flexbox、CSS 网格、尺寸与百分比、宽高比、绝对定位、用 onLayout 测量、响应式布局以及从右到左布局。
order: 34
group: ui
demo: guide-layout
---

# 布局

NSH 使用 [Taffy](https://github.com/DioxusLabs/taffy) 为每个元素做布局。Taffy 是 CSS flexbox、网格和块布局的 Rust 实现，NSH 对它做了配置，让子元素默认纵向排列且不收缩。布局在核心中运行，发生在每一轮 JavaScript 执行之后，只有发生变化的 frame 会被发送到原生视图。文字由平台的文本引擎在同一线程上测量，因此布局从不需要等待主线程。

下面的布局属性都是样式属性，每个都有对应的 [Tailwind](./tailwind.md) 写法（`flex-row`、`gap-4`、`grid-cols-3`、`absolute inset-0` 等）。

## Flexbox

视图用 flexbox 排列子元素。默认值与 Web 不同：

| 属性 | NSH 默认值 | Web 默认值 |
|---|---|---|
| `flexDirection` | `"column"` | `row` |
| `flexShrink` | `0` | `1` |
| `alignContent` | `"flex-start"` | `stretch` |
| `alignItems` | `"stretch"` | `stretch` |
| `minWidth` / `minHeight` | `0` | `auto` |
| `position` | `"relative"` | `static` |

```tsx
<View style={{ flexDirection: "row", alignItems: "center", gap: 12, padding: 16 }}>
  <Image src={avatar} style={{ width: 40, height: 40, borderRadius: 20 }} />
  <View style={{ flex: 1, gap: 2 }}>
    <Text style={{ fontWeight: "600" }}>{props.name}</Text>
    <Text style={{ color: "#6e7781" }} numberOfLines={1}>{props.message}</Text>
  </View>
  <Text style={{ color: "#6e7781" }}>{props.time}</Text>
</View>
```

| 属性 | 取值 |
|---|---|
| `flexDirection` | `"column"`、`"row"`、`"column-reverse"`、`"row-reverse"` |
| `justifyContent` | 主轴：`"flex-start"`、`"flex-end"`、`"center"`、`"space-between"`、`"space-around"`、`"space-evenly"`、`"stretch"` |
| `alignItems` | 交叉轴：`"stretch"`、`"flex-start"`、`"flex-end"`、`"center"`、`"baseline"` |
| `alignSelf` | 单个子元素在交叉轴上的对齐；`"auto"` 跟随父元素的 `alignItems` |
| `alignContent` | 换行容器中各行的对齐 |
| `flexWrap` | `"nowrap"`、`"wrap"`、`"wrap-reverse"` |
| `flex` | 单个数字的简写（见下文） |
| `flexGrow`、`flexShrink`、`flexBasis` | 与 CSS 相同 |
| `gap`、`rowGap`、`columnGap` | 子元素之间的间距 |

`flex` 只取一个数字，不是 CSS 简写：正数 `n` 表示按 `n` 增长、按 1 收缩、基准尺寸为 0，因此 `flex: 1` 的兄弟元素无论内容多少都会平分空间；`flex: 0`（默认值）让视图按内容确定大小；负数表示按该值收缩，直到最小尺寸。

```tsx
<View style={{ flexDirection: "row", gap: 8 }}>
  <View style={{ flex: 1, height: 36, backgroundColor: "#c8e1ff" }} />   {/* one share */}
  <View style={{ flex: 2, height: 36, backgroundColor: "#79b8ff" }} />   {/* two shares */}
  <View style={{ width: 36, height: 36, backgroundColor: "#0366d6" }} /> {/* fixed */}
</View>
```

由于最小尺寸默认为 0，行内 `flex: 1` 的文字会收缩并换行，而不会把兄弟元素挤出去。

> [!NOTE]
> 样式中的 `display: "flex"` 仍保持纵向默认值。只有 Tailwind 的 `flex` 类才像 CSS 那样表示横向（[Tailwind](./tailwind.md)）。

## 尺寸

| 属性 | 取值 |
|---|---|
| `width`、`height` | 点、相对父元素的百分比（`"50%"`）或 `"auto"` |
| `minWidth`、`maxWidth`、`minHeight`、`maxHeight` | 点或百分比 |
| `aspectRatio` | 宽除以高：`1`、`16 / 9` |
| `padding`、`paddingHorizontal`、`paddingVertical`、`paddingTop` 等 | 点或百分比 |
| `margin`、`marginHorizontal`、`marginVertical`、`marginTop` 等 | 点、百分比或 `"auto"` |

不指定尺寸时，视图的大小等于内容（加上内边距），并因默认的 `alignItems: "stretch"` 在父元素的交叉轴上被拉伸。百分比相对于父元素的尺寸（内边距和外边距与 CSS 一样，相对于父元素的宽度）。

`aspectRatio` 根据已知的一边计算另一边：

```tsx
<View style={{ width: "100%", aspectRatio: 16 / 9, backgroundColor: "#000000" }} />
```

从文件导入的图片有自然尺寸，未指定尺寸时按自然尺寸布局（[图片与资源](./images-and-assets.md)）。远程图片在加载完成前没有尺寸：请为它们指定尺寸。

## 网格

`display: "grid"` 把子元素排进 CSS 网格轨道：

```tsx
<View style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 8 }}>
  <View style={{ gridColumn: "span 2", height: 60, backgroundColor: "#d8b9ff" }} />
  <View style={{ height: 60, backgroundColor: "#b083f0" }} />
  <View style={{ gridRow: "span 2", backgroundColor: "#8250df" }} />
  <View style={{ gridColumn: "2 / 4", height: 60, backgroundColor: "#6639ba" }} />
</View>
```

| 属性 | 取值 |
|---|---|
| `gridTemplateColumns`、`gridTemplateRows` | CSS 轨道列表：`"repeat(3, 1fr)"`、`"200px 1fr"`、`minmax()`、`auto-fill` / `auto-fit`、线名 |
| `gridAutoColumns`、`gridAutoRows` | 隐式创建的列和行的轨道 |
| `gridAutoFlow` | `"row"`、`"column"`、`"dense"`、`"row dense"`、`"column dense"` |
| `gridColumn`、`gridRow` | `"1 / 3"`、`"span 2"`、线编号或线名 |
| `gridColumnStart`、`gridColumnEnd`、`gridRowStart`、`gridRowEnd` | 线编号或线名 |
| `justifyItems`、`justifySelf` | 项目在其区域内沿行方向的位置 |

`display: "block"` 使用 CSS 块布局（子元素依次堆叠，每个都与容器同宽），`display: "none"` 则把视图从布局中移除。

## 绝对定位

`position: "absolute"` 让视图脱离文档流，并根据 `top`、`right`、`bottom`、`left` 相对父元素定位：

```tsx
<View style={{ height: 200 }}>
  <Image src={cover} style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0 }} />
  <View style={{ position: "absolute", left: 12, right: 12, bottom: 12 }}>
    <Text style={{ color: "#ffffff", fontWeight: "700" }}>Caption</Text>
  </View>
</View>
```

偏移量接受点和百分比。同时设置了 `left` 和 `right`（或 `top` 和 `bottom`）而没有设置尺寸的视图，会在两者之间拉伸。靠后的兄弟元素绘制在靠前的之上；`zIndex` 可以改变这个顺序。

## 测量

`onLayout` 在每次布局导致视图 frame 变化后报告它相对父元素的 frame：

```tsx
const width = signal(0);

<View onLayout={(e) => width.set(e.width)}>
  <Text>{width()} points wide</Text>
</View>
```

事件内容为 `{ x, y, width, height }`，单位是点。要获取整个窗口的大小，读取 [`nsh/device`](../modules/nsh-device.md) 中的 `windowSize()`，这个访问器会跟随旋转和窗口缩放变化：

```tsx
import { View } from "nsh";
import { windowSize } from "nsh/device";

<View style={{ flexDirection: windowSize().width > 600 ? "row" : "column" }} />
```

Tailwind 的断点（`sm:`、`md:` 等）以及 `portrait:` / `landscape:` 不用写代码就能达到同样的效果。

## 安全区域

根视图覆盖整个屏幕，包括状态栏和主屏幕指示条下方的区域。把页面包在 `nsh/ui/mobile` 的 [`SafeAreaView`](../components/safe-area-view.md) 中，它会按安全区域的 inset 为内容加内边距；也可以用 `safeArea()` / `useSafeAreaInsets()` 读取 inset 自行处理。[导航栈](./navigation.md)中的页面本身已经位于标题栏下方。

## 从右到左

`nsh/device` 中的 `I18nManager` 控制整个应用的布局方向。当应用以从右到左方式布局时（系统语言为从右到左的语言，或调用了 `I18nManager.forceRTL(true)`）：

- 行从右侧开始排列；
- `left` 和 `right` 属性（外边距、内边距、位置、边框、圆角）左右互换；
- `start` 和 `end` 属性始终表示起始侧和结束侧：`marginStart`、`marginEnd`、`paddingStart`、`paddingEnd`、`start`、`end`、`borderStartWidth`、`borderTopStartRadius` 等；
- `textAlign: "auto"` 的文字右对齐，原生控件也会镜像。

```tsx
import { Text, View } from "nsh";
import { I18nManager } from "nsh/device";

<View style={{ flexDirection: "row", gap: 8 }}>
  <View style={{ marginStart: 16, borderTopStartRadius: 12, width: 44, height: 44, backgroundColor: "#54aeff" }} />
</View>

<View onPress={() => I18nManager.forceRTL(!I18nManager.isRTL)}>
  <Text>{I18nManager.isRTL ? "Right to left" : "Left to right"}</Text>
</View>
```

`I18nManager.isRTL` 是响应式的，修改会立即生效，无需重启。`direction` 样式（`"inherit"`、`"ltr"`、`"rtl"`）可以为单个视图及其子元素覆盖方向。在 Android 上，嵌入 NSH 的应用需要在 manifest 中设置 `android:supportsRtl="true"`，原生控件才会镜像。

## 相关内容

- [样式](./styling.md)：样式如何组合。
- [滚动](./scrolling.md)和[列表](./lists.md)：超出屏幕的内容。
- [样式参考](../modules/style-reference.md)：所有布局属性及其类型。

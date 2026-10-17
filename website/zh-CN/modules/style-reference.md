---
title: 样式参考
description: NSH 支持的全部样式属性，按 LayoutStyle、ViewStyle、TextStyle 和 ImageStyle 分组，列出类型与含义。
order: 16
---

# 样式参考

本页列出 `style` 属性可用的每一个键。键名和类型来自生成的声明文件 `.nsh/types/nsh/types.d.ts`，编辑器会在你输入时检查它们。布局是默认纵向排列的 flexbox，数字的单位是点，同时支持这些 CSS 能力：网格、渐变、盒阴影、滤镜、混合模式、轮廓线、过渡和关键帧动画。

样式怎么写、怎样与 class 组合、文字样式如何继承，见 [样式](../docs/styling.md)。class 的语法见 [Tailwind 参考](./tailwind-reference.md)。

## 四个接口

每个接口都扩展前一个：

| 接口 | 扩展自 | 用于 |
|---|---|---|
| `LayoutStyle` | | 所有元素的布局（flexbox、网格、尺寸、间距、定位） |
| `ViewStyle` | `LayoutStyle` | `<View>`、`<Scroll>`、`<List>`、控件及其他所有元素 |
| `TextStyle` | `ViewStyle` | `<Text>` 和 `<Input>`（以及视图：视图上的文字样式会传给其中的文字） |
| `ImageStyle` | `ViewStyle` | `<Image>` |

```tsx
import type { ViewStyle, TextStyle } from "nsh";
import { Text, View } from "nsh";

const card: ViewStyle = {
  padding: 16,
  borderRadius: 12,
  backgroundColor: "#fff",
  boxShadow: "0 4px 12px rgb(0 0 0 / 0.08)",
};

const title: TextStyle = { fontSize: 17, fontWeight: "600", color: "#111" };

<View style={card}>
  <Text style={title}>Hello</Text>
</View>;
```

`style` 也接受数组：各项从左到右合并，假值会被跳过，因此条件样式无需展开运算：

```tsx
<View style={[card, selected() && { borderColor: "#2563eb", borderWidth: 2 }]} />
```

> [!NOTE]
> 内联 `style` 总是覆盖 `class`。把某个键设为 `null` 或 `undefined` 会将其恢复为默认值。

## 值类型

| 类型 | 接受 |
|---|---|
| `number` | 点（与密度无关）。`1` 在 iOS 上是一个 point，在 Android 上是一个 dp。 |
| `DimensionValue` | `number`（点）、`` `${number}%` ``（相对父元素）或 `"auto"`。 |
| `LengthValue` | `number`（点）或 `` `${number}%` ``，不接受 `"auto"`。 |
| `ColorValue` | CSS 颜色字符串：`#rgb`、`#rgba`、`#rrggbb`、`#rrggbbaa`、`rgb()`、`rgba()`、`hsl()`、`hsla()`、`oklch()`、`oklab()`，或 CSS 命名颜色（`black`、`white`、`transparent`、`rebeccapurple` 等）。 |
| `NativeNode` | 原生动画值，或由它派生的节点（见 [nsh/animated](./nsh-animated.md)）。`backgroundColor`、`opacity` 和 `transform` 的各步骤接受它，值会在 UI 线程上动画，不经过 JS。 |

采用 CSS 语法的字符串（`boxShadow`、`backgroundImage`、`filter`、`gridTemplateColumns` 等）中，长度可写 `px` 或裸数字（都是点）、`pt`，或 `rem`（16 点）。

## LayoutStyle

布局在 Rust 中计算（flexbox 和 CSS 网格）。默认值与 CSS 不同：`display: "flex"`、`flexDirection: "column"`、`flexShrink: 0`、`alignContent: "flex-start"`、`alignItems: "stretch"`，并且没有自动最小尺寸。见 [布局](../docs/layout.md)。

### 显示与定位

| 键 | 类型 | 说明 |
|---|---|---|
| `display` | `"flex" \| "grid" \| "block" \| "none"` | 子元素的布局方式。`"none"` 将元素移出布局并隐藏。`"block"` 按 CSS 块级布局堆叠子元素。默认 `"flex"`。 |
| `position` | `"relative" \| "absolute"` | `"absolute"` 使元素脱离文档流，按 `top` / `right` / `bottom` / `left` 相对父元素定位。默认 `"relative"`。 |
| `top` | `DimensionValue` | 距父元素上边（绝对定位），或相对自身位置的偏移（相对定位）。 |
| `right` | `DimensionValue` | 距右边的偏移。从右到左布局中表示结束边。 |
| `bottom` | `DimensionValue` | 距下边的偏移。 |
| `left` | `DimensionValue` | 距左边的偏移。从右到左布局中表示起始边。 |
| `start` | `DimensionValue` | 从左到右布局中为左，从右到左布局中为右。 |
| `end` | `DimensionValue` | 从左到右布局中为右，从右到左布局中为左。 |
| `zIndex` | `number` | 在兄弟元素间的次序（值相同时保持原顺序），也是原生视图的叠放次序。 |
| `direction` | `"inherit" \| "ltr" \| "rtl"` | 布局方向，默认继承。从右到左时，`left` 和 `right` 属性表示起始和结束。 |

### Flexbox

| 键 | 类型 | 说明 |
|---|---|---|
| `flexDirection` | `"row" \| "column" \| "row-reverse" \| "column-reverse"` | 主轴方向。默认 `"column"`。 |
| `flexWrap` | `"wrap" \| "nowrap" \| "wrap-reverse"` | 子元素是否换行。默认 `"nowrap"`。 |
| `flex` | `number` | 单个数字，不是 CSS 简写：正数会把 `flexGrow` 设为该值，同时 `flexShrink: 1`、`flexBasis: 0`；`0` 让元素不伸缩；负数让元素可以收缩到最小尺寸。 |
| `flexGrow` | `number` | 占用剩余空间的比例。默认 `0`。 |
| `flexShrink` | `number` | 空间不足时的收缩比例。默认 `0`，而不是 CSS 的 `1`。 |
| `flexBasis` | `DimensionValue` | 分配剩余空间之前的主轴尺寸。 |
| `justifyContent` | `"flex-start" \| "flex-end" \| "center" \| "space-between" \| "space-around" \| "space-evenly" \| "stretch"` | 子元素沿主轴的分布。默认 `"flex-start"`。 |
| `alignItems` | `"flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | 子元素在交叉轴上的对齐。默认 `"stretch"`。 |
| `alignSelf` | `"auto" \| "flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | 为当前元素覆盖父元素的 `alignItems`。 |
| `alignContent` | `"flex-start" \| "flex-end" \| "center" \| "stretch" \| "space-between" \| "space-around" \| "space-evenly"` | 多行（及网格轨道）在交叉轴上的分布。默认 `"flex-start"`。 |
| `gap` | `LengthValue` | 子元素行与列之间的间距。 |
| `rowGap` | `LengthValue` | 行间距。 |
| `columnGap` | `LengthValue` | 列间距。 |

### 网格

设置 `display: "grid"` 后，子元素按 CSS 网格排列。轨道列表使用 CSS 语法。

| 键 | 类型 | 说明 |
|---|---|---|
| `gridTemplateColumns` | `string` | 列轨道：`"repeat(3, 1fr)"`、`"200px 1fr"`、`minmax()`、`auto-fill` / `auto-fit`、网格线名称。 |
| `gridTemplateRows` | `string` | 行轨道，语法同上。 |
| `gridAutoColumns` | `string` | 隐式创建的列的尺寸。 |
| `gridAutoRows` | `string` | 隐式创建的行的尺寸。 |
| `gridAutoFlow` | `"row" \| "column" \| "dense" \| "row dense" \| "column dense"` | 未指定位置的项目如何填入网格。 |
| `gridColumn` | `number \| string` | 列位置：`"1 / 3"`、`"span 2"`、网格线编号或名称。 |
| `gridColumnStart` | `number \| string` | 起始列线。 |
| `gridColumnEnd` | `number \| string` | 结束列线。 |
| `gridRow` | `number \| string` | 行位置，语法同 `gridColumn`。 |
| `gridRowStart` | `number \| string` | 起始行线。 |
| `gridRowEnd` | `number \| string` | 结束行线。 |
| `justifyItems` | `"flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | 网格项在其区域内沿行方向的位置。 |
| `justifySelf` | `"auto" \| "flex-start" \| "flex-end" \| "center" \| "stretch" \| "baseline"` | 为当前项覆盖 `justifyItems`。 |

```tsx
<View style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 8 }}>
  <View style={{ gridColumn: "span 2", height: 60, backgroundColor: "#c7d2fe" }} />
  <View style={{ height: 60, backgroundColor: "#a5b4fc" }} />
</View>
```

### 尺寸

| 键 | 类型 | 说明 |
|---|---|---|
| `width` | `DimensionValue` | 宽度。 |
| `height` | `DimensionValue` | 高度。 |
| `minWidth` | `LengthValue` | 最小宽度。 |
| `minHeight` | `LengthValue` | 最小高度。 |
| `maxWidth` | `LengthValue` | 最大宽度。 |
| `maxHeight` | `LengthValue` | 最大高度。 |
| `aspectRatio` | `number` | 宽高比（宽 ÷ 高），缺少的一边由另一边算出。 |

### 外边距

外边距接受 `"auto"`（与 flexbox 中一样吸收剩余空间）和负值。

| 键 | 类型 | 说明 |
|---|---|---|
| `margin` | `DimensionValue` | 四边。 |
| `marginHorizontal` | `DimensionValue` | 左右。 |
| `marginVertical` | `DimensionValue` | 上下。 |
| `marginTop` | `DimensionValue` | 上。 |
| `marginRight` | `DimensionValue` | 右。 |
| `marginBottom` | `DimensionValue` | 下。 |
| `marginLeft` | `DimensionValue` | 左。 |
| `marginStart` | `DimensionValue` | 从左到右布局中为左，从右到左布局中为右。 |
| `marginEnd` | `DimensionValue` | 与 `marginStart` 相对的一边。 |

### 内边距

| 键 | 类型 | 说明 |
|---|---|---|
| `padding` | `LengthValue` | 四边。 |
| `paddingHorizontal` | `LengthValue` | 左右。 |
| `paddingVertical` | `LengthValue` | 上下。 |
| `paddingTop` | `LengthValue` | 上。 |
| `paddingRight` | `LengthValue` | 右。 |
| `paddingBottom` | `LengthValue` | 下。 |
| `paddingLeft` | `LengthValue` | 左。 |
| `paddingStart` | `LengthValue` | 从左到右布局中为左，从右到左布局中为右。 |
| `paddingEnd` | `LengthValue` | 与 `paddingStart` 相对的一边。 |

## ViewStyle

视图绘制的一切。`LayoutStyle` 的全部键同样适用。

### 背景

| 键 | 类型 | 说明 |
|---|---|---|
| `backgroundColor` | `ColorValue \| NativeNode` | 填充色。接受原生动画颜色。 |
| `backgroundImage` | `string` | 画在背景色之上的 CSS 渐变：`linear-gradient(...)`、`radial-gradient(...)`、`conic-gradient(...)`，多个用逗号分隔。 |
| `experimental_backgroundImage` | `string` | `backgroundImage` 的别名。 |

```tsx
<View style={{ height: 120, borderRadius: 16, backgroundImage: "linear-gradient(135deg, #6366f1, #ec4899)" }} />
```

### 边框

| 键 | 类型 | 说明 |
|---|---|---|
| `borderWidth` | `number` | 四边宽度。 |
| `borderTopWidth` | `number` | 上边。 |
| `borderRightWidth` | `number` | 右边。 |
| `borderBottomWidth` | `number` | 下边。 |
| `borderLeftWidth` | `number` | 左边。 |
| `borderStartWidth` | `number` | 起始边（从左到右布局中为左）。 |
| `borderEndWidth` | `number` | 结束边。 |
| `borderColor` | `ColorValue` | 四边颜色。 |
| `borderTopColor` | `ColorValue` | 上边。 |
| `borderRightColor` | `ColorValue` | 右边。 |
| `borderBottomColor` | `ColorValue` | 下边。 |
| `borderLeftColor` | `ColorValue` | 左边。 |
| `borderStartColor` | `ColorValue` | 起始边。 |
| `borderEndColor` | `ColorValue` | 结束边。 |
| `borderStyle` | `"solid" \| "dashed" \| "dotted"` | 线型。默认 `"solid"`。 |

### 圆角

| 键 | 类型 | 说明 |
|---|---|---|
| `borderRadius` | `number` | 四个角的半径。 |
| `borderTopLeftRadius` | `number` | 左上角。 |
| `borderTopRightRadius` | `number` | 右上角。 |
| `borderBottomRightRadius` | `number` | 右下角。 |
| `borderBottomLeftRadius` | `number` | 左下角。 |
| `borderTopStartRadius` | `number` | 起始侧的上角。 |
| `borderTopEndRadius` | `number` | 结束侧的上角。 |
| `borderBottomStartRadius` | `number` | 起始侧的下角。 |
| `borderBottomEndRadius` | `number` | 结束侧的下角。 |
| `borderCurve` | `"circular" \| "continuous"` | `"continuous"`：曲率平滑过渡到直边的圆角，类似 iOS 应用图标。盒子太小时退回普通圆角，所以胶囊形依然是胶囊形。两个平台都支持。 |

### 轮廓线

轮廓线画在边框之外，不占布局空间。

| 键 | 类型 | 说明 |
|---|---|---|
| `outlineWidth` | `number` | 线宽。 |
| `outlineColor` | `ColorValue` | 颜色。 |
| `outlineStyle` | `"solid" \| "double" \| "dashed" \| "dotted" \| "none" \| "hidden"` | 线型。 |
| `outlineOffset` | `number` | 与边框外缘的距离；负值向内。 |

### 阴影

两种方式投射阴影：CSS 的 `boxShadow`（多层、扩展、内阴影），或四个 `shadow*` 键。

| 键 | 类型 | 说明 |
|---|---|---|
| `boxShadow` | `string` | CSS `box-shadow`：偏移、模糊、扩展和颜色，多层用逗号分隔，`inset` 表示内阴影。 |
| `shadowColor` | `ColorValue` | 阴影颜色。 |
| `shadowOffset` | `{ width: number; height: number }` | 偏移。默认 `{ width: 0, height: -3 }`。 |
| `shadowOpacity` | `number` | 不透明度，0 到 1。默认 `0`（无阴影）。 |
| `shadowRadius` | `number` | 模糊半径。默认 `3`。 |

```tsx
<View style={{ padding: 16, borderRadius: 12, backgroundColor: "#fff",
  boxShadow: "0 1px 2px rgb(0 0 0 / 0.06), 0 8px 24px -4px rgb(0 0 0 / 0.12)" }} />
```

> [!NOTE]
> 在 Android 上，阴影需要背景：背景透明的视图（例如没有底色的文字）不会投射阴影。若要 Android 的 Material 阴影，请用 `android={{ elevation }}` 属性，而不是样式。

### 不透明度、可见性与裁剪

| 键 | 类型 | 说明 |
|---|---|---|
| `opacity` | `number \| NativeNode` | 0（不可见）到 1。接受原生动画值。 |
| `visibility` | `"visible" \| "hidden" \| "collapse"` | 隐藏的视图仍占据布局空间（与 `display: "none"` 不同）。 |
| `overflow` | `"visible" \| "hidden"` | `"hidden"` 将子元素裁剪到视图边界（含圆角）。 |
| `backfaceVisibility` | `"visible" \| "hidden"` | `"hidden"`：3D 旋转转到背面时不可见。 |

### 变换

| 键 | 类型 | 说明 |
|---|---|---|
| `transform` | `TransformStep[]` | 步骤列表，与 CSS 一样从右向左应用。 |
| `transformOrigin` | `string \| (number \| string)[]` | `transform` 的作用中心：关键字、百分比和长度（`"top left"`、`"50% 0"`、`[10, 20]`）。默认为中心。 |

`TransformStep` 是只有一个键的对象：

| 步骤 | 值 | 原生动画 |
|---|---|---|
| `translateX`、`translateY` | `number`（点） | 支持 |
| `scale`、`scaleX`、`scaleY` | `number`（倍数） | 支持 |
| `rotate`、`rotateZ`、`rotateX`、`rotateY` | `number`（度）或 `string`（`"45deg"`、`"0.5rad"`） | 支持 |
| `skewX`、`skewY` | `number`（度）或 `string` | 不支持 |
| `perspective` | `number` | 不支持 |

```tsx
<View style={{ transform: [{ perspective: 600 }, { rotateY: "20deg" }, { scale: 0.95 }] }} />
```

> [!NOTE]
> Android 不支持斜切：`skewX` 和 `skewY` 在 Android 上会被忽略。

### 滤镜与混合

| 键 | 类型 | 说明 |
|---|---|---|
| `filter` | `string` | 作用于视图及其内容的 CSS 滤镜：`brightness()`、`contrast()`、`grayscale()`、`hue-rotate()`、`invert()`、`saturate()`、`sepia()`、`opacity()`、`blur()`、`drop-shadow()`。 |
| `backdropFilter` | `string` | 作用于视图背后内容的 CSS 滤镜，例如毛玻璃效果 `"blur(10px)"`。 |
| `mixBlendMode` | `"normal" \| "multiply" \| "screen" \| "overlay" \| "darken" \| "lighten" \| "color-dodge" \| "color-burn" \| "hard-light" \| "soft-light" \| "difference" \| "exclusion" \| "hue" \| "saturation" \| "color" \| "luminosity" \| "plus-darker" \| "plus-lighter"` | 视图与背后内容的混合方式。 |

> [!WARNING]
> 平台限制：
> - iOS 上视图只支持 `opacity()`、不超过 1 的 `brightness()` 和 `drop-shadow()`。`<Image>` 支持所有颜色滤镜和 `blur()`，在解码图片时应用。
> - iOS 的 `backdropFilter` 模糊使用按比例调整的系统材质，是对精确半径的近似。
> - Android 上 `blur()`、`drop-shadow()` 和 `backdropFilter` 需要 API 31；更低版本只应用颜色滤镜。混合模式需要 API 29。

### 过渡

当参与过渡的属性改变时（来自新样式、class，或 `active:` 等变体），平台以原生方式执行动画，期间不运行任何 JS。

| 键 | 类型 | 说明 |
|---|---|---|
| `transitionProperty` | `string` | 参与动画的属性，用逗号分隔：`"all"`、`"colors"`（`color`、`backgroundColor`、`borderColor` 等）、`"opacity"`、`"transform"`、`"boxShadow"`、`"borderRadius"`、`"borderWidth"`，或 `"none"`。 |
| `transitionDuration` | `number \| "${number}ms" \| "${number}s"` | 数字表示毫秒，也可写 `"150ms"` / `"0.2s"`。 |
| `transitionDelay` | `number \| "${number}ms" \| "${number}s"` | 过渡开始前的延迟。 |
| `transitionTimingFunction` | `"linear" \| "ease" \| "ease-in" \| "ease-out" \| "ease-in-out" \| "cubic-bezier(...)"` | 缓动曲线。 |

```tsx
<View
  onPress={() => open.set(!open())}
  style={{ height: open() ? 160 : 56, opacity: open() ? 1 : 0.8,
    transitionProperty: "opacity, transform", transitionDuration: 200 }}
/>
```

### 关键帧动画

内置的关键帧动画，以原生方式运行（对应 Tailwind 的 `animate-*`）。

| 键 | 类型 | 说明 |
|---|---|---|
| `animationName` | `"none" \| "spin" \| "ping" \| "pulse" \| "bounce"` | 动画名称。 |
| `animationDuration` | `number \| "${number}ms" \| "${number}s"` | 单次时长。 |
| `animationDelay` | `number \| "${number}ms" \| "${number}s"` | 首次开始前的延迟。 |
| `animationTimingFunction` | `"linear" \| "ease" \| "ease-in" \| "ease-out" \| "ease-in-out" \| "cubic-bezier(...)"` | 缓动曲线。 |
| `animationIterationCount` | `number \| "infinite"` | 重复次数。 |

由数值、手势或弹簧驱动的动画请使用 [nsh/animated](./nsh-animated.md)。

### 指针

| 键 | 类型 | 说明 |
|---|---|---|
| `cursor` | `"auto" \| "default" \| "pointer" \| "text" \| "grab" \| "grabbing" \| "not-allowed" \| ...`（所有 CSS 光标关键字及 `"none"`） | 指针悬停在视图上时的形状，适用于指针设备（iPad 触控板、Mac Catalyst、桌面）。 |

## TextStyle

文字样式用于 `<Text>` 和 `<Input>`。它们也像 CSS 一样会继承：视图上设置的文字属性会传给其中的文字和输入框，嵌套的 `<Text>` 片段继承父文字的样式。`ViewStyle` 的全部键同样适用。见 [文字](../docs/text.md)。

### 字体

| 键 | 类型 | 说明 |
|---|---|---|
| `color` | `ColorValue` | 文字颜色。 |
| `fontSize` | `number` | 字号，单位为点。 |
| `fontWeight` | `"normal" \| "bold" \| number \| "${number}"` | `"normal"`（400）、`"bold"`（700）或 100–900。 |
| `fontFamily` | `string` | 系统字体族，或导入字体得到的名字（`import Inter from "./Inter.ttf"`）。见 [字体](../docs/fonts.md)。 |
| `fontStyle` | `"normal" \| "italic"` | 斜体。 |
| `fontVariant` | `("small-caps" \| "oldstyle-nums" \| "lining-nums" \| "tabular-nums" \| "proportional-nums")[]` | OpenType 数字与字母形式。`tabular-nums` 让数字等宽，适合计数器和计时器。 |

### 间距与对齐

| 键 | 类型 | 说明 |
|---|---|---|
| `lineHeight` | `number` | 行高，单位为点。 |
| `letterSpacing` | `number` | 字符间的额外间距，单位为点。 |
| `textAlign` | `"auto" \| "left" \| "center" \| "right" \| "justify"` | 水平对齐。 |
| `textAlignVertical` | `"auto" \| "top" \| "center" \| "bottom"` | 文字比盒子矮时的位置（设置了高度的 `<Text>`）。 |
| `writingDirection` | `"auto" \| "ltr" \| "rtl"` | 文字的基础方向；`"auto"` 由内容决定。 |
| `textTransform` | `"none" \| "uppercase" \| "lowercase" \| "capitalize"` | 大小写转换。 |

> [!TIP]
> Tailwind 的 `leading-*` 和 `tracking-*` 把行高设为字号的倍数、把字间距设为 `em`，因此会随 `text-*` 字号缩放。

### 装饰线

| 键 | 类型 | 说明 |
|---|---|---|
| `textDecorationLine` | `"none" \| "underline" \| "line-through" \| "underline line-through"` | 与文字一起绘制的线。 |
| `textDecorationColor` | `ColorValue` | 线的颜色（默认为文字颜色）。 |
| `textDecorationStyle` | `"solid" \| "double" \| "dotted" \| "dashed" \| "wavy"` | 线型。 |
| `textDecorationThickness` | `number \| "auto" \| "from-font"` | 线的粗细。 |
| `textUnderlineOffset` | `number \| "auto"` | 下划线与基线的距离。 |

### 文字阴影

| 键 | 类型 | 说明 |
|---|---|---|
| `textShadowColor` | `ColorValue` | 阴影颜色。 |
| `textShadowOffset` | `{ width: number; height: number }` | 阴影偏移。 |
| `textShadowRadius` | `number` | 模糊半径。 |

> [!NOTE]
> iOS 不会在 `<Text>` 上绘制渐变（`backgroundImage`）或内阴影。

## ImageStyle

包含 `ViewStyle` 的全部键，另加：

| 键 | 类型 | 说明 |
|---|---|---|
| `tintColor` | `ColorValue` | 为模板图片重新着色（所有不透明像素都变成这个颜色），用于单色图标。 |

图片如何填充盒子由 `<Image>` 的 `resizeMode` 属性决定，不是样式；见 [image](../elements/image.md)。

## 看起来像样式的属性

有些设置在 NSH 中是属性而不是样式键。它们有对应的 Tailwind class，但显式设置的属性优先于 class：

| 设置 | 属性 | Class |
|---|---|---|
| 行数限制 | `<Text>` 的 `numberOfLines` | `line-clamp-*`、`truncate` |
| 省略位置 | `<Text>` 的 `ellipsizeMode` | `text-ellipsis`、`text-clip` |
| 触摸穿透 | `pointerEvents` | `pointer-events-none`、`pointer-events-auto` |
| 图片填充 | `<Image>` 的 `resizeMode` | `object-cover`、`object-contain` 等 |
| 文字可选 | `<Text>` 的 `selectable` | `select-text`、`select-none` |
| 占位符颜色 | `<Input>` 的 `placeholderTextColor` | `placeholder:text-*` |
| Material 阴影（Android） | `android={{ elevation }}` | |

## 相关

- [样式](../docs/styling.md)：`style`、`class`、继承与优先级。
- [Tailwind 参考](./tailwind-reference.md)：支持的全部工具类和变体。
- [布局](../docs/layout.md)：flexbox 和网格的实际用法。
- [view](../elements/view.md)、[text](../elements/text.md)、[image](../elements/image.md)：这些样式所作用的元素。

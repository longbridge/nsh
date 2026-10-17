---
title: 样式
description: style 属性：camelCase 属性，支持 flexbox、grid 和 CSS 效果，包括颜色、边框、圆角、阴影、渐变、滤镜、变换和过渡，以及各平台的差异。
order: 32
group: ui
demo: guide-styling
---

# 样式

每个元素都接受 `style` 属性：一个由 camelCase 属性组成的对象，包括盒阴影、渐变、滤镜、轮廓线、过渡和 grid 等 CSS 能力。样式在 Rust 中解析，只有变化了的属性才会发送给原生视图。你也可以用 [Tailwind 类名](./tailwind.md)设置样式，两者最终得到的是相同的原生样式。

```tsx
<View
  style={{
    padding: 16,
    borderRadius: 12,
    backgroundColor: "#ffffff",
    borderWidth: 1,
    borderColor: "#d0d7de",
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.12)",
  }}
>
  <Text style={{ fontSize: 17, fontWeight: "600", color: "#1f2328" }}>Card</Text>
</View>
```

本页介绍样式如何组合以及各种视觉属性。布局属性（flexbox、grid、尺寸、位置）见[布局](./layout.md)，文本属性见[文本](./text.md)，所有属性及其类型列在[样式参考](../modules/style-reference.md)中。

## style 属性

`style` 接受一个对象，或者一个从左到右合并的对象数组。假值项（`false`、`null`、`undefined`）会被跳过，这让条件样式写起来很简短：

```tsx
const styles = {
  button: { paddingHorizontal: 16, paddingVertical: 10, borderRadius: 10, backgroundColor: "#0969da" },
  disabled: { opacity: 0.4 },
  danger: { backgroundColor: "#cf222e" },
} as const;

<View style={[styles.button, props.danger && styles.danger, busy() && styles.disabled]} />
```

把共享的样式定义为组件外部的常量（加上 `as const`，让 TypeScript 保留 `"row"` 这样的字面量类型）。没有 `StyleSheet.create`：一个普通对象就够了，样式在每次设置时由 Rust 解析，对象无需事先注册。

读取 signal 的样式表达式会在 signal 变化时重建，NSH 只发送值不同的属性：

```tsx
<View style={{ backgroundColor: selected() ? "#ddf4ff" : "#ffffff", padding: 12 }} />
```

元素同时有 `style` 和 `class` 时，`style` 优先：先应用类名，再在其上叠加内联样式。值为 `undefined` 的键不设置任何东西（`backgroundColor: active ? accent : undefined` 在未激活时保留类名的颜色）；`null` 则设为默认值。

## 单位与取值

- **数字的单位是点**（与密度无关的像素）：`width: 100` 是 100pt，在 2× 或 3× 屏幕上分别是 200 或 300 个物理像素。
- **百分比**（`"50%"`）相对于父视图，可用于尺寸、位置、外边距和内边距（[布局](./layout.md)）。
- **角度**是以数字表示的度数，或带单位的字符串：`45`、`"45deg"`、`"0.5rad"`。
- **时长**是以数字表示的毫秒数，或字符串：`"150ms"`、`"0.2s"`。

## 默认值

NSH 的默认值与 Web 不同：

| | 默认值 |
|---|---|
| `flexDirection` | `"column"` |
| `flexShrink` | `0`（子视图不会收缩，除非显式要求） |
| `alignContent` | `"flex-start"` |
| `position` | `"relative"` |
| `minWidth` / `minHeight` | `0` |
| `backgroundColor` | 透明 |
| 文本 | 黑色，14pt，字重 400 |
| `shadowOffset` / `shadowRadius` / `shadowOpacity` | `{ width: 0, height: -3 }` / `3` / `0` |

新视图在你设置样式之前不携带任何样式，因此没有样式的包装视图在原生侧没有开销。

## 颜色

颜色属性接受 CSS 颜色字符串：

| 形式 | 示例 |
|---|---|
| 十六进制 | `"#09f"`、`"#09fc"`、`"#0099ff"`、`"#0099ffcc"` |
| `rgb()` / `rgba()` | `"rgb(0, 153, 255)"`、`"rgba(0, 0, 0, 0.5)"`、`"rgb(0 0 0 / 10%)"` |
| `hsl()` / `hsla()` | `"hsl(210 100% 50%)"` |
| `oklch()` / `oklab()` | `"oklch(62.3% 0.214 259.8)"` |
| 颜色名 | `"white"`、`"black"`、`"transparent"`、`"rebeccapurple"` 等 |

要支持深色模式，可以读取 `nsh/device` 中的 [`colorScheme()`](../modules/nsh-device.md)（一个跟随系统外观的访问器），或使用 Tailwind 的 `dark:` 变体：

```tsx
import { Text, View } from "nsh";
import { colorScheme } from "nsh/device";

const palette = () =>
  colorScheme() === "dark"
    ? { background: "#0d1117", text: "#e6edf3" }
    : { background: "#ffffff", text: "#1f2328" };

<View style={{ flex: 1, backgroundColor: palette().background }}>
  <Text style={{ color: palette().text }}>Follows the system appearance</Text>
</View>
```

两种外观下默认文本颜色都是黑色：支持深色模式的应用需要显式设置颜色。

## 边框与圆角

```tsx
<View
  style={{
    borderWidth: 1,
    borderColor: "#d0d7de",
    borderStyle: "dashed",          // "solid" | "dashed" | "dotted"
    borderRadius: 12,
    borderTopLeftRadius: 24,        // per corner, overrides borderRadius
    borderBottomWidth: 3,           // per side
    borderBottomColor: "#0969da",
  }}
/>
```

- **宽度和颜色**可按边设置：`borderTopWidth`、`borderRightWidth`……`borderTopColor`……；以及跟随方向的 `borderStartWidth`、`borderEndWidth`、`borderStartColor`、`borderEndColor`。
- **圆角半径**可按角设置：`borderTopLeftRadius`、`borderTopRightRadius`、`borderBottomRightRadius`、`borderBottomLeftRadius`，以及跟随[布局方向](./layout.md)的 `borderTopStartRadius` / `borderTopEndRadius` / `borderBottomStartRadius` / `borderBottomEndRadius`。
- **`borderCurve: "continuous"`** 绘制曲率平滑过渡到边的圆角，就像 iOS 应用图标，两个平台都支持。胶囊形状（`borderRadius` 为高度的一半）依然是胶囊。
- **`overflow: "hidden"`** 把子视图裁剪到圆角形状内。

超过边长一半的半径会被截断，所以 `borderRadius: 9999` 会得到圆形或胶囊。

## 阴影

有三种投射阴影的方式：

```tsx
// The shadow* properties
<View style={{ shadowColor: "#000", shadowOpacity: 0.2, shadowRadius: 8, shadowOffset: { width: 0, height: 4 } }} />

// CSS box-shadow: several, comma separated, with spread and inset
<View style={{ boxShadow: "0 1px 2px rgba(0,0,0,0.08), 0 8px 24px rgba(0,0,0,0.12)" }} />
<View style={{ boxShadow: "inset 0 2px 4px rgba(0,0,0,0.15)" }} />

// Android's material elevation (a prop, not a style)
<View android={{ elevation: 6 }} style={{ backgroundColor: "#ffffff", borderRadius: 12 }} />
```

`boxShadow` 遵循 CSS 语法：`offsetX offsetY blur spread color`，`inset` 表示内阴影。阴影会贴合视图的圆角形状。

> [!NOTE]
> 在 Android 上，阴影需要背景：背景透明的视图（例如没有背景的文本）不会投射阴影。`elevation` 的颜色取自 `shadowColor`。

## 渐变

`backgroundImage` 接受 CSS 渐变，绘制在背景色之上。多个渐变用逗号分隔。

```tsx
<View style={{ height: 120, borderRadius: 16, backgroundImage: "linear-gradient(135deg, #54aeff, #8250df)" }} />
<View style={{ backgroundImage: "radial-gradient(circle at top left, #fff8c5, #fd8c73)" }} />
<View style={{ backgroundImage: "conic-gradient(red, yellow, lime, aqua, blue, magenta, red)" }} />
```

也接受 `experimental_backgroundImage`，它是 `backgroundImage` 的别名。在 iOS 上，文本元素不绘制渐变。

## 滤镜与混合

| 属性 | 取值 |
|---|---|
| `filter` | 作用于视图及其内容的 CSS 滤镜：`brightness()`、`contrast()`、`grayscale()`、`hue-rotate()`、`invert()`、`saturate()`、`sepia()`、`opacity()`、`blur()`、`drop-shadow()`；多个之间用空格分隔 |
| `backdropFilter` | 作用于视图背后内容的滤镜：`"blur(10px)"`（毛玻璃） |
| `mixBlendMode` | 视图与其背后内容的混合方式：`"multiply"`、`"screen"`、`"overlay"` 等 |

```tsx
<Image src={photo} style={{ width: 120, height: 120, filter: "grayscale(1) contrast(1.2)" }} />
<View style={{ position: "absolute", bottom: 0, left: 0, right: 0, padding: 12, backgroundColor: "rgba(255,255,255,0.4)", backdropFilter: "blur(12px)" }} />
```

各平台的支持情况不同：

| | iOS | Android |
|---|---|---|
| 视图上的 `filter` | 仅支持 `opacity`、不超过 1 的 `brightness` 和 `drop-shadow` | Android 12（API 31）起全部支持；更低版本仅支持颜色滤镜 |
| 图片上的 `filter` | 所有颜色滤镜以及 `blur` | 与视图相同 |
| `backdropFilter` | 系统模糊材质，近似给定半径 | Android 12（API 31）起支持 |
| `mixBlendMode` | 支持 | Android 10（API 29）起支持 |

## 轮廓线、不透明度与可见性

- `outlineWidth`、`outlineColor`、`outlineStyle`、`outlineOffset`：边框之外、不占空间的一条线（负偏移会画在内侧）。
- `opacity`：0 到 1，作用于视图及其子视图。
- `visibility: "hidden"`：视图不绘制，但保留所占空间。`display: "none"` 则将它移出布局。
- `zIndex`：在兄弟视图中的顺序，也是原生的层叠顺序（值相同时保持稳定）。
- `cursor`：鼠标指针在视图上的形状，用于指针设备和桌面平台。

## 变换

`transform` 是一个步骤列表，与 CSS 一样从右到左应用：

```tsx
<View style={{ transform: [{ translateX: 20 }, { rotate: "-8deg" }, { scale: 0.9 }] }} />
<View style={{ transform: [{ perspective: 600 }, { rotateY: "35deg" }], backfaceVisibility: "hidden" }} />
<View style={{ transformOrigin: "top left", transform: [{ rotate: "30deg" }] }} />
```

可用步骤：`translateX`、`translateY`、`scale`、`scaleX`、`scaleY`、`rotate`（`rotateZ`）、`rotateX`、`rotateY`、`skewX`、`skewY`、`perspective`。`transformOrigin` 接受关键字、百分比和长度（`"top left"`、`"50% 0"`、`[10, 20]`），默认为中心。变换不影响布局：兄弟视图的位置按视图未变换时计算。

Android 不支持斜切（skew）变换。

## 过渡

只要指明要动画的属性，视图就会为自身样式的变化播放动画。动画在原生侧运行，每一帧都不会执行 JavaScript。

```tsx
<View
  onPress={() => open.set(!open())}
  style={{
    width: 150,
    height: 72,
    borderRadius: open() ? 36 : 12,
    backgroundColor: open() ? "#8250df" : "#54aeff",
    transform: [{ translateX: open() ? 120 : 0 }],
    transitionProperty: "all",
    transitionDuration: 300,
    transitionTimingFunction: "ease-in-out",
  }}
/>
```

| 属性 | 取值 |
|---|---|
| `transitionProperty` | 要动画的属性：`"all"`，或用逗号分隔的名称：颜色（`"colors"`、`"backgroundColor"`、`"borderColor"` 等）、`"opacity"`、`"transform"`、`"boxShadow"`、`"borderRadius"` / `"borderWidth"` |
| `transitionDuration` | `300`、`"300ms"`、`"0.3s"` |
| `transitionDelay` | 同时长 |
| `transitionTimingFunction` | `"linear"`、`"ease"`、`"ease-in"`、`"ease-out"`、`"ease-in-out"`、`"cubic-bezier(x1, y1, x2, y2)"` |

尺寸和位置属于布局而不是样式，不会产生过渡；要为布局变化做动画，使用 `LayoutAnimation`，或用原生动画驱动数值（[动画](./animation.md)）。

### 关键帧动画

`animationName` 以原生方式运行 Tailwind 内置的关键帧动画之一：`"spin"`、`"ping"`、`"pulse"` 或 `"bounce"`，配合 `animationDuration`、`animationDelay`、`animationTimingFunction` 和 `animationIterationCount`（数字或 `"infinite"`）：

```tsx
<View style={{ width: 24, height: 24, borderRadius: 4, backgroundColor: "#0969da", animationName: "spin", animationDuration: 1000, animationIterationCount: "infinite", animationTimingFunction: "linear" }} />
```

## 按压反馈

按下状态由原生处理，无需与 JavaScript 往返：

| 属性 | 按下时的效果 |
|---|---|
| `activeOpacity={0.6}` | 视图淡化到该不透明度 |
| `underlayColor="#eaeef2"` | 背景变为该颜色，子视图淡化到 `underlayOpacity`（默认 0.85） |
| `android={{ ripple: { color: "#0002" } }}` | Android 的 Material 水波纹 |
| `class="active:bg-zinc-100 active:scale-95"` | 通过 Tailwind 的 `active:` 变体设置任意样式 |

参见[触摸与手势](./touch-and-gestures.md)。

## 相关内容

- [Tailwind](./tailwind.md)：以类名表达同样的样式，并提供深色模式、按下状态和断点等变体。
- [布局](./layout.md)和[文本](./text.md)。
- [样式参考](../modules/style-reference.md)：所有样式属性及其类型。

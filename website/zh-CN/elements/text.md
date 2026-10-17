---
title: Text
description: 绘制文本的元素：嵌套片段、链接、行数限制、字体缩放、行内视图和文本布局事件。
order: 3
demo: el-text
---

# Text

`<Text>` 绘制一个段落。它是唯一能显示字符串的元素：`<View>` 的字符串子节点什么也不会画。段落在原生侧排版（iOS 和 Mac Catalyst 上用 CoreText，Android 上用 `StaticLayout`），并在引擎线程上测量，主线程只负责绘制。

```tsx
<Text style={{ fontSize: 16, lineHeight: 24, color: "#1f2328" }}>
  继续之前请阅读<text style={{ fontWeight: "700" }}>条款</Text>。
</Text>
```

## 片段

`<Text>` 里的 `<Text>` 是同一段落中的一个片段（span），而不是单独的视图。片段继承段落的文本样式（`color`、`fontSize`、`fontWeight` 等），并覆盖自己设置的部分。整个文本块是一个原生视图。

```tsx
<Text style={{ fontSize: 17 }}>
  <Text style={{ fontWeight: "300" }}>细体 </Text>
  <Text style={{ fontWeight: "bold" }}>粗体 </Text>
  <Text style={{ fontStyle: "italic", color: "#8250df" }}>斜体</Text>
</Text>
```

### 链接

带按压处理函数的片段就是链接：按在它的字符上由该片段处理，按在其他位置则由段落处理。

```tsx
<Text style={{ fontSize: 15 }} onPress={() => log("整句")}>
  {"继续即表示你同意"}
  <Text style={{ color: "#0969da", textDecorationLine: "underline" }} onPress={() => open("terms")}>
    服务条款
  </Text>
  。
</Text>
```

### 行内视图和图片

`<Text>` 中的其他元素（`<View>`、`<Image>`，即使位于片段内）都是行内附件：按自身尺寸布局，坐落在基线上，并随文字一起换行。它们可以单独响应按压。

```tsx
<Text style={{ fontSize: 16, lineHeight: 26 }}>
  {"由 "}
  <Image src={logo} style={{ width: 18, height: 18, borderRadius: 4 }} />
  {" nsh 构建。状态 "}
  <View style={{ backgroundColor: "#1a7f37", borderRadius: 9, paddingHorizontal: 7, height: 18, justifyContent: "center" }}>
    <Text style={{ color: "#ffffff", fontSize: 11, fontWeight: "700" }}>LIVE</Text>
  </View>
</Text>
```

被 `numberOfLines` 或省略号截掉的附件会被隐藏。复制可选中的文本时不包含附件。

### 空白字符

JSX 文本遵循 React 的规则：除文本首尾外，每行都会去掉首尾空白，只含空白的行被丢弃，剩下的行用一个空格连接。同一行内的空格会保留（`{a} {b}` 保留中间的空格）。片段需要前导或尾随空格时，写 `{" "}` 或字符串表达式。

## 属性

除[通用属性](./index.md)外（`style` 的类型为 `TextStyle`）：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `numberOfLines` | `number` | `0` | 最多显示的行数；`0` 表示不限。 |
| `ellipsizeMode` | `"tail" \| "head" \| "middle" \| "clip"` | `"tail"` | 超出 `numberOfLines` 的文本在哪里截断。 |
| `allowFontScaling` | `boolean` | `true` | 跟随系统的文字大小设置。 |
| `maxFontSizeMultiplier` | `number` | `0` | 系统缩放的上限；`0` 表示不限。 |
| `adjustsFontSizeToFit` | `boolean` | `false` | 缩小字号，直到文本能放进其行数（`numberOfLines`）和盒子。 |
| `minimumFontScale` | `number` | | `adjustsFontSizeToFit` 最多缩小到的比例（0..1；至少 0.1）。 |
| `selectable` | `boolean` | `false` | 用户可以选择文本的任意范围：长按选中一个词，拖动选择柄扩展，编辑菜单复制。 |
| `selectionColor` | `ColorValue` | | 可选文本的选区高亮（及选择柄）颜色；默认为平台的强调色。 |
| `onTextLayout` | `(e: { lines: TextLayoutLine[] }) => void` | | 每次布局后给出排好的各行。 |

Tailwind 类 `select-text` 等同于 `selectable`。

## 文本样式

`TextStyle` 在视图样式之上增加了：`color`、`fontSize`、`fontWeight`（`"normal"`、`"bold"` 或 100–900）、`fontStyle`、`fontFamily`、`lineHeight`、`letterSpacing`、`textAlign`、`textAlignVertical`、`textTransform`、`textDecorationLine`、`textDecorationColor`、`textDecorationStyle`、`textDecorationThickness`、`textUnderlineOffset`、`textShadowColor`、`textShadowOffset`、`textShadowRadius`、`fontVariant` 和 `writingDirection`。参见[样式参考](../modules/style-reference.md#textstyle)和[文本](../docs/text.md)。

`fontFamily` 接受系统字体族，或导入字体文件得到的名称：

```tsx
import { Text } from "nsh";
import Inter from "./fonts/Inter-Regular.ttf";

<Text style={{ fontFamily: Inter, fontSize: 17 }}>Inter</Text>;
```

字体会在第一帧之前注册。参见[字体](../docs/fonts.md)。

## 事件

| 事件 | 载荷 | 触发时机 |
|---|---|---|
| `onTextLayout` | `{ lines: TextLayoutLine[] }` | 段落每次布局之后。 |
| `onPress`、`onLongPress` 等 | 无 | 与 [view](./view.md) 相同；在片段上时，指按在其字符上的按压。 |
| `onLayout` | `{ x, y, width, height }` | 文本块的 frame 发生变化。 |

`TextLayoutLine` 为 `{ text, x, y, width, height, ascender, descender, capHeight, xHeight }`，坐标位于文本的内容盒中：

```tsx
const lines = signal(0);
<Text onTextLayout={(e) => lines.set(e.lines.length)}>{article}</Text>;
```

## ref 方法

没有自己的方法。

## 平台说明

- 选区只在一个文本块（一个 `<Text>` 及其片段）之内，不跨越多个文本块。iOS 显示其编辑菜单（拷贝、全选、查询、翻译、共享），Android 显示浮动工具栏（复制、全选、分享）。
- 在 iOS 上，非实线单线的文本装饰线（`double`、`dotted`、`dashed`、`wavy`）由 NSH 自行绘制。
- iOS 上的文本不绘制渐变背景和内阴影。
- [`nsh/device`](../modules/nsh-device.md) 的 `fontScale()` 给出当前系统的文字缩放比例。

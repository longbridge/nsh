---
title: 文字
description: 用 text 元素显示文字：嵌套 span 与链接、行内视图、字体与装饰线、截断、系统字号、选择以及 onTextLayout。
order: 35
group: ui
---

# 文字

所有文字都通过 `<Text>` 显示。一个 `<Text>` 元素就是一个段落，在布局时由平台的文本引擎（Apple 平台上是 CoreText，Android 上是 `StaticLayout`）在引擎线程上排版，主线程只负责绘制。嵌套的 `<Text>` 元素是同一段落中的 span，而不是独立的视图。

```tsx
<Text style={{ fontSize: 16, lineHeight: 24, color: "#1f2328" }}>
  By continuing you agree to the{" "}
  <Text style={{ color: "#0969da", textDecorationLine: "underline" }} onPress={openTerms}>
    Terms of service
  </Text>
  .
</Text>
```

> [!IMPORTANT]
> 字符串只有放在 `<Text>` 里才会显示。直接放在 `<View>` 中的字符串不会显示。

元素本身的说明见 [text](../elements/text.md)；本页介绍如何使用它。

## 字符串与空白

JSX 中的文字遵循与 React 相同的规则：每一行首尾的空白被去掉，只含空白的行被丢弃，剩下的行用一个空格连接。同一行内的空白会保留，所以 `{first} {last}` 中间的空格不会丢。在行首或行尾需要空格时用 `{" "}`，需要换行时用 `{"\n"}`：

```tsx
<Text>
  Hello,
  {name()}!{"\n"}
  Welcome back.
</Text>
```

文字中的表达式会跟随它读取的值：`<Text>{count()} items</Text>` 会在 `count` 变化时更新第一个 span。

## Span 与继承

`<Text>` 中的 `<Text>` 是一个 span。它继承父元素的文字样式，并覆盖自己设置的部分：

```tsx
<Text style={{ fontSize: 17, color: "#1f2328" }}>
  <Text style={{ fontWeight: "300" }}>Light </Text>
  <Text style={{ fontWeight: "700" }}>Bold </Text>
  <Text style={{ fontStyle: "italic" }}>Italic </Text>
  <Text style={{ letterSpacing: 4 }}>Spaced</Text>
</Text>
```

文字样式类也会像 CSS 一样从视图向下传递：`<View class="text-white text-sm">` 会为其中的 `<Text>` 和 `<Input>` 元素设置样式。

### 链接

带有 `onPress`（或 `onLongPress`）的 span 就是链接：按在它的字符上会交给这个 span，按在其他位置则交给整个段落。嵌套 span 中的链接同样有效。

```tsx
<Text onPress={() => log("the sentence")}>
  Read the <Text style={{ color: "#0969da" }} onPress={() => open(termsUrl)}>terms</Text> first.
</Text>
```

### Span 的背景

Span 的 `backgroundColor` 和 `borderRadius` 会在它所在的每一行、字符后面画一个与字体等高的方框；文字排版不变，复制时内容完整，所以行内代码就是一个 span（[Text 元素](../elements/text.md#片段背景)）：

```tsx
<Text>
  先运行 <Text style={{ fontFamily: "ui-monospace", backgroundColor: "#eff1f3", borderRadius: 4 }}> npm test </Text>。
</Text>
```

### 文字中的视图和图片

`<Text>` 中除 `<Text>` 以外的任何元素都会以行内方式布局在基线上，并随文字一起换行。它也可以有自己的按下处理：

```tsx
<Text style={{ fontSize: 16, lineHeight: 26 }}>
  Made with <Image src={logo} style={{ width: 18, height: 18 }} /> nsh. Status{" "}
  <View style={{ backgroundColor: "#1a7f37", borderRadius: 9, paddingHorizontal: 7, height: 18, justifyContent: "center" }}>
    <Text style={{ color: "#ffffff", fontSize: 11, fontWeight: "700" }}>LIVE</Text>
  </View>
</Text>
```

被 `numberOfLines` 或省略号截掉的行内元素会被隐藏；复制文字时也不会包含它们。

## 文字样式

| 属性 | 取值 |
|---|---|
| `color` | 一个[颜色](./styling.md)。默认黑色 |
| `fontSize` | 点。默认 14 |
| `fontWeight` | `"normal"`（400）、`"bold"`（700），或 `100`–`900`（数字或字符串） |
| `fontStyle` | `"normal"`、`"italic"` |
| `fontFamily` | 系统字体族、通用字体族或[导入的字体](./fonts.md) |
| `lineHeight` | 点 |
| `letterSpacing` | 点 |
| `textAlign` | `"auto"`、`"left"`、`"center"`、`"right"`、`"justify"` |
| `textAlignVertical` | `"auto"`、`"top"`、`"center"`、`"bottom"`：文字比所在框矮时的位置 |
| `textTransform` | `"none"`、`"uppercase"`、`"lowercase"`、`"capitalize"` |
| `textDecorationLine` | `"none"`、`"underline"`、`"line-through"`、`"underline line-through"` |
| `textDecorationColor`、`textDecorationStyle` | 颜色；`"solid"`、`"double"`、`"dotted"`、`"dashed"`、`"wavy"` |
| `textDecorationThickness`、`textUnderlineOffset` | 点或 `"auto"`（粗细还可以是 `"from-font"`） |
| `textShadowColor`、`textShadowOffset`、`textShadowRadius` | 字形后面的阴影 |
| `fontVariant` | OpenType 特性：`["tabular-nums"]`、`"small-caps"`、`"oldstyle-nums"`、`"lining-nums"`、`"proportional-nums"` |
| `writingDirection` | `"auto"`、`"ltr"`、`"rtl"`：段落的基础方向 |

`<Text>` 同时也是视图：它支持背景、边框、圆角、内边距以及其他[视图样式](./styling.md)。

```tsx
<Text style={{ fontSize: 18, fontVariant: ["tabular-nums"] }}>{price().toFixed(2)}</Text>
<Text style={{ textDecorationLine: "underline", textDecorationStyle: "wavy", textDecorationColor: "#cf222e" }}>Typo</Text>
<Text style={{ fontSize: 26, fontWeight: "800", color: "#ffffff", textShadowColor: "rgba(0,0,0,0.6)", textShadowOffset: { width: 2, height: 2 }, textShadowRadius: 4 }}>
  Shadow
</Text>
```

`tabular-nums` 让所有数字等宽，计数器和价格变化时就不会左右跳动。

## 截断

```tsx
<Text numberOfLines={2}>{longDescription}</Text>
<Text numberOfLines={1} ellipsizeMode="middle">{filePath}</Text>
```

| 属性 | 说明 |
|---|---|
| `numberOfLines` | 最多显示的行数；`0`（默认）表示不限 |
| `ellipsizeMode` | 放不下时在哪里截断：`"tail"`（默认）、`"head"`、`"middle"`、`"clip"` |
| `adjustsFontSizeToFit` | 缩小字号，直到文字能放进其行数和框内 |
| `minimumFontScale` | `adjustsFontSizeToFit` 最多能缩小到的比例（0–1） |

```tsx
<Text numberOfLines={1} adjustsFontSizeToFit minimumFontScale={0.5} style={{ fontSize: 48, width: 200 }}>
  {total()}
</Text>
```

Tailwind 类 `truncate` 和 `line-clamp-*` 效果相同；同时设置时以属性为准。

## 系统字号

文字默认跟随用户的字号设置（iOS 上的“动态字体”，Android 上的字体大小）。

| 属性 | 说明 |
|---|---|
| `allowFontScaling` | 设为 `false` 时字号固定。默认 `true` |
| `maxFontSizeMultiplier` | 允许的最大缩放比例，例如 `1.5`。`0` 表示不限 |

[`nsh/device`](../modules/nsh-device.md) 中的 `fontScale()` 是当前的缩放比例，这个访问器会在用户修改设置时变化。`<Input>` 同样支持 `allowFontScaling` 和 `maxFontSizeMultiplier`。

```tsx
<Text maxFontSizeMultiplier={1.3} style={{ fontSize: 13 }}>Tab label</Text>
```

与其关闭缩放，不如限制缩放比例：大字号是很多人依赖的辅助功能设置。

## 选择

`selectable` 允许用户像在系统自带的文本视图中那样选择和复制文字：长按选中手指下的词，拖动选择柄移动选区两端，编辑菜单提供“拷贝”和“全选”（iOS 还有查询、翻译和共享；Android 还有分享）。选区只在一个文本块之内。样式键 `selectionColor`（`selection:text-*`）设置高亮和选择柄的颜色。类 `select-text` 与 `selectable` 效果相同。

在桌面浏览器中，文字默认可以选择，和网页一样，但交互元素（带按压或触摸处理的视图，如按钮和可按压的列表行）里的文字除外；`selectable={false}` 或类 `select-none` 让文字在那里也不可选择。手机和平板浏览器遵循原生规则。

```tsx
<Text selectable style={{ selectionColor: "#7c3aed" }}>{orderNumber}</Text>
```

要在代码中复制（比如“复制”按钮），使用 [`nsh/clipboard`](../modules/nsh-clipboard.md)。

## onTextLayout

`onTextLayout` 在每次布局后报告排好的各行，坐标位于文字的内容框中：

```tsx
<Text
  onTextLayout={(e) => {
    lines.set(e.lines.length);
    console.log(e.lines.map((l) => `${l.text} (${l.width.toFixed(0)}pt)`));
  }}
>
  {body()}
</Text>
```

每一行包含 `text`、`x`、`y`、`width`、`height`、`ascender`、`descender`、`capHeight` 和 `xHeight`。可以用它在文字被截断时才显示“阅读更多”按钮，或让其他内容与基线对齐。

## 无障碍

屏幕阅读器会朗读 `<Text>` 的内容。为充当标题的文字设置角色（`role="header"`），为链接设置 `role="link"`。参见[无障碍](./accessibility.md)。

## 相关内容

- [字体](./fonts.md)：自定义字体与字重。
- [text 元素](../elements/text.md)：所有属性。
- [表单与键盘](./forms-and-keyboard.md)：文字输入。

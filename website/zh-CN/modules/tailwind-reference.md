---
title: Tailwind 参考
description: NSH 原生解析的全部 Tailwind 工具类与变体、任意值语法，以及如何在 nsh.json 中扩展主题。
order: 17
---

# Tailwind 参考

每个元素都可以在 `class` 属性中写 Tailwind CSS 类（条件类写在 `classList` 中）。NSH 在 Rust 中按 Tailwind 4 的默认主题解析它们：JS 只传递字符串，每个不同的字符串只解析一次并缓存；当外观、窗口尺寸或按压状态变化时，`dark:`、`active:` 等变体在原生侧重新求值，不运行任何 JS。

本页列出支持的内容。入门介绍见 [Tailwind](../docs/tailwind.md)；工具类最终设置的样式键见 [样式参考](./style-reference.md)。

```tsx
<View class="flex-row items-center gap-3 rounded-2xl bg-white p-4 shadow-md active:scale-[0.98] transition dark:bg-zinc-800">
  <Text class="text-base font-semibold text-zinc-900 dark:text-white">Inbox</Text>
  <Text classList={{ "text-red-500": unread() > 0, "text-zinc-400": unread() === 0 }}>{unread()}</Text>
</View>
```

## 类如何组合

- **后写的生效。** 同一变体层级的工具类中，字符串里靠后的那个生效：`"p-4 p-2"` 等于 `p-2`。组件可以接收 `class` 属性并直接拼接到自己的类后面，不需要 `tailwind-merge`。
- **变体在普通工具类之后应用**，顺序与 Tailwind 的变体顺序一致：位置（`first:` 等）、`group-*:`、`focus-within:`、`hover:`、`focus:`、`active:`、`enabled:`、`disabled:`、动效偏好、`max-*:` 断点、按宽度升序的断点、方向（横竖屏）、书写方向、`dark:`，最后是平台。因此 `"active:bg-red-500 bg-blue-500"` 按下时是红色。
- **`style` 优先于类。** 内联 `style` 叠加在解析后的类之上。重要标记的工具类（`!p-4` 或 `p-4!`）连内联样式也能覆盖。
- **文字样式会继承。** 写在视图上的文字类（`text-white`、`font-bold`、`text-center` 等）会像 CSS 一样传给其中的 `<Text>` 和 `<Input>`。
- **display 遵循 CSS。** `flex` 和 `inline-flex` 是横向排列，除非有方向类另行指定，且与其在字符串中的位置无关（`flex-col flex` 是纵向）。没有 `flex` 的视图保持 NSH 默认的纵向排列。

## 单位与主题

长度单位是点（iOS 上 1 point，Android 上 1 dp）。Tailwind 的 `1rem` 等于 16 点，所以在相同密度下与网页一致：一个间距步长是 4 点（`p-4` 为 16 点），`text-base` 为 16 点。

### 间距值

接受间距值的工具类（内外边距、gap、inset、宽高、`basis`、`translate`、`leading`）可以写：

| 值 | 示例 | 结果 |
|---|---|---|
| 步长（可用四分之一步） | `p-4`、`gap-1.5`、`m-0.25` | 步长 × 4 点 |
| `px` | `p-px`、`-mt-px` | 1 点 |
| `0` | `m-0` | 0 |
| 分数 | `w-1/2`、`left-1/3` | 父元素的百分比 |
| `full` | `w-full`、`inset-full` | 100% |
| `auto`（外边距、inset、宽高、basis） | `mx-auto`、`w-auto` | `"auto"` |
| 任意值 | `p-[13px]`、`w-[42%]`、`mt-[1.5rem]` | 给定长度 |

外边距、inset、`translate`、`rotate`、`skew`、`scale`、`z`、`hue-rotate`、`tracking`、`underline-offset`、`outline-offset`、网格线和渐变角度可以加前缀 `-` 表示负值：`-mt-2`、`-translate-x-1/2`、`-rotate-6`。

### 颜色

内置完整的 Tailwind 4 调色板：`red`、`orange`、`amber`、`yellow`、`lime`、`green`、`emerald`、`teal`、`cyan`、`sky`、`blue`、`indigo`、`violet`、`purple`、`fuchsia`、`pink`、`rose`、`slate`、`gray`、`zinc`、`neutral`、`stone`，每种有 `50`、`100`–`900`、`950` 色阶；另有 `black`、`white` 和 `transparent`。自定义颜色在 `nsh.json` 中添加（见 [主题定制](#主题定制)）。

所有颜色工具类都接受：

| 形式 | 示例 |
|---|---|
| 主题颜色 | `bg-sky-500`、`text-brand` |
| 不透明度修饰（百分比） | `bg-black/50`、`ring-black/5` |
| 任意不透明度 | `text-black/[0.35]` |
| 任意颜色 | `bg-[#0ea5e9]`、`border-[rgb(0_0_0_/_0.1)]`、`text-[oklch(70%_0.2_50)]` |
| 任意颜色加修饰 | `bg-[rgb(0_0_255)]/25` |

`currentColor` 和 `inherit` 不是可用的颜色值；未指定环颜色时，环默认使用元素的文字颜色。

## 任意值

方括号用于传入主题中没有的值：`w-[37px]`、`grid-cols-[200px_1fr]`、`bg-[linear-gradient(45deg,red,blue)]`。

- `_` 表示空格（`grid-cols-[1fr_2fr]`）；字面下划线写作 `\_`。
- 长度：裸数字或 `px`（点）、`pt`、`rem`（16 点），在合理的地方可用 `%`。
- 角度（`rotate-[...]`、`hue-rotate-[...]`、`bg-linear-[...]`）：`deg`、`rad`、`grad`、`turn`。
- 时间（`duration-[...]`、`delay-[...]`）：`ms` 或 `s`；裸数字表示毫秒。
- 类型提示用于消除歧义：`text-[length:22px]` 是字号，`text-[color:#333]` 是颜色；`border-[length:3px]` 同理。

不支持任意 CSS 属性（`[mask-type:luminance]`）。

## 布局

| Class | 设置 |
|---|---|
| `flex`、`inline-flex` | `display: "flex"`，没有方向类时为横向 |
| `grid`、`inline-grid` | `display: "grid"` |
| `block`、`flow-root` | `display: "block"`（块级布局） |
| `inline`、`inline-block` | `display: "flex"`（没有行内格式化上下文，按视图布局） |
| `hidden` | `display: "none"` |
| `relative`、`static`、`sticky` | `position: "relative"`（`sticky` 不会吸顶） |
| `absolute`、`fixed` | `position: "absolute"`（`fixed` 相对父元素而不是屏幕） |
| `inset-<n>`、`inset-x-<n>`、`inset-y-<n>` | `top` / `right` / `bottom` / `left` |
| `top-<n>`、`right-<n>`、`bottom-<n>`、`left-<n>` | 对应的偏移 |
| `start-<n>`、`end-<n>` | `start` / `end`（随书写方向） |
| `z-<n>`、`z-auto`、`z-[<n>]` | `zIndex`（`z-auto` 为 0） |
| `visible` | `visibility: "visible"` |
| `invisible`、`collapse` | `visibility: "hidden"`（保留空间） |
| `overflow-hidden`、`overflow-clip`、`overflow-x-hidden`、`overflow-y-hidden` | `overflow: "hidden"` |
| `overflow-visible` | `overflow: "visible"` |
| `aspect-square`、`aspect-video`、`aspect-<a>/<b>`、`aspect-[<a>/<b>]`、`aspect-auto` | `aspectRatio`（1、16/9、a/b、无） |

滚动由元素提供，而不是类：不支持 `overflow-auto` 和 `overflow-scroll`，请使用 [scroll](../elements/scroll.md)。

## Flexbox 与网格

| Class | 设置 |
|---|---|
| `flex-row`、`flex-row-reverse`、`flex-col`、`flex-col-reverse` | `flexDirection` |
| `flex-wrap`、`flex-wrap-reverse`、`flex-nowrap` | `flexWrap` |
| `flex-1` | grow 1、shrink 1、basis 0 |
| `flex-<n>` | grow n、shrink 1、basis 0 |
| `flex-auto` | grow 1、shrink 1、basis auto |
| `flex-initial` | grow 0、shrink 1、basis auto |
| `flex-none` | grow 0、shrink 0、basis auto |
| `flex-[<grow>]`、`flex-[<grow>_<shrink>_<basis>]` | CSS 简写：`flex-[2]`、`flex-[3_0_50%]` |
| `grow`、`grow-<n>`、`grow-[<n>]` | `flexGrow`（不带值为 1） |
| `shrink`、`shrink-<n>`、`shrink-[<n>]` | `flexShrink`（不带值为 1） |
| `basis-<n>` | `flexBasis`（间距、分数、`full`、`auto`、任意值） |
| `justify-start`、`justify-end`、`justify-center`、`justify-between`、`justify-around`、`justify-evenly`、`justify-stretch`、`justify-normal` | `justifyContent` |
| `items-start`、`items-end`、`items-center`、`items-baseline`、`items-stretch` | `alignItems` |
| `self-auto`、`self-start`、`self-end`、`self-center`、`self-stretch`、`self-baseline` | `alignSelf` |
| `content-start`、`content-end`、`content-center`、`content-between`、`content-around`、`content-evenly`、`content-stretch`、`content-normal` | `alignContent` |
| `justify-items-start`、`-end`、`-center`、`-stretch`、`-normal` | `justifyItems` |
| `justify-self-auto`、`-start`、`-end`、`-center`、`-stretch` | `justifySelf` |
| `place-content-center`、`-start`、`-end`、`-between`、`-stretch` | `alignContent` 和 `justifyContent` |
| `place-items-center`、`-start`、`-end`、`-stretch` | `alignItems` 和 `justifyItems` |
| `place-self-auto`、`-center`、`-start`、`-end`、`-stretch` | `alignSelf` 和 `justifySelf` |
| `gap-<n>`、`gap-x-<n>`、`gap-y-<n>` | `gap`、`columnGap`、`rowGap` |
| `space-x-<n>`、`space-y-<n>` | `columnGap`、`rowGap`（是 gap，而不是给子元素加外边距） |
| `grid-cols-<n>` | `gridTemplateColumns: "repeat(n, minmax(0, 1fr))"` |
| `grid-cols-[<tracks>]`、`grid-cols-none` | `gridTemplateColumns`（`grid-cols-[200px_1fr]`） |
| `grid-rows-<n>`、`grid-rows-[<tracks>]`、`grid-rows-none` | `gridTemplateRows` |
| `col-span-<n>`、`col-span-full`、`col-auto`、`col-<n>`、`col-[<value>]` | `gridColumn` |
| `col-start-<n>`、`col-end-<n>`（及 `-auto`） | `gridColumnStart`、`gridColumnEnd` |
| `row-span-<n>`、`row-span-full`、`row-auto`、`row-<n>`、`row-[<value>]` | `gridRow` |
| `row-start-<n>`、`row-end-<n>`（及 `-auto`） | `gridRowStart`、`gridRowEnd` |
| `grid-flow-row`、`grid-flow-col`、`grid-flow-dense`、`grid-flow-row-dense`、`grid-flow-col-dense` | `gridAutoFlow` |
| `auto-cols-auto`、`-min`、`-max`、`-fr`、`auto-cols-[<track>]` | `gridAutoColumns` |
| `auto-rows-auto`、`-min`、`-max`、`-fr`、`auto-rows-[<track>]` | `gridAutoRows` |

`order-*` 可以写，但不起作用：子元素保持原有顺序。

## 间距

| Class | 设置 |
|---|---|
| `p-<n>`、`px-<n>`、`py-<n>` | `padding`、`paddingHorizontal`、`paddingVertical` |
| `pt-<n>`、`pr-<n>`、`pb-<n>`、`pl-<n>` | `paddingTop` / `Right` / `Bottom` / `Left` |
| `ps-<n>`、`pe-<n>` | `paddingStart`、`paddingEnd` |
| `m-<n>`、`mx-<n>`、`my-<n>` | `margin`、`marginHorizontal`、`marginVertical` |
| `mt-<n>`、`mr-<n>`、`mb-<n>`、`ml-<n>` | `marginTop` / `Right` / `Bottom` / `Left` |
| `ms-<n>`、`me-<n>` | `marginStart`、`marginEnd` |

## 尺寸

| Class | 设置 |
|---|---|
| `w-<n>`、`h-<n>` | `width`、`height`（间距、分数、`full`、`auto`、任意值） |
| `size-<n>` | `width` 和 `height` |
| `min-w-<n>`、`max-w-<n>`、`min-h-<n>`、`max-h-<n>` | 对应的限制 |
| `w-screen`、`w-dvw`、`w-svw`、`w-lvw` | 窗口宽度（`min-w-` / `max-w-` 也可用） |
| `h-screen`、`h-dvh`、`h-svh`、`h-lvh` | 窗口高度（`min-h-` / `max-h-` 也可用） |
| `w-min`、`w-max`、`w-fit`（及 `h-`） | `auto`（由内容决定） |
| `min-w-min`、`min-w-max`、`min-w-fit`（及 `min-h-`） | 0 |
| `max-w-none`、`max-h-none`、`max-w-min`、`max-w-max`、`max-w-fit` | 不限制 |
| `w-<container>`、`min-w-<container>`、`max-w-<container>` | 容器宽度（见下） |

容器宽度（点）：`3xs` 256、`2xs` 288、`xs` 320、`sm` 384、`md` 448、`lg` 512、`xl` 576、`2xl` 672、`3xl` 768、`4xl` 896、`5xl` 1024、`6xl` 1152、`7xl` 1280、`prose` 520。

## 排版

### 字体

| Class | 设置 |
|---|---|
| `font-sans` | 系统字体 |
| `font-serif`、`font-mono` | `fontFamily: "serif"` / `"monospace"` |
| `font-thin`（100）、`font-extralight`（200）、`font-light`（300）、`font-normal`（400）、`font-medium`（500）、`font-semibold`（600）、`font-bold`（700）、`font-extrabold`（800）、`font-black`（900） | `fontWeight` |
| `font-[<weight>]`、`font-[<family>]` | 数字为字重，否则为字体族：`font-[650]`、`font-[Inter]` |
| `italic`、`not-italic` | `fontStyle` |
| `normal-nums`、`small-caps`、`oldstyle-nums`、`lining-nums`、`tabular-nums`、`proportional-nums` | `fontVariant`（可组合） |

### 字号、颜色与行高

`text-<size>` 同时设置字号和 Tailwind 对应的行高：

| Class | 字号 | 行高 |
|---|---|---|
| `text-xs` | 12 | 16 |
| `text-sm` | 14 | 20 |
| `text-base` | 16 | 24 |
| `text-lg` | 18 | 28 |
| `text-xl` | 20 | 28 |
| `text-2xl` | 24 | 32 |
| `text-3xl` | 30 | 36 |
| `text-4xl` | 36 | 40 |
| `text-5xl` | 48 | 1 倍 |
| `text-6xl` | 60 | 1 倍 |
| `text-7xl` | 72 | 1 倍 |
| `text-8xl` | 96 | 1 倍 |
| `text-9xl` | 128 | 1 倍 |

| Class | 设置 |
|---|---|
| `text-<size>/<leading>` | 字号和行高：`text-sm/6`（24 点）、`text-lg/tight` |
| `text-[<length>]` | `fontSize`：`text-[22px]`、`text-[length:22px]` |
| `text-<color>` | `color` |
| `leading-none`（1）、`leading-tight`（1.25）、`leading-snug`（1.375）、`leading-normal`（1.5）、`leading-relaxed`（1.625）、`leading-loose`（2） | `lineHeight`，字号的倍数 |
| `leading-<n>`、`leading-[<length>]` | `lineHeight`，单位为点（`leading-6` = 24） |
| `tracking-tighter`（-0.05em）、`tracking-tight`（-0.025em）、`tracking-normal`（0）、`tracking-wide`（0.025em）、`tracking-wider`（0.05em）、`tracking-widest`（0.1em） | 相对字号的 `letterSpacing` |
| `tracking-[<value>]` | `letterSpacing`：`tracking-[0.2em]`、`tracking-[1px]` |

### 对齐、大小写与换行

| Class | 设置 |
|---|---|
| `text-left`、`text-center`、`text-right`、`text-justify` | `textAlign` |
| `text-start` | `textAlign: "auto"` |
| `text-end` | `textAlign: "right"` |
| `uppercase`、`lowercase`、`capitalize`、`normal-case` | `textTransform` |
| `truncate` | 单行、尾部省略、不换行、`overflow: hidden` |
| `line-clamp-<n>`、`line-clamp-none` | `numberOfLines`（`none` 取消限制） |
| `text-ellipsis`、`text-clip` | `ellipsizeMode` 为 `"tail"` / `"clip"` |
| `whitespace-nowrap`、`text-nowrap` | 不换行 |
| `whitespace-normal`、`text-wrap`、`text-balance`、`text-pretty` | 正常换行（不做行平衡） |
| `select-text`、`select-all`、`select-none` | `selectable` |

### 装饰线与阴影

| Class | 设置 |
|---|---|
| `underline`、`line-through`、`no-underline` | `textDecorationLine` |
| `decoration-solid`、`decoration-double`、`decoration-dotted`、`decoration-dashed`、`decoration-wavy` | `textDecorationStyle` |
| `decoration-<n>`、`decoration-[<length>]` | `textDecorationThickness` |
| `decoration-auto`、`decoration-from-font` | `textDecorationThickness: "auto"` |
| `decoration-<color>` | `textDecorationColor` |
| `underline-offset-<n>`、`underline-offset-[<length>]`、`underline-offset-auto` | `textUnderlineOffset` |
| `text-shadow-2xs`、`-xs`、`-sm`、`-md`、`-lg` | 主题中的文字阴影 |
| `text-shadow-<color>` | 阴影颜色（颜色本身不带透明度时保留各层透明度） |
| `text-shadow-none` | 去掉阴影 |

Tailwind 的文字阴影有多层；原生只绘制一层，即最宽的那层。

### 输入框

| Class | 设置 |
|---|---|
| `placeholder:text-<color>` | `placeholderTextColor` |
| `caret-<color>` | 光标颜色 |

## 背景

| Class | 设置 |
|---|---|
| `bg-<color>` | `backgroundColor` |
| `bg-[<gradient>]` | `backgroundImage`：`bg-[linear-gradient(45deg,red,blue)]`、`bg-[radial-gradient(...)]` |
| `bg-linear-to-t`、`-tr`、`-r`、`-br`、`-b`、`-bl`、`-l`、`-tl` | 朝某边或某角的线性渐变（`bg-gradient-to-*` 是 v3 的写法） |
| `bg-linear-<angle>`、`bg-linear-[<angle>]` | 指定角度的线性渐变：`bg-linear-45`、`-bg-linear-12`、`bg-linear-[0.25turn]` |
| `bg-radial` | 从中心开始的径向渐变 |
| `bg-conic`、`bg-conic-<angle>` | 锥形渐变，可指定起始角度 |
| `from-<color>`、`via-<color>`、`to-<color>` | 渐变色标（两端默认透明） |
| `from-<p>%`、`via-<p>%`、`to-<p>%` | 色标位置：`via-40%`、`from-[15%]` |
| `bg-none` | 去掉渐变 |

```tsx
<View class="h-32 rounded-2xl bg-linear-to-br from-indigo-500 via-purple-500 via-40% to-pink-500" />
```

## 边框

| Class | 设置 |
|---|---|
| `border`、`border-<n>`、`border-[<length>]` | `borderWidth`（不带值为 1） |
| `border-x`、`border-y`、`border-t`、`border-r`、`border-b`、`border-l`、`border-s`、`border-e`（带 `-<n>` 或不带值） | 对应边的宽度 |
| `border-<color>`、`border-t-<color>` 等 | `borderColor` 及各边颜色 |
| `border-solid`、`border-dashed`、`border-dotted` | `borderStyle`（`border-double` 画实线） |
| `border-none`、`border-hidden` | 宽度为 0 |
| `rounded`、`rounded-<size>`、`rounded-none`、`rounded-full`、`rounded-[<length>]` | `borderRadius`（不带值为 `sm`） |
| `rounded-t-`、`-r-`、`-b-`、`-l-`、`-tl-`、`-tr-`、`-br-`、`-bl-` | 对应的角 |
| `rounded-s-`、`-e-`、`-ss-`、`-se-`、`-es-`、`-ee-` | 起始 / 结束侧的角（随书写方向） |
| `outline`、`outline-<n>`、`outline-[<length>]` | `outlineWidth` 及实线 `outlineStyle`（不带值为 1） |
| `outline-<color>` | `outlineColor` |
| `outline-solid`、`outline-dashed`、`outline-dotted`、`outline-none`、`outline-hidden` | `outlineStyle` |
| `outline-offset-<n>`、`outline-offset-[<length>]` | `outlineOffset` |

圆角尺寸（点）：`xs` 2、`sm` 4、`md` 6、`lg` 8、`xl` 12、`2xl` 16、`3xl` 24、`4xl` 32、`full` 9999。

### 环

环是带扩展的盒阴影，画在边框之外，不占布局空间。

| Class | 设置 |
|---|---|
| `ring`、`ring-<n>`、`ring-[<length>]` | 指定宽度的环（不带值为 1） |
| `ring-<color>` | 环的颜色（默认为元素的文字颜色） |
| `ring-inset` | 把环画在内侧 |
| `ring-offset-<n>`、`ring-offset-<color>` | 元素与环之间的间隙（默认白色） |
| `inset-ring`、`inset-ring-<n>`、`inset-ring-<color>` | 内环 |

## 效果

| Class | 设置 |
|---|---|
| `shadow-2xs`、`shadow-xs`、`shadow-sm`、`shadow`、`shadow-md`、`shadow-lg`、`shadow-xl`、`shadow-2xl` | 主题中的 `boxShadow` |
| `shadow-[<box-shadow>]` | 任意阴影：`shadow-[0_8px_24px_rgb(0_0_0_/_0.12)]` |
| `shadow-<color>` | 给阴影层重新着色：`shadow-indigo-500/40` |
| `shadow-none` | 无阴影 |
| `inset-shadow-2xs`、`-xs`、`-sm`、`inset-shadow`、`inset-shadow-[...]`、`inset-shadow-<color>`、`inset-shadow-none` | 内阴影 |
| `opacity-<n>`、`opacity-[<value>]` | `opacity`：`opacity-50` = 0.5 |
| `mix-blend-<mode>` | `mixBlendMode`：`normal`、`multiply`、`screen`、`overlay`、`darken`、`lighten`、`color-dodge`、`color-burn`、`hard-light`、`soft-light`、`difference`、`exclusion`、`hue`、`saturation`、`color`、`luminosity`、`plus-darker`、`plus-lighter` |

阴影、环和内阴影与 CSS 一样合并为一个 `boxShadow` 列表。阴影颜色本身不带透明度时，保留各层原有的透明度。

## 滤镜

| Class | 设置 |
|---|---|
| `blur`、`blur-xs`（4）、`-sm`（8）、`-md`（12）、`-lg`（16）、`-xl`（24）、`-2xl`（40）、`-3xl`（64）、`blur-none`、`blur-[<length>]` | `blur()` |
| `brightness-<n>`、`contrast-<n>`、`saturate-<n>` | 百分比倍数：`brightness-125` = 1.25 |
| `grayscale`、`grayscale-<n>`、`invert`、`invert-<n>`、`sepia`、`sepia-<n>` | 不带值为 100% |
| `hue-rotate-<deg>`、`-hue-rotate-<deg>` | `hue-rotate()` |
| `drop-shadow`、`drop-shadow-xs`、`-sm`、`-md`、`-lg`、`-xl`、`-2xl`、`drop-shadow-[...]` | `drop-shadow()`（原生只画一层：最宽的那层） |
| `filter-none` | 去掉所有滤镜 |
| `backdrop-blur`、`backdrop-blur-<size>`、`backdrop-blur-[<length>]` | 模糊视图背后的内容 |
| `backdrop-brightness-<n>`、`backdrop-contrast-<n>`、`backdrop-grayscale`、`backdrop-hue-rotate-<deg>`、`backdrop-invert`、`backdrop-opacity-<n>`、`backdrop-saturate-<n>`、`backdrop-sepia` | 背景滤镜 |
| `backdrop-filter-none` | 去掉背景滤镜 |

> [!WARNING]
> iOS 上视图只支持 `opacity`、不超过 1 的 `brightness` 和 `drop-shadow`；`<Image>` 支持所有颜色滤镜和 `blur`。Android 上 `blur`、`drop-shadow` 和背景模糊需要 API 31。见 [样式参考](./style-reference.md#滤镜与混合)。

## 变换

| Class | 设置 |
|---|---|
| `translate-x-<n>`、`translate-y-<n>`、`translate-<n>` | 平移（间距、`full`、元素自身尺寸的分数：`-translate-x-1/2`、任意值） |
| `rotate-<deg>`、`rotate-z-<deg>`、`rotate-[<angle>]` | 旋转 |
| `rotate-x-<deg>`、`rotate-y-<deg>` | 3D 旋转 |
| `scale-<pct>`、`scale-x-<pct>`、`scale-y-<pct>`、`scale-[<factor>]` | 缩放：`scale-95` = 0.95，`-scale-x-100` 为水平镜像 |
| `skew-<deg>`、`skew-x-<deg>`、`skew-y-<deg>` | 斜切（Android 不支持） |
| `perspective-dramatic`（100）、`-near`（300）、`-normal`（500）、`-midrange`（800）、`-distant`（1200）、`perspective-[<length>]` | 3D 旋转的透视距离 |
| `origin-center`、`-top`、`-top-right`、`-right`、`-bottom-right`、`-bottom`、`-bottom-left`、`-left`、`-top-left`、`origin-[<value>]` | `transformOrigin` |
| `backface-hidden`、`backface-visible` | `backfaceVisibility` |
| `transform-none` | 清除变换 |

变换类与 CSS 一样组合：`translate-*`、`rotate-*` 和 `scale-*` 无论顺序都会叠加。`transform`、`transform-gpu`、`transform-cpu`、`transform-3d` 和 `transform-flat` 可以写，但不起作用。按尺寸分数的平移会根据元素的实际尺寸计算，尺寸变化时自动更新。

## 过渡与动画

| Class | 设置 |
|---|---|
| `transition` | 颜色、不透明度、阴影和变换；150 ms，`cubic-bezier(0.4, 0, 0.2, 1)` |
| `transition-all`、`transition-colors`、`transition-opacity`、`transition-shadow`、`transition-transform` | 参与动画的属性 |
| `transition-none` | 无过渡 |
| `duration-<ms>`、`duration-[<time>]` | `transitionDuration`：`duration-300`、`duration-[0.4s]` |
| `delay-<ms>`、`delay-[<time>]` | `transitionDelay` |
| `ease-linear`、`ease-in`、`ease-out`、`ease-in-out`、`ease-initial`、`ease-[cubic-bezier(...)]` | `transitionTimingFunction` |
| `animate-spin`、`animate-ping`、`animate-pulse`、`animate-bounce` | 原生关键帧动画，无限循环 |
| `animate-none` | 停止动画 |

值发生变化时（包括变体开始或停止匹配时），过渡在 UI 线程上运行，所以 `transition active:scale-95` 无需 JS 就能为按压加上动画。把动画包在 `motion-safe:` 中即可尊重系统的减弱动态效果设置。

## 交互

| Class | 设置 |
|---|---|
| `pointer-events-none`、`pointer-events-auto` | `pointerEvents` |
| `cursor-<name>`、`cursor-[<name>]` | `cursor`：`cursor-pointer`、`cursor-grab`、`cursor-not-allowed` 等（所有 CSS 光标关键字） |
| `select-none`、`select-text`、`select-all`、`select-auto` | 文字选择（见排版） |

## 图片

| Class | 设置 |
|---|---|
| `object-cover`、`object-contain`、`object-fill`、`object-none`、`object-scale-down` | `resizeMode`（`fill` 拉伸，`none` 和 `scale-down` 居中） |
| `tint-<color>` | `tintColor`（为模板图片着色） |

## 可写但不起作用

以下网页工具类可以通过解析、不会产生警告，但在原生侧不起作用：`antialiased`、`subpixel-antialiased`、`isolate`、`isolation-auto`、`box-border`、`box-content`、`order-*`、`select-auto`、`transform`、`transform-gpu`、`transform-cpu`、`transform-3d`、`transform-flat`，以及所有 `will-change-*`、`touch-*`、`appearance-*`、`scroll-*`、`snap-*` 和 `overscroll-*` 类。滚动、吸附和越界滚动是 [scroll](../elements/scroll.md) 的属性。

## 不支持

`peer-*` 变体、伪元素（`before:`、`after:`）、任意属性（`[prop:value]`）、`overflow-auto` / `overflow-scroll`、`divide-*`、`columns-*`、`float-*`、`container` 以及容器查询。`nsh build` 和 `nsh dev` 会对源码中发现的未知类给出警告（`file:line:col: unknown class '...'`），运行时也会对每个未知类记录一次日志。

## 变体

变体让工具类按条件生效：`dark:bg-zinc-900`、`md:flex-row`、`active:opacity-70`。变体可以叠加（`dark:active:bg-zinc-700`），依赖的条件变化时在原生侧重新求值。

### 外观与环境

| 变体 | 生效条件 |
|---|---|
| `dark:` | 外观为深色 |
| `ios:` | 运行在 iOS 或 Mac Catalyst 上 |
| `android:` | 运行在 Android 上 |
| `portrait:`、`landscape:` | 窗口高大于宽 / 宽大于高 |
| `ltr:`、`rtl:` | 布局方向 |
| `motion-safe:`、`motion-reduce:` | 用户未开启 / 开启了减弱动态效果 |

### 断点

断点与窗口宽度（点）比较。

| 变体 | 最小宽度 |
|---|---|
| `sm:` | 640 |
| `md:` | 768 |
| `lg:` | 1024 |
| `xl:` | 1280 |
| `2xl:` | 1536 |

| 形式 | 示例 | 生效条件 |
|---|---|---|
| `<bp>:` | `md:p-8` | 宽度 ≥ 断点 |
| `max-<bp>:` | `max-md:p-1` | 宽度 < 断点 |
| `min-[<length>]:` | `min-[600px]:grid-cols-3` | 宽度 ≥ 该长度 |
| `max-[<length>]:` | `max-[400px]:text-sm` | 宽度 < 该长度 |

竖屏手机（宽 390 点）不匹配任何默认断点；平板或横屏手机会匹配其中几个。

### 交互状态

只有类中用到这些状态的元素才会由平台跟踪，状态变化在原生侧生效，不需要往返 JS。

| 变体 | 生效条件 |
|---|---|
| `active:` | 元素被按下 |
| `hover:` | 指针悬停其上（iPad 指针、Mac Catalyst、Android 鼠标） |
| `focus:`、`focus-visible:` | 输入框获得焦点 |
| `focus-within:` | 元素自身或其后代获得焦点 |
| `disabled:` | 元素设置了 `disabled` 属性 |
| `enabled:` | 元素没有设置 `disabled` 属性 |

`disabled` 适用于任何元素：它会让 `disabled:` 生效，并移除元素的按压处理器，因此也不会显示按压反馈。

```tsx
<View
  onPress={save}
  disabled={saving()}
  class="rounded-lg bg-blue-600 px-4 py-2 transition active:scale-95 active:bg-blue-700 disabled:opacity-40"
>
  <Text class="font-medium text-white">Save</Text>
</View>
```

### 分组

给祖先元素加上 `group`，后代即可用 `group-<state>:` 按它的状态设置样式。分组嵌套时给它命名：`group/card` 与 `group-hover/card:`。

| 变体 | 分组处于 |
|---|---|
| `group-active:` | 按下 |
| `group-hover:` | 悬停 |
| `group-focus:`、`group-focus-visible:` | 获得焦点 |
| `group-focus-within:` | 自身或内部获得焦点 |
| `group-disabled:` | 禁用 |

```tsx
<View onPress={open} class="group/row flex-row items-center gap-3 p-3 active:bg-zinc-100">
  <Text class="flex-1 text-zinc-900 group-active/row:text-blue-600">Settings</Text>
  <Text class="text-zinc-400 group-active/row:translate-x-1 transition-transform">›</Text>
</View>
```

### 在兄弟元素中的位置

| 变体 | 作用于 |
|---|---|
| `first:` | 第一个子元素 |
| `last:` | 最后一个子元素 |
| `only:` | 唯一的子元素 |
| `odd:` | 第 1、3……个子元素 |
| `even:` | 第 2、4……个子元素 |

```tsx
<Each of={items()}>{(item) => <Text class="border-b border-zinc-200 py-3 last:border-b-0">{item().name}</Text>}</Each>
```

### 占位符

`placeholder:` 作用于 `<Input>` 的占位文字，只接受文字颜色：`placeholder:text-zinc-400`。

### 重要标记

在工具类前后加 `!`（`!p-4`、`p-4!`）可以让它覆盖其他所有类以及内联 `style`。

## 主题定制

在应用 `nsh.json` 的 `"tailwind"` 对象中扩展主题。新名字的条目会被添加，与默认值同名的条目会替换默认值。长度单位是点。主题在打包时检查：未知的键或无效的值会导致构建错误。

```json
{
  "name": "My App",
  "id": "com.example.myapp",
  "tailwind": {
    "colors": {
      "brand": { "50": "#f5f3ff", "500": "#7c3aed", "600": "#6d28d9", "DEFAULT": "#7c3aed" },
      "accent": "#f59e0b"
    },
    "spacing": 4,
    "breakpoints": { "tablet": 600 },
    "fontFamily": { "display": "Inter" },
    "fontSize": { "huge": [48, 1.1] },
    "fontWeight": { "heavy": 850 },
    "radius": { "card": 14 },
    "shadow": { "card": "0 2px 8px rgb(0 0 0 / 0.12)" },
    "insetShadow": { "well": "inset 0 2px 6px rgb(0 0 0 / 0.1)" },
    "dropShadow": { "icon": "0 2px 4px rgb(0 0 0 / 0.2)" },
    "tracking": { "label": 0.08 },
    "leading": { "dense": 1.15 },
    "blur": { "glass": 20 },
    "perspective": { "card": 700 },
    "aspect": { "photo": 1.5 },
    "ease": { "snappy": [0.2, 0, 0, 1] },
    "container": { "content": 680 }
  }
}
```

有了这个主题，`bg-brand-500`、`bg-brand`（`DEFAULT` 色阶）、`text-accent`、`tablet:flex-row`、`font-display`、`text-huge`、`font-heavy`、`rounded-card`、`shadow-card`、`inset-shadow-well`、`drop-shadow-icon`、`tracking-label`、`leading-dense`、`blur-glass`、`perspective-card`、`aspect-photo`、`ease-snappy` 和 `max-w-content` 都可以使用。

| 键 | 值 | 用于 |
|---|---|---|
| `colors` | `{ name: color }` 或 `{ name: { shade: color } }`；`DEFAULT` 表示不带色阶的名字。可用任意 CSS 颜色语法。 | 所有颜色工具类 |
| `spacing` | 每个步长的点数（默认 4） | `p-*`、`m-*`、`gap-*`、`w-*` 等 |
| `rem` | 每 `rem` 的点数（默认 16） | |
| `breakpoints` | `{ name: width }` | 断点变体（按宽度排序） |
| `container` | `{ name: width }` | `w-*`、`min-w-*`、`max-w-*` |
| `fontFamily` | `{ name: family }`（系统字体族或打包字体的名字） | `font-*` |
| `fontSize` | `{ name: size }` 或 `{ name: [size, lineHeightMultiple] }` | `text-*` |
| `fontWeight` | `{ name: 1–1000 }` | `font-*` |
| `tracking` | `{ name: em }` | `tracking-*` |
| `leading` | `{ name: multiple }` | `leading-*` |
| `radius` | `{ name: points }` | `rounded-*` |
| `shadow`、`insetShadow`、`dropShadow` | `{ name: "CSS box-shadow" }` | `shadow-*`、`inset-shadow-*`、`drop-shadow-*` |
| `blur` | `{ name: points }` | `blur-*`、`backdrop-blur-*` |
| `perspective` | `{ name: points }` | `perspective-*` |
| `aspect` | `{ name: ratio }` | `aspect-*` |
| `ease` | `{ name: [x1, y1, x2, y2] }` | `ease-*` |

> [!NOTE]
> Tailwind 主题只影响应用自己的类。`nsh/ui/mobile` 的组件从组件主题（`nsh.json` 的 `"theme"` 键）取颜色，见 [组件](../components/index.md)。

## 相关

- [Tailwind](../docs/tailwind.md)：使用指南。
- [样式](../docs/styling.md)：`class`、`classList`、`style` 及其组合方式。
- [样式参考](./style-reference.md)：工具类所设置的样式键。

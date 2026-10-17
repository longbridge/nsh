---
title: Tailwind
description: 用原生解析的 Tailwind CSS 类给元素加样式：class 与 classList，按下状态、深色模式和断点等变体，nsh.json 中的主题，以及构建时的类名检查。
order: 33
group: ui
demo: guide-tailwind
---

# Tailwind

每个元素都可以在 `class` 中写 Tailwind CSS 类，它们会以 Tailwind 4 的默认主题为准，在原生层解析：

```tsx
<View class="flex-row items-center gap-3 rounded-2xl bg-white p-4 shadow-lg active:scale-[0.98] dark:bg-zinc-800">
  <Text class="text-base font-semibold text-zinc-900 dark:text-white">Tailwind on native views</Text>
</View>
```

整个过程不生成 CSS，也不在 JavaScript 中运行任何代码：字符串被直接交给核心，核心对每个不同的类字符串只解析一次（并缓存），把它转换为样式规则后应用。依赖状态的变体（按下、深色模式、窗口宽度）会在相应状态变化时由核心重新应用；按下和聚焦状态在原生层跟踪，因此 `active:` 的反馈无需往返 JavaScript。

类名和 [`style` 属性](./styling.md) 产生的是同样的原生样式，两者可以组合使用；本页只介绍类名特有的部分。[Tailwind 参考](../modules/tailwind-reference.md) 列出了所有支持的工具类。

## class 与 classList

```tsx
<View
  class="rounded-lg px-4 py-2 transition-colors"
  classList={{ "bg-indigo-600": selected(), "bg-zinc-200": !selected(), "opacity-50": busy() }}
/>
```

- **`class`** 是一个类名字符串，可以是动态的：``class={`h-6 rounded bg-${color()}-500`}`` 可以正常工作，因为类名在运行时解析。
- **`classList`** 把类名映射到条件：值为真时该类生效。它在 `class` 之后应用。

冲突的处理方式：

1. 同一字符串中**靠后的类优先**：`"p-2 p-4"` 的结果是 `p-4`。不需要 `tailwind-merge`：组件可以直接把调用方传入的 `class` 追加在后面。
2. **带变体的类在普通类之后应用**，并按 Tailwind 的变体顺序排列（`hover:` 在 `active:` 之前，断点从小到大，`dark:` 最后），与它们在字符串中的位置无关。和 CSS 一样，`odd:bg-a last:bg-b` 会让位于奇数位置的最后一行得到 `bg-a`，因为 Tailwind 把 `odd` 排在 `last` 之后。
3. **`style` 优先于类名。**
4. `!` 让一个类变为 important：`!p-0`。

`nsh/ui/mobile` 的组件同样接受 `class` 和 `classList`；你的类会合并在组件自身样式之后，同等优先级时以你的为准。

## 变体

| 变体 | 生效条件 |
|---|---|
| `active:` | 视图被按下 |
| `hover:` | 指针悬停在视图上（iPad 和 Mac 的指针） |
| `focus:`、`focus-visible:` | 输入框获得焦点 |
| `focus-within:` | 视图或其后代获得焦点 |
| `disabled:`、`enabled:` | 元素的 `disabled` 属性为 true / false |
| `group-active:`、`group-hover:`、`group-focus:`、`group-focus-within:`、`group-disabled:` | 最近的带 `group` 类的祖先处于该状态 |
| `group-active/name:` ... | 同上，针对标记为 `group/name` 的祖先 |
| `first:`、`last:`、`odd:`、`even:`、`only:` | 元素自身在兄弟节点中的位置。要按行的位置给行内标签上色，把 `last:text-*` 写在行上，标签会继承它 |
| `placeholder:` | 设置输入框占位文字的样式（`placeholder:text-zinc-400`） |
| `dark:` | 系统外观为深色 |
| `sm:`、`md:`、`lg:`、`xl:`、`2xl:` | 窗口宽度至少为 640、768、1024、1280、1536 点 |
| `max-sm:`、`max-md:` ... | 窗口宽度小于该断点 |
| `min-[600px]:`、`max-[600px]:` | 任意断点 |
| `portrait:`、`landscape:` | 窗口方向 |
| `ios:`、`android:` | 平台 |
| `rtl:`、`ltr:` | [布局方向](./layout.md) |
| `motion-safe:`、`motion-reduce:` | 用户的“减弱动态效果”设置 |

变体可以组合：`dark:active:bg-zinc-700`、`md:landscape:flex-row`。

```tsx
<View class="group flex-row items-center gap-3 rounded-xl bg-white p-3 active:bg-zinc-50">
  <Text class="flex-1 text-zinc-900">Settings</Text>
  <Text class="text-zinc-300 transition-transform group-active:translate-x-1">›</Text>
</View>
```

任何元素上的 `disabled` 都会应用 `disabled:` 类，并把它的按下处理器从挂载内容中移除，因此也不会显示按下反馈：

```tsx
<View disabled={saving()} onPress={save} class="rounded-lg bg-emerald-500 px-4 py-2 active:bg-emerald-600 disabled:opacity-40">
  <Text class="font-medium text-white">Save</Text>
</View>
```

断点比较的是窗口（NSH 渲染所在的表面）而不是设备：在 iPad 分屏或可调整大小的窗口中，它们跟随窗口宽度变化。

## 与 Web 上的 Tailwind 的差异

- **单位是点（point）。** `p-4` 是 16 点（1 个间距步长 = 4pt，1rem = 16pt）。以 `px` 写的任意值也按点计算：`p-[13px]` 就是 13pt。
- **`flex` 是横向排列**，与 CSS 一致：`class="flex"` 会让子元素横向排列，除非有方向类另行指定（在字符串中的任何位置都可以：`flex-col flex` 是纵向）。没有 `flex` 或 `inline-flex` 的视图保持 NSH 默认的纵向排列。`block` 和 `flow-root` 使用块布局；`inline` 和 `inline-block` 按视图布局（没有行内格式化上下文）。
- **视图上的文字类会被继承。** 视图上的 `text-*`、`font-*`、`leading-*`、`tracking-*` 等文字工具类会像 CSS 继承那样传给其内部的 `<Text>` 和 `<Input>` 元素：`<View class="text-white">` 会让其中的标签变成白色。
- **`space-x-*` / `space-y-*` 是间距**（等同 `gap-x` / `gap-y`）。
- **在 NSH 中属于属性的设置，以类的形式也可用**：`line-clamp-*`、`truncate`、`whitespace-nowrap`、`pointer-events-*`、`object-*`（图片缩放模式）、`select-text`、`caret-*` 和 `placeholder:text-*` 都能使用；若同时设置了对应的显式属性（`numberOfLines`、`pointerEvents`、`resizeMode` 等），以属性为准。
- **不支持**：`peer-*` 变体、伪元素（`before:`、`after:`）、任意属性（`[mask-type:luminance]`）。原生无需处理的纯 Web 工具类（`antialiased`、`will-change-*`、`touch-*`、`scroll-*`、`snap-*`、`overscroll-*`、`box-border` 等）会被接受并忽略。

其余部分都与 Tailwind 4 文档一致：调色板、透明度修饰符（`bg-black/40`）、任意值（`bg-[#34c759]`、`w-[72px]`、`grid-cols-[1fr_2fr]`）、分数（`w-1/2`、`translate-x-1/2`）、阴影和 ring、渐变（`bg-linear-to-r`、`bg-radial`、`bg-conic` 配合 `from-` / `via-` / `to-`）、滤镜、`backdrop-blur-*`、`mix-blend-*`、变换、`transition-*` 和 `duration-*`、`animate-spin` / `ping` / `pulse` / `bounce`，以及网格。[滤镜与混合](./styling.md) 的平台限制对类名同样适用。

## 主题

类名以 Tailwind 4 的默认主题为准解析。在 `nsh.json` 的 `"tailwind"` 下扩展它；你的条目会加入默认值（同名时替换默认值）：

```json
{
  "tailwind": {
    "colors": {
      "brand": { "50": "#f5f3ff", "500": "#7c3aed", "600": "#6d28d9", "DEFAULT": "#7c3aed" },
      "ink": "#1f2328"
    },
    "radius": { "card": 14 },
    "fontFamily": { "display": "Pacifico" },
    "breakpoints": { "tablet": 600 }
  }
}
```

有了这个主题，`bg-brand-500`、`text-brand`（即 `DEFAULT` 色阶）、`border-ink`、`rounded-card`、`font-display` 和 `tablet:flex-row` 都是有效的类名。

| 键 | 取值 | 生成 |
|---|---|---|
| `colors` | 一个 CSS 颜色，或色阶对象（`"DEFAULT"` 对应不带后缀的名称） | `bg-*`、`text-*`、`border-*`、`from-*`、`ring-*` 等 |
| `spacing` | 每个间距步长的点数（默认 4） | `p-*`、`m-*`、`gap-*`、`w-*` 等 |
| `rem` | 任意值中每个 `rem` 的点数（默认 16） | |
| `breakpoints` | 点 | `name:` 变体 |
| `container` | 点 | `max-w-*` 尺寸 |
| `fontSize` | 点，或 `[size, lineHeight]`，行高为字号的倍数 | `text-*` |
| `fontWeight` | 1–1000 | `font-*` |
| `fontFamily` | 字体族名（系统字体族或[导入字体](./fonts.md)的名称） | `font-*` |
| `tracking` | em | `tracking-*` |
| `leading` | 字号的倍数 | `leading-*` |
| `radius` | 点 | `rounded-*` |
| `shadow`、`insetShadow`、`dropShadow` | 一个 CSS `box-shadow` | `shadow-*`、`inset-shadow-*`、`drop-shadow-*` |
| `blur` | 点 | `blur-*`、`backdrop-blur-*` |
| `perspective` | 点 | `perspective-*` |
| `aspect` | 比例（`1.5`） | `aspect-*` |
| `ease` | `[x1, y1, x2, y2]` | `ease-*` |

未知的键和无效的值会导致构建错误（`nsh.json: tailwind: unknown theme key 'colours'`）。主题会编译进 bundle，并在第一帧之前生效。

这个主题只影响你自己的类名。`nsh/ui/mobile` 的组件从组件主题，即 `nsh.json` 中的 `"theme"` 获取颜色（[组件](../components/index.md)）。

## 类名检查

`nsh build`、`nsh dev`、`nsh run` 和 `nsh trace` 会用运行时相同的解析器和主题检查 JSX 中的静态类名（`class` 中的字符串字面量以及 `classList` 的键），并对不存在的类发出警告：

```text
 warn nsh::build App.tsx:12:36: unknown class `bg-brand-550`
```

警告不会导致构建失败。运行时拼接的类名（``bg-${color()}-500``）无法在构建时检查；运行时第一次遇到未知类时会记录一次日志（`unknown classes: ...`）。

## 用类名还是样式？

两者都在原生层解析，开销相当。类名更简短，并且无需写代码就能使用变体（按下状态、深色模式、断点）。内联样式可以接收在 JavaScript 中计算出的值，也能接受[原生动画值](./animation.md)（`opacity: fade`、`transform: [{ translateX: x }]`）。大多数应用用类名描述静态外观，用 `style` 处理需要计算或动画的部分。

## 相关内容

- [样式](./styling.md)：style 属性以及各种效果的平台限制。
- [Tailwind 参考](../modules/tailwind-reference.md)：所有工具类和变体。
- [布局](./layout.md)：flexbox 与网格，同样可以用类名表达。

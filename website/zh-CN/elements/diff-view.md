---
title: DiffView
description: DiffView 组件：两段文本之间或一个补丁中的改动，并排或合并显示，高亮行内改动的单词，未改动的行折叠成可以展开的分隔行，带语法高亮和针对每处改动的操作。
order: 26.3
group: base
demo: el-diff-view
---

# DiffView

`DiffView` 像代码评审工具那样显示两段文本之间的改动：并排显示时旧文本在左、新文本在右，行与行对齐；合并显示时删除的行在新增的行上方。改动的行内，变化的单词会被高亮；改动之间未改动的行折叠成可以展开的分隔行；文本使用 [`Editor`](./editor.md) 的语法颜色高亮。它显示两段文本（`oldValue` 和 `newValue`）或统一 diff 格式中的一个文件（`patch`：`git diff` 的输出、拉取请求的 diff），为评审改动而设计，其中包括 AI 提出的修改：应用在每处改动上的操作可以保留或撤销它。只创建视野中的行，所以长文件滚动起来与短文件一样流畅。

`DiffView` 负责行为，并带有一套由主题令牌构成的默认外观：主题的 `font-mono`，13pt 字号、20pt 行高，行号用 `muted-foreground`，新增的行染上绿色、删除的行染上红色，改动的单词颜色更深，每个改动行的行号栏边缘有一条竖条，分隔行在 `muted` 底色上。每个部件都在这些默认值之上接受应用的样式（`classes` 和 `styles`）。Rust 用 `nsh_base::diff_view()` 创建它，TSX 用 `nsh/base` 中的 `<DiffView>`。

```rust tabs
use nsh_base::diff_view;
use nsh_core::ui::prelude::*;

const OLD: &str = "fn total(prices: &[u32]) -> u32 {\n    prices.iter().sum()\n}\n";
const NEW: &str = "fn total(prices: &[u64]) -> u64 {\n    prices.iter().copied().sum()\n}\n";

fn review() -> impl IntoElement {
    diff_view().old_value(OLD).new_value(NEW).language("rust").flex_1()
}
```

```tsx tabs
import { mount } from "nsh";
import { DiffView } from "nsh/base";

const OLD = `fn total(prices: &[u32]) -> u32 {
    prices.iter().sum()
}
`;
const NEW = `fn total(prices: &[u64]) -> u64 {
    prices.iter().copied().sum()
}
`;

function App() {
  return <DiffView oldValue={OLD} newValue={NEW} language="rust" class="flex-1" />;
}

mount(App);
```

视图填满应用给它的盒子（`flex-1`、一个高度），在其中滚动：纵向滚动所有行，每一侧横向滚动到它最长的一行。

<a id="two-texts-or-a-patch"></a>

## 两段文本或一个补丁

- **两段文本**：`oldValue` 和 `newValue`。逐行按原样比较，所以增加或去掉末尾的换行符也是一处改动，与 git 的显示相同。
- **一个补丁**：`patch`，一个文件的统一 diff（`git diff`、`git format-patch`、`diff -u`；重命名、文件模式、新增和删除的文件，以及前面的提交信息，都按 git 的写法读取）。视图显示它各个 hunk 的行，行号与文件中相同，每个 hunk 位于显示其标题（所在的函数）的分隔行之下。未设置 `language` 时，由文件路径决定语言。

```tsx
const PATCH = `diff --git a/src/total.rs b/src/total.rs
--- a/src/total.rs
+++ b/src/total.rs
@@ -10,3 +10,3 @@ impl Cart {
-    fn total(&self) -> u32 {
+    fn total(&self) -> u64 {
         self.prices.iter().sum()
     }
`;

<DiffView patch={PATCH} layout="unified" class="flex-1" />
```

`language` 的写法与 `Editor` 相同：名称、别名、扩展名或文件名（`"rust"`、`"tsx"`、`"main.go"`、`"Dockerfile"`）。

<a id="layout-and-inline-changes"></a>

## 布局与行内改动

`layout` 是 `"split"`（默认：并排，每个删除的行放在与它最相似的新增行旁边）或 `"unified"`（一列，一处改动的删除行在新增行上方，两种行号都显示）。`inline` 决定改动行内高亮什么：

| `inline` | 高亮的内容 |
|---|---|
| `"wordAlt"`（默认） | 改动的单词，相隔一个字符时合并，使改动的短语作为一个整体呈现 |
| `"word"` | 每个改动的单词 |
| `"char"` | 每个改动的字符 |
| `"none"` | 不高亮：只有行的底色 |

单词是由字母、数字和 `_` 组成的连续字符；每个汉字、假名和韩文音节各自是一个单词，所以句子中改动了一个字时只标记这一个字。几乎整行都改动了的行只显示行的底色。

`indicators` 在行号旁标记改动的行：`"bars"`（默认，行号栏边缘的一条竖条）、`"classic"`（文本前的 `+` 和 `−`）或 `"none"`。`background={false}` 让改动的行不染色，`lineNumbers={false}` 隐藏行号。

```tsx
<DiffView oldValue={before()} newValue={after()} language="md" layout="unified" inline="word" indicators="classic" class="flex-1" />
```

<a id="collapsed-lines"></a>

## 折叠的行

每处改动周围显示 `contextLines` 行未改动的行（默认 3 行）；改动之间、第一处改动之前和最后一处改动之后未改动的行折叠成一个分隔行，标明它隐藏了多少行。点击它会显示全部。`expandAll` 显示所有行；ref 的 `expand`、`expandAll()` 和 `collapseAll()` 由应用展开和折叠它们。

`renderSeparator` 按应用的方式绘制分隔行。它得到分隔行的信息（`DiffSeparator`）：`gap`，折叠段的序号（第一处改动之前为 0，改动 `i - 1` 和 `i` 之间为 `i`）；`lines`，隐藏的行数；`first` 和 `last`；补丁的话还有 hunk 的 `header`。`expand(gap, direction, lines)` 显示 `"all"`，即全部行（默认），或者 `lines` 行（默认 20 行）：`"up"` 是分隔行下方那处改动紧上方的行，`"down"` 是分隔行上方那处改动紧下方的行。第一个分隔行向上展开，最后一个向下展开。

```tsx
let diff!: DiffViewRef;

<DiffView
  ref={diff}
  oldValue={before()}
  newValue={after()}
  language="ts"
  renderSeparator={(sep) => (
    <View class="flex-1 flex-row items-center gap-1 bg-muted px-2">
      <If when={!sep.first}>
        <Button onPress={() => diff.expand(sep.gap, "down")} class="px-1"><Text>↓</Text></Button>
      </If>
      <If when={!sep.last}>
        <Button onPress={() => diff.expand(sep.gap, "up")} class="px-1"><Text>↑</Text></Button>
      </If>
      <Button onPress={() => diff.expand(sep.gap)} class="flex-1">
        <Text class="text-xs text-muted-foreground">{sep.header ?? `${sep.lines} unchanged lines`}</Text>
      </Button>
    </View>
  )}
  class="flex-1"
/>
```

<a id="keeping-and-reverting-changes"></a>

## 保留和撤销改动

`renderHunkActions` 把应用的操作放在视野中每处改动的右上角，并告诉它这处改动（`DiffHunk`：`index`、`oldStart`、`oldLines`、`newStart`、`newLines`，行号从 0 开始）。这些操作调用 ref：`revertHunk(index)` 把这处改动的旧行放回新文本，`acceptHunk(index)` 让它的新行成为旧文本的内容。视图立即显示结果，并用 `onChange({ oldValue, newValue })` 报告两段文本，由应用把它们设为属性。逐处保留或撤销 AI 的修改就是这样：

```tsx
import { signal, Text, View } from "nsh";
import { Button, DiffView, type DiffViewRef } from "nsh/base";

function Proposal(props: { file: string; current: string; proposed: string; onApply: (text: string) => void }) {
  let diff!: DiffViewRef;
  const current = signal(props.current);
  const proposed = signal(props.proposed);
  return (
    <View class="flex-1">
      <DiffView
        ref={diff}
        oldValue={current()}
        newValue={proposed()}
        language={props.file}
        onChange={({ oldValue, newValue }) => {
          current.set(oldValue);
          proposed.set(newValue);
          props.onApply(oldValue);
        }}
        renderHunkActions={(hunk) => (
          <View class="flex-row gap-1">
            <Button onPress={() => diff.revertHunk(hunk.index)} class="rounded bg-background px-2 py-0.5 border border-border">
              <Text class="text-xs">Revert</Text>
            </Button>
            <Button onPress={() => diff.acceptHunk(hunk.index)} class="rounded bg-primary px-2 py-0.5">
              <Text class="text-xs text-primary-foreground">Keep</Text>
            </Button>
          </View>
        )}
        class="flex-1"
      />
    </View>
  );
}
```

这里旧文本就是文件现在的内容：保留一处改动就把它应用到文件（`onApply`），撤销它就把它从提议中去掉。显示 `patch` 的视图显示的是文件的片段，它的改动保持原样。

<a id="methods"></a>

## 方法

`ref={diff}` 提供视图的方法（`DiffViewRef`）。返回值的方法（`hunks()`、`stats()`）在 JS 中返回一个 `Promise`，所以要 `await` 它们：`const changes = await diff.hunks()`。要在界面上显示的统计数字，由 `onStatsChange` 在变化时报告。

| 方法 | 说明 |
|---|---|
| `hunks()` | `Promise<DiffHunk[]>`：所有改动，按顺序。 |
| `stats()` | `Promise<DiffStats>`：`{ added, deleted, hunks }`，新增和删除的行数，以及改动的数量。 |
| `goToHunk(index)` | 把第 `index` 处改动滚动到视野中，它的第一行在顶部。 |
| `revertHunk(index)` | 把第 `index` 处改动的旧行放回新文本，并报告两段文本（`onChange`）。 |
| `acceptHunk(index)` | 让第 `index` 处改动的新行成为旧文本的内容，并报告两段文本（`onChange`）。 |
| `expand(gap, direction?, lines?)` | 显示分隔行 `gap` 隐藏的行：`"all"`（默认），或者 `lines` 行（默认 20 行），`"up"` 或 `"down"`。 |
| `expandAll()` | 显示所有未改动的行。 |
| `collapseAll()` | 重新折叠未改动的行。 |

上一处和下一处改动的按钮，以及来自 `onStatsChange` 的统计数字：

```tsx
let diff!: DiffViewRef;
const at = signal(0);
const stats = signal<DiffStats>({ added: 0, deleted: 0, hunks: 0 });
const go = (index: number) => {
  at.set(index);
  diff.goToHunk(index);
};

<View class="flex-row items-center gap-2 p-2">
  <Text class="text-sm text-green-600">+{stats().added}</Text>
  <Text class="text-sm text-red-600">−{stats().deleted}</Text>
  <Text class="text-sm text-muted-foreground">{stats().hunks} changes</Text>
  <Button onPress={() => go(Math.max(0, at() - 1))}><Text>Previous</Text></Button>
  <Button onPress={() => go(Math.min(stats().hunks - 1, at() + 1))}><Text>Next</Text></Button>
</View>
<DiffView ref={diff} oldValue={before()} newValue={after()} onStatsChange={(s) => stats.set(s)} class="flex-1" />
```

<a id="comments-on-lines"></a>

## 行上的评论

点击行号会报告 `onLineNumberPress({ side, line })`：所在的一侧（`"old"` 或 `"new"`）和从 0 开始的行号，可以在这里放应用显示的评审评论。

<a id="styling-the-parts"></a>

## 部件样式

`class` 和 `style` 设置视图盒子的样式。`classes` 按部件接受 Tailwind 类，`styles` 按部件接受内联样式；部件的类在它自己的默认样式之后应用，因此优先，它的 `styles` 又优先于类。字体用 `fontFamily`、`fontSize` 和 `lineHeight` 设置。

```tsx
<DiffView
  oldValue={before()}
  newValue={after()}
  language="py"
  fontSize={12}
  lineHeight={18}
  class="flex-1 rounded-lg border border-border"
  classes={{
    gutter: "bg-muted",
    lineAdded: "bg-blue-500/10",
    wordAdded: "bg-blue-500/30",
    indicatorAdded: "bg-blue-500",
    separator: "bg-transparent",
  }}
/>
```

| 部件 | 是什么 | 默认值 |
|---|---|---|
| `side` | 每一侧（并排）或唯一的一列（合并） | 两侧之间一条 `border` 线 |
| `gutter` | 一侧的行号和标记 | |
| `lineNumber` | 每个行号 | 右对齐，`muted-foreground` |
| `lineNumberAdded` | 新增行的行号，在 `lineNumber` 之后应用 | |
| `lineNumberDeleted` | 删除行的行号，在 `lineNumber` 之后应用 | |
| `content` | 一侧的文本，在行号栏旁边 | 文本前后各 8pt |
| `line` | 一侧的每一行 | |
| `lineAdded` | 新增的行，在 `line` 之后应用 | 绿色底色 |
| `lineDeleted` | 删除的行，在 `line` 之后应用 | 红色底色 |
| `filler` | 并排行中另一侧没有对应行时空着的一半 | 淡淡的 `muted` 填充 |
| `wordAdded` | 新增行内改动的单词 | 更深的绿色 |
| `wordDeleted` | 删除行内改动的单词 | 更深的红色 |
| `indicatorAdded` | 新增行的竖条（`"bars"`）或 `+`（`"classic"`） | 绿色，3pt 宽 |
| `indicatorDeleted` | 删除行的竖条或 `−` | 红色，3pt 宽 |
| `separator` | 折叠行的分隔行，或补丁的 hunk 标题 | `muted` 底色上的 `⋯` 和行数 |
| `hunkActions` | 应用在一处改动上的操作所在的盒子 | 改动的右上角 |

<a id="props"></a>

## 属性

| 属性 | Rust | 类型 | 默认值 | 说明 |
|---|---|---|---|---|
| `oldValue` | `.old_value(..)` | `string` | | 旧文本。 |
| `newValue` | `.new_value(..)` | `string` | | 新文本。 |
| `patch` | `.patch(..)` | `string` | | 一个文件的统一 diff，在未设置 `oldValue` 和 `newValue` 时显示。 |
| `language` | `.language(..)` | `string` | 纯文本（补丁：其文件的语言） | 语言的名称、别名、扩展名或文件名。 |
| `layout` | `.layout(..)` | `"split" \| "unified"` | `"split"` | 并排或一列。 |
| `inline` | `.inline(..)` | `"wordAlt" \| "word" \| "char" \| "none"` | `"wordAlt"` | 改动行内高亮的内容。 |
| `indicators` | `.indicators(..)` | `"bars" \| "classic" \| "none"` | `"bars"` | 在行号旁如何标记改动的行。 |
| `contextLines` | `.context_lines(..)` | `number` | `3` | 每处改动周围显示的未改动行数。 |
| `expandAll` | `.expand_all(..)` | `boolean` | `false` | 显示所有未改动的行。 |
| `lineNumbers` | `.line_numbers(..)` | `boolean` | `true` | 行号。 |
| `background` | `.background(..)` | `boolean` | `true` | 改动的行带底色。 |
| `fontFamily` | `.font_family(..)` | `string` | 主题的 `font-mono` | 文本的字体。 |
| `fontSize` | `.font_size(..)` | `number` | `13` | 字号，单位为点。 |
| `lineHeight` | `.line_height(..)` | `number` | `20` | 一行的高度，单位为点。 |
| `renderSeparator` | `.render_separator(..)` | `(separator: DiffSeparator) => JSX.Element` | | 绘制折叠行的分隔行。 |
| `renderHunkActions` | `.render_hunk_actions(..)` | `(hunk: DiffHunk) => JSX.Element` | | 应用在每处改动上的操作。 |
| `classes` | `.classes(..)` | `DiffViewClasses` | `{}` | 按部件的 Tailwind 类。 |
| `styles` | `.styles(..)` | `DiffViewStyles` | `{}` | 按部件的内联样式，优先于类。 |
| `class`、`style` | 样式方法 | `string`、`StyleProp<ViewStyle>` | | 视图的盒子。 |

<a id="events"></a>

## 事件

| 事件 | Rust | 说明 |
|---|---|---|
| `onChange` | `.on_change(\|change: &DiffChange\| ..)` | 一处改动被保留或撤销：`{ oldValue, newValue }`，现在的两段文本。 |
| `onLineNumberPress` | `.on_line_number_press(\|line: &DiffLine\| ..)` | 行号被点击：`{ side, line }`。 |
| `onStatsChange` | `.on_stats_change(\|stats: &DiffStats\| ..)` | 统计数字变了：`{ added, deleted, hunks }`。视图创建时触发一次，之后每次比较文本得到不同的数字时再触发，用于显示“+8 −8 · 5 处改动”这样的一行。 |

<a id="related"></a>

## 相关内容

- [Editor](./editor.md)：代码编辑器，`DiffView` 使用它的语法颜色和语言。
- [TextView](./text-view.md)：Markdown 和 HTML 文档，用于评审周围的文字。
- [`nsh/base`](../modules/nsh-base.md)：其他基础组件。

---
title: Editor
description: Editor 组件：代码编辑器，带语法高亮、行号、多光标、折叠、软换行、查找与替换、诊断和装饰，以及代码编辑器的按键。
order: 26.2
group: base
demo: el-editor
---

# Editor

`Editor` 是一个代码编辑器：五十多种语言的语法高亮、行号、多光标、折叠、软换行、查找与替换、诊断和装饰，以及代码编辑器的按键（移动和复制行、注释、缩进、输入时自动闭合的括号和引号）。它能打开任意长度的文件：只创建视野中的行，高亮随每次编辑逐行更新，所以长文件滚动和输入起来与短文件一样流畅。输入经由平台的输入系统，所以输入法（拼音、假名、死键）和软键盘在其中的表现与任何文本框相同。

编辑器负责行为，并带有一套由主题令牌构成的默认外观：主题的 `font-mono`，13pt 字号、20pt 行高，行号用 `muted-foreground`，有光标的行后面是一条淡淡的 `muted` 底色，选区是 `primary` 的浅色，缩进参考线用 `border`，语法颜色是内置的，与 [`TextView`](./text-view.md) 的代码块相同，分浅色和深色两套。每个部件都在这些默认值之上接受应用的样式（`classes` 和 `styles`）。Rust 用 `nsh_base::editor()` 创建它，TSX 用 `nsh/base` 中的 `<Editor>`。

```rust tabs
use nsh_base::editor;
use nsh_core::ui::prelude::*;

const MAIN: &str = "fn main() {\n    println!(\"Hello, world!\");\n}\n";

fn code() -> impl IntoElement {
    let source = signal(SharedString::from(MAIN));
    editor()
        .value(move || Some(source.get()))
        .language("rust")
        .on_change(move |value: &String| source.set(value.clone().into()))
        .flex_1()
}
```

```tsx tabs
import { mount, signal } from "nsh";
import { Editor } from "nsh/base";

const MAIN = `fn main() {
    println!("Hello, world!");
}
`;

function App() {
  const source = signal(MAIN);
  return <Editor value={source()} onChange={(value) => source.set(value)} language="rust" class="flex-1" />;
}

mount(App);
```

编辑器填满应用给它的盒子（`flex-1`、一个高度），在其中滚动文本：纵向总是可以滚动；行比盒子长时可以横向滚动；开启 `softWrap` 后长行在编辑器的宽度处换行，不再横向滚动。

<a id="the-text"></a>

## 文本

- **受控**：传入 `value`，并在 `onChange` 中重新设置它，`onChange` 报告完整的新文本。应用修改的 `value`（加载了文件、格式化工具的输出）会作为一次可撤销的编辑替换文本，远离修改处的光标和折叠保持原位。
- **非受控**：传入 `defaultValue`，从 `onChange` 或 ref 的 `value()` 读取文本。

换行符是 `\n`：传入文本中的 `\r\n` 和 `\r` 都变成 `\n`。

<a id="languages"></a>

## 语言

`language` 用名称、别名、扩展名或文件名指定语言，不区分大小写：`"rust"`、`"rs"`、`"typescript"`、`"tsx"`、`"main.go"`、`"Dockerfile"`、`"Makefile"`。它决定高亮、`mod+/` 切换的注释、可匹配并参与缩进的括号、输入时自动闭合的符号对、组成单词的字符，以及默认的缩进：四个空格，Web 语言（JavaScript、TypeScript、JSON、HTML、CSS、YAML 等）两个空格，Go 和 Makefile 用制表符。`tabSize` 和 `hardTabs` 为所有语言设置缩进。其他名称都是纯文本。

支持的语言：Rust、JavaScript、JSX、TypeScript、TSX、JSON、Python、Go、C、C++、C#、Java、Kotlin、Swift、Objective-C、Objective-C++、Ruby、PHP、Lua、Shell、Fish、PowerShell、SQL、HTML、XML、Vue、Svelte、CSS、SCSS、Sass、Less、Markdown、YAML、TOML、INI、Dockerfile、Makefile、CMake、Dart、Scala、Elixir、Erlang、Haskell、OCaml、F#、Clojure、Zig、Perl、R、Julia、Nix、Terraform、Protocol Buffers、GraphQL 和 Diff。

```tsx
<Editor value={source()} onChange={(v) => source.set(v)} language={file().name} tabSize={2} softWrap class="flex-1" />
```

<a id="editing"></a>

## 编辑

编辑器的工作方式与代码编辑器相同：

- **多光标。** Alt 点击（Option 点击）添加一个光标，`mod+d` 选中选区的下一处出现，`mod+shift+l` 选中全部出现，添加光标的按键在上一行或下一行添加一个。输入、删除、移动和每个命令都在每个光标处执行；Escape 回到一个。
- **缩进。** 新行保持当前行的缩进，在左括号（Python 的 `:`、Ruby 的 `do` 等）之后再加一级；在一对括号之间按 Enter，右括号单独放到下一行。Tab 和 Shift+Tab 缩进和反缩进所选的行；在行首空格中按 Backspace 回到上一个缩进位置。`autoIndent` 关闭这些行为。
- **括号和引号** 在输入时自动闭合，包住选区，再次输入闭合符号时跳过它（`autoClose`）。光标处的括号和与它配对的括号会加上边框。
- **撤销** 以一整段输入为单位：一个单词、一次删除、一个命令。移动或点击开始新的一步。
- **折叠。** 后面跟着缩进更深的行的那一行，可以用行号栏中的标记把这些行折叠起来：`⌄` 折叠，`›` 和行后的 `⋯` 展开。光标移入折叠处时，折叠会打开。
- **点击。** 使用指针时，单击放置光标，双击选中一个单词，三击选中一行，Shift 点击扩展选区，拖动按点击所选的单位选择。在触摸屏上，轻点放置光标。
- **剪贴板。** 没有选区时复制和剪切取光标所在的整行，粘贴时这些行会作为整行插入到光标所在行的上方。

`readOnly` 保持文本不变，同时仍可选择、搜索和复制。

<a id="keys"></a>

## 按键

编辑器的按键在它拥有焦点时生效；其他输入框保留各自的按键。`mod` 在 macOS 和 iOS 上是 Command，在其他平台上是 Control（`nsh/keymap` 的按键写法）。

| 按键（macOS） | 按键（Windows、Linux） | 命令 |
|---|---|---|
| 方向键，加 Shift 选择 | 方向键，加 Shift 选择 | 按字符和行移动 |
| `alt+left` / `alt+right` | `ctrl+left` / `ctrl+right` | 按单词移动 |
| `cmd+left` / `cmd+right`、`home` / `end`、`ctrl+a` / `ctrl+e` | `home` / `end` | 行的第一个非空白字符（再按到行首）/ 行尾 |
| `cmd+up` / `cmd+down` | `ctrl+home` / `ctrl+end` | 文本的开头 / 结尾 |
| `pageup` / `pagedown` | `pageup` / `pagedown` | 上翻 / 下翻一页 |
| `alt+backspace` / `alt+delete` | `ctrl+backspace` / `ctrl+delete` | 向左 / 向右删除一个单词 |
| `cmd+backspace` / `cmd+delete`、`ctrl+k` | `ctrl+shift+backspace` / `ctrl+shift+delete` | 删除到行首 / 行尾 |
| `cmd+z` / `cmd+shift+z` | `ctrl+z` / `ctrl+shift+z`、`ctrl+y` | 撤销 / 重做 |
| `cmd+a` | `ctrl+a` | 全选 |
| `cmd+l` | `ctrl+l` | 选中整行（再按：连同下一行） |
| `cmd+d` | `ctrl+d` | 选中下一处出现 |
| `cmd+shift+l` | `ctrl+shift+l` | 选中每一处出现 |
| `cmd+alt+up` / `cmd+alt+down` | `ctrl+alt+up` / `ctrl+alt+down` | 在上方 / 下方添加光标 |
| `escape` | `escape` | 回到一个选区 |
| `alt+up` / `alt+down` | `alt+up` / `alt+down` | 把行上移 / 下移 |
| `alt+shift+up` / `alt+shift+down` | `alt+shift+up` / `alt+shift+down` | 向上 / 向下复制行 |
| `cmd+shift+k` | `ctrl+shift+k` | 删除行 |
| `cmd+enter` / `cmd+shift+enter` | `ctrl+enter` / `ctrl+shift+enter` | 在下方 / 上方新建一行 |
| `tab` / `shift+tab`、`cmd+]` / `cmd+[` | `tab` / `shift+tab`、`ctrl+]` / `ctrl+[` | 缩进 / 反缩进 |
| `cmd+/` | `ctrl+/` | 切换行注释 |
| `cmd+shift+\` | `ctrl+shift+\` | 跳到匹配的括号 |
| `ctrl+j` | `ctrl+j` | 合并行 |
| `ctrl+t` | | 交换光标两侧的字符 |
| `cmd+c` / `cmd+x` / `cmd+v` | `ctrl+c` / `ctrl+x` / `ctrl+v` | 复制 / 剪切 / 粘贴 |

macOS 和 iOS 还有 Cocoa 的文本按键：`ctrl+b` / `ctrl+f` / `ctrl+p` / `ctrl+n` 移动，`ctrl+h` / `ctrl+d` 删除。

查找、折叠和应用自己的命令使用应用通过 [`nsh/keymap`](../modules/nsh-keymap.md) 绑定的按键，经由 ref 的 `run` 执行：

```tsx
import { keymap } from "nsh/keymap";

let editor!: EditorRef;

keymap.bind({
  "mod+g": () => editor.run("findNext"),
  "mod+shift+g": () => editor.run("findPrevious"),
  "mod+alt+[": () => editor.run("fold"),
  "mod+alt+]": () => editor.run("unfold"),
});
```

<a id="find-and-replace"></a>

## 查找与替换

`search` 是要查找的内容：`{ query, caseSensitive?, wholeWord?, regex? }`。它的匹配项会被高亮，选区所在的那一项更醒目，`onSearchChange` 报告 `{ count, current, error }`：匹配项的数量、选区所在的是哪一项（从 0 开始），以及正则表达式无效的原因。`findNext` 和 `findPrevious` 选中下一个和上一个匹配项，到头后绕回；`replace(text)` 替换当前匹配项并选中下一个，`replaceAll(text)` 把所有匹配项作为一次编辑替换，并返回替换的数量。正则表达式的替换文本用 `$1` 和 `${name}` 引用它的分组。

查找栏由应用自己实现：

```tsx
import { Input, signal, Text, View } from "nsh";
import { Button, Editor, type EditorRef, type EditorSearchResult } from "nsh/base";

function Find(props: { source: string; onChange: (value: string) => void }) {
  let editor!: EditorRef;
  const query = signal("");
  const replacement = signal("");
  const result = signal<EditorSearchResult>({ count: 0 });
  const status = () =>
    result().error ?? (result().current != null ? `${result().current! + 1} of ${result().count}` : `${result().count} found`);
  return (
    <View class="flex-1">
      <View class="flex-row items-center gap-2 border-b border-border p-2">
        <Input value={query()} onChange={(q) => query.set(q)} onSubmit={() => editor.run("findNext")} placeholder="Find" class="flex-1" />
        <Text class="text-sm text-muted-foreground">{status()}</Text>
        <Button onPress={() => editor.run("findPrevious")} class="px-2"><Text>↑</Text></Button>
        <Button onPress={() => editor.run("findNext")} class="px-2"><Text>↓</Text></Button>
        <Input value={replacement()} onChange={(r) => replacement.set(r)} placeholder="Replace" class="flex-1" />
        <Button onPress={() => editor.replace(replacement())} class="px-2"><Text>Replace</Text></Button>
        <Button onPress={async () => console.log("replaced", await editor.replaceAll(replacement()))} class="px-2"><Text>All</Text></Button>
      </View>
      <Editor
        ref={editor}
        value={props.source}
        onChange={props.onChange}
        language="ts"
        search={query() ? { query: query() } : undefined}
        onSearchChange={(r) => result.set(r)}
        class="flex-1"
      />
    </View>
  );
}
```

<a id="diagnostics"></a>

## 诊断

`diagnostics` 标记语言服务器或 linter 报告的问题：在每个范围下方画一条线，颜色由严重程度决定（`error`、`warning`、`info`、`hint`）。位置是从 0 开始的行和 UTF-16 列，与语言服务器相同，所以它的诊断可以原样传入。用户输入时这些标记留在它们的文本上，直到应用传入新的诊断；消息的显示（一个列表、行旁的悬浮卡片）由应用负责。

```tsx
const problems = signal<EditorDiagnostic[]>([]);

async function lint(text: string) {
  problems.set(await check(text)); // [{ range, severity, message, source?, code? }]
}

<Editor
  value={source()}
  onChange={(v) => {
    source.set(v);
    lint(v);
  }}
  language="ts"
  diagnostics={problems()}
  classes={{ diagnosticWarning: "bg-orange-500" }}
  class="flex-1"
/>
```

<a id="decorations"></a>

## 装饰

`decorations` 为应用选择的范围设置样式：来自语言服务器的语义高亮、正在执行的语句、评审中改动的行。装饰的 `class` 和 `style` 设置其文本的样式；`lineClass` 和 `lineStyle` 设置其所在各行的样式，位于文本后面。与诊断一样，它们在编辑中留在自己的文本上，直到应用设置新的装饰。

```tsx
const range = (line: number, start: number, end: number) => ({ start: { line, column: start }, end: { line, column: end } });

<Editor
  value={source()}
  language="sql"
  readOnly
  decorations={[
    { range: range(4, 0, 0), lineClass: "bg-green-500/10" },
    { range: range(7, 6, 18), class: "underline text-blue-600" },
  ]}
  class="flex-1"
/>
```

<a id="breakpoints-and-context-menus"></a>

## 断点和上下文菜单

点击行号会报告 `onLineNumberPress(line)`，可以在这里设置断点，由应用画成一个装饰。辅助按键（右键单击、Mac 上的 Control 点击）报告 `onContextMenu({ position, x, y })`：指针下文本中的位置和根视图中的坐标，应用在那里打开自己的菜单。

```tsx
const breakpoints = signal<number[]>([]);

<Editor
  value={source()}
  language="py"
  onLineNumberPress={(line) =>
    breakpoints.set(breakpoints().includes(line) ? breakpoints().filter((l) => l !== line) : [...breakpoints(), line])
  }
  decorations={breakpoints().map((line) => ({ range: range(line, 0, 0), lineClass: "bg-red-500/15" }))}
  class="flex-1"
/>
```

<a id="methods"></a>

## 方法

`ref={editor}` 提供编辑器的方法（`EditorRef`），供应用的按钮、菜单和按键使用。返回值的方法（`value()`、`selections()`、`replaceAll()`、`canUndo()`、`canRedo()`）在 JS 中返回一个 `Promise`，所以要 `await` 它们：`const text = await editor.value()`。

| 方法 | 说明 |
|---|---|
| `focus()`、`blur()` | 获得焦点，或放弃焦点。 |
| `value()` | `Promise<string>`：文本。 |
| `setValue(value)` | 替换文本，作为一次可撤销的编辑。 |
| `insert(text)` | 在每个选区处原样输入 `text`（括号不会自动闭合）。 |
| `selections()` | `Promise<EditorSelection[]>`：每个选区 `{ anchor, head }`，主选区（最新的）在最后。 |
| `setSelections(selections)` | 设置选区（最后一个为主选区），并把主选区滚动到视野中。 |
| `run(command)` | 执行一个命令，与它的按键效果相同（[命令](#commands)）。 |
| `replace(text)` | 替换当前匹配项并选中下一个。 |
| `replaceAll(text)` | `Promise<number>`：把所有匹配项作为一次编辑替换；返回数量。 |
| `fold(line)`、`unfold(line)` | 折叠第 `line` 行（从 0 开始）下面的行，或展开它。 |
| `scrollToLine(line)` | 把第 `line` 行滚动到视野中，并展开隐藏它的折叠。 |
| `canUndo()`、`canRedo()` | `Promise<boolean>`：是否有可以撤销或重做的编辑。 |

```tsx
let editor!: EditorRef;

<Button onPress={() => editor.run("toggleComment")}><Text>Comment</Text></Button>
<Button onPress={async () => save(await editor.value())}><Text>Save</Text></Button>
<Button onPress={async () => (await editor.canUndo()) && editor.run("undo")}><Text>Undo</Text></Button>
<Button onPress={() => editor.setSelections([{ anchor: { line: 0, column: 0 }, head: { line: 0, column: 0 } }])}>
  <Text>Go to the top</Text>
</Button>
<Editor ref={editor} value={source()} onChange={(v) => source.set(v)} language="rs" class="flex-1" />
```

<a id="commands"></a>

### 命令

`run` 按名称接受编辑器的命令（`EditorCommand`）：

| 命令 | 作用 |
|---|---|
| `moveLeft`、`moveRight`、`moveUp`、`moveDown`、`moveWordLeft`、`moveWordRight`、`moveLineStart`、`moveLineEnd`、`movePageUp`、`movePageDown`、`moveDocumentStart`、`moveDocumentEnd` | 移动光标。 |
| `selectLeft`、`selectRight`、`selectUp`、`selectDown`、`selectWordLeft`、`selectWordRight`、`selectLineStart`、`selectLineEnd`、`selectPageUp`、`selectPageDown`、`selectDocumentStart`、`selectDocumentEnd` | 扩展选区。 |
| `moveToMatchingBracket` | 跳到与光标处括号匹配的括号。 |
| `selectAll`、`selectLine`、`selectWord`、`selectNextOccurrence`、`selectAllOccurrences` | 选择。 |
| `addCaretAbove`、`addCaretBelow`、`singleSelection` | 添加光标；回到一个选区。 |
| `deleteBackward`、`deleteForward`、`deleteWordLeft`、`deleteWordRight`、`deleteToLineStart`、`deleteToLineEnd`、`deleteLine` | 删除。 |
| `newline`、`newlineBelow`、`newlineAbove`、`indent`、`outdent`、`toggleComment` | 行与缩进。 |
| `moveLinesUp`、`moveLinesDown`、`duplicateLinesUp`、`duplicateLinesDown`、`joinLines`、`transposeCharacters` | 重新排列。 |
| `undo`、`redo` | 撤销和重做。 |
| `fold`、`unfold`、`foldAll`、`unfoldAll` | 在光标所在行折叠，或折叠全部。 |
| `findNext`、`findPrevious` | 选中 `search` 的下一个或上一个匹配项。 |
| `copy`、`cut`、`paste` | 剪贴板。 |

<a id="styling-the-parts"></a>

## 部件样式

`class` 和 `style` 设置编辑器盒子的样式。`classes` 按部件接受 Tailwind 类，`styles` 按部件接受内联样式；部件的类在它自己的默认样式之后应用，因此优先，它的 `styles` 又优先于类。字体用 `fontFamily`、`fontSize` 和 `lineHeight` 设置，编辑器用它们测量文本。

```tsx
<Editor
  value={source()}
  onChange={(v) => source.set(v)}
  language="tsx"
  fontFamily="JetBrains Mono"
  fontSize={14}
  lineHeight={22}
  class="flex-1 rounded-lg border border-border"
  classes={{
    gutter: "bg-muted",
    lineNumber: "text-xs",
    activeLine: "bg-yellow-500/10",
    selection: "bg-blue-500/30",
  }}
  styles={{ caret: { backgroundColor: "#f59e0b", width: 3 } }}
/>
```

| 部件 | 是什么 | 默认值 |
|---|---|---|
| `gutter` | 行号和折叠标记所在的列 | 数字前 12pt，标记后 4pt |
| `lineNumber` | 每个行号 | 右对齐，`muted-foreground` |
| `activeLineNumber` | 有光标的行的行号，在 `lineNumber` 之后应用 | `foreground` |
| `foldMarker` | 折叠开始处那一行的标记 | `⌄`，折叠后为 `›`，`muted-foreground` |
| `content` | 行号栏旁边的文本区域 | 文本前 4pt |
| `line` | 每一行文本 | |
| `activeLine` | 有光标的行的背后 | `muted`，很淡 |
| `selection` | 所选文本的背后 | `primary` 的浅色 |
| `caret` | 光标 | 2pt 宽，`foreground`，闪烁 |
| `composition` | 输入法正在组合的文本 | 下划线 |
| `searchMatch` | `search` 的匹配项 | 半透明的琥珀色 |
| `currentSearchMatch` | 选区所在的匹配项，在 `searchMatch` 之后应用 | 更深的琥珀色 |
| `matchingBracket` | 光标处的括号和与它配对的括号 | 1pt 边框 |
| `indentGuide` | 每级缩进一条参考线 | 一条 1pt 的 `border` 线 |
| `foldPlaceholder` | 折叠行在其文本之后显示的内容 | ` ⋯ `，`muted-foreground` |
| `placeholder` | 文本为空时显示 | `muted-foreground` |
| `diagnosticError` | 错误下方的线 | `destructive` |
| `diagnosticWarning` | 警告下方的线 | 琥珀色 |
| `diagnosticInfo` | 信息下方的线 | 蓝色 |
| `diagnosticHint` | 提示下方的线 | `muted-foreground` |

<a id="props"></a>

## 属性

| 属性 | Rust | 类型 | 默认值 | 说明 |
|---|---|---|---|---|
| `value` | `.value(..)` | `string` | | 文本，由应用控制（在 `onChange` 中重新设置）。 |
| `defaultValue` | `.default_value(..)` | `string` | | 编辑器自己保存文本时的初始文本。 |
| `language` | `.language(..)` | `string` | 纯文本 | 语言的名称、别名、扩展名或文件名（[语言](#languages)）。 |
| `placeholder` | `.placeholder(..)` | `string` | | 文本为空时显示。 |
| `readOnly` | `.read_only(..)` | `boolean` | `false` | 文本可以选择和复制，不能修改。 |
| `autoFocus` | `.auto_focus(..)` | `boolean` | `false` | 出现时获得焦点。 |
| `lineNumbers` | `.line_numbers(..)` | `boolean` | `true` | 行号栏中的行号。 |
| `softWrap` | `.soft_wrap(..)` | `boolean` | `false` | 长行在编辑器的宽度处换行。 |
| `tabSize` | `.tab_size(..)` | `number` | 语言的设置 | 一个制表符和一级缩进的列数。 |
| `hardTabs` | `.hard_tabs(..)` | `boolean` | 语言的设置 | 用制表符缩进。 |
| `folding` | `.folding(..)` | `boolean` | `true` | 缩进的行可以用行号栏中的标记折叠。 |
| `autoClose` | `.auto_close(..)` | `boolean` | `true` | 括号和引号在输入时自动闭合。 |
| `autoIndent` | `.auto_indent(..)` | `boolean` | `true` | 新行保持缩进，并在左括号之后增加缩进。 |
| `activeLine` | `.active_line(..)` | `boolean` | `true` | 高亮有光标的行。 |
| `indentGuides` | `.indent_guides(..)` | `boolean` | `true` | 缩进参考线。 |
| `scrollBeyondLastLine` | `.scroll_beyond_last_line(..)` | `boolean` | `true` | 文本可以滚动到最后一行之后，最多一屏。 |
| `fontFamily` | `.font_family(..)` | `string` | 主题的 `font-mono` | 绘制和测量文本所用的字体。 |
| `fontSize` | `.font_size(..)` | `number` | `13` | 字号，单位为点。 |
| `lineHeight` | `.line_height(..)` | `number` | `20` | 一行的高度，单位为点。 |
| `search` | `.search(..)` | `EditorSearch` | | 要查找的内容（[查找与替换](#find-and-replace)）。 |
| `diagnostics` | `.diagnostics(..)` | `EditorDiagnostic[]` | `[]` | 要标记的问题（[诊断](#diagnostics)）。 |
| `decorations` | `.decorations(..)` | `EditorDecoration[]` | `[]` | 应用设置样式的范围（[装饰](#decorations)）。 |
| `classes` | `.classes(..)` | `EditorClasses` | `{}` | 按部件的 Tailwind 类。 |
| `styles` | `.styles(..)` | `EditorStyles` | `{}` | 按部件的内联样式，优先于类。 |
| `accessibilityLabel` | `.accessibility_label(..)` | `string` | | 读屏软件对编辑器的称呼。 |
| `class`、`style` | 样式方法 | `string`、`StyleProp<ViewStyle>` | | 编辑器的盒子。 |

<a id="events"></a>

## 事件

| 事件 | Rust | 说明 |
|---|---|---|
| `onChange` | `.on_change(\|value: &String\| ..)` | 文本改变了：完整的新文本。 |
| `onSelectionChange` | `.on_selection_change(\|selections: &Vec<EditorSelection>\| ..)` | 选区改变了：所有选区，主选区在最后。 |
| `onFocus`、`onBlur` | `.on_focus(\|\| ..)`、`.on_blur(\|\| ..)` | 编辑器获得或失去焦点。 |
| `onSearchChange` | `.on_search_change(\|result: &EditorSearchResult\| ..)` | 搜索的匹配项改变了：`{ count, current, error }`。 |
| `onLineNumberPress` | `.on_line_number_press(\|line: &u32\| ..)` | 行号被点击：行号，从 0 开始。 |
| `onContextMenu` | `.on_context_menu(\|event: &EditorContextMenuEvent\| ..)` | 在文本上按下了辅助按键：`{ position, x, y }`。 |

<a id="accessibility"></a>

## 无障碍

读屏软件用 `accessibilityLabel` 称呼编辑器；它的焦点就是一个文本输入框的焦点，所以平台的文本服务（输入法、听写、软键盘）都能在其中使用。

<a id="related"></a>

## 相关内容

- [TextView](./text-view.md)：Markdown 和 HTML 文档，其代码块与编辑器使用相同的语法颜色。
- [DiffView](./diff-view.md)：两段文本之间的改动，使用编辑器的语法颜色。
- [Input](./input.md)：单行和多行文本框，使用平台自己的编辑。
- [`nsh/keymap`](../modules/nsh-keymap.md)：应用为编辑器命令绑定的按键。
- [`nsh/base`](../modules/nsh-base.md)：其他基础组件。

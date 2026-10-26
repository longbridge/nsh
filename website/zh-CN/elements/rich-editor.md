---
title: RichEditor
description: nsh/base 的富文本编辑器：由应用的 schema 定义的文档、命令、插件、历史、键位、输入规则和 Markdown，按 ProseMirror 的架构构建，状态在 Rust 中。
order: 26.25
group: base
---

# RichEditor

`RichEditor` 是按 ProseMirror 的架构构建的富文本编辑器。应用用 schema 定义文档由什么组成（段落、标题、列表、提及，以及它需要的任何节点和标记类型），编辑器让每个文档都符合它。修改是归入事务的步骤，所以它们可以撤销、映射位置，并以 JSON 传递；行为来自插件（历史、键位、输入规则、Markdown 快捷方式、应用自己的插件）；命令是工具栏运行和询问的值。编辑器的状态、命令和插件在 Rust 中运行：输入、键位绑定和输入规则从不等待 JS，应用在需要时以 JSON 读取状态。

一个编辑器有两部分：会话（`createRichEditor`），它拥有状态，由应用的代码驱动；视图（`<RichEditor editor={…} />`），它原生地渲染会话，并把平台的输入变成事务。多个视图可以显示同一个会话。

```tsx
import { mount } from "nsh";
import { RichEditor, createRichEditor, history, inputRules, keymap, markdown, markdownRules, starterKeymap, baseKeymap, toggleMark } from "nsh/base";

const editor = createRichEditor({
  content: "# Notes\n\nStart typing…",
  format: "markdown",
  plugins: [
    history(),
    keymap({ "Mod-b": toggleMark("bold"), "Mod-i": toggleMark("italic") }),
    starterKeymap(),
    baseKeymap(),
    inputRules(markdownRules()),
    markdown(),
  ],
});

function App() {
  return <RichEditor editor={editor} class="flex-1" />;
}

mount(App);
```

省略 `schema` 时编辑器使用起步 schema（`starterSchema()`）；省略 `plugins` 时它得到起步套件（`starterKit()`：起步的输入规则和键位绑定、基础键位绑定和历史）。空的 `plugins` 数组创建一个完全没有插件的编辑器。

## 样式

编辑器以应用的外观绘制文档：`classes` 和 `styles` 设置它的各个部件（`root`、`content`、`caret`、`selection`、`placeholder`、`composition`、`selectedNode`、`listMarker`），按类型设置节点（`nodes: { heading: "text-2xl font-bold", "heading[level=2]": "text-xl" }`），以及设置标记（`marks: { bold: "font-bold" }`）。节点规格把 `extra.listMarker` 设为 `"bullet"` 或 `"ordered"` 的列表（starter schema 的列表）中，每一项都在 `listMarker` 部件中显示标记：按层级显示 `•`、`◦`、`▪`，或从列表的 `order` 开始编号，放在列表项的缩进中。

```tsx
<RichEditor
  editor={editor}
  classes={{
    nodes: { heading: "text-2xl font-bold", list_item: "pl-6", code_block: "font-mono bg-muted p-3 rounded-md" },
    marks: { bold: "font-bold", italic: "italic", code: "font-mono bg-muted" },
    listMarker: "text-muted-foreground",
  }}
/>
```

## Schema

`defineSchema(spec)` 读取 ProseMirror 形式的 schema，并在出错时抛出错误说明哪里不对。节点类型声明它们的 `content`（内容表达式：`"inline*"`、`"paragraph+"`、`"(heading | paragraph) block*"`）、`group`、是否为 `inline` 或 `atom`、允许的 `marks`，以及带默认值和校验（`"string"`、`"number"`、`"boolean"`）的 `attrs`。第一个块类型是默认类型：回车创建的、填充必需内容的类型。spec 中其他的键作为类型的附加数据保留，供构建在编辑器之上的框架使用。

```tsx
import { createRichEditor, defineSchema } from "nsh/base";

const schema = defineSchema({
  nodes: {
    doc: { content: "block+" },
    paragraph: { content: "inline*", group: "block" },
    mention: { group: "inline", inline: true, atom: true, attrs: { id: {}, label: { default: "" } } },
    text: { group: "inline" },
  },
  marks: { bold: {}, link: { attrs: { href: {} }, inclusive: false } },
});

const editor = createRichEditor({ schema, doc: schema.parse("Hello") });
```

`RichSchema` 读写文档：`parse(content, format?)`（默认纯文本，也可以是 `"markdown"` 或 JSON）、`parseMarkdown(markdown)`、`serialize(doc, format?)`（默认 Markdown，也可以是 `"text"`、`"json"`）以及 `check(doc)`，它返回填好属性默认值的文档。`nodes`、`marks` 和 `topNode` 列出它的类型。Markdown 对应起步 schema 的名称（`paragraph`、`heading`、`blockquote`、`code_block`、`bullet_list`、`ordered_list`、`list_item`、`bold`、`italic`、`code`、`link`、`strike`）。

## 命令

命令是一个值：`toggleMark("bold")` 创建一个对象，应用可以保存它、绑定到按键、传给编辑器。`editor.run(command)` 运行它并返回是否生效；`editor.can(command)` 只询问而不做任何修改，工具栏用它决定按钮是否可用。`editor.isActive(name, attrs?)` 判断某个标记或节点在选区处是否生效（`isActive("heading", { level: 2 })`）。

| 命令 | 作用 |
| --- | --- |
| `toggleMark(name, attrs?)` | 给选区加上标记，已有时去掉；在光标处作用于接下来输入的文字 |
| `setMark(name, attrs?)` / `unsetMark(name)` | 加上或去掉标记 |
| `setBlockType(name, attrs?)` | 把选中的文本块变成某个类型（`setBlockType("heading", { level: 2 })`） |
| `wrapIn(name, attrs?)` / `lift()` | 用一个节点包裹选中的块（引用）/ 把它们提出父节点 |
| `wrapInList(name, attrs?)` | 用列表包裹选中的块 |
| `splitListItem(name)` / `sinkListItem(name)` / `liftListItem(name)` | 拆分、缩进或减少缩进列表项 |
| `undo()` / `redo()` | 撤销或重做修改（需要 `history()`） |
| `selectAll()`、`setTextSelection(anchor, head?)`、`setNodeSelection(pos)` | 选择 |
| `insertText(text)`、`insertContent(content, format?)` | 在选区处插入文字或内容 |
| `setNodeAttribute(pos, attr, value)` | 设置某位置节点的属性，作为一次可撤销的修改 |
| `command(name)` | 按名称取无参数的命令：`"splitBlock"`、`"joinBackward"`、`"exitCode"`、`"newlineInCode"`、`"selectParentNode"`…… |
| `chain(commands)` | 依次运行命令，合成一个事务（一个撤销步骤）；所有命令都生效时才生效 |
| `first(commands)` | 运行第一个生效的命令 |

```tsx
editor.run(chain([setBlockType("heading", { level: 1 }), toggleMark("bold")]));
const canIndent = editor.can(sinkListItem("list_item"));
```

## 会话

| 成员 | 含义 |
| --- | --- |
| `state`、`doc`、`selection` | 以 JSON 表示的状态、文档和选区，读取时生成 |
| `revision` | 状态已应用的事务数 |
| `isEmpty`、`undoDepth`、`redoDepth`、`composing` | 界面显示的编辑器信息 |
| `setContent(content, format?)` | 替换文档，作为一次可撤销的修改 |
| `insertContent(content, format?)` | 在选区处插入内容 |
| `getJSON()`、`getMarkdown()`、`getText()`、`textBetween(from?, to?, separator?)` | 某种格式的文档 |
| `dispatch(tr)` | 应用以 JSON 给出的事务（其步骤基于当前文档） |
| `subscribe(listener)` / `unsubscribe(id)` | 收到来自任何来源的每次修改，带新的 revision |
| `focus()`、`blur()` | 把键盘焦点移到显示编辑器的视图，或从中移开 |

位置的计算与 ProseMirror 相同：文本按 UTF-16 码元计数，叶子节点计为一，有内容的节点计其两个边界。需要在编辑后仍然有效的东西，在节点属性或标记中保存一个 id。

### 接管事务

`editor.dispatchTransaction = (tr) => { … }` 让应用自己提交每个事务：处理函数以 JSON 收到每个事务，用 `editor.applyTransaction(tr)` 提交，或者丢弃它。协作、日志和需要通过应用检查的事务会用到它；设为 `null` 把提交交还给编辑器。处理函数在 JS 的下一轮收到输入产生的事务，文档在它提交时改变。

## 插件

| 工厂函数 | 添加的内容 |
| --- | --- |
| `history({ depth?, newGroupDelay? })` | 撤销历史；`newGroupDelay` 毫秒（500）内的修改一起撤销 |
| `keymap(bindings)` | 按键名绑定命令：`"Mod-b"`（Apple 平台上为 Cmd，其他平台为 Ctrl）、`"Shift-Enter"`、`"Alt-ArrowUp"` |
| `baseKeymap()` | 回车、退格、删除、全选和平台的文本编辑按键 |
| `starterKeymap()` | 起步 schema 的键位：标记、标题、列表、代码块、硬换行 |
| `inputRules(rules)` | 光标前输入的文字匹配某条规则时变成规则生成的内容；紧接着按退格撤销它 |
| `markdown(options?)` | 边输入边转换的 Markdown、Markdown 粘贴和 Markdown 复制（`blockRules`、`inlineRules`、`paste`、`copyAsMarkdown`） |
| `starterKit()` | 起步的输入规则和键位、基础键位和历史 |

插件在列表中的位置就是它的优先级：列表中靠前的键位和输入规则先处理按键或文字。输入规则来自 `markdownRules()`（标题、引用、列表、代码围栏、分隔线、常用标记）、`typographyRules()`（智能引号、省略号、破折号）、`textRule(match, replacement)`、`markRule(match, mark)`、`wrappingRule(match, node)` 和 `textblockRule(match, node)`。规则的 `match` 是以 `$` 结尾的正则表达式，与光标前的文字匹配。

### 应用的插件

`new RichEditorPlugin(key)` 创建一个行为由应用决定的插件，用链式调用设置：

```tsx
import { RichEditorPlugin, createRichEditor, starterKit } from "nsh/base";

const suggestions = new RichEditorPlugin("suggestions")
  .captureKeys(["ArrowUp", "ArrowDown", "Enter", "Escape"], (key) => moveSuggestion(key))
  .filterTransaction((tr) => tr.meta?.locked !== true)
  .appendTransaction((trs, state) => null);

const editor = createRichEditor({ plugins: [starterKit(), suggestions] });

// 建议列表打开时，这些按键归列表处理。
suggestions.capturing = true;
editor.setDecorations("suggestions", [{ from: 5, to: 9, kind: "inline", attrs: { class: "bg-accent" } }]);
```

- `captureKeys(keys, handler)`：`capturing` 为 true 时，编辑器消费这些按键，并随即告诉 `handler` 按下的是哪一个；键盘响应与没有插件时一样快。
- `editor.setDecorations(key, decorations)`：视图绘制在文档之上、不改变文档的行内属性、节点属性和小部件。它们随每次修改移动，直到再次设置。
- `filterTransaction(hook)` 和 `appendTransaction(hook)` 在应用自己的调用（`run`、`dispatch`、`setContent`……）中看到这些调用产生的事务：`filterTransaction` 可以丢弃一个事务，`appendTransaction` 可以返回一个与之一起应用的事务。`subscribe` 收到每次修改，包括输入。

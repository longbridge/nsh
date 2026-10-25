---
title: DiffView
description: "The DiffView component: the changes between two texts or of a patch, side by side or unified, with the words changed within lines highlighted, unchanged lines collapsed into separators that expand, syntax highlighting and per-change actions."
order: 26.3
group: base
demo: el-diff-view
---

# DiffView

`DiffView` shows the changes between two texts as code review tools do: side by side, the old text on the left and the
new on the right with their lines aligned, or unified, the deleted lines above the added ones. Within a changed line
the words that changed are highlighted, the unchanged lines between changes fold into separators that expand, and the
text is highlighted in the [`Editor`](./editor.md)'s syntax colors. It shows two texts (`oldValue` and `newValue`) or a
file of a unified diff (`patch`: `git diff`'s, a pull request's), and it is made for reviewing changes, an AI's
proposed edits among them: the app's actions on each change keep it or revert it. Only the rows in view are made, so a
long file scrolls as a short one does.

`DiffView` owns behavior and comes with a look of its own built from the theme's tokens: the theme's `font-mono` at
13pt in 20pt rows, line numbers in `muted-foreground`, added lines tinted green and deleted lines red, the changed
words in a stronger tint, a bar at the gutter's edge of each changed line, and separators on `muted`. Each part takes
the app's styles over these defaults (`classes` and `styles`). Rust makes one with `nsh_base::diff_view()`, TSX with
`<DiffView>` from `nsh/base`.

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

The view fills the box the app gives it (`flex-1`, a height) and scrolls inside: vertically through the rows, and each
side sideways to its longest line.

## Two texts or a patch

- **Two texts**: `oldValue` and `newValue`. The lines are compared as they are, so a final line break added or
  removed is a change, as git shows it.
- **A patch**: `patch`, a unified diff of one file (`git diff`, `git format-patch`, `diff -u`; renames, modes, new and
  deleted files, and a commit message before it, read as git writes them). The view shows its hunks' lines, numbered as
  in the file, each hunk under a separator of its header (the function it is in). The file's path names the language
  when `language` is not set.

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

`language` names the language as the `Editor`'s does: a name, an alias, an extension or a file name (`"rust"`,
`"tsx"`, `"main.go"`, `"Dockerfile"`).

## Layout and inline changes

`layout` is `"split"` (the default: side by side, each deleted line beside the added line most like it) or
`"unified"` (one column, a change's deleted lines above its added lines, with both line numbers). `inline` decides
what is highlighted within a changed line:

| `inline` | What is highlighted |
|---|---|
| `"wordAlt"` (default) | The words that changed, joined when one character apart, so a changed phrase reads as one |
| `"word"` | Each word that changed |
| `"char"` | Each character that changed |
| `"none"` | Nothing: the lines' tints alone |

Words are runs of letters, digits and `_`; each CJK character, kana and hangul syllable is a word of its own, so a
changed character in a sentence marks only itself. A line that changed almost entirely shows its tint alone.

`indicators` marks changed lines beside their numbers: `"bars"` (the default, a bar at the gutter's edge),
`"classic"` (`+` and `−` before the text) or `"none"`. `background={false}` leaves the changed lines untinted, and
`lineNumbers={false}` hides the numbers.

```tsx
<DiffView oldValue={before()} newValue={after()} language="md" layout="unified" inline="word" indicators="classic" class="flex-1" />
```

## Collapsed lines

Around each change `contextLines` unchanged lines show (3 by default); the unchanged lines between changes, and those
before the first and after the last, collapse into a separator that says how many lines it hides. Pressing it shows
them all. `expandAll` shows every line; the ref's `expand`, `expandAll()` and `collapseAll()` reveal and fold them
from the app.

`renderSeparator` draws the separators the app's way. It gets the separator (`DiffSeparator`): `gap`, the index of
the collapsed run (0 before the first change, `i` between changes `i - 1` and `i`), `lines`, how many lines it hides,
`first` and `last`, and for a patch the hunk's `header`. `expand(gap, direction, lines)` shows `"all"` its lines (the
default), or `lines` (default 20) `"up"`, just above the change below the separator, or `"down"`, just below the
change above it. The first separator expands up and the last one down.

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

## Keeping and reverting changes

`renderHunkActions` puts the app's actions at the top right of each change in view, told the change (`DiffHunk`:
`index`, `oldStart`, `oldLines`, `newStart`, `newLines`, lines from 0). They call the ref: `revertHunk(index)` puts
the change's old lines back into the new text, `acceptHunk(index)` makes its new lines the old text's. The view shows
the result at once and reports both texts with `onChange({ oldValue, newValue })`, which the app sets as the props.
Keeping or reverting an AI's edit one change at a time is this:

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

Here the old text is the file as it is: keeping a change applies it to the file (`onApply`), reverting it takes it
out of the proposal. A view showing a `patch` shows excerpts of the files, and its changes stay as they are.

## Methods

`ref={diff}` gives the view's methods (`DiffViewRef`). Methods that return a value (`hunks()`, `stats()`) return a
`Promise` of it in JS, so `await` them: `const changes = await diff.hunks()`. For counts shown on screen,
`onStatsChange` reports them as they change.

| Method | Description |
|---|---|
| `hunks()` | `Promise<DiffHunk[]>`: the changes, in order. |
| `stats()` | `Promise<DiffStats>`: `{ added, deleted, hunks }`, the lines added and deleted and the number of changes. |
| `goToHunk(index)` | Scrolls change `index` into view, its first row at the top. |
| `revertHunk(index)` | Puts change `index`'s old lines back into the new text and reports the texts (`onChange`). |
| `acceptHunk(index)` | Makes change `index`'s new lines the old text's and reports the texts (`onChange`). |
| `expand(gap, direction?, lines?)` | Shows the lines separator `gap` hides: `"all"` (default), or `lines` (default 20) `"up"` or `"down"`. |
| `expandAll()` | Shows every unchanged line. |
| `collapseAll()` | Collapses the unchanged lines again. |

Next and previous change buttons, with the counts from `onStatsChange`:

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

## Comments on lines

A press on a line number reports `onLineNumberPress({ side, line })`: the side (`"old"` or `"new"`) and the line from
0, the place for a review comment the app shows.

## Styling the parts

`class` and `style` style the view's box. `classes` takes Tailwind classes by part and `styles` inline styles by part;
a part's classes come after its own default styles, so they win, and its `styles` win over its classes. The font is
set with `fontFamily`, `fontSize` and `lineHeight`.

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

| Part | What it is | Default |
|---|---|---|
| `side` | Each side (split) or the one column (unified) | A `border` line between the sides |
| `gutter` | A side's line numbers and indicators | |
| `lineNumber` | Every line number | Right-aligned, `muted-foreground` |
| `lineNumberAdded` | The numbers of added lines, after `lineNumber` | |
| `lineNumberDeleted` | The numbers of deleted lines, after `lineNumber` | |
| `content` | A side's text, beside its gutter | 8pt before and after the text |
| `line` | Every row of a side | |
| `lineAdded` | Added lines' rows, after `line` | A green tint |
| `lineDeleted` | Deleted lines' rows, after `line` | A red tint |
| `filler` | The empty half of a split row beside a line the other side lacks | A faint `muted` fill |
| `wordAdded` | The words changed within an added line | A stronger green |
| `wordDeleted` | The words changed within a deleted line | A stronger red |
| `indicatorAdded` | An added line's bar (`"bars"`) or `+` (`"classic"`) | Green, 3pt wide |
| `indicatorDeleted` | A deleted line's bar or `−` | Red, 3pt wide |
| `separator` | A separator of collapsed lines, or a patch's hunk header | `⋯` and the count on `muted` |
| `hunkActions` | The box of the app's actions on a change | At the change's top right |

## Props

| Prop | Rust | Type | Default | Description |
|---|---|---|---|---|
| `oldValue` | `.old_value(..)` | `string` | | The old text. |
| `newValue` | `.new_value(..)` | `string` | | The new text. |
| `patch` | `.patch(..)` | `string` | | A unified diff of one file, shown when `oldValue` and `newValue` are not set. |
| `language` | `.language(..)` | `string` | plain text (a patch: its file's) | A language's name, alias, extension or file name. |
| `layout` | `.layout(..)` | `"split" \| "unified"` | `"split"` | Side by side or one column. |
| `inline` | `.inline(..)` | `"wordAlt" \| "word" \| "char" \| "none"` | `"wordAlt"` | What is highlighted within changed lines. |
| `indicators` | `.indicators(..)` | `"bars" \| "classic" \| "none"` | `"bars"` | How changed lines are marked beside their numbers. |
| `contextLines` | `.context_lines(..)` | `number` | `3` | Unchanged lines shown around each change. |
| `expandAll` | `.expand_all(..)` | `boolean` | `false` | Every unchanged line shows. |
| `lineNumbers` | `.line_numbers(..)` | `boolean` | `true` | The line numbers. |
| `background` | `.background(..)` | `boolean` | `true` | Changed lines are tinted. |
| `fontFamily` | `.font_family(..)` | `string` | the theme's `font-mono` | The font of the text. |
| `fontSize` | `.font_size(..)` | `number` | `13` | The font size, in points. |
| `lineHeight` | `.line_height(..)` | `number` | `20` | The height of a row, in points. |
| `renderSeparator` | `.render_separator(..)` | `(separator: DiffSeparator) => JSX.Element` | | Draws the separators of collapsed lines. |
| `renderHunkActions` | `.render_hunk_actions(..)` | `(hunk: DiffHunk) => JSX.Element` | | The app's actions on each change. |
| `classes` | `.classes(..)` | `DiffViewClasses` | `{}` | Tailwind classes by part. |
| `styles` | `.styles(..)` | `DiffViewStyles` | `{}` | Inline styles by part, over the classes. |
| `class`, `style` | style methods | `string`, `StyleProp<ViewStyle>` | | The view's box. |

## Events

| Event | Rust | Description |
|---|---|---|
| `onChange` | `.on_change(\|change: &DiffChange\| ..)` | A change was kept or reverted: `{ oldValue, newValue }`, the texts now. |
| `onLineNumberPress` | `.on_line_number_press(\|line: &DiffLine\| ..)` | A line number was pressed: `{ side, line }`. |
| `onStatsChange` | `.on_stats_change(\|stats: &DiffStats\| ..)` | The counts changed: `{ added, deleted, hunks }`. It fires once when the view is made and again each time the texts are diffed to other counts, for a "+8 −8 · 5 changes" line. |

## Related

- [Editor](./editor.md): the code editor whose syntax colors and languages `DiffView` uses.
- [TextView](./text-view.md): documents of Markdown and HTML, for the text around a review.
- [`nsh/base`](../modules/nsh-base.md): the other base components.

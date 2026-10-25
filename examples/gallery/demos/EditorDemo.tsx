import { Input, signal, Text, View } from "nsh";
import { Button, Editor, type EditorDiagnostic, type EditorRef, type EditorSearchResult, type EditorSelection } from "nsh/base";
import { Menu } from "nsh/desktop";
import { colors, finish, hint, Segmented, Toggle } from "../ui";

// `nsh/base`'s `Editor` draws its text, gutter, carets and matches itself
// and keeps its own model (a rope, undo, folds, highlighting); what is
// around it is the app's: the find bar is the app's `Input`s passing a
// `search` and calling the ref, the status line reads the editor's events,
// and the diagnostics are data the app sets (here fixed, as a linter's).

type Language = "rust" | "typescript" | "python" | "json";

/** A sample file of each language, with a linter's problems in it. */
const SAMPLES: Record<Language, { text: string; diagnostics: EditorDiagnostic[] }> = {
  rust: {
    text: `use std::collections::HashMap;

/// Counts the words of a text, most frequent first.
fn word_counts(text: &str) -> Vec<(&str, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    sorted
}

fn main() {
    let unused = 42;
    for (word, count) in word_counts("the quick fox jumps over the lazy dog the end") {
        println!("{word}: {count}");
    }
}
`,
    diagnostics: [
      {
        range: { start: { line: 14, column: 8 }, end: { line: 14, column: 14 } },
        severity: "warning",
        message: "unused variable: `unused`",
        source: "rustc",
        code: "unused_variables",
      },
    ],
  },
  typescript: {
    text: `interface Todo {
  id: number;
  title: string;
  done: boolean;
}

export function summarize(todos: Todo[]): string {
  const open = todos.filter((todo) => !todo.done);
  const titles = open.map((todo) => todo.title).join(", ");
  return \`\${open.length} open: \${titles}\`;
}

const todos: Todo[] = [
  { id: 1, title: "Write the editor", done: true },
  { id: 2, title: "Ship it", done: false },
];

console.log(summarize(todos, true));
`,
    diagnostics: [
      {
        range: { start: { line: 17, column: 29 }, end: { line: 17, column: 33 } },
        severity: "error",
        message: "Expected 1 arguments, but got 2.",
        source: "ts",
        code: "2554",
      },
    ],
  },
  python: {
    text: `from dataclasses import dataclass


@dataclass
class Point:
    x: float
    y: float

    def distance(self, other: "Point") -> float:
        return ((self.x - other.x) ** 2 + (self.y - other.y) ** 2) ** 0.5


def perimeter(points):
    total = 0
    for a, b in zip(points, points[1:] + points[:1]):
        total += a.distance(b)
    return total


print(perimeter([Point(0, 0), Point(3, 0), Point(3, 4)]))
`,
    diagnostics: [
      {
        range: { start: { line: 12, column: 14 }, end: { line: 12, column: 20 } },
        severity: "info",
        message: "Missing type annotation for `points`",
        source: "mypy",
      },
    ],
  },
  json: {
    text: `{
  "name": "gallery",
  "version": "0.1.0",
  "entry": "App.tsx",
  "tailwind": {
    "theme": "theme.json"
  },
  "capabilities": ["network", "storage"],
  "debug": tru
}
`,
    diagnostics: [
      {
        range: { start: { line: 8, column: 11 }, end: { line: 8, column: 14 } },
        severity: "error",
        message: "Value expected",
        source: "json",
      },
    ],
  },
};

const LANGUAGES = [
  { value: "rust", label: "Rust" },
  { value: "typescript", label: "TypeScript" },
  { value: "python", label: "Python" },
  { value: "json", label: "JSON" },
] as const;

/** A small button of the find bar: `on` lights it while what it toggles is
 * on. */
function Tool(props: { label: string; name: string; onPress: () => void; on?: boolean }) {
  return (
    <Button
      onPress={props.onPress}
      selected={props.on}
      accessibilityLabel={props.label}
      class="h-8 min-w-8 items-center justify-center rounded-md border border-border px-2 active:opacity-70 selected:bg-primary"
      style={{ backgroundColor: props.on ? undefined : colors.raised, ...(props.on ? {} : finish.raised) }}
    >
      <Text class={`font-mono text-[12px] font-semibold ${props.on ? "text-primary-foreground" : "text-foreground"}`}>{props.name}</Text>
    </Button>
  );
}

/** A labelled switch of the toolbar. */
function Option(props: { label: string; value: boolean; onChange: (value: boolean) => void }) {
  return (
    <View class="flex-row items-center gap-2">
      <Toggle value={props.value} onChange={props.onChange} label={props.label} />
      <Text class="text-[13px] text-foreground">{props.label}</Text>
    </View>
  );
}

const field = "h-8 min-w-[120px] flex-1 rounded-md border border-border bg-card px-2.5 text-[13px] text-foreground placeholder:text-muted-foreground focus:border-ring";

/**
 * Editor: `nsh/base`'s `Editor` with a sample file per language, soft
 * wrap and line numbers switched from the toolbar, a find bar of the
 * app's own inputs (case, whole word and regular expression toggles, next
 * and previous, replace and replace all through the ref), a status line of
 * the caret and the matches, a linter's diagnostics and a context menu of
 * editing commands.
 */
export function EditorDemo() {
  const language = signal<Language>("rust");
  const softWrap = signal(false);
  const lineNumbers = signal(true);
  const query = signal("");
  const replacement = signal("");
  const caseSensitive = signal(false);
  const wholeWord = signal(false);
  const regex = signal(false);
  const selections = signal<EditorSelection[]>([]);
  const result = signal<EditorSearchResult>({ count: 0 });
  const last = signal("—");
  // Each sample's text as last edited, so switching back finds it as it was.
  const texts: Partial<Record<Language, string>> = {};
  let editor: EditorRef | undefined;

  const caret = () => {
    const primary = selections().at(-1);
    if (!primary) return "Ln 1, Col 1";
    const more = selections().length > 1 ? ` · ${selections().length} carets` : "";
    return `Ln ${primary.head.line + 1}, Col ${primary.head.column + 1}${more}`;
  };
  const matches = () => {
    const r = result();
    if (r.error) return "Invalid pattern";
    if (!query()) return "";
    if (r.count === 0) return "No results";
    return r.current === undefined || r.current === null ? `${r.count} matches` : `${r.current + 1} of ${r.count}`;
  };

  const menu = (x: number, y: number) =>
    void Menu.popup(
      [
        { label: "Cut", onSelect: () => editor?.run("cut") },
        { label: "Copy", onSelect: () => editor?.run("copy") },
        { label: "Paste", onSelect: () => editor?.run("paste") },
        { label: "Select All", onSelect: () => editor?.run("selectAll") },
        { label: "Toggle Comment", onSelect: () => editor?.run("toggleComment") },
        { label: "Fold All", onSelect: () => editor?.run("foldAll") },
        { label: "Unfold All", onSelect: () => editor?.run("unfoldAll") },
      ],
      { x, y },
    );

  return (
    <View class="flex-1 gap-3 bg-background p-4">
      <Segmented options={LANGUAGES} value={language()} onChange={(value) => language.set(value)} size="small" />
      <View class="flex-row flex-wrap items-center gap-4">
        <Option label="Soft wrap" value={softWrap()} onChange={(value) => softWrap.set(value)} />
        <Option label="Line numbers" value={lineNumbers()} onChange={(value) => lineNumbers.set(value)} />
        <Text class="flex-1 text-right text-xs text-muted-foreground">{hint("Long-press to select", "Right-click for commands")}</Text>
      </View>

      <View class="gap-2">
        <View class="flex-row items-center gap-1.5">
          <Input
            value={query()}
            onChange={(value) => query.set(value)}
            onSubmit={() => editor?.run("findNext")}
            submitBehavior="submit"
            placeholder="Find"
            accessibilityLabel="Find"
            autoCapitalize="none"
            autoCorrect={false}
            class={field}
          />
          <Tool label="Match case" name="Aa" on={caseSensitive()} onPress={() => caseSensitive.set(!caseSensitive())} />
          <Tool label="Whole word" name="W" on={wholeWord()} onPress={() => wholeWord.set(!wholeWord())} />
          <Tool label="Regular expression" name=".*" on={regex()} onPress={() => regex.set(!regex())} />
          <Tool label="Previous match" name="↑" onPress={() => editor?.run("findPrevious")} />
          <Tool label="Next match" name="↓" onPress={() => editor?.run("findNext")} />
        </View>
        <View class="flex-row items-center gap-1.5">
          <Input
            value={replacement()}
            onChange={(value) => replacement.set(value)}
            onSubmit={() => editor?.replace(replacement())}
            submitBehavior="submit"
            placeholder="Replace"
            accessibilityLabel="Replace"
            autoCapitalize="none"
            autoCorrect={false}
            class={field}
          />
          <Tool label="Replace" name="Replace" onPress={() => editor?.replace(replacement())} />
          <Tool
            label="Replace all"
            name="All"
            onPress={async () => {
              const n = (await editor?.replaceAll(replacement())) ?? 0;
              last.set(`Replaced ${n}`);
            }}
          />
        </View>
      </View>

      <View class="min-h-80 flex-1 overflow-hidden rounded-lg border border-border bg-card">
        {() => {
          // A sample of its own per language: switching remounts the editor,
          // so each keeps its undo history out of the others'.
          const lang = language();
          const sample = SAMPLES[lang];
          return (
            <Editor
              ref={(r) => (editor = r)}
              defaultValue={texts[lang] ?? sample.text}
              language={lang}
              softWrap={softWrap()}
              lineNumbers={lineNumbers()}
              diagnostics={sample.diagnostics}
              search={{ query: query(), caseSensitive: caseSensitive(), wholeWord: wholeWord(), regex: regex() }}
              accessibilityLabel={`${lang} source`}
              onChange={(value) => (texts[lang] = value)}
              onSelectionChange={(value) => selections.set(value)}
              onSearchChange={(value) => result.set(value)}
              onLineNumberPress={(line) => last.set(`Line ${line + 1} pressed`)}
              onContextMenu={(e) => menu(e.x, e.y)}
              class="flex-1"
            />
          );
        }}
      </View>

      <View class="flex-row flex-wrap items-center gap-x-4 gap-y-1">
        <Text class="font-mono text-[11px] tabular-nums text-foreground">{caret()}</Text>
        <Text class="font-mono text-[11px] tabular-nums text-muted-foreground">{matches()}</Text>
        <Text class="font-mono text-[11px] text-muted-foreground">{`${SAMPLES[language()].diagnostics.length} problem`}</Text>
        <Text class="flex-1 text-right font-mono text-[11px] text-muted-foreground">{last()}</Text>
      </View>
    </View>
  );
}

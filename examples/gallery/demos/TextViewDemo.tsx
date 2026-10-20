import { cleanup, signal, Text, View } from "nsh";
import { Clipboard } from "nsh/clipboard";
import { SelectionArea, TextView } from "nsh/base";
import { Button, Caption, Card, colors, DemoPage, Press } from "../ui";
import { Glyph } from "./chat/glyphs";

const MARKDOWN = `# Release notes

Version **2.4** brings *offline sync*, ~~beta~~ stable widgets and \`inline code\`. Press <kbd>⌘K</kbd> to search, or read [the guide](https://nsh.dev/guide).

## What changed

1. Sync runs in the background
2. Conflicts are merged per field
   - text by paragraph
   - lists by item

- [x] Offline cache
- [x] Background refresh
- [ ] Shared folders

| Platform | Startup | Memory |
|:---------|--------:|-------:|
| iOS      |  180 ms |  42 MB |
| Android  |  210 ms |  55 MB |
| Desktop  |  140 ms |  61 MB |

\`\`\`ts
const doc = await sync.pull({ since: lastRun });
for (const change of doc.changes) apply(change);
\`\`\`

> Measure before you optimize.

> [!TIP]
> Long-press any paragraph to select and copy it.

> [!WARNING]
> The old sync API is deprecated.

The energy of a body at rest is $E = mc^2$.

$$
\\int_0^1 x^2\\,dx = \\tfrac{1}{3}
$$

<details>
<summary>How merging works</summary>

Each field keeps the newest write; lists merge **by item** so nothing is lost.

</details>

---

Benchmarks ran on a mid-range phone.[^1]

[^1]: A Pixel 7a and an iPhone 13, release builds.
`;

const HTML = `<h3>From HTML</h3>
<p>Simple markup reads as a reader shows it: <b>bold</b>, <i>italic</i>, <mark>marked</mark>, H<sub>2</sub>O, x<sup>2</sup> and <a href="https://example.com">links</a>.</p>
<ul><li>Lists</li><li>and <code>code</code></li></ul>
<blockquote>Quotes, too.</blockquote>`;

const EXTENDED = `Thanks @maya and @liam for the review!

\`\`\`chart
Mon 12
Tue 18
Wed 9
Thu 22
Fri 15
\`\`\`

<Callout kind="tip">

Extensions turn the app's own syntax into **custom nodes**.

</Callout>

:::note[Heads up]
Containers wrap Markdown, like *this*.
:::
`;

const extensions = [
  { name: "mention", pattern: "@(\\w+)" },
  { name: "chart", language: "chart" },
  { name: "callout", tag: "callout" },
  { name: "note", container: "note" },
];

const REPLY = `Here is a plan for **the migration**:

1. Export the data with \`nsh export\`
2. Check the *dry run* report
3. Switch the app over

| Step | Time |
|------|-----:|
| Export | 2 min |
| Check | 5 min |

\`\`\`sh
nsh export --out backup.json
\`\`\`

That's it. Ping me if anything looks off.`;

/** A code block's Copy action. */
function CopyCode(props: { code: string }) {
  const copied = signal(false);
  return (
    <Press
      onPress={() => {
        Clipboard.setString(props.code);
        copied.set(true);
        setTimeout(() => copied.set(false), 1600);
      }}
      accessibilityLabel="Copy code"
      hitSlop={4}
    >
      <View style={{ height: 16, flexDirection: "row", alignItems: "center", gap: 4 }}>
        <Glyph name={copied() ? "check" : "copy"} size={12} tint={copied() ? colors.up : colors.muted} />
        <Text style={{ fontSize: 11, color: copied() ? colors.up : colors.muted }}>{copied() ? "Copied" : "Copy"}</Text>
      </View>
    </Press>
  );
}

/** A formula without a math renderer of the app's: its TeX, set apart. */
function Tex(props: { tex: string; display: boolean }) {
  return (
    <If when={props.display} else={<Text class="font-mono text-[13px] text-violet-600 dark:text-violet-300">{props.tex}</Text>}>
      <View class="items-center rounded-lg bg-violet-500/10 px-3 py-2">
        <Text class="font-mono text-[13px] text-violet-600 dark:text-violet-300">{props.tex}</Text>
      </View>
    </If>
  );
}

function Markdown() {
  const pressed = signal("nothing yet");
  return (
    <Card icon="text" title="Markdown" subtitle="CommonMark and GitHub's: tables, tasks, alerts, footnotes, math, HTML">
      <TextView
        markdown={MARKDOWN}
        selectable
        renderCodeBlockActions={(code) => <CopyCode code={code.code} />}
        renderMath={(math) => <Tex tex={math.tex} display={math.display} />}
        onLinkPress={(url) => pressed.set(url)}
      />
      <Caption label="Link pressed" note={pressed()} />
    </Card>
  );
}

function Html() {
  return (
    <Card icon="text" title="HTML" subtitle="The same parts, from simple markup">
      <TextView html={HTML} selectable />
    </Card>
  );
}

/** A tiny bar chart of a ```chart block's lines ("Mon 12"). */
function Chart(props: { source: string }) {
  const bars = () =>
    props.source
      .split("\n")
      .map((line) => line.trim().split(/\s+/))
      .filter((p) => p.length === 2)
      .map(([label, value]) => ({ label, value: Number(value) || 0 }));
  const max = () => Math.max(1, ...bars().map((b) => b.value));
  return (
    <View class="h-28 flex-row items-end gap-2 rounded-xl border border-paper-border bg-paper-inset p-3 dark:border-graphite-border dark:bg-graphite-inset">
      <Each of={bars()}>
        {(bar) => (
          <View class="flex-1 items-center gap-1">
            <View class="w-full rounded-md bg-citrus-deep dark:bg-citrus" style={{ height: (bar().value / max()) * 64 }} />
            <Text class="text-[11px] text-zinc-500">{bar().label}</Text>
          </View>
        )}
      </Each>
    </View>
  );
}

function Extensions() {
  return (
    <Card icon="components" title="Extensions" subtitle="Mentions, a chart block, a <Callout> tag, a :::note container">
      <TextView
        markdown={EXTENDED}
        extensions={extensions}
        renderCustom={(node) => {
          switch (node.name) {
            case "mention":
              return (
                <View class="rounded-full bg-sky-500/15 px-2">
                  <Text class="text-[14px] font-medium text-sky-700 dark:text-sky-300">@{node.attributes?.["1"]}</Text>
                </View>
              );
            case "chart":
              return <Chart source={node.text ?? ""} />;
            case "callout":
              return (
                <View class="flex-row gap-3 rounded-xl bg-emerald-500/10 p-3">
                  <Text class="text-emerald-600 dark:text-emerald-400">✦</Text>
                  <TextView document={{ blocks: node.children ?? [] }} class="flex-1" />
                </View>
              );
            default:
              return (
                <View class="gap-1 rounded-xl border border-sky-500/40 p-3">
                  <Text class="text-xs font-semibold uppercase tracking-wide text-sky-600 dark:text-sky-400">{node.attributes?.label ?? node.name}</Text>
                  <TextView document={{ blocks: node.children ?? [] }} />
                </View>
              );
          }
        }}
      />
    </Card>
  );
}

function Caret() {
  return <View class="ml-0.5 h-4 w-2 animate-pulse rounded-sm bg-citrus-deep dark:bg-citrus" />;
}

function Streaming() {
  const text = signal("");
  const streaming = signal(false);
  let timer: number | undefined;
  const stop = () => {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
    streaming.set(false);
  };
  cleanup(stop);
  const play = () => {
    stop();
    const tokens = REPLY.match(/\s*\S{1,4}|\s+/g) ?? [];
    let i = 0;
    text.set("");
    streaming.set(true);
    timer = setInterval(() => {
      if (i >= tokens.length) return stop();
      text.set(text() + tokens[i++]);
    }, 40);
  };
  return (
    <Card icon="chat" title="Streaming" subtitle="A reply as an AI model writes it, with a caret">
      <TextView
        markdown={text()}
        options={{ streaming: streaming() }}
        caret={streaming() ? <Caret /> : undefined}
        selectable
        renderCodeBlockActions={(code) => <CopyCode code={code.code} />}
      />
      <Button label={streaming() ? "Streaming…" : "Stream it"} icon="play" variant="secondary" disabled={streaming()} onPress={play} />
    </Card>
  );
}

const QUESTION = "How long does the **migration** take?";

const ANSWER = `About seven minutes:

- Export: 2 min
- Check: 5 min`;

/** Two documents and a plain text that select as one, around a label that stays out. */
function Selection() {
  return (
    <Card icon="text" title="SelectionArea" subtitle="One selection across texts and TextViews">
      <SelectionArea class="gap-3">
        <TextView markdown={QUESTION} />
        <TextView markdown={ANSWER} />
        <View class="flex-row items-baseline gap-2">
          <Text selectable={false} style={{ fontSize: 13, color: colors.faint }}>Not selectable</Text>
          <Text style={{ fontSize: 15, color: colors.text }}>A plain Text joins the selection too.</Text>
        </View>
      </SelectionArea>
      <Caption label="Drag from the question into the answer, or long press and move the handles" />
    </Card>
  );
}

export function TextViewDemo() {
  return <DemoPage cards={[Streaming, Markdown, Selection, Extensions, Html]} />;
}

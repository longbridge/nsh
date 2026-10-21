import { cleanup, signal, Text, View } from "nsh";
import { SelectionArea, TextView, type TextViewImage } from "nsh/base";
import { Button, Caption, Card, colors, DemoPage } from "../ui";
import { ImageViewer } from "./ImageViewer";

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

![Green mountains under low clouds](https://picsum.photos/id/1018/1200/750)

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

const HTML = `<article>
  <header>
    <h1 id="lantern-3">Lantern 3.0: notes that sync offline</h1>
    <p><small>Posted by the Lantern team &middot; 14 min read &middot; <a href="https://example.com/blog/lantern-3" title="Permalink">permalink</a></small></p>
  </header>

  <section>
    <h2 id="highlights">Highlights</h2>
    <p>
      Lantern    3.0 is our <b>largest</b> release so far:   notes now sync
      <i>without a connection</i>, the editor gained <u>inline comments</u>, and the
      <s>experimental</s> <mark>stable</mark> plugin API ships today. Search is
      <mark color="yellow">twice as fast</mark>, and errors stand out in
      <mark style="background-color: #fecaca">red</mark>.
    </p>
    <p>
      Formulas in notes render too: H<sub>2</sub>O, e<sup>i&pi;</sup> + 1 = 0, and
      2<sup>10</sup> = 1&#8239;024. Press <kbd>Ctrl</kbd> + <kbd>K</kbd> to open the
      command bar, or run <code>lantern sync --dry-run</code> first. Our designer put it best:
      <q>a note app should feel like paper that remembers</q>. The <abbr title="Conflict-free replicated data type">CRDT</abbr>
      engine behind it is open source.
    </p>
    <figure>
      <a href="https://picsum.photos/id/1015/600/300"><img src="https://picsum.photos/id/1015/320/160" alt="A river valley under clouds" width="320" height="160"></a>
      <figcaption>The new welcome screen, photographed on a slow train. <em>Tap to open it full size.</em></figcaption>
    </figure>
    <figure>
      <img src="https://picsum.photos/id/1036/1200/750" alt="A tent in the snow below mountains" width="320" height="200">
      <figcaption>Base camp, where the team wrote the sync engine. <em>Press it to zoom in.</em></figcaption>
    </figure>
  </section>

  <section>
    <h2 id="upgrading">Upgrading</h2>
    <h3>Step by step</h3>
    <ol start="3">
      <li>Back up your vault (steps 1 and 2 were installing and signing in)</li>
      <li>Open <b>Settings &rarr; Sync</b> and choose a mode:
        <ul>
          <li><i>Live</i>: every change, right away</li>
          <li><i>Batched</i>: every few minutes
            <ol>
              <li>on Wi-Fi only</li>
              <li>or on any network</li>
            </ol>
          </li>
        </ul>
      </li>
      <li>Restart the app</li>
    </ol>

    <h3>Checklist</h3>
    <ul>
      <li><input type="checkbox" checked> Export a backup</li>
      <li><input type="checkbox" checked> Update every device</li>
      <li><input type="checkbox"> Turn on end-to-end encryption</li>
    </ul>

    <h4>Terms</h4>
    <dl>
      <dt>Vault</dt>
      <dd>A folder of notes that syncs as one.</dd>
      <dt>Replica</dt>
      <dd>A device's copy of a vault, merged with the others when they meet.</dd>
    </dl>
  </section>

  <section>
    <h2 id="plugins">Plugins</h2>
    <blockquote>
      <p>We rewrote our tag manager against the new API in an afternoon, and it got faster.</p>
      <p>&mdash; <cite>Mara Okafor, maintainer of Tagline</cite></p>
    </blockquote>
    <p>A plugin is a module that exports <code>activate</code>:</p>
    <pre><code class="language-ts">export function activate(app: App) {
  app.commands.add("word-count", () =&gt; {
    const words = app.editor.text().split(/\\s+/).length;
    app.toast(\`\${words} words\`);
  });
}</code></pre>
  </section>

  <section>
    <h2 id="performance">Performance</h2>
    <table>
      <thead>
        <tr><th align="left">Operation</th><th align="right">2.9</th><th align="right">3.0</th><th style="text-align: center">Change</th></tr>
      </thead>
      <tbody>
        <tr><td>Cold start</td><td align="right">820 ms</td><td align="right">410 ms</td><td align="center">&minus;50%</td></tr>
        <tr><td>Search 10k notes</td><td align="right">96 ms</td><td align="right">44 ms</td><td align="center">&minus;54%</td></tr>
        <tr><td>Full sync</td><td align="right">3.1 s</td><td align="right">1.2 s</td><td align="center">&minus;61%</td></tr>
      </tbody>
      <tfoot>
        <tr><td colspan="3">Median of 20 runs on a 2021 laptop</td><td align="center">&#10003;</td></tr>
      </tfoot>
    </table>
  </section>

  <details>
    <summary>Known issues</summary>
    <p>Very large attachments (over 200&nbsp;MB) still upload in one piece. Resumable uploads arrive in 3.1.</p>
  </details>
  <details open>
    <summary>Thanks</summary>
    <p>
      NSH is created by <a href="https://github.com/sunli829"><img src="https://avatars.githubusercontent.com/u/20092316?s=48" alt="sunli829" style="width: 20px; height: 20px"></a> <a href="https://github.com/sunli829">@sunli829</a>
      and <a href="https://github.com/huacnlee"><img src="https://avatars.githubusercontent.com/u/5518?s=48" alt="huacnlee" width="20" height="20"></a> <a href="https://github.com/huacnlee">@huacnlee</a>,
      with&nbsp;&hearts;.
    </p>
  </details>
</article>`;

const EXTENDED = `Thanks @huacnlee and @sunli829 for the review!

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
  const viewing = signal<TextViewImage | null>(null);
  return (
    <Card icon="text" title="Markdown" subtitle="CommonMark and GitHub's: tables, tasks, alerts, footnotes, math, HTML">
      <TextView
        markdown={MARKDOWN}
        selectable
        codeBlockActions={[{ id: "copy", label: "Copy code" }]}
        renderMath={(math) => <Tex tex={math.tex} display={math.display} />}
        onLinkPress={(url) => pressed.set(url)}
        onImagePress={(image) => viewing.set(image)}
      />
      <Caption label="Link pressed" note={pressed()} />
      <ImageViewer image={viewing()} onClose={() => viewing.set(null)} />
    </Card>
  );
}

function Html() {
  const viewing = signal<TextViewImage | null>(null);
  return (
    <Card icon="text" title="HTML" subtitle="An article page: sections, figures, lists, tables, code">
      <TextView html={HTML} selectable onImagePress={(image) => viewing.set(image)} />
      <ImageViewer image={viewing()} onClose={() => viewing.set(null)} />
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
                <View class="translate-y-[7px] rounded-full bg-sky-500/15 px-2">
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
        codeBlockActions={[{ id: "copy", label: "Copy code" }]}
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

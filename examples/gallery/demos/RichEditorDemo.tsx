import { Scroll, signal, Text, View } from "nsh";
import {
  Button,
  RichEditor,
  RichEditorPlugin,
  type RichCommand,
  type RichEditorSession,
  type RichNodeJSON,
  baseKeymap,
  chain,
  createRichEditor,
  defineSchema,
  history,
  inputRules,
  insertContent,
  keymap,
  lift,
  liftListItem,
  markdown,
  markdownRules,
  redo,
  setBlockType,
  setTextSelection,
  sinkListItem,
  starterKeymap,
  toggleMark,
  typographyRules,
  undo,
  wrapIn,
  wrapInList,
} from "nsh/base";
import { Card, colors, finish, mono, Segmented } from "../ui";
import { IconButton, LucideIcon as Icon, ToggleGroup, type ToggleItem, ToolbarSeparator } from "./RichEditorUi";

// `nsh/base`'s `RichEditor` renders a session the app makes with
// `createRichEditor`: the state, the commands and the plugins run in Rust,
// so typing, key bindings and Markdown shortcuts never wait for JS. What
// is around it is the app's: the toolbar runs commands and asks the
// session which apply and which are active, the Markdown view reads the
// document when the session reports a change, and the chat composer's
// mention list is a `RichEditorPlugin` that takes the arrow keys and
// Enter while the list is open.

const DOC = `# Release notes

Type **Markdown** as you go: \`#\` and a space makes a heading, \`-\` a list, \`>\` a quote.

- Lists split on Enter
- Tab indents an item, Shift-Tab outdents it

> Select text and use the toolbar, or Mod-B and Mod-I.
`;

// --- The document editor ------------------------------------------------------

type Block = "paragraph" | "h1" | "h2" | "h3" | "code";
type Mark = "bold" | "italic" | "strike" | "code";
type Wrap = "bullet" | "ordered" | "quote";

const BLOCKS: ToggleItem<Block>[] = [
  { value: "paragraph", icon: "pilcrow", label: "Text" },
  { value: "h1", icon: "heading-1", label: "Heading 1" },
  { value: "h2", icon: "heading-2", label: "Heading 2" },
  { value: "h3", icon: "heading-3", label: "Heading 3" },
  { value: "code", icon: "square-code", label: "Code block" },
];

const MARKS: ToggleItem<Mark>[] = [
  { value: "bold", icon: "bold", label: "Bold" },
  { value: "italic", icon: "italic", label: "Italic" },
  { value: "strike", icon: "strikethrough", label: "Strikethrough" },
  { value: "code", icon: "code", label: "Inline code" },
];

const WRAPS: ToggleItem<Wrap>[] = [
  { value: "bullet", icon: "list", label: "Bulleted list" },
  { value: "ordered", icon: "list-ordered", label: "Numbered list" },
  { value: "quote", icon: "text-quote", label: "Quote" },
];

/** What a block toggle runs: pressed sets that type, released goes back to text. */
function blockCommand(block: Block, pressed: boolean): RichCommand {
  if (!pressed || block === "paragraph") return setBlockType("paragraph");
  if (block === "code") return setBlockType("codeBlock");
  return setBlockType("heading", { level: Number(block[1]) });
}

const MARK_TYPES: Record<Mark, string> = { bold: "bold", italic: "italic", strike: "strike", code: "code" };

/** Takes the block out of `wrap`: a quote lifts, a list item leaves its list. */
function unwrap(wrap: Wrap): RichCommand {
  return wrap === "quote" ? lift() : liftListItem("listItem");
}

/**
 * What a list or quote toggle runs: released takes the block out; pressed
 * takes it out of the wraps it is in (`current`) first, so a bulleted list
 * becomes a numbered one and a quote a list.
 */
function wrapCommand(wrap: Wrap, pressed: boolean, current: Wrap[]): RichCommand {
  if (!pressed) return unwrap(wrap);
  const into = wrap === "quote" ? wrapIn("blockquote") : wrapInList(wrap === "bullet" ? "bulletList" : "orderedList");
  const out = current.filter((w) => w !== wrap).map(unwrap);
  return out.length ? chain([...out, into]) : into;
}

/** The toolbar: undo and redo, the block type, the marks, lists and quotes, indents. */
function Toolbar(props: { editor: RichEditorSession; revision: number }) {
  const editor = props.editor;
  // Read again on every change of the session.
  const on = (name: string, attrs?: Record<string, unknown>) => {
    void props.revision;
    return editor.isActive(name, attrs);
  };
  const can = (command: RichCommand) => {
    void props.revision;
    return editor.can(command);
  };
  const run = (command: RichCommand) => {
    editor.run(command);
    editor.focus();
  };
  const block = (): Block[] => {
    if (on("codeBlock")) return ["code"];
    for (const level of [1, 2, 3] as const) if (on("heading", { level })) return [`h${level}`];
    return on("paragraph") ? ["paragraph"] : [];
  };
  const marks = (): Mark[] => MARKS.map((m) => m.value).filter((m) => on(MARK_TYPES[m]));
  const wraps = (): Wrap[] => [
    ...(on("bulletList") ? (["bullet"] as const) : []),
    ...(on("orderedList") ? (["ordered"] as const) : []),
    ...(on("blockquote") ? (["quote"] as const) : []),
  ];
  const able = <T extends string>(items: ToggleItem<T>[], command: (value: T) => RichCommand) =>
    items.map((item) => ({ ...item, disabled: !can(command(item.value)) }));
  return (
    <View testID="toolbar" style={{ flexDirection: "row", flexWrap: "wrap", alignItems: "center", rowGap: 6, columnGap: 4 }}>
      <IconButton icon="undo-2" label="Undo" disabled={!can(undo())} onPress={() => run(undo())} />
      <IconButton icon="redo-2" label="Redo" disabled={!can(redo())} onPress={() => run(redo())} />
      <ToolbarSeparator />
      <ToggleGroup
        type="single"
        items={able(BLOCKS, (b) => blockCommand(b, true))}
        value={block()}
        onToggle={(value, pressed) => run(blockCommand(value, pressed))}
      />
      <ToolbarSeparator />
      <ToggleGroup
        type="multiple"
        items={able(MARKS, (m) => toggleMark(MARK_TYPES[m]))}
        value={marks()}
        onToggle={(value) => run(toggleMark(MARK_TYPES[value]))}
      />
      <ToolbarSeparator />
      <ToggleGroup
        type="multiple"
        items={WRAPS.map((item) => ({ ...item, disabled: !can(wrapCommand(item.value, !wraps().includes(item.value), wraps())) }))}
        value={wraps()}
        onToggle={(value, pressed) => run(wrapCommand(value, pressed, wraps()))}
      />
      <IconButton icon="list-indent-decrease" label="Outdent" disabled={!can(liftListItem("listItem"))} onPress={() => run(liftListItem("listItem"))} />
      <IconButton icon="list-indent-increase" label="Indent" disabled={!can(sinkListItem("listItem"))} onPress={() => run(sinkListItem("listItem"))} />
      <IconButton icon="minus" label="Divider" disabled={!can(insertContent({ type: "horizontalRule" }))} onPress={() => run(insertContent({ type: "horizontalRule" }))} />
    </View>
  );
}

/** The marks both editors share: the look is the app's, by mark name. */
const markClasses = {
  bold: "font-bold",
  italic: "italic",
  code: "font-mono text-sm bg-muted rounded px-1",
  link: "text-primary underline",
  strike: "line-through",
};

/** The document's look, by node type (and heading level). */
const documentClasses = {
  content: "gap-3",
  placeholder: "text-muted-foreground",
  listMarker: "text-muted-foreground",
  nodes: {
    paragraph: "leading-relaxed",
    "heading[level=1]": "text-3xl font-bold tracking-tight",
    "heading[level=2]": "text-2xl font-semibold tracking-tight",
    "heading[level=3]": "text-xl font-semibold",
    blockquote: "border-l-4 border-border pl-4 text-muted-foreground italic",
    codeBlock: "font-mono text-sm bg-muted rounded-lg px-4 py-3",
    bulletList: "gap-1.5 pl-6",
    orderedList: "gap-1.5 pl-6",
    horizontalRule: "h-px bg-border my-3",
  },
  marks: markClasses,
};

function DocumentEditor() {
  const editor = createRichEditor({
    content: DOC,
    format: "markdown",
    plugins: [
      history(),
      keymap({ "Mod-b": toggleMark("bold"), "Mod-i": toggleMark("italic"), "Mod-z": undo(), "Shift-Mod-z": redo() }),
      inputRules([...markdownRules(), ...typographyRules()]),
      markdown(),
      starterKeymap(),
      baseKeymap(),
    ],
  });
  const revision = signal(editor.revision);
  const source = signal(editor.getMarkdown());
  const text = signal(editor.getText());
  editor.subscribe((r) => {
    revision.set(r);
    source.set(editor.getMarkdown());
    text.set(editor.getText());
  });
  const mode = signal<"edit" | "markdown">("edit");
  const words = () => text().split(/\s+/).filter(Boolean).length;
  return (
    <Card icon="pilcrow" title="Document" subtitle="A toolbar of commands, Markdown shortcuts, undo and redo">
      <View style={{ borderRadius: 12, borderWidth: 1, borderColor: colors.border, backgroundColor: colors.surface, overflow: "hidden" }}>
        <View style={{ paddingHorizontal: 8, paddingVertical: 6, borderBottomWidth: 1, borderColor: colors.border, backgroundColor: colors.inset }}>
          <Toolbar editor={editor} revision={revision()} />
        </View>
        <If
          when={mode() === "edit"}
          else={
            <View style={{ minHeight: 280, paddingHorizontal: 20, paddingVertical: 16 }}>
              <Text testID="markdown-source" style={{ fontFamily: mono, fontSize: 13, lineHeight: 20, color: colors.text }}>
                {source()}
              </Text>
            </View>
          }
        >
          <View testID="markdown-editor" style={{ minHeight: 280, paddingHorizontal: 20, paddingVertical: 16 }}>
            <RichEditor editor={editor} placeholder="Write something…" style={{ flex: 1 }} classes={documentClasses} />
          </View>
        </If>
        <View
          style={{
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            paddingHorizontal: 12,
            paddingVertical: 8,
            borderTopWidth: 1,
            borderColor: colors.border,
          }}
        >
          <Text style={{ fontSize: 12, color: colors.muted, fontVariant: ["tabular-nums"] }}>
            {`${words()} words · ${text().length} characters`}
          </Text>
          <View style={{ width: 180 }}>
            <Segmented
              size="small"
              options={[
                { value: "edit", label: "Edit" },
                { value: "markdown", label: "Markdown" },
              ]}
              value={mode()}
              onChange={(value) => mode.set(value)}
            />
          </View>
        </View>
      </View>
    </Card>
  );
}

// --- The chat composer --------------------------------------------------------

type Person = { id: string; label: string };

const PEOPLE: Person[] = [
  { id: "ada", label: "Ada Lovelace" },
  { id: "alan", label: "Alan Turing" },
  { id: "grace", label: "Grace Hopper" },
  { id: "linus", label: "Linus Torvalds" },
];

/** A chat message's schema: paragraphs of text, line breaks, bold and italic, and mentions. */
const chatSchema = defineSchema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "inline*" },
    mention: { group: "inline", inline: true, atom: true, attrs: { id: {}, label: { default: "" } } },
    hardBreak: { group: "inline", inline: true, leafText: "\n", linebreakReplacement: true },
    text: { group: "inline" },
  },
  marks: { bold: {}, italic: {} },
});

/** The composer's look: mentions read as tokens. */
const chatClasses = {
  placeholder: "text-muted-foreground",
  nodes: { mention: "text-primary font-medium" },
  marks: markClasses,
};

type Message = { from: string; mine: boolean; doc: RichNodeJSON };

const paragraph = (...content: RichNodeJSON[]): RichNodeJSON => ({ type: "doc", content: [{ type: "paragraph", content }] });
const said = (text: string): RichNodeJSON => ({ type: "text", text });
const mention = (person: Person): RichNodeJSON => ({ type: "mention", attrs: person });

const THREAD: Message[] = [
  { from: "Grace Hopper", mine: false, doc: paragraph(said("The release notes are up. Can someone give them a read?")) },
  { from: "You", mine: true, doc: paragraph(said("On it — "), mention(PEOPLE[1]), said(" can you check the "), { type: "text", text: "migration", marks: [{ type: "bold" }] }, said(" part?")) },
];

/** A message's text as spans: marks become styles, mentions tokens. */
function MessageText(props: { doc: RichNodeJSON; mine: boolean }) {
  const ink = () => (props.mine ? colors.onAccent : colors.text);
  const spans = () =>
    (props.doc.content ?? []).flatMap((block, i) => [
      ...(i > 0 ? [said("\n")] : []),
      ...(block.content ?? []),
    ]);
  return (
    <Text style={{ fontSize: 15, lineHeight: 21, color: ink() }}>
      {() =>
        spans().map((node) => {
          if (node.type === "mention") {
            const label = String(node.attrs?.label ?? "");
            return <Text style={{ fontWeight: "600", color: props.mine ? colors.onAccent : colors.info }}>{`@${label}`}</Text>;
          }
          if (node.type === "hardBreak") return <Text>{"\n"}</Text>;
          const marks = (node.marks ?? []).map((m) => m.type);
          return (
            <Text style={{ fontWeight: marks.includes("bold") ? "700" : undefined, fontStyle: marks.includes("italic") ? "italic" : undefined }}>
              {node.text ?? ""}
            </Text>
          );
        })
      }
    </Text>
  );
}

function Bubble(props: { message: Message }) {
  const mine = () => props.message.mine;
  return (
    <View style={{ alignSelf: mine() ? "flex-end" : "flex-start", maxWidth: "80%", gap: 3 }}>
      <If when={!mine()}>
        <Text style={{ fontSize: 12, color: colors.muted, marginLeft: 12 }}>{props.message.from}</Text>
      </If>
      <View
        style={{
          paddingHorizontal: 14,
          paddingVertical: 9,
          borderRadius: 18,
          borderBottomRightRadius: mine() ? 6 : 18,
          borderBottomLeftRadius: mine() ? 18 : 6,
          backgroundColor: mine() ? colors.accentFill : colors.raised,
          ...(mine() ? finish.accent : finish.raised),
        }}
      >
        <MessageText doc={props.message.doc} mine={mine()} />
      </View>
    </View>
  );
}

function Initials(props: { label: string }) {
  const letters = () =>
    props.label
      .split(" ")
      .map((w) => w[0])
      .join("")
      .slice(0, 2);
  return (
    <View style={{ width: 26, height: 26, borderRadius: 13, alignItems: "center", justifyContent: "center", backgroundColor: colors.inset }}>
      <Text style={{ fontSize: 11, fontWeight: "600", color: colors.muted }}>{letters()}</Text>
    </View>
  );
}

function ChatComposer() {
  const query = signal<{ text: string; from: number; to: number } | null>(null);
  const highlighted = signal(0);
  const thread = signal<Message[]>(THREAD);
  const empty = signal(true);
  const matches = () => {
    const q = query();
    return q ? PEOPLE.filter((p) => p.label.toLowerCase().includes(q.text.toLowerCase())) : [];
  };

  // The mention list's keys while it is open, then Enter to send: the
  // first plugin that captures a key takes it.
  const mentions = new RichEditorPlugin("mentions");
  const sender = new RichEditorPlugin("send");
  const editor = createRichEditor({
    schema: chatSchema,
    plugins: [
      mentions,
      sender,
      history(),
      keymap({
        "Mod-b": toggleMark("bold"),
        "Mod-i": toggleMark("italic"),
        "Mod-z": undo(),
        "Shift-Mod-z": redo(),
        "Shift-Enter": insertContent({ type: "hardBreak" }),
      }),
      baseKeymap(),
    ],
  });

  const close = () => {
    query.set(null);
    mentions.capturing = false;
  };

  const pick = (person: Person) => {
    const q = query();
    if (!q) return;
    editor.run(chain([setTextSelection(q.from, q.to), insertContent(mention(person)), insertContent(" ")]));
    close();
    editor.focus();
  };

  const send = () => {
    if (editor.isEmpty) return;
    thread.set([...thread(), { from: "You", mine: true, doc: editor.getJson() }]);
    editor.setContent("");
    close();
    editor.focus();
  };

  mentions.captureKeys(["ArrowUp", "ArrowDown", "Enter", "Escape"], (key) => {
    const count = matches().length;
    if (key === "ArrowDown") highlighted.set((highlighted() + 1) % Math.max(count, 1));
    else if (key === "ArrowUp") highlighted.set((highlighted() + count - 1) % Math.max(count, 1));
    else if (key === "Enter" && count > 0) pick(matches()[highlighted()]);
    else close();
  });
  sender.captureKeys(["Enter"], () => send());
  sender.capturing = true;

  // After each change: is the caret right after `@name`? Then the list
  // opens and takes the keys it needs.
  editor.subscribe(() => {
    empty.set(editor.isEmpty);
    const selection = editor.selection;
    if (selection.type !== "text" || selection.anchor !== selection.head) return close();
    const head = selection.head ?? 0;
    const before = editor.textBetween(Math.max(0, head - 30), head, "\n");
    const found = /(?:^|\s)@(\w*)$/.exec(before);
    if (!found) return close();
    query.set({ text: found[1], from: head - found[1].length - 1, to: head });
    highlighted.set(0);
    mentions.capturing = matches().length > 0;
  });

  return (
    <Card icon="message-circle" title="Chat" subtitle="Type @ to mention someone · Enter sends, Shift-Enter breaks the line">
      <View style={{ gap: 10, paddingVertical: 4 }}>
        <Each of={thread()}>{(message) => <Bubble message={message()} />}</Each>
      </View>
      <View style={{ position: "relative" }}>
        <If when={matches().length > 0}>
          <View
            testID="mention-list"
            style={{
              position: "absolute",
              left: 8,
              bottom: "100%",
              marginBottom: 6,
              width: 240,
              padding: 4,
              borderRadius: 12,
              borderWidth: 1,
              borderColor: colors.border,
              backgroundColor: colors.raised,
              ...finish.menu,
            }}
          >
            <Each of={matches()}>
              {(person, i) => (
                <Button
                  accessibilityLabel={person().label}
                  selected={i() === highlighted()}
                  onPress={() => pick(person())}
                  style={{
                    flexDirection: "row",
                    alignItems: "center",
                    gap: 8,
                    paddingHorizontal: 8,
                    paddingVertical: 6,
                    borderRadius: 8,
                    backgroundColor: i() === highlighted() ? colors.inset : "transparent",
                  }}
                >
                  <Initials label={person().label} />
                  <Text style={{ fontSize: 14, color: colors.text }}>{person().label}</Text>
                </Button>
              )}
            </Each>
          </View>
        </If>
        <View
          style={{
            flexDirection: "row",
            alignItems: "flex-end",
            gap: 8,
            padding: 6,
            paddingLeft: 14,
            borderRadius: 22,
            borderWidth: 1,
            borderColor: colors.border,
            backgroundColor: colors.field,
          }}
        >
          {/* As tall as the send button: centered on the last line as it is. */}
          <View style={{ height: 32, justifyContent: "center" }}>
            <Icon name="at-sign" size={16} tint={colors.faint} />
          </View>
          <View testID="chat-editor" style={{ flex: 1, minHeight: 32, maxHeight: 140, justifyContent: "center", paddingVertical: 6 }}>
            <RichEditor editor={editor} placeholder="Message" style={{ flex: 1 }} classes={chatClasses} />
          </View>
          <Button
            accessibilityLabel="Send"
            disabled={empty()}
            onPress={send}
            style={{
              width: 34,
              height: 34,
              borderRadius: 17,
              alignItems: "center",
              justifyContent: "center",
              backgroundColor: empty() ? colors.inset : colors.light,
              opacity: empty() ? 0.6 : 1,
            }}
          >
            <Icon name="send-horizontal" size={16} tint={empty() ? colors.faint : colors.onLight} />
          </Button>
        </View>
      </View>
    </Card>
  );
}

export function RichEditorDemo() {
  return (
    <Scroll style={{ flex: 1 }}>
      <View style={{ padding: 16, gap: 16 }}>
        <DocumentEditor />
        <ChatComposer />
      </View>
    </Scroll>
  );
}

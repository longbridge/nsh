import { Scroll, signal, Text, View } from "nsh";
import {
  RichEditor,
  RichEditorPlugin,
  type RichCommand,
  type RichEditorSession,
  baseKeymap,
  chain,
  createRichEditor,
  defineSchema,
  history,
  inputRules,
  insertContent,
  keymap,
  liftListItem,
  markdown,
  markdownRules,
  redo,
  setBlockType,
  setTextSelection,
  starterKeymap,
  toggleMark,
  typographyRules,
  undo,
  wrapIn,
  wrapInList,
} from "nsh/base";
import { Button, Card, colors } from "../ui";

// `nsh/base`'s `RichEditor` renders a session the app makes with
// `createRichEditor`: the state, the commands and the plugins run in Rust,
// so typing, key bindings and Markdown shortcuts never wait for JS. What
// is around it is the app's: the toolbar runs commands and asks the
// session which apply and which are active, the Markdown preview reads
// the document when the session reports a change, and the chat input's
// mention list is a `RichEditorPlugin` that takes the arrow keys and
// Enter while the list is open.

const DOC = `# Release notes

Type **Markdown** as you go: \`#\` and a space makes a heading, \`-\` a list, \`>\` a quote.

- Lists split on Enter
- Tab indents an item
`;

/** One toolbar button: enabled when its command applies, filled when active. */
function Tool(props: { editor: RichEditorSession; revision: number; label: string; command: RichCommand; active?: string; attrs?: Record<string, unknown> }) {
  const state = () => {
    void props.revision;
    return {
      can: props.editor.can(props.command),
      on: props.active ? props.editor.isActive(props.active, props.attrs) : false,
    };
  };
  return (
    <Button
      label={props.label}
      size="small"
      variant={state().on ? "accent" : "secondary"}
      disabled={!state().can}
      onPress={() => {
        props.editor.run(props.command);
        props.editor.focus();
      }}
    />
  );
}

function MarkdownEditor() {
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
  editor.subscribe((r) => {
    revision.set(r);
    source.set(editor.getMarkdown());
  });
  return (
    <Card icon="text" title="Markdown editor" subtitle="Toolbar commands, Markdown shortcuts, undo and redo">
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
        <Tool editor={editor} revision={revision()} label="Bold" command={toggleMark("bold")} active="bold" />
        <Tool editor={editor} revision={revision()} label="Italic" command={toggleMark("italic")} active="italic" />
        <Tool editor={editor} revision={revision()} label="H2" command={setBlockType("heading", { level: 2 })} active="heading" attrs={{ level: 2 }} />
        <Tool editor={editor} revision={revision()} label="Text" command={setBlockType("paragraph")} active="paragraph" />
        <Tool editor={editor} revision={revision()} label="List" command={wrapInList("bullet_list")} active="bullet_list" />
        <Tool editor={editor} revision={revision()} label="Outdent" command={liftListItem("list_item")} />
        <Tool editor={editor} revision={revision()} label="Quote" command={wrapIn("blockquote")} active="blockquote" />
        <Tool editor={editor} revision={revision()} label="Undo" command={undo()} />
        <Tool editor={editor} revision={revision()} label="Redo" command={redo()} />
      </View>
      <View testID="markdown-editor" style={{ minHeight: 220, borderRadius: 10, borderWidth: 1, borderColor: colors.border, padding: 12, backgroundColor: colors.surface }}>
        <RichEditor editor={editor} style={{ flex: 1 }} classes={markdownClasses} />
      </View>
      <Text style={{ fontSize: 12, color: colors.muted }}>Markdown</Text>
      <Text style={{ fontFamily: "monospace", fontSize: 12, color: colors.text }}>{source()}</Text>
    </Card>
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

/** The Markdown editor's look, by node type (and heading level). */
const markdownClasses = {
  content: "gap-2",
  nodes: {
    "heading[level=1]": "text-2xl font-bold",
    "heading[level=2]": "text-xl font-semibold",
    "heading[level=3]": "text-lg font-semibold",
    blockquote: "border-l-2 border-border pl-3 text-muted-foreground",
    code_block: "font-mono text-sm bg-muted rounded-md p-3",
    bullet_list: "gap-1 pl-6",
    ordered_list: "gap-1 pl-6",
    horizontal_rule: "h-px bg-border my-2",
  },
  marks: markClasses,
};

/** The chat input's look: its mentions read as tokens. */
const chatClasses = {
  nodes: { mention: "text-primary font-medium" },
  marks: markClasses,
};

const PEOPLE = [
  { id: "ada", label: "Ada Lovelace" },
  { id: "alan", label: "Alan Turing" },
  { id: "grace", label: "Grace Hopper" },
  { id: "linus", label: "Linus Torvalds" },
];

/** A chat message's schema: paragraphs of text, bold and italic, and mentions. */
const chatSchema = defineSchema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "inline*" },
    mention: { group: "inline", inline: true, atom: true, attrs: { id: {}, label: { default: "" } } },
    text: { group: "inline" },
  },
  marks: { bold: {}, italic: {} },
});

function ChatInput() {
  const query = signal<{ text: string; from: number; to: number } | null>(null);
  const highlighted = signal(0);
  const sent = signal<string[]>([]);
  const matches = () => {
    const q = query();
    return q ? PEOPLE.filter((p) => p.label.toLowerCase().includes(q.text.toLowerCase())) : [];
  };

  const mentions = new RichEditorPlugin("mentions");
  const editor = createRichEditor({
    schema: chatSchema,
    plugins: [
      mentions,
      history(),
      keymap({ "Mod-b": toggleMark("bold"), "Mod-i": toggleMark("italic"), "Mod-z": undo(), "Shift-Mod-z": redo() }),
      baseKeymap(),
    ],
  });

  const pick = (person: { id: string; label: string }) => {
    const q = query();
    if (!q) return;
    editor.run(chain([setTextSelection(q.from, q.to), insertContent({ type: "mention", attrs: person }), insertContent(" ")]));
    query.set(null);
    mentions.capturing = false;
  };

  mentions.captureKeys(["ArrowUp", "ArrowDown", "Enter", "Escape"], (key) => {
    const count = matches().length;
    if (key === "ArrowDown") highlighted.set((highlighted() + 1) % Math.max(count, 1));
    else if (key === "ArrowUp") highlighted.set((highlighted() + count - 1) % Math.max(count, 1));
    else if (key === "Enter" && count > 0) pick(matches()[highlighted()]);
    else {
      query.set(null);
      mentions.capturing = false;
    }
  });

  // After each change: is the caret right after `@name`? Then the list
  // opens and takes the keys it needs.
  editor.subscribe(() => {
    const selection = editor.selection;
    if (selection.type !== "text" || selection.anchor !== selection.head) {
      query.set(null);
      mentions.capturing = false;
      return;
    }
    const head = selection.head ?? 0;
    const before = editor.textBetween(Math.max(0, head - 30), head, "\n");
    const found = /(?:^|\s)@(\w*)$/.exec(before);
    if (found) {
      query.set({ text: found[1], from: head - found[1].length - 1, to: head });
      highlighted.set(0);
      mentions.capturing = true;
    } else {
      query.set(null);
      mentions.capturing = false;
    }
  });

  const send = () => {
    const text = editor.getText().trim();
    if (!text) return;
    sent.set([...sent(), text]);
    editor.setContent("");
  };

  return (
    <Card icon="chat" title="Chat input" subtitle="Type @ to mention someone; arrows and Enter pick">
      <View style={{ gap: 6 }}>
        {() => sent().map((message) => (
          <View style={{ alignSelf: "flex-end", backgroundColor: colors.accent, borderRadius: 14, paddingHorizontal: 12, paddingVertical: 8 }}>
            <Text style={{ color: "#ffffff" }}>{message}</Text>
          </View>
        ))}
      </View>
      <If when={matches().length > 0}>
        <View style={{ borderRadius: 10, borderWidth: 1, borderColor: colors.border, backgroundColor: colors.raised, overflow: "hidden" }}>
          {() => matches().map((person, i) => (
            <View
              onPress={() => pick(person)}
              style={{ paddingHorizontal: 12, paddingVertical: 8, backgroundColor: i === highlighted() ? colors.inset : "transparent" }}
            >
              <Text style={{ color: colors.text }}>{person.label}</Text>
            </View>
          ))}
        </View>
      </If>
      <View style={{ flexDirection: "row", alignItems: "flex-end", gap: 8 }}>
        <View style={{ flex: 1, minHeight: 44, maxHeight: 140, borderRadius: 12, borderWidth: 1, borderColor: colors.border, paddingHorizontal: 12, paddingVertical: 10, backgroundColor: colors.surface }}>
          <RichEditor editor={editor} style={{ flex: 1 }} classes={chatClasses} />
        </View>
        <Button label="Send" onPress={send} />
      </View>
    </Card>
  );
}

export function RichEditorDemo() {
  return (
    <Scroll style={{ flex: 1 }}>
      <View style={{ padding: 16, gap: 16 }}>
        <ChatInput />
        <MarkdownEditor />
      </View>
    </Scroll>
  );
}

import { cleanup, type JSX, Overlay, platform, Scroll, type ScrollRef, signal, Text, View } from "nsh";
import {
  Button,
  RichEditor,
  RichEditorPlugin,
  StarterKit,
  TableKit,
  chain,
  command,
  createRichEditor,
  defineExtension,
  deleteRange,
  insertContent,
  insertContentAt,
  liftListItem,
  setNode,
  setNodeSelection,
  setTextSelection,
  suggestion,
  type RichCommand,
  type RichEditorSession,
} from "nsh/base";
import { LucideIcon, type LucideIconName } from "../icons";
import { alpha, colors, focusRing, hovered, PopIn, popupSurface, richEditorClasses } from "../ui";

// A block editor in the manner of Notion, built from `nsh/base`'s
// `RichEditor` and Tiptap's extensions. Everything around the document is
// the app's: type `/` for a menu of blocks (Tiptap's Suggestion, which the
// editor runs natively, so Enter and the arrows pick without waiting for JS),
// hover a block for its handle (`+` adds a block below, the grip drags it
// and, pressed without moving, opens the block's menu), and select text for
// a floating toolbar. All of them are commands on the session.

const DOC = `# Q4 launch plan

Type \`/\` anywhere for the blocks, or hover a block and use the handle in the margin.

## Goals

- Ship the new order ticket to every region
- Cut the time to first trade by a third
- Move the help center to the new editor

## This week

- [x] Review the pricing page copy
- [x] Freeze the design of the ticket
- [ ] Walk support through the new flow
- [ ] Publish the migration guide

> Good tools disappear: people should remember the work, not the editor.

## Rollout

1. Confirm the launch date
2. Brief the regional leads
3. Open the beta to five percent of traffic

## Numbers to watch

| Metric | Now | Goal |
| --- | --- | --- |
| Time to first trade | 9 min | 6 min |
| Orders per session | 1.8 | 2.4 |

Select some text for the toolbar: **bold**, *italic*, ~~strike~~ and \`code\` are a press away.
`;

// --- The blocks ------------------------------------------------------------

/** A block the `/` menu and a handle's menu turn text into. */
type Block = {
  id: string;
  /** The heading it stands under in the `/` menu. */
  group: "Style" | "Lists" | "Insert";
  title: string;
  hint: string;
  icon: LucideIconName;
  keywords: string;
  apply: RichCommand;
  /** A list: it changes the list around the caret, others leave it first. */
  list?: boolean;
  /** The node the caret's block is while it is this block (`[type, attrs]`), for the toolbar's pressed state. */
  node?: [string, Record<string, unknown>?];
};

const BLOCKS: Block[] = [
  { id: "text", group: "Style", node: ["paragraph"], title: "Text", hint: "Plain text, the start of everything", icon: "pilcrow", keywords: "paragraph plain", apply: command("setParagraph") },
  { id: "h1", group: "Style", node: ["heading", { level: 1 }], title: "Heading 1", hint: "A big section heading", icon: "heading-1", keywords: "title h1 header", apply: setNode("heading", { level: 1 }) },
  { id: "h2", group: "Style", node: ["heading", { level: 2 }], title: "Heading 2", hint: "A medium heading", icon: "heading-2", keywords: "subtitle h2 header", apply: setNode("heading", { level: 2 }) },
  { id: "h3", group: "Style", node: ["heading", { level: 3 }], title: "Heading 3", hint: "A small heading", icon: "heading-3", keywords: "h3 header", apply: setNode("heading", { level: 3 }) },
  { id: "quote", group: "Style", node: ["blockquote"], title: "Quote", hint: "Set a thought apart", icon: "text-quote", keywords: "blockquote citation", apply: command("toggleBlockquote") },
  { id: "bullet", group: "Lists", node: ["bulletList"], list: true, title: "Bulleted list", hint: "A simple list of points", icon: "list", keywords: "ul unordered bullets", apply: command("toggleBulletList") },
  { id: "numbered", group: "Lists", node: ["orderedList"], list: true, title: "Numbered list", hint: "A list that counts", icon: "list-ordered", keywords: "ol ordered steps", apply: command("toggleOrderedList") },
  { id: "todo", group: "Lists", node: ["taskList"], list: true, title: "To-do list", hint: "Track tasks with checkboxes", icon: "list-todo", keywords: "task checkbox", apply: command("toggleTaskList") },
  { id: "code", group: "Insert", node: ["codeBlock"], title: "Code", hint: "Code with its own font", icon: "square-code", keywords: "snippet pre", apply: command("toggleCodeBlock") },
  { id: "divider", group: "Insert", title: "Divider", hint: "A line between sections", icon: "minus", keywords: "rule hr separator", // The caret goes on in a new line below the rule, as in Notion.
    apply: chain([command("setHorizontalRule"), command("createParagraphNear")]),
  },
  { id: "table", group: "Insert", title: "Table", hint: "Rows and columns", icon: "table", keywords: "grid", apply: command("insertTable", { rows: 3, cols: 3, withHeaderRow: true }) },
];

const BLOCK_TITLES = new Map(BLOCKS.map((b) => [b.id, b]));
/** The blocks of the floating toolbar (the others are in the `/` menu). */
const BAR_BLOCKS = ["h1", "h2", "h3", "bullet", "todo"].map((id) => BLOCK_TITLES.get(id)!);

const MARKS: { icon: LucideIconName; label: string; name: string; toggle: string }[] = [
  { icon: "bold", label: "Bold", name: "bold", toggle: "toggleBold" },
  { icon: "italic", label: "Italic", name: "italic", toggle: "toggleItalic" },
  { icon: "underline", label: "Underline", name: "underline", toggle: "toggleUnderline" },
  { icon: "strikethrough", label: "Strikethrough", name: "strike", toggle: "toggleStrike" },
  { icon: "code", label: "Code", name: "code", toggle: "toggleCode" },
];

/** `/` opens the blocks: Tiptap's Suggestion with its own keys. It ranks above the keymap, so Enter picks before it splits a paragraph. */
const slash = defineExtension({
  name: "slashCommand",
  priority: 1000,
  addProseMirrorPlugins: () => [
    suggestion({ key: "slash", char: "/", captureKeys: ["ArrowUp", "ArrowDown", "Enter", "Tab"], decorationClass: "text-muted-foreground" }),
  ],
});

// --- Positions ---------------------------------------------------------------

type Resolved = { depth: number; start: number; end: number; parent: string };
type Rect = { x: number; y: number; width: number; height: number };

/** Blocks that are items of a list: each has a handle of its own, as in Notion. */
const ITEMS = new Set(["listItem", "taskItem"]);
/** Blocks without an inside. */
const LEAVES = new Set(["horizontalRule", "image"]);

/** The blocks around `pos`, the innermost first: their type and where each starts. */
function blocksAround(editor: RichEditorSession, pos: number): { type: string; start: number }[] {
  let at = editor.resolve(pos) as Resolved;
  // Between blocks (a divider has no inside): the block after it.
  if (at.depth === 0) {
    const node = editor.nodeAt(pos);
    return node ? [{ type: node.type ?? "", start: pos }] : [];
  }
  const around: { type: string; start: number }[] = [];
  while (at.depth > 0) {
    around.push({ type: at.parent, start: at.start - 1 });
    at = editor.resolve(at.start - 1) as Resolved;
  }
  return around;
}

/** The block a handle belongs to at `pos`: the item of a list when it is in one, else the top-level block; and the top-level block, whose margin the handle stands in. */
function handleTarget(editor: RichEditorSession, pos: number): { start: number; top: number } | null {
  const around = blocksAround(editor, pos);
  const top = around[around.length - 1];
  if (!top) return null;
  return { start: (around.find((b) => ITEMS.has(b.type)) ?? top).start, top: top.start };
}

/** Where the block starting at `start` ends. */
function blockEnd(editor: RichEditorSession, start: number): number {
  const node = editor.nodeAt(start);
  if (!node || LEAVES.has(node.type ?? "")) return start + 1;
  return (editor.resolve(start + 1) as Resolved).end + 1;
}

type DocNode = { type?: string; text?: string | null; content?: DocNode[] | null };

/** The block starting at `start` among its siblings: where the one before it starts and how long the one after it is (`null` at an end). */
function neighbors(editor: RichEditorSession, start: number): { before: number | null; after: number | null; size: number } | null {
  const node = editor.nodeAt(start) as DocNode | null;
  if (!node) return null;
  const at = editor.resolve(start) as Resolved & { index: number };
  // The siblings before it, from the parent's start.
  let pos = at.start;
  let before: number | null = null;
  for (let i = 0; i < at.index; i++) {
    const sibling = editor.nodeAt(pos) as DocNode | null;
    if (!sibling) return null;
    before = pos;
    pos += nodeSize(sibling);
  }
  const size = nodeSize(node);
  const next = start + size < at.end ? (editor.nodeAt(start + size) as DocNode | null) : null;
  return { before, after: next ? nodeSize(next) : null, size };
}

/** How many positions a node takes: its text's length, one for a leaf, else its content between its two ends. */
function nodeSize(node: DocNode): number {
  if (node.text != null) return node.text.length;
  if (LEAVES.has(node.type ?? "") || node.type === "hardBreak") return 1;
  return 2 + (node.content ?? []).reduce((sum, child) => sum + nodeSize(child), 0);
}

const LISTS = new Set(["bulletList", "orderedList", "taskList"]);

/**
 * Two lists of one kind that came to stand next to each other (a block turned into a list beside one, an item
 * dropped between lists) become one, as in Notion, where items in a row are one list: the numbers run on and
 * the items are spaced alike.
 */
function joinLists(editor: RichEditorSession) {
  for (let pass = 0; pass < 8; pass++) {
    const blocks = ((editor.doc as DocNode).content ?? []) as DocNode[];
    let pos = 0;
    let joined = false;
    for (let i = 0; i < blocks.length - 1; i++) {
      pos += nodeSize(blocks[i]);
      if (LISTS.has(blocks[i].type ?? "") && blocks[i].type === blocks[i + 1].type) {
        // The first list's end and the second's start go: what is left is one list.
        joined = editor.run(deleteRange({ from: pos - 1, to: pos + 1 }));
        break;
      }
    }
    if (!joined) return;
  }
}

/** The text of a block, for the preview that follows the pointer while it is dragged. */
function plainText(node: { text?: string | null; content?: unknown[] | null }): string {
  if (node.text) return node.text;
  return (node.content ?? []).map((child) => plainText(child as { text?: string; content?: unknown[] })).join(" ");
}

// --- The parts ---------------------------------------------------------------

/** What every control answers: a fill under the pointer, a deeper one while pressed, and a ring where the keyboard is. */
const feedback = () => ({
  hoverStyle: hovered(),
  activeStyle: { backgroundColor: alpha(colors.text, 0.1) },
  focusVisibleStyle: focusRing(1),
});


const pointer = platform.desktop;

/** A block's icon on a small tile, as Notion's menu has them. */
function Tile(props: { icon: LucideIconName }) {
  return (
    <View style={{ width: 36, height: 36, borderRadius: 8, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background, alignItems: "center", justifyContent: "center" }}>
      <LucideIcon name={props.icon} size={18} tint={colors.text} />
    </View>
  );
}

/** The `/` menu's list: how tall it shows, and the height of a row and of a group's heading, for keeping the highlighted block in view. */
const MENU_HEIGHT = 340;
const ROW = 48;
const HEADING = 28;

/** Whether item `i` starts a group: its heading stands over it. */
const startsGroup = (items: Block[], i: number) => i === 0 || items[i - 1].group !== items[i].group;

/** Where row `i` starts in the list, the headings over it counted. */
function rowTop(items: Block[], i: number): number {
  let top = 0;
  for (let k = 0; k <= i; k++) if (startsGroup(items, k)) top += HEADING;
  return top + i * ROW;
}

/** A line of text in a preview: a bar of the text's color. */
function Bar(props: { width: number; height?: number; strong?: boolean }) {
  return <View style={{ width: props.width, height: props.height ?? 6, borderRadius: 3, backgroundColor: alpha(colors.text, props.strong ? 0.55 : 0.18) }} />;
}

/** What a block looks like, small: the picture beside the menu's highlighted row, as Notion shows one. */
function Sample(props: { id: string }) {
  const row = (lead: JSX.Element, width: number) => (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
      {lead}
      <Bar width={width} />
    </View>
  );
  const dot = () => <View style={{ width: 5, height: 5, borderRadius: 3, backgroundColor: alpha(colors.text, 0.55) }} />;
  const number = (n: number) => <Text style={{ width: 10, fontSize: 10, fontWeight: "600", color: colors.muted }}>{`${n}.`}</Text>;
  const box = (on: boolean) => (
    <View style={{ width: 10, height: 10, borderRadius: 3, borderWidth: 1, borderColor: on ? colors.accent : alpha(colors.text, 0.35), backgroundColor: on ? colors.accent : undefined }} />
  );
  switch (props.id) {
    case "h1":
    case "h2":
    case "h3": {
      const size = { h1: 12, h2: 10, h3: 8 }[props.id];
      return (
        <View style={{ gap: 8 }}>
          <Bar width={110} height={size} strong />
          <Bar width={150} />
          <Bar width={120} />
        </View>
      );
    }
    case "bullet":
      return <View style={{ gap: 8 }}>{row(dot(), 120)}{row(dot(), 96)}{row(dot(), 132)}</View>;
    case "numbered":
      return <View style={{ gap: 8 }}>{row(number(1), 120)}{row(number(2), 96)}{row(number(3), 132)}</View>;
    case "todo":
      return <View style={{ gap: 8 }}>{row(box(true), 120)}{row(box(true), 96)}{row(box(false), 132)}</View>;
    case "quote":
      return (
        <View style={{ flexDirection: "row", gap: 10 }}>
          <View style={{ width: 3, borderRadius: 2, backgroundColor: alpha(colors.text, 0.35) }} />
          <View style={{ gap: 8, paddingVertical: 2 }}>
            <Bar width={140} />
            <Bar width={104} />
          </View>
        </View>
      );
    case "code":
      return (
        <View style={{ gap: 7, padding: 10, borderRadius: 6, backgroundColor: alpha(colors.text, 0.06) }}>
          <Bar width={96} strong />
          <View style={{ paddingLeft: 12, gap: 7 }}>
            <Bar width={120} />
            <Bar width={72} />
          </View>
          <Bar width={20} strong />
        </View>
      );
    case "divider":
      return (
        <View style={{ gap: 10 }}>
          <Bar width={140} />
          <View style={{ height: 1, backgroundColor: alpha(colors.text, 0.35) }} />
          <Bar width={112} />
        </View>
      );
    case "table":
      return (
        <View style={{ borderWidth: 1, borderColor: alpha(colors.text, 0.25), borderRadius: 4, overflow: "hidden" }}>
          <Each of={[0, 1, 2]}>
            {(r) => (
              <View style={{ flexDirection: "row", borderTopWidth: r() ? 1 : 0, borderColor: alpha(colors.text, 0.25), backgroundColor: r() ? undefined : alpha(colors.text, 0.06) }}>
                <Each of={[0, 1, 2]}>
                  {(c) => (
                    <View style={{ flex: 1, padding: 6, borderLeftWidth: c() ? 1 : 0, borderColor: alpha(colors.text, 0.25) }}>
                      <Bar width={28} strong={!r()} />
                    </View>
                  )}
                </Each>
              </View>
            )}
          </Each>
        </View>
      );
    default:
      return (
        <View style={{ gap: 8 }}>
          <Bar width={150} />
          <Bar width={132} />
          <Bar width={90} />
        </View>
      );
  }
}

/** The `/` menu: the blocks matching what was typed under their groups' headings, the highlighted one picked by Enter and pictured beside the list. */
function SlashMenu(props: { items: Block[]; index: number; onPick: (block: Block) => void; onHover: (index: number, at: { x: number; y: number }) => void; scroll: (ref: ScrollRef) => void; onScroll: (offset: number) => void }) {
  const lit = () => props.items[props.index];
  return (
    <PopIn side="bottom" style={{ flexDirection: "row", alignItems: "flex-start", gap: 8 }}>
      <View testID="slash-menu" role="listbox" accessibilityLabel="Blocks" style={{ ...popupSurface(), width: 300, padding: 6 }}>
        <If
          when={props.items.length > 0}
          else={<Text style={{ padding: 10, fontSize: 13, color: colors.muted }}>No blocks match</Text>}
        >
          <Scroll ref={props.scroll} onScroll={(e) => props.onScroll(e.offset.y)} style={{ maxHeight: MENU_HEIGHT }}>
            <Each of={props.items}>
              {(block, i) => (
                // The pointer moves the highlight, as the arrows do: one row is lit, not a hovered one and a chosen one.
                <View onMouseMove={(e) => i() !== props.index && props.onHover(i(), e.pagePosition)}>
                  <If when={startsGroup(props.items, i())}>
                    <Text style={{ height: HEADING, paddingHorizontal: 8, paddingTop: 8, fontSize: 11, fontWeight: "600", letterSpacing: 0.4, color: colors.muted }}>{block().group.toUpperCase()}</Text>
                  </If>
                  <Button
                    role="option"
                    selected={i() === props.index}
                    accessibilityLabel={block().title}
                    onPress={() => props.onPick(block())}
                    {...feedback()}
                    style={{
                      height: ROW,
                      flexDirection: "row",
                      alignItems: "center",
                      gap: 10,
                      paddingHorizontal: 6,
                      borderRadius: 8,
                      backgroundColor: i() === props.index ? alpha(colors.text, 0.07) : undefined,
                    }}
                  >
                    <Tile icon={block().icon} />
                    <View style={{ flex: 1, gap: 1 }}>
                      <Text style={{ fontSize: 14, fontWeight: "500", color: colors.text }}>{block().title}</Text>
                      <Text style={{ fontSize: 12, color: colors.muted }}>{block().hint}</Text>
                    </View>
                  </Button>
                </View>
              )}
            </Each>
          </Scroll>
        </If>
      </View>
      <If when={pointer && lit()}>
        <View testID="slash-preview" style={{ ...popupSurface(), width: 200, padding: 14, gap: 12 }}>
          <View style={{ height: 84, justifyContent: "center" }}>
            {/* Made again for each block: a sample is a picture of one. */}
            <Each of={[lit().id]}>{(id) => <Sample id={id()} />}</Each>
          </View>
          <Text style={{ fontSize: 12, lineHeight: 17, color: colors.muted }}>{lit().hint}</Text>
        </View>
      </If>
    </PopIn>
  );
}

/** One of the floating toolbar's buttons. */
function BarButton(props: { icon: LucideIconName; label: string; active?: boolean; onPress: () => void }) {
  return (
    <Button
      accessibilityLabel={props.label}
      onPress={props.onPress}
      {...feedback()}
      style={{
        width: 30,
        height: 30,
        borderRadius: 6,
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: props.active ? alpha(colors.text, 0.1) : undefined,
      }}
    >
      <LucideIcon name={props.icon} size={16} tint={props.active ? colors.text : colors.muted} />
    </Button>
  );
}

function BarRule() {
  return <View style={{ width: 1, height: 16, marginHorizontal: 4, backgroundColor: colors.line }} />;
}

/** One row of a block's menu. */
function MenuRow(props: { icon: LucideIconName; label: string; tone?: string; onPress: () => void }) {
  return (
    <Button
      role="menuitem"
      accessibilityLabel={props.label}
      onPress={props.onPress}
      {...feedback()}
      style={{ flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 8, paddingVertical: 6, borderRadius: 6 }}
    >
      <LucideIcon name={props.icon} size={16} tint={props.tone ?? colors.muted} />
      <Text style={{ fontSize: 14, color: props.tone ?? colors.text }}>{props.label}</Text>
    </Button>
  );
}

// --- The editor ----------------------------------------------------------------

/** A block's handle: `x` is the editor's margin it stands in, `y` the block's first line; `left` and `width` the block's own box. */
type Handle = { start: number; x: number; y: number; left: number; width: number };

export function BlockEditorDemo() {
  // The block's keys, as Notion's: Mod-Shift-Up / Down move the caret's block, Mod-D duplicates it. An extension
  // above the keymap holds them, so no other binding takes them first.
  const keys = new RichEditorPlugin("blockKeys");
  const blockKeys = defineExtension({ name: "blockKeys", priority: 900, addProseMirrorPlugins: () => [keys] });
  const editor = createRichEditor({ extensions: [StarterKit, TableKit, slash, blockKeys], content: DOC, format: "markdown" });
  // What the parts read: the revision follows every change of the document and the selection.
  const revision = signal(editor.revision);

  // The `/` menu.
  const open = signal(false);
  const query = signal("");
  const index = signal(0);
  let seen = 0;
  // The menu's list, and where it is scrolled: the highlighted block stays in view as the arrows move it.
  let scroller: ScrollRef | undefined;
  // Where the pointer was at its last move over the editor.
  let lastPointer = { x: -1, y: -1 };
  let scrolled = 0;
  const reveal = (i: number) => {
    if (!scroller) return;
    const top = rowTop(items(), i);
    const bottom = top + ROW;
    let y = scrolled;
    if (i === 0) y = 0;
    else if (top < y) y = top - HEADING;
    else if (bottom > y + MENU_HEIGHT) y = bottom - MENU_HEIGHT + 6;
    if (y !== scrolled) scroller.scrollTo({ y, animated: false });
  };
  const items = () => {
    const q = query().trim().toLowerCase();
    return q ? BLOCKS.filter((b) => `${b.title} ${b.keywords}`.toLowerCase().includes(q)) : BLOCKS;
  };
  /** Makes the block around the caret a `block`: out of a list first, unless it is a list itself (Notion turns a to-do into a quote, not a quote in a to-do). */
  const convert = (block: Block) => {
    if (!block.list) {
      for (let i = 0; i < 4; i++) {
        const item = editor.isActive("taskItem") ? "taskItem" : editor.isActive("listItem") ? "listItem" : null;
        if (!item || !editor.run(liftListItem(item))) break;
      }
    }
    editor.run(block.apply);
  };
  const pick = (block: Block) => {
    const range = editor.suggestion("slash")?.range;
    if (!range) return;
    editor.run(deleteRange({ from: range.from, to: range.to }));
    convert(block);
    editor.focus();
  };

  // The handle of the block the pointer is over, and the menu it opened.
  const handle = signal<Handle | null>(null);
  const menu = signal<Handle | null>(null);
  const dragging = signal(false);
  // The block being carried: a preview of it follows the pointer.
  const ghost = signal<{ x: number; y: number; text: string } | null>(null);

  // The floating toolbar's selection: a range of text.
  const selected = signal(false);

  const sync = () => {
    revision.set(editor.revision);
    const state = editor.suggestion("slash");
    if (state?.active) {
      const q = state.query ?? "";
      if (!open()) {
        // Each opening starts at the first block, whatever the last one picked.
        open.set(true);
        index.set(0);
        scrolled = 0;
      }
      if (q !== query()) {
        query.set(q);
        index.set(0);
        reveal(0);
      }
      // The keys the plugin took: the list moves or picks.
      if (state.keyCount > seen) {
        seen = state.keyCount;
        const n = items().length;
        const move = (to: number) => {
          index.set(to);
          reveal(to);
        };
        if (state.lastKey === "ArrowDown" && n) move((index() + 1) % n);
        else if (state.lastKey === "ArrowUp" && n) move((index() + n - 1) % n);
        else if (n && (state.lastKey === "Enter" || state.lastKey === "Tab")) pick(items()[Math.min(index(), n - 1)]);
      }
    } else if (open()) {
      open.set(false);
      query.set("");
      index.set(0);
      scrolled = 0;
      seen = 0;
    }
    const sel = editor.selection;
    // A range of text, or the whole document (Select All): the toolbar shows and the handle goes.
    selected.set((sel.type === "all" || (sel.type === "text" && sel.anchor !== sel.head)) && !editor.isActive("codeBlock"));
  };
  editor.on("revision", sync);
  // A change of the document moves the blocks: the handle and its menu go until the pointer finds a block again.
  // Lists that came to stand next to each other join, however it happened (typed `1. `, a drop, a block turned
  // into a list). An undo is left alone, so the join itself can be undone.
  let depth = editor.undoDepth;
  let joining = false;
  editor.on("update", () => {
    const undone = editor.undoDepth < depth;
    depth = editor.undoDepth;
    if (!undone && !joining && !dragging()) {
      joining = true;
      joinLists(editor);
      joining = false;
      depth = editor.undoDepth;
    }
    if (dragging()) return;
    handle.set(null);
    menu.set(null);
  });

  // The handle is placed in page points, so it does not move with the document: once the block under it has
  // moved (a scroll by wheel, keys or the bar), the handle and its menu go until the pointer finds a block again.
  // Watched only while a handle shows.
  const watch = setInterval(() => {
    const h = handle();
    if (!h || dragging()) return;
    const rect = editor.pageNodeRect(h.start) as Rect | null;
    if (!rect || rect.y !== h.y) {
      handle.set(null);
      menu.set(null);
    }
  }, 120);
  cleanup(() => clearInterval(watch));

  /** The pointer over the editor: the handle goes to the block under it (a list's item, else the top-level block). */
  const onMove = (e: { pagePosition: { x: number; y: number } }) => {
    // The menu's rows hear a move before this does: they compare with where the pointer was.
    lastPointer = e.pagePosition;
    if (dragging() || menu()) return;
    const pos = editor.posAtPageCoords(e.pagePosition);
    if (pos === null) return;
    const target = handleTarget(editor, pos);
    if (!target) return;
    const rect = editor.pageNodeRect(target.start) as Rect | null;
    const top = (target.top === target.start ? rect : (editor.pageNodeRect(target.top) as Rect | null)) ?? rect;
    if (!rect || !top) return;
    const now = handle();
    if (!now || now.start !== target.start || now.y !== rect.y || now.x !== top.x) handle.set({ start: target.start, x: top.x, y: rect.y, left: rect.x, width: rect.width });
  };

  // The grip: a press that stays put opens the block's menu (`onPress`); a move past a few points takes the block
  // and the pointer carries it (the responder is claimed only then, so a tap keeps its press).
  const grip = (h: Handle) => ({
    onMoveShouldSetResponder: (_: unknown, g: { dx: number; dy: number }) => Math.hypot(g.dx, g.dy) > 4,
    onResponderTerminationRequest: () => false,
    onResponderGrant: (e: { pageX: number; pageY: number }) => {
      dragging.set(true);
      const node = editor.nodeAt(h.start);
      const text = node ? plainText(node) : "";
      ghost.set({ x: e.pageX, y: e.pageY, text: text.length > 80 ? `${text.slice(0, 80)}…` : text });
      editor.dragBlock(h.start, { x: e.pageX, y: e.pageY });
      return true;
    },
    onResponderMove: (e: { pageX: number; pageY: number }) => {
      const g = ghost();
      if (g) ghost.set({ ...g, x: e.pageX, y: e.pageY });
      editor.dragTo({ x: e.pageX, y: e.pageY });
    },
    // The drop changes the document, and with it the handle goes.
    onResponderRelease: () => {
      editor.endDrag();
      dragging.set(false);
      ghost.set(null);
    },
    onResponderTerminate: () => {
      editor.endDrag();
      dragging.set(false);
      ghost.set(null);
    },
  });

  /** Moves the block starting at `start` one place among its siblings; the caret goes with it. */
  const move = (start: number, by: -1 | 1) => {
    const around = neighbors(editor, start);
    const node = editor.nodeAt(start);
    if (!around || !node) return false;
    const to = by < 0 ? around.before : around.after === null ? null : start + around.after;
    if (to === null) return false;
    const head = editor.selection.head ?? -1;
    const inside = head >= start && head <= start + around.size ? head - start : 1;
    const moved = editor.run(chain([deleteRange({ from: start, to: start + around.size }), insertContentAt(to, node), setTextSelection(to + inside)]));
    editor.focus();
    return moved;
  };
  /** The block the caret is in (a list's item, else the top-level block). */
  const caretBlock = () => {
    const head = editor.selection.head;
    return head == null ? null : (handleTarget(editor, head)?.start ?? null);
  };
  keys.captureKeys(["Mod-Shift-ArrowUp", "Mod-Shift-ArrowDown", "Mod-d"], (key) => {
    const start = caretBlock();
    if (start === null) return;
    if (key === "Mod-d") {
      const node = editor.nodeAt(start);
      if (node) editor.run(insertContentAt(blockEnd(editor, start), node));
    } else move(start, key === "Mod-Shift-ArrowUp" ? -1 : 1);
  });
  keys.capturing = true;

  /** `+`: a new block below (an item of the same list below an item), with the `/` menu open in it. */
  const addBelow = (h: Handle) => {
    const end = blockEnd(editor, h.start);
    const node = editor.nodeAt(h.start);
    const item = node && ITEMS.has(node.type ?? "");
    const added = item ? { type: node.type, attrs: node.type === "taskItem" ? { checked: false } : undefined, content: [{ type: "paragraph" }] } : { type: "paragraph" };
    // Past the item's and its paragraph's opening, or the paragraph's.
    editor.run(chain([insertContentAt(end, added), setTextSelection(end + (item ? 2 : 1)), insertContent("/")]));
    editor.focus();
  };
  const turnInto = (h: Handle, block: Block) => {
    menu.set(null);
    // The caret goes into the block's first line (a point just inside it), then the block turns.
    const inside = editor.posAtPageCoords({ x: h.left + Math.min(32, h.width / 2), y: h.y + 8 });
    if (inside !== null) editor.run(setTextSelection(inside));
    convert(block);
    editor.focus();
  };
  const duplicate = (h: Handle) => {
    menu.set(null);
    const node = editor.nodeAt(h.start);
    if (node) editor.run(insertContentAt(blockEnd(editor, h.start), node));
    editor.focus();
  };
  const remove = (h: Handle) => {
    menu.set(null);
    editor.run(chain([setNodeSelection(h.start), command("deleteSelection")]));
    editor.focus();
  };

  /** What floats by the caret or the selection: the `/` menu, else the toolbar. */
  const bubble = () => {
    if (open()) return <SlashMenu items={items()} index={Math.min(index(), Math.max(items().length - 1, 0))} onPick={pick} onHover={(i, at) => {
                // A pointer at rest under the menu as it opens is not the user's hand: only a move lights a row.
                if (at.x !== lastPointer.x || at.y !== lastPointer.y) index.set(i);
              }} scroll={(r) => (scroller = r)} onScroll={(y) => (scrolled = y)} />;
    if (!selected()) return undefined;
    revision();
    return (
      <PopIn side="top">
      <View testID="format-bar" role="toolbar" accessibilityLabel="Format" style={{ ...popupSurface(), flexDirection: "row", alignItems: "center", padding: 3, gap: 1 }}>
        <Each of={BAR_BLOCKS}>
          {(block) => (
            <BarButton
              icon={block().icon}
              label={block().title}
              active={!!block().node && editor.isActive(block().node![0], block().node![1])}
              onPress={() => {
                convert(block());
                editor.focus();
              }}
            />
          )}
        </Each>
        <BarRule />
        <Each of={MARKS}>
          {(mark) => <BarButton icon={mark().icon} label={mark().label} active={editor.isActive(mark().name)} onPress={() => editor.run(command(mark().toggle))} />}
        </Each>
      </View>
      </PopIn>
    );
  };

  return (
    <View style={{ flex: 1, padding: 16 }}>
      <View
        testID="block-editor"
        onMouseMove={pointer ? onMove : undefined}
        style={{ flex: 1, minHeight: 360, borderRadius: 12, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface, overflow: "hidden" }}
      >
        <RichEditor
          testID="block-document"
          editor={editor}
          accessibilityLabel="Document"
          placeholders={{ paragraph: "Type '/' for blocks", heading: "Heading" }}
          style={{ flex: 1 }}
          classes={{
            ...richEditorClasses,
            content: pointer ? "gap-3 pl-16 pr-10 py-8" : "gap-3 px-5 py-6",
            // A block the handle took, and the line it will drop on.
            // A rule sits as close to its neighbors as they do to each other (the document's gap is around it too).
            nodes: { ...richEditorClasses.nodes, horizontalRule: "h-px bg-border my-1" },
            selectedNode: "bg-primary/10 rounded-md",
            dropCursor: "bg-primary rounded-full",
          }}
          bubbleMenu={bubble()}
          bubbleMenuPlacement={open() ? "bottom" : "top"}
          bubbleMenuOffset={6}
        />
        <If when={pointer && handle() && !open() && !selected()}>
          {(() => {
            const h = () => handle()!;
            return (
              <Overlay anchor={{ x: h().x, y: h().y }} placement="left" align="start" offset={6}>
                <PopIn side="right">
                <View testID="block-handle" style={{ flexDirection: "row", alignItems: "center", gap: 2, height: 28 }}>
                  <Button
                    accessibilityLabel="Add a block below"
                    onPress={() => addBelow(h())}
                    {...feedback()}
                    style={{ width: 24, height: 24, borderRadius: 6, alignItems: "center", justifyContent: "center" }}
                  >
                    <LucideIcon name="plus" size={16} tint={colors.faint} />
                  </Button>
                  <View
                    role="button"
                    accessibilityLabel="Drag to move, press for the menu"
                    onPress={() => menu.set(h())}
                    {...feedback()}
                    style={{ width: 24, height: 24, borderRadius: 6, alignItems: "center", justifyContent: "center", cursor: "grab" }}
                    {...grip(h())}
                  >
                    <LucideIcon name="grip-vertical" size={16} tint={colors.faint} />
                  </View>
                </View>
                </PopIn>
              </Overlay>
            );
          })()}
        </If>
        <If when={ghost()}>
          {(g) => (
            <Overlay anchor={{ x: g().x, y: g().y }} placement="right" align="start" offset={14} style={{ pointerEvents: "none" }}>
              <View testID="block-ghost" style={{ ...popupSurface(), maxWidth: 360, paddingHorizontal: 12, paddingVertical: 8, opacity: 0.92 }}>
                <Text style={{ fontSize: 14, color: colors.text }}>{g().text || "Block"}</Text>
              </View>
            </Overlay>
          )}
        </If>
        <If when={menu()}>
          {(m) => (
            <Overlay
              anchor={{ x: m().x, y: m().y + 28 }}
              placement="bottom"
              align="start"
              offset={4}
              margin={8}
              onPressOutside={() => menu.set(null)}
              onCancel={() => menu.set(null)}
            >
              <PopIn side="bottom">
              <View testID="block-menu" role="menu" accessibilityLabel="Block" style={{ ...popupSurface(), width: 220, padding: 4 }}>
                <Text style={{ paddingHorizontal: 8, paddingTop: 4, paddingBottom: 4, fontSize: 11, fontWeight: "600", color: colors.muted }}>TURN INTO</Text>
                <Each of={BLOCKS.slice(0, 8)}>{(block) => <MenuRow icon={block().icon} label={block().title} onPress={() => turnInto(m(), block())} />}</Each>
                <View style={{ height: 1, marginVertical: 4, backgroundColor: colors.line }} />
                <MenuRow icon="arrow-up" label="Move up" onPress={() => (menu.set(null), move(m().start, -1))} />
                <MenuRow icon="arrow-down" label="Move down" onPress={() => (menu.set(null), move(m().start, 1))} />
                <MenuRow icon="copy" label="Duplicate" onPress={() => duplicate(m())} />
                <MenuRow icon="trash" label="Delete" tone={colors.down} onPress={() => remove(m())} />
              </View>
              </PopIn>
            </Overlay>
          )}
        </If>
      </View>
    </View>
  );
}

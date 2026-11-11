// The block editor demo: a `/` menu of blocks (Tiptap's Suggestion, run
// natively), a handle on the block under the pointer, and a toolbar over a
// selection, each a command on the session the editor shows.
import { expect, open, test } from "nsh/test";

const ITEM = "Publish the migration guide";
const REVIEW = "Review the pricing page copy";

/** The demo on a phone, or on a desktop (`desktop`), whose pointer shows the handles. */
async function openDemo(desktop = false) {
  const app = await open("../App.tsx", desktop ? { platform: "macos", width: 1200, height: 900 } : undefined);
  await app.press("role:button[Block editor]");
  await app.waitFor(ITEM);
  return app;
}

/** A new empty line below the last task, with the caret in it. */
async function newLine(app: Awaited<ReturnType<typeof openDemo>>) {
  await app.press(ITEM);
  await app.keyboard.press("end");
  await app.keyboard.press("enter");
}

test("/ opens the blocks and typing narrows them", async () => {
  const app = await openDemo();
  await newLine(app);
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  expect(app.shows("text:Heading 1")).toBe(true);
  expect(app.count("role:option")).toBe(11);
  await app.type("label:Document", "list");
  // Three lists match: bulleted, numbered, to-do.
  await app.waitFor("role:option[Divider]", { state: "gone" });
  expect(app.count("role:option")).toBe(3);
  expect(app.shows("text:Numbered list")).toBe(true);
  expect(app.shows("text:To-do list")).toBe(true);
});

test("Enter picks the highlighted block and the arrows move it", async () => {
  const app = await openDemo();
  await newLine(app);
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  // Text, Heading 1, Heading 2: the second down is Heading 2.
  await app.keyboard.press("down");
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitFor("role:listbox[Blocks]", { state: "gone" });
  await app.type("label:Document", "Next steps");
  await app.waitFor("text:Next steps");
  // Heading 2 is 24 points (`text-2xl`).
  expect(app.spanStyle("text:Next steps").fontSize).toBe(24);
});

test("a press on a block of the menu picks it, and Escape closes the menu", async () => {
  const app = await openDemo();
  await newLine(app);
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  await app.keyboard.press("escape");
  await app.waitFor("role:listbox[Blocks]", { state: "gone" });
  // The slash stays as text.
  await app.keyboard.press("backspace");
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  await app.press("role:option[Quote]");
  await app.waitFor("role:listbox[Blocks]", { state: "gone" });
  await app.type("label:Document", "A quote");
  await app.waitFor("text:A quote");
  expect(app.spanStyle("text:A quote").italic).toBe(true);
});

test("a selection shows the toolbar, whose buttons format it", async () => {
  const app = await openDemo();
  await app.press(REVIEW);
  await app.keyboard.press("home");
  await app.keyboard.press("shift+end");
  await app.waitFor("role:toolbar[Format]");
  await app.press("role:button[Bold]");
  expect(app.spanStyle(`text:${REVIEW}`).fontWeight).toBe(700);
  // The toolbar follows the state: bold is on while the selection is bold.
  await app.press("role:button[Bold]");
  expect(app.spanStyle(`text:${REVIEW}`).fontWeight).toBe(400);
  // A caret without a range has no toolbar.
  await app.keyboard.press("right");
  await app.waitFor("role:toolbar[Format]", { state: "gone" });
});

test("the handle follows the pointer, and its menu turns the block into another", async () => {
  const app = await openDemo(true);
  await app.hover(`text:${REVIEW}`);
  await app.waitFor("role:button[Drag to move, press for the menu]");
  await app.press("role:button[Drag to move, press for the menu]");
  await app.waitFor("role:menu[Block]");
  await app.press("role:menuitem[Heading 3]");
  await app.waitFor("role:menu[Block]", { state: "gone" });
  // Heading 3 is 20 points (`text-xl`).
  expect(app.spanStyle(`text:${REVIEW}`).fontSize).toBe(20);
});

test("the handle's menu duplicates and deletes the block", async () => {
  const app = await openDemo(true);
  const before = app.count(`text:${REVIEW}`);
  await app.hover(`text:${REVIEW}`);
  await app.press("role:button[Drag to move, press for the menu]");
  await app.press("role:menuitem[Duplicate]");
  await app.waitFor(`text:${REVIEW} >> nth=1`);
  expect(app.count(`text:${REVIEW}`)).toBe(before + 1);
  await app.hover(`text:${REVIEW} >> nth=1`);
  await app.press("role:button[Drag to move, press for the menu]");
  await app.press("role:menuitem[Delete]");
  await app.waitFor(`text:${REVIEW} >> nth=1`, { state: "gone" });
  expect(app.count(`text:${REVIEW}`)).toBe(before);
});

test("+ adds a block below with the menu open", async () => {
  const app = await openDemo(true);
  await app.hover(`text:${REVIEW}`);
  await app.press("role:button[Add a block below]");
  await app.waitFor("role:listbox[Blocks]");
});

test("dragging the handle moves the block above another", async () => {
  const app = await openDemo(true);
  const QUOTE = "Good tools disappear: people should remember the work, not the editor.";
  const index = (text: string) => app.texts().findIndex((t) => t.includes(text));
  expect(index(QUOTE)).toBeGreaterThan(index("This week"));
  await app.hover(`text:${QUOTE}`);
  await app.waitFor("role:button[Drag to move, press for the menu]");
  // Up past the task list and its heading.
  await app.drag("role:button[Drag to move, press for the menu]", { y: -260 });
  await app.wait(300);
  expect(index(QUOTE)).toBeLessThan(index("This week"));
});

/** The index of the first text containing `text`, in the order the document shows them. */
function order(app: { texts(): string[] }) {
  return (text: string) => app.texts().findIndex((t) => t.includes(text));
}

test("every item of a list has a handle that moves it: bulleted, numbered and to-do", async () => {
  const app = await openDemo(true);
  const index = order(app);
  const handle = "role:button[Drag to move, press for the menu]";
  // A bulleted item down past the next one.
  await app.hover("text:Ship the new order ticket to every region");
  await app.waitFor(handle);
  await app.drag(handle, { y: 70 });
  await app.wait(300);
  expect(index("Cut the time to first trade by a third")).toBeLessThan(index("Ship the new order ticket to every region"));
  // The other items stay together and in order.
  expect(index("Cut the time to first trade by a third")).toBeLessThan(index("Move the help center to the new editor"));
  // A to-do item up past the one above it.
  await app.hover("text:Freeze the design of the ticket");
  await app.waitFor(handle);
  await app.drag(handle, { y: -50 });
  await app.wait(300);
  expect(index("Freeze the design of the ticket")).toBeLessThan(index("Review the pricing page copy"));
  // A numbered item down.
  await app.hover("text:Confirm the launch date");
  await app.waitFor(handle);
  await app.drag(handle, { y: 70 });
  await app.wait(300);
  expect(index("Brief the regional leads")).toBeLessThan(index("Confirm the launch date"));
  expect(index("Brief the regional leads")).toBeLessThan(index("Open the beta to five percent of traffic"));
});

test("a list item's menu turns it into another block and + adds an item of the same list", async () => {
  const app = await openDemo(true);
  const handle = "role:button[Drag to move, press for the menu]";
  await app.hover("text:Walk support through the new flow");
  await app.waitFor("role:button[Add a block below]");
  await app.press("role:button[Add a block below]");
  await app.waitFor("role:listbox[Blocks]");
  // The new line is an item of the list: its checkbox shows with the others.
  expect(app.count("role:checkbox")).toBe(5);
  await app.keyboard.press("escape");
  await app.type("label:Document", "Ship it");
  await app.wait(300);
  await app.hover("text:/Ship it");
  await app.waitFor(handle);
  await app.press(handle);
  await app.press("role:menuitem[Delete]");
  await app.wait(400);
  // The item and its checkbox are gone.
  expect(app.count("role:checkbox")).toBe(4);
});

test("a dragged block shows a preview that follows the pointer and a line where it will drop", async () => {
  const app = await openDemo(true);
  await app.hover("text:Good tools disappear: people should remember the work, not the editor.");
  await app.waitFor("role:button[Drag to move, press for the menu]");
  expect(app.count("id:block-ghost")).toBe(0);
  await app.dragAndHold("role:button[Drag to move, press for the menu]", { y: -200 });
  await app.waitFor("id:block-ghost");
  expect(app.texts().filter((t) => t.startsWith("Good tools disappear")).length).toBeGreaterThan(1);
  expect(app.count("id:rich-editor-drop-cursor")).toBe(1);
  await app.drop();
  await app.waitFor("id:block-ghost", { state: "gone" });
  expect(app.count("id:rich-editor-drop-cursor")).toBe(0);
});

test("the arrows keep the highlighted block in view in a list taller than the menu", async () => {
  const app = await openDemo();
  await newLine(app);
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  // Table is the last of eleven blocks, below what the menu shows at first.
  for (let i = 0; i < 10; i++) await app.keyboard.press("down");
  expect(app.shows("role:option[Table]")).toBe(true);
  await app.keyboard.press("enter");
  await app.waitFor("role:listbox[Blocks]", { state: "gone" });
});

test("the pointer over a block of the menu moves the highlight there, and Enter picks it", async () => {
  const app = await openDemo(true);
  await newLine(app);
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  await app.hover("role:option[Heading 2]");
  await app.keyboard.press("enter");
  await app.waitFor("role:listbox[Blocks]", { state: "gone" });
  await app.type("label:Document", "Plan");
  await app.waitFor("text:Plan");
  // Heading 2 is 24 points (`text-2xl`).
  expect(app.spanStyle("text:Plan").fontSize).toBe(24);
});

test("a pointer resting where the menu opens does not take the highlight, and each opening starts at the first block", async () => {
  const app = await openDemo(true);
  await newLine(app);
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  // The pointer goes over Divider (the highlight follows it) and stays; the menu is closed without a pick.
  await app.hover("role:option[Divider]");
  await app.keyboard.press("escape");
  await app.waitFor("role:listbox[Blocks]", { state: "gone" });
  await app.keyboard.press("backspace");
  // Opened again under the resting pointer: the first block is lit, so Down is Heading 1, not the one after Divider.
  await app.type("label:Document", "/");
  await app.waitFor("role:listbox[Blocks]");
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitFor("role:listbox[Blocks]", { state: "gone" });
  await app.type("label:Document", "Title");
  await app.waitFor("text:Title");
  // Heading 1 is 30 points (`text-3xl`).
  expect(app.spanStyle("text:Title").fontSize).toBe(30);
});

test("the handle goes when the document scrolls under it, and with Select All", async () => {
  const app = await open("../App.tsx", { platform: "macos", width: 1200, height: 520 });
  await app.press("role:button[Block editor]");
  await app.waitFor("Q4 launch plan");
  const handle = "role:button[Drag to move, press for the menu]";
  await app.hover("text:Ship the new order ticket to every region");
  await app.waitFor(handle);
  await app.scroll("id:block-document", 200);
  await app.waitFor(handle, { state: "gone" });
  // Back over a block, then everything selected: the toolbar shows, not the handle.
  await app.hover("text:Review the pricing page copy");
  await app.waitFor(handle);
  await app.press("Review the pricing page copy");
  await app.keyboard.press("mod+a");
  await app.waitFor("role:toolbar[Format]");
  expect(app.count(handle)).toBe(0);
});

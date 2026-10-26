// The Rich Editor demo: `nsh/base`'s `RichEditor` with the starter keymap
// and a toolbar whose buttons run commands on the session the editor shows.
import { expect, open, test } from "nsh/test";

test("Tab and Shift-Tab indent list items, and the toolbar edits the shown editor", async () => {
  const app = await open("../App.tsx");
  await app.press("role:button[Rich Editor]");
  await app.waitFor("Tab indents an item, Shift-Tab outdents it");
  // The caret in the second item: Tab nests it under the first.
  await app.press("Tab indents an item, Shift-Tab outdents it");
  await app.keyboard.press("tab");
  await app.waitFor("◦");
  // Shift-Tab takes it back out.
  await app.keyboard.press("shift+tab");
  await app.waitFor("◦", { state: "gone" });
  // A toolbar button runs a command while the editor is shown: the view
  // follows the session's new state.
  await app.keyboard.press("tab");
  await app.waitFor("◦");
  await app.press("role:button[Undo]");
  await app.waitFor("◦", { state: "gone" });
  expect(app.shows("Tab indents an item, Shift-Tab outdents it")).toBe(true);
});

test("HTML copied from a page keeps its formatting when pasted", async () => {
  const app = await open("../App.tsx");
  await app.press("role:button[Rich Editor]");
  await app.waitFor("Tab indents an item, Shift-Tab outdents it");
  await app.press("Tab indents an item, Shift-Tab outdents it");
  // As a browser copies a selection: plain text and the same as HTML.
  await app.paste("label:Document", {
    text: "Pasted title\nSome strong words",
    html:
      '<meta charset="utf-8"><h2>Pasted title</h2>' +
      '<p>Some <span style="font-weight: 700">strong</span> <em>words</em></p>',
  });
  // The heading's text joins the item the caret is in (an open slice), the
  // paragraph is a block of its own; marks keep their formatting.
  await app.waitFor("45 words · 237 characters");
  const texts = app.texts();
  expect(texts.some((t) => t.endsWith("Pasted title"))).toBe(true);
  expect(texts.some((t) => t.startsWith("Some strong words"))).toBe(true);
  expect(app.spanStyle("text:strong").fontWeight).toBe(700);
  expect(app.spanStyle("text:Some strong").fontWeight).toBe(400);
  expect(app.spanStyle("text:words >> nth=0").italic).toBe(true);
});

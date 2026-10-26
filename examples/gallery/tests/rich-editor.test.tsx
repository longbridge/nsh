// The Rich editor demo: `nsh/base`'s `RichEditor` with the starter keymap
// and a toolbar whose buttons run commands on the session the editor shows.
import { expect, open, test } from "nsh/test";

test("Tab and Shift-Tab indent list items, and the toolbar edits the shown editor", async () => {
  const app = await open("../App.tsx");
  await app.press("role:button[Rich editor]");
  await app.waitFor("Tab indents an item");
  // The caret in the second item: Tab nests it under the first.
  await app.press("Tab indents an item");
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
  expect(app.shows("Tab indents an item")).toBe(true);
});

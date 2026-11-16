// Presses on node views (`addNodeView`): one on a view's own area selects
// its node; a button inside a view keeps its press, and the selection
// stays where it was. (A press on an inline view's own area is checked in
// `nsh-base`'s Rust tests and on the platforms, `--target macos`: the
// headless screen does not place a text's inline views on its lines.)
import { expect, open, test } from "nsh/test";

const openEditor = async () => {
  const app = await open("./fixtures/rich-editor-node-view-press.tsx");
  await app.waitFor("text:Card 42");
  // The caret at the end of the last paragraph.
  await app.press("text:/^End/");
  await app.keyboard.press("end");
  await app.waitFor("text:selection: text 16");
  return app;
};

test("a press on a block view's own area selects its node", async () => {
  const app = await openEditor();
  await app.press("text:Card 42");
  await app.waitFor("text:selection: node 11");
  await app.waitFor("text:Card 42 selected");
});

test("a button inside a view keeps its press", async () => {
  const app = await openEditor();
  await app.press("id:card-button");
  await app.waitFor("text:pressed: card button");
  await app.press("id:tag-button");
  await app.waitFor("text:pressed: tag button");
  expect(app.shows("text:selection: text 16")).toBe(true);
  expect(app.shows("text:Tag AAPL selected")).toBe(false);
});

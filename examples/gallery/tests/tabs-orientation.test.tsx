// Tabs from TSX: a row (no `orientation`, the default) moves its selection
// with Right and leaves Down to the page; a vertical list moves with Down and
// leaves Right to the page. A horizontal list that also took Down would move
// the focus to the next tab there and select it (automatic activation).
import { expect, open, test } from "nsh/test";

const desktop = { width: 600, height: 500, platform: "macos" } as const;

test("a row moves with Right, and Down stays on its tab", async () => {
  const app = await open("./fixtures/tabs-orientation.tsx", desktop);
  await app.keyboard.press("tab");
  await app.keyboard.press("right");
  await app.waitFor("text:Row selected: two");
  await app.keyboard.press("down");
  await app.wait(300);
  expect(app.shows("text:Row selected: two")).toBe(true);
  expect(app.shows("text:Row selected: three")).toBe(false);
});

test("a vertical list moves with Down, and Right stays on its tab", async () => {
  const app = await open("./fixtures/tabs-orientation.tsx", desktop);
  // The row, then the column.
  await app.keyboard.press("tab");
  await app.keyboard.press("tab");
  await app.keyboard.press("down");
  await app.waitFor("text:Column selected: two");
  await app.keyboard.press("right");
  await app.wait(300);
  expect(app.shows("text:Column selected: two")).toBe(true);
  expect(app.shows("text:Column selected: three")).toBe(false);
});

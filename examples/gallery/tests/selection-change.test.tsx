// A selectable view's and a selectable TextView's `onSelectionChange`: a
// drag selects and reports the text, select all reports every text, and
// clearing the selection reports nothing selected. A headless run selects
// nothing (it draws no text): the real platforms run it (`nsh test
// --target web`, `--target linux`).
import { expect, open, test } from "nsh/test";

const desktop = { width: 600, height: 500 } as const;

test("a selectable view reports its selected text", async () => {
  const app = await open("./fixtures/selection-change.tsx", desktop);
  const web = app.shows("Platform: web");
  if (!web && !app.shows("Platform: linux")) return;
  expect(app.shows("text:Area []")).toBe(true);
  // From the first text (its box is wider than its line: from the line's
  // end) into the second.
  await app.drag("text:Alpha beta gamma", { x: 0, y: 40 });
  await app.waitUntil(() => app.texts().some((t) => t.startsWith("Area [") && t.includes("Delta") && t !== "Area [Alpha beta gamma\nDelta epsilon]"));
  if (web) {
    // The test agent's Ctrl+A selects in focused fields only elsewhere.
    await app.keyboard.press("ctrl+a");
    await app.waitFor("text:Area [Alpha beta gamma\nDelta epsilon]");
    await app.keyboard.press("escape");
  } else {
    // A click puts the caret: nothing selected.
    await app.drag("text:Delta epsilon", { x: 1, y: 0 });
  }
  await app.waitFor("text:Area []");
  expect(app.shows("text:Doc []")).toBe(true);
});

test("a selectable TextView reports its selected text", async () => {
  const app = await open("./fixtures/selection-change.tsx", desktop);
  const web = app.shows("Platform: web");
  if (!web && !app.shows("Platform: linux")) return;
  // Back from the line's end: its last characters.
  await app.drag("text:Quote this part", { x: -150, y: 0 });
  await app.waitUntil(() => app.texts().some((t) => /^Doc \[.*part\]$/.test(t) && t !== "Doc [Quote this part]"));
  if (web) {
    await app.keyboard.press("ctrl+a");
    await app.waitFor("text:Doc [Quote this part]");
    await app.keyboard.press("escape");
  } else {
    await app.drag("text:Quote this part", { x: 1, y: 0 });
  }
  await app.waitFor("text:Doc []");
  expect(app.shows("text:Area []")).toBe(true);
});

// The Rich editor demo's Markdown editor as a user types in it: these
// tests name only the editor's own text and use the keyboard, so they
// hold whatever the toolbar and the rest of the demo look like.
import { type App, expect, open, test } from "nsh/test";

const PARAGRAPH = "text:/^Type Markdown as you go/";

async function openDemo() {
  const app = await open("../App.tsx");
  await app.press("role:button[Rich editor]");
  await app.waitFor("Tab indents an item, Shift-Tab outdents it");
  return app;
}

/** Selects a textblock as a user does: three presses on it. */
async function selectBlock(app: App, selector: string) {
  await app.press(selector);
  await app.press(selector);
  await app.press(selector);
}

/** Types into the focused editor key by key, as a hardware keyboard does. */
async function typeKeys(app: App, text: string) {
  // Into the editor's input, as a keyboard types it (capitals and `#`
  // included).
  await app.type("id:markdown-editor", text);
}

test("## and a space make the block a heading", async () => {
  const app = await openDemo();
  await selectBlock(app, PARAGRAPH);
  await typeKeys(app, "## Overview");
  await app.waitFor("Overview");
  expect(app.shows(PARAGRAPH)).toBe(false);
});

test("- and a space make the block a list item", async () => {
  const app = await openDemo();
  const bullets = app.count("•");
  await selectBlock(app, PARAGRAPH);
  await typeKeys(app, "- Point");
  await app.waitFor("Point");
  expect(app.count("•")).toBe(bullets + 1);
});

test("Enter twice at the end of a list leaves it for a paragraph", async () => {
  const app = await openDemo();
  const bullets = app.count("•");
  // Replace the last item's text, leaving the caret at its end.
  await selectBlock(app, "Tab indents an item, Shift-Tab outdents it");
  await typeKeys(app, "Last");
  await app.waitFor("Last");
  await app.keyboard.press("enter");
  expect(app.count("•")).toBe(bullets + 1);
  await app.keyboard.press("enter");
  await typeKeys(app, "After");
  await app.waitFor("After");
  // The empty item became the paragraph: no marker in front of it.
  expect(app.count("•")).toBe(bullets);
});

test("Mod-z undoes typing and Shift-Mod-z redoes it", async () => {
  const app = await openDemo();
  await selectBlock(app, "Lists split on Enter");
  await typeKeys(app, "Changed");
  await app.waitFor("Changed");
  await app.keyboard.press("mod+z");
  await app.waitFor("Lists split on Enter");
  expect(app.shows("Changed")).toBe(false);
  await app.keyboard.press("shift+mod+z");
  await app.waitFor("Changed");
});

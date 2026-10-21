// The Overlays demo on a phone (headless runs are touch screens): the
// overlay over a clipping box, a popover, a tooltip a held finger opens, a
// sheet; Escape and back close the top one. Menus: menus.test.tsx.
import { beforeEach, expect, open, test } from "nsh/test";
import type { App } from "nsh/test";

let app: App;
beforeEach(async () => {
  app = await open("../App.tsx", { clock: {} });
  await app.press("role:button[Overlays]");
  await app.waitFor("Open the overlay");
});

test("an overlay shows outside the box that clips it and closes on a press outside", async () => {
  await app.press("Open the overlay");
  await app.waitFor("Outside the clipped box, on top of the page.");
  await app.waitFor("overlay at bottom");
  await app.press("Show the sheet");
  expect(app.texts()).not.toContain("Outside the clipped box, on top of the page.");
});

test("a popover opens from its trigger and back closes it", async () => {
  await app.press("Show the popover");
  await app.waitFor("A press outside, Escape or back closes it.");
  await app.waitFor("popover opened (triggerPress)");
  await app.back();
  await app.waitFor("popover closed (cancel)");
});

test("a held finger shows the tooltip", async () => {
  await app.longPress("Save");
  await app.waitFor("Saves the document");
});

test("a sheet closes with back", async () => {
  await app.press("Show the sheet");
  await app.waitFor("The backdrop, Escape and back close it.");
  await app.back();
  await app.waitFor("sheet closed (cancel)");
});

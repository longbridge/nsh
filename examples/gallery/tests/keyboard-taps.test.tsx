// A scroll view's `keyboardShouldPersistTaps` at its default, "never": with
// the keyboard up, a tap only hides it and presses nothing; long presses and
// drags reach the views inside as they do with the keyboard down. A headless
// run shows no keyboard: its taps press.
import { expect, open, test } from "nsh/test";

test("a tap in a scroll view hides the keyboard of a field below it", async () => {
  const app = await open("./fixtures/keyboard-taps.tsx");
  await app.type("id:message", "test1");
  const up = app.shows("text:keyboard: up");
  await app.press("text:/^Row 1/");
  await app.waitFor("text:keyboard: down");
  await app.wait(500);
  expect(app.texts()).toContain(up ? "Row 1, pressed 0" : "Row 1, pressed 1");
});

test("a long press in a scroll view goes through with the keyboard up", async () => {
  const app = await open("./fixtures/keyboard-taps.tsx");
  await app.type("id:message", "test1");
  const up = app.shows("text:keyboard: up");
  await app.longPress("text:/^Row 3/");
  await app.waitFor("text:Row 3, held 1");
  await app.wait(500);
  expect(app.texts()).toContain(up ? "keyboard: up" : "keyboard: down");
  expect(app.texts()).toContain("Row 1, pressed 0");
});

test("a held finger in a scroll view opens a context menu with the keyboard up", async () => {
  const app = await open("./fixtures/keyboard-taps.tsx");
  await app.type("id:message", "test1");
  await app.longPress("text:Row 2");
  await app.waitFor("text:menu-row");
});

test("a drag in a scroll view scrolls it with the keyboard up", async () => {
  const app = await open("./fixtures/keyboard-taps.tsx");
  await app.type("id:message", "test1");
  const drags = () => Number(app.texts().find((t) => t.startsWith("drags: "))?.slice(7));
  const before = drags();
  await app.drag("text:/^Row 3/", { x: 0, y: -200 });
  await app.waitUntil(() => drags() > before);
});

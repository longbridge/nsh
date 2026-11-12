// A hardware keyboard's key reaches the app's shortcuts (`nsh/keymap`)
// while no input has the focus: before any had it, and after one gave it
// up. On phones and tablets as on the desktops.
import { open, test } from "nsh/test";

test("a shortcut fires with no input focused", async () => {
  const app = await open("./fixtures/shortcut-no-focus.tsx");
  await app.waitFor("text:shortcut: 0");
  await app.keyboard.press("k");
  await app.waitFor("text:shortcut: 1");
  // The return key takes the focus out of a single-line input.
  await app.type("id:field", "note");
  await app.submit("id:field");
  await app.keyboard.press("k");
  await app.waitFor("text:shortcut: 2");
});

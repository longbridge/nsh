// `Editor`'s `keyBindings`: keys the app takes from the editor while it has
// the focus, reported by `onKeyBinding` (`fixtures/editor-key-bindings.tsx`).
import { expect, open, test } from "nsh/test";

const desktop = { platform: "macos", width: 600, height: 400 } as const;

async function start() {
  const app = await open("./fixtures/editor-key-bindings.tsx", desktop);
  await app.waitFor("text:/taken/");
  return app;
}

const status = (app: Awaited<ReturnType<typeof start>>) => app.texts().find((t) => t.startsWith("taken"));

test("a key the app takes runs instead of the editor's own", async () => {
  const app = await start();
  await app.press("text:select 1");
  // The editor's mod+enter inserts a line below; the app's runs instead.
  await app.keyboard.press("mod+enter");
  await app.waitFor("text:/taken mod\\+enter /");
  expect(status(app)).toBe("taken mod+enter app 0 lines 1");
  // The keystroke comes back as keyBindings writes it.
  await app.keyboard.press("mod+shift+f");
  await app.waitFor("text:/taken mod\\+enter,mod\\+shift\\+f /");
});

test("the editor keeps the keys the app does not take", async () => {
  const app = await start();
  await app.press("text:select 1");
  await app.keyboard.press("mod+a");
  await app.type("id:sql", "select 2");
  expect(app.value("id:sql")).toBe("select 2");
  expect(status(app)).toBe("taken - app 0 lines 1");
});

test("a key the editor binds only at times is the app's too", async () => {
  const app = await start();
  await app.press("text:select 1");
  // With a selection, the editor binds escape to collapse it.
  await app.keyboard.press("mod+a");
  await app.keyboard.press("escape");
  await app.waitFor("text:/taken escape /");
});

test("without the focus the editor binds none of them", async () => {
  const app = await start();
  await app.press("text:select 1");
  await app.press("blur");
  await app.wait(100);
  await app.keyboard.press("mod+enter");
  await app.waitFor("text:/app 1/");
  expect(status(app)).toBe("taken - app 1 lines 1");
});

test("a key the app gives back is the editor's again", async () => {
  const app = await start();
  await app.press("release");
  await app.press("text:select 1");
  await app.keyboard.press("mod+enter");
  await app.waitFor("text:/lines 2/");
  expect(status(app)).toBe("taken - app 0 lines 2");
});

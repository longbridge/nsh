import { expect, open, test } from "nsh/test";

test("a desktop app root that returns a view renders in the implicit first window", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  expect(app.shows("No window")).toBe(true);
  // titleBar() reads the implicit window in the app root.
  expect(app.shows("Bar: 0")).toBe(true);
  expect(app.windowIds()).toEqual(["main"]);
});

test("a desktop app root that declares no Window gets the implicit first window", async () => {
  const app = await open("./Empty.tsx", { platform: "macos" });
  expect(app.windowIds()).toEqual(["main"]);
});

test("off the desktops the same view is the one surface", async () => {
  const app = await open("./App.tsx", { platform: "ios" });
  expect(app.shows("No window")).toBe(true);
});

test("closing the implicit main window quits on macOS by default", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  expect(app.quit()).toBe(false);
  await app.window("main").pressClose();
  await app.waitUntil(() => app.window("main").state() === null);
  expect(app.quit()).toBe(true);
});

test("an implicit main window respects Application.setQuitMode(\"platform\")", async () => {
  const app = await open("./PlatformQuit.tsx", { platform: "macos" });
  await app.window("main").pressClose();
  await app.waitUntil(() => app.window("main").state() === null);
  expect(app.quit()).toBe(false);
});

test("an implicit main window respects nsh.json application.quitMode", async () => {
  const app = await open("./configured/App.tsx", { platform: "macos" });
  await app.window("main").pressClose();
  await app.waitUntil(() => app.window("main").state() === null);
  expect(app.quit()).toBe(false);
});

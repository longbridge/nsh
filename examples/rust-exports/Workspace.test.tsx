import { expect, open, test } from "nsh/test";

test("the app plugin selects a palette through the official theme service", async () => {
  const app = await open("./Workspace.tsx");
  await app.waitFor("Workspace: built");
  expect(app.spanStyle("Workspace preview").color).toBe("#2563ebff");

  await app.press("Ocean");
  await app.waitFor("Workspace: ocean");
  expect(app.shows("Read at once: #0369a1ff")).toBe(true);
  expect(app.spanStyle("Workspace preview").color).toBe("#0369a1ff");

  await app.setColorScheme("dark");
  await app.waitFor("Accent: #67e8f9ff");
  expect(app.spanStyle("Workspace preview").color).toBe("#67e8f9ff");

  await app.press("Built theme");
  await app.waitFor("Workspace: built");
  expect(app.shows("Read at once: #93c5fdff")).toBe(true);
  expect(app.spanStyle("Workspace preview").color).toBe("#93c5fdff");
});

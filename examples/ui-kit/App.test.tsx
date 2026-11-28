import { expect, open, test } from "nsh/test";

test("tabs switch panels", async () => {
  const app = await open("./App.tsx");
  await app.waitFor("Region");
  await app.press("Account");
  await app.waitFor("Signed in as demo@example.com");
  expect(app.texts()).not.toContain("Region");
});

test("select picks a region with the keyboard-capable element", async () => {
  const app = await open("./App.tsx");
  await app.press("Choose a region");
  await app.press("Singapore");
  await app.waitFor("Singapore");
});

test("menu runs an item and a submenu item", async () => {
  const app = await open("./App.tsx");
  await app.press("More actions");
  await app.press("Duplicate");
  await app.waitFor("Last action: Duplicate");
  await app.press("More actions");
  await app.press("Share");
  await app.press("Copy link");
  await app.waitFor("Last action: Copy link");
});

test("icon button has its label", async () => {
  const app = await open("./App.tsx");
  await app.press("Add");
  await app.waitFor("Last action: Add");
});

test("select works from the keyboard", async () => {
  const app = await open("./App.tsx");
  await app.press("Choose a region");
  await app.keyboard.press("down");
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitFor("Hong Kong");
});

test("escape closes the menu", async () => {
  const app = await open("./App.tsx");
  await app.press("More actions");
  await app.waitFor("Duplicate");
  await app.keyboard.press("escape");
  expect(app.texts()).not.toContain("Duplicate");
});

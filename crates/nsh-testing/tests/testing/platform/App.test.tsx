import { expect, open, test } from "nsh/test";

/** What `promise` rejected with, or "" when it did not. */
async function rejection(promise: Promise<void>): Promise<string> {
  try {
    await promise;
    return "";
  } catch (e) {
    return String(e);
  }
}

test("back pops the screen pushed last", async () => {
  const app = await open("./App.tsx");
  await app.press("Open");
  await app.waitFor("Detail body");
  await app.back();
  expect(app.shows("Detail body")).toBe(false);
  expect(app.texts()).toContain("Answer: none");
  expect(await rejection(app.back())).toContain("nothing to go back from");
});

test("a screen that prevents going back is asked", async () => {
  const app = await open("./App.tsx");
  await app.press("Draft");
  await app.back();
  await app.waitFor("Asked: yes");
});

test("back closes a modal", async () => {
  const app = await open("./App.tsx");
  await app.press("Sheet");
  await app.waitFor("In the sheet");
  expect(app.texts()).toContain("Shown: 1");
  await app.back();
  expect(app.shows("In the sheet")).toBe(false);
});

test("a dialog is cancelled unless the test answers it", async () => {
  const app = await open("./App.tsx");
  await app.press("Delete");
  await app.waitFor("Answer: Cancel");
  expect(app.dialogs()[0].message).toBe("It is gone after.");
  app.onDialog((dialog) => (dialog.title === "Delete?" ? "Delete" : null));
  await app.press("Delete");
  await app.waitFor("Answer: Delete");
  // Closing it without a button is the user's choice only when it can be.
  expect(await rejection(app.press("Update"))).toContain("cannot be dismissed");
});

test("a dialog without a cancel button needs an answer", async () => {
  const app = await open("./App.tsx");
  expect(await rejection(app.press("Update"))).toContain("needs an answer");
});

test("the clipboard holds what was copied", async () => {
  const app = await open("./App.tsx", { clipboard: "seed" });
  await app.press("Paste");
  await app.waitFor("Pasted: seed");
  await app.press("Copy");
  expect(app.clipboard()).toBe("copied text");
});

test("native animations run to their end", async () => {
  const app = await open("./App.tsx");
  await app.press("Move");
  await app.waitFor("Moved: done");
});

test("what only a real platform answers fails the step", async () => {
  const app = await open("./App.tsx");
  expect(await rejection(app.press("Measure"))).toContain("only a real platform answers");
});

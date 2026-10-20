// The Storage demo with storage set up by the test: `open`'s `storage` is
// what `nsh/storage` holds when the app starts, `app.storage()` what it
// holds now.
import { expect, open, test } from "nsh/test";

test("counts the demo's openings and saves the note as it is typed", async () => {
  const app = await open("../App.tsx", { storage: { launches: "4", note: "Milk" } });
  await app.press("label:Storage");
  await app.waitFor("Launches");
  await app.waitFor('setItem("launches", "5")');
  expect(app.value("label:Note")).toBe("Milk");
  await app.type("label:Note", ", eggs");
  await app.waitFor("Saved");
  expect(app.storage().launches).toBe("5");
  expect(app.storage().note).toBe("Milk, eggs");
});

test("clearing the storage asks first", async () => {
  const app = await open("../App.tsx", { storage: { launches: "4", note: "Milk" } });
  await app.press("label:Storage");
  await app.waitFor("Milk");
  // Unanswered, the dialog's cancel button keeps everything.
  await app.press("Clear all");
  expect(app.dialogs()[0].title).toBe("Clear the storage?");
  expect(app.storage().note).toBe("Milk");
  app.onDialog((dialog) => (dialog.title === "Clear the storage?" ? "Clear" : null));
  await app.press("Clear all");
  await app.waitFor("Nothing stored");
  expect(app.storage()).toEqual({});
});

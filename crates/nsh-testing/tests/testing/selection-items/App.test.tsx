import { open, test } from "nsh/test";

// Rows' texts are 154 points wide (20 characters of the test engine): a
// drag starts at offset 10, and 46 points are 6 characters.

test("the same text in two rows names the row it is in", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  await app.drag("id:text-b", { x: -45, y: 0 });
  await app.waitFor('Ends: b@10 b@4 "tests "');
  await app.drag("id:text-a", { x: -45, y: 0 });
  await app.waitFor('Ends: a@10 a@4 "tests "');
});

test("a drag forward keeps the anchor where it started", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  await app.drag("id:text-b", { x: 46, y: 0 });
  await app.waitFor('Ends: b@10 b@16 "passed"');
});

test("a drag across rows has its ends in each row", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  await app.drag("id:text-b", { x: 0, y: -37 });
  await app.waitFor('Ends: b@10 a@10 "passed.\\n\\nAll tests "');
});

test("the offset counts the item's earlier paragraphs as a copy gives them", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  // The TextView is 300 points wide: the drag starts past the end of the
  // paragraph (offset 12) and goes back to offset 6.
  await app.drag("text:Second para.", { x: -100, y: 0 });
  await app.waitFor('Ends: c@25 c@19 " para."');
});

test("a row inserted above the selection leaves its ends in their items", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  await app.press("id:insert");
  await app.drag("id:text-b", { x: -45, y: 0 });
  await app.waitFor("text:Inserted above.");
  // Shift extends the selection from its anchor, which stayed in row b.
  await app.keyboard.down("shift");
  await app.drag("id:text-a", { x: -45, y: 0 });
  await app.waitFor('Ends: b@10 a@4 "tests passed.\\n\\nAll tests "');
});

test("a row replaced under the selection names the item now there", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  await app.press("id:replace");
  await app.drag("id:text-b", { x: -45, y: 0 });
  await app.waitFor("id:text-d");
  await app.keyboard.down("shift");
  await app.drag("id:text-a", { x: -45, y: 0 });
  await app.waitFor('Ends: d@10 a@4 "tests passed.\\n\\nAll tests "');
});

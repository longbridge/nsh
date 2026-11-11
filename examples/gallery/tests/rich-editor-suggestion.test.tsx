// A suggestion takes its captured keys only while the app's list has
// something to pick (`setSuggestionItemCount`).
import { expect, open, test } from "nsh/test";

const openComposer = () => open("./fixtures/rich-editor-suggestion.tsx");

test("Enter is the list's while people match", async () => {
  const app = await openComposer();
  await app.type("id:composer", "hi @al");
  await app.waitFor("text:items: 1");
  await app.keyboard.press("enter");
  await app.waitFor("text:lastKey: Enter");
  expect(app.shows("text:paragraphs: 1")).toBe(true);
});

test("Enter goes to the editor when nobody matches", async () => {
  const app = await openComposer();
  await app.type("id:composer", "mail a @zz");
  await app.waitFor("text:items: 0");
  await app.keyboard.press("enter");
  await app.waitFor("text:paragraphs: 2");
  expect(app.shows("text:lastKey: none")).toBe(true);
});

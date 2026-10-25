// The Editor demo: `nsh/base`'s `Editor` with the app's find bar, whose
// inputs pass a `search` and whose buttons call the ref.
import { expect, open, test } from "nsh/test";

test("the Editor demo finds, replaces and switches samples", async () => {
  const app = await open("../App.tsx");
  await app.press("role:button[Editor]");
  // The Rust sample, a row per line, with its warning.
  await app.waitFor("fn main() {");
  expect(app.shows("1 problem")).toBe(true);
  expect(app.shows("Ln 1, Col 1")).toBe(true);
  // "word" in any case, anywhere: in `words` and `word_counts` too.
  await app.type("label:Find", "word");
  await app.waitFor("7 matches");
  // Whole words only, then every one replaced as one edit.
  await app.press("label:Whole word");
  await app.waitFor("4 matches");
  await app.type("label:Replace", "term");
  await app.press("label:Replace all");
  await app.waitFor("Replaced 4");
  await app.waitFor("No results");
  // Another language's sample, in an editor of its own.
  await app.press("TypeScript");
  await app.waitFor("interface Todo {");
});

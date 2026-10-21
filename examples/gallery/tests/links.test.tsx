// Spans of a text that take presses (a nested `<Text onPress>`): found by
// their characters, pressed where they are drawn.
import { expect, open, test } from "nsh/test";

test("the links of a sentence take their own presses", async () => {
  const app = await open("../App.tsx");
  await app.press("label:Text");
  await app.waitFor("Spans and links");
  expect(app.shows("nothing yet")).toBe(true);
  await app.press("Privacy policy");
  await app.waitUntil(() => app.texts().includes("Privacy policy"));
  await app.press("text:links");
  await app.waitFor("a link inside bold text");
  await app.longPress("Terms of service");
  await app.waitFor("Terms (long press)");
  // The span's characters are not texts of their own.
  expect(app.texts().filter((t) => t === "Terms of service")).toEqual([]);
});

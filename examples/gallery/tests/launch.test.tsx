// Launch parameters (`host.params`) open a demo, or one section of it, with
// no taps or scrolling: what tests and screenshots of one demo start from.
import { expect, open, test } from "nsh/test";

test("`demo` opens that demo, its name in any case", async () => {
  const app = await open("../App.tsx", { host: { params: { demo: "accordion" } } });
  await app.waitFor("When will my order ship?");
  expect(app.texts()).toContain("Settings");
});

test("`section` shows only the section with that title", async () => {
  const app = await open("../App.tsx", { host: { params: { demo: "Accordion", section: "Settings" } } });
  await app.waitFor("Several sections open together, controls inside, one nested");
  expect(app.texts()).not.toContain("When will my order ship?");
  expect(app.texts()).not.toContain("Last event");
});

test("a demo no one has is ignored: the index shows", async () => {
  const app = await open("../App.tsx", { host: { params: { demo: "Nothing like it" } } });
  await app.waitFor("Accordion");
  expect(app.texts()).not.toContain("When will my order ship?");
});

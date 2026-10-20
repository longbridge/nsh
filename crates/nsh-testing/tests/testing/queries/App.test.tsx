import { expect, open, test } from "nsh/test";

test("an action on a selector matching several views says which to pick", async () => {
  const app = await open("./App.tsx");
  expect(app.count("role:button[Delete]")).toBe(2);
  await expect(() => app.press("text:Delete")).rejects.toThrow(/matches 2 pressable views/);
  await app.press("id:row-Eggs >> role:button[Delete]");
  expect(app.texts()).not.toContain("Eggs");
  await app.press("text:Delete >> nth=0");
  expect(app.count("role:button[Delete]")).toBe(0);
});

test("a disabled button takes no press", async () => {
  const app = await open("./App.tsx");
  expect(app.enabled("role:button[Pay]")).toBe(false);
  expect(app.enabled("role:button[Delete] >> nth=0")).toBe(true);
  await expect(() => app.press("Pay")).rejects.toThrow("is disabled");
  expect(app.texts()).toContain("Log: none");
});

test("a long press is held, and no press follows", async () => {
  const app = await open("./App.tsx");
  await app.longPress("Hold me");
  expect(app.texts()).toContain("Log: held");
});

test("clearing an input empties it", async () => {
  const app = await open("./App.tsx");
  await app.clear("id:draft");
  expect(app.value("id:draft")).toBe("");
  await app.type("id:draft", "New");
  expect(app.texts()).toContain("Draft: New");
});

test("waits for what goes away and for a condition", async () => {
  const app = await open("./App.tsx");
  await app.waitFor("Loading…", { state: "gone" });
  await app.type("id:draft", "!");
  await app.waitUntil(() => app.value("id:draft") === "Old text!");
  await expect(() => app.waitUntil(() => false, { timeout: 100 })).rejects.toThrow("never held");
});

test("text selectors take a regex", async () => {
  const app = await open("./App.tsx");
  expect(app.count("text:/^(Milk|Eggs)$/")).toBe(2);
  expect(app.shows("text:/^draft:/i")).toBe(true);
});

test("expectations beyond equality", async () => {
  expect("Saved at 9:41").toMatch(/at \d+:\d+/);
  expect([1, 2, 3]).toHaveLength(3);
  expect(null).toBeNull();
  expect(5).toBeGreaterThan(4);
  expect(5).toBeLessThanOrEqual(5);
  expect(() => JSON.parse("{")).toThrow();
  expect(() => 1).not.toThrow();
});

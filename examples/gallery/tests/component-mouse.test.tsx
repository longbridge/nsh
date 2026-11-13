import { expect, open, test } from "nsh/test";

const openFixture = () => open("./fixtures/component-mouse.tsx", { platform: "macos" });

test("a Button, a Toggle and a Link hear the mouse as a view does", async () => {
  const app = await openFixture();
  await app.hover("id:preview");
  await app.waitFor("text:/^heard: button enter [\\d.]+,20; button move [\\d.]+,20$/");
  await app.hover("id:pin");
  await app.waitFor("text:/button leave; toggle enter$/");
  await app.hover("id:docs");
  await app.waitFor("text:/toggle leave; link enter$/");
});

test("a plot hears the mouse itself and hands it to the app too", async () => {
  const app = await openFixture();
  await app.hover("id:chart");
  await app.waitFor("text:/chart move/");
  expect(app.shows("text:/plot hover \\d/")).toBe(true);
});

test("a component the app gives no mouse listener hears none", async () => {
  const app = await openFixture();
  await app.hover("id:quiet");
  await app.wait(100);
  expect(app.shows("text:heard: nothing")).toBe(true);
});

import { expect, open, test } from "nsh/test";

const start = async () => {
  const app = await open("../App.tsx", { platform: "macos", width: 1200, height: 800, clock: { time: 1700000000000 } });
  await app.waitFor("Composite");
  return app;
};

test("stock quotes keep updating across periods, resizing and pause/resume", async () => {
  const app = await start();
  const price = () => app.texts().find((t) => t.startsWith("Price "));
  const initial = price();
  expect(initial).toBeDefined();
  await app.clock!.runFor(1000);
  expect(price()).not.toBe(initial);
  await app.press("label:Pause the market");
  const paused = price();
  await app.clock!.runFor(1000);
  expect(price()).toBe(paused);

  for (const period of ["1m", "5m", "15m", "1D"]) {
    await app.press(period);
    expect(app.shows(`label:ORBX ${period} chart`)).toBe(true);
    expect(app.texts().some((t) => t.startsWith("MA5 "))).toBe(true);
  }
  await app.setViewport(1000, 700);
  await app.press("Intraday");
  expect(price()).toBe(paused);
  await app.press("label:Resume the market");
  await app.clock!.runFor(1000);
  expect(price()).not.toBe(paused);

  await app.press("label:Lumora Optics");
  expect(app.shows("label:LUMO intraday chart")).toBe(true);
  expect(app.shows("label:ORBX intraday chart")).toBe(false);
});

test("stock chart overlays let a drag reach the crosshair after resize and period changes", async () => {
  const app = await start();
  await app.press("label:Pause the market");
  await app.dragAndHold("label:ORBX intraday chart", { x: -80, y: 0 });
  expect(app.texts().some((t) => t.startsWith("avg "))).toBe(true);
  await app.drop();
  expect(app.texts().some((t) => t.startsWith("avg "))).toBe(false);

  await app.setViewport(1000, 700);
  await app.press("1D");
  await app.dragAndHold("label:ORBX 1D chart", { x: -40, y: 0 });
  expect(app.texts().some((t) => t.startsWith("O ") && t.includes(" H "))).toBe(true);
  await app.drop();
  await app.press("Intraday");
  await app.dragAndHold("label:ORBX intraday chart", { x: -40, y: 0 });
  expect(app.texts().some((t) => t.startsWith("avg "))).toBe(true);
  await app.drop();
});

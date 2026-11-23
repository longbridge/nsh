// Where a header with no padding of its own lies under the traffic lights,
// on the macOS host (`nsh test crates/nsh-platform-macos/tests/host --target
// macos`): the window's buttons take `titleBar().left` points at the left of
// a hidden bar, so a first tab at x 0 is under them. `TitleBarTabs.tsx`.
import { expect, open, test } from "nsh/test";

function bar(app: { texts(): string[] }, window: string) {
  const line = app.texts().find((t) => t.startsWith(`Bar ${window}: `)) ?? "";
  const [height, left, right] = line.slice(`Bar ${window}: `.length).split(" ").map(Number);
  return { height, left, right };
}

test("a header with no padding lies under the traffic lights", async () => {
  const app = await open("./TitleBarTabs.tsx");
  await app.waitFor("Selected main: ");
  const area = bar(app, "main");
  const tab = app.layout("label:Tab !1113 main");
  const close = app.layout("label:Close !1113 main");
  console.log(`main bar ${JSON.stringify(area)} tab ${JSON.stringify(tab)} close ${JSON.stringify(close)}`);
  expect(area.height).toBe(44);
  expect(area.left).toBeGreaterThan(60);
  expect(tab?.x ?? -1).toBeLessThan(area.left);
});

test("each window reports its own title bar area", async () => {
  const app = await open("./TitleBarTabs.tsx");
  await app.waitFor("Selected main: ");
  await app.press("label:Open second");
  await app.waitFor("Selected second: ");
  const main = bar(app, "main");
  const second = bar(app, "second");
  console.log(`main ${JSON.stringify(main)} second ${JSON.stringify(second)}`);
  expect(main.height).toBe(44);
  expect(second.height).toBe(60);
  expect(second.left).toBe(main.left);
});

// Needs an unlocked desktop session: AppKit ignores `toggleFullScreen:`
// while the screen is locked, and the window stays in `normal`.
test("full screen has no title bar area, and leaving it brings the area back", async () => {
  const app = await open("./TitleBarTabs.tsx");
  await app.waitFor("Selected main: ");
  const main = bar(app, "main");
  await app.press("label:Full screen main");
  await app.waitFor("Mode main: fullScreen", { timeout: 10000 });
  await app.waitFor("Bar main: 0 0 0", { timeout: 10000 });
  await app.press("label:Normal main");
  await app.waitFor("Mode main: normal", { timeout: 10000 });
  await app.waitFor(`Bar main: 44 ${main.left} 0`, { timeout: 10000 });
});

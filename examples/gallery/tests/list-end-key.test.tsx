// End (Command+Down on macOS) scrolls a list to its end in one press, though
// the rows below were never rendered and the list only estimates their size:
// the list follows its end as they are measured. The desktops' and the
// web's keys (a headless run has no key scrolling); on Linux the keys must be
// real key events: Xvfb, or `--foreground` (docs/dev/linux.md).
import { open, test } from "nsh/test";

const desktop = { width: 480, height: 520 } as const;

test("End scrolls a list with rows not measured yet to its end in one press", async () => {
  const app = await open("./fixtures/list-end-key.tsx", desktop);
  if (!["macos", "windows", "linux", "web"].some((os) => app.shows(`OS: ${os}`))) return;
  await app.waitFor("Row 0");
  // 40 rows of 100 pt in 300 pt.
  await app.hover("id:list");
  await app.keyboard.press("end");
  await app.waitFor("text:y=3700 rows=40");

  // Two pages up, 30 rows more below, End again.
  await app.keyboard.press("pageup");
  await app.keyboard.press("pageup");
  await app.press("Add 30 rows");
  await app.waitUntil(() => app.texts().some((t) => t.endsWith("rows=70")));
  await app.hover("id:list");
  await app.keyboard.press("end");
  await app.waitFor("text:y=6700 rows=70");

  // At the end, the list follows rows added below.
  await app.press("Add 30 rows");
  await app.waitFor("text:y=9700 rows=100");
});

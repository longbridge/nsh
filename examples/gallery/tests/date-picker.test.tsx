// The DatePicker demo: a due date on weekdays with a clear button, a
// trip's range with presets, and a meeting's day and time. On a phone
// (headless runs are touch screens) the calendar is a sheet; on a desktop
// a popup. The app's clock starts on Wednesday 2026-10-28, so today and
// the presets are known; days are named by their ISO dates.
import { beforeEach, describe, expect, open, test } from "nsh/test";
import type { App } from "nsh/test";

describe("on a phone", () => {
  let app: App;
  beforeEach(async () => {
    app = await open("../App.tsx", { clock: { time: new Date(2026, 9, 28, 12) } });
    await app.press("role:button[DatePicker]");
    await app.waitFor("role:combobox[Due date]");
  });

  test("a due date is a weekday, and the clear button empties it", async () => {
    await app.press("role:combobox[Due date]");
    await app.waitFor("label:2026-10-30");
    // Saturday and Sunday take no press.
    expect(app.enabled("label:2026-10-31")).toBe(false);
    expect(app.enabled("label:2026-10-25")).toBe(false);
    expect(app.enabled("label:2026-10-29")).toBe(true);
    await app.press("label:2026-10-30");
    await app.waitFor("Due: 2026-10-30");
    await app.waitUntil(() => !app.shows("label:2026-10-30"));
    await app.waitFor("Oct 30, 2026");
    await app.press("label:Clear the due date");
    await app.waitFor("Due: none");
    await app.waitFor("No due date");
  });

  test("a trip takes two days, or a preset", async () => {
    await app.press("role:combobox[Trip]");
    await app.waitFor("label:2026-10-29");
    // The first day starts the range; only the second ends and reports it.
    await app.press("label:2026-10-29");
    expect(app.shows("label:2026-10-30")).toBe(true);
    expect(app.shows("—")).toBe(true);
    await app.press("label:2026-10-30");
    await app.waitFor("Trip: 2026-10-29 → 2026-10-30");
    await app.waitFor("Oct 29 – Oct 30");
    await app.waitUntil(() => !app.shows("label:2026-10-29"));
    // The coming weekend, from the app's today.
    await app.press("role:combobox[Trip]");
    await app.waitFor("role:button[Weekend]");
    await app.press("role:button[Weekend]");
    await app.waitFor("Trip: 2026-10-31 → 2026-11-01");
    await app.waitUntil(() => !app.shows("role:button[Weekend]"));
    await app.waitFor("Oct 31 – Nov 1");
  });

  test("the meeting's sheet stays open for its time, and its day again closes it", async () => {
    await app.press("role:combobox[Meeting]");
    await app.waitFor("label:2026-10-29");
    await app.press("label:2026-10-29");
    // The day at the default time; the sheet stays.
    await app.waitFor("Meeting: 2026-10-29T09:00");
    expect(app.shows("label:2026-10-29")).toBe(true);
    // The time field takes digits: the hour, then the minute.
    await app.type("label:Meeting time", "1430");
    await app.waitFor("Meeting: 2026-10-29T14:30");
    await app.press("label:2026-10-29");
    await app.waitUntil(() => !app.shows("label:2026-10-29"));
    await app.waitFor("Oct 29, 2026 · 14:30");
  });
});

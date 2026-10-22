// The page's answers for QuickJS' local time (`local_time.rs`) and Intl's
// default zone, in several zones:
// `node --test crates/nsh-platform-web/web/local-time.test.mjs`.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("nsh.js", import.meta.url), "utf8");
// The helpers are not exported; the page only uses them as wasm imports.
const { localOffset, timeZoneName } = await import(
  `data:text/javascript,${encodeURIComponent(`${source}\nexport { localOffset, timeZoneName };`)}`
);

/** Runs `f` with the process in time zone `zone` (node follows `TZ` as it changes). */
function inZone(zone, f) {
  const before = process.env.TZ;
  process.env.TZ = zone;
  try {
    f();
  } finally {
    if (before === undefined) delete process.env.TZ;
    else process.env.TZ = before;
  }
}

const january = Date.UTC(2026, 0, 15);
const july = Date.UTC(2026, 6, 15);

test("Shanghai is eight hours east of UTC all year", () => {
  inZone("Asia/Shanghai", () => {
    assert.equal(localOffset(january), 8 * 3600);
    assert.equal(localOffset(july), 8 * 3600);
    assert.equal(timeZoneName(), "Asia/Shanghai");
  });
});

test("New York's offset follows daylight saving time", () => {
  inZone("America/New_York", () => {
    assert.equal(localOffset(january), -5 * 3600);
    assert.equal(localOffset(july), -4 * 3600);
    assert.equal(timeZoneName(), "America/New_York");
  });
});

test("UTC is no offset, and a time the browser cannot place is UTC", () => {
  inZone("UTC", () => {
    assert.equal(localOffset(january), 0);
    assert.ok(["UTC", "Etc/UTC"].includes(timeZoneName()), timeZoneName());
  });
  inZone("Asia/Kolkata", () => {
    assert.equal(localOffset(january), 5.5 * 3600);
    assert.equal(localOffset(NaN), 0);
  });
});

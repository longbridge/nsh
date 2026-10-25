// The Data table demo: `nsh/base`'s `DataTable` over a million rows the app
// serves a page at a time, whose selection, keys, sorting and scrolling the
// tests walk through.
import { expect, open, test } from "nsh/test";

const desktop = { platform: "macos", width: 1000, height: 700 } as const;

test("the Data table demo makes only the cells in view, and selects a row", async () => {
  const app = await open("../App.tsx", desktop);
  await app.press("role:button[Data table]");
  await app.waitFor("Ada 0");
  // The first rows and columns are made; the last ones, far away, are not.
  expect(app.shows("Metric 1")).toBe(true);
  expect(app.shows("Ada 999999")).toBe(false);
  expect(app.shows("Metric 56")).toBe(false);
  await app.press("text:Alan 1");
  await app.waitFor("Row 2");
  // The arrow keys move the selection and the End key goes to the last row,
  // which the page served follows.
  await app.keyboard.press("down");
  await app.waitFor("Row 3");
  await app.keyboard.press("end");
  await app.waitFor("Row 1000000");
  await app.waitFor("Ken 999999");
  expect(app.shows("Ada 0")).toBe(false);
});

test("the Data table demo selects cells and columns", async () => {
  const app = await open("../App.tsx", desktop);
  await app.press("role:button[Data table]");
  await app.waitFor("Ada 0");
  await app.press("Cells");
  await app.press("text:Oslo >> nth=0");
  await app.waitFor("Cell row 1, City");
  await app.keyboard.press("right");
  await app.waitFor("Cell row 1, Score");
  await app.keyboard.press("down");
  await app.waitFor("Cell row 2, Score");
  // A title selects its column.
  await app.press("text:Name");
  await app.waitFor("Column Name");
});

test("the Data table demo sorts with a title's sort button and scrolls to a row", async () => {
  const app = await open("../App.tsx", desktop);
  await app.press("role:button[Data table]");
  await app.waitFor("Ada 0");
  // Ascending, then descending: the app serves the rows the other way up.
  await app.press("id:sort:id");
  await app.press("id:sort:id");
  await app.waitFor("Ken 999999");
  await app.press("label:Go to row 500000");
  await app.waitFor("text:/rows 500000–/");
});

test("the Data table demo's live quotes tick and sort", async () => {
  const app = await open("../App.tsx", desktop);
  await app.press("role:button[Data table]");
  await app.waitFor("Ada 0");
  await app.press("Live quotes");
  // Ticks come every 33 ms, colored by rise and fall.
  await app.waitFor("text:/tick [1-9]\\d/");
  // The price's sort button sorts ascending, then descending.
  await app.press("id:sort:price");
  await app.waitFor("↑");
  await app.press("id:sort:price");
  await app.waitFor("↓");
});

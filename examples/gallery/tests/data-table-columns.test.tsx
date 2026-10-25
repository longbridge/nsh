// `DataTable`'s columns: a column's own classes, and a double press on its
// resize handle that fits it to its widest text.
import { expect, open, test } from "nsh/test";

const desktop = { platform: "macos", width: 600, height: 400 } as const;

test("a double press on a resize handle fits its column", async () => {
  const app = await open("./fixtures/data-table-columns.tsx", desktop);
  await app.waitFor("Ada");
  await app.drag("id:resize:note", { x: 0, y: 0 });
  expect(app.shows("text:/widths note/")).toBe(false);
  await app.drag("id:resize:note", { x: 0, y: 0 });
  await app.waitFor("text:/widths note=\\d+/");
  expect(app.shows("widths note=80")).toBe(false);
});

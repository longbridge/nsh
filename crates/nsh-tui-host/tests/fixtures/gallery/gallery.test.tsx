import { expect, open, test } from "nsh/test";

const SIDEBAR_COLUMNS = 33;
const PHOTOS_FOCUS_STOP = 12;
const LABEL_SAMPLE_COLUMNS = 12;
type Picture = { rows: { symbol: string }[][] };

test("the official desktop gallery shows focus without drawing through its labels and opens Photos", async () => {
  const app = await open("./App.tsx", { width: 1600, height: 800 });
  await app.waitFor("A live watchlist and a stock: candles, depth, the tape, a tick every 50 ms");
  const before: Picture = JSON.parse(new TextDecoder().decode(new Uint8Array(await app.screenshot())));
  const photoRow = before.rows.findIndex(row =>
    row.slice(0, SIDEBAR_COLUMNS).map(cell => cell.symbol).join("").includes("Photos"));
  expect(photoRow >= 0).toBe(true);
  // Reading order interleaves the sidebar with the Stocks controls.
  for (let index = 0; index < PHOTOS_FOCUS_STOP; index++) await app.keyboard.press("ctrl+n");
  const focused: Picture = JSON.parse(new TextDecoder().decode(new Uint8Array(await app.screenshot())));
  await app.keyboard.press("enter");
  await app.waitFor("A long recycled list: images, pull to refresh, paging");
  expect(app.texts()).toContain("A long recycled list: images, pull to refresh, paging");
  const sidebar = (picture: Picture) => JSON.stringify(
    picture.rows.slice(photoRow - 1, photoRow + 2).map(row => row.slice(0, SIDEBAR_COLUMNS)));
  expect(sidebar(focused) !== sidebar(before)).toBe(true);
  for (let column = 0; column < SIDEBAR_COLUMNS; column++) {
    const symbol = before.rows[photoRow][column].symbol;
    if (symbol.trim()) expect(focused.rows[photoRow][column].symbol).toBe(symbol);
  }
  const rowText = (picture: Picture) => picture.rows[photoRow].map(cell => cell.symbol).join("");
  const photoColumn = rowText(before).indexOf("Photos");
  expect(rowText(focused).slice(photoColumn, photoColumn + LABEL_SAMPLE_COLUMNS))
    .toBe(rowText(before).slice(photoColumn, photoColumn + LABEL_SAMPLE_COLUMNS));
}, 60000);

test("Escape returns from a page pushed by the desktop Navigation demo", async () => {
  const app = await open("./App.tsx", { width: 1600, height: 800 });
  await app.press("label:Navigation");
  await app.waitFor("Native stack");
  await app.press("Large title");
  await app.waitFor("iOS: the title shrinks into the bar as the content scrolls.");
  await app.keyboard.press("escape");
  await app.waitFor("Native stack");
  expect(app.shows("iOS: the title shrinks into the bar as the content scrolls.")).toBe(false);
});

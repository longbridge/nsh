// The post editor, headless: the suggestions of the three triggers, the
// trend menu of a stock tag, a tag deleted whole, and the HTML going out
// and coming back.
import { expect, open, test } from "nsh/test";

const SAMPLE =
  '<p>看多 <span type="security-tag" counter_id="ST/US/NVDA" trend="up">$NVIDIA</span> ' +
  '<span class="mention" member_id="1001">@Alice</span> ' +
  '<span class="hashtag">#AI 算力</span> 的下一个季度</p>';
const EDITOR = "id:post-editor";

/** The HTML the page shows as `getHTML()`. */
function html(app: Awaited<ReturnType<typeof open>>): string {
  return app.texts().find((t) => t.startsWith("<p>")) ?? "";
}

/** Opens the page with the caret at the end of the post. */
async function start() {
  const app = await open("./App.tsx");
  await app.press("看多 $NVIDIA @Alice #AI 算力 的下一个季度");
  await app.keyboard.press("mod+End");
  return app;
}

test("the post it starts from goes out as the HTML it came in as", async () => {
  const app = await open("./App.tsx");
  expect(html(app)).toBe(SAMPLE);
  expect(app.shows("21 / 1000")).toBe(true);
});

test("$ finds a stock, and Enter makes it a tag", async () => {
  const app = await start();
  await app.type(EDITOR, " $te");
  await app.waitFor("id:suggestions");
  expect(app.shows("TSLA.US")).toBe(true);
  expect(app.shows("NVDA.US")).toBe(false);
  await app.keyboard.press("enter");
  await app.waitFor("id:suggestions", { state: "gone" });
  expect(html(app)).toContain('<span type="security-tag" counter_id="ST/US/TSLA">$Tesla</span>');
});

test("@ finds a member and # a topic, picked by the arrow keys", async () => {
  const app = await start();
  await app.type(EDITOR, " @");
  await app.waitFor("id:suggestions");
  // Alice, Bob: the second one.
  await app.keyboard.press("down");
  await app.keyboard.press("enter");
  await app.waitFor("id:suggestions", { state: "gone" });
  expect(html(app)).toContain('<span class="mention" member_id="1002">@Bob</span>');
  await app.type(EDITOR, "#降");
  await app.waitFor("降息");
  await app.keyboard.press("enter");
  await app.waitFor("id:suggestions", { state: "gone" });
  expect(html(app)).toContain('<span class="hashtag">#降息</span>');
});

test("a press on a suggestion picks it", async () => {
  const app = await start();
  await app.type(EDITOR, " $te");
  await app.waitFor("id:suggestions");
  await app.press("Tesla");
  await app.waitFor("id:suggestions", { state: "gone" });
  expect(html(app)).toContain('counter_id="ST/US/TSLA"');
});

test("Escape closes the list and typing on does not open it again", async () => {
  const app = await start();
  await app.type(EDITOR, " $");
  await app.waitFor("id:suggestions");
  await app.keyboard.press("escape");
  await app.waitFor("id:suggestions", { state: "gone" });
  await app.type(EDITOR, "te");
  expect(app.count("id:suggestions")).toBe(0);
  expect(html(app)).not.toContain("TSLA");
});

test("a press on a stock tag opens its menu: the trend is set, changed and undone", async () => {
  const app = await open("./App.tsx");
  await app.clear("label:HTML");
  await app.type("label:HTML", '<p>看多 <span type="security-tag" counter_id="ST/HK/700">$腾讯控股</span></p>');
  await app.press("label:载入 HTML");
  await app.waitFor("看多 $腾讯控股");
  expect(app.count("id:trend-menu")).toBe(0);
  // The line's end is beside the tag: a press there lands on it.
  await app.press("看多 $腾讯控股");
  await app.waitFor("id:trend-menu");
  await app.press("label:看空");
  expect(html(app)).toContain('counter_id="ST/HK/700" trend="down">$腾讯控股</span>');
  await app.press("label:看多");
  expect(html(app)).toContain('counter_id="ST/HK/700" trend="up">$腾讯控股</span>');
  await app.keyboard.press("mod+z");
  expect(html(app)).toContain('counter_id="ST/HK/700" trend="down">$腾讯控股</span>');
  await app.press("label:清除观点");
  expect(html(app)).toContain('<span type="security-tag" counter_id="ST/HK/700">$腾讯控股</span>');
});

test("Backspace deletes a tag whole", async () => {
  const app = await start();
  await app.type(EDITOR, " $te");
  await app.waitFor("id:suggestions");
  await app.keyboard.press("enter");
  await app.waitFor("id:suggestions", { state: "gone" });
  for (let i = 0; i < 3 && html(app).includes("TSLA"); i++) {
    await app.keyboard.press("backspace");
    // Never a tag with part of its text gone.
    if (html(app).includes("$Tes")) expect(html(app)).toContain("$Tesla</span>");
  }
  expect(html(app)).not.toContain("TSLA");
  expect(html(app)).toContain("的下一个季度");
});

test("HTML read back becomes the document", async () => {
  const app = await open("./App.tsx");
  const input =
    '<p><span type="security-tag" counter_id="ST/HK/700">$腾讯控股</span> 与 ' +
    '<span class="mention" member_id="1003">@老王</span> ' +
    '<span class="hashtag">#财报季</span></p>';
  await app.clear("label:HTML");
  await app.type("label:HTML", input);
  await app.press("label:载入 HTML");
  await app.waitFor("$腾讯控股 与 @老王 #财报季");
  expect(html(app)).toBe(input);
});

test("the HTML the editor wrote reads back to the same HTML", async () => {
  const app = await start();
  await app.type(EDITOR, " $te");
  await app.waitFor("id:suggestions");
  await app.keyboard.press("enter");
  await app.waitFor("id:suggestions", { state: "gone" });
  await app.type(EDITOR, "@bo");
  await app.waitFor("id:suggestions");
  await app.keyboard.press("enter");
  await app.waitFor("id:suggestions", { state: "gone" });
  await app.type(EDITOR, "#降");
  await app.waitFor("id:suggestions");
  await app.keyboard.press("enter");
  await app.waitFor("id:suggestions", { state: "gone" });
  // A trailing space is the one thing reading drops (white space collapses
  // as in a browser): the output settles after one round.
  const written = html(app);
  await app.press("label:读回上面的输出");
  const settled = html(app);
  expect(settled).toBe(written.replace(" </p>", "</p>"));
  await app.press("label:读回上面的输出");
  expect(html(app)).toBe(settled);
});

// The live stream every development host registers (`nsh/test-pattern`)
// plays in a video on each target (`nsh test --target macos | ios |
// android | windows | linux | gpui`): the first frame's size is `onLoad`,
// and the stream can be cleared and set again. Headless and in the browser
// (`--target web`) no host registers it: the videos show nothing and the
// test checks only that the name is the stream's.
import { expect, open, test } from "nsh/test";
import type { App } from "nsh/test";

/// Whether `app` runs headless, where nothing draws it (a screenshot needs a
/// real platform).
async function headless(app: App): Promise<boolean> {
  try {
    await app.screenshot();
    return false;
  } catch {
    return true;
  }
}

test("the development host's test pattern plays in a video", async () => {
  const app = await open("./fixtures/stream/App.tsx");
  await app.waitFor("Stream nsh/test-pattern live");
  // Headless and in the browser nothing registers the pattern.
  if ((await headless(app)) || app.shows("On web")) {
    expect(app.shows("First waiting")).toBe(true);
    return;
  }
  await app.waitFor("First 320×180");
  // The second video, paused, shows the stream's first frame on every
  // platform; it goes on alone once the first stops.
  await app.waitFor("Second 320×180");
  await app.press("label:Remove first");
  await app.press("label:Toggle");
  await app.waitFor("Stream nsh/test-pattern cleared");
  await app.press("label:Toggle");
  await app.waitFor("Stream nsh/test-pattern live");
}, 30_000);

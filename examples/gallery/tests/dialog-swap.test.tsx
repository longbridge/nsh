// A page sheet dialog closed and another opened in the same press: the new
// one stays open and takes presses, whichever comes first in the tree, and
// also when the old one leaves with the view it is in. Dialogs closed while
// another's dismissal runs report their close, and do not keep later ones
// from opening.
import { expect, open, test } from "nsh/test";

// Long enough for the old sheet's dismissal to end, which took the new one
// along when it was presented over the old one.
const SETTLE = 1500;

async function replace(from: string, press: string, to: string) {
  const app = await open("./fixtures/dialog-swap.tsx");
  await app.press(`text:${from}`);
  await app.waitFor(`text:${press}`);
  await app.press(`text:${press}`);
  await app.waitFor(`text:shown: ${to}`);
  await app.wait(SETTLE);
  const title = `${to[0].toUpperCase()}${to.slice(1)} sheet`;
  await app.press(`text:Count in ${title}`);
  await app.waitFor("text:count: 1");
  expect(app.texts()).toContain(`shown: ${to}`);
}

test("a dialog written after the one it replaces stays open", async () => {
  await replace("Open first", "Replace with later", "later");
});

test("a dialog written before the one it replaces stays open", async () => {
  await replace("Open first", "Replace with earlier", "earlier");
});

test("a dialog replacing one closed with its view stays open", async () => {
  await replace("Open wrapped", "Replace wrapped with later", "later");
});

// Whether the app runs headless: `requests()` is a headless run's only.
// Headless dialogs report no close (`onClose`).
function headless(app: { requests(): unknown }) {
  try {
    app.requests();
    return true;
  } catch {
    return false;
  }
}

test("a dialog closed while the one under it leaves reports its close", async () => {
  const app = await open("./fixtures/dialog-swap.tsx");
  await app.press("text:Open base");
  await app.press("text:Open top");
  await app.press("text:Close base, then top");
  await app.waitFor("text:Open base");
  await app.wait(SETTLE);
  if (!headless(app)) await app.waitFor("text:top: closed");
});

test("a dialog opens after one closed while the sheet over it leaves", async () => {
  const app = await open("./fixtures/dialog-swap.tsx");
  await app.press("text:Open outer");
  await app.press("text:Open inner");
  await app.press("text:Close inner, then outer, then open later");
  await app.waitFor("text:shown: later");
  await app.wait(SETTLE);
  await app.press("text:Count in Later sheet");
  await app.waitFor("text:count: 1");
});

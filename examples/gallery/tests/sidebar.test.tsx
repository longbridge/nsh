// The desktop layout's sidebar: its demos are one stop of Tab, as a macOS
// source list is, whose Up and Down select the demo before and after while
// it has the keyboard focus; Tab moves on to the rest of the window.
import { expect, open, test } from "nsh/test";

test("the sidebar's demos are one stop of Tab, whose Up and Down select the demos", async () => {
  const app = await open("../App.tsx", { platform: "macos", width: 1200, height: 800, clock: {} });
  await app.press("role:button[Checkbox]");
  await app.waitFor("Select all");
  // Tab on to the demos, past the page's checkboxes (the sidebar, stacked
  // above the page, comes after it among the views): Down there selects
  // Radio, the demo after Checkbox.
  let tabs = 0;
  while (!app.shows("Continue with Plus")) {
    expect(++tabs).toBeLessThan(20);
    await app.keyboard.press("tab");
    await app.keyboard.press("down");
  }
  await app.keyboard.press("down");
  await app.waitFor("Notifications");
  await app.keyboard.press("up");
  await app.waitFor("Continue with Plus");
  // One stop: one Tab leaves the demos, so Down no longer selects the next
  // one, and one Shift+Tab comes back to them.
  await app.keyboard.press("tab");
  await app.keyboard.press("down");
  await app.wait(100);
  expect(app.shows("Notifications")).toBe(false);
  await app.keyboard.press("shift+tab");
  await app.keyboard.press("up");
  await app.waitFor("Select all");
});

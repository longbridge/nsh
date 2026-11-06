import test from "node:test";
import assert from "node:assert/strict";
import { configureAPIs } from "./scope.js";

test("desktop tray and global shortcuts retain their off-platform behavior", async () => {
  const desktop = {};
  const modules = { exports: new Map([["nsh/desktop", desktop]]) };
  configureAPIs({}, modules, {}, () => true, { titleBar: () => ({ height: 0 }) });
  assert.equal(desktop.isAvailable, false);
  desktop.Tray.set({ tooltip: "Gallery", menu: [] });
  desktop.Tray.set(null);
  let pressed = false;
  const registered = desktop.GlobalShortcut.register("ctrl+alt+k", () => { pressed = true; });
  assert.ok(registered instanceof Promise);
  assert.equal(await registered, false);
  desktop.GlobalShortcut.unregister("ctrl+alt+k");
  desktop.GlobalShortcut.unregisterAll();
  assert.equal(pressed, false);
});

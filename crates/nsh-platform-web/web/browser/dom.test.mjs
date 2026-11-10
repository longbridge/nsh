import test from "node:test";
import assert from "node:assert/strict";
import { createDom } from "./dom.js";

test("WebView evaluation returns rejected promises on the browser backend", async () => {
  const values = { nodeIds: new WeakMap(), nodes: new Map() };
  createDom(() => { throw new Error("unexpected native operation"); }, values, { getOwner: () => null }, () => true, () => {});
  const web = values.makeNode(2);
  const unsupported = web.evaluateJavaScript("document.title");
  assert.ok(unsupported instanceof Promise);
  await assert.rejects(unsupported, { code: "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM" });
  await assert.rejects(web.evaluateJavaScript(42), TypeError);
  await assert.rejects(web.evaluateJavaScript.call(null, "1"), TypeError);
});

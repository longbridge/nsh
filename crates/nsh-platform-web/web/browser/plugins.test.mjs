import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { Values } from "./values.js";
import { createReactive } from "./reactive.js";
import { createModules } from "./modules.js";
import { createEnvironment } from "./environment.js";
import { createDom } from "./dom.js";
import { createComponents } from "./components.js";

const path = process.env.NSH_BROWSER_WASM;
const built = { cssVars: { light: { brand: "#2563eb" }, dark: { brand: "#93c5fd" } } };
const rose = { cssVars: { light: { brand: "#e11d48" }, dark: { brand: "#fb7185" } } };

async function fixture() {
  const module = await WebAssembly.compile(await readFile(path));
  let bridge;
  const imports = {};
  for (const entry of WebAssembly.Module.imports(module)) {
    assert.equal(entry.kind, "function");
    (imports[entry.module] ??= {})[entry.name] = () => 0;
  }
  imports.nsh_browser = { value: (...args) => bridge.value(...args) };
  const raw = (await WebAssembly.instantiate(module, imports)).exports;
  for (const name of Object.keys(raw)) if (/^nsh_(part|std|feature)_/.test(name)) raw[name]();
  const start = () => {
    const values = new Values();
    const reports = [];
    values.report = error => reports.push(error);
    bridge = values.imports(() => raw.memory);
    const wasm = values.bind(raw);
    wasm.nsh_browser_initialize();
    wasm.nsh_browser_test_plugins_start();
    let live = true;
    const invoke = (op, args) => values.boundary(
      () => values.with([args], ([id]) => values.result(wasm.nsh_browser_call(op, id))),
      () => raw.nsh_browser_trace(),
    );
    const reactive = createReactive(invoke, () => live, values);
    const dom = createDom(invoke, values, reactive, () => live, () => {});
    const components = createComponents(invoke, values, reactive, dom);
    const env = createEnvironment(reactive, invoke, dom);
    const notifications = [];
    invoke(400, [(name) => {
      notifications.push(name);
      if (name === "tokens") env.changedTokens();
    }]);
    const modules = createModules(invoke, values, () => live, () => {}, components.make);
    const close = () => {
      if (!live) return;
      wasm.nsh_browser_dispose();
      live = false;
      values.dispose();
    };
    return {
      env, reactive, notifications, reports, close, invoke,
      nsh: modules.exports.get("nsh"),
      workspace: modules.exports.get("test/browser"),
      themeOps: () => values.result(wasm.nsh_browser_test_theme_ops()).map(json => JSON.parse(json)),
      hostOverride: () => wasm.nsh_browser_test_theme_override(),
      stops: () => raw.nsh_browser_test_plugin_stops(),
      restart() { close(); return start(); },
    };
  };
  return start();
}

test("official and user plugins publish through the same actual-Wasm theme service", { skip: !path }, async () => {
  const app = await fixture();
  try {
    app.env.setup(null, JSON.stringify(built));
    app.themeOps();
    app.notifications.length = 0;
    const seen = [];
    let dispose;
    app.reactive.root(stop => {
      dispose = stop;
      app.reactive.effect(() => seen.push(app.env.api.token("brand")));
    });
    assert.deepEqual(seen, ["#2563ebff"]);

    // This is the generated module export, rather than the legacy browser opcode.
    assert.equal(app.nsh.setTheme(rose), undefined);
    assert.equal(app.env.api.token("brand"), "#e11d48ff");
    assert.deepEqual(seen, ["#2563ebff", "#e11d48ff"]);
    assert.deepEqual(app.notifications, ["tokens"]);
    assert.deepEqual(app.themeOps(), [rose]);

    // The user plugin was initialized with the official provider as its dependency.
    assert.equal(app.workspace.workspaceCalls(), 0);
    assert.equal(app.workspace.applyWorkspace(built), undefined);
    assert.equal(app.env.api.token("brand"), "#2563ebff");
    assert.equal(app.workspace.workspaceCalls(), 1);
    assert.deepEqual(app.themeOps(), [built]);
    assert.deepEqual(app.notifications, ["tokens", "tokens"]);

    app.workspace.applyWorkspace(built);
    assert.equal(app.workspace.workspaceCalls(), 2);
    assert.deepEqual(app.notifications, ["tokens", "tokens"]);
    assert.deepEqual(seen, ["#2563ebff", "#e11d48ff", "#2563ebff"]);
    dispose();
  } finally { app.close(); }
  assert.equal(app.stops(), 1);
  app.close();
  assert.equal(app.stops(), 1);
});

test("invalid theme input is atomic for generated exports and the legacy facade", { skip: !path }, async () => {
  const app = await fixture();
  try {
    app.env.setup(null, JSON.stringify(built));
    app.nsh.setTheme(rose);
    app.themeOps();
    app.notifications.length = 0;
    const bad = { cssVars: { light: { brand: "#zz" } } };
    for (const apply of [app.nsh.setTheme, app.workspace.applyWorkspace, app.env.api.setTheme]) {
      assert.throws(() => apply(bad), error => error instanceof TypeError && /setTheme/.test(error.message));
      assert.equal(app.env.api.token("brand"), "#e11d48ff");
      assert.deepEqual(app.themeOps(), []);
      assert.deepEqual(app.notifications, []);
    }
    assert.equal(app.workspace.workspaceCalls(), 0);
    for (const apply of [app.nsh.setTheme, app.env.api.setTheme]) {
      for (const bad of [3, true, "rose", []]) assert.throws(() => apply(bad), TypeError);
    }
    assert.equal(app.env.api.token("brand"), "#e11d48ff");
    assert.deepEqual(app.themeOps(), []);
    assert.deepEqual(app.notifications, []);
  } finally { app.close(); }
});

test("token reader failures reach the caller after official and user plugin state commits", { skip: !path }, async () => {
  for (const route of ["official", "user", "legacy"]) {
    const app = await fixture();
    try {
      app.env.setup(null, JSON.stringify(built));
      app.themeOps();
      app.notifications.length = 0;
      const failure = new Error(`${route} token reader failed`);
      let dispose;
      app.reactive.root(stop => {
        dispose = stop;
        app.reactive.effect(() => {
          if (app.env.api.token("brand") === "#e11d48ff") throw failure;
        });
      });
      const apply = route === "official" ? app.nsh.setTheme : route === "user" ? app.workspace.applyWorkspace : app.env.api.setTheme;
      assert.throws(() => apply(rose), error => error === failure, route);
      assert.equal(app.env.api.token("brand"), "#e11d48ff", route);
      assert.deepEqual(app.themeOps(), [rose], route);
      assert.deepEqual(app.notifications, ["tokens"], route);
      assert.equal(app.workspace.workspaceCalls(), route === "user" ? 1 : 0, route);
      assert.deepEqual(app.reports, [], route);
      dispose();
      assert.equal(apply(built), undefined, route);
      assert.equal(app.env.api.token("brand"), "#2563ebff", route);
      assert.equal(app.workspace.workspaceCalls(), route === "user" ? 2 : 0, route);
    } finally { app.close(); }
  }
});

test("reset and host overrides retain both appearances on the shared browser service", { skip: !path }, async () => {
  const app = await fixture();
  try {
    app.env.setup(null, JSON.stringify(built));
    app.workspace.applyWorkspace(rose);
    app.env.api.setColorScheme("dark");
    assert.equal(app.env.api.token("brand"), "#fb7185ff");
    app.nsh.setTheme(null);
    assert.equal(app.env.api.token("brand"), "#93c5fdff");
    app.env.api.setColorScheme("light");
    assert.equal(app.env.api.token("brand"), "#2563ebff");
    app.hostOverride();
    app.nsh.setTheme(rose);
    assert.equal(app.env.api.token("brand"), "#16a34aff");
    app.env.api.setColorScheme("dark");
    assert.equal(app.env.api.token("brand"), "#86efacff");
    app.nsh.setTheme(null);
    assert.equal(app.env.api.token("brand"), "#86efacff");
    app.env.api.setColorScheme("light");
    assert.equal(app.env.api.token("brand"), "#16a34aff");
  } finally { app.close(); }
});

test("engine restart creates new plugin state and releases the previous dependency", { skip: !path }, async () => {
  let app = await fixture();
  app.workspace.applyWorkspace(rose);
  assert.equal(app.workspace.workspaceCalls(), 1);
  assert.equal(app.stops(), 0);
  app = app.restart();
  try {
    assert.equal(app.stops(), 1);
    assert.equal(app.workspace.workspaceCalls(), 0);
    assert.equal(app.env.api.token("brand"), undefined);
    app.nsh.setTheme(null);
    assert.equal(app.env.api.token("brand"), undefined);
    assert.equal(app.workspace.workspaceCalls(), 0);
  } finally { app.close(); }
  assert.equal(app.stops(), 2);
});

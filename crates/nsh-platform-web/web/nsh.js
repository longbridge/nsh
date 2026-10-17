// The browser side of nsh's WebAssembly runtime (`nsh-platform-web`):
// loads `nsh.wasm`, runs an app bundle and mounts what the core commits as
// DOM elements. The core lays everything out, so every view is an
// absolutely positioned element at the frame it was given; this file only
// assigns frames and the CSS the Rust side made, draws text runs at their
// laid-out positions, and turns pointer, keyboard and scroll input into
// engine events.
//
//   const player = await createPlayer(container, { wasm: "nsh.wasm" });
//   player.run(await (await fetch("app.js")).text());
//
// Options: `wasm` (its URL, or a `Response` promise), `assetBase` (where
// `asset:/<path>` is served) or `assets` (a map from `<path>` to its URL,
// as `nsh package --target web` writes it), `readAsset(uri)` (the bytes
// of an asset the engine needs at once, a font: a `Uint8Array` or null),
// `chunks` / `chunkBase` (see `run`), `dark`, `overlay` (false: no error box), `onError`, `onLog`.
//
// A split bundle (`nsh build --split`) loads its chunks on demand: from
// `chunks[id]` when given (`app.<digest>.js` on a CDN), else `id` relative
// to `chunkBase` (default: the page):
//
//   player.run(code, { chunks: { "Screen.js": "/app.3f2a.js" }, chunkBase });

const ROOT_ID = 1;
const PRESSED = 1;
const HOVERED = 2;
const FOCUSED = 4;
const PRESS_EVENTS = ["onPress", "onLongPress", "onPressIn", "onPressOut"];

const KEYFRAMES = `
@keyframes nsh-spin { to { transform: rotate(360deg); } }
@keyframes nsh-ping { 75%, 100% { transform: scale(2); opacity: 0; } }
@keyframes nsh-pulse { 50% { opacity: .5; } }
@keyframes nsh-bounce {
  0%, 100% { transform: translateY(-25%); animation-timing-function: cubic-bezier(0.8,0,1,1); }
  50% { transform: none; animation-timing-function: cubic-bezier(0,0,0.2,1); }
}
.nsh-root { position: relative; overflow: hidden; -webkit-user-select: none; user-select: none;
  -webkit-font-smoothing: antialiased; -webkit-tap-highlight-color: transparent; touch-action: manipulation; }
.nsh-root * { box-sizing: border-box; }
.nsh-v { position: absolute; margin: 0; padding: 0; min-width: 0; min-height: 0; }
.nsh-scroll { overflow: auto; overscroll-behavior: contain; -webkit-overflow-scrolling: touch; }
/* Scroll indicators: a thin, translucent thumb with no track, as on phones,
   rather than the platform's scrollbar (with arrow buttons on Windows).
   Chromium ignores the pseudo-elements once scrollbar-width / scrollbar-color
   are set, so those are only for browsers without them (Firefox). */
.nsh-scroll::-webkit-scrollbar { width: 6px; height: 6px; }
.nsh-scroll::-webkit-scrollbar-track, .nsh-scroll::-webkit-scrollbar-corner { background: transparent; }
.nsh-scroll::-webkit-scrollbar-thumb { border-radius: 3px; background: rgba(128,128,128,.45); }
@supports not selector(::-webkit-scrollbar) {
  .nsh-scroll { scrollbar-width: thin; scrollbar-color: rgba(128,128,128,.45) transparent; }
}
.nsh-noindicator { scrollbar-width: none; }
.nsh-noindicator::-webkit-scrollbar { display: none; }
.nsh-run { position: absolute; white-space: pre; }
.nsh-border { position: absolute; inset: 0; pointer-events: none; }
.nsh-field { position: absolute; border: 0; outline: none; background: transparent; padding: 0; margin: 0;
  resize: none; font: inherit; color: inherit; -webkit-user-select: text; user-select: text; }
.nsh-selectable, .nsh-selectable * { -webkit-user-select: text; user-select: text; }
.nsh-selectable ::selection { background: var(--nsh-selection, Highlight); }
.nsh-header { position: absolute; left: 0; right: 0; top: 0; display: flex; align-items: center;
  justify-content: center; font: 600 17px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  border-bottom: 0.5px solid rgba(0,0,0,.15); z-index: 10; }
.nsh-header-back { position: absolute; left: 4px; top: 0; bottom: 0; display: flex; align-items: center;
  padding: 0 8px; font-weight: 400; cursor: pointer; background: none; border: 0; color: inherit; font: inherit; font-weight: 400; }
.nsh-header-right { position: absolute; right: 8px; top: 0; bottom: 0; display: flex; align-items: center; }
.nsh-screen { transition: transform .35s cubic-bezier(.2,.8,.2,1); isolation: isolate; }
.nsh-modal { transition: transform .35s cubic-bezier(.2,.8,.2,1), opacity .25s ease; z-index: 100; }
.nsh-dialog-backdrop { position: absolute; inset: 0; background: rgba(0,0,0,.35); display: flex;
  align-items: center; justify-content: center; z-index: 1000; }
.nsh-dialog { width: 270px; border-radius: 14px; background: rgba(245,245,245,.97); color: #000; text-align: center;
  font: 13px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; overflow: hidden; }
.nsh-dialog h3 { font-size: 17px; font-weight: 600; margin: 18px 16px 4px; }
.nsh-dialog p { margin: 0 16px 18px; }
.nsh-dialog button { display: block; width: 100%; border: 0; border-top: 0.5px solid rgba(0,0,0,.2); background: none;
  font: 17px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; color: #007aff; padding: 11px; cursor: pointer; }
.nsh-error { position: absolute; left: 8px; right: 8px; bottom: 8px; max-height: 45%; overflow: auto; z-index: 2000;
  background: #b42318; color: #fff; font: 12px/1.4 ui-monospace, Menlo, monospace; padding: 10px 12px;
  border-radius: 10px; white-space: pre-wrap; -webkit-user-select: text; user-select: text; }
`;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function measureContext() {
  const canvas = typeof OffscreenCanvas !== "undefined"
    ? new OffscreenCanvas(1, 1)
    : document.createElement("canvas");
  return canvas.getContext("2d");
}

/** Loads the runtime and makes a player in `container`. */
export async function createPlayer(container, options = {}) {
  const player = new Player(container, options);
  await player.load(options.wasm ?? new URL("nsh.wasm", import.meta.url));
  return player;
}

class Player {
  constructor(container, options) {
    this.options = options;
    this.assetBase = options.assetBase ?? "";
    this.container = container;
    if (!document.getElementById("nsh-style")) {
      const style = document.createElement("style");
      style.id = "nsh-style";
      style.textContent = KEYFRAMES;
      document.head.append(style);
    }
    this.root = document.createElement("div");
    this.root.className = "nsh-root";
    this.root.style.width = "100%";
    this.root.style.height = "100%";
    container.append(this.root);
    this.views = new Map();
    this.timer = 0;
    this.frame = 0;
    this.measure = measureContext();
    this.metricsCache = new Map();
    this.layoutAnimation = null;
    this.images = new Map();
    this.pointers = new Map();
    this.requests = new Map();
    this.dark = options.dark ?? matchMedia("(prefers-color-scheme: dark)").matches;
  }

  async load(url) {
    const imports = {
      env: { __rquickjs_host_now_us: () => Date.now() * 1000 },
      nsh: {
        now_ms: () => performance.now(),
        date_now_ms: () => Date.now(),
        measure_text: (fp, fl, tp, tl) => {
          this.measure.font = this.str(fp, fl);
          return this.measure.measureText(this.str(tp, tl)).width;
        },
        font_ascent: (fp, fl) => this.metrics(this.str(fp, fl))[0],
        font_descent: (fp, fl) => this.metrics(this.str(fp, fl))[1],
        register_font: (fp, fl, dp, dl, weight, italic) => {
          const family = this.str(fp, fl);
          const data = this.bytes(dp, dl).slice();
          const face = new FontFace(family, data, { weight: String(weight), style: italic ? "italic" : "normal" });
          document.fonts.add(face);
          face.load().catch(() => {});
        },
        submit: (p, l) => {
          const batch = JSON.parse(this.str(p, l));
          this.apply(batch);
        },
        request_frame: () => this.requestFrame(),
        report_error: (p, l) => this.showError(this.str(p, l)),
        log: (level, p, l) => {
          const message = this.str(p, l);
          (level <= 1 ? console.error : level === 2 ? console.warn : console.log)("[nsh]", message);
          this.options.onLog?.(level <= 1 ? "error" : level === 2 ? "warn" : "log", message);
        },
        read_asset: (p, l) => {
          const data = this.options.readAsset?.(this.str(p, l));
          if (!data) return 0;
          this.withStrings([data], (dp, dl) => this.wasm.nsh_asset_bytes(dp, dl));
          return 1;
        },
        fetch_start: (id, mp, ml, up, ul, hp, hl, bp, bl, kind, follow) =>
          this.fetchStart(id, this.str(mp, ml), this.str(up, ul), JSON.parse(this.str(hp, hl)),
            kind === 1 ? this.bytes(bp, bl).slice() : null, kind === 2, follow !== 0),
        fetch_send: (id, p, l) => this.fetchSend(id, this.bytes(p, l).slice()),
        fetch_finish: (id) => this.requests.get(id)?.finish?.(),
        fetch_pause: (id, paused) => this.fetchPause(id, paused !== 0),
        fetch_cancel: (id) => this.fetchCancel(id),
        load_chunk: (p, l) => this.loadChunk(this.str(p, l)),
      },
    };
    const response = () => (typeof url === "string" || url instanceof URL ? fetch(url) : url);
    let module;
    try {
      module = await WebAssembly.compileStreaming(response());
    } catch {
      module = await WebAssembly.compile(await (await response()).arrayBuffer());
    }
    // The compiler's dependencies link wasm-bindgen glue they never call
    // here (oxc-browserslist's dates): stubs that say so if they are.
    for (const { module: name, name: field, kind } of WebAssembly.Module.imports(module)) {
      if (kind !== "function" || imports[name]?.[field]) continue;
      imports[name] ??= {};
      imports[name][field] = () => {
        throw new Error(`nsh.wasm called ${name}.${field}, which the web runtime does not provide`);
      };
    }
    imports.__wbindgen_placeholder__ && (imports.__wbindgen_placeholder__.__wbindgen_describe = () => {});
    this.wasm = (await WebAssembly.instantiate(module, imports)).exports;
    // Relative request URLs (`fetch("/api")`) are the page's, as in the browser.
    this.withStrings([document.baseURI], (p, l) => this.wasm.nsh_set_base_url(p, l));
    new ResizeObserver(() => this.resized()).observe(this.root);
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => {
      if (this.options.dark === undefined) this.setDark(e.matches);
    });
    this.installInput();
  }

  // Memory --------------------------------------------------------------

  str(ptr, len) {
    return decoder.decode(new Uint8Array(this.wasm.memory.buffer, ptr, len));
  }

  bytes(ptr, len) {
    return new Uint8Array(this.wasm.memory.buffer, ptr, len);
  }

  /** Calls `f(ptr, len, ...)` for each string, freeing them afterwards. */
  withStrings(strings, f) {
    const passed = strings.map((s) => {
      const data = typeof s === "string" ? encoder.encode(s) : s;
      const ptr = this.wasm.nsh_alloc(data.length);
      new Uint8Array(this.wasm.memory.buffer, ptr, data.length).set(data);
      return [ptr, data.length];
    });
    try {
      return f(...passed.flat());
    } finally {
      for (const [ptr, len] of passed) this.wasm.nsh_dealloc(ptr, len);
    }
  }

  metrics(font) {
    let m = this.metricsCache.get(font);
    if (!m) {
      this.measure.font = font;
      const t = this.measure.measureText("Hg");
      m = [t.fontBoundingBoxAscent ?? 0, t.fontBoundingBoxDescent ?? 0];
      this.metricsCache.set(font, m);
    }
    return m;
  }

  // Running -------------------------------------------------------------

  /**
   * Runs `code` (a bundle from `nsh build`), replacing what ran. Options:
   * `release` (hardened, as packaged apps run; default: development
   * settings), `capabilities` (the app's `capabilities.json`),
   * `sourceMap` (JSON), and where a split bundle's chunks are: `chunks[id]`
   * (a URL), else `id` relative to `chunkBase` (default: the page).
   */
  run(code, {
    release = false,
    capabilities = "",
    sourceMap = "",
    chunks = this.options.chunks,
    chunkBase = this.options.chunkBase,
  } = {}) {
    this.stop();
    this.app = (this.app ?? 0) + 1;
    this.chunks = { map: chunks ?? {}, base: chunkBase ?? document.baseURI };
    const root = this.root;
    root.textContent = "";
    this.views.clear();
    root._id = ROOT_ID;
    root._kind = "root";
    root._events = new Set();
    root._animation = {};
    this.views.set(ROOT_ID, root);
    const width = root.clientWidth || 390;
    const height = root.clientHeight || 844;
    this.size = [width, height];
    this.call(() => this.withStrings([capabilities ?? "", sourceMap ?? ""], (cp, cl, mp, ml) =>
      this.wasm.nsh_bundle_options(release ? 1 : 0, cp, cl, mp, ml)));
    this.call(() => this.withStrings([code], (p, l) =>
      this.wasm.nsh_start(p, l, width, height, this.dark ? 1 : 0, devicePixelRatio || 1)));
  }

  /** Fetches chunk `id` of the running bundle and hands it to the engine. */
  async loadChunk(id) {
    const { map, base } = this.chunks;
    const app = this.app;
    let code;
    let ok = 1;
    try {
      const response = await fetch(new URL(map[id] ?? id, base));
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`.trim());
      code = await response.text();
    } catch (err) {
      code = `${err.message ?? err} (${map[id] ?? id})`;
      ok = 0;
    }
    // A chunk of an app that was replaced meanwhile is dropped.
    if (app !== this.app) return;
    this.call((w) => this.withStrings([id, code], (ip, il, cp, cl) => w.nsh_chunk(ip, il, cp, cl, ok)));
  }

  /**
   * Compiles one TSX module to what `run` takes, in the page (only with
   * `nsh-playground.wasm`): `{ code }` or `{ error }` (with a code frame).
   */
  compile(source, name = "App.tsx") {
    if (!this.wasm.nsh_compile) return { error: "this runtime has no compiler (load nsh-playground.wasm)" };
    const ok = this.withStrings([name, source], (np, nl, sp, sl) => this.wasm.nsh_compile(np, nl, sp, sl));
    const text = this.str(this.wasm.nsh_compiled_ptr(), this.wasm.nsh_compiled_len());
    return ok ? { code: text } : { error: text };
  }

  /** Compiles and runs `source`; the compile error, or null. */
  runSource(source, name = "App.tsx") {
    const result = this.compile(source, name);
    if (result.error) return result.error;
    this.run(result.code);
    return null;
  }

  /**
   * Applies a hot update from the dev server (a script replacing changed
   * modules) to the running app. Returns false when it could not be
   * applied in place: run the new bundle instead.
   */
  hotUpdate(code, name = "hot.js", sourceMap = "") {
    let applied = true;
    this.call(() => {
      const delay = this.withStrings([name, code, sourceMap ?? ""], (np, nl, cp, cl, mp, ml) =>
        this.wasm.nsh_hot_update(np, nl, cp, cl, mp, ml));
      applied = !this.wasm.nsh_hot_failed();
      return delay;
    });
    return applied;
  }

  stop() {
    for (const id of [...this.requests.keys()]) this.fetchCancel(id);
    clearTimeout(this.timer);
    cancelAnimationFrame(this.frame);
    this.frame = 0;
    this.wasm?.nsh_stop();
  }

  /** The engine is due again in `delay` ms (-1: only for input). */
  schedule(delay) {
    clearTimeout(this.timer);
    if (delay >= 0) this.timer = setTimeout(() => this.call((w) => w.nsh_pump()), Math.max(0, delay));
  }

  /**
   * Calls into the engine (`f(exports)` returns its next pump delay). The
   * engine is not reentrant: a call made while it runs (from a batch being
   * applied) waits until it returns.
   */
  call(f) {
    if (this.depth > 0) {
      (this.deferred ??= []).push(f);
      return;
    }
    this.depth = 1;
    try {
      this.schedule(f(this.wasm));
    } finally {
      this.depth = 0;
    }
    const deferred = this.deferred;
    if (deferred?.length) {
      this.deferred = [];
      for (const g of deferred) this.call(g);
    }
  }

  requestFrame() {
    if (this.frame) return;
    this.frame = requestAnimationFrame((now) => {
      this.frame = 0;
      this.call((w) => w.nsh_frame(now));
    });
  }

  resized() {
    const width = this.root.clientWidth;
    const height = this.root.clientHeight;
    if (!this.size || !width || (width === this.size[0] && height === this.size[1])) return;
    this.size = [width, height];
    this.call((w) => w.nsh_resize(width, height));
    for (const el of this.views.values()) if (el._kind === "modal" && el.isConnected) this.reportRoot(el, width, height);
  }

  setDark(dark) {
    this.dark = dark;
    this.call((w) => w.nsh_set_dark(dark ? 1 : 0));
  }

  event(id, name, payload = null) {
    this.call(() => this.withStrings([name, JSON.stringify(payload)], (np, nl, pp, pl) =>
      this.wasm.nsh_event(id, np, nl, pp, pl)));
  }

  showError(message) {
    console.error("[nsh]", message);
    this.options.onError?.(message);
    if (this.options.overlay === false) return;
    let box = this.root.querySelector(":scope > .nsh-error");
    if (!box) {
      box = document.createElement("div");
      box.className = "nsh-error";
      box.title = "Click to dismiss";
      box.addEventListener("click", () => box.remove());
      this.root.append(box);
    }
    box.textContent = message;
  }

  asset(uri) {
    if (!uri) return uri;
    if (uri.startsWith("asset:/")) {
      const path = uri.slice("asset:/".length).replace(/^\/+/, "");
      return this.options.assets?.[path] ?? this.assetBase + path;
    }
    return uri;
  }

  // Mounting ------------------------------------------------------------

  apply(batch) {
    for (const item of batch) {
      const handler = this[`op_${item[0]}`];
      if (handler) handler.call(this, item);
    }
    this.layoutAnimation = null;
  }

  view(id) {
    return this.views.get(id);
  }

  op_c([, id, kind]) {
    let el;
    if (kind === "canvas") {
      el = document.createElement("div");
      const canvas = document.createElement("canvas");
      canvas.style.cssText = "position:absolute;left:0;top:0;width:100%;height:100%";
      el.append(canvas);
      el._canvas = canvas;
      el._slots = [];
    } else if (kind === "web") {
      el = document.createElement("div");
      const frame = document.createElement("iframe");
      frame.style.cssText = "position:absolute;inset:0;width:100%;height:100%;border:0";
      el.append(frame);
      el._frameEl = frame;
    } else if (kind === "video") {
      el = document.createElement("div");
      const video = document.createElement("video");
      video.playsInline = true;
      video.style.cssText = "position:absolute;inset:0;width:100%;height:100%";
      el.append(video);
      el._video = video;
    } else {
      el = document.createElement("div");
    }
    el._id = id;
    el._kind = kind;
    el._events = new Set();
    el._animation = {};
    el._css = "";
    el._frame = null;
    el.className = kind === "scroll" ? "nsh-v nsh-scroll" : "nsh-v";
    if (kind === "image") {
      const img = document.createElement("img");
      img.draggable = false;
      img.style.cssText = "position:absolute;left:0;top:0;width:100%;height:100%;object-fit:cover;pointer-events:none";
      img.addEventListener("load", () => this.imageLoaded(el, img));
      img.addEventListener("error", () => this.imageFailed(el));
      el.append(img);
      el._img = img;
      el._base = "overflow:hidden;";
    }
    if (kind === "scroll") {
      el._spacer = document.createElement("div");
      el._spacer.style.cssText = "position:absolute;left:0;top:0;width:1px;height:1px;pointer-events:none;visibility:hidden";
      el.append(el._spacer);
      el._applied = 0;
      el.addEventListener("scroll", () => this.scrolled(el), { passive: true });
      el.addEventListener("scrollend", () => this.scrollEnded(el));
    }
    if (kind === "screen") el.classList.add("nsh-screen");
    if (kind === "modal") el.classList.add("nsh-modal");
    if (kind === "stack") el._base = "overflow:hidden;";
    if (this.layoutAnimation?.create) this.animateIn(el, this.layoutAnimation.create);
    this.views.set(id, el);
  }

  op_i([, parentId, childId, beforeId]) {
    const parent = this.view(parentId);
    const child = this.view(childId);
    if (!parent || !child) return;
    if (child._kind === "modal") {
      this.present(child);
      return;
    }
    const before = beforeId == null ? null : this.view(beforeId);
    if (child._kind === "header") {
      child._headerParent = parent;
      return;
    }
    parent.insertBefore(child, before && before.parentNode === parent ? before : null);
    if (parent._kind === "stack") this.stackChanged(parent, child);
  }

  op_r([, id]) {
    const el = this.view(id);
    if (!el) return;
    if (el._kind === "modal") {
      this.dismiss(el);
      return;
    }
    const parent = el.parentNode;
    el.remove();
    if (parent?._kind === "stack") this.stackChanged(parent);
  }

  op_d([, id]) {
    const el = this.view(id);
    if (!el) return;
    this.views.delete(id);
    if (el._kind === "modal") {
      this.dismiss(el);
      return;
    }
    const parent = el.parentNode;
    if (this.layoutAnimation?.delete && parent) {
      this.animateOut(el, this.layoutAnimation.delete);
    } else {
      el.remove();
    }
    if (parent?._kind === "stack") this.stackChanged(parent);
  }

  op_f([, id, x, y, w, h]) {
    const el = this.view(id);
    if (!el) return;
    const update = this.layoutAnimation?.update;
    if (update && el._frame) {
      const t = `${update.duration}ms ${update.easing} ${update.delay}ms`;
      el.style.transition = ["left", "top", "width", "height"].map((p) => `${p} ${t}`).join(",");
      clearTimeout(el._transitionTimer);
      el._transitionTimer = setTimeout(() => { el.style.transition = ""; }, update.duration + update.delay + 50);
    }
    el._frame = [x, y, w, h];
    this.placeFrame(el);
    if (el._kind === "canvas") this.drawSoon(el);
    if (el._kind === "stack") for (const screen of el.children) if (screen._kind === "screen") this.reportRoot(screen, w, h);
  }

  placeFrame(el) {
    const f = el._frame;
    if (!f) return;
    const s = el.style;
    s.left = `${f[0]}px`;
    s.top = `${f[1]}px`;
    s.width = `${f[2]}px`;
    s.height = `${f[3]}px`;
  }

  /** Visual CSS; the frame and animated properties are kept. */
  op_s([, id, css, border, tint]) {
    const el = this.view(id);
    if (!el) return;
    el._css = css;
    const transition = el.style.transition;
    el.style.cssText = (el._base ?? "") + css + (el._extra ?? "");
    if (transition) el.style.transition = transition;
    this.placeFrame(el);
    for (const [prop, value] of Object.entries(el._animation)) {
      if (prop !== "border-color" && prop !== "color") el.style.setProperty(prop, value);
    }
    if (el._sticky) el.style.translate = el._sticky;
    if (border) {
      if (!el._border) {
        el._border = document.createElement("div");
        el._border.className = "nsh-border";
      }
      el._border.style.cssText = border;
      if (el._animation["border-color"]) el._border.style.borderColor = el._animation["border-color"];
      el.append(el._border);
    } else if (el._border) {
      el._border.remove();
      el._border = null;
    }
    if (el._kind === "image") {
      el._tint = tint;
      this.applyTint(el);
    }
  }

  op_a([, id, prop, value]) {
    const el = this.view(id);
    if (!el) return;
    if (prop.startsWith("canvas:")) {
      el._slots[Number(prop.slice(7))] = isNaN(Number(value)) ? value : Number(value);
      this.drawSoon(el);
      return;
    }
    el._animation[prop] = value;
    if (prop === "border-color") {
      // Borders are drawn by an overlay.
      if (el._border) el._border.style.borderColor = value;
      return;
    }
    if (prop === "color") {
      // Each run has its own color.
      for (const span of el.querySelectorAll(":scope > .nsh-run")) span.style.color = value;
      return;
    }
    el.style.setProperty(prop, value);
  }

  op_t([, id, cx, cy, , , runs, styles, selectable, selectionColor]) {
    const el = this.view(id);
    if (!el) return;
    const frag = document.createDocumentFragment();
    for (const [x, y, text, style] of runs) {
      const span = document.createElement("span");
      span.className = "nsh-run";
      span.style.cssText = styles[style] ?? "";
      span.style.left = `${cx + x}px`;
      span.style.top = `${cy + y}px`;
      span.textContent = text;
      if (el._animation.color) span.style.color = el._animation.color;
      frag.append(span);
    }
    for (const span of [...el.querySelectorAll(":scope > .nsh-run")]) span.remove();
    el.insertBefore(frag, el.firstChild);
    el.classList.toggle("nsh-selectable", !!selectable);
    if (selectionColor) el.style.setProperty("--nsh-selection", selectionColor);
    else el.style.removeProperty("--nsh-selection");
    el.setAttribute("aria-label", runs.map((r) => r[2]).join(""));
  }

  op_e([, id, events]) {
    const el = this.view(id);
    if (!el) return;
    el._events = new Set(events);
    if (PRESS_EVENTS.some((e) => el._events.has(e))) el.style.cursor ||= "pointer";
  }

  op_ts([, id, states]) {
    const el = this.view(id);
    if (!el) return;
    el._tracked = states;
    el._states = 0;
    if (states & HOVERED && !el._hoverListeners) {
      el._hoverListeners = true;
      el.addEventListener("pointerenter", (e) => e.pointerType === "mouse" && this.setState(el, HOVERED, true));
      el.addEventListener("pointerleave", () => this.setState(el, HOVERED, false));
    }
  }

  setState(el, bit, on) {
    if (!el._tracked || !(el._tracked & bit)) return;
    const next = on ? el._states | bit : el._states & ~bit;
    if (next === el._states) return;
    el._states = next;
    this.call((w) => w.nsh_states(el._id, next));
  }

  op_vp([, id, props]) {
    const el = this.view(id);
    if (!el) return;
    el._props = props;
    if (props.label) el.setAttribute("aria-label", props.label);
    if (props.testID) el.dataset.testid = props.testID;
    if (props.role && props.role !== "none") el.setAttribute("role", props.role === "header" ? "heading" : props.role);
    const pe = props.pointerEvents;
    el.style.pointerEvents = pe === "none" || pe === "box-none" ? "none" : "";
    if (pe === "box-none") el._boxNone = true;
  }

  op_al([, id, label]) {
    const el = this.view(id);
    if (!el) return;
    if (el._props) el._props.label = label;
    if (label) el.setAttribute("aria-label", label);
    else el.removeAttribute("aria-label");
  }

  op_la([, create, update, del]) {
    this.layoutAnimation = { create, update, delete: del };
  }

  animateIn(el, t) {
    el.style.opacity = "0";
    requestAnimationFrame(() => {
      el.style.transition = `opacity ${t.duration}ms ${t.easing} ${t.delay}ms`;
      el.style.opacity = el._animation.opacity ?? "";
      setTimeout(() => { el.style.transition = ""; }, t.duration + t.delay + 50);
    });
  }

  animateOut(el, t) {
    el.style.transition = `opacity ${t.duration}ms ${t.easing} ${t.delay}ms`;
    el.style.opacity = "0";
    setTimeout(() => el.remove(), t.duration + t.delay);
  }

  op_dir([, rtl]) {
    this.root.dir = rtl ? "rtl" : "ltr";
  }

  // The app's appearance (`setColorScheme`): 0 the system's, 1 light, 2 dark.
  op_appearance([, appearance]) {
    this.appearance = appearance;
    this.root.style.colorScheme = appearance === 1 ? "light" : appearance === 2 ? "dark" : "";
  }

  /** Dark appearance as shown: the app's, else the system's. */
  get shownDark() {
    return this.appearance ? this.appearance === 2 : this.dark;
  }

  // Images

  op_img([, id, uri, mode, placeholder]) {
    const el = this.view(id);
    if (!el?._img) return;
    el._img.style.objectFit = mode;
    const src = this.asset(uri ?? placeholder ?? "");
    if (el._src === src) return;
    el._src = src;
    if (!src) {
      el._img.removeAttribute("src");
      return;
    }
    if (el._events.has("onLoadStart")) this.event(id, "onLoadStart");
    el._img.src = src;
    this.applyTint(el);
  }

  applyTint(el) {
    if (!el._img) return;
    if (el._tint && el._src) {
      el._img.style.visibility = "hidden";
      el.style.backgroundColor = el._tint;
      const mask = `url("${el._src}") center / ${el._img.style.objectFit === "fill" ? "100% 100%" : el._img.style.objectFit || "cover"} no-repeat`;
      el.style.mask = mask;
      el.style.webkitMask = mask;
    } else {
      el._img.style.visibility = "";
    }
  }

  imageLoaded(el, img) {
    if (el._events.has("onLoad")) this.event(el._id, "onLoad", { width: img.naturalWidth, height: img.naturalHeight });
    if (el._events.has("onLoadEnd")) this.event(el._id, "onLoadEnd");
  }

  imageFailed(el) {
    if (el._events.has("onError")) this.event(el._id, "onError", { message: `failed to load ${el._src}` });
    if (el._events.has("onLoadEnd")) this.event(el._id, "onLoadEnd");
  }

  // Scroll views

  op_sc([, id, horizontal, indicator]) {
    const el = this.view(id);
    if (!el) return;
    el._horizontal = horizontal;
    this.scrollOverflow(el);
    el.classList.toggle("nsh-noindicator", !indicator);
  }

  op_scs([, id, w, h]) {
    const el = this.view(id);
    if (!el?._spacer) return;
    el._content = [w, h];
    el._spacer.style.width = `${w}px`;
    el._spacer.style.height = `${h}px`;
    this.snapMarkers(el);
  }

  op_sb([, id, dx, dy]) {
    const el = this.view(id);
    if (!el) return;
    el._applied++;
    el.scrollLeft += dx;
    el.scrollTop += dy;
  }

  op_sbh([, id, behavior]) {
    const el = this.view(id);
    if (!el) return;
    el._behavior = behavior;
    this.scrollOverflow(el);
    this.snapMarkers(el);
  }

  /** Scrolls only along the view's axis, and not at all while scrolling is
   * disabled. With `overflow: auto` on both axes, a vertical view's own bar
   * narrows it below its content's laid-out width and adds a horizontal bar.
   * Kept in `_base` so that restyling (`op_s`) keeps it. */
  scrollOverflow(el) {
    const scroll = el._behavior?.enabled === false ? "hidden" : "auto";
    const [x, y] = el._horizontal ? [scroll, "hidden"] : ["hidden", scroll];
    el._base = `overflow-x:${x};overflow-y:${y};`;
    el.style.overflowX = x;
    el.style.overflowY = y;
  }

  snapMarkers(el) {
    const b = el._behavior;
    for (const m of el._markers ?? []) m.remove();
    el._markers = [];
    if (!b || !el._content || !el._frame) {
      el.style.scrollSnapType = "";
      return;
    }
    const axis = el._horizontal ? 0 : 1;
    const viewport = el._frame[2 + axis];
    const max = Math.max(0, el._content[axis] - viewport);
    let points = [];
    if (b.snapInterval > 0) for (let p = 0; p <= max + 0.5; p += b.snapInterval) points.push(p);
    else if (b.snapOffsets.length) points = b.snapOffsets;
    else if (b.paging && viewport > 0) for (let p = 0; p <= max + 0.5; p += viewport) points.push(p);
    if (!points.length) {
      el.style.scrollSnapType = "";
      return;
    }
    el.style.scrollSnapType = `${el._horizontal ? "x" : "y"} mandatory`;
    for (const p of points) {
      const m = document.createElement("div");
      m.style.cssText = `position:absolute;width:1px;height:1px;pointer-events:none;scroll-snap-align:start;${el._horizontal ? `left:${p}px;top:0` : `top:${p}px;left:0`}`;
      if (b.paging || b.snapInterval > 0) m.style.scrollSnapStop = "always";
      el.append(m);
      el._markers.push(m);
    }
  }

  op_sticky([, id, cells]) {
    const el = this.view(id);
    if (!el) return;
    el._stickyCells = cells;
    this.updateSticky(el);
  }

  updateSticky(el) {
    if (!el._stickyCells) return;
    const offset = el._horizontal ? el.scrollLeft : el.scrollTop;
    for (const [cellId, start, end] of el._stickyCells) {
      const cell = this.view(cellId);
      if (!cell) continue;
      const shift = Math.min(Math.max(offset - start, 0), end - start);
      cell._sticky = el._horizontal ? `${shift}px 0` : `0 ${shift}px`;
      cell.style.translate = cell._sticky;
      cell.style.zIndex = shift > 0 ? "5" : "";
    }
  }

  scrolled(el) {
    this.updateSticky(el);
    this.call((w) => w.nsh_scroll(el._id, el.scrollLeft, el.scrollTop, el._applied));
  }

  scrollEnded(el) {
    const payload = { x: el.scrollLeft, y: el.scrollTop };
    for (const name of ["onScrollEnd", "onMomentumScrollEnd"]) if (el._events.has(name)) this.event(el._id, name, payload);
  }

  op_ref() {}

  // Canvas

  op_cv([, id, ops]) {
    const el = this.view(id);
    if (!el) return;
    el._ops = ops;
    this.drawSoon(el);
  }

  drawSoon(el) {
    if (el._drawQueued) return;
    el._drawQueued = true;
    queueMicrotask(() => {
      el._drawQueued = false;
      this.draw(el);
    });
  }

  draw(el) {
    const canvas = el._canvas;
    if (!canvas || !el._frame || !el._ops) return;
    const ratio = devicePixelRatio || 1;
    const [, , w, h] = el._frame;
    const pw = Math.max(1, Math.round(w * ratio));
    const ph = Math.max(1, Math.round(h * ratio));
    if (canvas.width !== pw || canvas.height !== ph) {
      canvas.width = pw;
      canvas.height = ph;
    }
    const ctx = canvas.getContext("2d");
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.save();
    const slots = el._slots;
    const v = (n) => (Array.isArray(n) && n[0] === "$" ? (slots[n[1]] ?? 0) : n);
    const paint = (p) => {
      if (Array.isArray(p) && p[0] === "$") return slots[p[1]] ?? "transparent";
      if (p && typeof p === "object" && p.g) {
        const a = p.args;
        const g = p.g === "linear" ? ctx.createLinearGradient(a[0], a[1], a[2], a[3])
          : p.g === "radial" ? ctx.createRadialGradient(a[0], a[1], a[2], a[3], a[4], a[5])
            : ctx.createConicGradient(a[0], a[1], a[2]);
        for (const [offset, color] of p.stops) g.addColorStop(Math.min(1, Math.max(0, offset)), color);
        return g;
      }
      return p;
    };
    const path = (ops) => {
      const p = new Path2D();
      for (const [name, ...args] of ops) {
        if (name === "roundRect") p.roundRect(...args.slice(0, 4).map(v), args[4].map(v));
        else p[name](...args.map(v));
      }
      return p;
    };
    for (const [name, ...args] of el._ops) {
      try {
        if (name.startsWith("=")) {
          const prop = name.slice(1);
          ctx[prop] = prop.endsWith("Style") ? paint(args[0]) : v(args[0]);
        } else if (name === "fill" || name === "clip") {
          if (args[0]) ctx[name](path(args[0]), args[1]);
          else ctx[name](args[1]);
        } else if (name === "stroke") {
          if (args[0]) ctx.stroke(path(args[0]));
          else ctx.stroke();
        } else if (name === "shadowOffset") {
          ctx.shadowOffsetX = args[0];
          ctx.shadowOffsetY = args[1];
        } else if (name === "roundRect") {
          ctx.roundRect(...args.slice(0, 4).map(v), args[4].map(v));
        } else if (name === "drawImage") {
          const img = this.canvasImage(args[0], el);
          if (!img) continue;
          const dest = args.slice(2).map(v);
          if (args[1]) ctx.drawImage(img, ...args[1], ...dest);
          else ctx.drawImage(img, ...dest);
        } else if (name === "fillText" || name === "strokeText") {
          ctx[name](args[0], ...args.slice(1).map(v));
        } else if (name === "setLineDash") {
          ctx.setLineDash(args[0]);
        } else if (typeof ctx[name] === "function") {
          ctx[name](...args.map(v));
        }
      } catch (err) {
        console.warn("[nsh] canvas", name, err);
      }
    }
    ctx.restore();
  }

  canvasImage(uri, el) {
    const src = this.asset(uri);
    let entry = this.images.get(src);
    if (!entry) {
      const img = new Image();
      entry = { img, ready: false, waiting: new Set() };
      img.onload = () => {
        entry.ready = true;
        for (const w of entry.waiting) this.drawSoon(w);
      };
      img.src = src;
      this.images.set(src, entry);
    }
    if (!entry.ready) entry.waiting.add(el);
    return entry.ready ? entry.img : null;
  }

  // Gestures

  op_g([, id, pan, pinch, rotation]) {
    const el = this.view(id);
    if (el) el._gestures = { pan, pinch, rotation };
  }

  // Inputs

  op_in([, id, config]) {
    const el = this.view(id);
    if (!el) return;
    let field = el._field;
    const tag = config.multiline ? "TEXTAREA" : "INPUT";
    if (!field || field.tagName !== tag) {
      field?.remove();
      field = document.createElement(tag);
      field.className = "nsh-field";
      field._count = 0;
      this.wireInput(el, field);
      el._field = field;
      el.append(field);
      if (el._content) this.placeContent(el);
    }
    field.style.cssText = config.css + (el._content ? "" : "inset:0;");
    field.style.textAlign = config.align;
    if (config.lineHeight > 0) field.style.lineHeight = `${config.lineHeight}px`;
    if (config.placeholderColor) field.style.setProperty("--ph", config.placeholderColor);
    if (config.selectionColor) field.style.caretColor = config.selectionColor;
    field.placeholder = config.placeholder;
    if (!config.multiline) {
      field.type = config.secure ? "password"
        : config.keyboard === "email-address" ? "email"
          : config.keyboard === "url" ? "url"
            : config.keyboard === "phone-pad" ? "tel" : "text";
      field.inputMode = { "number-pad": "numeric", numeric: "decimal", "decimal-pad": "decimal", "numbers-and-punctuation": "text", "phone-pad": "tel", "email-address": "email", url: "url" }[config.keyboard] ?? "text";
    }
    // A textarea that submits shows the return key's action too.
    if (!config.multiline || config.submit !== "newline") {
      field.enterKeyHint = { done: "done", go: "go", next: "next", search: "search", send: "send" }[config.returnKey] ?? "enter";
    } else field.removeAttribute("enterkeyhint");
    field.readOnly = !config.editable;
    field.autocapitalize = config.autoCapitalize === "none" ? "off" : config.autoCapitalize;
    field.spellcheck = config.autoCorrect;
    if (config.maxLength != null) field.maxLength = config.maxLength;
    else field.removeAttribute("maxlength");
    el._config = config;
    if (config.placeholderColor && !el._phStyle) {
      el._phStyle = true;
      field.classList.add("nsh-ph");
      if (!document.getElementById("nsh-ph-style")) {
        const s = document.createElement("style");
        s.id = "nsh-ph-style";
        s.textContent = ".nsh-ph::placeholder { color: var(--ph); opacity: 1; }";
        document.head.append(s);
      }
    }
    if (config.autoFocus && !el._autoFocused) {
      el._autoFocused = true;
      setTimeout(() => field.focus(), 0);
    }
  }

  wireInput(el, field) {
    field.addEventListener("input", () => {
      field._count++;
      this.event(el._id, "onChangeText", { text: field.value, count: field._count });
    });
    field.addEventListener("keydown", (e) => {
      if (el._events.has("onKeyPress")) {
        const key = e.key === "Backspace" || e.key === "Enter" ? e.key : e.key.length === 1 ? e.key : null;
        if (key) this.event(el._id, "onKeyPress", { key });
      }
      // Return submits unless it inserts a line break (`submitBehavior`);
      // Shift / Alt+Return always do in a textarea, and a return that
      // commits an input method's composition never submits.
      const submit = el._config?.submit ?? "blurAndSubmit";
      const lineBreak = field.tagName === "TEXTAREA" && (submit === "newline" || e.shiftKey || e.altKey);
      if (e.key === "Enter" && !lineBreak && !e.isComposing && e.keyCode !== 229) {
        e.preventDefault();
        this.event(el._id, "onSubmitEditing", { text: field.value, count: field._count });
        if (submit === "blurAndSubmit") field.blur();
      }
    });
    field.addEventListener("focus", () => {
      this.setState(el, FOCUSED, true);
      this.event(el._id, "onFocus");
    });
    field.addEventListener("blur", () => {
      this.setState(el, FOCUSED, false);
      this.event(el._id, "onBlur");
    });
    field.addEventListener("select", () => this.selectionChanged(el, field));
    field.addEventListener("keyup", () => this.selectionChanged(el, field));
    field.addEventListener("pointerup", () => this.selectionChanged(el, field));
  }

  selectionChanged(el, field) {
    if (!el._events.has("onSelectionChange")) return;
    const sel = [field.selectionStart ?? 0, field.selectionEnd ?? 0];
    if (el._selection && el._selection[0] === sel[0] && el._selection[1] === sel[1]) return;
    el._selection = sel;
    this.event(el._id, "onSelectionChange", { start: sel[0], end: sel[1] });
  }

  op_iv([, id, text, count]) {
    const field = this.view(id)?._field;
    if (!field || count < field._count) return;
    if (field.value !== text) field.value = text;
  }

  op_ic([, id, x, y, w, h]) {
    const el = this.view(id);
    if (!el) return;
    el._content = [x, y, w, h];
    this.placeContent(el);
  }

  placeContent(el) {
    const field = el._field;
    if (!field || !el._content) return;
    const [x, y, w, h] = el._content;
    field.style.inset = "";
    field.style.left = `${x}px`;
    field.style.top = `${y}px`;
    field.style.width = `${w}px`;
    field.style.height = `${h}px`;
  }

  // Navigation

  op_scr([, id, title, shown, height, background, titleColor, tint, backVisible, preventBack, left, right, titleView]) {
    const el = this.view(id);
    if (!el) return;
    el._screen = { title, shown, height, background, titleColor, tint, backVisible, preventBack, left, right, titleView };
    this.renderHeader(el);
  }

  renderHeader(screen) {
    const c = screen._screen;
    if (!c) return;
    let header = screen._header;
    if (!c.shown || c.height <= 0) {
      header?.remove();
      screen._header = null;
      return;
    }
    if (!header) {
      header = document.createElement("div");
      header.className = "nsh-header";
      screen._header = header;
    }
    header.textContent = "";
    header.style.height = `${c.height}px`;
    header.style.background = c.background ?? (this.shownDark ? "#1c1c1e" : "#f9f9f9");
    header.style.color = c.titleColor ?? (this.shownDark ? "#fff" : "#000");
    const title = document.createElement("span");
    title.textContent = c.title ?? "";
    const titleView = c.titleView != null ? this.view(c.titleView) : null;
    if (titleView) {
      titleView.style.position = "relative";
      header.append(titleView);
    } else {
      header.append(title);
    }
    const stack = screen.parentNode;
    const screens = stack ? [...stack.children].filter((s) => s._kind === "screen") : [];
    const index = screens.indexOf(screen);
    const leftView = c.left != null ? this.view(c.left) : null;
    if (leftView) {
      const slot = document.createElement("div");
      slot.className = "nsh-header-back";
      leftView.style.position = "relative";
      slot.append(leftView);
      header.append(slot);
    } else if (index > 0 && c.backVisible) {
      const back = document.createElement("button");
      back.className = "nsh-header-back";
      back.style.color = c.tint ?? "#007aff";
      back.textContent = "‹ Back";
      back.addEventListener("click", () => this.goBack(screen));
      header.append(back);
    }
    const rightView = c.right != null ? this.view(c.right) : null;
    if (rightView) {
      const slot = document.createElement("div");
      slot.className = "nsh-header-right";
      rightView.style.position = "relative";
      slot.append(rightView);
      header.append(slot);
    }
    screen.append(header);
  }

  goBack(screen) {
    if (screen._screen?.preventBack) {
      this.event(screen._id, "onBackRequest");
      return;
    }
    const stack = screen.parentNode;
    const screens = stack ? [...stack.children].filter((s) => s._kind === "screen") : [];
    const below = screens[screens.indexOf(screen) - 1];
    if (below) {
      clearTimeout(stack._hideTimer);
      below.style.visibility = "";
      below.style.transform = "";
    }
    screen.style.transform = "translateX(100%)";
    setTimeout(() => this.event(screen._id, "onDismiss"), 300);
  }

  stackChanged(stack, added) {
    const screens = [...stack.children].filter((s) => s._kind === "screen");
    clearTimeout(stack._hideTimer);
    // The screen under the top one shows while the top one slides.
    stack._hideTimer = setTimeout(() => {
      screens.forEach((screen, i) => {
        if (i < screens.length - 1 && screen.parentNode === stack) screen.style.visibility = "hidden";
      });
    }, 400);
    screens.forEach((screen, i) => {
      const top = i === screens.length - 1;
      screen.style.visibility = top || i === screens.length - 2 ? "" : "hidden";
      if (screen === added && i > 0) {
        screen.style.transform = "translateX(100%)";
        requestAnimationFrame(() => requestAnimationFrame(() => { screen.style.transform = ""; }));
      } else {
        screen.style.transform = top ? "" : "translateX(-30%)";
      }
      this.renderHeader(screen);
    });
    if (added && stack._frame) this.reportRoot(added, stack._frame[2], stack._frame[3]);
  }

  reportRoot(el, width, height) {
    this.call((w) => w.nsh_root_size(el._id, width, height, 0, 0, 0, 0));
  }

  // Modals

  op_mc([, id, animation, transparent]) {
    const el = this.view(id);
    if (!el) return;
    el._modal = { animation, transparent };
    el.style.background = transparent ? "" : (this.shownDark ? "#000" : "#fff");
  }

  present(el) {
    const [w, h] = this.size;
    el.style.left = "0";
    el.style.top = "0";
    el.style.width = `${w}px`;
    el.style.height = `${h}px`;
    const animation = el._modal?.animation ?? "slide";
    el.style.transition = "none";
    el.style.transform = animation === "slide" ? "translateY(100%)" : "";
    el.style.opacity = animation === "fade" ? "0" : "";
    this.root.append(el);
    this.reportRoot(el, w, h);
    requestAnimationFrame(() => requestAnimationFrame(() => {
      el.style.transition = "";
      el.style.transform = "";
      el.style.opacity = "";
      setTimeout(() => this.event(el._id, "onShow"), animation === "none" ? 0 : 350);
    }));
  }

  dismiss(el) {
    const animation = el._modal?.animation ?? "slide";
    if (animation === "slide") el.style.transform = "translateY(100%)";
    if (animation === "fade") el.style.opacity = "0";
    setTimeout(() => {
      el.remove();
      this.event(el._id, "onDismiss");
    }, animation === "none" ? 0 : 350);
  }

  // Web views and videos

  op_web([, id, url, html]) {
    const frame = this.view(id)?._frameEl;
    if (!frame) return;
    if (html != null) frame.srcdoc = html;
    else if (url && frame.src !== url) frame.src = url;
  }

  op_video([, id, src, paused, muted, volume, rate, repeat, fit, controls]) {
    const el = this.view(id);
    const video = el?._video;
    if (!video) return;
    if (src && video.src !== this.asset(src)) video.src = this.asset(src);
    video.muted = muted;
    video.volume = Math.min(1, Math.max(0, volume));
    video.playbackRate = rate;
    video.loop = repeat;
    video.controls = controls;
    video.style.objectFit = fit;
    if (paused) video.pause();
    else video.play().catch(() => {});
  }

  // Commands

  op_cmd([, id, name, ...args]) {
    const el = this.view(id);
    switch (name) {
      case "focus": el?._field?.focus(); break;
      case "blur":
        if (el?._field) el._field.blur();
        else document.activeElement?.blur?.();
        break;
      case "setSelection": el?._field?.setSelectionRange(args[0], args[1]); break;
      case "scrollTo":
        if (!el) break;
        el._applied++;
        el.scrollTo({ left: args[0], top: args[1], behavior: args[2] ? "smooth" : "instant" });
        break;
      case "scrollToEnd":
        if (!el) break;
        el._applied++;
        el.scrollTo({ left: el._horizontal ? el.scrollWidth : 0, top: el._horizontal ? 0 : el.scrollHeight, behavior: args[0] ? "smooth" : "instant" });
        break;
      case "dialog": this.dialog(args[0], args[1], args[2], args[3]); break;
      case "reload": el?._frameEl?.contentWindow?.location.reload(); break;
      case "seek": if (el?._video) el._video.currentTime = args[0]; break;
      case "setClipboard": navigator.clipboard?.writeText(args[0]).catch(() => {}); break;
      case "readClipboard": this.readClipboard(args[0], args[1]); break;
      case "imageInfo": this.imageInfo(args[0], args[1]); break;
      default: break;
    }
  }

  /** `Clipboard.getString` / `hasString`: browsers only tell by reading (which may ask the user). */
  async readClipboard(request, hasOnly) {
    let text = "";
    try {
      text = (await navigator.clipboard?.readText()) ?? "";
    } catch {
      // Denied, or no clipboard access (an insecure context).
    }
    const payload = { id: request, has: text.length > 0 };
    if (!hasOnly && text) payload.text = text;
    this.event(ROOT_ID, "onClipboard", payload);
  }

  /** `Image.getSize` / `prefetch`: loads the image (the browser keeps it cached for the views that show it). */
  imageInfo(request, uri) {
    const img = new Image();
    // Answers for an app that was replaced meanwhile are dropped.
    const app = this.app;
    const answer = (payload) => app === this.app && this.event(ROOT_ID, "onImageInfo", { id: request, ...payload });
    img.onload = () => answer({ width: img.naturalWidth, height: img.naturalHeight });
    img.onerror = () => answer({ error: `failed to load ${uri}` });
    img.src = this.asset(uri);
  }

  // Network -------------------------------------------------------------

  /**
   * Request `id` from the engine's `fetch` (`net.rs`): made with the
   * browser's `fetch`, its head and body chunks reported back as they come.
   * A streamed body (`streamed`) is collected until `fetch_finish`.
   */
  fetchStart(id, method, url, headers, body, streamed, follow) {
    const request = { controller: new AbortController(), paused: false, resume: null, reader: null, done: false };
    this.requests.set(id, request);
    const report = (f) => {
      if (this.requests.get(id) === request) this.call(f);
    };
    const fail = (message) => {
      report((w) => this.withStrings([message], (p, l) => w.nsh_fetch_failed(id, p, l)));
      this.requests.delete(id);
    };
    const send = async (body) => {
      const init = { method, headers: new Headers(), body, redirect: follow ? "follow" : "manual", signal: request.controller.signal };
      for (const [name, value] of headers) {
        try {
          init.headers.append(name, value);
        } catch {
          // A name or value the browser refuses.
        }
      }
      let response;
      try {
        response = await fetch(url, init);
      } catch (e) {
        if (request.controller.signal.aborted) return;
        fail(`fetch ${method} ${url} failed: ${e?.message ?? e} (a network error, or a cross-origin server that does not allow this page's origin)`);
        return;
      }
      if (response.type === "opaqueredirect") {
        // Browsers hide a redirect that is not followed: neither its status
        // nor its Location can be delivered (`redirect: "manual"`) or
        // checked against the app's capabilities before the next hop.
        fail(`fetch ${method} ${url} was redirected: on the web a redirect is only followed by the browser, not delivered or checked hop by hop`);
        return;
      }
      const list = [];
      response.headers.forEach((value, name) => list.push([name, value]));
      report((w) => this.withStrings([response.url || url, response.statusText, JSON.stringify(list)], (up, ul, tp, tl, hp, hl) =>
        w.nsh_fetch_head(id, response.status, up, ul, tp, tl, hp, hl, response.redirected ? 1 : 0)));
      if (!response.body) {
        report((w) => w.nsh_fetch_end(id));
        this.requests.delete(id);
        return;
      }
      request.reader = response.body.getReader();
      try {
        for (;;) {
          while (request.paused) await new Promise((resolve) => (request.resume = resolve));
          if (this.requests.get(id) !== request) return;
          const { done, value } = await request.reader.read();
          if (done) break;
          report((w) => this.withStrings([value], (p, l) => w.nsh_fetch_data(id, p, l)));
        }
      } catch (e) {
        if (!request.controller.signal.aborted) fail(`fetch ${method} ${url}: the body broke off: ${e?.message ?? e}`);
        return;
      }
      report((w) => w.nsh_fetch_end(id));
      this.requests.delete(id);
    };
    if (streamed) {
      // Chromium streams uploads only over HTTP/2, other browsers not at
      // all: the chunks are collected and sent as one body.
      request.chunks = [];
      request.wanted = () => report((w) => w.nsh_fetch_body_wanted(id));
      request.finish = () => {
        const chunks = request.chunks;
        request.chunks = null;
        if (chunks) send(new Blob(chunks));
      };
      request.wanted();
    } else {
      send(body ?? undefined);
    }
  }

  fetchSend(id, chunk) {
    const request = this.requests.get(id);
    if (!request?.chunks) return;
    request.chunks.push(chunk);
    request.wanted();
  }

  fetchPause(id, paused) {
    const request = this.requests.get(id);
    if (!request) return;
    request.paused = paused;
    if (!paused && request.resume) {
      request.resume();
      request.resume = null;
    }
  }

  fetchCancel(id) {
    const request = this.requests.get(id);
    if (!request) return;
    this.requests.delete(id);
    request.controller.abort();
    request.resume?.();
  }

  dialog(dialogId, title, message, buttons) {
    const backdrop = document.createElement("div");
    backdrop.className = "nsh-dialog-backdrop";
    const box = document.createElement("div");
    box.className = "nsh-dialog";
    if (title) {
      const h = document.createElement("h3");
      h.textContent = title;
      box.append(h);
    }
    if (message) {
      const p = document.createElement("p");
      p.textContent = message;
      box.append(p);
    }
    const close = (index) => {
      backdrop.remove();
      this.event(ROOT_ID, "onDialog", { id: dialogId, index });
    };
    (buttons.length ? buttons : ["OK"]).forEach((text, index) => {
      const b = document.createElement("button");
      b.textContent = text;
      b.addEventListener("click", () => close(index));
      box.append(b);
    });
    backdrop.addEventListener("click", (e) => e.target === backdrop && close(-1));
    backdrop.append(box);
    this.root.append(backdrop);
  }

  op_comp() {}

  // Input ---------------------------------------------------------------

  /** The view element an event target is in. */
  viewOf(target) {
    let el = target;
    while (el && el !== this.root && el._id === undefined) el = el.parentNode;
    return el && el._id !== undefined ? el : null;
  }

  ancestors(el) {
    const out = [];
    for (let e = el; e && e !== this.root.parentNode; e = e.parentNode) if (e._id !== undefined) out.push(e);
    return out;
  }

  installInput() {
    const root = this.root;
    root.addEventListener("pointerdown", (e) => this.pointerDown(e));
    window.addEventListener("pointermove", (e) => this.pointerMove(e), { passive: false });
    window.addEventListener("pointerup", (e) => this.pointerUp(e, false));
    window.addEventListener("pointercancel", (e) => this.pointerUp(e, true));
    // A drag that scrolls must not also select or drag images.
    root.addEventListener("dragstart", (e) => e.preventDefault());
  }

  point(e) {
    const r = this.root.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  }

  touchEvent(phase, changedId, activated) {
    const points = [];
    for (const [id, p] of this.pointers) {
      if (!p.touchPath.length) continue;
      const target = p.touchPath[0];
      const rect = this.view(target)?.getBoundingClientRect();
      const r = this.root.getBoundingClientRect();
      points.push({
        id, target, path: p.touchPath,
        pageX: p.x, pageY: p.y,
        x: rect ? p.x + r.left - rect.left : p.x,
        y: rect ? p.y + r.top - rect.top : p.y,
        changed: id === changedId,
      });
    }
    if (!points.length) return;
    const json = JSON.stringify({ phase, time: performance.now(), points, activated });
    this.call(() => this.withStrings([json], (p, l) => this.wasm.nsh_touch(p, l)));
  }

  pointerDown(e) {
    if (e.button > 0) return;
    const target = this.viewOf(e.target);
    if (!target) return;
    const isField = e.target.classList?.contains("nsh-field");
    const chain = this.ancestors(target);
    const [x, y] = this.point(e);
    const p = {
      x, y, startX: x, startY: y, t: performance.now(), vx: 0, vy: 0,
      touchPath: chain.filter((v) => v._events.has("onTouch")).map((v) => v._id),
      press: isField ? null : chain.find((v) => PRESS_EVENTS.some((n) => v._events.has(n)) && !v._props?.disabled) ?? null,
      pan: chain.find((v) => v._gestures?.pan || v._events.has("onPanStart") || v._events.has("onPanEnd")) ?? null,
      scroll: e.pointerType === "mouse" && !isField ? chain.find((v) => v._kind === "scroll") ?? null : null,
      pinch: chain.find((v) => v._gestures?.pinch || v._gestures?.rotation) ?? null,
      // `panActivation`: decided here as the pointer moves (PanActivation::decide).
      gate: chain.find((v) => v._props?.panActivation) ?? null,
      gateState: 0,
      states: chain.filter((v) => v._tracked & PRESSED),
      mode: null,
    };
    this.pointers.set(e.pointerId, p);
    if (p.touchPath.length) this.touchEvent(0, e.pointerId);
    if (this.pointers.size === 2) this.startTwoFinger();
    if (p.press) this.pressIn(p);
    for (const v of p.states) this.setState(v, PRESSED, true);
    if (p.scroll || p.pan) e.preventDefault();
  }

  pressIn(p) {
    const el = p.press;
    p.pressed = true;
    const opacity = el._props?.activeOpacity;
    if (opacity != null && opacity < 1) {
      el.style.transition = "opacity .05s";
      el.style.opacity = String(opacity * Number(el._animation.opacity ?? getComputedStyle(el).opacity ?? 1));
    }
    if (el._events.has("onPressIn")) this.event(el._id, "onPressIn");
    if (el._events.has("onLongPress")) {
      p.longTimer = setTimeout(() => {
        p.long = true;
        this.event(el._id, "onLongPress");
      }, el._props?.longPressDelay ?? 500);
    }
  }

  pressOut(p, inside) {
    const el = p.press;
    if (!el || !p.pressed) return;
    p.pressed = false;
    clearTimeout(p.longTimer);
    if (el._props?.activeOpacity != null) {
      el.style.transition = "opacity .2s";
      el.style.opacity = el._animation.opacity ?? "";
    }
    if (el._events.has("onPressOut")) this.event(el._id, "onPressOut");
    if (inside && !p.long && el._events.has("onPress")) this.event(el._id, "onPress");
  }

  pointerMove(e) {
    const p = this.pointers.get(e.pointerId);
    if (!p) return;
    const [x, y] = this.point(e);
    const now = performance.now();
    const dt = Math.max(1, now - p.t);
    p.vx = ((x - p.x) / dt) * 1000;
    p.vy = ((y - p.y) / dt) * 1000;
    const dx = x - p.lastX0 || 0;
    p.lastX0 = x;
    p.x = x;
    p.y = y;
    p.t = now;
    const tx = x - p.startX;
    const ty = y - p.startY;
    const activated = this.decideGate(p, tx, ty);
    if (p.touchPath.length) this.touchEvent(1, e.pointerId, activated);
    if (this.twoFinger) {
      this.moveTwoFinger();
      return;
    }
    // A scroll view waits while the gate's pan may still activate, and
    // never gets an activated one.
    const gated = p.gate && p.gateState !== 2;
    if (gated && p.gateState === 1) e.preventDefault();
    if (!p.mode && Math.hypot(tx, ty) > 6) {
      if (p.pan) {
        p.mode = "pan";
        this.call((w) => w.nsh_pan(p.pan._id, 0, 0, 0, 0, 0));
      } else if (p.scroll && !gated) {
        p.mode = "scroll";
        p.scrollStart = [p.scroll.scrollLeft, p.scroll.scrollTop];
        if (p.scroll._events.has("onScrollBeginDrag")) this.event(p.scroll._id, "onScrollBeginDrag", { x: p.scroll.scrollLeft, y: p.scroll.scrollTop });
      }
      if (p.mode || Math.hypot(tx, ty) > 10) {
        this.pressOut(p, false);
        for (const v of p.states) this.setState(v, PRESSED, false);
      }
    }
    if (p.mode === "pan") {
      e.preventDefault();
      this.call((w) => w.nsh_pan(p.pan._id, 1, tx, ty, p.vx, p.vy));
    } else if (p.mode === "scroll") {
      e.preventDefault();
      const s = p.scroll;
      s.style.scrollSnapType = "none";
      s.scrollLeft = p.scrollStart[0] - (s._horizontal ? tx : 0);
      s.scrollTop = p.scrollStart[1] - (s._horizontal ? 0 : ty);
    }
    void dx;
  }

  /** The gate's node id when its pan activates with this move (else undefined). */
  decideGate(p, dx, dy) {
    if (!p.gate || p.gateState !== 0) return undefined;
    const [ax, ay, fx, fy] = p.gate._props.panActivation;
    const past = (limit, d) => limit != null && Math.abs(d) > limit;
    if (past(fx, dx) || past(fy, dy)) {
      p.gateState = 2;
      return undefined;
    }
    const active = ax == null && ay == null ? Math.hypot(dx, dy) > 10 : past(ax, dx) || past(ay, dy);
    if (!active) return undefined;
    p.gateState = 1;
    this.pressOut(p, false);
    return p.gate._id;
  }

  pointerUp(e, cancelled) {
    const p = this.pointers.get(e.pointerId);
    if (!p) return;
    if (p.touchPath.length) this.touchEvent(cancelled ? 3 : 2, e.pointerId);
    this.pointers.delete(e.pointerId);
    if (this.twoFinger) {
      this.endTwoFinger();
      return;
    }
    for (const v of p.states) this.setState(v, PRESSED, false);
    if (p.mode === "pan") {
      this.call((w) => w.nsh_pan(p.pan._id, 2, p.x - p.startX, p.y - p.startY, p.vx, p.vy));
    } else if (p.mode === "scroll") {
      const s = p.scroll;
      if (s._events.has("onScrollEndDrag")) this.event(s._id, "onScrollEndDrag", { x: s.scrollLeft, y: s.scrollTop });
      this.glide(s, s._horizontal ? -p.vx : -p.vy);
    } else if (p.press) {
      const rect = p.press.getBoundingClientRect();
      const inside = e.clientX >= rect.left - 8 && e.clientX <= rect.right + 8 && e.clientY >= rect.top - 8 && e.clientY <= rect.bottom + 8;
      this.pressOut(p, inside && !cancelled);
    }
  }

  /** Momentum after a mouse drag (touch scrolling is the browser's). */
  glide(s, velocity) {
    let v = Math.abs(velocity) > 80 ? velocity / 1000 : 0;
    let last = performance.now();
    const restoreSnap = () => this.snapMarkers(s);
    if (!v) {
      restoreSnap();
      return;
    }
    const step = (now) => {
      const dt = now - last;
      last = now;
      if (s._horizontal) s.scrollLeft += v * dt;
      else s.scrollTop += v * dt;
      v *= Math.pow(0.995, dt);
      if (Math.abs(v) > 0.02) requestAnimationFrame(step);
      else restoreSnap();
    };
    requestAnimationFrame(step);
  }

  startTwoFinger() {
    const [a, b] = [...this.pointers.values()];
    const el = a.pinch ?? b.pinch;
    if (!el) return;
    for (const p of [a, b]) this.pressOut(p, false);
    this.twoFinger = { el, dist: Math.hypot(a.x - b.x, a.y - b.y), angle: Math.atan2(b.y - a.y, b.x - a.x) };
    if (el._gestures?.pinch) this.call((w) => w.nsh_transform_gesture(el._id, 0, 0, 1));
    if (el._gestures?.rotation) this.call((w) => w.nsh_transform_gesture(el._id, 1, 0, 0));
  }

  twoFingerAmounts() {
    const [a, b] = [...this.pointers.values()];
    const t = this.twoFinger;
    if (!a || !b) return [t.scale ?? 1, t.rotation ?? 0];
    const scale = Math.hypot(a.x - b.x, a.y - b.y) / Math.max(1, t.dist);
    const rotation = ((Math.atan2(b.y - a.y, b.x - a.x) - t.angle) * 180) / Math.PI;
    t.scale = scale;
    t.rotation = rotation;
    return [scale, rotation];
  }

  moveTwoFinger() {
    const t = this.twoFinger;
    const [scale, rotation] = this.twoFingerAmounts();
    if (t.el._gestures?.pinch) this.call((w) => w.nsh_transform_gesture(t.el._id, 0, 1, scale));
    if (t.el._gestures?.rotation) this.call((w) => w.nsh_transform_gesture(t.el._id, 1, 1, rotation));
  }

  endTwoFinger() {
    const t = this.twoFinger;
    this.twoFinger = null;
    const scale = t.scale ?? 1;
    const rotation = t.rotation ?? 0;
    if (t.el._gestures?.pinch) this.call((w) => w.nsh_transform_gesture(t.el._id, 0, 2, scale));
    if (t.el._gestures?.rotation) this.call((w) => w.nsh_transform_gesture(t.el._id, 1, 2, rotation));
    this.pointers.clear();
  }
}

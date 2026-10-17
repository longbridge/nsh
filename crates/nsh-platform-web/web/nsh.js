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
// `chunks` / `chunkBase` (see `run`), `prepareChunk(id)` (a promise for
// what chunk `id` needs once it runs, such as fonts for `readAsset`;
// awaited with the chunk's download), `dark`, `idiom` ("phone", "pad" or
// "desktop" instead of the one the pointer and the viewport suggest),
// `overlay` (false: no error box), `onError`, `onLog`.
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
/** `accessibilityRole`s whose ARIA role has another name; others are the same. */
const ARIA_ROLES = {
  none: null, header: "heading", image: "img", imagebutton: "button", keyboardkey: "button", adjustable: "slider",
  search: "searchbox", text: null, summary: "status", togglebutton: "button", dropdownlist: "combobox",
};

/** How far a refreshable scroll view is pulled to refresh, and the room its spinner takes while refreshing (as on Linux and macOS). */
const REFRESH_PULL = 64;
const REFRESH_SPACE = 56;

const KEYFRAMES = `
@keyframes nsh-spin { to { transform: rotate(360deg); } }
@keyframes nsh-ping { 75%, 100% { transform: scale(2); opacity: 0; } }
@keyframes nsh-pulse { 50% { opacity: .5; } }
@keyframes nsh-bounce {
  0%, 100% { transform: translateY(-25%); animation-timing-function: cubic-bezier(0.8,0,1,1); }
  50% { transform: none; animation-timing-function: cubic-bezier(0,0,0.2,1); }
}
/* Touches pan only scroll views (the page neither scrolls, zooms nor
   refreshes under the app, and pans and presses get no pointercancel);
   overflow is clipped, never hidden, so that the browser cannot scroll a
   box the core laid out to show a focused field. */
.nsh-root { position: relative; width: 100%; height: 100%; overflow: clip; -webkit-user-select: none; user-select: none;
  -webkit-font-smoothing: antialiased; -webkit-tap-highlight-color: transparent; -webkit-touch-callout: none;
  touch-action: none; }
.nsh-root * { box-sizing: border-box; }
.nsh-v { position: absolute; margin: 0; padding: 0; min-width: 0; min-height: 0; }
.nsh-v:focus { outline: none; }
.nsh-v:focus-visible { outline: 2px solid var(--nsh-focus-ring, #0a84ff); outline-offset: -2px; }
.nsh-scroll { overflow: auto; overscroll-behavior: contain; -webkit-overflow-scrolling: touch; }
.nsh-scroll { touch-action: pan-x pan-y; }
/* In an iframe, swipes the app does not take scroll the embedding page. */
.nsh-root.nsh-embedded { touch-action: pan-x pan-y; }
/* Views with native pans: none, or the other axis's scrolling. */
.nsh-pan { touch-action: none; }
.nsh-pan-x { touch-action: pan-y; }
.nsh-pan-y { touch-action: pan-x; }
.nsh-live { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
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
.nsh-selectable, .nsh-selectable * { -webkit-user-select: text; user-select: text; -webkit-touch-callout: default; }
.nsh-selectable ::selection { background: var(--nsh-selection, Highlight); }
/* With the desktop idiom text is selectable as on a web page unless it is
   set off (selectable={false}) or inside something that takes presses
   or touches (nsh-press). */
.nsh-desktop .nsh-autoselect, .nsh-desktop .nsh-autoselect * { -webkit-user-select: text; user-select: text; }
.nsh-desktop .nsh-press .nsh-autoselect, .nsh-desktop .nsh-press .nsh-autoselect *,
.nsh-desktop .nsh-autoselect.nsh-press, .nsh-desktop .nsh-autoselect.nsh-press * { -webkit-user-select: none; user-select: none; }
.nsh-autoselect ::selection { background: var(--nsh-selection, Highlight); }
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
  font: 13px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; overflow: clip; }
.nsh-dialog h3 { font-size: 17px; font-weight: 600; margin: 18px 16px 4px; }
.nsh-dialog p { margin: 0 16px 18px; }
.nsh-dialog button { display: block; width: 100%; border: 0; border-top: 0.5px solid rgba(0,0,0,.2); background: none;
  font: 17px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; color: #007aff; padding: 11px; cursor: pointer; }
/* Dialogs on desktop browsers: the in-surface cards of the Linux and macOS backends. */
.nsh-desktop .nsh-dialog-backdrop { background: rgba(0,0,0,.4); }
.nsh-desktop .nsh-dialog { width: min(340px, calc(100% - 64px)); border-radius: 12px; padding: 20px; text-align: left;
  background: #fff; color: #0a0a0a; border: 1px solid #e5e5e5; box-shadow: 0 10px 30px rgba(0,0,0,.25);
  font: 14px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }
.nsh-desktop .nsh-dialog h3 { margin: 0 0 6px; font-size: 17px; }
.nsh-desktop .nsh-dialog p { margin: 0; font-size: 14px; color: #737373; overflow-wrap: anywhere; }
.nsh-desktop .nsh-dialog-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
.nsh-desktop .nsh-dialog-actions.column { flex-direction: column; }
.nsh-desktop .nsh-dialog button { display: block; width: auto; padding: 7px 16px; border-radius: 8px; border: 1px solid #e5e5e5;
  background: #fff; color: #0a0a0a; font: 500 14px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }
.nsh-desktop .nsh-dialog-actions.column button { width: 100%; }
.nsh-desktop .nsh-dialog button:hover { background: #f5f5f5; }
.nsh-desktop .nsh-dialog button:active { background: #e5e5e5; }
.nsh-desktop .nsh-dialog button.default { background: #171717; color: #fafafa; border-color: #171717; }
.nsh-desktop .nsh-dialog button.default:hover { background: #262626; }
.nsh-desktop .nsh-dialog button.destructive { background: #ef4444; color: #fff; border-color: #ef4444; }
.nsh-desktop .nsh-dialog button.destructive:hover { background: #dc2626; }
.nsh-desktop.nsh-dark .nsh-dialog { background: #171717; color: #fafafa; border-color: #262626; }
.nsh-desktop.nsh-dark .nsh-dialog p { color: #a3a3a3; }
.nsh-desktop.nsh-dark .nsh-dialog button { background: #171717; color: #fafafa; border-color: #262626; }
.nsh-desktop.nsh-dark .nsh-dialog button:hover { background: #262626; }
.nsh-desktop.nsh-dark .nsh-dialog button.default { background: #fafafa; color: #171717; border-color: #fafafa; }
.nsh-desktop.nsh-dark .nsh-dialog button.default:hover { background: #e5e5e5; }
.nsh-desktop.nsh-dark .nsh-dialog button.destructive { background: #f87171; color: #0a0a0a; border-color: #f87171; }
/* Phones: an iOS alert. */
.nsh-dialog-actions.row { display: flex; }
.nsh-dialog-actions.row button + button { border-left: 0.5px solid rgba(0,0,0,.2); }
.nsh-dialog button.cancel { font-weight: 600; }
.nsh-desktop .nsh-dialog button.cancel { font-weight: 500; }
.nsh-dialog button.destructive { color: #ff3b30; }
.nsh-dark .nsh-dialog { background: rgba(44,44,46,.97); color: #fff; }
.nsh-dark .nsh-dialog button { border-top-color: rgba(255,255,255,.15); color: #0a84ff; }
.nsh-dark .nsh-dialog button.destructive { color: #ff453a; }
.nsh-dark .nsh-dialog-actions.row button + button { border-left-color: rgba(255,255,255,.15); }
/* Action sheets: cards at the bottom. */
.nsh-sheet-backdrop { align-items: flex-end; }
.nsh-sheet { width: min(420px, calc(100% - 16px)); margin-bottom: 8px; display: flex; flex-direction: column; gap: 8px;
  font: 13px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }
.nsh-sheet > div { border-radius: 14px; background: rgba(245,245,245,.97); color: #000; overflow: hidden; text-align: center; }
.nsh-sheet .nsh-sheet-head { padding: 12px 16px; color: #8a8a8e; border-bottom: 0.5px solid rgba(0,0,0,.2); }
.nsh-sheet .nsh-sheet-head b { display: block; font-weight: 600; }
.nsh-sheet button { display: block; width: 100%; border: 0; background: none; padding: 16px; cursor: pointer;
  font: 19px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; color: #007aff; }
.nsh-sheet button + button { border-top: 0.5px solid rgba(0,0,0,.2); }
.nsh-sheet button.cancel { font-weight: 600; }
.nsh-sheet button.destructive { color: #ff3b30; }
.nsh-dark .nsh-sheet > div { background: rgba(44,44,46,.97); color: #fff; }
.nsh-dark .nsh-sheet button { color: #0a84ff; border-top-color: rgba(255,255,255,.15); }
.nsh-dark .nsh-sheet button.destructive { color: #ff453a; }
.nsh-desktop .nsh-sheet > div { border-radius: 12px; padding: 6px; background: #fff; color: #0a0a0a; border: 1px solid #e5e5e5;
  box-shadow: 0 10px 30px rgba(0,0,0,.25); }
.nsh-desktop .nsh-sheet .nsh-sheet-head { color: #737373; border-bottom: 1px solid #e5e5e5; margin-bottom: 4px; padding: 8px 12px 12px; }
.nsh-desktop .nsh-sheet button { border: 0; border-radius: 8px; padding: 11px 16px; font-size: 16px; color: inherit; }
.nsh-desktop .nsh-sheet button:hover { background: rgba(128,128,128,.12); }
.nsh-desktop .nsh-sheet button.destructive { color: #ef4444; }
.nsh-desktop.nsh-dark .nsh-sheet > div { background: #171717; color: #fafafa; border-color: #262626; }
.nsh-desktop.nsh-dark .nsh-sheet .nsh-sheet-head { color: #a3a3a3; border-bottom-color: #262626; }
.nsh-desktop.nsh-dark .nsh-sheet button.destructive { color: #f87171; }
.nsh-toast { position: absolute; left: 50%; bottom: 64px; transform: translateX(-50%); max-width: min(480px, calc(100% - 32px));
  width: max-content; padding: 9px 16px; border-radius: 18px; background: rgba(23,23,23,.92); color: #fafafa; text-align: center;
  font: 14px -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; pointer-events: none; z-index: 1500;
  overflow-wrap: anywhere; transition: opacity .2s; }
.nsh-dark .nsh-toast { background: rgba(250,250,250,.92); color: #0a0a0a; }
.nsh-live { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
.nsh-layer { position: absolute; left: 0; top: 0; width: 0; height: 0; transform-origin: 0 0; }
.nsh-spinner { position: absolute; left: 50%; width: 20px; height: 20px; margin-left: -10px; pointer-events: none; z-index: 5; }
.nsh-spinner i { position: absolute; left: 8.75px; top: 0; width: 2.5px; height: 5.5px; border-radius: 1.25px;
  background: #404040; transform-origin: 1.25px 10px; }
.nsh-dark .nsh-spinner i { background: #fff; }
.nsh-spinner.spin { animation: nsh-spin .8s steps(8) infinite; }
.nsh-error { position: absolute; left: 8px; right: 8px; bottom: 8px; max-height: 45%; overflow: auto; z-index: 2000;
  background: #b42318; color: #fff; font: 12px/1.4 ui-monospace, Menlo, monospace; padding: 10px 12px;
  border-radius: 10px; white-space: pre-wrap; -webkit-user-select: text; user-select: text; }
`;

/** An Apple keyboard: Command is the shortcut key, Option+Left moves by word. */
const APPLE = /mac|iphone|ipad/i.test(navigator.userAgentData?.platform ?? navigator.platform);

/** `nsh_key`'s codes of the keys that are not characters. */
const NAMED_KEYS = {
  Enter: 256, Escape: 257, Tab: 258, Backspace: 259, Delete: 260, Insert: 261, ArrowUp: 262, ArrowDown: 263,
  ArrowLeft: 264, ArrowRight: 265, Home: 266, End: 267, PageUp: 268, PageDown: 269,
};
/** The US layout's characters of the punctuation keys (`KeyboardEvent.code`). */
const CODE_CHARS = {
  Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\", IntlBackslash: "\\", Semicolon: ";",
  Quote: "'", Comma: ",", Period: ".", Slash: "/", Backquote: "`", Space: " ",
};

/**
 * The key of a key event as `nsh_key` takes it (-1: none): the character
 * it types without modifiers, or, when Shift or Option change it or the
 * layout types no ASCII there, the US layout's character of the same key
 * (shortcuts work on any layout, as menus' do).
 */
function keyCode(e) {
  const named = NAMED_KEYS[e.key];
  if (named) return named;
  const f = /^F([0-9]{1,2})$/.exec(e.key);
  if (f) return Number(f[1]) >= 1 && Number(f[1]) <= 24 ? 512 + Number(f[1]) : -1;
  const k = e.key;
  if (k.length === 1 && !e.shiftKey && !e.altKey && k >= " " && k <= "~") return k.toLowerCase().charCodeAt(0);
  const c = e.code ?? "";
  if (/^Key[A-Z]$/.test(c)) return c.charCodeAt(3) + 32;
  if (/^(Digit|Numpad)[0-9]$/.test(c)) return c.charCodeAt(c.length - 1);
  return CODE_CHARS[c]?.charCodeAt(0) ?? -1;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * The script a web view's HTML starts with: the first message from the
 * player carrying `token` hands it the port `ReactNativeWebView.postMessage`
 * writes to, and which runs `injectJavaScript` and history steps. The
 * page's own `message` listeners never see that message.
 */
function webBridge(token) {
  return `(() => {
  let port = null;
  const queue = [];
  window.ReactNativeWebView = { postMessage(data) { data = String(data); port ? port.postMessage(data) : queue.push(data); } };
  addEventListener("message", (e) => {
    if (port || e.source !== parent || e.data !== ${JSON.stringify(token)} || !e.ports[0]) return;
    e.stopImmediatePropagation();
    port = e.ports[0];
    port.onmessage = (m) => {
      const d = m.data;
      if (d && d.eval != null) { try { (0, eval)(String(d.eval)); } catch (err) { console.error(err); } }
      else if (d && d.go) history.go(d.go);
    };
    for (const data of queue.splice(0)) port.postMessage(data);
  });
})();`;
}

function measureContext() {
  const canvas = typeof OffscreenCanvas !== "undefined"
    ? new OffscreenCanvas(1, 1)
    : document.createElement("canvas");
  return canvas.getContext("2d");
}

/** Whether `locale`'s script is written right to left. */
function rightToLeft(locale) {
  try {
    const l = new Intl.Locale(locale);
    const info = l.getTextInfo?.() ?? l.textInfo;
    if (info?.direction) return info.direction === "rtl";
    const script = l.maximize().script;
    if (script) return ["Arab", "Hebr", "Thaa", "Syrc", "Nkoo", "Adlm", "Rohg"].includes(script);
  } catch {
    // Not a valid tag.
  }
  return /^(ar|he|iw|fa|ur|ps|sd|ug|yi|dv|ckb|syr)(-|$)/i.test(locale);
}

/** Loads the runtime and makes a player in `container`. */
export async function createPlayer(container, options = {}) {
  const player = new Player(container, options);
  await player.load(options.wasm ?? new URL("nsh.wasm", import.meta.url));
  return player;
}

class Player {
  /** The player that last took input (keys go to it while nothing has the focus). */
  static active = null;

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
    this.root.className = window.top === window ? "nsh-root" : "nsh-root nsh-embedded";
    // Its size is in the class: the core styles the root too (a
    // right-to-left layout), which replaces its inline style.
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
    // The navigation stacks and modals are the browser's history: by
    // default only when the app is the page (in an iframe each push would
    // be an entry of the embedding page's history too).
    this.history = options.history ?? window.top === window;
    if (window.top === window) document.documentElement.style.overscrollBehavior = "none";
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
          this.fontLoading(face.load());
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
    this.installEnvironment();
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

  /**
   * A face registered from data loads asynchronously: text is measured
   * again once it can be drawn. Faces registered while the app starts hide
   * it until they load, so its first frame shows the right fonts.
   */
  fontLoading(loading) {
    const pending = (this.fontsPending ??= []);
    pending.push(loading.catch((err) => console.warn("[nsh] font failed to load:", err)));
    if (this.starting && this.root.style.visibility !== "hidden") {
      this.root.style.visibility = "hidden";
      // A face that never settles must not keep the app hidden.
      clearTimeout(this.fontTimer);
      this.fontTimer = setTimeout(() => { this.root.style.visibility = ""; }, 3000);
    }
    if (pending.length > 1) return;
    const settle = () => {
      const waiting = this.fontsPending;
      Promise.all(waiting).then(() => {
        if (this.fontsPending.length > waiting.length) {
          this.fontsPending = this.fontsPending.slice(waiting.length);
          settle();
          return;
        }
        this.fontsPending = [];
        this.metricsCache.clear();
        this.call((w) => w.nsh_fonts_loaded());
        clearTimeout(this.fontTimer);
        this.root.style.visibility = "";
      });
    };
    settle();
  }

  // Running -------------------------------------------------------------

  /**
   * Runs `code` (a bundle from `nsh build`: `bundle.js` as a string, or its
   * bytecode `bundle.nshc` as an `ArrayBuffer` / `Uint8Array`, the only
   * form the runtime of published apps runs), replacing what ran. Options:
   * `release` (hardened, as packaged apps run; default: development
   * settings), `harden` (with bytecode: the hardening's bytecode the CLI
   * ships, `harden.nshc`), `capabilities` (the app's `capabilities.json`),
   * `sourceMap` (JSON), and where a split bundle's chunks are: `chunks[id]`
   * (a URL), else `id` relative to `chunkBase` (default: the page); the
   * chunks of a bytecode bundle are bytecode too.
   */
  run(code, {
    release = false,
    harden = null,
    capabilities = "",
    sourceMap = "",
    chunks = this.options.chunks,
    chunkBase = this.options.chunkBase,
  } = {}) {
    this.stop();
    this.resetHistory();
    this.app = (this.app ?? 0) + 1;
    if (code instanceof ArrayBuffer) code = new Uint8Array(code);
    const bytecode = typeof code !== "string";
    this.chunks = { map: chunks ?? {}, base: chunkBase ?? document.baseURI, bytecode };
    const root = this.root;
    root.textContent = "";
    root.append(this.live);
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
    if (harden) this.call(() => this.withStrings([new Uint8Array(harden)], (p, l) => this.wasm.nsh_bundle_harden(p, l)));
    this.idiom = this.options.idiom ?? this.measureIdiom(width);
    const environment = (this.reported = JSON.stringify(this.environment()));
    this.updateRootClasses();
    // Fonts registered while it starts hold its first frame back (`fontLoading`).
    this.starting = true;
    try {
      this.call(() => {
        this.withStrings([environment], (p, l) => this.wasm.nsh_environment(p, l));
        const start = bytecode ? this.wasm.nsh_start_bytecode : this.wasm.nsh_start;
        return this.withStrings([code], (p, l) =>
          start(p, l, width, height, this.dark ? 1 : 0, devicePixelRatio || 1));
      });
    } finally {
      this.starting = false;
    }
  }

  /** Fetches chunk `id` of the running bundle and hands it to the engine. */
  async loadChunk(id) {
    const { map, base, bytecode } = this.chunks;
    const app = this.app;
    let code;
    let ok = bytecode ? 2 : 1;
    // `nsh build --split` writes a chunk's bytecode next to it: `Screen.nshc`.
    const url = map[id] ?? (bytecode ? id.replace(/\.js$/, ".nshc") : id);
    try {
      // What the chunk needs synchronously once it runs (its fonts), from
      // the page, alongside it.
      const [response] = await Promise.all([fetch(new URL(url, base)), this.options.prepareChunk?.(id)]);
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`.trim());
      code = bytecode ? new Uint8Array(await response.arrayBuffer()) : await response.text();
    } catch (err) {
      code = `${err.message ?? err} (${url})`;
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
    this.environmentChanged();
    for (const el of this.views.values()) if (el._kind === "modal" && el.isConnected) this.reportRoot(el, width, height);
  }

  // Environment ---------------------------------------------------------

  /**
   * The kind of device, decided when an app starts (it never changes while
   * it runs, as on every platform): a mouse (a fine pointer that hovers)
   * and a viewport at least 600 wide make a desktop, a touch screen whose
   * short side is at least 600 a tablet, the rest a phone.
   */
  measureIdiom(width) {
    if (matchMedia("(pointer: fine) and (hover: hover)").matches && width >= 600) return "desktop";
    return Math.min(screen.width, screen.height) >= 600 ? "pad" : "phone";
  }

  /** Follows what the page's environment depends on. */
  installEnvironment() {
    const changed = () => this.environmentChanged();
    this.reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
    this.reducedMotion.addEventListener("change", changed);
    // The pixel ratio changes with the page's zoom and the screen: a query
    // for the current one says when it no longer holds.
    const resolution = () => {
      matchMedia(`(resolution: ${devicePixelRatio || 1}dppx)`).addEventListener("change", () => {
        resolution();
        changed();
      }, { once: true });
    };
    resolution();
    addEventListener("languagechange", changed);
    addEventListener("resize", changed);
    visualViewport?.addEventListener("resize", changed);
    visualViewport?.addEventListener("scroll", changed);
    new MutationObserver(changed).observe(document.documentElement, { attributes: true, attributeFilter: ["dir", "lang"] });
    // `env(safe-area-inset-*)` is only readable through a style it sets.
    this.safeProbe = document.createElement("div");
    this.safeProbe.style.cssText = "position:fixed;left:0;top:0;width:0;height:0;visibility:hidden;pointer-events:none;"
      + "padding:env(safe-area-inset-top,0px) env(safe-area-inset-right,0px) env(safe-area-inset-bottom,0px) env(safe-area-inset-left,0px)";
    document.body.append(this.safeProbe);
  }

  /** The page's insets covered by system UI, where they overlap the root. */
  safeArea() {
    const cs = getComputedStyle(this.safeProbe);
    const r = this.root.getBoundingClientRect();
    const px = (v) => parseFloat(v) || 0;
    return [
      Math.max(0, px(cs.paddingTop) - r.top),
      Math.max(0, px(cs.paddingRight) - (innerWidth - r.right)),
      Math.max(0, px(cs.paddingBottom) - (innerHeight - r.bottom)),
      Math.max(0, px(cs.paddingLeft) - r.left),
    ].map(Math.round);
  }

  /**
   * How much of the root the on-screen keyboard covers: mobile browsers
   * shrink the visual viewport under it, not the layout (0 while zoomed).
   */
  keyboardHeight() {
    const vv = visualViewport;
    if (!vv || vv.scale > 1.01) return 0;
    const bottom = this.root.getBoundingClientRect().bottom;
    return Math.max(0, Math.round(bottom - (vv.offsetTop + vv.height)));
  }

  /** What the page knows about the user and the device (`nsh_environment`). */
  environment() {
    const languages = navigator.languages?.length ? navigator.languages : [navigator.language];
    const locale = languages.find(Boolean) ?? Intl.DateTimeFormat().resolvedOptions().locale;
    const dir = document.documentElement.getAttribute("dir");
    // The root's font size is the browser's default text size unless the
    // page sets one: 16px at 100%.
    const fontSize = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    return {
      idiom: this.idiom,
      locale,
      rtl: dir === "rtl" || dir === "ltr" ? dir === "rtl" : rightToLeft(locale),
      reduceMotion: this.reducedMotion.matches,
      fontScale: Math.round(fontSize / 16 * 100) / 100,
      pixelRatio: devicePixelRatio || 1,
      safeArea: this.safeArea(),
      keyboard: this.keyboardHeight(),
      appleKeyboard: APPLE,
    };
  }

  /** Reports the environment if it changed since it was last reported. */
  environmentChanged() {
    if (!this.reported) return;
    const json = JSON.stringify(this.environment());
    if (json === this.reported) return;
    this.reported = json;
    this.call(() => this.withStrings([json], (p, l) => this.wasm.nsh_environment(p, l)));
  }

  setDark(dark) {
    this.dark = dark;
    this.updateRootClasses();
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
      frame.addEventListener("load", () => this.webLoaded(el));
    } else if (kind === "video") {
      el = document.createElement("div");
      const video = document.createElement("video");
      video.playsInline = true;
      video.style.cssText = "position:absolute;inset:0;width:100%;height:100%";
      el.append(video);
      el._video = video;
      this.wireVideo(el, video);
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
      el._base = "overflow:clip;";
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
    if (kind === "stack") el._base = "overflow:clip;";
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
    const host = parent._layer ?? parent;
    host.insertBefore(child, before && before.parentNode === host ? before : null);
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
    const focused = el.contains(document.activeElement);
    el.remove();
    if (parent?._kind === "stack") this.stackChanged(parent);
    this.screenGone(el, focused);
  }

  op_d([, id]) {
    const el = this.view(id);
    if (!el) return;
    this.views.delete(id);
    if (el._video) {
      // A removed media element keeps playing until it is collected.
      clearInterval(el._progressTimer);
      el._video.pause();
      el._video.removeAttribute("src");
      el._video.load();
    }
    if (el._kind === "modal") {
      this.dismiss(el);
      return;
    }
    const parent = el.parentNode;
    const focused = el.contains(document.activeElement);
    if (this.layoutAnimation?.delete && parent) {
      this.animateOut(el, this.layoutAnimation.delete);
    } else {
      el.remove();
    }
    if (parent?._kind === "stack") this.stackChanged(parent);
    this.screenGone(el, focused);
  }

  /** A screen left its stack: the focus goes back to what had it when the
   * screen was pushed, unless it moved elsewhere meanwhile. */
  screenGone(el, focused) {
    const restore = el._restoreFocus;
    el._restoreFocus = null;
    const focus = document.activeElement;
    if (restore?.isConnected && (focused || !focus || focus === document.body)) restore.focus({ preventScroll: true });
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
    // The press cursor goes before the view's own `cursor`, which wins.
    el.style.cssText = (el._base ?? "") + (el._pressable ? "cursor:pointer;" : "") + css + (el._extra ?? "");
    if (transition) el.style.transition = transition;
    if (el._kind === "scroll") this.scrollOverflow(el);
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
    for (const [x, y, text, style, after] of runs) {
      const span = document.createElement("span");
      span.className = "nsh-run";
      span.style.cssText = styles[style] ?? "";
      span.style.left = `${cx + x}px`;
      span.style.top = `${cy + y}px`;
      span.textContent = text;
      // The line's break, for copying (`copy`).
      if (after) span._after = after;
      if (el._animation.color) span.style.color = el._animation.color;
      frag.append(span);
    }
    for (const span of [...el.querySelectorAll(":scope > .nsh-run")]) span.remove();
    el.insertBefore(frag, el.firstChild);
    el.classList.toggle("nsh-selectable", !!selectable);
    el.classList.toggle("nsh-autoselect", selectable == null);
    if (selectionColor) el.style.setProperty("--nsh-selection", selectionColor);
    else el.style.removeProperty("--nsh-selection");
    el._textLabel = runs.map((r) => r[2]).join("");
    if (el._props?.label == null) el.setAttribute("aria-label", el._textLabel);
  }

  op_e([, id, events]) {
    const el = this.view(id);
    if (!el) return;
    el._events = new Set(events);
    const pressable = PRESS_EVENTS.some((e) => el._events.has(e));
    if (pressable && !el._pressable && !el.style.cursor) el.style.cursor = "pointer";
    el._pressable = pressable;
    this.updateFocusable(el);
    this.updateTouchAction(el);
    if (el._kind === "scroll" && el._events.has("onRefresh")) this.installPull(el);
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
    if (props.testID) el.dataset.testid = props.testID;
    else delete el.dataset.testid;
    const pe = props.pointerEvents;
    // Kept in `_extra` so that restyling (`op_s`) keeps it.
    el._extra = pe === "none" || pe === "box-none" ? "pointer-events:none;" : "";
    el.style.pointerEvents = el._extra ? "none" : "";
    if (pe === "box-none") el._boxNone = true;
    this.updateFocusable(el);
    this.updateTouchAction(el);
  }

  op_al([, id, label]) {
    const el = this.view(id);
    if (!el) return;
    (el._props ??= {}).label = label;
    this.updateAria(el);
  }

  /** ARIA attributes from the accessibility props (`op_vp`). */
  updateAria(el) {
    const p = el._props ?? {};
    const set = (name, value) => (value == null || value === false ? el.removeAttribute(name) : el.setAttribute(name, String(value)));
    const role = ARIA_ROLES[p.role] ?? (p.role && p.role !== "none" ? p.role : el._pressable ? "button" : null);
    set("role", role);
    set("aria-label", p.label ?? el._textLabel);
    set("aria-description", p.hint);
    set("aria-disabled", p.disabled && "true");
    set("aria-selected", p.selected && "true");
    set("aria-checked", p.checked);
    set("aria-busy", p.busy && "true");
    set("aria-expanded", p.expanded);
    const [min, max, now, text] = p.value ?? [];
    set("aria-valuemin", min);
    set("aria-valuemax", max);
    set("aria-valuenow", now);
    set("aria-valuetext", text);
    set("aria-live", p.live);
    set("aria-hidden", p.hidden && "true");
  }

  /** Pressables take the keyboard focus (Tab; Enter and Space press them),
   * unless disabled or hidden from assistive technologies. */
  updateFocusable(el) {
    const p = el._props;
    const focusable = el._pressable && !p?.disabled && !p?.hidden;
    if (focusable) el.tabIndex = 0;
    else if (el.hasAttribute("tabindex") && !el._a11yFocus) el.removeAttribute("tabindex");
    if (focusable && !el._focusListeners) {
      el._focusListeners = true;
      // `focus:` classes follow the keyboard focus (the browser draws a
      // ring only then), not the focus a click would give.
      el.addEventListener("focus", () => el.matches(":focus-visible") && this.setState(el, FOCUSED, true));
      el.addEventListener("blur", () => this.setState(el, FOCUSED, false));
    }
    this.updateAria(el);
  }

  /** What touches on `el` leave to the browser: nothing on views with
   * native pans, pinches or rotations; a `panActivation` gate along one
   * axis leaves scrolling along the other. Views that take presses or
   * touches are marked: text in them is not selected by default. */
  updateTouchAction(el) {
    const pan = el._gestures?.pan || el._events.has("onPanStart") || el._events.has("onPanEnd");
    const pinch = el._gestures?.pinch || el._gestures?.rotation;
    el.classList.toggle("nsh-press", !!(el._pressable || pan || pinch || el._events.has("onTouch")));
    const gate = el._props?.panActivation;
    const horizontal = gate && gate[0] != null && gate[1] == null;
    const vertical = gate && gate[1] != null && gate[0] == null;
    el.classList.toggle("nsh-pan", !!(pinch || (pan && !horizontal && !vertical)));
    el.classList.toggle("nsh-pan-x", !pinch && !!horizontal);
    el.classList.toggle("nsh-pan-y", !pinch && !!vertical);
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
    this.updateRootClasses();
  }

  /** Dark appearance as shown: the app's, else the system's. */
  get shownDark() {
    return this.appearance ? this.appearance === 2 : this.dark;
  }

  // Images

  /**
   * `source`, `resizeMode`, `defaultSource` (shown until the image loads,
   * then faded out over `fadeDuration`) and `blurRadius` (a CSS blur).
   * Any URL the browser loads works: `asset:/`, http(s), data: and blob:.
   */
  op_img([, id, uri, mode, placeholder, blur, fade]) {
    const el = this.view(id);
    if (!el?._img) return;
    const img = el._img;
    img.style.objectFit = mode;
    img.style.filter = blur > 0 ? `blur(${blur}px)` : "";
    el._fade = fade;
    const ph = placeholder ? this.asset(placeholder) : "";
    if (ph && !el._ph) {
      el._ph = document.createElement("img");
      el._ph.draggable = false;
      el._ph.style.cssText = img.style.cssText;
      el.insertBefore(el._ph, img);
    }
    if (el._ph) {
      el._ph.style.objectFit = mode;
      el._ph.style.filter = img.style.filter;
      if (ph) {
        if (el._ph.getAttribute("src") !== ph) el._ph.src = ph;
      } else {
        el._ph.remove();
        el._ph = null;
      }
    }
    const src = this.asset(uri ?? "");
    if (el._src === src) return;
    el._src = src;
    el._failed = false;
    if (el._ph) el._ph.style.visibility = "";
    if (!src) {
      img.removeAttribute("src");
      return;
    }
    // After the batch: its events may come after its props.
    queueMicrotask(() => el._events.has("onLoadStart") && this.event(id, "onLoadStart"));
    // Shown when it loaded: faded in over the placeholder, or at once.
    img.style.transition = "";
    img.style.opacity = fade > 0 ? "0" : "";
    img.src = src;
    this.applyTint(el);
  }

  applyTint(el) {
    if (!el._img) return;
    if (el._tint && el._src) {
      el._img.style.visibility = "hidden";
      el.style.backgroundColor = el._tint;
      const fit = el._img.style.objectFit;
      const size = fit === "fill" ? "100% 100%" : fit === "scale-down" ? "contain" : fit || "cover";
      const mask = `url("${el._src}") center / ${size} no-repeat`;
      el.style.mask = mask;
      el.style.webkitMask = mask;
    } else {
      el._img.style.visibility = el._failed ? "hidden" : "";
    }
  }

  imageLoaded(el, img) {
    if (el._fade > 0) {
      img.style.transition = `opacity ${el._fade}ms`;
      img.style.opacity = "";
    }
    if (el._ph) {
      const ph = el._ph;
      clearTimeout(el._phTimer);
      el._phTimer = setTimeout(() => { if (el._ph === ph) ph.style.visibility = "hidden"; }, el._fade > 0 ? el._fade : 0);
    }
    if (el._events.has("onLoad")) this.event(el._id, "onLoad", { width: img.naturalWidth, height: img.naturalHeight });
    if (el._events.has("onLoadEnd")) this.event(el._id, "onLoadEnd");
  }

  imageFailed(el) {
    // No broken-image icon: the placeholder, or nothing, shows.
    el._failed = true;
    el._img.style.visibility = "hidden";
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
    this.placeLayer(el);
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
    if (this.zooms(el)) {
      this.ensureLayer(el);
      el._zoom = Math.min(Math.max(el._zoom, behavior.minZoom), behavior.maxZoom);
      this.placeLayer(el);
    }
    this.scrollOverflow(el);
    this.snapMarkers(el);
  }

  /** Scrolls only along the view's axis, and not at all while scrolling is
   * disabled. With `overflow: auto` on both axes, a vertical view's own bar
   * narrows it below its content's laid-out width and adds a horizontal bar.
   * Kept in `_base` so that restyling (`op_s`) keeps it. */
  scrollOverflow(el) {
    const scroll = el._behavior?.enabled === false ? "hidden" : "auto";
    // Zoomable content scrolls both ways, panned and pinched here (touch-action none).
    const zooms = this.zooms(el);
    const [x, y] = zooms ? [scroll, scroll] : el._horizontal ? [scroll, "hidden"] : ["hidden", scroll];
    el._base = `overflow-x:${x};overflow-y:${y};${zooms ? "touch-action:none;" : ""}`;
    el.style.overflowX = x;
    el.style.overflowY = y;
    el.style.touchAction = zooms ? "none" : "";
  }

  snapMarkers(el) {
    const b = el._behavior;
    for (const m of el._markers ?? []) m.remove();
    el._markers = [];
    el._snapPoints = null;
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
    el._snapPoints = [...points].sort((a, b) => a - b);
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
    // Still moving after the finger left (touch scrolling is the
    // browser's): a glide.
    if (el._released && !el._momentum && performance.now() - el._released < 150) {
      el._momentum = true;
      this.scrollEvent(el, "onMomentumScrollBegin");
    }
    // Pulled to refresh, the offset is negative, as on the native backends.
    this.call((w) => w.nsh_scroll(el._id, el.scrollLeft, el.scrollTop - (el._pull ?? 0), el._applied));
  }

  /** The browser's scrolling stopped: the end, unless the scroll is driven
   * here (`drive`) or a finger still holds it. */
  scrollEnded(el) {
    if (el._driven || el._touchDrag) return;
    // The browser's end of a scroll that ended here (`ownEnd`).
    const own = el._ownEnd;
    el._ownEnd = null;
    if (own && own[0] === el.scrollLeft && own[1] === el.scrollTop) return;
    el._released = 0;
    if (el._restoreSnap) {
      el._restoreSnap = false;
      this.snapMarkers(el);
    }
    if (el._momentum) {
      el._momentum = false;
      this.scrollEvent(el, "onMomentumScrollEnd");
    }
    this.scrollEvent(el, "onScrollEnd");
  }

  // Content layers: the children of a zoomable or refreshable scroll view
  // go into a layer that scales them (zoom) and moves them down (pull to
  // refresh); the core's frames stay the unzoomed ones.

  ensureLayer(el) {
    if (el._layer) return el._layer;
    const layer = document.createElement("div");
    layer.className = "nsh-layer";
    for (const child of [...el.children]) if (child._id !== undefined) layer.append(child);
    el.insertBefore(layer, el._spacer.nextSibling);
    el._layer = layer;
    el._zoom = 1;
    el._pull = 0;
    return layer;
  }

  zooms(el) {
    const b = el._behavior;
    return el._kind === "scroll" && !!b && b.maxZoom > b.minZoom;
  }

  /** Where the zoomed content starts in the scrolled area: centered while smaller than the viewport (`centerContent`). */
  layerOffset(el) {
    if (!el._behavior?.centerContent || !el._frame || !el._content) return [0, 0];
    const z = el._zoom ?? 1;
    return [Math.max(0, (el._frame[2] - el._content[0] * z) / 2), Math.max(0, (el._frame[3] - el._content[1] * z) / 2)];
  }

  /** Places the layer and sizes the scrolled area for the zoom and the pull. */
  placeLayer(el) {
    const [w, h] = el._content ?? [1, 1];
    const z = el._zoom ?? 1;
    const pull = el._pull ?? 0;
    el._spacer.style.width = `${w * z}px`;
    el._spacer.style.height = `${h * z + pull}px`;
    if (!el._layer) return;
    const [ox, oy] = this.layerOffset(el);
    el._layer.style.transform = `translate(${ox}px, ${oy + pull}px)${z !== 1 ? ` scale(${z})` : ""}`;
  }

  /** The zoom the content of `el` is shown at (its zoomable scroll views', multiplied). */
  zoomOf(el) {
    let z = 1;
    for (let e = el.parentNode; e && e !== this.root; e = e.parentNode) if (e._layer && e._zoom) z *= e._zoom;
    return z;
  }

  /** Zooms `el` to `scale` with content point (cx, cy) (unzoomed) at (fx, fy) in its viewport. */
  zoomContent(el, scale, cx, cy, fx, fy) {
    el._zoom = scale;
    this.placeLayer(el);
    const [ox, oy] = this.layerOffset(el);
    el.scrollLeft = cx * scale + ox - fx;
    el.scrollTop = cy * scale + oy - fy;
    this.call((w) => w.nsh_zoom(el._id, scale));
    this.scrolled(el);
  }

  /** The content point (unzoomed) at (fx, fy) in the viewport of `el`. */
  contentPoint(el, fx, fy) {
    const [ox, oy] = this.layerOffset(el);
    const z = el._zoom ?? 1;
    return [(el.scrollLeft + fx - ox) / z, (el.scrollTop + fy - oy) / z];
  }

  /** The zoom limits; past them by a rubber band while pinching when `bouncesZoom`. */
  limitZoom(el, scale, rubber) {
    const b = el._behavior;
    const min = b.minZoom;
    const max = Math.max(b.maxZoom, min);
    if (scale > max) return rubber && b.bouncesZoom ? max * Math.pow(scale / max, 0.3) : max;
    if (scale < min) return rubber && b.bouncesZoom ? min * Math.pow(scale / min, 0.3) : min;
    return scale;
  }

  /** Animates the zoom to `scale` with content point (cx, cy) moving to (fx, fy). */
  animateZoom(el, scale, cx, cy, fx, fy, done) {
    cancelAnimationFrame(el._zoomAnimation);
    const z0 = el._zoom;
    const [x0, y0] = this.contentPoint(el, fx, fy);
    const t0 = performance.now();
    const step = (now) => {
      const t = Math.min(1, (now - t0) / 250);
      const k = 1 - Math.pow(1 - t, 3);
      this.zoomContent(el, z0 + (scale - z0) * k, x0 + (cx - x0) * k, y0 + (cy - y0) * k, fx, fy);
      if (t < 1) el._zoomAnimation = requestAnimationFrame(step);
      else done?.();
    };
    el._zoomAnimation = requestAnimationFrame(step);
  }

  /** A zoom ended: back within the limits, and canvases drawn again at the new resolution. */
  zoomEnded(el, fx, fy) {
    const settled = () => {
      for (const canvas of el._layer?.querySelectorAll("canvas") ?? []) {
        const view = this.viewOf(canvas);
        if (view) this.drawSoon(view);
      }
    };
    const target = this.limitZoom(el, el._zoom, false);
    if (Math.abs(target - el._zoom) > 1e-3) {
      const [cx, cy] = this.contentPoint(el, fx, fy);
      this.animateZoom(el, target, cx, cy, fx, fy, settled);
    } else settled();
  }

  /** `zoomTo({ scale, x, y, animated })`: content point (x, y) to the middle of the viewport, or the point there now. */
  zoomCommand(el, scale, x, y, animated) {
    if (!this.zooms(el) || !el._frame) return;
    el._applied++;
    const target = this.limitZoom(el, scale, false);
    const fx = el._frame[2] / 2;
    const fy = el._frame[3] / 2;
    const [px, py] = this.contentPoint(el, fx, fy);
    const cx = x ?? px;
    const cy = y ?? py;
    if (animated) this.animateZoom(el, target, cx, cy, fx, fy, () => this.zoomEnded(el, fx, fy));
    else {
      this.zoomContent(el, target, cx, cy, fx, fy);
      this.zoomEnded(el, fx, fy);
    }
  }

  /** Ctrl+wheel (and touchpad pinches, which browsers send as one) zooms the zoomable scroll view under the pointer; whether there was one. */
  wheelZoom(e) {
    const el = this.ancestors(this.viewOf(e.target)).find((v) => this.zooms(v) && v._behavior.pinch);
    if (!el) return false;
    const r = el.getBoundingClientRect();
    const fx = e.clientX - r.left;
    const fy = e.clientY - r.top;
    // A mouse wheel's notches are large deltas, a touchpad pinch's small ones.
    const dy = e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY;
    const factor = Math.abs(dy) >= 50 ? Math.pow(1.25, -Math.sign(dy)) : Math.exp(-dy * 0.01);
    const [cx, cy] = this.contentPoint(el, fx, fy);
    cancelAnimationFrame(el._zoomAnimation);
    this.zoomContent(el, this.limitZoom(el, el._zoom * factor, false), cx, cy, fx, fy);
    clearTimeout(el._wheelZoomTimer);
    el._wheelZoomTimer = setTimeout(() => this.zoomEnded(el, fx, fy), 200);
    return true;
  }

  /** Safari's touchpad pinches (gesture events; it sends no ctrl+wheel). */
  gestureZoom(e, phase) {
    const el = phase === 0
      ? this.ancestors(this.viewOf(e.target)).find((v) => this.zooms(v) && v._behavior.pinch)
      : this.gesture;
    if (!el) return;
    e.preventDefault();
    const r = el.getBoundingClientRect();
    const fx = (e.clientX ?? r.left + r.width / 2) - r.left;
    const fy = (e.clientY ?? r.top + r.height / 2) - r.top;
    if (phase === 0) {
      this.gesture = el;
      el._gestureStart = el._zoom;
    }
    const [cx, cy] = this.contentPoint(el, fx, fy);
    this.zoomContent(el, this.limitZoom(el, el._gestureStart * e.scale, true), cx, cy, fx, fy);
    if (phase === 2) {
      this.gesture = null;
      this.zoomEnded(el, fx, fy);
    }
  }

  // Pull to refresh (vertical scroll views with `onRefresh`, and
  // `refreshing`): dragged past the top, the content moves down over a
  // spinner; released far enough, it refreshes and keeps room for the
  // spinner until `refreshing` turns false. Mouse drags go through the
  // pointer handling, touches through touch events here (the browser
  // scrolls them otherwise).

  op_ref([, id, refreshing]) {
    const el = this.view(id);
    if (!el || el._kind !== "scroll" || el._horizontal) return;
    this.ensureLayer(el);
    el._refreshing = refreshing;
    if (!el._pulling) {
      if (refreshing && el.scrollTop <= 0.5) this.animatePull(el, REFRESH_SPACE);
      else if (!refreshing && el._pull > 0) this.animatePull(el, 0);
    }
    this.updateSpinner(el);
  }

  refreshable(el) {
    return el._kind === "scroll" && !el._horizontal && !this.zooms(el) && (el._events.has("onRefresh") || el._refreshing);
  }

  installPull(el) {
    this.ensureLayer(el);
    if (el._pullTouch) return;
    el._pullTouch = true;
    let start = null;
    el.addEventListener("touchstart", (e) => {
      start = e.touches.length === 1 && this.refreshable(el) && el._behavior?.enabled !== false
        ? { y: e.touches[0].clientY, v: el.scrollTop - el._pull, pulling: false }
        : null;
    }, { passive: true });
    el.addEventListener("touchmove", (e) => {
      if (!start || e.touches.length !== 1) return;
      const v = start.v - (e.touches[0].clientY - start.y);
      if (v >= 0 && el._pull <= 0) return;
      // Once the browser scrolls, the gesture is its own.
      if (!e.cancelable) {
        start = null;
        return;
      }
      e.preventDefault();
      if (!start.pulling) {
        start.pulling = true;
        el._pulling = true;
        cancelAnimationFrame(el._pullAnimation);
      }
      this.dragPull(el, v);
    }, { passive: false });
    const end = () => {
      if (start?.pulling) {
        el._pulling = false;
        this.releasePull(el);
      }
      start = null;
    };
    el.addEventListener("touchend", end);
    el.addEventListener("touchcancel", end);
  }

  /** A drag to offset `v` (negative: pulled past the top by that much). */
  dragPull(el, v) {
    if (v >= 0) {
      this.setPull(el, 0);
      el.scrollTop = v;
      return;
    }
    el.scrollTop = 0;
    const over = -v;
    // Refreshing, the spinner's room is the pull's start.
    this.setPull(el, el._refreshing ? Math.min(over, REFRESH_SPACE) + Math.max(0, over - REFRESH_SPACE) * 0.5 : over * 0.5);
  }

  releasePull(el) {
    if (!el._refreshing && el._events.has("onRefresh") && el._pull >= REFRESH_PULL) {
      // Shown right away, as UIRefreshControl does; JS turns it off.
      el._refreshing = true;
      this.event(el._id, "onRefresh");
    }
    this.animatePull(el, el._refreshing && el._pull > 0 ? REFRESH_SPACE : 0);
  }

  setPull(el, pull) {
    if (pull === el._pull) return;
    el._pull = pull;
    this.placeLayer(el);
    this.updateSpinner(el);
    this.scrolled(el);
  }

  animatePull(el, target) {
    cancelAnimationFrame(el._pullAnimation);
    const from = el._pull ?? 0;
    if (from === target) return;
    const t0 = performance.now();
    const step = (now) => {
      const t = Math.min(1, (now - t0) / 250);
      this.setPull(el, from + (target - from) * (1 - Math.pow(1 - t, 3)));
      if (t < 1) el._pullAnimation = requestAnimationFrame(step);
    };
    el._pullAnimation = requestAnimationFrame(step);
  }

  /** The spinner above the content: its spokes appear as it is pulled, and turn while refreshing. */
  updateSpinner(el) {
    const shown = el._pull > 0;
    let spinner = el._spinner;
    if (!shown) {
      if (spinner) spinner.style.display = "none";
      return;
    }
    if (!spinner) {
      spinner = el._spinner = document.createElement("div");
      spinner.className = "nsh-spinner";
      for (let i = 0; i < 8; i++) {
        const spoke = document.createElement("i");
        spoke.style.transform = `rotate(${i * 45}deg)`;
        spinner.append(spoke);
      }
      el.append(spinner);
    }
    spinner.style.display = "";
    spinner.style.top = `${el._pull - REFRESH_SPACE / 2 - 10}px`;
    const progress = el._refreshing ? 1 : Math.min(1, el._pull / REFRESH_PULL);
    const count = Math.min(8, Math.ceil(progress * 8));
    [...spinner.children].forEach((spoke, i) => {
      spoke.style.visibility = i < count ? "" : "hidden";
      // Turning, the spokes fade behind the lead one.
      spoke.style.opacity = progress < 1 ? "0.8" : String(1 - ((8 - i) % 8) / 8 * 0.85);
    });
    spinner.classList.toggle("spin", !!el._refreshing);
  }


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
    // Zoomed in, drawn at the resolution it is shown at.
    const ratio = (devicePixelRatio || 1) * Math.min(4, this.zoomOf(el));
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
    if (!el) return;
    el._gestures = { pan, pinch, rotation };
    this.updateTouchAction(el);
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
        const key = e.key === "Backspace" || e.key === "Enter" || e.key === "Escape" ? e.key : e.key.length === 1 ? e.key : null;
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
    screen._popping = true;
    screen.style.transform = "translateX(100%)";
    setTimeout(() => this.event(screen._id, "onDismiss"), 300);
    this.syncHistorySoon();
  }

  /** A stack's screens, without one going back. */
  screensOf(stack) {
    return [...stack.children].filter((s) => s._kind === "screen" && !s._popping);
  }

  /** The newest modal that is presented and not leaving. */
  topModal() {
    const modals = [...this.root.children].filter((el) => el._kind === "modal" && !el._leaving);
    return modals[modals.length - 1] ?? null;
  }

  /** The innermost shown stack (inside `within`, else anywhere) that goes
   * back: it has a screen under the top one, or a top one that asks. */
  backStack(within) {
    const shown = (el) => el.isConnected && (el.checkVisibility?.({ visibilityProperty: true }) ?? true);
    const candidates = [];
    for (const el of this.views.values()) {
      if (el._kind !== "stack" || !shown(el) || (within && !within.contains(el))) continue;
      const screens = this.screensOf(el);
      if (screens.length > 1 || screens[screens.length - 1]?._screen?.preventBack) candidates.push(el);
    }
    return candidates.find((c) => !candidates.some((o) => o !== c && c.contains(o))) ?? null;
  }

  /**
   * Going back (Escape, the browser's or the mouse's back button): a stack
   * inside the newest modal pops, else that modal asks to close; without a
   * modal the app's `onBackPress` hears it first when it listens (it sends
   * `Command::Back` when no listener takes it), else the innermost stack
   * pops. Decided here from what is mounted, as on Android and desktops.
   * Whether something took it.
   */
  back() {
    if (!this.topModal() && this.root._events?.has("onBackPress")) {
      this.event(ROOT_ID, "onBackPress");
      return true;
    }
    return this.defaultBack();
  }

  /** Going back as without the app's listeners (`Command::Back`). */
  defaultBack() {
    const modal = this.topModal();
    const stack = this.backStack(modal);
    if (stack) {
      const screens = this.screensOf(stack);
      this.goBack(screens[screens.length - 1]);
      return true;
    }
    if (modal) {
      this.requestClose(modal);
      return true;
    }
    return false;
  }

  // History: one entry per screen over the bottom one of each shown stack
  // and per presented modal, so that the browser's back button goes back
  // in the app. Entries the app takes away itself are gone back over
  // (`history.go`), their `popstate` ignored.

  installHistory() {
    this.historyDepth = 0;
    this.ignoredPops = 0;
    // Entries are this page load's: those of an earlier load (a reload
    // keeps them, with their depths) are foreign, below this one's.
    this.historySession = `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
    history.replaceState(this.historyState(0), "");
    window.addEventListener("popstate", (e) => this.popState(e));
  }

  historyState(depth) {
    return { ...(history.state ?? {}), nsh: depth, nshSession: this.historySession };
  }

  /** A new run starts at the app's first entry. */
  resetHistory() {
    if (!this.history || !this.historyDepth) return;
    this.ignoredPops++;
    history.go(-this.historyDepth);
    this.historyDepth = 0;
  }

  backDepth() {
    let depth = [...this.root.children].filter((el) => el._kind === "modal" && !el._leaving).length;
    for (const el of this.views.values()) {
      if (el._kind === "stack" && el.isConnected && (el.checkVisibility?.({ visibilityProperty: true }) ?? true)) {
        depth += Math.max(0, this.screensOf(el).length - 1);
      }
    }
    return depth;
  }

  syncHistorySoon() {
    if (!this.history || this.historySync) return;
    this.historySync = setTimeout(() => {
      this.historySync = 0;
      this.syncHistory();
    }, 0);
  }

  syncHistory() {
    const depth = this.backDepth();
    const have = this.historyDepth;
    for (let i = have + 1; i <= depth; i++) history.pushState(this.historyState(i), "");
    if (depth < have) {
      this.ignoredPops++;
      history.go(depth - have);
    }
    this.historyDepth = depth;
  }

  popState(e) {
    if (this.ignoredPops > 0) {
      this.ignoredPops--;
      return;
    }
    const have = this.historyDepth;
    if (e.state?.nshSession !== this.historySession) {
      // Back past the app's first entry: the app goes back to its start.
      // An earlier load's entries (a reload keeps them) are passed over,
      // as if the page had been left from its first screen.
      this.historyDepth = 0;
      for (let i = 0; i < have; i++) if (!this.back()) break;
      if (e.state?.nsh != null) history.back();
      return;
    }
    const at = e.state.nsh ?? 0;
    if (at < have) {
      // Back (several entries from the back button's menu): what the app
      // does not let go back gets its entry again.
      this.historyDepth = at;
      for (let i = at; i < have; i++) if (!this.back()) break;
      this.syncHistorySoon();
    } else if (at > have) {
      // Forward to a screen that is gone: back where the app is.
      this.ignoredPops++;
      history.go(have - at);
    }
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
        const focus = document.activeElement;
        if (focus && focus !== document.body && this.root.contains(focus) && !added.contains(focus)) added._restoreFocus = focus;
        screen.style.transform = "translateX(100%)";
        requestAnimationFrame(() => requestAnimationFrame(() => { screen.style.transform = ""; }));
      } else {
        screen.style.transform = top ? "" : "translateX(-30%)";
      }
      this.renderHeader(screen);
    });
    if (added && stack._frame) this.reportRoot(added, stack._frame[2], stack._frame[3]);
    this.syncHistorySoon();
  }

  reportRoot(el, width, height) {
    // Modals cover the root, and so its safe area.
    const [top, right, bottom, left] = el._kind === "modal" && this.safeProbe ? this.safeArea() : [0, 0, 0, 0];
    this.call((w) => w.nsh_root_size(el._id, width, height, top, right, bottom, left));
  }

  // Modals

  op_mc([, id, animation, transparent, dismissible]) {
    const el = this.view(id);
    if (!el) return;
    el._modal = { animation, transparent, dismissible };
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
    el._leaving = false;
    el._closed = false;
    // The focus leaves the views under it, and comes back when it closes.
    const focus = document.activeElement;
    el._restoreFocus = focus !== document.body && this.root.contains(focus) ? focus : null;
    el._restoreFocus?.blur();
    el.setAttribute("aria-modal", "true");
    this.root.append(el);
    this.scopeFocus();
    this.syncHistorySoon();
    this.reportRoot(el, w, h);
    requestAnimationFrame(() => requestAnimationFrame(() => {
      el.style.transition = "";
      el.style.transform = "";
      el.style.opacity = "";
      setTimeout(() => this.event(el._id, "onShow"), animation === "none" ? 0 : 350);
    }));
  }

  /** Takes modal `el` away; `onDismiss` follows unless `reported`. */
  dismiss(el, reported = el._closed) {
    if (el._leaving) return;
    el._leaving = true;
    const animation = el._modal?.animation ?? "slide";
    if (animation === "slide") el.style.transform = "translateY(100%)";
    if (animation === "fade") el.style.opacity = "0";
    this.scopeFocus();
    this.syncHistorySoon();
    const restore = el._restoreFocus;
    if (restore?.isConnected && !this.topModal()) restore.focus({ preventScroll: true });
    setTimeout(() => {
      if (el._leaving) el.remove();
      if (!reported) this.event(el._id, "onDismiss");
    }, animation === "none" ? 0 : 350);
  }

  /** Going back or Escape on a modal: it asks to close (`onRequestClose`);
   * a `dismissible` one closes natively, then reports `onDismiss`. */
  requestClose(el) {
    this.event(el._id, "onRequestClose");
    if (el._modal?.dismissible && !el._leaving) {
      el._closed = true;
      this.event(el._id, "onDismiss");
      this.dismiss(el, true);
    }
  }

  /** Only the newest modal or dialog takes the focus, input and the
   * screen reader: what lies under it is inert. */
  scopeFocus() {
    const top = this.dialogs?.[this.dialogs.length - 1] ?? this.topModal();
    for (const child of this.root.children) {
      if (child === this.live || child.classList.contains("nsh-error")) continue;
      child.inert = (child._kind === "modal" && !!child._leaving) || (!!top && child !== top);
    }
  }

  // Web views and videos

  /**
   * A web view is an <iframe>. HTML (`source={{ html }}`) loads as
   * `srcdoc` in a sandbox with an origin of its own (it never reaches the
   * player's page), with a bridge script first: the page's
   * `window.ReactNativeWebView.postMessage(data)` reaches `onMessage`
   * through a `MessageChannel`, and `injectJavaScript` runs through it.
   * `injectedJavaScriptBeforeContentLoaded` / `injectedJavaScript` are
   * scripts at the document's start and end. A URL loads as it is
   * (sandboxed without its origin when it is the player's own); the browser
   * hides what such a page does, so it only gets `postMessage` and load
   * events, with the URL it was given.
   */
  op_web([, id, url, html, base, before, after, scroll, js]) {
    const el = this.view(id);
    const frame = el?._frameEl;
    if (!frame) return;
    const key = JSON.stringify([url, html, base, before, after, scroll, js]);
    if (el._web === key) return;
    el._web = key;
    el._port?.close();
    el._port = null;
    el._token = html != null && js ? Math.random().toString(36).slice(2) : null;
    const sandbox = ["allow-forms", "allow-popups", "allow-popups-to-escape-sandbox", "allow-modals", "allow-downloads"];
    if (js) sandbox.push("allow-scripts");
    let same = false;
    if (html == null && url) {
      try {
        same = new URL(url, location.href).origin === location.origin;
      } catch {
        // Not a URL the frame can load anyway.
      }
      // Pages of other sites keep their own origin (their storage works); one of the player's own must not reach it.
      if (!same) sandbox.push("allow-same-origin");
    }
    frame.setAttribute("sandbox", sandbox.join(" "));
    if (html != null) {
      let head = "";
      if (base) head += `<base href="${base.replace(/"/g, "&quot;")}">`;
      if (el._token) head += `<script>${webBridge(el._token)}</script>`;
      if (!scroll) head += "<style>html,body{overflow:hidden!important}</style>";
      if (before) head += `<script>${before.replace(/<\/script/gi, "<\\/script")}</script>`;
      const tail = after ? `<script>${after.replace(/<\/script/gi, "<\\/script")}</script>` : "";
      // After a doctype, which must stay first (quirks mode otherwise).
      const doctype = /^\s*<!doctype[^>]*>/i.exec(html)?.[0] ?? "";
      el._webUrl = base ?? "about:srcdoc";
      el._webTitle = new DOMParser().parseFromString(html, "text/html").title;
      frame.removeAttribute("src");
      frame.removeAttribute("scrolling");
      frame.srcdoc = doctype + head + html.slice(doctype.length) + tail;
    } else {
      el._webUrl = url ?? "";
      el._webTitle = "";
      frame.removeAttribute("srcdoc");
      if (scroll) frame.removeAttribute("scrolling");
      else frame.setAttribute("scrolling", "no");
      if (url) frame.src = url;
      else frame.removeAttribute("src");
    }
    // After the batch: its events may come after its props.
    if (html != null || url) queueMicrotask(() => this.webNavigation(el, "onLoadStart", true));
  }

  webNavigation(el, name, loading) {
    const state = { url: el._webUrl ?? "", title: el._webTitle ?? "", loading, canGoBack: false, canGoForward: false };
    if (el._events.has(name)) this.event(el._id, name, state);
    if (el._events.has("onNavigationStateChange")) this.event(el._id, "onNavigationStateChange", state);
  }

  webLoaded(el) {
    const frame = el._frameEl;
    if (!frame.getAttribute("src") && !frame.hasAttribute("srcdoc")) return;
    if (el._token) {
      const channel = new MessageChannel();
      channel.port1.onmessage = (e) => {
        if (el._events.has("onMessage")) this.event(el._id, "onMessage", { data: String(e.data) });
      };
      el._port?.close();
      el._port = channel.port1;
      frame.contentWindow?.postMessage(el._token, "*", [channel.port2]);
    }
    this.webNavigation(el, "onLoad", false);
  }

  op_video([, id, src, paused, muted, volume, rate, repeat, fit, controls, progressInterval]) {
    const el = this.view(id);
    const video = el?._video;
    if (!video) return;
    const url = src ? this.asset(src) : "";
    if (url !== el._src) {
      el._src = url;
      if (url) video.src = url;
      else video.removeAttribute("src");
    }
    el._progressInterval = progressInterval > 0 ? progressInterval : 250;
    video.muted = muted;
    video.volume = Math.min(1, Math.max(0, volume));
    video.playbackRate = rate;
    video.loop = repeat;
    video.controls = controls;
    video.style.objectFit = fit;
    if (paused) video.pause();
    else if (url && video.paused) {
      video.play().catch((err) => {
        // Browsers only start sound on their own after a user gesture.
        if (err?.name === "NotAllowedError" && !el._autoplayWarned) {
          el._autoplayWarned = true;
          console.warn("[nsh] the browser did not let the video play before a user gesture (muted videos can)");
        }
      });
    }
  }

  /** `<Video>`'s events from the media element's. */
  wireVideo(el, video) {
    const emit = (name, payload) => el._events.has(name) && this.event(el._id, name, payload);
    const progress = () => emit("onProgress", { currentTime: video.currentTime, duration: video.duration || 0 });
    const stopProgress = () => {
      clearInterval(el._progressTimer);
      el._progressTimer = 0;
    };
    let buffering = false;
    const buffer = (on) => {
      if (buffering === on) return;
      buffering = on;
      emit("onBuffer", { isBuffering: on });
    };
    video.addEventListener("loadstart", () => {
      if (video.getAttribute("src")) emit("onLoadStart");
    });
    video.addEventListener("loadedmetadata", () =>
      emit("onLoad", { duration: video.duration || 0, width: video.videoWidth, height: video.videoHeight }));
    video.addEventListener("playing", () => {
      buffer(false);
      stopProgress();
      if (el._events.has("onProgress")) el._progressTimer = setInterval(progress, el._progressInterval ?? 250);
    });
    video.addEventListener("pause", () => {
      stopProgress();
      progress();
    });
    video.addEventListener("waiting", () => buffer(true));
    video.addEventListener("canplay", () => buffer(false));
    video.addEventListener("ended", () => {
      stopProgress();
      progress();
      emit("onEnd");
    });
    video.addEventListener("error", () => {
      stopProgress();
      const e = video.error;
      if (e && video.getAttribute("src")) emit("onError", { message: e.message || `media error ${e.code}` });
    });
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
      case "zoomTo": if (el) this.zoomCommand(el, ...args); break;
      case "dialog": this.dialog(args[0], args[1], args[2], args[3], args[4] ?? [], args[5] ?? true, args[6] ?? false); break;
      case "toast": this.toast(args[0], args[1]); break;
      case "reload": {
        const frame = el?._frameEl;
        if (!frame) break;
        // The frame is another origin: it is loaded again rather than told to.
        if (frame.hasAttribute("srcdoc")) frame.srcdoc = frame.srcdoc;
        else if (frame.getAttribute("src")) frame.src = frame.getAttribute("src");
        this.webNavigation(el, "onLoadStart", true);
        break;
      }
      case "postMessage": el?._frameEl?.contentWindow?.postMessage(args[0], "*"); break;
      case "injectJavaScript":
      case "goBack":
      case "goForward":
        if (el?._port) el._port.postMessage(name === "injectJavaScript" ? { eval: args[0] } : { go: name === "goBack" ? -1 : 1 });
        else if (el?._frameEl && !el._noBridgeWarned) {
          el._noBridgeWarned = true;
          console.warn(`[nsh] web view: ${name} only reaches HTML sources (a page of another site is the browser's own)`);
        }
        break;
      case "seek": if (el?._video) el._video.currentTime = args[0]; break;
      case "setClipboard": this.writeClipboard(args[0]); break;
      case "readClipboard": this.readClipboard(args[0], args[1]); break;
      case "imageInfo": this.imageInfo(args[0], args[1]); break;
      case "statusBar": this.setThemeColor(args[0]); break;
      case "back": this.defaultBack(); break;
      case "announce":
        // Cleared first: the same text again is announced again.
        this.live.textContent = "";
        setTimeout(() => { this.live.textContent = args[0]; }, 50);
        break;
      case "a11yFocus":
        if (!el) break;
        if (!el.hasAttribute("tabindex")) {
          el.tabIndex = -1;
          el._a11yFocus = true;
        }
        el.focus();
        break;
      default: break;
    }
  }

  /** `StatusBar`'s `web={{ themeColor }}`: the page's theme color ("" for the page's own). */
  setThemeColor(color) {
    let meta = document.querySelector('meta[name="theme-color"]:not([media])');
    if (!meta && !color) return;
    if (!meta) {
      meta = document.createElement("meta");
      meta.name = "theme-color";
      meta._nshAdded = true;
      document.head.append(meta);
    }
    if (meta._nshOriginal === undefined) meta._nshOriginal = meta.getAttribute("content");
    if (color) meta.content = color;
    else if (meta._nshAdded) {
      meta.remove();
    } else if (meta._nshOriginal === null) meta.removeAttribute("content");
    else meta.content = meta._nshOriginal;
  }

  /**
   * `Clipboard.setString`. The async Clipboard API needs a secure context
   * (and, in some browsers, a user gesture); a copy command is the fallback.
   */
  writeClipboard(text) {
    const copy = () => {
      const area = document.createElement("textarea");
      area.value = text;
      area.style.cssText = "position:fixed;left:-9999px;top:0;opacity:0";
      document.body.append(area);
      area.select();
      try {
        document.execCommand("copy");
      } catch {
        // Nothing left to try.
      }
      area.remove();
    };
    if (navigator.clipboard?.writeText) navigator.clipboard.writeText(text).catch(copy);
    else copy();
  }

  /**
   * `Clipboard.getString` / `hasString`: browsers only tell by reading,
   * which may ask the user (Chromium asks once per site; Safari and Firefox
   * show a paste button), and fails in an insecure context. `hasString`
   * reads too: there is no way to ask without reading.
   */
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

  /**
   * `Alert.alert` / action sheets, drawn inside the surface: with the
   * desktop idiom as the Linux and macOS backends' cards (up to three
   * buttons in a row, the last regular one the default), otherwise as
   * iOS's alert and action sheet. Answers the button's index, -1 when
   * dismissed (a click beside it, or Escape, when cancelable); Escape
   * presses the cancel button if any (`keyDown`, `_cancel`).
   */
  dialog(dialogId, title, message, buttons, styles, cancelable, sheet) {
    const list = buttons.length ? buttons : ["OK"];
    const style = (i) => styles[i] ?? "default";
    const desktop = this.desktopUI;
    const backdrop = document.createElement("div");
    backdrop.className = sheet ? "nsh-dialog-backdrop nsh-sheet-backdrop" : "nsh-dialog-backdrop";
    const focused = document.activeElement;
    const restore = focused !== document.body && this.root.contains(focused) ? focused : null;
    let answered = false;
    // A tap's compatibility click lands after the press that opened the
    // dialog: only clicks pressed inside it (or from the keyboard) count.
    let down = null;
    backdrop.addEventListener("pointerdown", (e) => { down = e.target; }, true);
    const pressed = (e, el) => e.detail === 0 || (down !== null && el.contains(down));
    const close = (index) => {
      if (answered) return;
      answered = true;
      backdrop.remove();
      this.dialogs = this.dialogs.filter((d) => d !== backdrop);
      this.scopeFocus();
      if (restore?.isConnected) restore.focus({ preventScroll: true });
      this.event(ROOT_ID, "onDialog", { id: dialogId, index });
    };
    const cancel = list.findIndex((_, i) => style(i) === "cancel");
    backdrop._cancel = () => (cancel >= 0 ? close(cancel) : cancelable && close(-1));
    const button = (index) => {
      const b = document.createElement("button");
      b.textContent = list[index];
      if (style(index) !== "default") b.classList.add(style(index));
      b.addEventListener("click", (e) => pressed(e, b) && close(index));
      return b;
    };
    let focus = null;
    const box = document.createElement("div");
    box.setAttribute("role", "alertdialog");
    box.setAttribute("aria-modal", "true");
    box.setAttribute("aria-label", title ?? message ?? "");
    if (sheet) {
      box.className = "nsh-sheet";
      const options = document.createElement("div");
      if (title || message) {
        const head = document.createElement("div");
        head.className = "nsh-sheet-head";
        if (title) {
          const b = document.createElement("b");
          b.textContent = title;
          head.append(b);
        }
        if (message) head.append(message);
        options.append(head);
      }
      list.forEach((_, i) => {
        if (i === cancel) return;
        const b = button(i);
        focus ??= b;
        options.append(b);
      });
      if (options.firstChild) box.append(options);
      if (cancel >= 0) {
        const card = document.createElement("div");
        const b = button(cancel);
        focus ??= b;
        card.append(b);
        box.append(card);
      }
    } else {
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
      const actions = document.createElement("div");
      // Desktop: up to three in a row; iOS: two side by side.
      const row = desktop ? list.length <= 3 : list.length === 2;
      actions.className = `nsh-dialog-actions ${row ? "row" : "column"}`;
      // The last regular button is the default action (Return).
      let preferred = -1;
      for (let i = list.length - 1; i >= 0 && preferred < 0; i--) if (style(i) === "default") preferred = i;
      list.forEach((_, i) => {
        const b = button(i);
        if (i === preferred) {
          if (desktop) b.classList.add("default");
          focus = b;
        }
        actions.append(b);
      });
      focus ??= actions.firstChild;
      box.append(actions);
    }
    backdrop.addEventListener("click", (e) => e.target === backdrop && down === backdrop && cancelable && close(-1));
    backdrop.append(box);
    this.root.append(backdrop);
    (this.dialogs ??= []).push(backdrop);
    this.scopeFocus();
    focus?.focus({ preventScroll: true });
  }

  /** Dialogs and other system UI look as the desktop backends' with the desktop idiom, else as on phones. */
  get desktopUI() {
    return this.idiom === "desktop";
  }

  /** Classes system UI (dialogs, toasts, spinners) styles itself with. */
  updateRootClasses() {
    this.root.classList.toggle("nsh-dark", this.shownDark);
    this.root.classList.toggle("nsh-desktop", this.desktopUI);
  }

  /** A short message at the bottom (`Toast.show`), as the desktop backends show it; one at a time. */
  toast(text, long) {
    this.root.querySelector(":scope > .nsh-toast")?.remove();
    const el = document.createElement("div");
    el.className = "nsh-toast";
    el.textContent = text;
    el.style.opacity = "0";
    this.root.append(el);
    // Read out as `announce` does.
    this.live.textContent = "";
    setTimeout(() => { this.live.textContent = text; }, 50);
    requestAnimationFrame(() => { el.style.opacity = ""; });
    setTimeout(() => {
      el.style.opacity = "0";
      setTimeout(() => el.remove(), 200);
    }, long ? 3500 : 2000);
  }

  /** Registered native components are the native apps' own: the web has none, so the box stays empty (said once per name). */
  op_comp([, , name]) {
    if (this.missingComponents?.has(name)) return;
    (this.missingComponents ??= new Set()).add(name);
    const message = `native component "${name}" is not available on the web: its view stays empty`;
    console.warn("[nsh]", message);
    this.options.onLog?.("warn", message);
  }

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
    // A click leaves the keyboard focus where it is (in a field, too) and
    // selects nothing, as in native apps; fields, selectable text and
    // hosted pages and videos take it as usual.
    root.addEventListener("mousedown", (e) => {
      if (this.lastPointer === "touch" || this.editable(e.target)) return;
      e.preventDefault();
      // Which would also keep the keys from an app in an iframe.
      if (!document.hasFocus()) window.focus();
    });
    // No page menu over the app (nor iOS's callout on a long press); fields
    // and selectable text keep theirs.
    root.addEventListener("contextmenu", (e) => this.editable(e.target) || e.preventDefault());
    root.addEventListener("copy", (e) => this.copyText(e));
    root.addEventListener("wheel", (e) => this.wheel(e), { passive: false });
    // Safari's touchpad pinches (it sends no ctrl+wheel).
    root.addEventListener("gesturestart", (e) => this.gestureZoom(e, 0));
    root.addEventListener("gesturechange", (e) => this.gesture && this.gestureZoom(e, 1));
    root.addEventListener("gestureend", (e) => this.gesture && this.gestureZoom(e, 2));
    root.addEventListener("pointermove", (e) => {
      if (e.pointerType === "mouse") this.hover = [e.clientX, e.clientY];
    });
    root.addEventListener("focusin", () => (Player.active = this));
    window.addEventListener("keydown", (e) => this.keyDown(e));
    window.addEventListener("keyup", (e) => this.keyUp(e));
    // The mouse's back button goes back in the app (with history, as the
    // browser's back button does).
    root.addEventListener("mouseup", (e) => {
      if (e.button !== 3 || this.history) return;
      e.preventDefault();
      this.back();
    });
    this.live = document.createElement("div");
    this.live.className = "nsh-live";
    this.live.setAttribute("aria-live", "polite");
    if (this.history) this.installHistory();
  }

  /** Whether the browser handles presses at `target` itself: fields,
   * selectable text, hosted pages and videos. */
  editable(target) {
    return !!target?.closest?.(".nsh-field, .nsh-selectable, iframe, video") || !!this.textAt(target);
  }

  /** The text at `target` that is selectable by default (desktop idiom:
   * not set off, in nothing that takes presses or touches). */
  textAt(target) {
    if (!this.desktopUI) return null;
    const text = target?.closest?.(".nsh-autoselect");
    return text && !text.closest(".nsh-press") ? text : null;
  }

  /** This player takes the keys: the focus is in it, or it was the last
   * one used and the focus is on no element. */
  takesKeys() {
    const focus = document.activeElement;
    if (focus && focus !== document.body) return this.root.contains(focus);
    return (Player.active ?? this) === this;
  }

  // Keyboard

  keyDown(e) {
    if (!this.wasm || !this.takesKeys()) return;
    const focus = document.activeElement;
    const inRoot = focus && this.root.contains(focus);
    const editing = inRoot && !!focus.closest(".nsh-field, iframe");
    // The app's shortcuts first, where the focus is (`nsh/keymap`): matched
    // natively against the mounted table, then reported.
    const code = e.isComposing ? -1 : keyCode(e);
    if (code >= 0) {
      const mods = (e.ctrlKey ? 1 : 0) | (e.altKey ? 2 : 0) | (e.shiftKey ? 4 : 0) | (e.metaKey ? 8 : 0);
      const found = this.wasm.nsh_key(code, mods, editing ? 1 : 0, e.repeat ? 1 : 0);
      if (found !== -1) {
        e.preventDefault();
        e.stopPropagation();
        if (found >= 0) this.call((w) => w.nsh_key_command(found));
        return;
      }
    }
    if (e.defaultPrevented || e.isComposing) return;
    const plain = !e.ctrlKey && !e.altKey && !e.metaKey;
    const dialog = this.dialogs?.[this.dialogs.length - 1];
    if (dialog) {
      if (e.key === "Escape") {
        e.preventDefault();
        dialog._cancel();
      }
      return;
    }
    if (editing) {
      // Escape leaves the field.
      if (e.key === "Escape" && plain && focus.classList.contains("nsh-field")) {
        e.preventDefault();
        focus.blur();
      }
      return;
    }
    if (this.selectionKey(e)) return;
    if (inRoot && focus._pressable && plain && !e.shiftKey && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      if (e.repeat) return;
      // Return presses at once, Space when it is released, as buttons do.
      this.keyPress = { press: focus, states: this.ancestors(focus).filter((v) => v._tracked & PRESSED) };
      this.pressIn(this.keyPress);
      for (const v of this.keyPress.states) this.setState(v, PRESSED, true);
      if (e.key === "Enter") this.keyPressEnd(true);
      return;
    }
    const backKey = e.key === "BrowserBack" || (e.altKey && !APPLE && e.key === "ArrowLeft") || (e.metaKey && APPLE && e.key === "[");
    if ((e.key === "Escape" && plain) || backKey) {
      // With history the browser goes back itself on its keys (`popState`).
      if (backKey && this.history) return;
      if (this.back()) e.preventDefault();
      return;
    }
    if (plain && this.scrollKey(e, inRoot ? focus : null)) e.preventDefault();
  }

  keyUp(e) {
    if (e.key === " " && this.keyPress) this.keyPressEnd(true);
  }

  keyPressEnd(inside) {
    const p = this.keyPress;
    this.keyPress = null;
    this.pressOut(p, inside);
    for (const v of p.states) this.setState(v, PRESSED, false);
  }

  /** The selectable text `el` is in. */
  selectableText(el) {
    return el?.closest(".nsh-selectable") ?? this.textAt(el);
  }

  /** What copying the selection in text gives: its runs in order, each
   * line's break after its last run (`_after`) and a newline between
   * texts; absolutely placed runs would each be a line of their own. */
  copyText(e) {
    const sel = getSelection();
    if (e.target.closest?.(".nsh-field")) return;
    if (!sel?.rangeCount || sel.isCollapsed || !this.root.contains(sel.anchorNode)) return;
    const range = sel.getRangeAt(0);
    let out = "";
    let last = null;
    let found = false;
    for (const run of this.root.querySelectorAll(".nsh-run")) {
      // Text that cannot be selected is not copied, as by the browser.
      if (!range.intersectsNode(run) || !this.selectableText(run)) continue;
      const node = run.firstChild;
      if (!node) continue;
      const text = node.data;
      const from = range.startContainer === node ? range.startOffset : 0;
      const to = range.endContainer === node ? range.endOffset : text.length;
      if (from >= to) continue;
      found = true;
      if (last && last !== run.parentNode && !out.endsWith("\n")) out += "\n";
      last = run.parentNode;
      out += text.slice(from, to);
      if (to === text.length && run._after) out += run._after;
    }
    if (!found) return;
    e.preventDefault();
    e.clipboardData.setData("text/plain", out.replace(/\s+$/, ""));
  }

  /** Select all and Escape in selectable text with a selection: within it. */
  selectionKey(e) {
    const sel = getSelection();
    const anchor = sel?.anchorNode;
    const text = sel?.rangeCount && !sel.isCollapsed && anchor && this.root.contains(anchor)
      ? this.selectableText(anchor.nodeType === 1 ? anchor : anchor.parentElement)
      : null;
    if (!text) return false;
    if (e.key === "a" && (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) {
      e.preventDefault();
      sel.selectAllChildren(text);
      return true;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      sel.removeAllRanges();
      return true;
    }
    return false;
  }

  /** Arrows (40 points), Page Up / Down and Space / Shift+Space (a page),
   * Home / End scroll the scroll view around the focus, else the one under
   * the pointer. */
  scrollKey(e, focus) {
    const k = e.key;
    const horizontal = k === "ArrowLeft" || k === "ArrowRight";
    if (!["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown", " ", "Home", "End"].includes(k)) return false;
    if (e.shiftKey && k !== " ") return false;
    let start = focus && focus !== this.root ? focus : null;
    if (!start && this.hover) start = document.elementFromPoint(this.hover[0], this.hover[1]);
    const chain = this.scrollChain(start);
    const el = chain.find((v) => !!v._horizontal === horizontal) ?? (horizontal ? null : chain[0]);
    if (!el) return false;
    const h = !!el._horizontal;
    const pos = h ? el.scrollLeft : el.scrollTop;
    const max = h ? el.scrollWidth - el.clientWidth : el.scrollHeight - el.clientHeight;
    const page = (h ? el.clientWidth : el.clientHeight) * 0.875;
    const d = { ArrowUp: -40, ArrowLeft: -40, ArrowDown: 40, ArrowRight: 40, PageUp: -page, PageDown: page,
      " ": e.shiftKey ? -page : page, Home: -pos, End: max - pos }[k];
    if (el._snapPoints) this.snapStep(el, Math.sign(d));
    else this.scrollSmoothly(el, h ? d : 0, h ? 0 : d, true);
    return true;
  }

  // Wheels and touchpads

  /** The scroll views around `target` that scroll, innermost first. */
  scrollChain(target) {
    const view = target ? this.viewOf(target) : null;
    return view ? this.ancestors(view).filter((v) => v._kind === "scroll" && v._behavior?.enabled !== false) : [];
  }

  /**
   * A wheel or touchpad scroll goes to the innermost scroll view along its
   * axis that can move that way (a vertical wheel scrolls a horizontal view
   * when no vertical one can), and stays on it until the scroll ends
   * (events 200 ms apart). Notches glide; Ctrl + wheel (a touchpad's pinch)
   * pinches the view under the pointer and never zooms the page.
   */
  wheel(e) {
    if (e.ctrlKey) {
      e.preventDefault();
      // Over a zoomable scroll view it zooms that, else it pinches.
      if (!this.wheelZoom(e)) this.wheelPinch(e);
      return;
    }
    const unit = e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? this.size[1] : 1;
    let dx = e.deltaX * unit;
    let dy = e.deltaY * unit;
    if (e.shiftKey && !dx) [dx, dy] = [dy, 0];
    const now = performance.now();
    let lock = this.wheelLock;
    if (!lock || now - lock.t > 200 || !lock.el.isConnected) {
      const chain = this.scrollChain(e.target);
      const horizontal = Math.abs(dx) > Math.abs(dy);
      const can = (v, d) => {
        const h = !!v._horizontal;
        const pos = h ? v.scrollLeft : v.scrollTop;
        const max = h ? v.scrollWidth - v.clientWidth : v.scrollHeight - v.clientHeight;
        return d < 0 ? pos > 0.5 : d > 0 && pos < max - 0.5;
      };
      const el = chain.find((v) => !!v._horizontal === horizontal && can(v, horizontal ? dx : dy))
        ?? (horizontal ? null : chain.find((v) => v._horizontal && can(v, dy)));
      // Nothing to scroll: the page around may.
      this.wheelLock = lock = el ? { el, total: 0, snapped: false } : null;
      if (!lock) return;
    }
    e.preventDefault();
    lock.t = now;
    const el = lock.el;
    const d = el._horizontal ? dx || dy : dy;
    if (el._snapPoints) {
      // One snap point per scroll.
      lock.total += d;
      if (!lock.snapped && Math.abs(lock.total) >= 20) {
        lock.snapped = true;
        this.snapStep(el, Math.sign(lock.total));
      }
      return;
    }
    // Wheel notches glide; touchpads move with the fingers.
    const smooth = e.deltaMode !== 0 || Math.abs(d) >= 40;
    this.scrollSmoothly(el, el._horizontal ? d : 0, el._horizontal ? 0 : d, smooth);
  }

  /** Moves `el` by (`dx`, `dy`), gliding there when `smooth`; once it
   * settles (no more input for 200 ms), `onScrollEnd`. */
  scrollSmoothly(el, dx, dy, smooth) {
    let a = el._glide;
    if (!a) {
      a = el._glide = { x: el.scrollLeft, y: el.scrollTop, frame: 0, timer: 0, last: 0 };
      this.drive(el, 1);
    }
    a.x = Math.min(Math.max(a.x + dx, 0), el.scrollWidth - el.clientWidth);
    a.y = Math.min(Math.max(a.y + dy, 0), el.scrollHeight - el.clientHeight);
    const settle = () => {
      if (a.frame || a.timer) return;
      el._glide = null;
      this.drive(el, -1);
      this.ownEnd(el);
    };
    if (!smooth) {
      cancelAnimationFrame(a.frame);
      a.frame = 0;
      el.scrollLeft = a.x;
      el.scrollTop = a.y;
    } else if (!a.frame) {
      a.last = performance.now();
      // Where the glide is: browsers round the offsets they are given.
      a.at = [el.scrollLeft, el.scrollTop];
      const step = (now) => {
        const k = 1 - Math.pow(0.75, (now - a.last) / 16.7);
        a.last = now;
        const x = a.at[0] + (a.x - a.at[0]) * k;
        const y = a.at[1] + (a.y - a.at[1]) * k;
        const done = Math.abs(a.x - x) < 0.5 && Math.abs(a.y - y) < 0.5;
        a.at = done ? [a.x, a.y] : [x, y];
        el.scrollLeft = a.at[0];
        el.scrollTop = a.at[1];
        a.frame = done ? 0 : requestAnimationFrame(step);
        if (done) settle();
      };
      a.frame = requestAnimationFrame(step);
    }
    clearTimeout(a.timer);
    a.timer = setTimeout(() => {
      a.timer = 0;
      settle();
    }, 200);
  }

  /** The snap point of a snapping view next to where it is towards `dir`
   * (0: the nearest). */
  snapPoint(el, dir) {
    const pos = el._horizontal ? el.scrollLeft : el.scrollTop;
    const points = el._snapPoints;
    if (dir > 0) return points.find((p) => p > pos + 1);
    if (dir < 0) return [...points].reverse().find((p) => p < pos - 1);
    return points.reduce((a, b) => (Math.abs(b - pos) < Math.abs(a - pos) ? b : a));
  }

  /** Scrolls a snapping view to its next snap point towards `dir`. */
  snapStep(el, dir) {
    if (dir) this.snapTo(el, this.snapPoint(el, dir));
  }

  /** Glides to snap point `target` as the momentum of a scroll (the
   * browser's `scrollend` ends it); false when it is there. */
  snapTo(el, target) {
    const h = !!el._horizontal;
    if (target == null || Math.abs(target - (h ? el.scrollLeft : el.scrollTop)) < 1) return false;
    el._momentum = true;
    this.scrollEvent(el, "onMomentumScrollBegin");
    el.scrollTo({ [h ? "left" : "top"]: target, behavior: "smooth" });
    return true;
  }

  /** Ctrl + wheel pinches the view under the pointer (native values),
   * ending when the wheel stops for 150 ms. */
  wheelPinch(e) {
    let t = this.wheelPinchState;
    if (!t) {
      const view = this.viewOf(e.target);
      const el = view && this.ancestors(view).find((v) => v._gestures?.pinch);
      if (!el) return;
      t = this.wheelPinchState = { el, scale: 1, timer: 0 };
      this.call((w) => w.nsh_transform_gesture(el._id, 0, 0, 1));
    }
    t.scale *= Math.exp(-e.deltaY / 100);
    this.call((w) => w.nsh_transform_gesture(t.el._id, 0, 1, t.scale));
    clearTimeout(t.timer);
    t.timer = setTimeout(() => {
      this.wheelPinchState = null;
      this.call((w) => w.nsh_transform_gesture(t.el._id, 0, 2, t.scale));
    }, 150);
  }

  /** Counts what drives `el`'s scrolling here (mouse drags, glides, wheels,
   * keys): meanwhile the browser's `scrollend` is not the end. */
  drive(el, n) {
    el._driven = Math.max(0, (el._driven ?? 0) + n);
  }

  /** `onScrollEnd` of a scroll driven here; the browser's `scrollend`
   * that may follow at the same offset is not another. */
  ownEnd(el) {
    el._ownEnd = [el.scrollLeft, el.scrollTop];
    this.scrollEvent(el, "onScrollEnd");
  }

  scrollEvent(el, name) {
    if (el._events.has(name)) this.event(el._id, name, { x: el.scrollLeft, y: el.scrollTop });
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
    this.lastPointer = e.pointerType;
    Player.active = this;
    if (e.button > 0) return;
    // A mouse drag selects in selectable text and fields instead of scrolling.
    const selects = this.editable(e.target);
    // A press elsewhere clears a selection in text, as on a web page (the
    // browser does not, as presses here select nothing).
    const sel = getSelection();
    if (!selects && e.pointerType === "mouse" && sel?.rangeCount && !sel.isCollapsed && this.root.contains(sel.anchorNode)) {
      sel.removeAllRanges();
    }
    const target = this.viewOf(e.target);
    if (!target) return;
    const isField = e.target.classList?.contains("nsh-field");
    const chain = this.ancestors(target);
    const [x, y] = this.point(e);
    // Mouse drags scroll here (browsers only scroll touches), and so do
    // touches in zoomable views, which pan and pinch here.
    const scrollView = isField ? null : chain.find((v) => v._kind === "scroll") ?? null;
    const p = {
      x, y, startX: x, startY: y, t: performance.now(), vx: 0, vy: 0,
      touchPath: chain.filter((v) => v._events.has("onTouch")).map((v) => v._id),
      press: isField ? null : chain.find((v) => PRESS_EVENTS.some((n) => v._events.has(n)) && !v._props?.disabled) ?? null,
      pan: chain.find((v) => v._gestures?.pan || v._events.has("onPanStart") || v._events.has("onPanEnd")) ?? null,
      scroll: !selects && scrollView && (e.pointerType === "mouse" || this.zooms(scrollView)) ? scrollView : null,
      // Touch scrolling is the browser's, reported from here (`touchScroll`).
      scrolls: e.pointerType === "touch" ? chain.filter((v) => v._kind === "scroll") : null,
      zoom: chain.find((v) => this.zooms(v) && v._behavior.pinch) ?? null,
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
        p.scrollStart = [p.scroll.scrollLeft, p.scroll.scrollTop - (p.scroll._pull ?? 0)];
        cancelAnimationFrame(p.scroll._pullAnimation);
        cancelAnimationFrame(p.scroll._zoomAnimation);
        p.scroll._pulling = this.refreshable(p.scroll);
        this.drive(p.scroll, 1);
        this.scrollEvent(p.scroll, "onScrollBeginDrag");
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
      const both = this.zooms(s);
      s.scrollLeft = p.scrollStart[0] - (s._horizontal || both ? tx : 0);
      if (s._pulling) this.dragPull(s, p.scrollStart[1] - ty);
      else s.scrollTop = p.scrollStart[1] - (s._horizontal && !both ? 0 : ty);
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
    if (cancelled && p.scrolls?.length) this.touchScroll(p, this.point(e));
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
      this.scrollEvent(s, "onScrollEndDrag");
      if (s._pulling) {
        s._pulling = false;
        if (s._pull > 0) {
          this.drive(s, -1);
          this.releasePull(s);
          return;
        }
      }
      const v = s._horizontal ? -p.vx : -p.vy;
      if (this.zooms(s)) this.glide(s, -p.vy, -p.vx);
      else if (s._snapPoints) {
        // Snapping views settle on the next snap point along a flick, else
        // on the nearest one; snapping comes back once they are there.
        this.drive(s, -1);
        s._restoreSnap = true;
        if (!this.snapTo(s, this.snapPoint(s, Math.abs(v) > 300 ? Math.sign(v) : 0))) this.scrollEnded(s);
      } else this.glide(s, v);
    } else if (p.press) {
      const rect = p.press.getBoundingClientRect();
      const inside = e.clientX >= rect.left - 8 && e.clientX <= rect.right + 8 && e.clientY >= rect.top - 8 && e.clientY <= rect.bottom + 8;
      this.pressOut(p, inside && !cancelled);
    }
  }

  /** The browser took a touch to scroll (`pointercancel`): the drag of
   * the scroll view along the way it moved, until the fingers lift. */
  touchScroll(p, [x, y]) {
    const horizontal = Math.abs(x - p.startX) > Math.abs(y - p.startY);
    const el = p.scrolls.find((v) => !!v._horizontal === horizontal) ?? p.scrolls[0];
    if (el._touchDrag) return;
    el._touchDrag = true;
    el._momentum = false;
    (this.touchDrags ??= new Set()).add(el);
    this.scrollEvent(el, "onScrollBeginDrag");
    if (!this.touchEndListener) {
      this.touchEndListener = (e) => {
        if (e.touches.length) return;
        for (const v of this.touchDrags) {
          v._touchDrag = false;
          v._released = performance.now();
          this.scrollEvent(v, "onScrollEndDrag");
          // Without a glide no scroll follows (nor, sometimes, `scrollend`).
          setTimeout(() => v._released && !v._momentum && this.scrollEnded(v), 200);
        }
        this.touchDrags.clear();
      };
      window.addEventListener("touchend", this.touchEndListener);
      window.addEventListener("touchcancel", this.touchEndListener);
    }
  }

  /** Momentum after a mouse drag (touch scrolling is the browser's); `across`: a zoomable view's other axis. */
  glide(s, velocity, across) {
    let v = Math.abs(velocity) > 80 ? velocity / 1000 : 0;
    let w = across != null && Math.abs(across) > 80 ? across / 1000 : 0;
    let last = performance.now();
    const momentum = v !== 0 || w !== 0;
    const end = () => {
      this.drive(s, -1);
      this.snapMarkers(s);
      if (momentum) this.scrollEvent(s, "onMomentumScrollEnd");
      this.ownEnd(s);
    };
    if (!v && !w) {
      end();
      return;
    }
    this.scrollEvent(s, "onMomentumScrollBegin");
    const step = (now) => {
      const dt = now - last;
      last = now;
      if (across != null) {
        s.scrollTop += v * dt;
        s.scrollLeft += w * dt;
      } else if (s._horizontal) s.scrollLeft += v * dt;
      else s.scrollTop += v * dt;
      v *= Math.pow(0.995, dt);
      w *= Math.pow(0.995, dt);
      if (Math.abs(v) > 0.02 || Math.abs(w) > 0.02) requestAnimationFrame(step);
      else end();
    };
    requestAnimationFrame(step);
  }

  startTwoFinger() {
    const [a, b] = [...this.pointers.values()];
    const el = a.pinch ?? b.pinch;
    if (!el) {
      const zoom = a.zoom ?? b.zoom;
      if (zoom) this.startPinchZoom(zoom, a, b);
      return;
    }
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

  /** Two fingers on a zoomable scroll view: the content point between them stays between them as they spread and move. */
  startPinchZoom(el, a, b) {
    for (const p of [a, b]) this.pressOut(p, false);
    cancelAnimationFrame(el._zoomAnimation);
    const r = el.getBoundingClientRect();
    const root = this.root.getBoundingClientRect();
    const fx = (a.x + b.x) / 2 + root.left - r.left;
    const fy = (a.y + b.y) / 2 + root.top - r.top;
    this.twoFinger = { zoom: el, dist: Math.hypot(a.x - b.x, a.y - b.y), scale: el._zoom, point: this.contentPoint(el, fx, fy) };
  }

  moveTwoFinger() {
    const t = this.twoFinger;
    if (t.zoom) {
      const [a, b] = [...this.pointers.values()];
      if (!a || !b) return;
      const r = t.zoom.getBoundingClientRect();
      const root = this.root.getBoundingClientRect();
      t.focus = [(a.x + b.x) / 2 + root.left - r.left, (a.y + b.y) / 2 + root.top - r.top];
      const scale = this.limitZoom(t.zoom, (t.scale * Math.hypot(a.x - b.x, a.y - b.y)) / Math.max(1, t.dist), true);
      this.zoomContent(t.zoom, scale, t.point[0], t.point[1], t.focus[0], t.focus[1]);
      return;
    }
    const [scale, rotation] = this.twoFingerAmounts();
    if (t.el._gestures?.pinch) this.call((w) => w.nsh_transform_gesture(t.el._id, 0, 1, scale));
    if (t.el._gestures?.rotation) this.call((w) => w.nsh_transform_gesture(t.el._id, 1, 1, rotation));
  }

  endTwoFinger() {
    const t = this.twoFinger;
    this.twoFinger = null;
    if (t.zoom) {
      const f = t.focus ?? [t.zoom.clientWidth / 2, t.zoom.clientHeight / 2];
      this.zoomEnded(t.zoom, f[0], f[1]);
      this.pointers.clear();
      return;
    }
    const scale = t.scale ?? 1;
    const rotation = t.rotation ?? 0;
    if (t.el._gestures?.pinch) this.call((w) => w.nsh_transform_gesture(t.el._id, 0, 2, scale));
    if (t.el._gestures?.rotation) this.call((w) => w.nsh_transform_gesture(t.el._id, 1, 2, rotation));
    this.pointers.clear();
  }
}

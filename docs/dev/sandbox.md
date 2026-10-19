# Sandbox and capabilities

Each surface is one QuickJS VM. A bundle holds only the app's code (NSH's runtime is native), so nothing in JS is
trusted: every limit and check is enforced in Rust, never in JS.

- **Limits** (`nsh_js::Limits`, `EngineConfig::limits`, per VM): the QuickJS heap (`Limits::default`: 256 MB, for apps embedded in another app;
  `Limits::standalone_desktop`: 1 GB, for the desktop hosts and runners and Catalyst; `Limits::standalone_mobile`: 512 MB, for
  the iOS and Android hosts and runners) and stack, a
  time budget per call from Rust into JS (`sandbox.rs`: loading the bundle 10 s, any other call 2 s — one event with the
  rendering it causes, one list bind, one timer, one frame's callbacks, the microtask drain; nested calls share the
  outermost budget), and quotas for what the heap limit does not cover: changes at once (the op queue,
  `max_op_values`: ops are Rust memory), ops per canvas drawing, nodes in the core (`Core::set_max_nodes`; each may hold a native view) and dense
  node ids (an id far past the ids in use would grow the id table: `ApplyError::IdOutOfRange`).
- An overrun call is cut off by the interrupt handler with an exception no `catch` swallows, reported with the budget it
  exceeded; the VM goes on. Running out of memory or going over a quota **stops the VM** (`Host::fatal`), also when the
  app catches the error: the heap's allocator (`HeapAllocator`) enforces the limit and records reaching it, since QuickJS
  reports running out in several ways (an `InternalError`, a `null` exception). It is reported
  once (`MountSink::report_error`, the embedder's error callback), no JS runs after that and the views stay as they were.
- **Hardening** (`Policy::harden`, on unless the bundle came from a dev server): `eval` is deleted, the `Function`,
  `AsyncFunction` and generator constructors throw, and the shared prototypes (`Object`, `Array`, `Function`, `String`,
  `Number`, `Promise`, `Map`, typed arrays, ...) are frozen so one module cannot change them for the others.
  `Error.prototype` stays writable (subclasses set `this.name`). Their commonly overridden members (`constructor`,
  `toString`, `valueOf`, `toJSON`, ...) become accessors whose setter defines the property on the object assigned to:
  without that, `Sub.prototype.constructor = Sub` or `obj.toString = …` throws (the "override mistake"), which breaks
  libraries such as axios. Hot updates are evaluated from Rust and need none of it.
- **Capabilities** (`nsh_core::access`): an app declares what it may reach in `nsh.json` `"capabilities"` — `network`
  (`hosts`: `*`, `*.example.com` or a host, any method; `http`: a host with `methods` / `schemes` / `pathPrefix`),
  `webview` (`hosts` a web view may show and navigate to), `statusBar`, `clipboard` (`nsh/clipboard`: reading and
  writing the system clipboard; the copy menu of selectable text is the user's own act and needs none), `storage`
  (`nsh/storage`: the app's key-value storage on the device). Everything not listed is denied and unknown keys
  are build errors; an app without the key may do everything (a standalone app is its developer's own). The embedding app
  may grant less (`NshAppView.grantCapabilities` / `NshView.setGrantedCapabilities`, the same JSON): a request must be
  allowed by both (`Access`). The declaration travels with the bundle (`capabilities.json` next to it, written by
  `nsh build` / `nsh package`; the dev protocol's `Bundle` message, so development enforces the same capabilities; a change
  reloads instead of patching) into `Policy::declared`.
- Enforced in Rust: `fetch` in the engine (`access.rs`; refused requests never reach the network, and redirects are
  followed by the engine with `follow_redirects` off for the platform, so every hop is checked — hops are issued on the
  engine thread when a hop's head arrives; a streamed request body is not sent again, so a 307 / 308 then fails); remote images, placeholders, canvas images, `Image.getSize` / `prefetch`, video sources, web view
  sources, the status bar and the clipboard in the core (`Core::check_prop`; a refused image fails with `onError`, clipboard reads resolve empty, the rest show nothing;
  storage in `nsh_core::storage`, whose calls reject;
  each refusal is logged once, target `js`, saying which key to add); top-level web view navigations on the platforms
  (`WKNavigationDelegate decidePolicyForNavigationAction` / `WebViewClient.shouldOverrideUrlLoading` → `Native.allowsWebview`).
  `asset:` URIs are always allowed. Host checks take the host after the last `@` (`https://ok.com@evil.io` is evil.io).
- **Embedding limits**: `NshAppView.limits` (`NshLimits`) / `NshView.setLimits` (heap MB, stack KB, load / call
  budgets, node quota; unset / 0 keeps a default), from the next load; on GPUI `AppOptions::limits` /
  `AppView::set_limits` (a whole `Limits`), likewise from the next load.

Implementation: `crates/nsh-js/src/sandbox.rs` (limits, budgets, hardening), `crates/nsh-core/src/access.rs`
(capabilities), `crates/nsh-engine/src/access.rs` (`fetch` and redirects).

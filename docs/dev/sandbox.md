# Sandbox and capabilities

Each surface is one QuickJS VM. A bundle holds only the app's code (NSH's runtime is native), so nothing in JS is
trusted: every limit and check is enforced in Rust, never in JS.

- **Who sets the limits.** Only apps embedded in another app have limits, and the embedding app decides them
  (`nsh_core::js::HostLimits` over `Limits::default`: 256 MB heap, 4 MB stack, 10 s to load, 2 s per call, 200 000
  nodes); `nsh.json` has no say. An app on its own (every platform's hosts and runners, a web package, `nsh run`
  without `--limits`) is its developer's code, as a native app's is, and runs with `Limits::standalone()`: no heap
  limit (0), no time budget (`Duration::MAX`: no deadline), no quota (`usize::MAX`), only the interpreter stack's
  4 MB, which turns deep recursion into a `RangeError` instead of a crash of the process (it is also the most an
  embedder may set: the engine thread's stack is 8 MB). `nsh run` / `nsh dev --limits [memory=..,stack=..,load=..,call=..,nodes=..]`
  run the app as an embedding app setting those would (alone: the embedding defaults): the dev server sends the spec in
  its bundle message (`limits`) and the dev hosts start the app with `Limits::development(spec)`. On the web the page
  decides (`run(code, { limits })`, `nsh_bundle_limits`); the website's playground runs its visitors' code with the
  embedding defaults, since the engine runs on the page's main thread.
- **Limits** (`nsh_js::Limits`, `EngineConfig::limits`, per VM): the QuickJS heap and stack, a
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
  writing the system clipboard; the copy menu of selectable text is the user's own act and needs none). `nsh/storage`
  is no capability: an app's storage holds only its own data, and the host decides where it is kept. Everything not listed is denied and unknown keys
  are build errors (`Capabilities::from_json`); a signed package's declaration is read with `from_json_lenient`, as a
  newer signer may have written it: an unknown capability is not granted, an unknown key inside one drops that
  capability or `http` grant (it may be a restriction), so unknown keys never widen access; an app without the key may do everything (a standalone app is its developer's own). The embedding app
  may grant less (`NshAppView.grantCapabilities` / `NshView.setGrantedCapabilities`, the same JSON): a request must be
  allowed by both (`Access`). The declaration travels with the bundle (`capabilities.json` next to it, written by
  `nsh build` / `nsh package`; the dev protocol's `Bundle` message, so development enforces the same capabilities; a change
  reloads instead of patching) into `Policy::declared`.
- Enforced in Rust: `fetch` in the engine (`access.rs`; refused requests never reach the network, and redirects are
  followed by the engine with `follow_redirects` off for the platform, so every hop is checked — hops are issued on the
  engine thread when a hop's head arrives; a streamed request body is not sent again, so a 307 / 308 then fails); remote images, placeholders, canvas images, `Image.getSize` / `prefetch`, video sources, web view
  sources, the status bar and the clipboard in the core (`Core::check_prop`; a refused image fails with `onError`, clipboard reads resolve empty, the rest show nothing;
  each refusal is logged once, target `js`, saying which key to add); top-level web view navigations on the platforms
  (`WKNavigationDelegate decidePolicyForNavigationAction` / `WebViewClient.shouldOverrideUrlLoading` → `Native.allowsWebview`).
  `asset:` URIs are always allowed. Host checks take the host after the last `@` (`https://ok.com@evil.io` is evil.io).
- **Embedding limits**: `NshAppView.limits` (`NshLimits`) / `NshView.setLimits` (heap MB, stack KB, load / call
  budgets, node quota; unset / 0 keeps a default; both through `HostLimits`, as `--limits` is), from the next load;
  on macOS, Windows, Linux and GPUI `AppOptions::limits` / `AppView::set_limits` (a whole `Limits`), likewise from
  the next load. nsh's own Android activities run apps on their own (`NshView.useStandaloneLimits`, package-private).

Implementation: `crates/nsh-js/src/sandbox.rs` (limits, budgets, hardening), `crates/nsh-core/src/access.rs`
(capabilities), `crates/nsh-engine/src/access.rs` (`fetch` and redirects).

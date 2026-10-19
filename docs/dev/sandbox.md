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
  (`hosts`: `*`, `*.example.com` or a host; `http`: scoped scheme/host/port/method/path rules),
  `webview` (`hosts` a web view may show and navigate to), `statusBar`, `process` (`execute`: exact executable names or absolute paths; `exit`: host exit requests; `env`: additional
  visible environment names), `fs` (`read` / `write`: trusted path roots), `clipboard` (`nsh/clipboard`: reading and
  writing the system clipboard; the copy menu of selectable text is the user's own act and needs none). `nsh/storage`
  is no capability: an app's storage holds only its own data, and the host decides where it is kept. Everything not listed is denied and unknown keys
  are build errors; an app without the key retains unrestricted network, webview, status bar and clipboard access.
  Process execution and filesystem access always require explicit declarations, including standalone apps. The embedding app
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
- **Desktop system APIs**: `process` and `fs/promises` (the only process and file APIs) check the intersection of the
  declaration and the optional host grant in Rust. Process commands match exactly: a grant for `codex` grants that
  command, while `/usr/bin/codex` requires its own entry. The runtime launches the executable directly with an
  argument array, in the app data directory or a `cwd` within an `fs.write` root, with a cleared environment
  refilled from the host's `PATH`, `HOME`, `USERPROFILE`, `SystemRoot`, `TEMP`, `TMP`, `TMPDIR`, `LANG`, `LC_*`
  and the names `process.env` grants (`nsh_core::runtime::process`). A child leads its own process group; its exit is noticed
  without reaping it (`waitid(WNOWAIT)` on Linux, a kqueue `NOTE_EXIT` on Apple platforms), so the group is only
  signalled while the leader's pid is still taken. VM teardown sends every child SIGKILL before joining any
  supervisor. Windows ends trees with one `taskkill /T /F` per child, started without waiting. Filesystem checks
  reject traversal and canonicalize the nearest existing ancestor before checking containment in each granted root,
  including symlinks and paths whose final components will be created. The operation then runs in the granted
  root, opened once (on Unix component by component with `O_NOFOLLOW` from `/`) as a cap-std `Dir`, so no symlink
  leads it outside; operations on links (`lstat`, `readlink`, `exists`, `unlink`, `rename`, `rmdir`) resolve only
  the parent. Files are opened non-blocking and must be regular files. One VM has at most 1024 operations and
  64 MiB of file data in flight (`nsh_core::runtime::fs`, counted in the VM's `Quotas`). `${dataDir}`, `${pluginDir}` and
  `${homeDir}` are resolved from trusted host configuration. Relative roots are anchored at the host plugin
  directory; an unconfigured anchor grants nothing. Read and write grants are separate. Environment visibility
  uses the default metadata-name list in `default-env` (language, time zone, terminal and desktop kind only: no
  identity, paths or proxies, which can carry credentials), plus explicit case-insensitive `process.env` names
  allowed by both policies. `process.exit` requests a host handler and never exits the embedding process itself.
- **Embedding limits**: `NshAppView.limits` (`NshLimits`) / `NshView.setLimits` (heap MB, stack KB, load / call
  budgets, node quota; unset / 0 keeps a default), from the next load; on GPUI `AppOptions::limits` /
  `AppView::set_limits` (a whole `Limits`), likewise from the next load.

Implementation: `crates/nsh-js/src/sandbox.rs` (limits, budgets, hardening), `crates/nsh-core/src/access.rs`
(capabilities), `crates/nsh-engine/src/access.rs` (`fetch` and redirects).

Standard networking: global `fetch` keeps engine checks and redirect handling. Scoped `network.http` rules use
`scheme` (HTTPS by default), `host`, optional `port`, required GET/POST `methods`, and `paths` / `path_prefixes`.
Paths match exactly; prefixes match slash-delimited boundaries. `{host, methods}` grants no paths. A scoped rule
names one exact host: a wildcard there is a parse error (it would compare exactly and match nothing), so host
patterns go in `hosts` or legacy rules. Legacy rules are explicitly selected by `schemes` or `pathPrefix`; mixing
rule forms is rejected and serialization preserves the form. WebSockets (the global and the `websocket` module)
go through `Access::allows_websocket`: a handshake is an HTTP GET, so `ws` / `wss` are checked as GETs over `http` /
`https` (or under their own scheme, for legacy rules listing it), on the parsed URL the handshake then connects
to, before DNS; redirects are rejected; both APIs share one cap of 32 per VM (the `Quotas` kind `runtime::WEBSOCKETS`). Raw TCP
`net.connect` requires an exact `network.sockets` entry (`"host:port"`, no patterns, port 1 to 65535): a socket
bypasses every HTTP rule, so neither `hosts` nor `http` grants imply one, and `Capabilities::all()` (apps without a
declaration) has none. Sockets are capped at 32 per VM (the call's `System` quotas) and 4 MiB of pending writes each;
`close()` ends pending reads and writes. `zlib` decompression output is capped at 64 MiB or the heap limit if
smaller (`standard/zlib.rs` wraps LLRT's module and clamps `maxOutputLength`), as it is native memory until it
becomes a buffer. `fs/promises`, `process` and `net` check the call's `System` (the VM's `Access`, data directory,
`TrustedPaths`) in Rust before any work starts, and count their limits in its quotas. The standard catalog is
eleven bare specifiers and nothing else: `node:` prefixes and other Node modules (`fs`, `child_process`,
`events`, ...) do not import, so no module reaches files, processes or sockets around these checks. See
`website/modules/standard-modules.md` for the public surface.

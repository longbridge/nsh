---
title: Sandbox and Capabilities
description: The limits every NSH app runs under (memory, time, nodes), how release builds are hardened, and the capabilities an app declares in nsh.json and a host app grants.
order: 21
group: delivery
---

# Sandbox and Capabilities

The sandbox lets a host app run TSX and JavaScript it did not write, such as a [signed package](./signed-packages.md)
or a third-party mini app, under rules the host controls. Rust code is different: it is native code compiled into
the app, and NSH trusts it like the rest of the app.

Each view runs its JS in its own QuickJS VM. Anything the JS does outside that VM goes through NSH's native runtime.
That runtime applies three layers:

- **Limits** cap the memory, time and native resources one app can use.
- **Hardening** removes every way to turn a string into code in release builds.
- **Capabilities** list what an app may reach: which hosts it may fetch from, which pages a web view may show, whether
  it may change the status bar. The app declares them, and a host app may grant less.

A bundle holds only the app's own code. The reactive graph, the renderer and every imported module are native. NSH
trusts nothing in JS and enforces every limit and check in Rust, so there is no JS an app could patch to get around
them. Rust code calling NSH services (`fetch`, images, web views, the status bar) goes through the same native checks
as TSX, within what the host grants.

## Limits

Limits apply to a VM, so they cover TSX and JavaScript.

- **An embedded app** runs within limits its host app sets.
- **A standalone app** (packaged with `nsh package`, the web included, or run with `nsh run`) is your own code, like a
  native app. It uses whatever memory and time the device gives it. Its 4096 KB interpreter stack still turns deep
  recursion into a `RangeError` the app can catch.

Default limits for an embedded app:

| Limit | Default | When it is hit |
|---|---|---|
| JS heap | 256 MB | The VM stops. |
| Interpreter stack | 4096 KB (also the maximum) | Deep recursion throws a `RangeError`, which the app can catch. |
| Time to evaluate the bundle | 10 s | The evaluation is cut off. |
| Time of any other call into JS | 2 s | The call is cut off; the app goes on. |
| Nodes kept | 200 000 | The VM stops. |
| Changes at once (queued operations) | 8 000 000 values | The VM stops. |
| Operations in one canvas drawing | 1 000 000 | The VM stops. |

**Time is budgeted per call, not per app.** A call is one job the runtime gives JS:

- handle one event, including the rendering it causes
- bind one list row
- run one timer
- run one frame's `requestAnimationFrame` callbacks
- drain the microtask queue

A long-running app never builds up time toward a limit. Only a single call that does not return in time is
interrupted. Nested calls share the outer call's budget.

A call is charged the time it actually runs:

| Platform | Clock |
|---|---|
| Linux, Android, macOS, iOS | The thread's CPU time |
| Windows, web | Monotonic time |

Time spent frozen, suspended in the background or waiting for a CPU does not count.

An interrupted call ends with an exception that no `catch` can swallow. The error names the budget:

```text
the script was interrupted: a call into JS exceeded its budget of 2000 ms
```

The VM keeps running. The next event, timer or frame runs normally.

**Running out of memory or going over a quota stops the VM.** NSH reports the error once: to the host app's error
handler, and over the app in development builds. No JS runs after that, and the views stay as they were. The node and
operation quotas exist because native views and queued changes live in native memory, outside the JS heap limit.
Without them, an app could exhaust the device's memory through the UI.

**Arrays that NSH reads natively hold at most 2^20 (1 048 576) items, in every app.** Native code reads these outside
the call budget, and an array is as long as its `length` says, even if empty (`new Array(2 ** 32 - 1)`). The limit
covers:

- children (nested arrays counted together), `<Each>` items
- menus (submenus counted together), a tray's title
- `Animated.sequence` / `parallel` / `stagger`, an animation's ranges and easing, the animations a composite runs (each
  counted together)
- `split`'s keys, a navigation stack's routes
- a native component's `props` and `events` and the values passed to it
- a pan's `touches`, a canvas call's arguments
- a style (its nested styles, shadows and transforms), a prop's lists

What happens with a longer array:

| Where | Result |
|---|---|
| Most places | A `RangeError` the app can catch |
| A menu or the tray | A `TypeError` |
| An animation's `start` | Its promise rejects |
| A canvas call | Ignored, with a warning |
| Navigation stack | Holds up to 2^20 screens; `push` beyond that throws a `RangeError` |

Children, submenus and styles nested more than 64 levels deep throw a `TypeError`. A `<List>`'s `each` takes any
number of rows.

**Views nest up to 1024 levels deep, in every app.** Layout and mounting walk the tree level by level in native code,
where the interpreter's stack limit does not reach. An app that nests deeper is stopped, as with a quota:

```text
the app nested views more than 1024 deep and was stopped
```

> [!TIP]
> These limits are generous for normal apps. If you hit the 2 s call budget, the work is too long for one event
> anyway: it would freeze the app's updates for that long. Split it with `setTimeout`, or move it out of the event
> handler.

Host apps set limits per view. They apply from the next load:

| Host | API |
|---|---|
| [iOS](./ios.md#limits) | `NshAppView.limits` |
| [Android](./android.md#limits) | `NshView.setLimits` |
| [GPUI app](./gpui.md#embedding-in-a-gpui-app) | `AppOptions::limits` / `AppView::set_limits` |

To test a TSX app under a host's limits during development, use [`--limits`](./cli.md#nsh-run):

- `nsh run App.tsx --limits` uses the defaults above.
- `nsh run --limits memory=128MB,call=500ms` sets those two, as a host app would. The rest keep their defaults.

## Hardening

Release builds (everything not loaded from a dev server) are hardened on load:

- `eval` is removed. The `Function`, `AsyncFunction`, `GeneratorFunction` and `AsyncGeneratorFunction` constructors
  throw. NSH runs no code made from strings:

  ```text
  TypeError: the Function constructor is disabled: nsh runs no code from strings (development builds allow it)
  ```

- The shared prototypes (`Object`, `Array`, `Function`, `String`, `Number`, `Promise`, `Map`, typed arrays and the
  rest) are frozen. One module or library cannot change `Array.prototype` for everyone else. `Error.prototype` stays
  writable, so error subclasses can set `this.name`.

Frozen prototypes normally break a common pattern, the "override mistake": `Sub.prototype.constructor = Sub` or
`obj.toString = …` throws when the inherited property is frozen. NSH turns the commonly overridden members
(`constructor`, `toString`, `valueOf`, `toJSON`, ...) into accessors whose setter defines the property on the target
object. Such code keeps working, as do libraries that rely on it, like axios.

Bundles served by `nsh dev` / `nsh run` (with or without `--release`) are not hardened, so you can debug with `eval`.
Before shipping code that relies on dynamic code, test the release behavior with `nsh test` (which hardens) or a
packaged build.

## Capabilities

An app declares what it may reach outside its VM in `nsh.json`:

```json
{
  "name": "Quotes",
  "id": "com.example.quotes",
  "version": "1.0.0",
  "build": 3,
  "capabilities": {
    "network": {
      "hosts": ["api.example.com", "*.cdn.example.com"],
      "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }],
      "sockets": ["feed.example.com:7000"]
    },
    "webview": { "hosts": ["*.example.com"], "input": true },
    "statusBar": true,
    "clipboard": true,
    "picker": true,
    "share": true,
    "speech": true,
    "notifications": true,
    "audio": true,
    "application": true,
    "homeDir": true,
    "host": ["getUserInfo", "bridge.*"],
    "openUrl": { "schemes": ["https", "mailto"] },
    "windows": { "kinds": ["normal", "dialog", "panel"], "max": 4 }
  }
}
```

The rules:

- **No `"capabilities"` key: the app may do everything.** A standalone app belongs to its developer. Exceptions are
  granted only by name in every app: raw TCP sockets (`network.sockets`) and the host's functions (`host`), plus the
  keys marked "never granted unless declared" below.
- **With `"capabilities"`: the app may do only what it lists.** Everything else is denied.
- **A key or value NSH cannot read grants nothing.** The build warns about it, so a typo shows up at build time instead
  of as a missing permission at runtime.
- A [signed package](./signed-packages.md#packages-from-newer-versions-of-nsh) from a newer NSH may declare capabilities
  an older runtime does not know. The older runtime does not grant them.

### `network`

Covers `fetch` (including every redirect), WebSockets, remote images (`<Image>`, `placeholder`, canvas images,
`Image.getSize` / `Image.prefetch`), video and sound sources, and raw TCP sockets (`net.connect`).

| Key | Type | Description |
|---|---|---|
| `hosts` | `string[]` | Hosts the app may reach with any method and scheme (`http`, `https`, `ws`, `wss`). |
| `http` | `object[]` | Finer grants: a host, narrowed by any of the keys below. |
| `http[].host` | `string` | The host or host pattern (required). |
| `http[].methods` | `string[]` | Allowed methods, such as `["GET", "HEAD"]`. Missing: any. |
| `http[].schemes` | `string[]` | Allowed schemes (`scheme`: one, as a string). Missing: any of `http`, `https`, `ws`, `wss`. |
| `http[].port` | `number` | The port; a URL without one has its scheme's default (443 for `https`). Missing: any. |
| `http[].paths` | `string[]` | Exact paths, such as `["/v1/quote"]` (the query is not part of the path). |
| `http[].pathPrefixes` | `string[]` | Path prefixes covering whole segments: `/v1` covers `/v1` and `/v1/quote`, `/v1/` what follows it. `pathPrefix` (one, as a string) and `path_prefixes` read the same. |
| `sockets` | `string[]` | Raw TCP destinations for [`net.connect`](../modules/standard-modules.md), each an exact `"host:port"` (`"[::1]:9000"` for IPv6). |

An `http` rule with no `paths` or `pathPrefixes` covers every path. With both, a path matching either is covered.

**Path matching.** NSH matches the path the request actually sends, after WHATWG's URL parser resolves dot segments,
including percent-encoded ones:

- `/v1/../admin` and `/v1/%2e%2e/admin` are `/admin`, which `pathPrefix: "/v1"` does not cover.
- The request goes out with the checked URL, so the server receives the path the rule matched.
- Rule paths are parsed the same way: `/my photos` covers `/my%20photos`. A rule path with a `.` or `..` segment is an
  error.
- An encoded `/` or `\` (`%2F`, `%5C`) stays as written, so `/v1/x%2F..%2F..%2Fadmin` is under `/v1`. A server that
  decodes these before resolving dot segments serves a different path.

**Host patterns** are `*` (any host), `*.example.com` (any subdomain of `example.com`, not `example.com` itself) or an
exact host name. Matching ignores case and the port. The host is read after the last `@`, so
`https://api.example.com@evil.io/` is checked as `evil.io`.

Some URLs are read as different hosts or paths by different platforms. Only `*` in `hosts` (or a `*` rule with no port
or path) grants them:

- a backslash before the query (write `%5C`)
- a control character
- a host with percent-encoding, or with a character hosts cannot contain: a space, `<`, `>`, `^`, `|`, or `[` / `]`
  outside an IPv6 address

**WebSockets.** The handshake is an HTTP GET. `ws://` is checked as `http://` and `wss://` as `https://`, so any grant
that allows a GET of the URL allows the WebSocket. **Raw sockets** bypass every HTTP rule, so only `sockets` grants them,
each by exact host and port.

**Media and asset sources.**

- `asset:` URIs (the app's own files) are always allowed while the path stays inside the app's files, as with `fetch`:
  no empty, `.` or `..` segment (encoded or not), and no backslash, colon or encoded `/`.
- Images, videos and sounds may also use `data:` and `blob:` URIs.
- A relative path is relative to the page on the web. A native app has no page, so with capabilities it loads nothing
  there (a sound's relative path is a bundled file).
- A source starting with `//` names a host. On the web it takes the page's scheme and is checked as that URL. In a
  native app there is no scheme to take, so it loads nothing; write `https://…`.
- Any other source with a scheme is checked as a URL and loads exactly what was checked.
- Spaces and control characters around a source are dropped, as the URL parser does, and the cleaned URL loads.
- A source that platforms read differently (a backslash, a control character, tab or newline inside it, an unusual
  host) is granted only by `*` and loads as written. In a native app, one starting with `//` still loads nothing.

**Local files.** A local media source (a `file:` URL, or in a native app an absolute path such as `/Users/a/x.png` or
`C:\pictures\x.png`) is read under the [`fs`](../modules/standard-modules.md) capability's `read` roots, like
`fs/promises`, on every native platform. Symlinks are resolved when the source is set, and the platform loads the
resolved path, so a link inside a granted root cannot lead out of it. A link created there later is followed while the
source stays set. On the web, `/x.png` is the page's own path. An app without `capabilities` loads local files as
before.

### `webview`

| Key | Type | Description |
|---|---|---|
| `hosts` | `string[]` | Hosts a [web view](./web-view-and-video.md) may show and navigate to (`http` and `https`). Without `webview`, the app may not show web pages at all. |
| `input` | `boolean` | A web view's ref may give its page trusted input ([`sendInput`](./web-view-and-video.md#input-as-the-users-own)). Never granted unless declared, also for an app without `"capabilities"`. |
| `media` | `string[]` | What web pages may capture with `getUserMedia`: `"microphone"`, `"camera"`. Never granted unless declared, also for an app without `"capabilities"`. |
| `runtimeOrigin` | `boolean` | A web view's `allowOrigin` may add one origin, decided while the app runs, to the hosts that web view may show ([`allowOrigin`](../elements/web-view.md)). Never granted unless declared, also for an app without `"capabilities"`. |

**What is checked.** The platform's navigation delegate checks top-level navigations inside the web view before they
load. The page's own subresources (scripts, images, XHRs) are not checked.

**Inline content.** `about:blank` and inline HTML (`html`) only need web views to be granted:
`"webview": { "hosts": [] }` allows them without any remote page. A `baseUrl` gives inline HTML that URL's origin in
native web views, so its host must be in `hosts`.

**Blob URLs.** A page may show and download a `blob:` URL it created (`URL.createObjectURL`, an `<a download>` link).
A blob URL embeds the origin of the document that made it. It is allowed when that origin is in `hosts`. A blob from
a host the page may not show, or an opaque one (`blob:null/…`), is refused. Every platform checks this the same way.

**Downloads.** A download whose response comes from a host not in `hosts` is not saved, and `onDownload` reports it
`failed` with `code` `"ERR_ACCESS_DENIED"`. On iOS, macOS and Linux this holds wherever redirects lead. On iOS and
macOS, a redirect to such a host is cancelled before it is requested. Downloads of the page's own content (`data:` and
`blob:` URLs) come from no host and are saved as before.

**Why `input` is separate.** Trusted input carries user activation, which `injectJavaScript` never gets. With it a page
may open windows, write the clipboard, open the system file panel, start WebAuthn or permission prompts, and press any
button that checks `isTrusted`. Without the grant, the core drops `sendInput` and logs which key to add.

**`media`** answers the web view's own capture requests. Declared kinds are allowed on macOS, iOS, Android, Windows and
Linux; the rest are denied. The system still asks the user the first time, so `nsh package` writes what the package
needs:

- `NSMicrophoneUsageDescription` / `NSCameraUsageDescription` (reword them with `ios.infoPlist` / `macos.infoPlist`)
- the macOS `com.apple.security.device.audio-input` / `com.apple.security.device.camera` entitlements
- Android's `RECORD_AUDIO` / `CAMERA` permissions, which the app requests when a page first needs them

The development hosts (`nsh run`) carry the usage descriptions for any app. `getUserMedia` needs a secure context
(`https:` or `localhost`).

```json
{ "capabilities": { "webview": { "hosts": ["phone.example.com"], "media": ["microphone"] } } }
```

### `statusBar`

`"statusBar": true` lets the app change the status bar: the `statusBar` of pages and dialogs in
[`nsh/base`](../modules/nsh-base.md#status-bar), and the `"statusBar"` default in `nsh.json`.

### `clipboard`

`"clipboard": true` lets the app read and write the system clipboard ([`Clipboard`](../modules/nsh.md#clipboard)).
Copying from a selectable text's edit menu is the user's own action and needs no capability.

### `picker`

`"picker": true` lets the app show the system photo picker ([`Picker`](../modules/nsh.md#picker)). The user picks what
reaches the app.

It also grants the photo library to an app using `nsh-extra`'s [`Photos`](./nshm.md#photos): reading recent photos,
their thumbnails and the photos themselves. The user's permission covers the whole app, so this capability decides
which code may use it.

### `share`

`"share": true` lets the app show the system share sheet ([`Share`](../modules/nsh.md#share)). The user chooses where
the content goes. A shared file must also be readable under `fs.read`.

### `speech`

`"speech": true` lets the app recognize speech with `nsh-extra`'s [`Speech`](./nshm.md#speech): it uses the microphone
and the system speech recognizer, and the transcript reaches the app. The system still asks the user once.

### `audio`

`"audio": true` lets the app play sound ([`Sound` and `AudioPlayer`](../modules/nsh.md#sound)). A remote source also
needs a `network` grant for its host.

### `notifications`

`"notifications": true` lets the app show system notifications and set its icon badge
([`Notifications`](../modules/nsh.md#notifications)). Where the system asks the user, it asks once.

It also grants remote notifications to an app using `nsh-extra`'s [`Push`](./nshm.md#push): registering the device and
reading its token, listening to deliveries and presses, choosing how notifications show in the foreground, and removing
a thread's delivered notifications. Registration and background deliveries never ask the user, and the user's
permission covers the whole app, so this capability decides which code may use it.

### `application`

`"application": true` lets the app act on the application as a whole:

- **Desktop** ([`nsh/desktop`](../modules/nsh-desktop.md)): the application menu (`Menu.setApplicationMenu`), the tray,
  global shortcuts, `Application.quit()`, `Application.setQuitMode()`, `Application.restart()` and
  `Application.setLaunchAtLogin()`.
- **iOS and Android**: the app's page in system settings ([`AppSettings.open()`](../modules/nsh-mobile.md#appsettings)
  of `nsh/mobile`; for an embedded view, the embedding app's page).

Without it, these do nothing. `Application.launchAtLogin()` and `AppSettings.available()` resolve `false`, and
`setLaunchAtLogin` and `AppSettings.open()` reject with `ERR_ACCESS_DENIED` (`permission` `"application"`).

Context menus (`Menu.popup`) show in the app's own view, and app windows are granted by [`windows`](#windows), so
neither needs `application`.

### `homeDir`

`"homeDir": true` lets [`os.homedir()`](../modules/standard-modules.md) return the user's home directory. Without it
the call throws an error with `code` `ERR_ACCESS_DENIED`. The path is only a name: reading or writing there still needs
`fs` grants.

### `openUrl`

Lets the app open URLs outside itself ([`openUrl`](../modules/nsh.md#openurl), and `TextView` links with
`links="open"`):

- `"openUrl": true` allows every scheme.
- `"openUrl": { "schemes": ["https", "mailto"] }` allows the listed schemes.

A URL with a scheme not granted rejects with `ERR_ACCESS_DENIED` (`permission` `"openUrl"`, `resource` the URL). URLs
that would run code or reach the app's own data (`javascript:`, `data:`, `file:`, ...) are never opened, whatever the
capability.

### `windows`

Lets the app open its own windows on the desktop ([`Window`](../modules/nsh-desktop.md#windows)). `"windows": true`
allows every kind except overlays. An object narrows it:

| Key | Type | Description |
|---|---|---|
| `kinds` | `string[]` | The kinds it may open: `"normal"`, `"dialog"`, `"panel"`, `"popup"`, `"overlay"`. Missing: every kind but `"overlay"`. |
| `levels` | `string[]` | The levels its windows may float at: `"normal"`, `"floating"`, `"aboveDock"`, `"desktopIcon"`. Missing: every level. |
| `max` | `number` | How many windows of its own may be open at once, the first window included. Missing: any number. |
| `overlay` | `boolean` | Overlays and click-through windows (`pointerEvents: "none"`). Never granted unless declared, also for an app without `"capabilities"`. |

`overlay` is separate because a transparent, click-through window above everything can pass for system UI. Every
window needs the grant, the first one included. No window is privileged.

### Storage

Every app may keep its own data on the device with [`Storage`](../modules/nsh.md#storage). No capability is needed:
the storage holds only that app's data, and the embedding app decides where it lives.

### `host`

`host` lists which functions of the embedding app `host.call(name, args)` (from `nsh`) may call:

- exact names, such as `"getUserInfo"`
- prefixes ending in `*`: `"bridge.*"` matches `bridge.call` and `bridge.share`
- `"host": true` allows every function

Unlisted functions are refused, even in an app without `"capabilities"`. Host functions are never granted by default.
Building an app that imports `host` from `nsh` without a `host` grant warns that every call will be refused.

Names are 1 to 128 characters from `A`–`Z`, `a`–`z`, `0`–`9`, `_`, `.`, `:` and `-`, without `..`. A call with any
other name is refused before the grants are read. End a prefix with a separator: `"bridge*"` also matches
`bridgeAdmin`.

Host events (`host.on(event, listener)`) reach the app's listeners as the host sends them.

```ts
import { host } from "nsh";

if (host.available()) {
  const user = await host.call("getUserInfo");
  const stop = host.on("loginChanged", (payload) => console.log(payload));
  stop();
}
```

`host.available()` is true on platforms that embed a host. Elsewhere `host.call` rejects with `code`
`"ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"`.

### What a refusal looks like

| Request | Result |
|---|---|
| `fetch` | The promise rejects with a `TypeError`, as every failed request, whose `code` is `ERR_ACCESS_DENIED`, `permission` `"network"` and `resource` the URL (the request's, or the redirect's); the request never reaches the network. |
| `Image.getSize`, `Image.prefetch` | The promise rejects with `ERR_ACCESS_DENIED`, `permission` `"network"` and `resource` the URL; the image is never requested. |
| Remote image, video source, web view source or `html` | Nothing is shown, and the element's `onError` reports it with `code` `"ERR_ACCESS_DENIED"`. A web view's navigation to a host not granted is cancelled and reported the same way. |
| Status bar change, canvas image | Nothing changes or is drawn; the refusal is logged. |
| Clipboard | Every function of `Clipboard` rejects with `ERR_ACCESS_DENIED`, `permission` `"clipboard"` and `resource` `""`; the system clipboard is never touched. |
| Picker | `pickImages` and `pickFiles` show nothing and reject with `ERR_ACCESS_DENIED`, `permission` `"picker"` and `resource` `""`. |
| Share | `Share.available()` resolves `false` and `Share.show` shows nothing and rejects with `ERR_ACCESS_DENIED`, `permission` `"share"` and `resource` `""`. |
| Speech | `Speech.start` throws `ERR_ACCESS_DENIED`, `permission` `"speech"` and `resource` `""`; the microphone stays off. |
| Sound | `Sound.load` rejects and `new AudioPlayer` throws `ERR_ACCESS_DENIED`, `permission` `"audio"` and `resource` the source; nothing is read or played. |
| Notifications | Each function of `Notifications` rejects or throws `ERR_ACCESS_DENIED`, `permission` `"notifications"` and `resource` `""`; nothing shows. |
| Photos | Each function of `nsh-extra`'s `Photos` but `access()` rejects or throws `ERR_ACCESS_DENIED`, `permission` `"picker"` and `resource` `""`; the library is not read and the user is not asked. |
| Push | Each function of `nsh-extra`'s `Push` but `available()` rejects or throws `ERR_ACCESS_DENIED`, `permission` `"notifications"` and `resource` `""`; the device is not registered, and no token or delivery reaches the app. |
| `openUrl` | The promise rejects with `ERR_ACCESS_DENIED`, `permission` `"openUrl"` and `resource` the URL; nothing opens. |
| Host call | `host.call` rejects with `ERR_ACCESS_DENIED`, `permission` `"host"` and `resource` the function's name (`host.post` throws it); the call never reaches the host. |
| Files (`fs/promises`) | The promise rejects with `ERR_ACCESS_DENIED`, `permission` `"fs"` and `resource` the path; the disk is never touched. |
| Processes (`process`) | `run` rejects, and `spawn` and `exit` throw, with `ERR_ACCESS_DENIED`, `permission` `"process"` and `resource` the command (`"exit"` for `exit`); no child starts. |
| TCP socket (`net.connect`) | The promise rejects with `ERR_ACCESS_DENIED`, `permission` `"network"` and `resource` the `"host:port"`, before any DNS lookup. |
| Window (`Window.open`, `<Window>`) | Throws `ERR_ACCESS_DENIED`, `permission` `"windows"` and `resource` the window's id; nothing opens. An attribute of `set()` not granted is left as it is, with a warning. |
| WebSocket | The `websocket` module's `connect` rejects with `ERR_ACCESS_DENIED`, `permission` `"network"` and `resource` the URL; the global `WebSocket` fires `error` and then `close` (code 1006), as a connection that fails does. |

Errors from the [standard modules](../modules/standard-modules.md#failures) and from [`nsh`](../modules/nsh.md) modules
carry `code` `"ERR_ACCESS_DENIED"`, so an app can tell a refusal from a failed operation. `permission` names the
capability and `resource` names what was requested. `fetch` rejects with a `TypeError`, the class of every failed
request.

```ts
import { Clipboard } from "nsh";

try {
  await Clipboard.writeText(link);
} catch (error) {
  if (error.code !== "ERR_ACCESS_DENIED") throw error;
  console.warn(`${error.permission} is not granted`); // "clipboard"
}
```

Rust components see refusals in Rust terms:

| Call | Refused result |
|---|---|
| Clipboard read | Same as an empty clipboard |
| Pick | No files |
| `open_url` | `false` |
| `fetch` | An `Err` with the message |

Each refusal is logged once, with the key to add:

```text
fetch GET https://evil.example.org/x is not granted: add `evil.example.org` to capabilities.network in nsh.json
```

NSH follows redirects itself instead of the platform's HTTP stack, so every hop is checked. A streamed request body is
not sent twice, so a `307` / `308` redirect of a streamed upload fails.

### Where capabilities travel

The declaration goes everywhere the app goes:

- `nsh build` and `nsh package` write it next to the bundle as `capabilities.json`.
- `nsh publish` puts it in the [signed package](./signed-packages.md) header, covered by the signature.
- `nsh dev` sends it with every build, so development enforces the same rules. Changing `capabilities` reloads the app
  instead of hot-patching it.
- An app whose root is a Rust component (`"root"` in `nsh.json`) has it compiled in with the root. Every runner,
  including the web and `nsh test`, checks the app's calls against it, as for a TSX app. Without `"capabilities"` it
  behaves like any app without a declaration.

## Grants: what a host app allows

A host app that embeds NSH can grant **at most** a set of capabilities, in the same JSON form:

```rust
// iOS, in Rust (nsh_platform_ios::AppView)
let grant = nsh_engine::Capabilities::from_json(&serde_json::json!({
    "network": { "hosts": ["api.example.com"] }
}))?;
app.set_granted_capabilities(Some(grant));
```

```swift
// iOS
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
```

```java
// Android
nsh.setGrantedCapabilities("{ \"network\": { \"hosts\": [\"api.example.com\"] } }");
```

A request must be allowed by **both** the app's declaration and the host's grant. With the grant above, an app that
declares `"hosts": ["*"]` can still reach only `api.example.com`, and cannot show web pages or change the status bar.
Without a grant (`nil` / `null`, the default), each app's declaration applies.

Grants apply from the next load. A typical setup for third-party mini apps: grant each one only its own API host, run
it from a [signed package](./signed-packages.md) whose declaration you reviewed, and keep the default limits or lower
them.

> [!WARNING]
> [Native components](./native-components.md) the host app registers are the host's own code: capabilities do not
> apply to them. Register only components that are safe for every app the view may run.

## Summary

| Layer | Enforced by | Set by |
|---|---|---|
| Limits (heap, stack, time, nodes) | the runtime, per VM | the host app per view (embedded apps) |
| Hardening | the runtime, on load | always on outside development |
| Capabilities | the runtime (fetch, images, video, web views, status bar, windows, files, processes, sockets) | the app (`nsh.json`) and the host app (grant) |
| Code integrity | signature verification | `nsh publish`; the host app's trusted roots |

## See also

- [Signed packages](./signed-packages.md)
- [Networking](./networking.md)
- [Embedding in iOS apps](./ios.md), [Embedding in Android apps](./android.md)

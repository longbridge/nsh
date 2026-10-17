---
title: Sandbox and Capabilities
description: The limits every NSH app runs under (memory, time, nodes), the language hardening of release builds, and capabilities declared in nsh.json and granted by host apps.
order: 21
group: delivery
---

# Sandbox and Capabilities

Every NSH app runs in its own QuickJS VM, and everything an app can do outside that VM goes through NSH's native
runtime. That makes it possible to run code you did not write, such as third-party mini apps or downloaded updates,
under rules the host app controls:

- **Limits** bound the memory, time and native resources one app can use.
- **Hardening** removes the ways to turn strings into code in release builds.
- **Capabilities** list what an app may reach: which hosts it may fetch from, which pages a web view may show, whether
  it may change the status bar. The app declares them; a host app may grant less.

A bundle holds only the app's code: the reactive graph, the renderer and every module the app imports are native. So
nothing in JS is trusted, and every limit and check is enforced in Rust; there is no JS code an app could patch to get
around them.

## Limits

Each VM has these limits:

| Limit | Default | When it is hit |
|---|---|---|
| JS heap | 1 GB for an app on its own on macOS, Windows and Linux (Mac Catalyst too); 512 MB on iOS and Android; 256 MB for an app embedded in another app (the embedder may set another) | The VM stops. |
| Interpreter stack | 4096 KB (also the maximum) | Deep recursion throws a `RangeError`, which the app can catch. |
| Time to evaluate the bundle | 10 s | The evaluation is cut off. |
| Time of any other call into JS | 2 s | The call is cut off; the app goes on. |
| Nodes kept | 200 000 | The VM stops. |
| Changes at once (queued operations) | 8 000 000 values | The VM stops. |
| Operations in one canvas drawing | 1 000 000 | The VM stops. |

**Time is budgeted per call, not per app.** A "call" is one thing the runtime asks JS to do: handle one event
(including the rendering it causes), bind one list row, run one timer, run one frame's `requestAnimationFrame`
callbacks, drain the microtask queue. A long-running app never accumulates time toward a limit; only a single call
that does not return in time is interrupted. Calls nested inside another share the outer call's budget.

An interrupted call ends with an exception that no `catch` can swallow, and the error is reported with the budget it
exceeded:

```text
the script was interrupted: a call into JS exceeded its budget of 2000 ms
```

The VM keeps running: the next event, timer or frame runs normally.

**Running out of memory or going over a quota stops the VM.** The error is reported once (to the host app's error
handler, and in development builds over the app), no JS runs after that, and the views stay as they were. The node
and operation quotas exist because native views and queued changes live in native memory, which the JS heap limit does
not cover: without them, an app could exhaust the device's memory through the UI.

> [!TIP]
> These limits are generous for normal apps. If you hit the 2 s call budget, the work is too long for one event
> anyway: it would freeze the app's updates for that long. Split it with `setTimeout`, or move it out of the event
> handler.

Host apps can change the limits per view: `NshAppView.limits` on [iOS](./embedding-ios.md#limits),
`NshView.setLimits` on [Android](./embedding-android.md#limits). They apply from the next load.

## Hardening

Release builds (everything not loaded from a dev server) are hardened when they load:

- `eval` is removed, and the `Function`, `AsyncFunction`, `GeneratorFunction` and `AsyncGeneratorFunction`
  constructors throw. NSH runs no code made from strings:

  ```text
  TypeError: the Function constructor is disabled: nsh runs no code from strings (development builds allow it)
  ```

- The prototypes all modules share (`Object`, `Array`, `Function`, `String`, `Number`, `Promise`, `Map`, typed arrays
  and the rest) are frozen, so one module (or one library) cannot change `Array.prototype` for everyone else.
  `Error.prototype` stays writable, so error subclasses can set `this.name`.

Freezing prototypes would normally break a common pattern, the "override mistake": assigning
`Sub.prototype.constructor = Sub` or `obj.toString = …` throws when the inherited property is frozen. NSH turns the
commonly overridden members (`constructor`, `toString`, `valueOf`, `toJSON`, ...) into accessors whose setter defines
the property on the object assigned to, so such code (and libraries like axios that use it) keeps working.

Bundles served by `nsh dev` / `nsh run` (with or without `--release`) are not hardened, so you can debug with `eval`. Test the release
behavior with `nsh trace` (which hardens) or a packaged build before shipping code that relies on dynamic code.

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
      "http": [{ "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }]
    },
    "webview": { "hosts": ["*.example.com"] },
    "statusBar": true,
    "clipboard": true
  }
}
```

**An app without `"capabilities"` may do everything**: a standalone app is its developer's own, and nothing is
restricted. **An app with `"capabilities"` may do only what it lists**: everything else is denied. Unknown keys are
build errors, so a typo is caught when you build instead of becoming a missing permission at runtime.

### `network`

Covers `fetch` (including every redirect), remote images (`<Image>`, `defaultSource`, canvas images,
`Image.getSize` / `Image.prefetch`) and video sources.

| Key | Type | Description |
|---|---|---|
| `hosts` | `string[]` | Hosts the app may reach with any method and scheme (`http`, `https`, `ws`, `wss`). |
| `http` | `object[]` | Finer grants, one per host. |
| `http[].host` | `string` | The host (required). |
| `http[].methods` | `string[]` | Allowed methods, such as `["GET", "HEAD"]`. Empty or missing: any. |
| `http[].schemes` | `string[]` | Allowed schemes. Empty or missing: any of `http`, `https`, `ws`, `wss`. |
| `http[].pathPrefix` | `string` | The URL's path must start with it. |

Host patterns are `*` (any host), `*.example.com` (any subdomain of `example.com`, not `example.com` itself) or an
exact host name. Matching ignores case and the port. The host is taken after the last `@`, so
`https://api.example.com@evil.io/` is checked as `evil.io`.

`asset:` URIs (the app's own files) are always allowed; images and videos may also use `data:` URIs and relative
paths.

### `webview`

| Key | Type | Description |
|---|---|---|
| `hosts` | `string[]` | Hosts a [web view](./web-view-and-video.md) may show and navigate to (`http` and `https`). Without `webview`, the app may not show web pages at all. |

Top-level navigations inside the web view are checked too, by the platform's navigation delegate, before they load.
The page's own subresources (its scripts, images, XHRs) are the web content's business and are not checked.
`about:blank` and inline HTML (`source={{ html }}`) need only web views to be granted at all: `"webview": { "hosts": [] }`
allows them without allowing any remote page.

### `statusBar`

`"statusBar": true` lets the app change the status bar (the `StatusBar` component and its functions in
[`nsh/ui/mobile`](../components/status-bar.md)).

### `clipboard`

`"clipboard": true` lets the app read and write the system clipboard ([`nsh/clipboard`](../modules/nsh-clipboard.md)). Copying from a selectable text's edit menu is the user's own act and needs no capability.

### What a refusal looks like

| Request | Result |
|---|---|
| `fetch` | The promise rejects; the request never reaches the network. |
| Remote image | The image fails with `onError`. |
| Video source, web view source, status bar change | Nothing is shown or changed. |
| Clipboard | `setString` does nothing; `getString` resolves `""` and `hasString` `false`. |

Each refusal is logged once, saying which key to add:

```text
fetch GET https://evil.example.org/x is not granted: add `evil.example.org` to capabilities.network in nsh.json
```

Redirects are followed by NSH, not by the platform's HTTP stack, so every hop is checked against the capabilities. (A
streamed request body is not sent twice, so a `307` / `308` redirect of a streamed upload fails.)

### Where capabilities travel

The declaration goes with the app wherever it goes:

- `nsh build` and `nsh package` write it next to the bundle as `capabilities.json`;
- `nsh publish` puts it in the [signed package](./signed-packages.md) header, covered by the signature;
- `nsh dev` sends it with every build, so development enforces the same rules. Changing `capabilities` reloads the app
  instead of hot-patching it.

## Grants: what a host app allows

A host app that embeds NSH can grant **at most** some capabilities, in the same JSON form:

```swift
// iOS
try app.grantCapabilities(#"{ "network": { "hosts": ["api.example.com"] } }"#)
```

```java
// Android
nsh.setGrantedCapabilities("{ \"network\": { \"hosts\": [\"api.example.com\"] } }");
```

A request must be allowed by **both** the app's declaration and the host's grant. With the grant above, an app that
declares `"hosts": ["*"]` can still only reach `api.example.com`, and cannot show web pages or change the status bar.
Without a grant (`nil` / `null`, the default), what each app declares applies.

Grants apply from the next load. A typical setup for third-party mini apps: grant each one only its own API host, run
it from a [signed package](./signed-packages.md) whose declaration you reviewed, and keep the default limits or lower
them.

> [!WARNING]
> [Native components](./native-components.md) the host app registers are the host's own code: capabilities do not
> apply to them. Register only components that are safe for every app the view may run.

## Summary

| Layer | Enforced by | Set by |
|---|---|---|
| Limits (heap, stack, time, nodes) | the runtime, per VM | defaults; host app per view |
| Hardening | the runtime, on load | always on outside development |
| Capabilities | the runtime (fetch, images, video, web views, status bar) | the app (`nsh.json`) and the host app (grant) |
| Code integrity | signature verification | `nsh publish`; the host app's trusted roots |

## See also

- [Signed packages](./signed-packages.md)
- [Networking](./networking.md)
- [Embedding in iOS apps](./embedding-ios.md), [Embedding in Android apps](./embedding-android.md)

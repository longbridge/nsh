---
title: Networking
description: HTTP with fetch - JSON, headers, uploads with FormData, streaming responses and request bodies, cancelling with AbortController, errors, and the network capabilities an app declares.
order: 46
group: ui
---

# Networking

NSH apps make HTTP requests with the standard `fetch`. It runs on the platform's network stack (NSURLSession on iOS, HttpURLConnection on Android), so requests use the system's TLS, proxies and HTTP/2, and it streams both ways: a response body arrives as a stream as the stack delivers it, and a request body can be a stream sent as you produce it. The Fetch API's companions are there too: `Headers`, `Request`, `Response`, `AbortController`, `FormData`, `Blob`, `File`, `URL`, `URLSearchParams`, `TextEncoder` / `TextDecoder` and Web Streams.

## Getting JSON

```tsx
import { mount, Scroll, signal, Text, View } from "nsh";

type Post = { id: number; title: string };

function App() {
  const posts = signal<Post[]>([]);
  const status = signal("loading…");

  (async () => {
    try {
      const response = await fetch("https://jsonplaceholder.typicode.com/posts?_limit=5");
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      posts.set(await response.json());
      status.set("");
    } catch (e) {
      status.set(String(e));
    }
  })();

  return (
    <Scroll style={{ flex: 1 }}>
      <View style={{ padding: 16, paddingTop: 60, gap: 8 }}>
        <Text style={{ color: "#57606a" }}>{status()}</Text>
        <Each of={posts()}>{(post) => <Text numberOfLines={1}>{post().title}</Text>}</Each>
      </View>
    </Scroll>
  );
}

mount(App);
```

A `Response` reads its body once, as `json()`, `text()`, `arrayBuffer()`, `bytes()`, `blob()` or `formData()`, or as the `body` stream. `response.ok` is true for 2xx statuses; like in browsers, an HTTP error status does not reject the promise.

## Sending data

```ts
// JSON
const created = await fetch("https://api.example.com/notes", {
  method: "POST",
  headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
  body: JSON.stringify({ title: "Groceries", items: ["milk", "eggs"] }),
});

// A form, URL-encoded
await fetch("https://api.example.com/login", {
  method: "POST",
  body: new URLSearchParams({ user: "ada", password: "…" }),
});

// Query strings
const url = new URL("https://api.example.com/search");
url.searchParams.set("q", "native shell");
url.searchParams.set("page", "2");
const results = await (await fetch(url)).json();
```

`body` accepts a string, `URLSearchParams`, `FormData`, a `Blob` or `File`, an `ArrayBuffer` or typed array, or a `ReadableStream`.

### Uploading files with FormData

A `FormData` body is sent as `multipart/form-data` with its boundary. NSH always sets that content type for a `FormData` body, even if a library sets a default `Content-Type` first, because without the boundary the server cannot parse it.

```ts
const form = new FormData();
form.append("title", "Sunset");
form.append("photo", new Blob([bytes], { type: "image/jpeg" }), "sunset.jpg");

const response = await fetch("https://api.example.com/photos", { method: "POST", body: form });
```

## Streaming

### Reading a response as it arrives

`response.body` is a `ReadableStream<Uint8Array>` filled as the network delivers bytes. Read it chunk by chunk to show progress or to process server-sent lines as they come:

```ts
const response = await fetch("https://api.example.com/large.csv");
const total = Number(response.headers.get("Content-Length") ?? 0);
let received = 0;

for await (const chunk of response.body!) {
  received += chunk.length;
  progress.set(total ? received / total : 0);
}
```

Text streams decode with `TextDecoderStream`, and a `TransformStream` splits them into lines:

```ts
const response = await fetch("https://api.example.com/events", { headers: { Accept: "text/event-stream" } });
let rest = "";
const lines = response
  .body!.pipeThrough(new TextDecoderStream())
  .pipeThrough(
    new TransformStream<string, string>({
      transform(chunk, controller) {
        const parts = (rest + chunk).split("\n");
        rest = parts.pop() ?? "";
        for (const line of parts) controller.enqueue(line);
      },
    }),
  );

for await (const line of lines) {
  if (line.startsWith("data: ")) messages.update((m) => [...m, line.slice(6)]);
}
```

Back pressure works end to end: while more than 1 MiB of a response waits unread, the request is paused at the network level until your code catches up.

### Streaming a request body

A `ReadableStream` body is sent chunk by chunk as the network stack asks for it (chunked transfer). Pass `duplex: "half"`, as the Fetch standard requires:

```ts
const encoder = new TextEncoder();
const body = new ReadableStream<Uint8Array>({
  async start(controller) {
    for (const line of logLines) controller.enqueue(encoder.encode(line + "\n"));
    controller.close();
  },
});

await fetch("https://api.example.com/logs", { method: "POST", body, duplex: "half" });
```

A streamed body cannot be sent twice, so a 307 or 308 redirect of a streamed request fails.

## Cancelling and timeouts

Pass an `AbortSignal` to cancel a request; the promise rejects with the signal's reason (an `AbortError` `DOMException` by default), and a body being read stops.

```ts
const controller = new AbortController();
const request = fetch("https://api.example.com/search?q=" + encodeURIComponent(query), { signal: controller.signal });

// Typing again: cancel the previous search.
controller.abort();
```

`AbortSignal.timeout(ms)` makes a signal that aborts after a delay, and `AbortSignal.any([...])` combines signals:

```ts
const response = await fetch(url, { signal: AbortSignal.timeout(10_000) });
```

A component that loads data should cancel when it goes away, with [`cleanup`](./reactivity.md):

```tsx
import { cleanup, signal, Text } from "nsh";

function Weather(props: { city: string }) {
  const data = signal<unknown>(null);
  const controller = new AbortController();
  cleanup(() => controller.abort());
  fetch(`https://api.example.com/weather?city=${encodeURIComponent(props.city)}`, { signal: controller.signal })
    .then((r) => r.json())
    .then((d) => data.set(d))
    .catch(() => {});
  return <Text>{JSON.stringify(data())}</Text>;
}
```

## Errors

A request that does not produce an HTTP response rejects with a `TypeError` whose message starts with `Network request failed:`: no connection, a DNS or TLS failure, an unsupported URL scheme, or a request your app's capabilities do not allow. Check `response.ok` or `response.status` for HTTP errors.

```ts
try {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Server answered ${response.status}`);
  return await response.json();
} catch (e) {
  if (e instanceof DOMException && e.name === "AbortError") return; // cancelled
  showError(String(e));
}
```

## Capabilities: which hosts an app may reach

An app may declare in `nsh.json` which hosts it talks to. Once `"capabilities"` is present, everything not listed is denied:

```json
{
  "capabilities": {
    "network": {
      "hosts": ["api.example.com", "*.cdn.example.com"],
      "http": [
        { "host": "quotes.example.com", "methods": ["GET"], "pathPrefix": "/v1/" }
      ]
    }
  }
}
```

| Key | Description |
|---|---|
| `network.hosts` | Hosts reachable with any method: `"*"` (any host), `"*.example.com"` (its subdomains, not `example.com` itself) or a host name. |
| `network.http` | Finer grants: a `host`, optionally limited to `methods`, `schemes` and a `pathPrefix`. |

The checks are enforced in Rust, outside your JS, before a request reaches the network:

- A refused `fetch` rejects with a `TypeError` whose message names the key to add, such as ``Network request failed: fetch GET https://evil.io/ is not granted: add `evil.io` to capabilities.network in nsh.json``.
- **Every redirect is checked again.** NSH follows redirects itself (up to 20) instead of letting the platform do it, so a granted host cannot redirect your app to one that is not.
- The same `network` grants cover remote images (also in canvases and `Image.getSize` / `prefetch`) and video sources. `asset:` URIs of your own bundle are always allowed.
- The host is taken after the last `@`, so `https://ok.com@evil.io` counts as `evil.io`.

An app without `"capabilities"` may reach every host: a standalone app is its developer's own. Apps embedded in a host app or distributed as [signed packages](./signed-packages.md) should declare them; the host app can grant less than an app declares, and a request must be allowed by both. Development with `nsh dev` / `nsh run` enforces the same capabilities. See [Sandbox and capabilities](./sandbox-and-capabilities.md).

## Differences from browsers

NSH's `fetch` follows the Fetch standard with the differences that make sense for an app rather than a web page:

- **No CORS.** An app talks to any host its capabilities allow; there are no preflight requests and no opaque responses.
- **No forbidden headers.** An app may set `Cookie`, `User-Agent`, `Origin` and others.
- **`redirect: "manual"`** gives the 3xx response itself (with its `Location` header) rather than an opaque redirect. `redirect: "error"` rejects on a redirect.
- **`data:` and `blob:` URLs** are answered locally. `URL.createObjectURL(blob)` makes a `blob:` URL that `fetch` can read until you revoke it.
- Browser-only options (`mode`, `credentials`, `cache`, `referrer`, `integrity`, `keepalive`, `priority`) are accepted for compatibility with libraries.

Libraries built on `fetch` (and on `AbortController`, `FormData` and streams) generally work unchanged. There is no `XMLHttpRequest`, and no WebSocket API.

## Related

- [Global APIs](../modules/globals.md): every global, with its members.
- [Sandbox and capabilities](./sandbox-and-capabilities.md): capabilities, limits and how the embedding app grants them.
- [Images and assets](./images-and-assets.md): remote images.
- [Lists](./lists.md#loading-more-onendreached): paging through an API.

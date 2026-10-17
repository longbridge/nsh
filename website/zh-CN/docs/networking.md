---
title: 网络
description: 用 fetch 发起 HTTP 请求：JSON、请求头、用 FormData 上传、流式响应与请求体、用 AbortController 取消、错误处理，以及应用声明的网络能力。
order: 46
group: ui
---

# 网络

NSH 应用用标准的 `fetch` 发起 HTTP 请求。它运行在平台的网络栈上（iOS 上是 NSURLSession，Android 上是 HttpURLConnection），因此请求使用系统的 TLS、代理和 HTTP/2；而且它双向流式传输：响应体随网络栈的交付以流的形式到达，请求体也可以是一个边生成边发送的流。Fetch API 的配套对象也都具备：`Headers`、`Request`、`Response`、`AbortController`、`FormData`、`Blob`、`File`、`URL`、`URLSearchParams`、`TextEncoder` / `TextDecoder` 以及 Web Streams。

## 获取 JSON

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

`Response` 的响应体只能读取一次，可以用 `json()`、`text()`、`arrayBuffer()`、`bytes()`、`blob()` 或 `formData()`，也可以读取 `body` 流。2xx 状态码时 `response.ok` 为 true；与浏览器一样，HTTP 错误状态码不会让 promise 被拒绝。

## 发送数据

```ts
// JSON
const created = await fetch("https://api.example.com/notes", {
  method: "POST",
  headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
  body: JSON.stringify({ title: "Groceries", items: ["milk", "eggs"] }),
});

// URL 编码的表单
await fetch("https://api.example.com/login", {
  method: "POST",
  body: new URLSearchParams({ user: "ada", password: "…" }),
});

// 查询字符串
const url = new URL("https://api.example.com/search");
url.searchParams.set("q", "native shell");
url.searchParams.set("page", "2");
const results = await (await fetch(url)).json();
```

`body` 可以是字符串、`URLSearchParams`、`FormData`、`Blob` 或 `File`、`ArrayBuffer` 或类型化数组，或者 `ReadableStream`。

### 用 FormData 上传文件

`FormData` 请求体以带 boundary 的 `multipart/form-data` 发送。即使某个库事先设置了默认的 `Content-Type`，NSH 也总会为 `FormData` 请求体设置这个类型，因为缺少 boundary 时服务器无法解析。

```ts
const form = new FormData();
form.append("title", "Sunset");
form.append("photo", new Blob([bytes], { type: "image/jpeg" }), "sunset.jpg");

const response = await fetch("https://api.example.com/photos", { method: "POST", body: form });
```

## 流式传输

### 边到达边读取响应

`response.body` 是一个 `ReadableStream<Uint8Array>`，随网络送达的字节逐步填充。逐块读取它，可以显示进度，或在服务器推送的行到达时立即处理：

```ts
const response = await fetch("https://api.example.com/large.csv");
const total = Number(response.headers.get("Content-Length") ?? 0);
let received = 0;

for await (const chunk of response.body!) {
  received += chunk.length;
  progress.set(total ? received / total : 0);
}
```

文本流用 `TextDecoderStream` 解码，再用 `TransformStream` 拆成行：

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

背压贯穿全程：响应中有超过 1 MiB 的数据尚未读取时，请求会在网络层暂停，直到你的代码跟上。

### 流式发送请求体

`ReadableStream` 请求体会在网络栈需要时逐块发送（分块传输）。按照 Fetch 标准的要求，传入 `duplex: "half"`：

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

流式请求体无法发送第二次，因此流式请求遇到 307 或 308 重定向会失败。

## 取消与超时

传入 `AbortSignal` 即可取消请求；promise 会以 signal 的 reason 被拒绝（默认是 `AbortError` 类型的 `DOMException`），正在读取的响应体也会停止。

```ts
const controller = new AbortController();
const request = fetch("https://api.example.com/search?q=" + encodeURIComponent(query), { signal: controller.signal });

// 用户又输入了：取消上一次搜索。
controller.abort();
```

`AbortSignal.timeout(ms)` 创建一个在延迟后中止的 signal，`AbortSignal.any([...])` 组合多个 signal：

```ts
const response = await fetch(url, { signal: AbortSignal.timeout(10_000) });
```

加载数据的组件应该在销毁时取消请求，使用 [`cleanup`](./reactivity.md)：

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

## 错误

没有得到 HTTP 响应的请求会以 `TypeError` 被拒绝，其消息以 `Network request failed:` 开头：无网络连接、DNS 或 TLS 失败、不支持的 URL 协议，或者应用的能力声明不允许该请求。HTTP 错误请检查 `response.ok` 或 `response.status`。

```ts
try {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Server answered ${response.status}`);
  return await response.json();
} catch (e) {
  if (e instanceof DOMException && e.name === "AbortError") return; // 已取消
  showError(String(e));
}
```

## 能力声明：应用可以访问哪些主机

应用可以在 `nsh.json` 中声明它要访问的主机。一旦存在 `"capabilities"`，未列出的一切都会被拒绝：

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

| 键 | 说明 |
|---|---|
| `network.hosts` | 可以用任何方法访问的主机：`"*"`（任意主机）、`"*.example.com"`（其子域名，不含 `example.com` 本身）或主机名。 |
| `network.http` | 更精细的授权：一个 `host`，可以再限制 `methods`、`schemes` 和 `pathPrefix`。 |

这些检查在 Rust 中、在你的 JS 之外执行，请求到达网络之前就已完成：

- 被拒绝的 `fetch` 会以 `TypeError` 失败，消息中指出需要添加的键，例如 ``Network request failed: fetch GET https://evil.io/ is not granted: add `evil.io` to capabilities.network in nsh.json``。
- **每次重定向都会再次检查。** NSH 自己跟随重定向（最多 20 次），而不交给平台处理，所以获准的主机无法把应用重定向到未获准的主机。
- 同样的 `network` 授权也适用于远程图片（包括画布中的图片和 `Image.getSize` / `prefetch`）和视频来源。应用自身包中的 `asset:` URI 始终允许。
- 主机取最后一个 `@` 之后的部分，所以 `https://ok.com@evil.io` 被视为 `evil.io`。

没有 `"capabilities"` 的应用可以访问任何主机：独立应用属于开发者自己。嵌入在宿主应用中或以[签名包](./signed-packages.md)分发的应用应当声明能力；宿主应用可以授予比应用声明更少的能力，请求必须同时被两者允许。使用 `nsh dev` / `nsh run` 开发时执行同样的能力检查。见[沙箱与能力声明](./sandbox-and-capabilities.md)。

## 与浏览器的差异

NSH 的 `fetch` 遵循 Fetch 标准，但在对应用（而不是网页）有意义的地方有所不同：

- **没有 CORS。** 应用可以访问其能力允许的任何主机；没有预检请求，也没有不透明响应。
- **没有禁止的请求头。** 应用可以设置 `Cookie`、`User-Agent`、`Origin` 等。
- **`redirect: "manual"`** 返回 3xx 响应本身（带有 `Location` 头），而不是不透明重定向。`redirect: "error"` 遇到重定向时拒绝。
- **`data:` 和 `blob:` URL** 在本地应答。`URL.createObjectURL(blob)` 生成一个 `blob:` URL，在撤销之前 `fetch` 都可以读取它。
- 浏览器专用的选项（`mode`、`credentials`、`cache`、`referrer`、`integrity`、`keepalive`、`priority`）为兼容各种库而被接受。

基于 `fetch`（以及 `AbortController`、`FormData` 和流）构建的库通常无需修改即可使用。没有 `XMLHttpRequest`，也没有 WebSocket API。

## 相关内容

- [全局 API](../modules/globals.md)：所有全局对象及其成员。
- [沙箱与能力声明](./sandbox-and-capabilities.md)：能力、限制以及嵌入应用如何授予它们。
- [图片与资源](./images-and-assets.md)：远程图片。
- [列表](./lists.md#加载更多onendreached)：分页加载 API 数据。

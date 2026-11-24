---
title: 发布到 Web
description: "把 NSH 应用作为网站发布：nsh package --target web、它写出的文件、基础路径和 CDN、缓存头和压缩、部署到静态主机和从 CI 部署，以及常见错误。"
order: 19.7
group: publishing
---

# 发布到 Web

在 Web 上，应用就是一个静态站点。`nsh package --target web` 写出它；你把目录复制到任意 Web 服务器、对象存储或静态托管服务上。没有应用商店，不需要签名，也没有审核，部署之后每个用户下次加载时就会更新。这里没有 `nsh publish` 要做的事。

## 配置应用

| 键 | 在站点中 | 说明 |
|---|---|---|
| `name` | 页面标题和 Web 应用清单的名称 | |
| `id` | Web 应用清单的 id | |
| `icon`（或 `icon.web`） | favicon、`apple-touch-icon` 和清单图标 | 缩放为 32、180、192 和 512 像素（[图标](./packaging.md#icons)）。 |
| `appearance` | 页面加载时的颜色 | |
| `web.base` | 站点所在的路径 | 应用在自己的路径下打开时需要（[链接到应用内](./web.md#links-into-the-app)）。 |
| `web.host` | 页面的宿主脚本 | [宿主的函数](./web.md#the-hosts-functions)。 |

## 构建

```sh
nsh package --target web            # a Rust app, from its directory
nsh package App.tsx --target web    # a TSX app
nsh package --target web --run      # and serve it on http://127.0.0.1:4173/ to try it
```

这会写出 `.nsh/out/<Name>/`（用 `--out` 指定其他目录）：`index.html`、`manifest.webmanifest`，以及其余所有按内容命名的文件（`nsh.<digest>.wasm`、`app.<digest>.js`、`assets/…`），旁边还有它们的 Brotli 和 gzip 副本。一次发布只会改变 `index.html` 和真正变化了的文件。完整的文件列表和运行时大小见 [Web：发布应用](./web.md#publish-an-app)。

站点默认使用相对 URL，因此放在任何主机的任何目录下都能工作。如果想让文件从 CDN 提供、页面仍留在应用自己的地址，传入 `--asset-url https://cdn.example.com/notes/`。

## 部署

任何静态托管都可以。遵守两条规则，应用就能快速加载并立即更新：

| 文件 | `Cache-Control` |
|---|---|
| `index.html`、`manifest.webmanifest` | `no-cache` |
| 其他所有文件 | `public, max-age=31536000, immutable` |

以 `application/wasm` 提供 `.wasm`，并向支持的浏览器发送预压缩的 `.br` 和 `.gz` 副本（[部署](./web.md#deploy-it)中有 nginx 和 Caddy 的配置）。先上传带哈希的文件，再上传 `index.html`，这样访客永远不会拿到所引用文件尚未就位的页面。

### GitHub Pages

```yaml
on:
  push:
    tags: ["v*"]

permissions:
  pages: write
  id-token: write

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: github-pages
    steps:
      - uses: actions/checkout@v4
      # Install nsh (pin the version), then:
      - run: nsh package --target web --out site
      - uses: actions/upload-pages-artifact@v3
        with:
          path: site
      - uses: actions/deploy-pages@v4
```

项目站点位于 `/<repo>/` 下；相对 URL 能处理这一点。在自己的路径下打开的应用要设置 `"web": { "base": "/<repo>/" }`，并添加一份 `index.html` 的副本作为 `404.html`（上传前执行 `cp site/index.html site/404.html`），GitHub Pages 会用它响应未知路径。GitHub Pages 不能按文件设置 `Cache-Control`，所以回访的用户也会重新验证带哈希的文件；能自行设置响应头的主机加载更快。

### 对象存储和 CDN

```sh
nsh package --target web --out site
aws s3 sync site s3://example-notes --exclude index.html --exclude manifest.webmanifest \
  --cache-control "public, max-age=31536000, immutable"
aws s3 cp site/index.html s3://example-notes/ --cache-control no-cache
aws s3 cp site/manifest.webmanifest s3://example-notes/ --cache-control no-cache
```

把 CDN 配置为以 `application/wasm` 提供 `.wasm`，并用 Brotli 压缩，或直接提供 `.br` 副本。

## 支持的浏览器

Safari 16.4（iOS 和 iPadOS 16.4）、Chrome 和 Edge 97、Firefox 108 及更高版本。更早的浏览器会显示空白页面（[支持的浏览器](./web.md#supported-browsers)）。

## 常见错误

| 问题 | 处理方法 |
|---|---|
| 页面空白，控制台中出现 `Failed to load module script` | 主机以错误的内容类型提供 `.js` 或 `.wasm`，或文件没有上传：查看“网络”面板。 |
| `WebAssembly.instantiateStreaming` 因响应的 MIME 类型不正确而失败 | 以 `application/wasm` 提供 `.wasm`。 |
| 打开应用的某个路径或在该路径上刷新时返回 404 | 用 `index.html` 响应应用下的所有路径，并设置 `web.base`（[链接到应用内](./web.md#links-into-the-app)）。 |
| 部署后用户看到的仍是旧版本 | `index.html` 被缓存了：用 `Cache-Control: no-cache` 提供它。 |
| 来自 CDN 的文件被 CORS 拦截 | `--asset-url` 指向 CDN 时，CDN 必须针对页面的源返回 `Access-Control-Allow-Origin`。 |
| 在较旧的 iPhone 或 Android WebView 上页面空白 | 浏览器版本低于支持的版本。 |

## 另请参阅

- [发布](./publishing.md)：所有平台、凭据和发布 CI
- [Web](./web.md)：应用如何在浏览器中运行
- [打包：Web](./packaging.md#web)

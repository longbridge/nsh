---
title: Publish on the web
description: "Ship an NSH app as a website: nsh package --target web, the files it writes, base paths and CDNs, cache headers and compression, deploying to static hosts and from CI, and common errors."
order: 19.7
group: publishing
---

# Publish on the web

On the web an app is a static site. `nsh package --target web` writes it; you copy the directory to any web server,
object store or static host. There is no store, no signing and no review, and a deploy updates every user on their next
load. `nsh publish` has nothing to do here.

## Configure the app

| Key | In the site | Notes |
|---|---|---|
| `name` | The page title and the web app manifest's name | |
| `id` | The web app manifest's id | |
| `icon` (or `icon.web`) | The favicon, `apple-touch-icon` and manifest icons | Resized to 32, 180, 192 and 512 pixels ([Icons](./packaging.md#icons)). |
| `appearance` | The page's colors while it loads | |
| `web.base` | The path the site is served under | Needed when the app is opened at its own paths ([Links into the app](./web.md#links-into-the-app)). |
| `web.host` | The page's host script | [The host's functions](./web.md#the-hosts-functions). |

## Build

```sh
nsh package --target web            # a Rust app, from its directory
nsh package App.tsx --target web    # a TSX app
nsh package --target web --run      # and serve it on http://127.0.0.1:4173/ to try it
```

This writes `.nsh/out/<Name>/` (`--out` for another directory): `index.html`, `manifest.webmanifest`, and every other
file named after its content (`nsh.<digest>.wasm`, `app.<digest>.js`, `assets/…`), with Brotli and gzip copies
beside them. A release changes only `index.html` and the files that really changed. The full list of files and the
runtime's size: [Web: Publish an app](./web.md#publish-an-app).

By default the site uses relative URLs, so it works from any directory of any host. To serve the files from a CDN while
the page stays at the app's own address, pass `--asset-url https://cdn.example.com/notes/`.

## Deploy

Any static host works. Two rules make the app load fast and update at once:

| Files | `Cache-Control` |
|---|---|
| `index.html`, `manifest.webmanifest` | `no-cache` |
| everything else | `public, max-age=31536000, immutable` |

Serve `.wasm` as `application/wasm`, and send the precompressed `.br` and `.gz` copies to browsers that accept them
([Deploy it](./web.md#deploy-it) has nginx and Caddy settings). Upload the hashed files before `index.html`, so a
visitor never gets a page whose files are not there yet.

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

A project site lives under `/<repo>/`; relative URLs handle that. An app opened at its own paths sets
`"web": { "base": "/<repo>/" }` and adds a copy of `index.html` as `404.html` (`cp site/index.html site/404.html`
before the upload), which GitHub Pages answers unknown paths with. GitHub Pages cannot set `Cache-Control` per file, so returning visitors revalidate
the hashed files too; a host that lets you set headers loads faster.

### An object store and a CDN

```sh
nsh package --target web --out site
aws s3 sync site s3://example-notes --exclude index.html --exclude manifest.webmanifest \
  --cache-control "public, max-age=31536000, immutable"
aws s3 cp site/index.html s3://example-notes/ --cache-control no-cache
aws s3 cp site/manifest.webmanifest s3://example-notes/ --cache-control no-cache
```

Configure the CDN to serve `.wasm` as `application/wasm` and to compress with Brotli, or to serve the `.br` copies.

## Supported browsers

Safari 16.4 (iOS and iPadOS 16.4), Chrome and Edge 97, and Firefox 108 or later. Earlier browsers show a blank page
([Supported browsers](./web.md#supported-browsers)).

## Common errors

| Problem | What to do |
|---|---|
| A blank page and `Failed to load module script` in the console | The host serves `.js` or `.wasm` with the wrong content type, or the files were not uploaded: check the Network panel. |
| `WebAssembly.instantiateStreaming` fails with an incorrect response MIME type | Serve `.wasm` as `application/wasm`. |
| Opening a path of the app, or reloading on one, gives 404 | Answer every path under the app with `index.html` and set `web.base` ([Links into the app](./web.md#links-into-the-app)). |
| Users see the old version after a deploy | `index.html` is cached: serve it with `Cache-Control: no-cache`. |
| Files from the CDN are blocked by CORS | The CDN must answer with `Access-Control-Allow-Origin` for the page's origin when `--asset-url` names it. |
| A blank page on an older iPhone or Android WebView | The browser is older than the supported ones. |

## See also

- [Publishing](./publishing.md): every platform, credentials and release CI
- [Web](./web.md): how apps run in the browser
- [Packaging: Web](./packaging.md#web)

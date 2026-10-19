---
title: Images and assets
description: Showing images with the Image element - imported assets and @2x files, SVG images, remote URLs, resize modes, placeholders, blur, tint, load events, and Image.getSize / Image.prefetch.
order: 36
group: ui
---

# Images and assets

`<Image>` shows a bitmap or an SVG drawing: a file shipped with the app, or an image downloaded from a URL. Images are read and decoded off the main thread, downsampled to the size the view shows them at, and cached; the main thread only displays the decoded result.

```tsx
import { Image } from "nsh/image";
import logo from "./assets/logo@2x.png";

<Image src={logo} />
<Image src="https://picsum.photos/seed/nsh/600/400" style={{ width: "100%", height: 200, borderRadius: 12 }} />
```

The element's props are listed on [image](../elements/image.md).

## Images in the app

Import an image file like a module. The import gives an `ImageAsset`, `{ uri, width, height }`, and the bundler ships the file with the app:

```tsx
import logo from "./assets/logo@2x.png";

logo; // { uri: "asset:/assets/logo@2x.png", width: 64, height: 64 }
```

Supported files: `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.heic`, and `.svg` ([SVG images](#svg-images)).

**Sizes are in points.** An `@2x` or `@3x` suffix in the file name says the file has 2 or 3 pixels per point, so `logo@2x.png` at 128×128 pixels is a 64×64 point asset. Without a suffix, one pixel is one point. There is no automatic choice between `logo@2x.png` and `logo@3x.png`: the file you import is the one that ships. Export at the highest density you need (`@3x` for the sharpest result on every phone); images are downsampled to the screen when decoded.

**Natural size.** An image from an asset lays out at its natural size when its style leaves the width or height open, so `<Image src={logo} />` needs no style. Set one dimension and `aspectRatio`, or both, to size it differently.

Assets are resolved relative to the importing module, and their `uri` is relative to the app's root (the entry's directory). `nsh build` copies them to `out/assets/`, `nsh package` puts them in the app, and during development the dev server serves them.

> [!TIP]
> `src` also accepts `asset:/path` strings, but only imported files are guaranteed to ship. A path that is never imported works with the dev server, which serves the whole app directory, and is missing from packaged apps. Import every image you use.

## SVG images

Import an `.svg` file like any image. Its size in points is its `width` and `height` (or one of them, with the other from the `viewBox`'s aspect ratio), else its `viewBox`: a CSS pixel of the SVG is a point, and `@2x` suffixes leave SVG files as they are.

```tsx
import logo from "./assets/logo.svg"; // <svg width="120" height="40" …>
import star from "./assets/star.svg"; // <svg viewBox="0 0 24 24" …>

logo; // { uri: "asset:/assets/logo.svg", width: 120, height: 40 }

<Image src={logo} />
<Image src={star} style={{ width: 36, height: 36, tintColor: "#0969da" }} />
```

An SVG is sharp at the size it is shown. The browser draws it as vectors; iOS, macOS and Android draw it at the size the view shows it, in pixels, when the image loads (as they downsample bitmaps; a view that grows afterwards scales that drawing, until the image loads again); Windows, Linux and GPUI draw it once at four pixels per point (up to 2048 pixels a side) and scale it to each view. Everything else works as for bitmaps: `resizeMode`, `tintColor` (through the SVG's opaque shapes, so one-color icons follow the theme), filters, `blurRadius`, placeholders, the load events, `Image.getSize` and `Image.prefetch`. `onLoad` and `Image.getSize` report an SVG's size in whole points; the image lays out at its exact size. Remote SVG files load the same way, through the same downloads and [capability](./sandbox-and-capabilities.md) checks; NSH recognizes an SVG by its content (also gzip-compressed, `.svgz`), whatever its URL ends with.

**What native platforms draw.** iOS, macOS, Android, Windows, Linux and GPUI draw SVGs with one renderer (resvg), so a drawing looks the same on each: shapes and paths, fills and strokes, gradients and patterns, clip paths and masks, opacity, filters and transforms. Text (`<text>`), animation (SMIL and CSS), embedded images (`<image>`) and HTML (`<foreignObject>`) show in the browser; native platforms draw the rest of the SVG, still, and `nsh build` names the files that use them. Export text as outlines (paths) for the same result everywhere.

**A self-contained picture.** On native platforms an SVG draws only what it holds: files, URLs, `data:` images and other documents it references are left out. Native platforms draw SVGs, remote ones included, within bounds that keep memory in check: a document of up to 16 MiB (a compressed one once inflated), a drawing of up to 8192 pixels a side and 4096 × 4096 pixels in all (64 MiB; a larger one is drawn smaller), and layers (groups with opacity, masks, clip paths and filters, which the renderer draws apart) of up to 256 MiB at once. An SVG over the document or layer bound fails to load, with `onError`.

**Reported sizes on the web.** The browser measures SVGs itself, imported or remote: `onLoad` and `Image.getSize` report an SVG's `width` and `height` when it has them, and otherwise the size the browser gives it, which browsers typically derive from a default of 300 × 150 and the `viewBox`'s aspect ratio. Imported SVGs lay out at the bundler's size on every platform. Give SVGs a `width` and `height` for the same reported size everywhere.

## Remote images

Give `src` a URL string or `{ uri }`:

```tsx
<Image src={user().avatarUrl} style={{ width: 48, height: 48, borderRadius: 24, backgroundColor: "#eaeef2" }} />
```

- **Give remote images a size.** Their size is unknown until they load, so without a width and height (or one of them and `aspectRatio`) they collapse to no height.
- **A background color** shows while the image loads, or use `defaultSource` (below).
- Downloads are shared: several views showing the same URL download it once, also when they decode it at different sizes. A download nobody waits for any more is cancelled.
- Decoded images are cached in memory (the cache is purged under memory pressure), keyed by URL, decode size and blur radius.
- When the app declares [capabilities](./sandbox-and-capabilities.md), the image's host must be allowed under `network`; a refused image fails with `onError`.

## Resize modes

`resizeMode` decides how an image fills a box of a different shape:

| Value | Result |
|---|---|
| `"cover"` (default) | Fills the box, keeping the aspect ratio, cropping what overflows |
| `"contain"` | Fits inside the box, keeping the aspect ratio, leaving empty bands |
| `"stretch"` | Fills the box exactly, distorting the image |
| `"center"` | Natural size, centered |

```tsx
<Image src={photo} resizeMode="contain" style={{ width: 140, height: 140, backgroundColor: "#eaeef2" }} />
```

The Tailwind classes `object-cover`, `object-contain`, ... do the same.

## Styling images

Images are views: they take `borderRadius` (also per corner), borders, shadows, `opacity` and transforms. Corners clip the image.

```tsx
<Image src={avatar} style={{ width: 90, height: 90, borderRadius: 45, borderWidth: 3, borderColor: "#8250df" }} />
```

- **`tintColor`** (a style) recolors the image's opaque pixels: use it for one-color icons that follow the theme.
- **`filter`** (`grayscale()`, `sepia()`, `blur()`, ...) applies to images on both platforms ([Filters](./styling.md#filters-and-blending)).

```tsx
<Image src={icon} style={{ width: 24, height: 24, tintColor: active() ? "#0969da" : "#6e7781" }} />
```

## Placeholders, fading and blur

| Prop | Description |
|---|---|
| `defaultSource` | An image shown until `src` has loaded; best a local asset |
| `fadeDuration` | Milliseconds the loaded image fades in over. Default 0 on iOS, 300 on Android |
| `blurRadius` | Blurs the image by this radius in points, applied once when decoding |

```tsx
<Image
  src={`https://picsum.photos/seed/${id()}/300/300`}
  defaultSource={placeholder}
  fadeDuration={250}
  style={{ width: 100, height: 100, borderRadius: 12 }}
/>
<Image src={cover} blurRadius={16} style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0 }} />
```

## Animated images

Animated GIF, WebP and APNG files play automatically, as assets or from URLs. They take the same styles and resize modes; a tinted animated image shows its first frame on iOS.

## Load events

| Event | Payload | When |
|---|---|---|
| `onLoadStart` | | Loading starts |
| `onProgress` | `{ loaded, total }` in bytes (`total` 0 when unknown) | During a download, at most every 100 ms |
| `onLoad` | `{ width, height }` | The image is ready; the source's size in pixels (an SVG's in points, [on the web as the browser measures it](#svg-images)) |
| `onError` | `{ message }` | Loading or decoding failed |
| `onLoadEnd` | | After `onLoad` or `onError` |

```tsx
const status = signal("loading…");

<Image
  src={url}
  onLoad={(e) => status.set(`${e.width}×${e.height}`)}
  onError={(e) => status.set(e.message)}
  style={{ width: 90, height: 60 }}
/>
```

`onLoad` reports the source image's size, not the size it was decoded at.

## Image.getSize and Image.prefetch

The same `Image` that renders the element also works with images that are not on screen:

```tsx
import { Image } from "nsh/image";

const size = await Image.getSize("https://example.com/photo.jpg");   // { width, height } in pixels
const ok = await Image.prefetch("https://example.com/next.jpg");      // true once the bytes are in memory
```

| Method | Returns |
|---|---|
| `Image.getSize(source)` | The image's size in pixels (an SVG's in points, [on the web as the browser measures it](#svg-images)). Rejects when it cannot be loaded |
| `Image.prefetch(source)` | `true` once the image's bytes are loaded |

`source` is a URL string or `{ uri }`. Both keep the downloaded bytes for the views that show the image next, so an image prefetched before it is shown appears without a download.

A common use is sizing a remote image by its aspect ratio:

```tsx
function RemoteImage(props: { uri: string }) {
  const ratio = signal(1);
  Image.getSize(props.uri).then((s) => ratio.set(s.width / s.height));
  return <Image src={props.uri} style={{ width: "100%", aspectRatio: ratio() }} />;
}
```

## Other assets

| Import | Gives | Page |
|---|---|---|
| `.ttf`, `.otf` | The font's family name, registered before the first frame | [Fonts](./fonts.md) |
| `.json` | The parsed value, bundled into the script | [Project structure](./project-structure.md#modules) |

### A module's own URL

`import.meta.url` is the URL of the module it is written in, so `new URL("./data.json", import.meta.url)` names a file beside it. A local or packaged desktop app's modules have `file:` URLs in the app's directory, which `fs/promises` reads under a `${pluginDir}` grant; a web app's are the page's; a packaged mobile app's are `asset:/` URLs like those of imported images. Import an image to ship it: a URL made this way names the file, it does not package it.

Bitmap images are also drawn on a canvas with `drawImage` ([Canvas](./canvas.md)), and the gradients of `backgroundImage` replace many decorative images ([Styling](./styling.md#gradients)).

## Related

- [image element](../elements/image.md): every prop.
- [`nsh/image`](../modules/nsh-image.md): `Image`, with `getSize` and `prefetch`.
- [Lists](./lists.md): long lists of images recycle their views.

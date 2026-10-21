---
title: Images and assets
description: Showing images in Rust and TSX - the app's assets and @2x files, remote URLs, resize modes, placeholders, blur, tint, load events, and reading an image's size or prefetching it.
order: 36
group: ui
---

# Images and assets

The image element (`image(..)` in Rust, `<Image>` in TSX) shows a bitmap: a file shipped with the app, or an image downloaded from a URL. Images are read and decoded off the main thread, downsampled to the size the view shows them at, and cached; the main thread only displays the decoded result.

```rust tabs
const LOGO: ImageAsset = asset!("logo@2x.png");

image(LOGO)
image("https://picsum.photos/seed/nsh/600/400").w_full().h(px(200.0)).rounded(px(12.0))
```

```tsx tabs
import { Image } from "nsh";
import logo from "./assets/logo@2x.png";

<Image src={logo} />
<Image src="https://picsum.photos/seed/nsh/600/400" style={{ width: "100%", height: 200, borderRadius: 12 }} />
```

The element's props are listed on [image](../elements/image.md).

## Images in the app

A Rust crate keeps its files in its `assets/` directory: `asset!("logo@2x.png")` (the path relative to `assets/`) is an `ImageAsset`, `{ uri, size, scales }`, read when the crate compiles, and `registry.assets(assets!())` in the crate's `#[native_module]` function ships every file of the directory with the app. In TSX, import an image file like a module: the import gives an `ImageAsset`, `{ uri, width, height, scales? }`, and the bundler ships the file with the app:

```rust tabs
const LOGO: ImageAsset = asset!("logo@2x.png");

// LOGO.uri: "asset:/@my-app/logo@2x.png", LOGO.size: 64 × 64 points

#[native_module("app/native")]
fn native(registry: &mut Registry) {
    registry.assets(assets!());
}
```

```tsx tabs
import logo from "./assets/logo@2x.png";

logo; // { uri: "asset:/assets/logo@2x.png", width: 64, height: 64 }
```

Supported files: `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.heic`.

**Sizes are in points.** An `@2x` or `@3x` suffix in the file name says the file has 2 or 3 pixels per point, so `logo@2x.png` at 128×128 pixels is a 64×64 point asset. Without a suffix, one pixel is one point. A file named with a suffix is the one that shows; a file named without one shows its [pixel-density variants](#pixel-densities) when it has them.

**Natural size.** An image from an asset lays out at its natural size when its style leaves the width or height open, so `image(LOGO)` and `<Image src={logo} />` need no style. Set one dimension and the aspect ratio, or both, to size it differently.

In Rust, a missing or broken file fails the build, and a changed one builds the crate again. In TSX, assets are resolved relative to the importing module, and their `uri` is relative to the app's root (the entry's directory). `nsh build` copies them to `out/assets/`, `nsh package` puts them in the app, and during development the dev server serves them.

> [!TIP]
> In TSX, `src` also accepts `asset:/path` strings, and imported files are the ones guaranteed to ship. A path that is never imported works with the dev server, which serves the whole app directory, and is missing from packaged apps. Import every image you use. A Rust crate ships its whole `assets/` directory through `assets!()`.

### Pixel densities

Name the file without a suffix and put its denser versions next to it, and the app loads the one that fits the screen:

```text
assets/
├── logo.png       64×64 pixels
├── logo@2x.png    128×128
└── logo@3x.png    192×192
```

```rust tabs
const LOGO: ImageAsset = asset!("logo.png");

// LOGO.size: 64 × 64 points, LOGO.scales: [1.0, 2.0, 3.0]
```

```tsx tabs
import logo from "./assets/logo.png";

logo; // { uri: "asset:/assets/logo.png", width: 64, height: 64, scales: [1, 2, 3] }
```

- `asset!` and the bundler look for `@1.5x`, `@2x`, `@3x` and `@4x` siblings of an image named without a suffix. `scales` lists them, with 1 for the named file; an image without siblings has no scales (in TSX it imports as `{ uri, width, height }`). The bundler ships the siblings it finds, and `assets!()` ships the whole directory.
- The image loads the smallest scale at least the screen's pixel ratio, otherwise the largest, as React Native picks: `logo.png` on a 1× screen, `logo@2x.png` on a 2× one, `logo@3x.png` on a 3× phone (and at 2.625). On the web only that file is downloaded.
- When the pixel ratio changes (a window moved to another screen, browser zoom), the images whose choice changes load again, with their [load events](#load-events). Their size stays the named file's size, in points.
- A variant whose pixels are not the named file's size times its scale gets a build warning in TSX and fails the build in Rust.
- A file named with a density suffix (`./logo@3x.png`, `asset!("logo@3x.png")`) is that file alone.

One file is fine too: export it at the highest density you need (`@3x` for the sharpest result on every phone) and name it with its suffix; images are downsampled to the screen when decoded. Variants let each screen download, decode and keep the file that fits it.

## Remote images

Give the element a URL: a string in Rust (a closure for one that changes), a URL string or `{ uri }` in TSX:

```rust tabs
image(move || user.with(|u| u.avatar_url.clone()))
    .size(px(48.0))
    .rounded(px(24.0))
    .bg(rgb(0xeaeef2))
```

```tsx tabs
<Image src={user().avatarUrl} style={{ width: 48, height: 48, borderRadius: 24, backgroundColor: "#eaeef2" }} />
```

- **Give remote images a size.** Their size is unknown until they load, so without a width and height (or one of them and an aspect ratio) they collapse to no height.
- **A background color** shows while the image loads, or use a placeholder (below).
- Downloads are shared: several views showing the same URL download it once, also when they decode it at different sizes. A download nobody waits for any more is cancelled.
- Decoded images are cached in memory (the cache is purged under memory pressure), keyed by URL, decode size and blur radius.
- When the app declares [capabilities](./sandbox-and-capabilities.md), the image's host must be allowed under `network`; a refused image fails with its error event.

## Resize modes

The resize mode (`.resize_mode(..)` in Rust, `resizeMode` in TSX) decides how an image fills a box of a different shape:

| Value | Result |
|---|---|
| `"cover"` (`ResizeMode::Cover`, default) | Fills the box, keeping the aspect ratio, cropping what overflows |
| `"contain"` (`ResizeMode::Contain`) | Fits inside the box, keeping the aspect ratio, leaving empty bands |
| `"stretch"` (`ResizeMode::Stretch`) | Fills the box exactly, distorting the image |
| `"center"` (`ResizeMode::Center`) | Natural size, centered |

```rust tabs
image(photo).resize_mode(ResizeMode::Contain).size(px(140.0)).bg(rgb(0xeaeef2))
```

```tsx tabs
<Image src={photo} resizeMode="contain" style={{ width: 140, height: 140, backgroundColor: "#eaeef2" }} />
```

The Tailwind classes `object-cover`, `object-contain`, ... do the same.

## Styling images

Images are views: they take corner radii (also per corner), borders, shadows, opacity and transforms. Corners clip the image.

```rust tabs
image(avatar).size(px(90.0)).rounded(px(45.0)).border(px(3.0)).border_color(rgb(0x8250df))
```

```tsx tabs
<Image src={avatar} style={{ width: 90, height: 90, borderRadius: 45, borderWidth: 3, borderColor: "#8250df" }} />
```

- **Tint** (`.tint_color(..)` in Rust, the `tintColor` style in TSX) recolors the image's opaque pixels: use it for one-color icons that follow the theme.
- **Filters** (`.grayscale(..)`, `.sepia(..)`, `.blur(..)`; `filter` in TSX) apply to images on both platforms ([Filters](./styling.md#filters-and-blending)).

```rust tabs
image(ICON).size(px(24.0)).tint_color(AdaptiveColor::rgb(0x0969da, 0x58a6ff))
```

```tsx tabs
<Image src={icon} style={{ width: 24, height: 24, tintColor: active() ? "#0969da" : "#6e7781" }} />
```

## Placeholders, fading and blur

| Prop | Rust | Description |
|---|---|---|
| `defaultSource` | `.placeholder(..)` | An image shown until `src` has loaded; best a local asset |
| `fadeDuration` | `.fade_duration(ms)` | Milliseconds the loaded image fades in over. Default 0 on iOS, 300 on Android |
| `blurRadius` | `.blur_radius(px(..))` | Blurs the image by this radius in points, applied once when decoding |

```rust tabs
image(move || format!("https://picsum.photos/seed/{}/300/300", id.get()))
    .placeholder(PLACEHOLDER)
    .fade_duration(250.0)
    .size(px(100.0))
    .rounded(px(12.0))
image(cover).blur_radius(px(16.0)).absolute().inset_0()
```

```tsx tabs
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

| Event | Rust | Payload | When |
|---|---|---|---|
| `onLoadStart` | `.on_load_start(..)` | | Loading starts |
| `onProgress` | `.on_progress(..)` | `{ loaded, total }` in bytes (`total` 0 when unknown) | During a download, at most every 100 ms |
| `onLoad` | `.on_load(..)` | `{ width, height }` | The image is ready; the source's size in pixels |
| `onError` | `.on_error(..)` | `{ message }` (a `&str` in Rust) | Loading or decoding failed |
| `onLoadEnd` | `.on_load_end(..)` | | After `onLoad` or `onError` |

```rust tabs
let status = signal(SharedString::from("loading…"));

image(url)
    .on_load(move |size: &Size<Points>| status.set(format!("{}×{}", size.width.0, size.height.0).into()))
    .on_error(move |message: &str| status.set(message.to_owned().into()))
    .w(px(90.0))
    .h(px(60.0))
```

```tsx tabs
const status = signal("loading…");

<Image
  src={url}
  onLoad={(e) => status.set(`${e.width}×${e.height}`)}
  onError={(e) => status.set(e.message)}
  style={{ width: 90, height: 60 }}
/>
```

The load event reports the source image's size, not the size it was decoded at.

An image with [pixel-density variants](#pixel-densities) loads one of its files, and the load event reports that file's pixels: 192×192 for `logo@3x.png` on a 3× screen, where `Image.getSize(logo)` resolves with the imported file's 64×64. When the pixel ratio changes and the image loads another variant, that load reports its events too: `onLoadStart`, then `onLoad` with the new file's size (or `onError`), then `onLoadEnd`.

## An image's size and prefetching

Images that are not on screen can be measured and loaded ahead: `image_size(..)` and `prefetch_image(..)` in Rust, `Image.getSize` and `Image.prefetch` (on the same `Image` that renders the element) in TSX:

```rust tabs
spawn(async move {
    // Ok(ImageSize { width, height }), in pixels.
    let size = image_size("https://example.com/photo.jpg").await;
    // true once the bytes are in memory.
    let ok = prefetch_image("https://example.com/next.jpg").await;
});
```

```tsx tabs
import { Image } from "nsh";

const size = await Image.getSize("https://example.com/photo.jpg");   // { width, height } in pixels
const ok = await Image.prefetch("https://example.com/next.jpg");      // true once the bytes are in memory
```

| Rust | TSX | Returns |
|---|---|---|
| `image_size(uri)` | `Image.getSize(source)` | The image's size in pixels; an error when it cannot be loaded |
| `prefetch_image(uri)` | `Image.prefetch(source)` | `true` once the image's bytes are loaded |

In TSX, `source` is a URL string, `{ uri }` or an imported image. For an imported image with [variants](#pixel-densities), both load the variant the views load on this screen, and `getSize` gives the imported file's size. Both keep the downloaded bytes for the views that show the image next, so an image prefetched before it is shown appears without a download.

A common use is sizing a remote image by its aspect ratio:

```rust tabs
fn remote_image(uri: &'static str) -> impl IntoElement {
    let ratio = signal(1.0_f32);
    spawn(async move {
        if let Ok(size) = image_size(uri).await {
            ratio.set(size.width.0 as f32 / size.height.0 as f32);
        }
    });
    image(uri).w_full().aspect_ratio(ratio)
}
```

```tsx tabs
function RemoteImage(props: { uri: string }) {
  const ratio = signal(1);
  Image.getSize(props.uri).then((s) => ratio.set(s.width / s.height));
  return <Image src={props.uri} style={{ width: "100%", aspectRatio: ratio() }} />;
}
```

## Other assets

| File | Rust (`asset!`) | TSX import | Page |
|---|---|---|---|
| `.ttf`, `.otf` | A `FontAsset` with its family name | The font's family name | [Fonts](./fonts.md) |
| `.json` and other files | A `FileAsset` with its URI | `.json`: the parsed value, bundled into the script | [Project structure](./project-structure.md#modules) |

Fonts are registered before the first frame in both.

### A module's own URL

In TSX, `import.meta.url` is the URL of the module it is written in, so `new URL("./data.json", import.meta.url)` names a file beside it. A local or packaged desktop app's modules have `file:` URLs in the app's directory, which `fs/promises` reads under a `${pluginDir}` grant; a web app's are the page's; a packaged mobile app's are `asset:/` URLs like those of imported images. Import an image to ship it: a URL made this way names the file, it does not package it.

Images are also drawn on a canvas with `draw_image` / `drawImage` ([Canvas](./canvas.md)), and gradients replace many decorative images ([Styling](./styling.md#gradients)).

## Related

- [image element](../elements/image.md): every prop, with `getSize` and `prefetch`.
- [Lists](./lists.md): long lists of images recycle their views.

---
title: Image
description: The element that shows a bundled or remote image, with placeholders, blur, fades, tinting and load events.
order: 4
---

# Image

`<Image>`, imported from [`nsh/image`](../modules/nsh-image.md), shows a picture from the app's assets or from a URL. Images are downloaded and decoded off the main thread,
downsampled to the size the view shows them at, and cached in memory. Animated GIF, WebP and APNG images play.

```tsx
import { View } from "nsh";
import { Image } from "nsh/image";
import logo from "./assets/logo.png";

<View style={{ flexDirection: "row", gap: 12 }}>
  <Image src={logo} />
  <Image
    src="https://example.com/avatar.jpg"
    defaultSource={logo}
    resizeMode="cover"
    style={{ width: 64, height: 64, borderRadius: 32 }}
  />
</View>;
```

An imported asset knows its size: without a `width` or `height` the image takes it, and with only one of them the other
follows the asset's aspect ratio. A URL has no known size before it loads, so give remote images a `width` and a `height`
(or an `aspectRatio` with one of them).

## Sources

`src` (and `defaultSource`) takes:

| Value | Example | Description |
|---|---|---|
| An imported asset | `import logo from "./logo.png"` | An `ImageAsset` `{ uri, width, height }`, sized in points: a `@2x` file counts half its pixels. `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp` and `.heic` can be imported, and `.svg`, sized by its `width` and `height` or `viewBox` ([SVG images](../docs/images-and-assets.md#svg-images)). |
| A URL | `"https://example.com/a.jpg"` | Downloaded and cached. Downloads of the same URL are shared between views. |
| An asset path | `"asset:/images/a.png"` | A file of the app's assets. |
| `{ uri }` | `{ uri: "https://…" }` | The same as the string. |

During development assets are served by the dev server; `nsh build` and `nsh package` copy them into the app. See
[images and assets](../docs/images-and-assets.md).

> [!NOTE]
> If the app declares [capabilities](../docs/sandbox-and-capabilities.md), remote images are allowed only from the hosts
> it lists; a refused image fails with `onError`. `asset:` URIs are always allowed.

## Props

Besides the [common props](./index.md#common-props) (with `style` typed as `ImageStyle`):

| Prop | Type | Default | Description |
|---|---|---|---|
| `src` | `string \| ImageAsset \| { uri: string }` | | The image. |
| `resizeMode` | `"cover" \| "contain" \| "stretch" \| "center"` | `"cover"` | How the image fits its box. |
| `defaultSource` | `string \| ImageAsset \| { uri: string }` | | Shown until `src` has loaded; best a local asset. |
| `blurRadius` | `number` | | Blurs the image by this radius in points (applied once while decoding, off the main thread). |
| `fadeDuration` | `number` | iOS `0`, Android `300` | How long a loaded image fades in, in milliseconds. |
| `onLoadStart` | `() => void` | | Loading began. |
| `onProgress` | `(e: { loaded: number; total: number }) => void` | | Download progress in bytes (`total` is 0 when unknown), at most every 100 ms. |
| `onLoad` | `(e: { width: number; height: number }) => void` | | Loaded; the source image's size in pixels (not the decoded, downsampled one; an SVG's size in points, [on the web as the browser measures it](../docs/images-and-assets.md#svg-images)). |
| `onError` | `(e: { message: string }) => void` | | Loading failed. |
| `onLoadEnd` | `() => void` | | After `onLoad` or `onError`. |

`ImageStyle` adds `tintColor` to the view styles: it recolors a template image (an icon drawn in one color).

```tsx
<Image src={icon} style={{ width: 24, height: 24, tintColor: "#0969da" }} />
```

## Events

| Event | Payload | When |
|---|---|---|
| `onLoadStart` | none | Loading began. |
| `onProgress` | `{ loaded, total }` | Bytes arrived (at most every 100 ms). |
| `onLoad` | `{ width, height }` | The image is shown; its source size in pixels (an SVG's in points, [on the web as the browser measures it](../docs/images-and-assets.md#svg-images)). |
| `onError` | `{ message }` | Loading or decoding failed, or the host is not allowed. |
| `onLoadEnd` | none | After `onLoad` or `onError`. |

```tsx
const state = signal("loading");
<Image
  src={url}
  style={{ width: "100%", aspectRatio: 16 / 9 }}
  onLoad={(e) => state.set(`${e.width}×${e.height} px`)}
  onError={(e) => state.set(e.message)}
/>;
```

To know an image's size or load it before showing it, call `Image.getSize(uri)` and `Image.prefetch(uri)`: the same
`Image` from [`nsh/image`](../modules/nsh-image.md) that renders the element carries them.

## Ref methods

None of its own.

## Platform notes

- The cache key includes the decode size and the blur radius: the same URL shown at two sizes is decoded twice but
  downloaded once.
- iOS plays animated images as a keyframe animation (up to 600 frames); a tinted animated image shows its first frame.
- Android plays animated images from API 28 (`AnimatedImageDrawable`); animated images are not cached there and are not
  blurred.
- On Android borders are drawn above the image, as on iOS.

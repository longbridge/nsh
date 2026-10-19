---
title: Image
description: 显示打包资源或远程图片的元素，支持占位图、模糊、淡入、着色和加载事件。
order: 4
---

# Image

`<Image>` 显示来自应用资源或 URL 的图片。图片在主线程之外下载和解码，按视图显示的尺寸降采样，并缓存在内存中。动态 GIF、WebP 和 APNG 图片会自动播放。

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

导入的资源知道自己的尺寸：不设 `width` 和 `height` 时图片使用该尺寸；只设其中一个时，另一个按资源的宽高比计算。URL 在加载完成前没有已知尺寸，所以要给远程图片设置 `width` 和 `height`（或者其中一个加上 `aspectRatio`）。

## 图片来源

`src`（以及 `defaultSource`）接受：

| 值 | 示例 | 说明 |
|---|---|---|
| 导入的资源 | `import logo from "./logo.png"` | 一个 `ImageAsset` `{ uri, width, height }`，以点为单位：`@2x` 文件按像素的一半计算。可以导入 `.png`、`.jpg`、`.jpeg`、`.gif`、`.webp` 和 `.heic`，以及按 `width` 和 `height` 或 `viewBox` 确定尺寸的 `.svg`（[SVG 图片](../docs/images-and-assets.md#svg-图片)）。 |
| URL | `"https://example.com/a.jpg"` | 下载并缓存。同一 URL 的下载在多个视图之间共享。 |
| 资源路径 | `"asset:/images/a.png"` | 应用资源中的文件。 |
| `{ uri }` | `{ uri: "https://…" }` | 与字符串形式相同。 |

开发期间资源由开发服务器提供；`nsh build` 和 `nsh package` 会把它们拷贝进应用。参见[图片与资源](../docs/images-and-assets.md)。

> [!NOTE]
> 如果应用声明了[能力](../docs/sandbox-and-capabilities.md)，远程图片只能来自其中列出的主机；被拒绝的图片会以 `onError` 失败。`asset:` URI 始终允许。

## 属性

除[通用属性](./index.md)外（`style` 的类型为 `ImageStyle`）：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `src` | `string \| ImageAsset \| { uri: string }` | | 图片。 |
| `resizeMode` | `"cover" \| "contain" \| "stretch" \| "center"` | `"cover"` | 图片如何适配其盒子。 |
| `defaultSource` | `string \| ImageAsset \| { uri: string }` | | `src` 加载完成前显示的图片；最好是本地资源。 |
| `blurRadius` | `number` | | 以该半径（点）模糊图片（在解码时于主线程外处理一次）。 |
| `fadeDuration` | `number` | iOS `0`，Android `300` | 图片加载完成后淡入的时长，单位毫秒。 |
| `onLoadStart` | `() => void` | | 开始加载。 |
| `onProgress` | `(e: { loaded: number; total: number }) => void` | | 以字节计的下载进度（未知时 `total` 为 0），最多每 100 ms 一次。 |
| `onLoad` | `(e: { width: number; height: number }) => void` | | 加载完成；给出源图片的像素尺寸（不是解码、降采样后的尺寸；SVG 为它的点尺寸，[在 Web 上为浏览器测得的尺寸](../docs/images-and-assets.md#svg-图片)）。 |
| `onError` | `(e: { message: string }) => void` | | 加载失败。 |
| `onLoadEnd` | `() => void` | | 在 `onLoad` 或 `onError` 之后。 |

`ImageStyle` 在视图样式之上增加了 `tintColor`：为模板图片（单色绘制的图标）重新着色。

```tsx
<Image src={icon} style={{ width: 24, height: 24, tintColor: "#0969da" }} />
```

## 事件

| 事件 | 载荷 | 触发时机 |
|---|---|---|
| `onLoadStart` | 无 | 开始加载。 |
| `onProgress` | `{ loaded, total }` | 收到数据（最多每 100 ms 一次）。 |
| `onLoad` | `{ width, height }` | 图片已显示；给出源图片的像素尺寸（SVG 为点，[在 Web 上为浏览器测得的尺寸](../docs/images-and-assets.md#svg-图片)）。 |
| `onError` | `{ message }` | 加载或解码失败，或主机不被允许。 |
| `onLoadEnd` | 无 | 在 `onLoad` 或 `onError` 之后。 |

```tsx
const state = signal("加载中");
<Image
  src={url}
  style={{ width: "100%", aspectRatio: 16 / 9 }}
  onLoad={(e) => state.set(`${e.width}×${e.height} px`)}
  onError={(e) => state.set(e.message)}
/>;
```

如果需要在显示前获取图片尺寸或预先加载，请调用 `Image.getSize(uri)` 和 `Image.prefetch(uri)`：它们就挂在渲染这个元素的、来自 [`nsh/image`](../modules/nsh-image.md) 的同一个 `Image` 上。

## ref 方法

没有自己的方法。

## 平台说明

- 缓存键包含解码尺寸和模糊半径：同一 URL 以两种尺寸显示会解码两次，但只下载一次。
- iOS 以关键帧动画播放动态图片（最多 600 帧）；着色的动态图片只显示第一帧。
- Android 从 API 28 起播放动态图片（`AnimatedImageDrawable`）；在 Android 上动态图片不缓存，也不会被模糊。
- 在 Android 上边框绘制在图片之上，与 iOS 一致。

---
title: 图片与资源
description: 用 Image 元素显示图片：导入的资源与 @2x 文件、远程 URL、缩放模式、占位图、模糊、着色、加载事件，以及 Image.getSize / Image.prefetch。
order: 36
group: ui
---

# 图片与资源

`<Image>` 显示一张位图：随应用一起发布的文件，或从 URL 下载的图片。图片在主线程之外读取和解码，按视图实际显示的尺寸降采样后缓存；主线程只负责显示解码结果。

```tsx
import { Image } from "nsh/image";
import logo from "./assets/logo@2x.png";

<Image src={logo} />
<Image src="https://picsum.photos/seed/nsh/600/400" style={{ width: "100%", height: 200, borderRadius: 12 }} />
```

元素的属性列在 [image](../elements/image.md) 中。

## 应用内的图片

像导入模块一样导入图片文件。导入得到一个 `ImageAsset`，即 `{ uri, width, height }`，打包器会把文件随应用一起发布：

```tsx
import logo from "./assets/logo@2x.png";

logo; // { uri: "asset:/assets/logo@2x.png", width: 64, height: 64 }
```

支持的文件：`.png`、`.jpg`、`.jpeg`、`.gif`、`.webp`、`.heic`。

**尺寸以点为单位。** 文件名中的 `@2x` 或 `@3x` 后缀表示文件每个点有 2 或 3 个像素，因此 128×128 像素的 `logo@2x.png` 是一个 64×64 点的资源。没有后缀时，一个像素就是一个点。NSH 不会在 `logo@2x.png` 和 `logo@3x.png` 之间自动选择：你导入的那个文件就是最终发布的文件。请按需要的最高密度导出（`@3x` 能在所有手机上得到最清晰的效果）；解码时图片会降采样到屏幕所需的大小。

**自然尺寸。** 当样式没有指定宽度或高度时，来自资源的图片按自然尺寸布局，所以 `<Image src={logo} />` 不需要任何样式。若要改变大小，设置其中一边加 `aspectRatio`，或同时设置两边。

资源路径相对于导入它的模块解析，其 `uri` 相对于应用根目录（入口文件所在目录）。`nsh build` 会把它们复制到 `out/assets/`，`nsh package` 会把它们放进应用，开发时则由开发服务器提供。

> [!TIP]
> `src` 也接受 `asset:/path` 字符串，但只有被导入的文件才保证会被发布。从未导入的路径在开发服务器下可以使用（它提供整个应用目录），在打包后的应用中却会缺失。请导入你用到的每一张图片。

## 远程图片

给 `src` 传一个 URL 字符串或 `{ uri }`：

```tsx
<Image src={user().avatarUrl} style={{ width: 48, height: 48, borderRadius: 24, backgroundColor: "#eaeef2" }} />
```

- **为远程图片指定尺寸。** 加载完成前无法得知它们的尺寸，所以如果没有宽和高（或其中一边加 `aspectRatio`），它们的高度会塌缩为 0。
- 加载期间会显示**背景色**，也可以使用 `defaultSource`（见下文）。
- 下载是共享的：多个视图显示同一个 URL 时只下载一次，即使它们按不同尺寸解码也是如此。没有任何视图再等待的下载会被取消。
- 解码后的图片缓存在内存中（内存紧张时会清理），以 URL、解码尺寸和模糊半径为键。
- 当应用声明了[能力](./sandbox-and-capabilities.md)时，图片的主机必须在 `network` 中被允许；被拒绝的图片会以 `onError` 失败。

## 缩放模式

`resizeMode` 决定图片如何填充形状不同的框：

| 取值 | 效果 |
|---|---|
| `"cover"`（默认） | 保持宽高比填满整个框，裁掉溢出部分 |
| `"contain"` | 保持宽高比完整放入框内，留出空白 |
| `"stretch"` | 精确填满框，图片会变形 |
| `"center"` | 自然尺寸，居中 |

```tsx
<Image src={photo} resizeMode="contain" style={{ width: 140, height: 140, backgroundColor: "#eaeef2" }} />
```

Tailwind 类 `object-cover`、`object-contain` 等效果相同。

## 图片样式

图片也是视图：它们支持 `borderRadius`（也可按角设置）、边框、阴影、`opacity` 和变换。圆角会裁剪图片。

```tsx
<Image src={avatar} style={{ width: 90, height: 90, borderRadius: 45, borderWidth: 3, borderColor: "#8250df" }} />
```

- **`tintColor`**（一个样式）会给图片的不透明像素重新着色：适合随主题变色的单色图标。
- **`filter`**（`grayscale()`、`sepia()`、`blur()` 等）在两个平台上都可用于图片（[滤镜](./styling.md)）。

```tsx
<Image src={icon} style={{ width: 24, height: 24, tintColor: active() ? "#0969da" : "#6e7781" }} />
```

## 占位图、淡入与模糊

| 属性 | 说明 |
|---|---|
| `defaultSource` | `src` 加载完成前显示的图片；最好是本地资源 |
| `fadeDuration` | 加载完成的图片淡入所用的毫秒数。iOS 默认 0，Android 默认 300 |
| `blurRadius` | 按此半径（点）模糊图片，在解码时一次性应用 |

```tsx
<Image
  src={`https://picsum.photos/seed/${id()}/300/300`}
  defaultSource={placeholder}
  fadeDuration={250}
  style={{ width: 100, height: 100, borderRadius: 12 }}
/>
<Image src={cover} blurRadius={16} style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0 }} />
```

## 动图

动态 GIF、WebP 和 APNG 文件会自动播放，无论是资源还是来自 URL。它们支持相同的样式和缩放模式；在 iOS 上，着色后的动图只显示第一帧。

## 加载事件

| 事件 | 负载 | 触发时机 |
|---|---|---|
| `onLoadStart` | | 开始加载 |
| `onProgress` | `{ loaded, total }`，单位为字节（未知时 `total` 为 0） | 下载过程中，最多每 100 ms 一次 |
| `onLoad` | `{ width, height }` | 图片就绪；源图的像素尺寸 |
| `onError` | `{ message }` | 加载或解码失败 |
| `onLoadEnd` | | 在 `onLoad` 或 `onError` 之后 |

```tsx
const status = signal("loading…");

<Image
  src={url}
  onLoad={(e) => status.set(`${e.width}×${e.height}`)}
  onError={(e) => status.set(e.message)}
  style={{ width: 90, height: 60 }}
/>
```

`onLoad` 报告的是源图的尺寸，而不是解码时的尺寸。

## Image.getSize 与 Image.prefetch

渲染元素的同一个 `Image` 也可以处理不在屏幕上的图片：

```tsx
import { Image } from "nsh/image";

const size = await Image.getSize("https://example.com/photo.jpg");   // { width, height } in pixels
const ok = await Image.prefetch("https://example.com/next.jpg");      // true once the bytes are in memory
```

| 方法 | 返回值 |
|---|---|
| `Image.getSize(source)` | 图片的像素尺寸。无法加载时 reject |
| `Image.prefetch(source)` | 图片字节加载完成后为 `true` |

`source` 是 URL 字符串或 `{ uri }`。两者都会为接下来显示该图片的视图保留已下载的字节，因此先预取再显示的图片无需再次下载。

一个常见用法是按宽高比确定远程图片的尺寸：

```tsx
function RemoteImage(props: { uri: string }) {
  const ratio = signal(1);
  Image.getSize(props.uri).then((s) => ratio.set(s.width / s.height));
  return <Image src={props.uri} style={{ width: "100%", aspectRatio: ratio() }} />;
}
```

## 其他资源

| 导入 | 得到 | 页面 |
|---|---|---|
| `.ttf`、`.otf` | 字体的字体族名，在第一帧之前注册 | [字体](./fonts.md) |
| `.json` | 解析后的值，打包进脚本 | [项目结构](./project-structure.md) |

图片也可以用 `drawImage` 绘制在画布上（[Canvas](./canvas.md)），而 `backgroundImage` 的渐变能替代许多装饰性图片（[样式](./styling.md)）。

## 相关内容

- [image 元素](../elements/image.md)：所有属性。
- [`nsh/image`](../modules/nsh-image.md)：`Image`，以及 `getSize` 和 `prefetch`。
- [列表](./lists.md)：长图片列表会复用视图。

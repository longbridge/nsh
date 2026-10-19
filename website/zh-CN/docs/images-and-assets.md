---
title: 图片与资源
description: 用 Image 元素显示图片：导入的资源与 @2x 文件、SVG 图片、远程 URL、缩放模式、占位图、模糊、着色、加载事件，以及 Image.getSize / Image.prefetch。
order: 36
group: ui
---

# 图片与资源

`<Image>` 显示一张位图或一幅 SVG 图：随应用一起发布的文件，或从 URL 下载的图片。图片在主线程之外读取和解码，按视图实际显示的尺寸降采样后缓存；主线程只负责显示解码结果。

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

支持的文件：`.png`、`.jpg`、`.jpeg`、`.gif`、`.webp`、`.heic`，以及 `.svg`（[SVG 图片](#svg-图片)）。

**尺寸以点为单位。** 文件名中的 `@2x` 或 `@3x` 后缀表示文件每个点有 2 或 3 个像素，因此 128×128 像素的 `logo@2x.png` 是一个 64×64 点的资源。没有后缀时，一个像素就是一个点。NSH 不会在 `logo@2x.png` 和 `logo@3x.png` 之间自动选择：你导入的那个文件就是最终发布的文件。请按需要的最高密度导出（`@3x` 能在所有手机上得到最清晰的效果）；解码时图片会降采样到屏幕所需的大小。

**自然尺寸。** 当样式没有指定宽度或高度时，来自资源的图片按自然尺寸布局，所以 `<Image src={logo} />` 不需要任何样式。若要改变大小，设置其中一边加 `aspectRatio`，或同时设置两边。

资源路径相对于导入它的模块解析，其 `uri` 相对于应用根目录（入口文件所在目录）。`nsh build` 会把它们复制到 `out/assets/`，`nsh package` 会把它们放进应用，开发时则由开发服务器提供。

> [!TIP]
> `src` 也接受 `asset:/path` 字符串，但只有被导入的文件才保证会被发布。从未导入的路径在开发服务器下可以使用（它提供整个应用目录），在打包后的应用中却会缺失。请导入你用到的每一张图片。

## SVG 图片

像导入其他图片一样导入 `.svg` 文件。它的尺寸（以点为单位）取自 `width` 和 `height`（只给出其中一个时，另一个按 `viewBox` 的宽高比算出），否则取自 `viewBox`：SVG 中的一个 CSS 像素就是一个点，文件名中的 `@2x` 后缀对 SVG 文件不起作用。

```tsx
import logo from "./assets/logo.svg"; // <svg width="120" height="40" …>
import star from "./assets/star.svg"; // <svg viewBox="0 0 24 24" …>

logo; // { uri: "asset:/assets/logo.svg", width: 120, height: 40 }

<Image src={logo} />
<Image src={star} style={{ width: 36, height: 36, tintColor: "#0969da" }} />
```

SVG 在显示的尺寸下清晰。浏览器以矢量方式绘制它；iOS、macOS 和 Android 在图片加载时按视图显示它的像素尺寸绘制（与位图的降采样相同；之后变大的视图会缩放这次绘制的结果，直到图片重新加载）；Windows、Linux 和 GPUI 以每点四个像素绘制一次（每边最多 2048 像素），再缩放到各个视图。其余一切与位图相同：`resizeMode`、`tintColor`（透过 SVG 的不透明形状着色，单色图标因此能随主题变色）、滤镜、`blurRadius`、占位图、加载事件、`Image.getSize` 与 `Image.prefetch`。`onLoad` 和 `Image.getSize` 以整数点报告 SVG 的尺寸；图片按它的精确尺寸布局。远程 SVG 文件以同样的方式加载，经过同样的下载与[能力](./sandbox-and-capabilities.md)检查；NSH 根据内容识别 SVG（包括 gzip 压缩的 `.svgz`），与 URL 的结尾无关。

**原生平台绘制的内容。** iOS、macOS、Android、Windows、Linux 和 GPUI 用同一个渲染器（resvg）绘制 SVG，因此同一幅图在各平台上看起来一致：形状与路径、填充与描边、渐变与图案、裁剪路径与蒙版、不透明度、滤镜和变换。文本（`<text>`）、动画（SMIL 与 CSS）、嵌入的图片（`<image>`）和 HTML（`<foreignObject>`）在浏览器中显示；原生平台绘制 SVG 的其余部分，保持静止，`nsh build` 会指出用到它们的文件。把文本导出为轮廓（路径），即可在各处得到相同的效果。

**自成一体的图片。** 在原生平台上，SVG 只绘制它自身包含的内容：它引用的文件、URL、`data:` 图片和其他文档都不会加载。原生平台在控制内存的范围内绘制 SVG（包括远程的 SVG）：文档最大 16 MiB（压缩的 SVG 按解压后计算），绘制结果每边最多 8192 像素、总共最多 4096 × 4096 像素（64 MiB；更大的会缩小绘制），图层（带不透明度、蒙版、裁剪路径和滤镜的组，渲染器会单独绘制它们）同时最多 256 MiB。超出文档或图层范围的 SVG 加载失败，并触发 `onError`。

**Web 上报告的尺寸。** 浏览器自行测量 SVG，无论是导入的还是远程的：SVG 有 `width` 和 `height` 时，`onLoad` 和 `Image.getSize` 报告这两个值，否则报告浏览器给出的尺寸，浏览器通常由默认尺寸 300 × 150 和 `viewBox` 的宽高比得出它。导入的 SVG 在所有平台上都按打包器给出的尺寸布局。为 SVG 设置 `width` 和 `height`，即可在各处得到相同的报告尺寸。

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
| `onLoad` | `{ width, height }` | 图片就绪；源图的像素尺寸（SVG 为点，[在 Web 上为浏览器测得的尺寸](#svg-图片)） |
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
| `Image.getSize(source)` | 图片的像素尺寸（SVG 为点，[在 Web 上为浏览器测得的尺寸](#svg-图片)）。无法加载时 reject |
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

### 模块自身的 URL

`import.meta.url` 是书写它的模块的 URL，因此 `new URL("./data.json", import.meta.url)` 指向它旁边的文件。本地运行或打包的桌面应用中，模块的 URL 是应用目录下的 `file:` URL，可在 `${pluginDir}` 授权下用 `fs/promises` 读取；Web 应用中是页面的 URL；打包的移动应用中是 `asset:/` URL，与导入的图片相同。要随应用发布图片，请导入它：这样构造的 URL 只指向文件，并不会打包它。

位图也可以用 `drawImage` 绘制在画布上（[Canvas](./canvas.md)），而 `backgroundImage` 的渐变能替代许多装饰性图片（[样式](./styling.md)）。

## 相关内容

- [image 元素](../elements/image.md)：所有属性。
- [`nsh/image`](../modules/nsh-image.md)：`Image`，以及 `getSize` 和 `prefetch`。
- [列表](./lists.md)：长图片列表会复用视图。

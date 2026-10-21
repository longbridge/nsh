---
title: 图片与资源
description: 用 Rust 和 TSX 显示图片：应用的资源与 @2x 文件、远程 URL、缩放模式、占位图、模糊、着色、加载事件，以及读取图片尺寸和预取图片。
order: 36
group: ui
---

# 图片与资源

图片元素（Rust 中的 `image(..)`，TSX 中的 `<Image>`）显示一张位图：随应用一起发布的文件，或从 URL 下载的图片。图片在主线程之外读取和解码，按视图实际显示的尺寸降采样后缓存；主线程只负责显示解码结果。

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

元素的属性列在 [image](../elements/image.md) 中。

## 应用内的图片

Rust crate 把文件放在它的 `assets/` 目录中：`asset!("logo@2x.png")`（路径相对于 `assets/`）是一个 `ImageAsset`，即 `{ uri, size, scales }`，在 crate 编译时读取；在 crate 的 `#[native_module]` 函数中调用 `registry.assets(assets!())` 会把该目录中的每个文件随应用发布。在 TSX 中，像导入模块一样导入图片文件：导入得到一个 `ImageAsset`，即 `{ uri, width, height, scales? }`，打包器会把文件随应用一起发布：

```rust tabs
const LOGO: ImageAsset = asset!("logo@2x.png");

// LOGO.uri："asset:/@my-app/logo@2x.png"，LOGO.size：64 × 64 点

#[native_module("app/native")]
fn native(registry: &mut Registry) {
    registry.assets(assets!());
}
```

```tsx tabs
import logo from "./assets/logo@2x.png";

logo; // { uri: "asset:/assets/logo@2x.png", width: 64, height: 64 }
```

支持的文件：`.png`、`.jpg`、`.jpeg`、`.gif`、`.webp`、`.heic`。

**尺寸以点为单位。** 文件名中的 `@2x` 或 `@3x` 后缀表示文件每个点有 2 或 3 个像素，因此 128×128 像素的 `logo@2x.png` 是一个 64×64 点的资源。没有后缀时，一个像素就是一个点。带后缀的文件显示的就是该文件本身；不带后缀的文件如果有[像素密度变体](#像素密度)，会显示适合屏幕的那一个。

**自然尺寸。** 当样式没有指定宽度或高度时，来自资源的图片按自然尺寸布局，所以 `image(LOGO)` 和 `<Image src={logo} />` 不需要任何样式。若要改变大小，设置其中一边加宽高比，或同时设置两边。

在 Rust 中，缺失或损坏的文件会让构建失败，文件改动后 crate 会重新构建。在 TSX 中，资源路径相对于导入它的模块解析，其 `uri` 相对于应用根目录（入口文件所在目录）。`nsh build` 会把它们复制到 `out/assets/`，`nsh package` 会把它们放进应用，开发时则由开发服务器提供。

> [!TIP]
> 在 TSX 中，`src` 也接受 `asset:/path` 字符串，被导入的文件才保证会被发布。从未导入的路径在开发服务器下可以使用（它提供整个应用目录），在打包后的应用中却会缺失。请导入你用到的每一张图片。Rust crate 通过 `assets!()` 发布整个 `assets/` 目录。

### 像素密度

指定不带后缀的文件，并把更高密度的版本放在它旁边，应用就会加载适合屏幕的那一个：

```text
assets/
├── logo.png       64×64 像素
├── logo@2x.png    128×128
└── logo@3x.png    192×192
```

```rust tabs
const LOGO: ImageAsset = asset!("logo.png");

// LOGO.size：64 × 64 点，LOGO.scales：[1.0, 2.0, 3.0]
```

```tsx tabs
import logo from "./assets/logo.png";

logo; // { uri: "asset:/assets/logo.png", width: 64, height: 64, scales: [1, 2, 3] }
```

- 对不带后缀的图片，`asset!` 和打包器会查找它的 `@1.5x`、`@2x`、`@3x` 和 `@4x` 同名文件。`scales` 列出它们，其中 1 表示指定的文件本身；没有同名文件的图片没有 scales（在 TSX 中导入为 `{ uri, width, height }`）。打包器发布找到的同名文件，`assets!()` 发布整个目录。
- 图片会加载不小于屏幕像素比的最小倍数，否则加载最大的倍数，与 React Native 的选择规则相同：1× 屏幕上加载 `logo.png`，2× 屏幕上加载 `logo@2x.png`，3× 手机上（以及 2.625 时）加载 `logo@3x.png`。在 Web 上只会下载这一个文件。
- 像素比变化时（窗口移到另一块屏幕、浏览器缩放），选择发生变化的图片会重新加载，并触发各自的[加载事件](#加载事件)。它们的尺寸保持为指定文件的尺寸，以点为单位。
- 当某个变体的像素尺寸不等于指定文件尺寸乘以其倍数时，TSX 构建会给出警告，Rust 构建会失败。
- 指定带密度后缀的文件（`./logo@3x.png`、`asset!("logo@3x.png")`）时，就只是该文件本身。

只用一个文件也可以：按需要的最高密度导出（`@3x` 能在所有手机上得到最清晰的效果），并带后缀指定它；解码时图片会降采样到屏幕所需的大小。变体让每块屏幕只下载、解码和保存适合它的那个文件。

## 远程图片

给元素一个 URL：在 Rust 中是字符串（会变化的 URL 用闭包），在 TSX 中是 URL 字符串或 `{ uri }`：

```rust tabs
image(move || user.with(|u| u.avatar_url.clone()))
    .size(px(48.0))
    .rounded(px(24.0))
    .bg(rgb(0xeaeef2))
```

```tsx tabs
<Image src={user().avatarUrl} style={{ width: 48, height: 48, borderRadius: 24, backgroundColor: "#eaeef2" }} />
```

- **为远程图片指定尺寸。** 加载完成前无法得知它们的尺寸，所以如果没有宽和高（或其中一边加宽高比），它们的高度会塌缩为 0。
- 加载期间会显示**背景色**，也可以使用占位图（见下文）。
- 下载是共享的：多个视图显示同一个 URL 时只下载一次，即使它们按不同尺寸解码也是如此。没有任何视图再等待的下载会被取消。
- 解码后的图片缓存在内存中（内存紧张时会清理），以 URL、解码尺寸和模糊半径为键。
- 当应用声明了[能力](./sandbox-and-capabilities.md)时，图片的主机必须在 `network` 中被允许；被拒绝的图片会触发错误事件。

## 缩放模式

缩放模式（Rust 中的 `.resize_mode(..)`，TSX 中的 `resizeMode`）决定图片如何填充形状不同的框：

| 取值 | 效果 |
|---|---|
| `"cover"`（`ResizeMode::Cover`，默认） | 保持宽高比填满整个框，裁掉溢出部分 |
| `"contain"`（`ResizeMode::Contain`） | 保持宽高比完整放入框内，留出空白 |
| `"stretch"`（`ResizeMode::Stretch`） | 精确填满框，图片会变形 |
| `"center"`（`ResizeMode::Center`） | 自然尺寸，居中 |

```rust tabs
image(photo).resize_mode(ResizeMode::Contain).size(px(140.0)).bg(rgb(0xeaeef2))
```

```tsx tabs
<Image src={photo} resizeMode="contain" style={{ width: 140, height: 140, backgroundColor: "#eaeef2" }} />
```

Tailwind 类 `object-cover`、`object-contain` 等效果相同。

## 图片样式

图片也是视图：它们支持圆角（也可按角设置）、边框、阴影、不透明度和变换。圆角会裁剪图片。

```rust tabs
image(avatar).size(px(90.0)).rounded(px(45.0)).border(px(3.0)).border_color(rgb(0x8250df))
```

```tsx tabs
<Image src={avatar} style={{ width: 90, height: 90, borderRadius: 45, borderWidth: 3, borderColor: "#8250df" }} />
```

- **着色**（Rust 中的 `.tint_color(..)`，TSX 中的 `tintColor` 样式）会给图片的不透明像素重新着色：适合随主题变色的单色图标。
- **滤镜**（`.grayscale(..)`、`.sepia(..)`、`.blur(..)`；TSX 中的 `filter`）在两个平台上都可用于图片（[滤镜](./styling.md#滤镜与混合)）。

```rust tabs
image(ICON).size(px(24.0)).tint_color(AdaptiveColor::rgb(0x0969da, 0x58a6ff))
```

```tsx tabs
<Image src={icon} style={{ width: 24, height: 24, tintColor: active() ? "#0969da" : "#6e7781" }} />
```

## 占位图、淡入与模糊

| 属性 | Rust | 说明 |
|---|---|---|
| `defaultSource` | `.placeholder(..)` | `src` 加载完成前显示的图片；最好是本地资源 |
| `fadeDuration` | `.fade_duration(ms)` | 加载完成的图片淡入所用的毫秒数。iOS 默认 0，Android 默认 300 |
| `blurRadius` | `.blur_radius(px(..))` | 按此半径（点）模糊图片，在解码时一次性应用 |

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

## 动图

动态 GIF、WebP 和 APNG 文件会自动播放，无论是资源还是来自 URL。它们支持相同的样式和缩放模式；在 iOS 上，着色后的动图只显示第一帧。

## 加载事件

| 事件 | Rust | 负载 | 触发时机 |
|---|---|---|---|
| `onLoadStart` | `.on_load_start(..)` | | 开始加载 |
| `onProgress` | `.on_progress(..)` | `{ loaded, total }`，单位为字节（未知时 `total` 为 0） | 下载过程中，最多每 100 ms 一次 |
| `onLoad` | `.on_load(..)` | `{ width, height }` | 图片就绪；源图的像素尺寸 |
| `onError` | `.on_error(..)` | `{ message }`（Rust 中为 `&str`） | 加载或解码失败 |
| `onLoadEnd` | `.on_load_end(..)` | | 在 `onLoad` 或 `onError` 之后 |

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

加载事件报告的是源图的尺寸，而不是解码时的尺寸。

带有[像素密度变体](#像素密度)的图片会加载其中一个文件，加载事件报告的是该文件的像素尺寸：在 3× 屏幕上，`logo@3x.png` 报告 192×192，而 `Image.getSize(logo)` resolve 为导入文件的 64×64。像素比变化、图片加载另一个变体时，这次加载同样会触发事件：先是 `onLoadStart`，然后是带有新文件尺寸的 `onLoad`（或 `onError`），最后是 `onLoadEnd`。

## 图片尺寸与预取

不在屏幕上的图片也可以测量尺寸和提前加载：Rust 中是 `image_size(..)` 和 `prefetch_image(..)`，TSX 中是 `Image.getSize` 和 `Image.prefetch`（就在渲染元素的那个 `Image` 上）：

```rust tabs
spawn(async move {
    // Ok(ImageSize { width, height })，单位为像素。
    let size = image_size("https://example.com/photo.jpg").await;
    // 字节进入内存后为 true。
    let ok = prefetch_image("https://example.com/next.jpg").await;
});
```

```tsx tabs
import { Image } from "nsh";

const size = await Image.getSize("https://example.com/photo.jpg");   // { width, height } in pixels
const ok = await Image.prefetch("https://example.com/next.jpg");      // true once the bytes are in memory
```

| Rust | TSX | 返回值 |
|---|---|---|
| `image_size(uri)` | `Image.getSize(source)` | 图片的像素尺寸；无法加载时返回错误 |
| `prefetch_image(uri)` | `Image.prefetch(source)` | 图片字节加载完成后为 `true` |

在 TSX 中，`source` 是 URL 字符串、`{ uri }` 或导入的图片。对于带有[变体](#像素密度)的导入图片，两者都会加载视图在当前屏幕上加载的那个变体，`getSize` 返回导入文件的尺寸。两者都会为接下来显示该图片的视图保留已下载的字节，因此先预取再显示的图片无需再次下载。

一个常见用法是按宽高比确定远程图片的尺寸：

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

## 其他资源

| 文件 | Rust（`asset!`） | TSX 导入 | 页面 |
|---|---|---|---|
| `.ttf`、`.otf` | 带字体族名的 `FontAsset` | 字体的字体族名 | [字体](./fonts.md) |
| `.json` 及其他文件 | 带 URI 的 `FileAsset` | `.json`：解析后的值，打包进脚本 | [项目结构](./project-structure.md#模块) |

两种方式下，字体都在第一帧之前注册。

### 模块自身的 URL

在 TSX 中，`import.meta.url` 是书写它的模块的 URL，因此 `new URL("./data.json", import.meta.url)` 指向它旁边的文件。本地运行或打包的桌面应用中，模块的 URL 是应用目录下的 `file:` URL，可在 `${pluginDir}` 授权下用 `fs/promises` 读取；Web 应用中是页面的 URL；打包的移动应用中是 `asset:/` URL，与导入的图片相同。要随应用发布图片，请导入它：这样构造的 URL 只指向文件，并不会打包它。

图片也可以用 `draw_image` / `drawImage` 绘制在画布上（[Canvas](./canvas.md)），而渐变能替代许多装饰性图片（[样式](./styling.md#渐变)）。

## 相关内容

- [image 元素](../elements/image.md)：所有属性，以及 `getSize` 和 `prefetch`。
- [列表](./lists.md)：长图片列表会复用视图。

---
title: Web（WebAssembly）
description: 用 WebAssembly 版运行时在浏览器里运行 NSH 应用，渲染到 DOM；用 NSH package --target web 发布为静态站点，用 NSH run --target web 开发。
order: 16
group: platforms
demo: counter
---

# Web（WebAssembly）

NSH 运行时也能编译成 WebAssembly。同一个引擎跑在浏览器页面里，只是渲染目标从 UIKit 或 Android 视图换成了 DOM：核心、响应式图、渲染器、Taffy 布局、`nsh/ui/mobile` 组件，以及 QuickJS 本身，全都一样。`nsh package --target web` 把应用发布成可放在任何 Web 服务器或 CDN 上的静态站点（[发布应用](#发布应用)），`nsh run --target web` 在你编辑时把它运行在浏览器里（[在浏览器里开发](#在浏览器里开发)），本站每一个实时演示，都是 `nsh build` 打出的应用包运行在这个运行时上。

## 各部分在哪里运行

在每个原生平台上，引擎都有自己的线程，每次提交把一个 `MountBatch` 交给主线程。浏览器页面只有一个线程，所以 Web 版由页面的事件循环一步步驱动同一个引擎（`nsh_engine::LocalEngine`）：

```text
指针 / 键盘 / 滚动事件 ──────────┐
setTimeout（引擎给出的截止时间）─┼─▶ nsh.wasm：引擎的一轮（JS、核心、布局）─▶ 一个 JSON 批次 ─▶ nsh.js：DOM
requestAnimationFrame ───────────┘                                                   ▲
                                                           原生动画的输出 ────────────┘
```

- **布局由核心完成。** 每个视图都是一个绝对定位的元素，位置和大小就是 Taffy 算出的 frame，所以 flexbox、grid、`aspectRatio` 和百分比的表现与设备上完全一致，浏览器自身的布局不参与。
- **样式在 Rust 里转成 CSS。** 背景、边框、圆角、阴影（`shadow*` 和 `boxShadow`）、渐变、滤镜、`backdropFilter`、变换、`transformOrigin`、`outline`、`mixBlendMode`、过渡和内置的关键帧动画都会变成 CSS 属性。Tailwind 类、`active:`、`hover:`、`focus:` 状态和 `dark:` 与设备上一样在原生侧解析。
- **文本在 Rust 里排版**，字体度量来自浏览器（`CanvasRenderingContext2D.measureText`），再按排版确定的位置逐段绘制，所以测量的就是绘制的。换行发生在空格之后、CJK 字符前后和 `\n` 处；`numberOfLines` 会截断并加省略号。
- **原生动画在 Rust 里运行**，由 `requestAnimationFrame` 驱动：`Animated` 的 timing、spring、decay，`interpolate`，绑定到 `opacity`、`transform`、`backgroundColor` 的原生值，用 `pan` 绑定的拖拽、滚动联动的值和画布参数。和设备上一样，每一帧都不经过 JS。
- **输入**：带 `activeOpacity` 的按压和长按、响应者系统（`onTouch*`、`PanResponder`）、原生拖拽和双指捏合、旋转、滚动视图（浏览器自身的滚动）、下拉刷新、可缩放的滚动视图（双指捏合、Ctrl+滚轮和触控板捏合、`zoomTo`）、受控的 `<Input>`（焦点、提交）、`Canvas`、带标题栏的 `Stack` 和 `Screen`、`Modal`。
- **系统界面绘制在页面内**：`Alert` 和 `ActionSheet` 在桌面浏览器上与桌面后端的对话框外观一致，在手机上与 iOS 一致（即 `desktop` 界面类型，见下文），`Toast.show` 在底部显示提示。自定义字体在浏览器加载完成后会重新测量文本，应用导入的字体加载好之前不显示第一帧。
- **模态视图**按浏览器所像的平台呈现：`pageSheet` 和 `formSheet`（`desktop={{ presentationStyle }}` 或 `web={{ presentationStyle }}`）在桌面和平板浏览器中是居中于变暗页面上的卡片，尺寸与桌面后端相同，可用 Escape、浏览器的返回按钮或点击卡片外部关闭（都会通过 `onRequestClose` 询问）；在手机浏览器中则像 iPhone 的表单一样从底部升起，停在顶部稍下方。`fullScreen` 和透明的模态视图覆盖整个应用；`animationType`、`onShow` 和 `onDismiss` 与设备上一致。

## 键盘、鼠标和触摸

发布出去的应用，在桌面浏览器里表现得像桌面应用，在手机浏览器里表现得像手机应用：

- **滚轮和触控板**滚动沿其方向、还能继续滚动的最内层滚动视图（没有可滚动的纵向视图时，纵向滚轮也会滚动横向列表），并且直到这次滚动结束都停留在这个视图上；滚轮的每一格平滑滑动，分页和吸附的视图每次滚动移动一页。Ctrl + 滚轮和触控板捏合会捏合带原生捏合值的视图，而不是缩放页面。和桌面平台一样，用鼠标拖动不会滚动：鼠标拖动交给拖拽手势、响应者系统和文字选择。滚动视图像桌面平台一样上报 `onScrollBeginDrag` / `onScrollEndDrag`、`onMomentumScrollBegin` / `End` 和 `onScrollEnd`。
- **键盘**：Tab 和 Shift+Tab 在可按压的视图和输入框之间移动（焦点环和 `focus:` 样式标出键盘焦点）；回车按下获得焦点的视图，空格在松开时按下；方向键、Page Up / Page Down、空格、Home 和 End 滚动焦点所在或鼠标下方的滚动视图；Escape 先离开输入框、关闭对话框，再返回。`nsh/keymap` 的快捷键在浏览器里同样有效，当场匹配（Mac 上的 Safari 和 Chrome 里 `mod` 是 Command，其他地方是 Control）。
- **返回**：Escape、Alt+左方向键（Mac 上是 Cmd+[）和鼠标的后退键会弹出最内层的栈，或请求关闭最上层的模态框，和 Android 返回键一样；`preventBack` 和 `onBackPress` 也照常生效。应用占据整个页面时，每个压入的页面和弹出的模态框都是一条浏览器历史记录，浏览器的后退按钮会在应用内返回。
- **触摸**：只有滚动视图可以平移；应用下面的页面本身不会滚动、缩放或下拉刷新，点按没有延迟。嵌在 iframe 里时，应用不处理的滑动会滚动外层页面。在带原生平移、捏合的视图和响应者系统上的拖动都交给应用处理。
- **无障碍**：`accessibilityRole`、`accessibilityLabel`、`accessibilityHint`、`accessibilityState`、`accessibilityValue`、`accessibilityLiveRegion` 和 `accessibilityElementsHidden` 会转成 ARIA；`AccessibilityInfo.announceForAccessibility` 通过实时区域朗读，`setAccessibilityFocus` 移动焦点。模态框或提示框下面的视图不可交互。
- **选择和菜单**：在桌面浏览器中，文字默认可以选择，和网页一样，但交互元素里的文字（带按压或触摸处理的视图：按钮、可按压的列表行、标签栏）以及设置了 `selectable={false}` 或 `select-none` 的文字除外；在上面拖动鼠标会选择文字，Ctrl/Cmd+C 按行复制，Ctrl/Cmd+A 全选选区所在的这段文字。在手机和平板浏览器中，只有 `selectable` 文字可以选择，和原生应用一样。浏览器的右键菜单只在可选文字和输入框里出现。


## 发布应用

```sh
nsh package App.tsx --target web           # out/<Name>/：一个静态站点
nsh package App.tsx --target web --run     # 并在本机提供服务，在浏览器中打开
```

`nsh package --target web` 写出 `out/<Name>/`（用 `--out` 指定其他目录），结构与前端构建产物相同：一个页面，其余文件都以内容命名。

| 文件 | 内容 |
|---|---|
| `index.html` | 应用的页面：以应用名作为标题，带图标、按 `appearance` 设定的颜色、加载指示，以及应用无法加载或启动时的提示 |
| `manifest.webmanifest` | 浏览器安装应用时使用的名称、id、图标和颜色 |
| `nsh.<digest>.js` | DOM 运行时，已压缩（79 KB，brotli 后 22.5 KB） |
| `nsh.<digest>.wasm` | 引擎、核心、移动端组件和不含编译器的 QuickJS（约 3.5 MB，gzip 后 1.2 MB，brotli 后 0.93 MB） |
| `app.<digest>.nshc` | 应用本身，构建方式与 `nsh build --release --split` 相同，并编译成 QuickJS 字节码：浏览器里不再编译，启动更快 |
| `harden.<digest>.nshc` | [沙箱](./sandbox-and-capabilities.md)的加固脚本，同样编译成字节码（1.4 KB） |
| `chunks/<name>.<digest>.nshc` | `import()` 加载的代码，同样是字节码，第一次导入时才下载（[拆分 bundle](#拆分-bundle)） |
| `assets/<path>.<digest>.<ext>` | 应用导入的每个资源，用到时才从其 URL 加载（字体在注册它们的代码运行前加载，因为文本要用它们测量：在应用启动前，或随导入它们的分块一起加载） |
| `icons/<size>.<digest>.png` | `nsh.json` 中的图标，尺寸为 32（favicon）、180（`apple-touch-icon`）、192 和 512 像素 |

`<digest>` 是文件内容 SHA-256 的前几位：文件名只在内容变化时才变化，所以新版本只会改动 `index.html` 和真正变化了的文件。所有 URL 都是相对的，站点放在任何主机的任何目录下都能运行（`https://example.com/apps/counter/`）。

### 部署

把目录上传到任意静态托管：CDN 后面的对象存储、Cloudflare Pages、Netlify、GitHub Pages、nginx。遵循两条规则，加载就快，更新也可靠：

| 文件 | `Cache-Control` |
|---|---|
| `index.html`、`manifest.webmanifest` | `no-cache`（每次加载都重新验证，新版本立即生效） |
| 其余所有文件 | `public, max-age=31536000, immutable` |

以 `application/wasm` 类型提供 `.wasm`（浏览器会边下载边编译），并让托管服务或 CDN 压缩文本和 WebAssembly（gzip 或 brotli）。`nsh package --target web --run` 就按这种方式在 `http://127.0.0.1:4173/` 上提供站点。

发布到 Web 的应用与打包的原生应用运行方式相同：以发布模式运行，经过[加固](./sandbox-and-capabilities.md)（没有 `eval`），具有其 `nsh.json` 声明的能力。当能力限制了 `network` 时，页面还会带上 Content-Security-Policy `connect-src`，其中列出声明的主机（以及页面自身的源），由浏览器自己拒绝发往其他主机的请求和重定向。导致应用无法启动的错误会显示在页面上；之后的错误输出到浏览器控制台。

### 用 iframe 嵌入

Web 上没有嵌入 API：用 `<iframe>` 嵌入已发布的应用，它的尺寸就是应用的窗口：

```html
<iframe src="https://apps.example.com/counter/" title="Counter"
        style="width: 390px; height: 844px; border: 0"></iframe>
```

## 在浏览器里开发

```sh
nsh run App.tsx --target web
```

会启动开发服务器，并在浏览器中打开它的页面 `http://127.0.0.1:8790/`。这个页面和其他开发宿主一样：运行应用的每次构建，就地应用修改（热更新），在应用上方显示构建错误和运行时错误，并把应用的 `console` 输出发送到终端。同一网络中的手机打开 `nsh run` 打印的地址（`browsers on this network: http://192.168.…:8790/`）即可。只要 Web 运行时存在（发行包中总是存在），`nsh dev` 也会提供同一个页面。

## 构建本站的播放器

```sh
cargo xtask web                       # nsh.wasm、nsh.js、播放器页面，以及 website/demos/*.tsx
cargo xtask web --out target/play     # 输出到其他目录
```

输出目录包含：

| 文件 | 内容 |
|---|---|
| `gallery/nsh.<digest>.wasm` | 已发布应用使用的运行时（约 3.5 MB，gzip 后 1.2 MB，brotli 后 0.93 MB；用 `web` profile 构建），播放器和示例应用共用 |
| `nsh-playground.wasm` | 同上，另含 NSH 的 TSX 编译器，供[在线试用](#在线试用)使用（约 8 MB，gzip 后 2.4 MB） |
| `gallery/nsh.<digest>.js` | DOM 运行时（已压缩）：加载 wasm，应用批次，把输入转成事件 |
| `index.html` | 播放器页面：根据 `#<name>` 运行 `demos/<name>.nshc` |
| `demos/<name>.nshc` | 用 `nsh build --release` 编译的各个演示 |
| `gallery/` | 用 `nsh package --target web` 发布的示例应用 Gallery，显示在[示例应用](/zh-CN/gallery)页面 |

用任意静态服务器托管该目录即可（流式编译需要 `application/wasm` 这个 MIME 类型），然后打开 `index.html#counter`。

## 在线试用

[在线试用](/zh-CN/playground)页面可以在浏览器里编辑并运行应用：页面用 NSH 自己的 JSX 编译器（编进 `nsh-playground.wasm`，不需要 rolldown）编译 TSX，每次停止输入后立即运行。本站每个演示都有「在线编辑」链接；「分享」会把代码直接放进链接里，不经过任何服务器存储。

在线试用的应用只有一个模块：可以导入 NSH 的模块（`nsh`、`nsh/*`、`nsh/ui/mobile`），但不能导入其他文件或包。`console.log` 的输出和错误显示在手机下方。

编译器也是一个 Rust 函数：`nsh_bundler::compile_module(name, source, ui_sets)`，不开启该 crate 的 `bundle` 特性也能使用。

## 嵌入到页面

`nsh.js` 是一个 ES 模块。`createPlayer` 把运行时加载进一个容器，`run` 启动 `nsh build` 写出的应用包：字节码 `bundle.nshc`（以字节传入），或 `bundle.js`（以字符串传入）。`nsh package --target web` 附带的运行时（`nsh.wasm`）不含 JavaScript 编译器，只运行字节码；开发用的运行时（发行包中的 `nsh-dev.wasm`）两者都能运行：

```js
import { createPlayer } from "./nsh.js";

const player = await createPlayer(document.querySelector("#phone"), {
  wasm: "./nsh.wasm",
  assetBase: "./assets/",   // `asset:/…` URI（导入的图片）从这里加载
  // dark: true,            // 强制外观；不设置时跟随系统
  // idiom: "phone",        // 强制 "phone"、"pad" 或 "desktop"；不设置时由指针和尺寸决定
  // history: false,        // 页面和模态框是否作为浏览器历史记录；默认只在应用占据整个页面时开启，iframe 里不开启
});
player.run(await (await fetch("./bundle.nshc")).arrayBuffer());
```

`assets`（从 `asset:/` 路径到 URL 的映射）可代替 `assetBase`，`readAsset(uri)` 返回引擎需要立即读取的资源（字体）的字节，或 `null`；`player.run(code, { release: true, capabilities })` 以发布模式、在应用声明的能力下运行应用包（默认是开发模式）；运行字节码时，发布模式还要传入加固脚本的字节码（`harden`，即 `harden.<digest>.nshc` 的字节）。`nsh package --target web` 写出的页面都用到了。

`run(code, { chunks, chunkBase })` 还可以指定[拆分 bundle](#拆分-bundle) 的 chunk 所在位置。

使用 `nsh-playground.wasm` 时，`player.runSource(tsx)` 会先编译 TSX，返回编译错误或 `null`；`player.compile(tsx)` 返回 `{ code }` 或 `{ error }`。

容器的尺寸就是应用的窗口（`windowSize()`），调整容器大小会同步调整应用。`player.setDark(true)` 可在运行时切换外观，应用通过 `colorScheme()` 和 `dark:` 类感知到变化。

> [!NOTE]
## 浏览器作为平台

`Platform.OS` 为 `"web"`。浏览器所在设备的类型决定应用的行为，即 `Platform.constants.interfaceIdiom`：

- `"desktop"`：有鼠标（能悬停的精确指针），且容器宽度至少 600 点。此时 `Platform.isDesktop` 为 `true`，`Platform.select` 选 `desktop`，`desktop={{…}}` 属性生效，标题栏高度与桌面后端相同。
- `"pad"`：短边至少 600 点的触摸屏；其余为 `"phone"`。两者都属于移动端：`Platform.select` 选 `mobile`。

界面风格在应用启动时确定，运行期间不变，与其他平台一致；变窄的桌面窗口仍是桌面，按自己的 `windowSize()` 布局。`web={{…}}` 属性、`Platform.select({ web })` 和 `web:` 类只在浏览器中生效；`ios={{…}}`、`native` 和 `ios:` 在浏览器中不生效。

环境的其余部分由页面报告，并随时更新：

| 值 | 来自浏览器的 |
|---|---|
| 区域设置（`Intl` 的默认值） | `navigator.languages[0]` |
| `I18nManager.isRTL` | 设置了 `<html dir>` 时取它，否则取区域设置的书写方向 |
| `reduceMotion()` | `prefers-reduced-motion` |
| `fontScale()` | 根元素字号 / 16 px（浏览器的文字大小设置） |
| `PixelRatio.get()` | `devicePixelRatio`（随缩放和屏幕变化） |
| `colorScheme()` | `prefers-color-scheme` |
| `safeArea()` | 与容器重叠部分的 `env(safe-area-inset-*)`（手机上 `viewport-fit=cover` 的页面） |
| `keyboard()` | 屏幕键盘遮住容器的高度（`visualViewport`） |

`Platform.Version` 为 `"0.0"`，`screenReaderEnabled()` 始终为 `false`（浏览器不提供这一信息）。[`StatusBar`](../components/status-bar.md) 的 `web={{ themeColor }}` 设置页面的 `<meta name="theme-color">`，也就是浏览器给页面周围的栏使用的颜色。

## 网络

`fetch`（包括流、`AbortController` 和 `FormData`）以及 `<Image>` 的 URL 都走浏览器自己的网络栈，在此之前 nsh 已按应用的[能力声明](./sandbox-and-capabilities.md)做过检查。请求是浏览器发出的，所以遵循浏览器的规则：

- **相对 URL** 与浏览器里一样，相对于页面的 URL 解析：发布出去的应用用 `fetch("/api/items")` 就能访问自己的接口，`<Image src="/photos/a.jpg" />` 从自己的服务器加载。（原生平台没有页面，仍需使用绝对 URL。）
- **CORS。** 发往页面所在源以外的请求，只有对方服务器允许页面的源（`Access-Control-Allow-Origin`）才会成功，否则 `fetch` 以网络错误拒绝。只能读到服务器公开的响应头。
- **Cookie**（以及 HTTP 认证信息）只发送给页面自己的源（`credentials: "same-origin"`），浏览器禁止的请求头（`Cookie`、`User-Agent`、`Host` 等）会被丢弃。
- **重定向**由浏览器跟随。不应跟随重定向的请求遇到重定向时会失败：`redirect: "manual"` 和 `"error"`，以及能力声明限制了 `network` 的应用发出的请求（nsh 要逐跳检查，而浏览器在跟随之前不会透露重定向的目标）。
- 作为 `ReadableStream` 的**请求体**会先收齐再一次发出；响应体则随到随读，以流的形式交给应用。

## 拆分 bundle

Web 应用不必在第一个界面出现之前下载全部代码。`nsh package --target web` 以拆分方式发布应用：每个 chunk 写成 `chunks/<name>.<digest>.nshc`，由页面把 chunk 映射到这些文件（以字节码运行的应用包，其 chunk 也以字节码加载：`Screen.js` 旁边的 `Screen.nshc`）。要自己拆分 bundle，可以用 `nsh build --split`，它把 `import()` 加载的模块放进各自的 chunk，放在 `bundle.js` 旁边：

```sh
nsh build --release --split -o site/app
# bundle.js        应用到第一个界面为止的代码
# Screen.js        import("./Screen") 以及只有它用到的代码
# detail.js        两个 chunk 共用的代码（如果有，由打包器生成）
```

第一次 `import("./Screen")` 会一次性下载 `Screen.js` 和它导入的 chunk，然后执行它们；之后再导入会立即兑现。页面负责告诉运行时 chunk 在哪里：默认在页面旁边（`chunkBase`），也可以为每个 chunk 指定一个 URL，例如带内容摘要的文件名，便于 CDN 永久缓存：

```js
player.run(code, {
  chunks: { "Screen.js": "/app.3f2a9c.js", "detail.js": "/app.81bb07.js" },
});
```

无法下载的 chunk 会让 `import()` 以一个指明该 chunk 的 `TypeError` 拒绝；下一次 `import()` 会重新尝试。chunk 名称是 bundle 中固定的文件名：应用无法让页面下载其他任何东西。

[示例应用](/zh-CN/gallery)就是这样加载每个演示的：`lazy`（位于 `examples/gallery/ui.tsx`）生成一个组件，它在第一次渲染时开始导入，导入完成前显示占位内容，之后渲染导入的组件。首次加载只有索引和最先显示的演示；演示导入的字体随该演示的 chunk 一起加载。原生构建包含所有模块，因此导入会立即完成：

```tsx
const Stocks = lazy(() => import("./demos/Stocks").then((m) => m.Stocks));
```

## 限制

Web 运行时用于演示和文档，下列能力不提供：

- **其他网站的 Web 视图**：`WebView` 是一个 `<iframe>`。内联 HTML（`source={{ html }}`）功能完整：`onMessage`、`postMessage`、`injectJavaScript`、注入脚本和加载事件都可用（它在独立的源中运行，与所在页面隔离）。其他网站的页面只有 `postMessage` 和加载事件，事件中的 URL 是传入的那个：浏览器不向外层页面透露它的导航、标题、历史和错误，`userAgent` 也无法设置。
- **自动播放的有声视频**：`Video` 是一个 `<video>`，支持全部属性和事件，但浏览器只在用户操作之后才允许有声视频开始播放（静音视频可以自动播放）。
- **原生组件**：注册的原生组件属于嵌入 nsh 的原生应用；在 Web 上它的视图保持为空，并在控制台给出警告。
- **平台服务**：读取剪贴板（`Clipboard.getString`，`hasString` 也一样）需要安全上下文（HTTPS），并可能询问用户。图片不报告 `onProgress`。
- **文本整形细节**：换行规则只是 UAX #14 的一个子集（空格、CJK、连字符），没有自动断词，除段落方向外也不做双向重排。

播放器和开发服务器以开发模式运行应用（不做加固），发布的应用经过加固。两者都使用默认的[资源限制](./sandbox-and-capabilities.md)。

## 另请参阅

- [打包](./packaging.md)：`nsh package` 的其他目标。
- [架构](./architecture.md)：Web 版复用的引擎、核心和挂载批次。
- [测试与 trace](./testing-and-trace.md)：`nsh trace` 以近似的文本引擎无界面地运行同一个引擎。

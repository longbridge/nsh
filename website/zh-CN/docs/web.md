---
title: Web（WebAssembly）
description: 用 WebAssembly 版运行时在浏览器里运行 NSH 应用，渲染到 DOM；本站的实时演示都由它驱动。
order: 16
group: platforms
demo: counter
---

# Web（WebAssembly）

NSH 运行时也能编译成 WebAssembly。同一个引擎跑在浏览器页面里，只是渲染目标从 UIKit 或 Android 视图换成了 DOM：核心、响应式图、渲染器、Taffy 布局、`nsh/ui/mobile` 组件，以及 QuickJS 本身，全都一样。本站每一个实时演示，都是 `nsh build` 打出的应用包运行在这个运行时上。

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
- **输入**：带 `activeOpacity` 的按压和长按、响应者系统（`onTouch*`、`PanResponder`）、原生拖拽和双指捏合、旋转、滚动视图（浏览器自身的滚动，并支持鼠标拖动）、受控的 `<Input>`（焦点、提交）、`Canvas`、带标题栏的 `Stack` 和 `Screen`、`Modal`、`Alert`。

## 构建

```sh
cargo xtask web                       # nsh.wasm、nsh.js、播放器页面，以及 website/demos/*.tsx
cargo xtask web --out target/play     # 输出到其他目录
```

输出目录包含：

| 文件 | 内容 |
|---|---|
| `nsh.wasm` | 引擎、核心、移动端组件和 QuickJS（约 4.6 MB，gzip 后 1.5 MB） |
| `nsh-playground.wasm` | 同上，另含 NSH 的 TSX 编译器，供[在线试用](#在线试用)使用（约 8 MB，gzip 后 2.4 MB） |
| `nsh.js` | DOM 运行时：加载 `nsh.wasm`，应用批次，把输入转成事件 |
| `index.html` | 播放器页面：根据 `#<name>` 运行 `demos/<name>.js` |
| `demos/<name>.js` | 用 `nsh build --release` 打包的各个演示 |

用任意静态服务器托管该目录即可（流式编译需要 `application/wasm` 这个 MIME 类型），然后打开 `index.html#counter`。

## 在线试用

[在线试用](/zh-CN/playground)页面可以在浏览器里编辑并运行应用：页面用 NSH 自己的 JSX 编译器（编进 `nsh-playground.wasm`，不需要 rolldown）编译 TSX，每次停止输入后立即运行。本站每个演示都有「在线编辑」链接；「分享」会把代码直接放进链接里，不经过任何服务器存储。

在线试用的应用只有一个模块：可以导入 NSH 的模块（`nsh`、`nsh/*`、`nsh/ui/mobile`），但不能导入其他文件或包。`console.log` 的输出和错误显示在手机下方。

编译器也是一个 Rust 函数：`nsh_bundler::compile_module(name, source, ui_sets)`，不开启该 crate 的 `bundle` 特性也能使用。

## 嵌入到页面

`nsh.js` 是一个 ES 模块。`createPlayer` 把运行时加载进一个容器，`run` 启动一个应用包（即 `nsh build` 写出的 `bundle.js`）：

```js
import { createPlayer } from "./nsh.js";

const player = await createPlayer(document.querySelector("#phone"), {
  wasm: "./nsh.wasm",
  assetBase: "./assets/",   // `asset:/…` URI（导入的图片）从这里加载
  // dark: true,            // 强制外观；不设置时跟随系统
});
player.run(await (await fetch("./bundle.js")).text());
```

使用 `nsh-playground.wasm` 时，`player.runSource(tsx)` 会先编译 TSX，返回编译错误或 `null`；`player.compile(tsx)` 返回 `{ code }` 或 `{ error }`。

容器的尺寸就是应用的窗口（`windowSize()`），调整容器大小会同步调整应用。`player.setDark(true)` 可在运行时切换外观，应用通过 `colorScheme()` 和 `dark:` 类感知到变化。

> [!NOTE]
> Web 版把 `Platform.OS` 报告为 `"ios"`，所以应用呈现的是 iOS 外观。它的用途是展示应用，而不是第三个目标平台。

## 限制

Web 运行时用于演示和文档，下列能力不提供：

- **网络**：`fetch` 会以 "this host has no network stack" 失败。
- **Web 视图、视频和原生组件**：`WebView` 只以最简单的方式映射为 `<iframe>`，`Video` 映射为 `<video>`；注册的原生组件渲染为空盒子。
- **平台服务**：状态栏、Toast 和 Android 返回键不起作用，也没有需要避让的软键盘（`keyboard()` 始终为 0）。
- **首次使用前加载好的自定义字体**：导入的字体通过 `FontFace` 注册，它是异步加载的，所以第一帧可能用后备字体测量。
- **文本整形细节**：换行规则只是 UAX #14 的一个子集（空格、CJK、连字符），没有自动断词，除段落方向外也不做双向重排。
- **下拉刷新和可缩放的滚动视图。**

QuickJS 虚拟机以开发模式运行（不做加固），使用默认的[资源限制](./sandbox-and-capabilities.md)。

## 另请参阅

- [架构](./architecture.md)：Web 版复用的引擎、核心和挂载批次。
- [测试与 trace](./testing-and-trace.md)：`nsh trace` 以近似的文本引擎无界面地运行同一个引擎。

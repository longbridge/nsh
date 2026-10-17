---
title: GPUI
description: 在 macOS、Windows 和 Linux 上运行由 GPUI 绘制的 nsh 应用、打包应用、用 AppView 嵌入 GPUI 或 GPUI Kit 应用，以及 GPUI 后端的不同之处。
order: 15.5
group: platforms
---

# GPUI

GPUI 后端用 [GPUI](https://gpui-kit.com)（GPUI Fast）绘制 nsh 应用。GPUI 是 GPUI Kit 所基于的 GPU 加速 UI 框架，在 macOS、
Windows 和 Linux 上表现一致。它的主要用途是嵌入：`AppView` 是一个 GPUI 视图，GPUI 或 GPUI Kit 应用可以在自己元素树的任意位置
显示 nsh 模块。核心、响应式图、渲染器、Taffy 布局和 `nsh/ui/mobile` 组件与其他所有平台完全相同，因此同一个 bundle 可以在所有
平台上运行。`Platform.OS` 是它所运行的操作系统（`"macos"`、`"windows"` 或 `"linux"`），并会应用 `desktop={{…}}` 属性。

> [!WARNING]
> GPUI 后端正在开发中。原生 [macOS](./macos.md)（AppKit）、[Windows](./windows.md)（Win32）和 [Linux](./linux.md)（GTK）
> 后端的元素和模块它都已具备，但部分绘制效果不同（见[差异](#差异)），而且目前只在 Linux 上试过。目前要发布桌面应用，请使用原生后端。

## 工作方式

- **每个界面一个元素。** 一个界面（surface）是一个 GPUI 元素，它填满父元素给它的空间，用 GPUI 的图元（矩形、阴影、路径、字形和
  图片）绘制整棵视图树。与 Windows 后端一样，nsh 为每个节点保留一个视图，只在提交改变了内容时才重绘。
- **文字在引擎线程上排版**，使用 GPUI 的文字系统（应用自己的字体），由 nsh 断行；主线程只绘制字形。
- **每次提交一次跳转。** 引擎运行在自己的线程上；每次提交是发往 GPUI 主线程的一个闭包，由它通知视图重绘。
- **输入**来自 GPUI 的鼠标、滚动、按键和输入法事件；命中测试、按压、响应者系统、滚动和文本编辑与 Windows 后端是同一套逻辑。

## 环境要求

- macOS、Windows 或 Linux，以及 GPUI 能使用的 GPU（macOS 上是 Metal，Windows 上是 DirectX，Linux 上是 Vulkan）。
- Linux 上：Vulkan 驱动、带 `libxkbcommon` 的 Wayland 或 X11、GTK 3 和 WebKitGTK 4.1（Web 视图），以及 GStreamer 1.20
  或更新版本及其 base 和 good 插件（视频）。Web 视图需要 X11：在 Wayland 桌面上请让应用运行在 Xwayland 下（把
  `WAYLAND_DISPLAY=` 设为空），否则显示占位内容。从源码构建运行时还需要开发包；在 Debian 和 Ubuntu 上：

  ```sh
  sudo apt-get install clang libclang-dev pkg-config libfontconfig-dev libfreetype-dev libwayland-dev libx11-xcb-dev \
    libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libzstd-dev libvulkan1 libgtk-3-dev libwebkit2gtk-4.1-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good
  ```

## 运行应用

```sh
nsh run my-app --target gpui
```

在一个 GPUI 窗口（默认 1200×800）中打开应用，和其他目标一样支持热重载、在终端显示日志并提供错误浮层。开发宿主
（`nsh-gpui-host`）在每个操作系统上都直接运行，不需要外面包一层应用包。在源码仓库中，`nsh run` 会先构建它；发行版把它放在
`runtime/gpui/` 中。宿主也可以单独运行一个构建好的 bundle：

```sh
nsh build my-app --release -o build
dist/nsh/runtime/gpui/nsh-gpui-host --bundle build/bundle.nshc --size 390x844   # 源码仓库中：target/debug/nsh-gpui-host
```

`--size` 是窗口的尺寸，单位为点（`<宽>x<高>`）。

## 打包

```sh
nsh package my-app --target gpui [--run]
```

包的结构与打包所在机器的原生后端相同，只是可执行文件换成 GPUI 的发布版运行器（`nsh-gpui-app`）：

| 打包所在系统 | 输出 |
|---|---|
| macOS | ad hoc 签名的 `<Name>.app`，与 [`--target macos`](./macos.md#打包) 相同 |
| Windows | 一个目录 `<Name>\`（`<Name>.exe` 和 `nsh\`）和 `<Name>.zip`，与 [`--target windows`](./windows.md#打包) 相同 |
| Linux | 一个目录 `<Name>/`（可执行文件、`nsh/` 和 `.desktop` 文件）和 `<Name>.tar.gz`，与 [`--target linux`](./linux.md#打包) 相同 |

和原生后端一样，运行器只为构建它的机器构建：Mac 应用在 Mac 上打包，Windows 应用在 Windows 上打包，Linux 应用在 Linux 上打包。
`--run` 会在打包后启动应用。

Linux 包动态链接 WebKitGTK 4.1（Web 视图）和 GStreamer（视频），因此运行它的机器需要安装它们，以及 GTK 3 和 GStreamer 的 base、good 插件（Debian / Ubuntu：`libwebkit2gtk-4.1-0`、`libgstreamer1.0-0`、`libgstreamer-plugins-base1.0-0`、`gstreamer1.0-plugins-good`），此外还需要 Vulkan 驱动和 `libxkbcommon`。

## 嵌入 GPUI 应用

`nsh-platform-gpui` crate（来自 nsh 仓库）为 Rust GPUI 应用提供 `AppView`：一个以 GPUI 视图（`Entity<AppView>`）形式存在的
nsh 引擎，填满父元素给它的空间。每个 `AppView` 相互独立，可以同时运行多个。

```rust
use gpui::{AppContext, Application, WindowOptions};
use nsh_platform_gpui::{App, AppOptions, AppView, Bundle};

gpui_platform::application().run(|cx| {
    cx.open_window(WindowOptions::default(), |window, cx| {
        cx.new(|cx| {
            let view = AppView::new(AppOptions::default(), window, cx);
            let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
            view.load(App::Js(Bundle::source(code)));
            view
        })
    })
    .unwrap();
});
```

像其他视图一样把这个 entity 放进元素树（GPUI Kit 的 `Root`、一个面板、一个停靠区）。`AppView::load` 替换正在运行的内容；
`stop`（或丢弃视图）结束应用。`AppOptions::asset_base` 指定 `asset:/` URI 的解析位置（`AssetBase::Directory` 或
`AssetBase::Http`）。字节码 bundle（`bundle.nshc`）用 `unsafe { Bundle::bytecode(bytes) }`（或同样是 unsafe 的
`Bundle::read_trusted(path)`）加载：QuickJS 字节码是受信任的输入，只应从你自己应用的文件中加载。

- `open_app_window(source, options, cx)` 在运行中的 GPUI 应用里打开一个带 `AppView` 的窗口。
- `run_app(source, options)` 启动 GPUI，在一个窗口中运行一个应用直到窗口关闭（发布版运行器就是这样做的）。
- `AppView::register_component(name, factory)` 在加载前注册你自己的原生组件（[原生组件](./native-components.md)）。
- `set_ui_sets(sets)` 替换默认的 `nsh/ui/mobile` 套件，用于带有自己 Rust 组件的应用。

## 差异

与原生桌面后端在绘制或行为上的不同之处：

- **变换**：旋转、斜切和透视的视图，其背景、边框和阴影会随之变换，但文字保持直立，图片、视频和画布按其边界绘制。
- **不透明度**作用于每种颜色而不是整组，因此半透明视图中相互重叠的子视图会彼此透出；图片的不透明度（淡入、半透明视图）以
  十六分之一为级变化。
- **圆角裁剪**（带圆角的 `overflow: "hidden"`）会让子视图的背景和图片跟随圆角，但文字、边框、视频和原生组件按矩形裁剪。
- **不绘制**：`filter: blur()`、`backdropFilter` 和 `mixBlendMode`；阴影会透过半透明填充显示；`borderCurve: "continuous"`
  绘制为圆形圆角。
- **画布文字**是近似的：旋转时保持直立，`maxWidth` 下缩小字号而不是压窄，渐变以单一颜色绘制，`strokeText` 绘制为填充，阴影
  不模糊，合成操作和 `clearRect` 对它不起作用。
- **滚动视图**不支持缩放（`zoomTo` 和缩放相关属性被忽略）。
- **视频**在 Linux 上播放（GStreamer）；在 macOS 和 Windows 上显示占位内容并报告 `onError`。
- **Web 视图**是位于 nsh 在该位置所绘制一切内容之上的原生视图。Linux 上需要 X11（见[环境要求](#环境要求)）；在 Xwayland
  下页面一直是黑的（日志中有 "Failed to create GBM buffer"）时，请用 `WEBKIT_DISABLE_DMABUF_RENDERER=1` 运行应用。macOS 和
  Windows 上裁剪祖先不会裁剪它们，加载失败也不会报告 `onError`。不支持 `source.baseUrl`、`scrollEnabled` 和
  `allowsBackForwardNavigationGestures`。
- **无障碍**通过 AccessKit 实现：读屏软件在所有平台上都能激活、聚焦、设置值并把元素滚动到可见；自定义操作和上下文菜单操作
  尚未传递。
- **尚未在 macOS 和 Windows 上试过**：已为它们构建，但目前只在 Linux 上运行过。

可对照各原生后端自己的列表：[macOS](./macos.md#macos-上的差异)、[Windows](./windows.md#windows-上的差异)、
[Linux](./linux.md#linux-上的差异)。

## 另见

- [命令行](./cli.md)：`nsh run --target gpui`、`nsh package --target gpui`
- [打包](./packaging.md)
- [平台概览](./platforms.md)

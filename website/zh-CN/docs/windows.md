---
title: Windows
description: 在 Windows 上用 Direct2D 绘制的 Win32 窗口运行 NSH 应用、打包应用、用 AppView 嵌入 Win32 应用，以及 Windows 上的差异。
order: 14
group: platforms
---

# Windows

NSH 应用也能在 Windows 上运行。Windows 后端在 Win32 窗口中用 Direct2D 绘制应用，用 DirectWrite 排版文字；核心、响应式图、
渲染器、Taffy 布局和 `nsh/ui/mobile` 组件与 iOS、Android 上完全相同，因此同一个 bundle 可以在所有平台上运行。在 Windows 上，
`Platform.OS` 为 `"windows"`，并会应用 `desktop={{…}}` 属性。窗口标题、自定义标题栏、
菜单栏和右键菜单由 [`nsh/desktop`](../modules/nsh-desktop.md) 提供。

## 环境要求

- **Windows 10 1809** 及以上，或 Windows 11。
- Web 视图需要 **WebView2 Runtime**。Windows 11 自带，Windows 10 上随 Microsoft Edge 安装；没有它时 `WebView` 显示一个占位视图。
- 应用要播放的视频格式对应的 **Media Foundation 解码器**（视频通过 Windows 自带的媒体栈播放）。
- 构建 NSH 的 Windows 运行时（从源码构建，或在 Windows 上运行 `cargo xtask dist`）需要带 C++ 工作负载的 Visual Studio
  Build Tools、Windows SDK 和 LLVM（用于生成 QuickJS 的绑定）。

## 运行应用

在 Windows 上，`nsh run` 会在一个窗口（默认 1200×800）中打开应用，和其他目标一样支持热重载、在终端显示日志并提供错误浮层：

```sh
nsh run my-app                 # 在 Windows 上默认就是 --target windows
```

在源码仓库中，`nsh run` 会先构建开发宿主（`nsh-windows-host.exe`）；在 Windows 上制作的发行版把它放在 `runtime\windows\` 中。
Ctrl-C 会关闭窗口，关闭窗口也会结束 `nsh run`；无论 `nsh` 以何种方式结束，宿主都不会比它活得更久。宿主也可以单独运行一个构建好的 bundle：

```sh
nsh build my-app --release -o build
dist\nsh\runtime\windows\nsh-windows-host --bundle build\bundle.nshc --size 390x844   # 源码仓库中：target\debug\nsh-windows-host
```

`--size` 是窗口的尺寸，单位为点（`<宽>x<高>`）。

## 打包

```sh
nsh package my-app --target windows [--run]
```

会写出一个目录，以及同样内容的压缩包：

```text
out\<Name>\
  <Name>.exe             可执行文件（nsh 的发布版运行器），带应用图标
  nsh\bundle.nshc         编译后的应用
  nsh\assets\…           应用导入的资源
  nsh\app.json           窗口标题、应用 id、版本和外观
out\<Name>.zip
```

可执行文件会在自身旁边找到 `nsh\`；把整个目录复制到（或把 `.zip` 解压到）应用要放的位置即可，不需要安装。`nsh.json`
中的图标会直接嵌入可执行文件（资源管理器和任务栏都会显示它），不需要额外的工具；在其他系统上打包则保留默认图标。应用 id
用于在任务栏中归组应用的窗口。`--run` 会在打包后启动应用。可执行文件没有签名：要分发，请用你自己的证书签名（`signtool`）。

## 嵌入 Win32 应用

`nsh-platform-windows` crate（位于 NSH 仓库中）为 Rust 编写的 Win32 应用提供 `AppView`：一个在你的窗口的子窗口中运行的 NSH
引擎。每个 `AppView` 相互独立，可以同时运行多个。

```rust
use std::rc::Rc;
use nsh_platform_windows::{
    windows::{self, Win32::{Foundation::HWND, System::Ole::OleInitialize, UI::WindowsAndMessaging::MoveWindow}},
    AppOptions, AppView, AssetBase, Bundle,
};

/// 在运行消息循环的线程上调用，传入你的窗口。
fn embed(parent: HWND) -> windows::core::Result<Rc<AppView>> {
    // 每个线程一次：COM 和 OLE（剪贴板、图片解码）。
    unsafe { OleInitialize(None)? };
    let view = AppView::new(
        Some(parent),
        AppOptions {
            asset_base: Some(AssetBase::Directory("app/assets".into())),
            app_id: "com.example.host".into(),
            ..AppOptions::default()
        },
    )?;
    // 在加载之前注册原生组件
    let code = std::fs::read_to_string("app/bundle.js").expect("bundle");
    view.load(Bundle::source(code).into());
    Ok(view)
}

// 在 `parent` 的 WM_SIZE 中：
// unsafe { MoveWindow(view.hwnd(), 0, 0, width, height, true) };
```

视图是一个子窗口：由你的消息循环驱动，父窗口改变大小时用 `MoveWindow` 摆放它的 `hwnd()`。`AppView::load` 会替换正在运行的
内容；`stop`（或销毁窗口）结束应用。`asset_base` 指定 `asset:/` URI 从哪里解析（`AssetBase::Directory` 或 `AssetBase::Http`）。
字节码 bundle（`bundle.nshc`）用 `unsafe { Bundle::bytecode(bytes) }`（或同样是 unsafe 的 `Bundle::read_trusted(path)`）加载：
QuickJS 字节码是受信任的输入，只能从你自己应用的文件中加载。独立窗口用 `nsh_platform_windows::run_app(source, options)`。

原生组件通过 `AppView::register_component` 注册，见[原生组件](./native-components.md#在-windows-上注册rust)。

## Windows 上的差异

运行环境是桌面环境：安全区和键盘高度为 0，`Platform.constants.interfaceIdiom` 为 `"desktop"`，`Platform.Version` 为 `"10.0"`
（Windows 11 上也是：Windows 沿用了这个版本号）。深色模式实时跟随“设置 > 个性化 > 颜色”，`fontScale()` 跟随“文本大小”设置，
减少动态效果跟随“动画效果”设置，区域设置跟随用户的区域格式。见[设备与环境](./device-and-environment.md)。

### 键盘

| 按键 | 作用 |
|---|---|
| Return | 在 `Input` 中按 `submitBehavior` 行事：提交（`onSubmitEditing`）、提交并离开输入框，或换行（多行输入框） |
| Shift+Return、Alt+Return | 在回车即提交的多行输入框中换行（Ctrl+Return 提交） |
| Tab、Shift+Tab | 移到下一个 / 上一个文本框，只在最上层的模态窗口之内；与 Windows 的对话框一样，文本框中的文字会被选中 |
| Escape | 离开有焦点的输入框；否则清除文本选区，再返回上一页或请求关闭最上层的模态窗口 |
| Alt+Left、浏览器后退键、鼠标的后退键 | 返回上一页（在输入框中也是） |
| Ctrl+方向键、Home / End、Ctrl+Home / End，加 Shift；Ctrl+Backspace / Delete | 在输入框中按词和行移动、选择、删除，与其他 Windows 应用一致 |
| Ctrl+Z、Ctrl+Y、Ctrl+X / C / V / A；Shift+Delete、Ctrl+Insert、Shift+Insert | 撤销、重做、剪切、复制、粘贴、全选（密码输入框拒绝剪切和复制）；Ctrl+A / Ctrl+C 选择并复制 `selectable` 文本 |
| 方向键；Page Up / Down、Space、Shift+Space；Home / End、Ctrl+Home / End | 滚动有焦点的视图所在或鼠标下方的滚动视图：一行、一页、到开头 / 结尾（输入框有焦点时不滚动） |
| F11、Alt+F4 | 全屏、关闭窗口（`nsh package` 和 `nsh run` 的应用） |
| Ctrl+R、F5 | 重新加载最新的构建（`nsh run`） |

快捷键适用于任何键盘布局（在西里尔或希腊字母布局下，Ctrl+C 是美式布局中输入 C 的那个键）。确认输入法组合文字（中文、日文等）的回车
不会提交。`onKeyPress` 报告的与 React Native 一致：输入的文字、`"Enter"`、`"Backspace"` 和 `"Escape"`；粘贴的文字不报告。

应用自己的快捷键（[`nsh/keymap`](../modules/nsh-keymap.md)）先于有焦点的输入框、导航和滚动处理按键；Alt+F4、F11 和 Ctrl+Q 不能被绑定。

不同之处或不支持的功能：

- **`WebView`**：位于 3D 变换（透视）视图内的网页不显示；网页上方的 `backdropFilter` 不会模糊网页。`source.baseUrl`
  只接受 `http(s)` 地址。自带窗口的原生组件始终位于 NSH 绘制的所有内容之上。
- **视频**：只能播放已安装解码器支持的格式；没有封面图（poster）；开启 `controls` 时控制栏始终显示。
- **可缩放的滚动视图**（`zoomTo`、缩放相关属性）；滚动条不能拖动。
- **`mixBlendMode` 和 `backdropFilter`** 会绘制，但在带 `filter` 或 3D 变换的视图内部不生效；`borderCurve: "continuous"`
  绘制为圆形圆角。
- **文本**：头部和中间省略号只用于单行（多行时改为尾部省略号）；彩色 emoji 只支持 COLR 字体；小型大写字母只在字体提供时可用。
- **文本输入**：键盘类型和自动填充不适用（没有软键盘）；Tab 在文本框之间切换。右键打开编辑菜单（撤销、剪切、复制、粘贴、删除、
  全选），可选中的文本右键也有菜单（复制、全选）。
- **无障碍**（UI Automation）：导航栏的标题和按钮、对话框和 toast 不会暴露出来；标题（heading）的级别都报告为 1。
- **输入辅助视图**和**状态栏**（桌面上没有对应物）。

## 另见

- [CLI](./cli.md)：`nsh run --target windows`、`nsh package --target windows`
- [打包](./packaging.md)
- [平台特定代码](./platform-specific.md)

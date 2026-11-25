---
title: 架构
description: NSH 应用是如何运行的：构建流程、引擎线程与主线程、从 signal 变化到原生视图的路径、原生响应式图、文本布局以及各个 crate。
order: 63
group: tools
---

# 架构

本页讲从 `count.set(1)` 到屏幕上的标签变化，中间发生了什么。Rust 应用和 TSX 应用都适用。写应用不必先读它，
但它能解释 NSH 的几个行为：组件为什么只运行一次，JS 忙时动画为什么仍然流畅，主线程为什么从不等你的代码。

## 全局概览

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/architecture-light.svg" alt="nsh 应用如何运行：nsh CLI 把 App.tsx 构建为字节码；在引擎线程上，QuickJS 运行事件处理函数，nsh-js 的响应式图把操作排入队列，nsh-core 提交生成一个 MountBatch；它一次跳转到主线程，应用到 UIKit 或 Android 视图上；触摸作为事件传回引擎线程。原生动画在主线程运行，不经过 JavaScript。">
<img class="diagram diagram-dark" src="/diagrams/architecture-dark.svg" alt="nsh 应用如何运行：nsh CLI 把 App.tsx 构建为字节码；在引擎线程上，QuickJS 运行事件处理函数，nsh-js 的响应式图把操作排入队列，nsh-core 提交生成一个 MountBatch；它一次跳转到主线程，应用到 UIKit 或 Android 视图上；触摸作为事件传回引擎线程。原生动画在主线程运行，不经过 JavaScript。">
</div>

## 构建：只有你的代码会成为 JS

`nsh build`、`nsh dev` 以及其他命令使用一套用 Rust 编写的工具链（无需 Node.js）打包你的应用：

1. **Oxc** 去掉 TypeScript 类型。
2. **NSH 的 JSX 编译器**在同一遍处理中把 JSX 转换为对原生渲染器的直接调用。静态的 prop 和子节点只设置一次。可能变化的
   表达式（调用了函数或读取了属性）会变成绑定，输入变化时渲染器会更新它。组件变成普通函数调用，
   会变化的 prop 在 props 对象里以 getter 提供。
3. **rolldown** 把你的模块打包成一个 ES 模块。对 NSH 模块（`nsh`、`nsh/base`、`nsh/mobile` 等）的导入原样保留，
   因为这些模块由运行时以原生方式提供。**bundle 只包含你的应用代码。** 计数器示例的 JS 不到 1 KB。
4. bundle 编译成 **字节码**（`bundle.nshc`），加载比源码快。整个流程还会生成一份 **source map**，
   堆栈跟踪因此能指向你的 `.tsx` 代码行。

你导入的图片和字体会作为资源收集起来，供编辑器使用的类型声明则写入 `.nsh/types`。

Rust 应用没有 bundle。Cargo 为每个目标构建运行器，并把你的 crate 链接进去。`nsh.json` 指定的根组件直接在引擎上以原生方式运行。下文同样适用于 Rust 应用，区别只在于组件代码跑在 Rust 里，而不是 QuickJS 里。

## 运行：每个应用两个线程

每个运行中的应用就是一个**引擎**：在 iOS 上是一个 `NshAppView`，在 Android 上是一个 `NshView`，在打包应用里是一个窗口。
每个引擎有自己的**引擎线程**，TSX 应用还有自己的 QuickJS 虚拟机。没有任何全局单例，所以一个宿主应用中可以并行运行多个应用。

### 引擎线程

应用逻辑的一切都发生在这里：

- **你的代码**在这里运行：组件、事件处理函数、定时器、effect。Rust 组件以原生方式运行；TSX 运行在 **QuickJS** 中。
- **响应式图**（signal、computed 值、effect、owner、context）是原生的。图中节点是原生类的 QuickJS 对象，
  垃圾回收器能借此看到引用关系。依赖追踪和更新调度在 Rust 中完成，算法沿用 Solid 1.9。
- 编译后的 JSX 所调用的**渲染器**是原生的。它创建节点，把每个变化转成一个 **op**（一条小的原生指令），直接推入原生队列。
  不走 JSON，也不会每个 prop 发一条消息。
- **core** 维护所有节点的影子树（shadow tree）。一个 JS 轮次结束时，它一次取出所有排队的 op，然后 **commit**。一次 commit 会：
  - 渲染 Rust 组件（`nsh/base` 的组件和应用自己的组件）；
  - 解析 padding 和 margin 里的 `env()` 边距；
  - 重建变化了的文本；
  - 用 **Taffy** 布局（flexbox 和 grid，默认纵向排列，不收缩）；
  - 对 frame 和样式做 diff；
  - 生成一个 **MountBatch**，即原生视图需要执行的操作列表。

### 主线程

平台的主（UI）线程一次跳转就应用完一个 MountBatch：创建并插入视图，设置样式和 frame，交付排好版的文本。
除此之外，它只做必须在主线程上做的事：

- **绘制文本**，这些文本已在引擎线程上布局完成；
- **原生动画**：`nsh` 的动画值逐帧驱动（iOS 上用 `CADisplayLink`，Android 上用 UI 线程动画器），并直接写入视图；
- **手势和滚动**：触摸转换为事件发送到引擎线程；`scrollValue` 和 `pan` 在同一帧内为动画值提供数据。

**主线程从不等待 JS。** 有些问题系统要立刻得到答复，比如这段文本能不能改、返回键能不能退出当前页面、这个 sheet 能不能关闭。
NSH 事先把答案作为 prop 挂载好，JS 之后再通过事件得知结果。引擎线程忙时，更新可能延迟，但滚动、动画和系统手势不会卡住。

## 从一次点击到屏幕

```text
 main thread               engine thread                                              main thread
 ───────────               ─────────────────────────────────────────────────────      ───────────
 tap on a view ─ event ─▶  handler runs: count.set(count() + 1)
                           reactive graph: the bindings that read count() run again
                           renderer: one op per changed binding → native queue
                           JS turn ends → core commit: render, text, layout, diff
                           → MountBatch (e.g. 1 Text item)                 ─ hop ─▶   apply: draw the new text
```

在计数器示例中，用 Mac 上的 release 构建测，一次点击的 JS 和 commit 合计约 0.12 ms，只产生几个 mount item（参见
[测试](./testing-and-trace.md)）。

在 Rust 应用中，处理函数是一个 Rust 闭包（`count.set(count.get() + 1)`），之后是同样的响应式图、commit 和 batch，中间没有解释器。

这一设计带来三个特性：

- **一个轮次就是一次 commit。** 无论处理函数设置了多少个 signal，视图都只一起变化一次。
- **组件只运行一次。** 只有绑定会重新运行。组件不会重新渲染，也没有虚拟 DOM diff。唯一剩下的 diff 是 core 在 Rust 里
  比较 frame 和样式。
- **只传输变化。** 没变的样式和 frame 不发送。新视图不带默认值。

## 文本：在主线程之外只布局一次

文本往往是 UI 里开销最大的部分。NSH 在引擎线程上用平台自带的文本引擎排版，所以效果和原生文本完全一致。排版分三步：

1. **prepare**：把一个 `<Text>` 和其中嵌套的 span 合成一个属性字符串（Apple 平台用 CoreText，Android 用
   `PrecomputedText` / `StaticLayout`）。
2. **measure**：按布局要求的宽度测量。结果按内容和宽度缓存，只改颜色不会重新测量。
3. **layout**：按最终尺寸排版。

主线程拿到不可变的结果，只负责绘制。`<Text>` 里再嵌套的 `<Text>` 是 span，不是视图。写在 `<Text>` 外面的字符串不会显示。

## 列表

`List` 在原生侧做虚拟化：只有靠近视口的行才会创建视图。core 挑出这些行，计算位置、多列布局和吸顶标题，并复用单元格。
单元格要显示另一个条目时，运行时会重新绑定它，行函数里的 `item()` 和 `index()` accessor 随之变化。没有露出新行的帧完全不调用 JS。
参见[列表](./lists.md)。

## 运行时做的其他事情

- **网络**：`fetch` 和其他全局 API 都是原生类。请求交给平台的 HTTP 栈（`NSURLSession`、`HttpURLConnection`），双向流式传输。
- **图片**在主线程之外下载和解码，按视图尺寸处理，并带有内存缓存。
- **限制和能力**：运行时在 Rust 中执行检查，参见[沙箱与能力](./sandbox-and-capabilities.md)。
- **热模块替换**也是原生的。运行时应用一个补丁远不到一毫秒（参见[开发服务器与调试](./dev-server-and-debugging.md)）。

<a id="three-layers"></a>

## 三层结构

NSH 的界面分为三层，与 GPUI Kit 的分层相同：

| 层 | NSH | GPUI Kit | 说明 |
|---|---|---|---|
| 基础元素 | `nsh-core` 和运行时（`nsh`、`nsh/mobile`、`nsh/desktop`） | `gpui` | 宿主元素、样式、布局、渲染、响应式、环境 |
| 基础组件 | `nsh-base`（`nsh/base`） | `gpui-base` | 无样式的组件，负责行为、状态、焦点、键盘和无障碍，外观取自应用 |
| 组件库 | 你的应用自己的，或社区的 | `gpui-component` | 带样式的组件：带有外观的按钮、卡片、列表行 |

前两层属于 Native Shell，第三层由你决定。你可以在 `nsh/base` 和基础元素之上搭自己的组件库，用 TSX 写，或者写成
[自己 crate](./rust.md) 里的 Rust 组件。也可以用社区发布的组件库，或者直接在应用里写样式。

## crate 一览

NSH 是一个 Rust workspace。写应用时不需要自己构建它，发行版已经带了 CLI 和预构建的运行时。不过这些 crate 名会出现在日志和文档里：

| Crate | 说明 |
|---|---|
| `nsh-core` | 平台无关的核心：影子树、样式、Taffy 布局、文本合并、列表虚拟化、mount item；CSS 值和 Tailwind；原生动画图（值、插值、timing / spring / decay）；Rust 组件的框架和主题；运行时用 Rust 编写的模块（`nsh/mobile` 的 `BackHandler` 等）。 |
| `nsh-base` | `nsh/base`，无样式的基础组件（NSH 的 `gpui-base`）：导航、对话框、控件和状态栏；Rust 应用以 `nsh_base` 使用它。 |
| `nsh-js` | QuickJS 宿主和 NSH 的运行时：响应式图、渲染器、所有原生模块、全局 API、热模块替换、限制；以及这一切的 TypeScript 声明。 |
| `nsh-engine` | 引擎线程：驱动 `nsh-js` 和 `nsh-core`，每帧向平台交付一个 batch。 |
| `nsh-package` | 签名包：密钥、证书、吊销、信任库。 |
| `nsh-bundler` | 打包器：rolldown、Oxc、NSH 的 JSX 编译器、source map、Tailwind 类名检查。 |
| `nsh-dev` | 开发服务器及其客户端。 |
| `nsh-platform-ios` | UIKit 后端（iOS、模拟器、Mac Catalyst）：视图挂载、CoreText 文本引擎、输入；嵌入 SDK。 |
| `nsh-platform-android` | Android 后端：`libnsh.so` 和 Java 运行时（`NshView`、各类视图、图片、网络）。 |
| `nsh-cli` | `nsh` 命令。 |

依赖方向是 `nsh-core` ← `nsh-js` ← `nsh-engine` ← 各平台；`nsh-base` 构建在 `nsh-core` 之上。core 不依赖任何平台，也不依赖 JS 引擎。所以
`nsh test` 能在任何机器上以无界面方式跑真实的运行时。

## 另请参阅

- [性能](./performance.md)
- [响应式](./reactivity.md)

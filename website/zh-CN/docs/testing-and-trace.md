---
title: 用 nsh trace 测试
description: 用 nsh trace 无界面运行应用，读懂它打印的原生视图操作和耗时，模拟点击和滚动，并在 CI 中使用；以及用于 UI 测试的 testID。
order: 61
group: tools
---

# 用 nsh trace 测试

`nsh trace` 以 **无界面（headless）** 方式运行应用：使用真实的运行时（与设备上相同的 QuickJS VM、响应式图、渲染器、
组件、布局和列表虚拟化），但不需要模拟器、设备或窗口。它不进行绘制，而是打印运行时本应发送给原生视图的每一个操作，
并附带耗时。只要 CLI 能运行的地方它都能运行，包括 Linux CI 机器。

```sh
nsh trace App.tsx
```

它的用途：

- 检查应用（或某个 demo）能否无错误地构建、执行和渲染；
- 准确查看一次改动创建、移动或更新了哪些原生视图，以及数量；
- 测量首帧、一次点击以及每一帧滚动的开销；
- 在 CI 中发现回归。

## 它做了什么

1. 打包入口（开发配置，不压缩）并编译为字节码。
2. 以 390×844 的视口启动引擎，渲染首帧。
3. 可选地滚动第一个滚动视图或列表。
4. 在应用空闲时等待（默认 1.2 s，让引擎的空闲垃圾回收先运行，从而暴露对象保留方面的 bug），同时打印应用在此期间渲染
   的任何内容（定时器、fetch、动画）。
5. 对第一个可按压视图（第一个带 `onPress` 的视图）点击若干次，并打印每次点击带来的变化。

| 参数 | 默认值 | 说明 |
|---|---|---|
| `--taps <n>` | 2 | 对第一个可按压视图发送的点击次数。 |
| `--idle-ms <ms>` | 1200 | 点击前的空闲时间。 |
| `--scroll <points>` | 无 | 将第一个滚动视图或列表滚动这么多点，每帧 10 pt，并打印每一帧的批次。 |
| `--width <points>` | 390 | 视口宽度。 |
| `--height <points>` | 844 | 视口高度。 |
| `--data-dir <DIR>` | | 为此次运行设置显式磁盘数据根目录；默认使用内存数据。 |
| `--os <OS>` | `ios` | 模拟 `ios`、`android`、`macos`、`windows`、`linux` 或 `web`；影响平台 API 和属性的选择。 |


## 隔离磁盘数据

使用显式数据根目录验证原生文件读写：

```sh
nsh trace App.tsx --os linux --data-dir "$HOME/.cache/my-app-trace" --taps 0
```

运行时在该根目录下按应用 id 隔离数据，`${dataDir}` 授权仍由应用声明与宿主配置决定。选择专用于测试的目录，避免改写日常应用数据。默认情况下 trace 使用内存数据，原生文件操作需要显式的 `--data-dir` 根目录。

`--os linux` 选择 Linux 平台环境；`--data-dir` 本身不会改变平台。进程等平台 API 还要求实际原生宿主支持该能力；模拟平台不能让移动端或浏览器宿主获得桌面进程能力。

## 读懂输出

下面是计数器示例（`examples/counter/App.tsx`）的输出，有删节：

```text
✓ bundled App.tsx (3255 B in 14.3 ms)
perf commit: 56 ops, 395µs (list js 6µs), 49 mount items, 21 nodes
perf first frame: eval 271µs, total 1235µs
── initial render: 49 items
   LayoutDirection { rtl: false }
   Create { id: 2, kind: View }
   Create { id: 4, kind: Text }
   Insert { parent: 2, child: 4, before: None }
   ...
   Events { id: 20, events: EventMask(1) }
   Frame { id: 2, x: 0, y: 0, w: 390, h: 844 }
   Frame { id: 4, x: 24, y: 64, w: 342, h: 34 }
   ...
   Text { id: 4, text: "nsh counter", content: 342x34 @ 0,0 }
   Text { id: 14, text: "0", content: 36x77 @ 0,0 }
perf event Press on view 20
perf event js: 196µs
perf commit: 10 ops, 248µs (list js 6µs), 8 mount items, 24 nodes
── tap #1 on view 20: 8 items
   Create { id: 42, kind: Text }
   Insert { parent: 32, child: 42, before: None }
   Frame { id: 14, x: 136, y: 49, w: 71, h: 77 }
   Frame { id: 32, x: 24, y: 366, w: 342, h: 43 }
   Frame { id: 42, x: 0, y: 23, w: 342, h: 20 }
   Text { id: 14, text: "-1", content: 71x77 @ 0,0 }
   Text { id: 42, text: "• -1", content: 342x20 @ 0,0 }
   Text { id: 34, text: "Recent values: 1", content: 342x17 @ 0,0 }
```

### 批次与 mount item

每个 `── title: N items` 块是一个 **mount 批次（mount batch）**：平台在一帧内应用到原生视图上的全部内容，一次性切换到
主线程完成。其中的每一行是一个 **mount item**：

| Item | 含义 |
|---|---|
| `Create { id, kind }` | 创建一个原生视图（`View`、`Text`、`Image`、`Scroll` 等）。 |
| `Insert { parent, child, before }` | 把视图放进父视图，位于某个兄弟视图之前（`None`：放在末尾）。 |
| `Remove` / `Delete` | 将视图从父视图中移除 / 释放视图。 |
| `ViewStyle { id, style }` | 视图的装饰：背景、边框、圆角、阴影、不透明度、变换等。 |
| `ViewProps { id, props }` | 触摸和无障碍设置、`testID`。 |
| `Events { id, events }` | 视图上报哪些事件（`EventMask(1)` 表示按压）。 |
| `Frame { id, x, y, w, h }` | 它在父视图中的位置和大小，单位为点。 |
| `Text { id, text, content }` | 文本视图排版后的文本。 |
| `Image { id, source, ... }` | 要加载的图片。 |

只发送发生变化的内容：一次只改变一个数字的点击只发送一个 `Text` item；视图不会收到默认样式，也不会收到空的事件掩码；
未变化的 frame 不会发送。一帧滚动如果没有露出新行，**根本不会产生批次**。

### `perf` 日志行

| 行 | 含义 |
|---|---|
| `✓ bundled App.tsx (3255 B in 14.3 ms)` | 打包耗时和 bundle 大小（开发配置）。 |
| `commit: 56 ops, 395µs (list js 6µs), 49 mount items, 21 nodes` | 一次提交：JS 排队的操作数、核心花费的时间（渲染组件、布局、diff）、绑定列表行花费的 JS 时间、产生的 mount item 数，以及存活的节点数。 |
| `turn: 2640µs (timers and tasks 1964µs)` | 一次带提交的轮次：总时间，以及提交前 JS 定时器和 Rust 组件任务花费的时间（Rust 组件在提交中渲染）。 |
| `first frame: eval 271µs, total 1235µs` | 执行 bundle 的时间，以及从启动到第一个批次的时间。 |
| `event Press on view 20` / `event js: 196µs` | 分发了一个事件，以及它的处理函数（及其引起的一切）在 JS 中花费的时间。 |

> [!NOTE]
> 无界面模式下文本使用近似的文本引擎测量，因此文本尺寸与设备上（使用 CoreText 或 Android 的文本布局）略有差异。布局
> 逻辑、视图数量和操作数量则完全相同。

单次运行的耗时很大程度上取决于 CPU 的频率状态（刚从空闲中唤醒的机器会比较慢）。请在同一台机器上比较多次运行，取多次
运行中的最小值，并且更多地相信操作数和 item 数，而不是微秒数。

## 滚动

```sh
nsh trace examples/gallery/Bench.tsx --scroll 3000 --taps 0
```

以每帧 10 点的速度滚动第一个 `<Scroll>` 或 `List`，并打印每一帧的批次：

```text
perf commit: 26 ops, 297µs (list js 111µs), 22 mount items, 233 nodes
── scroll to 20: 22 items
   Create { id: 448, kind: View }
   ...
   Text { id: 456, text: "Photo #25", content: 298x20 @ 0,0 }
```

对于[列表](./lists.md)，这能展示虚拟化的实际效果：哪些帧绑定了新行、一行需要多少操作，以及没有露出新行的帧不产生任何
内容。要得到有意义的耗时，请使用 release 构建的 CLI（在源码检出中为 `./target/release/nsh`）。

## 点击

`nsh trace` 点击首帧中 **第一个** 带 `onPress` 的视图。要设计便于 trace 的入口，可以把要测量的交互放在最前面，或者为它
单独写一个小的入口文件：

```tsx
// trace/AddItem.tsx: renders the screen and puts the button under test first
import { mount } from "nsh";
import { Cart } from "../src/Cart";

mount(() => <Cart initialItems={[]} />);
```

```sh
nsh trace trace/AddItem.tsx --taps 5
```

如果某次点击没有产生任何更新，`nsh trace` 会以 `error tap #1 produced no update` 停止，并返回非零退出码：它正是靠这一点
发现失效的处理函数，例如应用的节点被垃圾回收的情况。

## 在 CI 中

`nsh trace` 只需要 CLI，因此可以在 Linux runner 上运行。应用中的错误（bundle 执行失败、未捕获的异常）会打印为 `error`
行；构建失败、应用什么都没渲染，或者某次点击没有带来任何变化，都会使它以非零退出码退出。两者都要检查：

```sh
#!/bin/sh
set -e
for entry in App.tsx trace/*.tsx; do
  out=$(nsh trace "$entry" --idle-ms 300 2>&1) || { echo "$out"; exit 1; }
  if echo "$out" | grep -E '^ *error' >/dev/null; then
    echo "$out"
    echo "errors in $entry"
    exit 1
  fi
done
```

```yaml
# .github/workflows/trace.yml
jobs:
  trace:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: ./scripts/trace.sh
```

（在 runner 上安装 `nsh` CLI 的方式与本地安装相同；参见[安装](./installation.md)。）

要守住性能，可以记录几个交互的 item 数（`── … : N items` 行以及 `ops` / `mount items` 的数字），在它们增长时让 CI 失败。
这些计数是确定的；共享 CI 机器上的耗时则不是。

> [!NOTE]
> `nsh trace` 会应用构建产物中的应用 id 和 `nsh.json` 能力声明。`fetch` 会按网络授权实际访问网络；
> 进程和文件操作需要显式授权，与桌面宿主中的规则相同。

## 用于 UI 测试的 `testID`

`nsh trace` 检查的是运行时渲染出的内容。如果要用平台的测试工具（XCUITest、Espresso / UI Automator、Appium、Maestro）
驱动真实的原生 UI，请给视图设置 `testID`：

```tsx
<View testID="add-to-cart" onPress={add} style={{ padding: 12 }}>
  <Text testID="cart-count">{`${count()} items`}</Text>
</View>
```

| 平台 | `testID` 对应到哪里 |
|---|---|
| iOS | 视图的 `accessibilityIdentifier`（在 XCUITest 中为 `app.otherElements["add-to-cart"]`）。 |
| Android | 无障碍节点的 view id 资源名（在 UI Automator 中为 `By.res("add-to-cart")`）。 |

`testID` 是每个元素以及 `nsh/ui/mobile` 中各组件都有的 prop。它不会改变屏幕阅读器读出的内容；那应该用
`accessibilityLabel`（参见[无障碍](./accessibility.md)）。在 `nsh trace` 的输出中，它出现在视图的 `ViewProps` item 里，
形如 `test_id: Some("add-to-cart")`。

## 另请参阅

- [性能](./performance.md)：这些数字里该关注什么
- [开发服务器与调试](./dev-server-and-debugging.md)
- [架构](./architecture.md)：操作、提交和 mount 批次

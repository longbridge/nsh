---
title: 性能
description: NSH 如何让应用保持高速（原生运行时、批量跨线程、原生动画、虚拟化列表），这对你的代码意味着什么，以及如何用 nsh trace 测量。
order: 62
group: tools
---

# 性能

性能是 NSH 的第一优先级，而其中大部分由框架而非你的应用决定：运行时是原生的，每次更新只跨一次线程，动画和列表虚拟化
都无需 JS 参与。本页介绍 NSH 遵循的规则、它们对你所写代码的意义，以及如何测量。

## 规则

### 1. JS 做得越少越好

QuickJS 是一个解释器：体积小、启动快，但做重活时比原生代码慢得多。因此在 NSH 中，**只有你的应用代码是 JS**。响应式图
（signal、computed 值、effect）、编译后的 JSX 所调用的渲染器、布局引擎、`nsh/ui/mobile` 的组件、列表虚拟化、动画以及
你导入的每个模块都是 Rust。解析、diff、布局和字符串处理都在原生侧完成。

效果是可以测量的。同一个计数器应用分别跑在 NSH 早先的 JS 运行时和原生运行时上（release 构建）：

| | 早先的 JS 运行时 | 原生运行时 |
|---|---|---|
| bundle 大小 | 14 KB JS / 19.6 KB 字节码 | 886 B JS / 1.7 KB 字节码 |
| 执行 bundle | ~390 µs | ~95 µs |
| 一次点击的 JS 耗时（中位数） | ~14 µs | ~5 µs |
| 一次点击的 commit | ~22 µs | ~15 µs |
| 滚动 1000 行列表，每个重新绑定帧的 JS 耗时 | ~150 µs | ~77 µs |

组件也是如此：`nsh/ui/mobile` 中的 `Button`（Rust）与一个等价的 TSX 按钮对比，各 200 个：

| | Rust `Button`：JS / core | TSX 按钮：JS / core |
|---|---|---|
| 首帧（ops） | 1.32 ms / 0.77 ms (1015) | 2.32 ms / 0.70 ms (2415) |
| 更新全部 200 个标题（ops） | 0.42 ms / 0.42 ms (200) | 0.77 ms / 0.36 ms (600) |

（在 Apple silicon 上测得；第二张表基于早先的运行时。两者均出自 NSH 的性能基线。）

**对你的代码而言：** 让事件处理函数和 computed 值保持轻量。繁重的计算（解析大型数据、每次按键都对上千条数据排序）在
QuickJS 中的代价，比你在带 JIT 的 V8 或 JavaScriptCore 中习惯的要高。只做一次，用 `computed` 缓存，或者把它移出交互路径。

### 2. 批量跨线程

应用的 JS 运行在引擎线程上，从不在主线程上运行。一个 JS 轮次，例如一个事件处理函数及其引起的全部改动，会成为
**一次 commit**：运行时渲染组件、布局、diff，并生成一批原生视图操作，以**一次跳转**送达主线程。不存在每个视图一条的
bridge 消息，也没有每个 prop 一次的序列化。

**对你的代码而言：** 无需为了性能合并写入；一个处理函数里调用十次 `set` 仍然只产生一次 commit。如果你希望 effect 和
computed 值一次性看到所有写入，而不是每次写入后都运行一遍，请使用 `batch`。

### 3. 细粒度更新

组件只运行**一次**。之后，当某个 signal 变化时，只有读取它的绑定会重新运行：一段文本、一个样式属性、`<If>` 的一个分支、
`<Each>` 的各行。不会为了找出变化而重新渲染整棵子树。

```tsx
import type { Accessor } from "nsh";
import { Text } from "nsh";

type Quote = { price: number; change: number };

function Price(props: { quote: Accessor<Quote> }) {
  // Runs once. When quote() changes, only the text and the color update.
  return (
    <Text style={{ color: props.quote().change < 0 ? "#cf222e" : "#1a7f37" }}>
      {props.quote().price.toFixed(2)}
    </Text>
  );
}
```

**对你的代码而言：**

- 在使用 signal 的地方读取它（JSX 表达式、`computed`、`effect` 内部），而不是在组件顶部读取，这样更新的是范围最小的绑定。
- 集合使用 `<Each>`（按 key 保留行）或 [`List`](./lists.md)；JSX 中普通的 `.map()` 会在每次变化时重新创建所有行。
- 当多个绑定用到同一个派生值时，用 `computed` 计算它，这样每次变化只计算一次。

向一个三项列表追加第四项，`nsh trace` 显示的结果如下：

```tsx
{items().map((n) => <Text>{`row ${n}`}</Text>)}                   // 22 mount items: 3 views removed, 4 created
<Each of={items()}>{(n) => <Text>{`row ${n()}`}</Text>}</Each>      // 4 mount items: 1 view created
```

参见[响应式](./reactivity.md)。

### 4. 不发送不需要的内容

core 只发送变化的部分：新视图不带默认样式，也不带空的事件掩码；未变化的样式和 frame 不会发送；文本颜色的变化不会让文本
重新布局。一次只改变一个数字的点击只发送一个文本条目。你可以在 `nsh trace` 的输出中看到这一点（见下文）。

### 5. 原生执行动画

由 JS 驱动的动画（在 `requestAnimationFrame` 中写入 signal）每帧都要付出一次 JS 调用和一次 commit 的代价，而且引擎线程
繁忙时会卡顿。[`nsh/animated`](./animation.md) 则以原生方式运行动画：

```tsx
import { View } from "nsh";
import { createNativeValue } from "nsh/animated";
import { List } from "nsh/list";

const scrollY = createNativeValue(0);

<View
  style={{
    opacity: scrollY.interpolate({ inputRange: [0, 120], outputRange: [1, 0.35] }),
    transform: [{ translateY: scrollY.interpolate({ inputRange: [0, 120], outputRange: [0, -6] }) }],
  }}
/>;
<List each={photos()} scrollValue={scrollY} estimatedItemSize={64} style={{ flex: 1 }}>
  {(photo) => <Row photo={photo()} />}
</List>;
```

- `timing`、`spring` 和 `decay` 在平台的 UI 线程上逐帧运行，每帧不涉及 JS。
- `scrollValue` 在同一帧内把滚动偏移写入原生值；`pan={{ x, y }}` 让拖动直接移动视图。JS 只在结束时得到通知（promise
  resolve，`onPanEnd` 拿到速度）。
- `Animated.sequence` / `parallel` / `stagger` / `loop` 作为一棵原生树运行：每一步在上一步结束的瞬间开始，中间没有
  等待 JS 的间隙。
- `LayoutAnimation.configureNext` 以原生方式为下一次 commit 的 frame 变化、插入和删除添加动画。

### 6. 虚拟化长列表

[`List`](./lists.md) 只保留视口附近的行，复用它们的单元格，并在原生侧计算位置、多列布局、吸顶标题和可见性。没有露出新行
的滚动帧不会调用 JS，也不会产生 batch；在 gallery 的照片列表中，绑定一个新行的帧只有 4 个操作和 4 个 mount item。

**对你的代码而言：**

- `estimatedItemSize` 要接近真实的行高。
- 把每个条目的状态放在数据里（按 id 存放），而不是放在行内创建的 signal 中：单元格会被复用给其他条目，复用时
  `item()` / `index()` 会变化。
- 让行保持轻量：行在每次复用时绑定一次，所以它的 JS 是在用户滚动时运行的。
- 对跟随滚动位置的效果，使用 `scrollValue` 而不是 `onScroll`。`onScroll` 每个引擎轮次最多投递一个事件，但每个事件
  仍然是一次对 JS 的调用。

## 测量

### 无界面运行：`nsh trace`

```sh
nsh trace App.tsx                                   # first frame + 2 taps
nsh trace App.tsx --scroll 3000 --taps 0            # scrolling, 10 pt per frame
```

```text
perf commit: 56 ops, 395µs (list js 6µs), 49 mount items, 21 nodes
perf first frame: eval 271µs, total 1235µs
perf event js: 88µs
perf commit: 10 ops, 151µs (list js 2µs), 8 mount items, 27 nodes
```

需要关注的指标：

- **ops**（你的 JS 排入队列的操作）和 **mount items**（原生视图操作）：这两者是确定性的。如果一次本应只改变一个标签的
  点击产生了 40 个条目，说明有地方在重新创建本应更新的视图。
- **event js**：一个处理函数及其触发内容的 JS 开销。
- **commit**：core 渲染组件、布局和 diff 的开销。
- **list js**：该次 commit 中绑定列表行所花费的 JS 时间。

完整指南：[用 nsh trace 测试](./testing-and-trace.md)。

### 在设备上：`--trace`

```sh
nsh run App.tsx --trace
nsh run App.tsx --target android --trace --release
```

会在真实宿主上输出同样的 commit 日志行，使用的是平台自己的文本引擎。在嵌入式应用中，`NshAppView(trace: true)` 和
`new NshView(context, true)` 的作用相同。

### 在屏幕上：`--fps`

```sh
nsh run App.tsx --target macos --fps --release
```

会在运行中的应用上方显示帧率、每帧和每次 commit 的开销，以及 CPU、GPU 和内存（[macOS：性能监视器](./macos.md#性能监视器)）。

> [!TIP]
> 单次测量主要受 CPU 时钟状态影响：刚刚休眠过的线程会在一段时间内运行得更慢。请在同一台机器上比较多次运行的最小值，
> 计时时优先使用 release 构建。

## 参考数据

来自 NSH 自己的基线（除特别注明外均为 Apple silicon）：

| 场景 | 数据 |
|---|---|
| 计数器应用，无界面（debug） | 首帧 ~0.5–0.65 ms（58 ops / 49 mount items）；一次点击 ~0.15 ms |
| 计数器应用，Mac Catalyst（release） | 首帧 ~1.05–1.15 ms（其中约 0.7 ms 用于首次用 CoreText 布局 10 段文本）；一次点击 ~0.12 ms |
| Gallery 照片列表，无界面（debug） | 首帧 653 ops / 224 nodes，~2.7 ms |
| 滚动照片列表，Catalyst（release） | 慢速滚动时重新绑定一行 ~0.4 ms；快速甩动时一整屏约 32 行 0.9–1.6 ms |
| 滚动照片列表，Android 模拟器（arm64，release） | 重新绑定一行 ~0.3–0.4 ms（JS ~60 µs） |
| 引擎启动（release） | ~75–155 µs；全局 API（`fetch`、`URL` 等）在首次使用前没有任何开销，首次使用后 ~90 µs |
| 热重载 | 从保存文件到新版本运行 ~120 ms；应用更新 0.2–0.5 ms |

## 检查清单

- 交互：一次点击只改变少量 mount item，而不是几十个。
- 列表：长列表一律用 `List`，并设置 `estimatedItemSize`；每个条目的状态放在数据里。
- 动画：使用原生值，而不是每帧写入的 signal。
- 滚动效果：使用 `scrollValue`，而不是 `onScroll`。
- 处理函数：不在每次按键或每个滚动事件中做繁重工作；用 `computed` 缓存。
- 组件：合适时优先使用 `nsh/ui/mobile` 的 Rust 组件；它们比用 TSX 写的等价组件花费更少的 JS。

## 另请参阅

- [架构](./architecture.md)
- [用 nsh trace 测试](./testing-and-trace.md)
- [列表](./lists.md)、[动画](./animation.md)、[响应式](./reactivity.md)

---
title: 测试
description: 用 TypeScript 编写测试并用 nsh test 无界面运行：像用户一样打开应用、点击、输入、滚动和等待，模拟网络和存储；读懂 --trace 打印的原生视图操作和耗时；以及用于 UI 测试的 testID。
order: 61
group: tools
---

# 测试

`nsh test` 以 **无界面（headless）** 方式运行应用：使用真实的运行时（与设备上相同的 QuickJS VM、响应式图、渲染器、
组件、布局和列表虚拟化），但不需要模拟器、设备或窗口。只要 CLI 能运行的地方它都能运行，包括 Linux CI 机器。它做两件事：

- 运行用 TypeScript 编写的测试：每个测试打开应用，像用户一样操作（点击、输入、滚动），并检查屏幕上的内容；
- 加上 `--trace` 时，打印运行时发送给原生视图的每一个操作并附带耗时：首帧、一次点击或一帧滚动的开销。

## 用 nsh test 编写测试

测试文件是放在代码旁边的 `*.test.tsx` 或 `*.test.ts` 文件，它导入 `nsh/test`：

```tsx
// App.test.tsx
import { expect, open, test } from "nsh/test";

test("adds a todo", async () => {
  const app = await open("./App.tsx", { storage: { todos: "[]" } });
  await app.type("id:new-todo", "Buy milk");
  await app.press("text:Add");
  await app.waitFor("text:Buy milk");
  expect(app.texts()).toContain("1 item left");
});
```

```sh
nsh test                      # 当前目录中应用的所有测试文件
nsh test App.test.tsx         # 一个文件
nsh test --filter "adds"      # 名称包含 "adds" 的测试
```

```text
✓ App.test.tsx › adds a todo (84ms)
✗ App.test.tsx › keeps todos after a restart (312ms)
    expected ["Todos","0 items left"] to contain "1 item left"
    at <anonymous> (App.test.tsx:21:22)
    on screen: "Todos", "0 items left"
error 2 tests: 1 passed, 1 failed (0.9s)
```

失败的测试会显示抛出的内容、它在测试文件中的位置，以及所打开应用当时屏幕上的文字。只要有失败，`nsh test` 就以非零
退出码结束。`nsh init` 会随应用创建一个 `App.test.tsx`，`nsh check` 会连同其他代码一起对测试文件做类型检查
（`.nsh/types` 中声明了 `nsh/test`）。

每个测试文件在各自的引擎上运行，文件中的测试按顺序逐个运行。`open` 在另一个全新的引擎上启动应用，因此测试之间从不
共享状态；测试结束时，它打开的应用会被关闭。测试在抛出异常、返回的 promise 被 reject 或超过 30 秒时失败。带有自己
Rust crate 的应用（`nsh.json` 的 `"rust"`）以同样的方式，在 NSH 用这些 crate 构建的无界面运行器中运行测试；在 Rust 应用
（`nsh.json` 的 `"root"`，[用 Rust 开发](./rust.md)）中，`open(".")` 启动它的根组件。

### 打开应用

`open(entry, options?)` 构建 `entry` 处的应用（路径相对于测试文件；每次运行中每个入口只构建一次），以 390×844 的视口
启动它，并在它完成渲染且空闲后 resolve。

| 选项 | 默认值 | 说明 |
|---|---|---|
| `storage` | 空 | `nsh/storage` 初始保存的内容：`{ todos: "[]" }`。 |
| `network` | 无 | 应用的请求得到的响应，按 `"<METHOD> <URL>"` 给出（只写 URL 表示 `GET`）：`{ "GET https://api.example.com/todos": { status: 200, body: [] } }`。`body` 是字符串，或者作为 JSON 发送的值。未列出的请求会失败，因此测试不会在未经允许时访问网络。`"live"`：使用真实网络。 |
| `width`、`height` | 390、844 | 视口大小，单位为点。 |
| `safeArea` | 0 | 安全区域内边距，单位为点：`{ top: 47, bottom: 34 }`。 |
| `clipboard` | 空 | 剪贴板初始的文字。 |

### 操作步骤

步骤用 **选择器** 指定视图：`id:<testID>`、`label:<accessibilityLabel>`、`text:<text>`、`view:<id>`（来自
`--trace` 输出的视图 id），或者一个裸字符串，依次按 `testID`、标签、文字匹配。选择器找到的是用户最先会看到的视图：先在
最上层的模态窗口中找，再在屏幕上找；对于导航栈只看最上面的页面。

| 步骤 | 说明 |
|---|---|
| `await app.press(selector)` | 按下该视图，或者它外层最近的可按压视图（文字外面的按钮）：依次触发 `onPressDown`、`onPressUp`、`onPress`。 |
| `await app.type(selector, text)` | 在输入框（或该视图内的第一个输入框；`text:` 也匹配输入框的内容或占位文字）已有内容的末尾逐键输入：输入框先获得焦点，然后每个字符触发一次 `onKeyPress` 和一次 `onChange`，不超过 `maxLength`。单行输入框中的 `\n` 相当于回车键。 |
| `await app.submit(selector)` | 在输入框中按回车键，行为由它的 `submitBehavior` 决定：触发 `onSubmit`（单行输入框随后失去焦点），或者在多行输入框中换行。 |
| `await app.scroll(selector, distance)` | 沿滚动视图或列表（该视图、它外层的滚动视图，或它内部的第一个）的方向拖动 `distance` 点，每帧 10 点，不超出内容范围。负数表示往回滚。 |
| `await app.wait(ms)` | 让应用运行 `ms` 毫秒：定时器、动画、请求。 |
| `await app.waitFor(selector, timeout?)` | 有视图匹配时 resolve；`timeout` 毫秒（5000）后 reject。 |
| `app.shows(selector)` | 当前是否有视图匹配。 |
| `app.texts()` | 当前屏幕上的文字，按顺序排列。 |
| `app.value(selector)` | 输入框的内容，没有匹配时为 `null`。 |
| `app.storage()` | 应用存储当前保存的内容。 |
| `await app.back()` | 像用户那样返回（导航栏的返回按钮、下滑关闭弹出页）：最新的模态窗口里（没有模态窗口时就在整个屏幕上）最内层的导航栈弹出栈顶页面（`onDismiss`），栈顶页面设置了 `preventBack` 时改为询问它（`onBackRequest`）；没有可返回的栈时，最新的模态窗口请求关闭（`onRequestClose`）。没有可返回的内容时 reject。 |
| `app.onDialog(handler)` | 回答应用之后弹出的对话框（`Alert.alert`、操作表）：`handler` 接收对话框（`title`、`message`、`buttons`、`cancelable`、`sheet`），返回要按的按钮文字，返回 `null` 则不按按钮直接关闭。 |
| `app.dialogs()` | 应用弹出过的对话框，按顺序排列。 |
| `app.clipboard()` | 剪贴板当前的文字，没有时为 `null`。 |

步骤会等到应用连续 100 ms 没有渲染、并且没有正在运行的原生动画（对一直在渲染的应用最多等 2 s）后才 resolve，所以下一行
代码运行时，这一步带来的变化已经在屏幕上；之后才出现的内容（响应、定时器）用 `waitFor` 等待。选择器没有匹配到任何视图
（错误信息会列出屏幕上的文字）、应用在此期间抛出了异常，或者应用请求了只有真实平台才能回答的内容时，步骤会 reject，
测试随之失败。

### 无界面运行替平台做了什么

无界面运行有真实的运行时，但没有平台。它只扮演 NSH 协议规定的平台角色，不模拟只有真实平台才能做的事：

- **原生动画会运行**，逐帧跑到结束：`start(callback)` 的回调会执行，等待动画结束的逻辑也会继续。
- **返回**会像平台那样弹出页面、关闭模态窗口（`app.back()`）。
- **对话框会被回答**：由测试的 `onDialog` 回答，否则按它的取消按钮，否则在可以关闭时直接关闭；只能用其他按钮回答的对话框，
  会让弹出它的步骤失败，直到测试指定要按哪个按钮。
- **剪贴板保存文字**：应用复制的内容，粘贴时可以读回来（用 `clipboard` 设定初始内容）。
- **模态窗口立即出现**：弹出时就会触发 `onShow`。
- **图片不会加载**：不会有 `onLoad` 或 `onError`。
- **只有真实平台才能回答的请求会让步骤失败**：图片尺寸（`Image.getSize`、`Image.prefetch`）、剪贴板里的图片或文件。

### 断言

`expect(value)` 按 JSON 的方式比较值（`undefined` 视为 `null`，数字按数值比较），每个检查不成立时都会抛出异常。
`.not` 把检查反过来：`expect(app.texts()).not.toContain("Error")`。

| 检查 | 成立条件 |
|---|---|
| `toBe(expected)`、`toEqual(expected)` | 值逐个成员地等于 `expected`。 |
| `toContain(item)` | 数组包含 `item`，或者字符串包含它。 |
| `toBeTruthy()`、`toBeFalsy()` | 按 JS 的规则，值为真 / 为假。 |

### 从标准输入给出步骤

如果传入的是应用的入口而不是测试，`nsh test` 会在真实网络下打开应用，让它运行 1.2 s，然后执行标准输入给出的步骤，
其中 `app` 就是这个应用：

```sh
echo 'await app.press("text:+1"); expect(app.texts()).toContain("1");' | nsh test App.tsx
```

这 1.2 s 让引擎的空闲垃圾回收在第一步之前运行，这样，如果应用丢掉了它仍在使用的对象，问题就会暴露出来：它的处理函数
不再生效。

示例中有可以参考的测试：TSX 应用见 `examples/counter` 和 `examples/gallery/tests`（存储、输入框、长列表、模拟请求），带
Rust crate 的应用见 `examples/rust-exports`，Rust 应用见 `examples/gallery-rs`。

## 用 --trace 跟踪

`--trace` 会打印测试所打开应用的每个挂载批次，标注它所属的步骤，以及每次提交的耗时。可以对测试文件使用，也可以对应用
入口使用，并从标准输入给出步骤：

```sh
nsh test App.test.tsx --trace
nsh test App.tsx --trace                                   # 首帧，然后空闲 1.2 s
echo 'await app.press("Add")' | nsh test App.tsx --trace    # 再加一次点击
```

它的用途：

- 检查应用（或某个 demo）能否无错误地构建、执行和渲染；
- 准确查看一次改动创建、移动或更新了哪些原生视图，以及数量；
- 测量首帧、一次点击以及每一帧滚动的开销；
- 在 CI 中发现回归。

### 读懂输出

下面是计数器示例按一次按钮的输出（`echo 'await app.press("+1")' | nsh test examples/counter/App.tsx --trace`），
有删节：

```text
✓ bundled App.tsx (3255 B in 6.2 ms)
perf commit: 56 ops, 271µs (list js 5µs), 49 mount items, 21 nodes
perf first frame: eval 187µs, total 820µs
── open(App.tsx): 49 items
   LayoutDirection { rtl: false }
   Create { id: 2, kind: View }
   Create { id: 4, kind: Text }
   Insert { parent: 2, child: 4, before: None }
   ...
   Frame { id: 2, x: 0, y: 0, w: 390, h: 844 }
   Frame { id: 4, x: 24, y: 64, w: 342, h: 34 }
   ...
   Text { id: 4, text: "nsh counter", content: 342x34 @ 0,0 }
   Text { id: 14, text: "0", content: 36x77 @ 0,0 }
perf event Press on view 26
perf event js: 413µs
perf commit: 10 ops, 672µs (list js 7µs), 7 mount items, 24 nodes
── press(+1): 7 items
   Create { id: 42, kind: Text }
   Insert { parent: 32, child: 42, before: None }
   Frame { id: 32, x: 24, y: 366, w: 342, h: 43 }
   Frame { id: 42, x: 0, y: 23, w: 342, h: 20 }
   Text { id: 14, text: "1", content: 36x77 @ 0,0 }
   Text { id: 42, text: "• 1", content: 342x20 @ 0,0 }
   Text { id: 34, text: "Recent values: 1", content: 342x17 @ 0,0 }
✓ App.tsx › steps (1.4s)
```

#### 批次与 mount item

每个 `── step: N items` 块是一个 **mount 批次（mount batch）**，标注它所属的步骤：平台在一帧内应用到原生视图上的全部内容，一次性切换到
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

#### `perf` 日志行

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

### 滚动

```sh
echo 'await app.scroll("id:photos", 3000)' | nsh test examples/gallery/Bench.tsx --trace
```

`scroll` 以每秒 60 帧、每帧 10 点的速度滚动列表，并打印每一帧的批次：

```text
perf commit: 55 ops, 1667µs (list js 771µs), 46 mount items, 414 nodes
── scroll(id:photos, 3000): 46 items
   Create { id: 792, kind: View }
   Insert { parent: 94, child: 792, before: None }
   ...
```

对于[列表](./lists.md)，这能展示虚拟化的实际效果：哪些帧绑定了新行、一行需要多少操作，以及没有露出新行的帧不产生任何
内容。要得到有意义的耗时，请使用 release 构建的 CLI（在源码检出中为 `./target/release/nsh`）。

### 在 CI 中

`nsh test` 只需要 CLI，因此可以在 Linux runner 上运行；只要有测试失败，它就以非零退出码退出：测试本身失败、应用无法构建
或什么都没渲染、应用在某一步中抛出异常，都算失败。

```yaml
# .github/workflows/test.yml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: nsh test
```

（在 runner 上安装 `nsh` CLI 的方式与本地安装相同；参见[安装](./installation.md)。）

要守住性能，可以记录几个交互的 item 数（`--trace` 输出中的 `── … : N items` 行以及 `ops` / `mount items` 的数字），在它们
增长时让 CI 失败。这些计数是确定的；共享 CI 机器上的耗时则不是。

> [!NOTE]
> 测试以全部能力运行：不应用 `nsh.json` 中的 `capabilities`。能力规则请在设备上或打包后的构建中测试。

## 用于 UI 测试的 `testID`

`nsh test` 检查的是运行时渲染出的内容，它的 `id:` 选择器按 `testID` 查找视图。如果要用平台的测试工具（XCUITest、Espresso / UI Automator、Appium、Maestro）
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

`testID` 是每个元素以及每个 Rust 组件（`SafeAreaView`、`KeyboardAvoidingView` 等）都有的 prop。它不会改变屏幕阅读器读出的内容；那应该用
`accessibilityLabel`（参见[无障碍](./accessibility.md)）。在 `--trace` 的输出中，它出现在视图的 `ViewProps` item 里，
形如 `test_id: Some("add-to-cart")`。

## 另请参阅

- [性能](./performance.md)：这些数字里该关注什么
- [开发服务器与调试](./dev-server-and-debugging.md)
- [架构](./architecture.md)：操作、提交和 mount 批次

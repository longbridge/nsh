---
title: 元素
description: 构成每个 NSH 应用的宿主元素，以及它们共有的属性、事件和 ref 方法。
order: 1
---

# 元素

宿主元素是构成每个界面的原语：`<View>`、`<Text>`、`<Image>` 等，从 NSH 的模块导入。每个元素都对应一个原生视图（iOS 和 Mac Catalyst 上是
UIView 子类，Android 上是 `View`），由 NSH 运行时创建并布局。应用显示的一切都由它们组成：
[`nsh/ui/mobile`](../components/index.md) 的组件同样是用这些元素绘制的。

```tsx
import { mount, signal, Text, View } from "nsh";

function App() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <Text style={{ fontSize: 24 }}>已点击 {count()} 次</Text>
      <View onPress={() => count.update((n) => n + 1)} activeOpacity={0.6} class="rounded-lg bg-blue-600 px-4 py-2">
        <Text class="text-white">点我</Text>
      </View>
    </View>
  );
}

mount(App);
```

每个标签都使用 PascalCase，并指向作用域中的绑定：按下表从各自的模块导入每个元素。只有 `<If>` 和 `<Each>` 是无需导入的全局组件。`<view>` 这样的小写标签会导致构建错误，因为 TypeScript 只能对照每个名称一个的全局类型来检查它，`List`、`Each` 这类泛型组件就无法推断条目类型。编译器通过导入识别这些元素并就地创建原生视图，所以导入它们在运行时没有任何开销。参见 [JSX 与组件](../docs/jsx-and-components.md)。

## 全部元素

| 标签 | 导入自 | 原生视图（iOS / Android） | 页面 |
|---|---|---|---|
| `<View>` | `nsh` | UIView / ViewGroup | [View](./view.md) |
| `<Text>` | `nsh` | CoreText 视图 / StaticLayout 视图 | [Text](./text.md) |
| `<Image>` | `nsh/image` | 带图片图层的视图 / 图片视图 | [Image](./image.md) |
| `<Scroll>` | `nsh` | UIScrollView / ScrollView | [Scroll](./scroll.md) |
| `<Input>` | `nsh` | UITextField 或 UITextView / EditText | [Input](./input.md) |
| `<List>` | `nsh/list` | 复用单元格的 UIScrollView / ScrollView | [List](./list.md) |
| `<Canvas>` | `nsh/canvas` | 回放 2D 绘制的视图 | [Canvas](./canvas.md) |
| `<WebView>` | `nsh/webview` | WKWebView / android.webkit.WebView | [WebView](./web-view.md) |
| `<Video>` | `nsh/video` | AVPlayerLayer / TextureView 上的 MediaPlayer | [Video](./video.md) |
| `<Modal>` | `nsh/ui/mobile` | 模态呈现的 view controller / Dialog | [Modal](./modal.md) |
| `<InputAccessoryView>` | `nsh/ios` | 键盘上方的工具栏（仅 iOS） | [nsh/ios](../modules/nsh-ios.md) |
| `<If>`、`<Each>` | 无：全局组件 | 无：控制流 | [控制流](./control-flow.md) |

> [!NOTE]
> 字符串只有放在 `<Text>` 里才会被绘制。`<View>` 的字符串子节点不会创建原生视图，也不参与布局。

## 通用属性

所有宿主元素都接受下列属性（即 `CommonProps` 类型）。各元素页面只列出该元素自己的属性。

### 样式

| 属性 | 类型 | 说明 |
|---|---|---|
| `style` | `StyleProp<ViewStyle>`（`<Text>` 和 `<Input>` 为 `TextStyle`，`<Image>` 为 `ImageStyle`） | 内联样式，或从左到右合并的样式数组（假值项会被跳过；数组可以嵌套，所以 `style={[base, props.style]}` 中的 `props.style` 本身也可以是数组）。优先级高于 `class`。参见[样式参考](../modules/style-reference.md)。 |
| `class` | `string` | Tailwind 类名，在原生侧解析（`"flex-row gap-2 p-4 dark:bg-zinc-900"`）。后面的类覆盖前面的类。参见 [Tailwind](../docs/tailwind.md)。 |
| `classList` | `Record<string, boolean \| null \| undefined>` | 值为真时才应用的类（`{ "opacity-50": busy() }`）。 |
| `disabled` | `boolean` | 忽略按压（也没有按压反馈），应用 `disabled:` 类，并禁用输入框等控件。 |

### 按压与反馈

按压处理在原生侧完成：由平台识别按压并显示反馈，JS 只在事后收到通知。只有最内层可按压的视图会响应；在滚动视图内，按下会等待约 130 ms，让滚动有机会先开始。

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `onPress` | `() => void` | | 在视图内完成一次点击。 |
| `onLongPress` | `() => void` | | 手指按住达到 `delayLongPress`。 |
| `onPressIn` | `() => void` | | 手指在视图上按下。 |
| `onPressOut` | `() => void` | | 手指抬起或移出。 |
| `delayLongPress` | `number` | `500` | 按压变为长按前的毫秒数。 |
| `activeOpacity` | `number` | | 按下时视图的不透明度。 |
| `underlayColor` | `ColorValue` | | 按下时的背景色；子元素淡化到 `underlayOpacity`。 |
| `underlayOpacity` | `number` | `0.85` | 显示 `underlayColor` 时子元素的不透明度。 |
| `hitSlop` | `number \| { top?, right?, bottom?, left? }` | | 把触摸区域扩展到边界之外，单位为点。 |
| `pointerEvents` | `"auto" \| "none" \| "box-none" \| "box-only"` | `"auto"` | 视图及其子元素接收哪些触摸：`"none"` 都不接收，`"box-none"` 只有子元素接收，`"box-only"` 只有视图本身接收。 |

视图被按下时，`active:` Tailwind 类同样会生效，无需经过 JS。

### 悬停

JS 可以得知鼠标移到视图上、在视图上移动和离开视图：macOS、Windows、Linux、GPUI 后端和桌面浏览器都支持；iOS 上是 iPad 的指针或 Mac 的鼠标，Android 上是鼠标或手写笔。`hover:` 类在同样的地方为悬停的视图设置样式。

| 属性 | 类型 | 说明 |
|---|---|---|
| `onHoverIn` | `() => void` | 鼠标移到视图或其内部的视图上。 |
| `onHoverMove` | `(e: HoverEvent) => void` | 没有按下按键时，鼠标在视图（或其内部的视图）上移动。 |
| `onHoverOut` | `() => void` | 鼠标离开视图及其内部的视图，或离开窗口。 |

与 CSS 的 `:hover` 一样，鼠标在视图上或在其内部任何内容上时，视图都算作被悬停，因此鼠标下方视图外层的视图也会收到 `onHoverIn`（由外向内；`onHoverOut` 由内向外）。事件在发生之后才通知 JS，触摸从不触发；按着按键移动属于触摸（`onTouchMove`、响应者系统）。从鼠标下方移除的视图会收到 `onHoverOut`，在 Linux 和 Windows 上要等到鼠标下一次移动。

`HoverEvent` 给出鼠标的位置：`locationX` / `locationY` 是在视图中的位置（单位为点，已去掉视图的变换），`pageX` / `pageY` 是在视图所在根视图中的位置（与触摸相同），以及按住的修饰键（`altKey`、`ctrlKey`、`metaKey`：Apple 平台上是 Command，其他平台是 Windows 键，`shiftKey`）。移动事件每帧最多一次，总是最新的那次，并且一定在随后的 `onHoverOut` 之前到达，因此处理函数可以在每次移动时重绘：

```tsx
const hovered = signal<string | null>(null);

<View onHoverIn={() => hovered.set(file.path)} onHoverOut={() => hovered.set(null)} class="hover:bg-zinc-100">
  <Text>{file.name}</Text>
</View>

// 图表上的十字线。
const at = signal<number | null>(null);

<View onHoverMove={(e) => at.set(e.locationX)} onHoverOut={() => at.set(null)} style={{ height: 200 }}>
  <If when={at() !== null}>
    <View style={{ position: "absolute", left: at()!, top: 0, bottom: 0, width: 1, backgroundColor: "#888" }} />
  </If>
</View>
```

### 上下文菜单

`onContextMenu` 告诉视图：辅助键在它上面按下了——右键单击、macOS 上的 Control 单击、触控板的双指单击、iPad 指针的辅助点按；在浏览器里还有键盘的菜单键。在这里用 [`Menu.popup`](../modules/nsh-desktop.md) 打开应用自己的菜单：

```tsx
import { Menu } from "nsh/desktop";

<View onContextMenu={(e) => Menu.popup(rowMenu(row), { x: e.pageX, y: e.pageY })}>…</View>
```

| 属性 | 类型 | 说明 |
|---|---|---|
| `onContextMenu` | `(e: ContextMenuEvent) => void` | 辅助键在视图或其内部的视图上按下。 |

`ContextMenuEvent` 包含视图中的 `locationX` / `locationY`、根视图中的 `pageX` / `pageY`（`Menu.popup` 放置菜单的位置）以及修饰键（`altKey`、`ctrlKey`、`metaKey`、`shiftKey`）。从指针下方的视图向外，最近的一个监听者收到事件。如果比它更近的是文本输入框，或带有编辑菜单的可选中文本（Windows、GPUI 后端、浏览器），则显示它们自己的菜单；周围没有监听者时什么也不会发生。平台在按键按下时根据已挂载的内容做出决定，因此处理函数在事后得到通知，不需要取消任何东西。在触摸屏上用长按打开菜单：用同一个函数处理 `onLongPress`。

### 键盘焦点

在 macOS 和 Web 上，Tab 在可按压的视图（以及输入框）之间移动键盘焦点，按 Return 或空格会按下获得焦点的视图，`focus:` 类为它设置样式。
点击会按下视图但不让它获得焦点，所以 `focus:` 只在键盘聚焦时生效。

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `focusable` | `boolean` | `true` | 可按压的视图是否是 Tab 的停靠点；`false` 把它留给指针（关闭浮层的背景层、用方向键切换的列表行）。 |

```tsx
<View class="rounded-lg p-3 focus:bg-sky-100" onPress={open}>
  <Text>Open</Text>
</View>

{/* 浮层的背景层：点击关闭浮层，Tab 直接越过它进入内容。 */}
<View class="absolute inset-0" focusable={false} onPress={close} />
```

### 布局与标识

| 属性 | 类型 | 说明 |
|---|---|---|
| `onLayout` | `(e: LayoutEvent) => void` | 每次布局改变视图位置尺寸后，给出相对父视图的 frame `{ x, y, width, height }`。 |
| `ref` | `YNode \| (node: YNode) => void` | 原生节点（参见 [ref 方法](#ref-方法)）。`ref={el}` 赋值给变量，`ref={fn}` 调用函数。 |
| `nativeID` | `string` | 为视图命名，供其他元素引用，例如输入框引用的 `InputAccessoryView`。 |
| `testID` | `string` | UI 测试使用的标识（在 Android 上是视图的 resource id）。 |

### 驱动动画值的手势

这些属性把手势绑定到[原生动画值](../modules/nsh-animated.md)，值在 UI 线程上变化，每帧都不经过 JS。参见[触摸与手势](../docs/touch-and-gestures.md)。

| 属性 | 类型 | 说明 |
|---|---|---|
| `pan` | `{ x?: AnimatedValue; y?: AnimatedValue }` | 拖动驱动这些值（单位为点）。 |
| `onPanStart` | `(e: PanEvent) => void` | 拖动开始。 |
| `onPanEnd` | `(e: PanEvent) => void` | 拖动结束，带有松手时的速度（例如用来启动 `decay`）。 |
| `pinch` | `{ scale?: AnimatedValue }` | 双指捏合驱动 `scale`。 |
| `onPinchStart` / `onPinchEnd` | `(e: { scale: number; velocity: number }) => void` | 捏合开始 / 结束。 |
| `rotation` | `{ angle?: AnimatedValue }` | 双指旋转驱动 `angle`（角度）。 |
| `onRotationStart` / `onRotationEnd` | `(e: { rotation: number; velocity: number }) => void` | 旋转开始 / 结束。 |

`PanEvent` 为 `{ translationX, translationY, velocityX, velocityY, valueX?, valueY? }`：自开始以来的位移、以点每秒计的速度，以及此刻所绑定值的数值（拖动可能在动画中途抓住一个值）。

### 原始触摸与响应者系统

对于上面属性覆盖不到的手势，每个元素都支持触摸与响应者属性：
`onTouchStart`、`onTouchMove`、`onTouchEnd`、`onTouchCancel`（以及对应的 `…Capture` 形式）、
`onStartShouldSetResponder`、`onMoveShouldSetResponder`（以及 `…Capture`）、`onResponderGrant`、`onResponderReject`、
`onResponderStart`、`onResponderMove`、`onResponderEnd`、`onResponderRelease`、`onResponderTerminate` 和
`onResponderTerminationRequest`。每个处理函数都会收到一个 `GestureResponderEvent`
（`{ identifier, locationX, locationY, pageX, pageY, target, timestamp, touches, changedTouches, nativeEvent,
stopPropagation() }`）；`…ShouldSetResponder` 回调还会收到 `gestureState`（`dx`、`dy`、`vx`、`vy` 等）。
`onResponderGrant` 返回 `true` 可阻止原生滚动视图接管触摸。`panActivation`
（`{ activeOffsetX, activeOffsetY, failOffsetX, failOffsetY }`）让视图只在平台激活它的拖动后才成为响应者，
滚动视图里的拖动因此把另一个轴留给滚动视图（[触摸与手势](../docs/touch-and-gestures.md#滚动视图里的拖动panactivation)）。
[`nsh/gesture`](../modules/nsh-gesture.md) 中的 `PanResponder` 就构建在它们之上。

### 无障碍

| 属性 | 类型 | 说明 |
|---|---|---|
| `accessible` | `boolean` | 视图对辅助技术而言是一个整体元素，子元素不再单独暴露。 |
| `accessibilityLabel` | `string` | 屏幕阅读器朗读的内容。没有标签的视图会朗读其中的文本。 |
| `accessibilityHint` | `string` | 激活该元素会发生什么。 |
| `accessibilityRole` / `role` | `AccessibilityRole` | 元素是什么（`"button"`、`"header"`、`"link"`、`"switch"` 等）。 |
| `accessibilityState` | `{ disabled?, selected?, checked?, busy?, expanded? }` | 辅助技术报告的状态（`checked` 可以是 `"mixed"`）。 |
| `accessibilityValue` | `{ min?, max?, now?, text? }` | 范围类或文本类元素的值。 |
| `accessibilityElementsHidden` | `boolean` | 对辅助技术隐藏该视图及其子树。 |
| `importantForAccessibility` | `"auto" \| "yes" \| "no" \| "no-hide-descendants"` | 是否暴露该视图。 |
| `accessibilityActions` | `{ name: string; label?: string }[]` | 提供给辅助技术的自定义操作。 |
| `onAccessibilityAction` | `(e: { actionName: string }) => void` | 用户触发了 `accessibilityActions` 中的某个操作。 |

参见[无障碍](../docs/accessibility.md)。

### 平台专属属性

只在某个平台上存在的属性放在以该平台命名的对象里，这样共享属性在各平台上保持一致。只有当前平台的对象会被发送，其他的会被忽略。

| 属性 | 说明 |
|---|---|
| `ios={{…}}` | iOS 和 Mac Catalyst。`<Input>`、`Modal` 和 `WebView` 有各自的键。 |
| `android={{…}}` | Android。所有元素都支持 `ripple`、`elevation` 和 `accessibilityLiveRegion`（见下表）。 |
| `desktop={{…}}` | 桌面平台和桌面浏览器；在移动端被忽略。所有元素都支持 `windowDrag`（见下表）。 |
| `web={{…}}` | 浏览器；在其他平台被忽略。 |

| `android` 键 | 类型 | 说明 |
|---|---|---|
| `ripple` | `{ color?, borderless?, radius?, foreground? }` | 按下时的 Material 水波纹。 |
| `elevation` | `number` | 根据视图形状生成的 Material 阴影（单位为点）。 |
| `accessibilityLiveRegion` | `"none" \| "polite" \| "assertive"` | 视图变化如何播报给 TalkBack。 |

| `desktop` 键 | 类型 | 说明 |
|---|---|---|
| `windowDrag` | `boolean` | 按住视图拖动窗口，像按住标题栏一样；`false` 让可拖动视图中的一部分保持可点击。见[自定义标题栏](../modules/nsh-desktop.md#自定义标题栏)。 |

```tsx
<View onPress={save} android={{ ripple: { color: "#00000022" }, elevation: 2 }} class="rounded-lg bg-white p-4">
  <Text>保存</Text>
</View>
```

参见[平台相关代码](../docs/platform-specific.md)。

## ref 方法

`ref` 收到的是一个 `YNode`。它的方法向原生视图发送命令，与同一轮中先前的改动按顺序执行。各元素页面会说明哪些方法适用。

| 方法 | 适用于 | 说明 |
|---|---|---|
| `focus()` / `blur()` | `<Input>` | 获取焦点（弹出键盘）/ 失去焦点。 |
| `setSelection(start, end?)` | `<Input>` | 选中 `start..end`（UTF-16 单位）；省略 `end` 时为光标。 |
| `scrollTo({ x?, y?, animated? })` | `<Scroll>`、`List` | 滚动到某个偏移量。 |
| `zoomTo({ scale, x?, y?, animated? })` | 可缩放的 `<Scroll>` | 围绕内容中的某点缩放到 `scale`。 |
| `scrollToOffset({ offset, animated? })` | `List` | 沿列表方向滚动到某个偏移量。 |
| `scrollToIndex({ index, animated?, viewPosition?, viewOffset? })` | `List` | 把某一项滚动到可见区域。 |
| `scrollToEnd({ animated? })` | `List` | 滚动到内容末尾。 |
| `reload()`、`goBack()`、`goForward()`、`stopLoading()`、`injectJavaScript(script)`、`postMessage(data)` | `WebView` | 控制页面。 |
| `seek(seconds)` | `Video` | 跳转到某个时间点。 |
| `command(name, args?)` | 原生组件 | 调用已注册原生组件的命令（[`nsh/native`](../modules/nsh-native.md)）。 |

```tsx
import { Input, View, type YNode } from "nsh";

let input!: YNode;
<View onPress={() => input.focus()}>
  <Input ref={input} placeholder="点击这一行开始输入" />
</View>;
```

## 为什么处理函数不会阻塞 UI

主线程从不等待 JS。当系统需要立即得到答复时（这段文本能否修改、返回键能否返回上一页、这个 sheet 能否关闭），NSH 会根据事先设置好的属性（`maxLength`、`preventBack`、`dismissible`、`pointerEvents` 等）在原生侧作答，事件随后才送达 JS。正因如此，这类行为都是属性，而不是处理函数的返回值。

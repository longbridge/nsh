---
title: 触摸与手势
description: 按压与按压反馈、点击区域、pointerEvents、原始触摸事件、手势响应者系统、PanResponder，以及原生的拖动、捏合与旋转手势。
order: 38
group: ui
demo: guide-gestures
---

# 触摸与手势

NSH 的触摸处理分三个层次。**按压**（`onPress` 等）用于按钮、列表行和链接，按压反馈由原生绘制。**原生手势**（`pan`、`pinch`、`rotation`）在 UI 线程上驱动[原生值](./animation.md#原生值)，即使 JS 正忙，被拖动的卡片也能紧跟手指。**原始触摸与手势响应者系统**（`onTouchStart`、`onResponderMove`、`PanResponder`）把每根手指的位置都交给 JS，用来实现前两者表达不了的交互，例如手写绘图。

选择能完成任务的最高层次：层次越高，从 UI 线程传到 JS 的工作就越少。

## 按压

所有元素都接受按压事件，不需要单独的 "touchable" 组件。把 `onPress` 加在 `<View>`、`<Text>`、`<Image>` 或任何其他元素上即可。

```tsx
import { mount, signal, Text, View } from "nsh";

function App() {
  const count = signal(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
      <View
        onPress={() => count.update((n) => n + 1)}
        onLongPress={() => count.set(0)}
        activeOpacity={0.6}
        accessibilityRole="button"
        style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10, backgroundColor: "#0969da" }}
      >
        <Text style={{ color: "#ffffff", fontWeight: "600" }}>Pressed {count()} times</Text>
      </View>
    </View>
  );
}

mount(App);
```

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `onPress` | `() => void` | | 手指在视图内抬起（普通按压，不是长按）。 |
| `onLongPress` | `() => void` | | 手指按住达到 `delayLongPress`。之后不会再触发 `onPress`。 |
| `onPressIn` | `() => void` | | 按压开始。 |
| `onPressOut` | `() => void` | | 按压结束：抬起、滑出，或被滚动视图接管。 |
| `delayLongPress` | `number` | `500` | 按压变为长按前的毫秒数。 |
| `disabled` | `boolean` | `false` | 不接受按压，也不显示反馈；应用 `disabled:` 类。 |

按压的行为：

- **只有最内层的可按压视图响应。** 可按压的行里包含一个可按压的按钮时，按下按钮不会触发行的按压。
- **在滚动视图内，按压会等待约 130 ms** 才开始（并显示反馈），这样从按钮上开始的拖动会滚动页面，而不是按下按钮。
- **滑出即取消。** 手指移出视图时，按压以 `onPressOut` 结束，不触发 `onPress`。
- **`disabled` 会把按压监听从挂载的视图上移除**，因此完全没有反馈，同时应用 `disabled:` 类（`class="disabled:opacity-40"`）。
- **带 `onPress` 的嵌套 `<Text>` 片段就是链接。** 整段文字是一个原生视图，按压会交给手指下的那个片段。见[文本](./text.md)。

## 按压反馈

按压反馈在手指按下的瞬间由平台绘制，不需要往返 JS。每个视图选一种：

| 属性 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `activeOpacity` | `number` | | 按下时视图的不透明度。 |
| `underlayColor` | `ColorValue` | | 按下时的背景色；子视图淡到 `underlayOpacity`。 |
| `underlayOpacity` | `number` | `0.85` | 显示 `underlayColor` 时子视图的不透明度。 |
| `android={{ ripple }}` | `{ color?, borderless?, radius?, foreground? }` | | Android 的 Material 水波纹。iOS 上忽略。 |

```tsx
<View onPress={open} activeOpacity={0.5}>…</View>

<View onPress={open} underlayColor="#ddf4ff" style={{ padding: 16 }}>…</View>

<View
  onPress={open}
  activeOpacity={0.6}
  android={{ ripple: { color: "rgba(9,105,218,0.3)" } }}
>
  …
</View>
```

无边界水波纹会扩散到视图之外，适合图标按钮：`android={{ ripple: { borderless: true, radius: 24 } }}`。`activeOpacity` 与水波纹常常一起使用：iOS 用不透明度，Android 用水波纹。见[平台特定代码](./platform-specific.md)。

第三种选择是 Tailwind 的 `active:` 变体。它在视图被按下时应用对应的类，配合 `transition` 类时变化会有动画：

```tsx
<View
  onPress={save}
  class="rounded-xl bg-emerald-600 p-4 transition active:scale-95 active:bg-emerald-800"
>
  <Text class="font-semibold text-white">Save</Text>
</View>
```

## 点击区域：hitSlop

`hitSlop` 把接收触摸的区域扩展到视图边界之外，而不改变布局。适合关闭按钮、开关之类的小目标。

```tsx
<View hitSlop={12} onPress={close} style={{ width: 20, height: 20 }}>…</View>
<View hitSlop={{ top: 10, bottom: 10, left: 4, right: 4 }} onPress={next}>…</View>
```

| 属性 | 类型 | 说明 |
|---|---|---|
| `hitSlop` | `number \| { top?, right?, bottom?, left? }` | 在边界四周增加的点数。传数字时四边相同。 |

扩展出的区域要留在父视图的范围内（给小视图留出一些 margin 或 padding）：命中测试要经过父视图才能到达子视图，落在父视图之外的触摸到不了子视图的扩展区域。

## pointerEvents

`pointerEvents` 决定子树中哪些视图可以成为触摸的目标。它在原生命中测试时求值，每次触摸都不产生额外开销。

| 值 | 视图本身 | 子视图 |
|---|---|---|
| `"auto"`（默认） | 接收触摸 | 接收触摸 |
| `"none"` | 永远不是目标 | 永远不是目标：触摸穿透到后面的内容 |
| `"box-none"` | 永远不是目标 | 接收触摸 |
| `"box-only"` | 接收触摸 | 永远不是目标 |

典型用法：不能挡住下层内容的覆盖层（`"none"`）；全屏容器的空白处要把触摸让给后面的内容、而其中的按钮仍然可用（`"box-none"`）；手指落在子视图上时也要由整行接收按压（`"box-only"`）。

```tsx
<View style={{ flex: 1 }}>
  <Map />
  {/* 浮动工具栏：空白区域把触摸交给地图。 */}
  <View pointerEvents="box-none" style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, justifyContent: "flex-end", padding: 16 }}>
    <View onPress={locate} style={{ alignSelf: "flex-end", padding: 12, borderRadius: 24, backgroundColor: "#ffffff" }}>
      <Text>Locate</Text>
    </View>
  </View>
</View>
```

Tailwind 类 `pointer-events-none` 和 `pointer-events-auto` 设置的是同一件事；显式属性优先于类。

## 原生手势：拖动、捏合与旋转

要让拖动、捏合和旋转移动屏幕上的东西，就把[原生值](./animation.md#原生值)绑定到手势上。平台的手势识别器在 UI 线程上写入这些值，绑定了它们的视图在同一帧移动；JS 只在手势开始和结束时得到通知。

```tsx
import { mount, View } from "nsh";
import { createNativeValue } from "nsh/animated";

function App() {
  const x = createNativeValue(0);
  const y = createNativeValue(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
      <View
        pan={{ x, y }}
        onPanEnd={(e) => {
          // 以松手时的速度弹回原位。
          x.spring(0, { velocity: e.velocityX });
          y.spring(0, { velocity: e.velocityY });
        }}
        style={{ width: 120, height: 120, borderRadius: 24, backgroundColor: "#8250df", transform: [{ translateX: x }, { translateY: y }] }}
      />
    </View>
  );
}

mount(App);
```

| 属性 | 类型 | 说明 |
|---|---|---|
| `pan` | `{ x?: NativeValue; y?: NativeValue }` | 单指拖动驱动这些值，单位为点。 |
| `pinch` | `{ scale?: NativeValue }` | 双指捏合驱动 `scale`（倍数）。 |
| `rotation` | `{ angle?: NativeValue }` | 双指旋转驱动 `angle`，单位为度。 |
| `onPanStart` / `onPanEnd` | `(e: PanEvent) => void` | 拖动开始 / 结束。 |
| `onPinchStart` / `onPinchEnd` | `(e: { scale; velocity }) => void` | 捏合开始 / 结束。 |
| `onRotationStart` / `onRotationEnd` | `(e: { rotation; velocity }) => void` | 旋转开始 / 结束。 |

`PanEvent` 包含 `translationX` / `translationY`（自开始以来移动的点数）、`velocityX` / `velocityY`（每**秒**的点数）以及 `valueX` / `valueY`（此刻所绑定的值）。

原生端替你做了这些事：

- **拖动从值当前的位置继续。** 开始时读取这些值，再把位移加上去；抓住一个仍在动画中的值（例如正在弹回的弹簧）会让动画停在原处。
- **只绑定一个轴时，另一个轴照常滚动。** 只有 `pan={{ x }}` 时，只有手指的水平移动大于垂直移动，拖动才会开始，所以竖向滚动视图里的横向滑动行仍然可以上下滚动。这个判断在系统询问手势能否开始时由原生做出。
- **拖动、捏合和旋转可以同时进行**：同一张卡片可以同时被拖动、缩放和旋转。
- **用松手速度做物理效果。** `onPanEnd` 的速度可以直接传给 `spring({ velocity })` 或 `decay({ velocity })`；`valueX` / `valueY` 用来挑选吸附点。

```tsx
// 吸附到左边或右边的卡片。
<View
  pan={{ x }}
  onPanEnd={(e) => {
    const projected = (e.valueX ?? 0) + e.velocityX * 0.2;
    x.spring(projected > 0 ? 140 : -140, { velocity: e.velocityX });
  }}
  style={{ transform: [{ translateX: x }] }}
/>
```

> [!TIP]
> 只要拖动只是移动东西，就用 `pan` 而不是 `PanResponder`。用 `pan` 时，繁忙的 JS 线程不会让拖动卡顿；用 `PanResponder` 时，每一次移动都是一次发往 JS 的事件和一次重新渲染。

## 原始触摸事件

触摸事件报告每根手指的按下、移动和抬起。它们适合记录日志、自定义命中测试和响应多点触摸；它们不会占有触摸（按压和滚动视图照常工作）。

| 属性 | 说明 |
|---|---|
| `onTouchStart` | 有手指在视图上或其内部按下。 |
| `onTouchMove` | 手指移动。同一引擎轮次内的移动会被合并。 |
| `onTouchEnd` | 手指抬起。 |
| `onTouchCancel` | 系统接管了触摸（滚动视图开始滚动、来电等）。 |
| `onTouchStartCapture`、`onTouchMoveCapture`、`onTouchEndCapture`、`onTouchCancelCapture` | 同上，但从外向内调用，先于冒泡阶段的处理函数。 |

每个处理函数收到一个 `GestureResponderEvent`：

| 字段 | 说明 |
|---|---|
| `identifier` | 手指按下期间的标识。 |
| `locationX`、`locationY` | 在手指按下的那个视图（`target`）中的位置。 |
| `pageX`、`pageY` | 在整个 surface 中的位置。 |
| `target` | 手指按下的那个视图的 id。 |
| `timestamp` | 毫秒。 |
| `touches` | 所有按下的手指。 |
| `changedTouches` | 本次事件涉及的手指。 |
| `nativeEvent` | 事件本身，因此 `e.nativeEvent.locationX` 也可用。 |
| `stopPropagation()` | 外层视图不再收到这次触摸事件。 |

```tsx
<View
  onTouchStart={(e) => console.log("down", e.locationX, e.locationY, e.touches.length)}
  onTouchMove={(e) => console.log("move", e.pageX, e.pageY)}
  onTouchEnd={() => console.log("up")}
/>
```

## 手势响应者系统

当 JS 必须独占一段触摸序列时（手写绘图、自定义滑块、原生识别器没有提供的手势），使用手势响应者系统。同一时间只有一个视图是**响应者**并接收移动事件，由各视图协商出谁来担任。

成为响应者：

| 属性 | 返回值 | 说明 |
|---|---|---|
| `onStartShouldSetResponder` | `boolean` | 手指按下时从目标向外逐层询问。返回 `true` 即成为响应者。 |
| `onMoveShouldSetResponder` | `boolean` | 在没有响应者时（或为了接管）每次移动都会询问。 |
| `onStartShouldSetResponderCapture` | `boolean` | 同上，但从外向内询问，先于冒泡阶段。父视图用它从子视图手里拿走触摸。 |
| `onMoveShouldSetResponderCapture` | `boolean` | 移动时的版本。 |

这四个回调还有第二个参数 `gestureState`：从第一根手指按下以来的手势，字段与 [PanResponder](#panresponder) 的相同（`dx`、`dy`、`vx`、`vy`、`moveX`、`moveY`、`x0`、`y0`、`numberActiveTouches`）。只接管水平滑动：`onMoveShouldSetResponder={(e, g) => Math.abs(g.dx) > 10 && Math.abs(g.dx) > Math.abs(g.dy)}`。这个决定到达 JS 时已经晚了一次移动，外层的滚动视图可能已经开始滚动；滚动视图里的拖动请用 [`panActivation`](#滚动视图里的拖动panactivation)。

成为响应者之后：

| 属性 | 说明 |
|---|---|
| `onResponderGrant` | 该视图现在是响应者。返回 `true` 可阻止原生滚动视图接管触摸。 |
| `onResponderReject` | 另一个视图是响应者且不肯放手。 |
| `onResponderStart` | 又有一根手指按下。 |
| `onResponderMove` | 手指移动。 |
| `onResponderEnd` | 有手指抬起。 |
| `onResponderRelease` | 最后一根手指抬起：手势结束。 |
| `onResponderTerminationRequest` | 另一个视图想要接管。返回 `false` 保留触摸（默认 `true`）。 |
| `onResponderTerminate` | 另一个视图或某个原生手势接管了触摸。 |

```tsx
import { type GestureResponderEvent, mount, signal } from "nsh";
import { Canvas } from "nsh/canvas";

function Sketch() {
  const strokes = signal<[number, number][][]>([]);
  const point = (e: GestureResponderEvent): [number, number] => [e.locationX, e.locationY];
  return (
    <Canvas
      style={{ flex: 1 }}
      onStartShouldSetResponder={() => true}
      onResponderGrant={(e) => {
        strokes.set([...strokes(), [point(e)]]);
        return true; // 外层的滚动视图不能接管触摸
      }}
      onResponderTerminationRequest={() => false}
      onResponderMove={(e) => {
        const all = strokes();
        strokes.set([...all.slice(0, -1), [...all[all.length - 1], point(e)]]);
      }}
      draw={(ctx) => {
        ctx.lineWidth = 3;
        for (const s of strokes()) {
          ctx.beginPath();
          s.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
          ctx.stroke();
        }
      }}
    />
  );
}

mount(Sketch);
```

运行方式：带有任何触摸或响应者处理函数的视图会在原生端监听触摸；平台把发生在它上面（及其内部）的触摸连同沿途正在监听的视图路径一起上报，协商过程在运行时的 Rust 代码中进行，并按顺序调用你的处理函数：先从最外层视图向内调用 `…Capture` 处理函数，再从目标向外调用其他处理函数。由此有两个结果：

- **原生滚动视图与 JS 响应者相互竞争。** 如果 `onResponderGrant` 返回 `true`，响应者外层的滚动视图会停止接管触摸，直到手势结束。如果原生滚动视图先开始拖动，则滚动视图获胜：响应者收到 `onResponderTerminate`，触摸处理函数收到 `onTouchCancel`。
- **每次移动都是一次发往 JS 的事件。** 由响应者驱动的拖动，流畅程度取决于 JS 的速度；只是移动视图的拖动，优先使用 [`pan`](#原生手势拖动捏合与旋转)。

## PanResponder

`nsh/gesture` 中的 `PanResponder` 封装了手势响应者系统，并附加手势状态：自开始以来的距离、速度、手指数量。把它的 `panHandlers` 展开到视图上。

```tsx
import { mount, signal, View } from "nsh";
import { PanResponder } from "nsh/gesture";

function Ball() {
  const pos = signal({ x: 0, y: 0 });
  let start = { x: 0, y: 0 };
  const responder = PanResponder.create({
    onStartShouldSetPanResponder: () => true,
    onPanResponderGrant: () => {
      start = pos();
    },
    onPanResponderMove: (_, g) => pos.set({ x: start.x + g.dx, y: start.y + g.dy }),
    onPanResponderRelease: (_, g) => console.log("released at", g.vx, g.vy, "pt/ms"),
  });
  return (
    <View
      {...responder.panHandlers}
      style={{ width: 60, height: 60, borderRadius: 30, backgroundColor: "#0969da", transform: [{ translateX: pos().x }, { translateY: pos().y }] }}
    />
  );
}

mount(Ball);
```

回调参数为 `(event, gestureState)`：

| `gestureState` 字段 | 说明 |
|---|---|
| `stateID` | 手势的标识。 |
| `moveX`、`moveY` | 各触点中心的最新位置，surface 坐标（点）。 |
| `x0`、`y0` | 成为响应者时触点中心的位置。 |
| `dx`、`dy` | 自成为响应者以来移动的距离。 |
| `vx`、`vy` | 速度，单位为每**毫秒**的点数。 |
| `numberActiveTouches` | 按下的手指数。 |

可用的回调有 `onStartShouldSetPanResponder(Capture)`、`onMoveShouldSetPanResponder(Capture)`、`onPanResponderGrant`、`onPanResponderReject`、`onPanResponderStart`、`onPanResponderMove`、`onPanResponderEnd`、`onPanResponderRelease`、`onPanResponderTerminate`、`onPanResponderTerminationRequest` 和 `onShouldBlockNativeResponder`。最后一个默认为 `true`：成为响应者的 PanResponder 会阻止原生滚动视图接管它的触摸。

## 滚动视图里的拖动：panActivation

纵向滚动视图里一个可以横向拖动查看数值的图表，需要接管横向拖动、把纵向拖动留给滚动视图。按下时就接管触摸会让滚动从第一个像素起就被挡住；在 `onMoveShouldSetResponder` 里判断又太晚：等 JS 回答时，滚动视图可能已经拿走了触摸。`panActivation` 让平台在主线程上随着手指移动来决定：

```tsx
<View
  panActivation={{ activeOffsetX: 10, failOffsetY: 10 }}
  onResponderGrant={(e) => scrub(e.locationX)}
  onResponderMove={(e) => scrub(e.locationX)}
  onResponderRelease={() => scrub(null)}
>
  <Chart />
</View>
```

| 字段 | 说明 |
|---|---|
| `activeOffsetX`、`activeOffsetY` | 手指沿该轴离按下的位置超过这个距离（任一方向，单位点）时，拖动激活。 |
| `failOffsetX`、`failOffsetY` | 手指先超过这个距离时拖动失败：触摸留给滚动视图。 |

没有激活偏移时，任一方向移动 10 点后激活。激活之前，外层的滚动视图会等待它（iOS），视图也不会被询问是否成为响应者：它的 `…ShouldSetResponder` 回调会被跳过。激活后，滚动视图不能再拿走这些触摸，视图成为响应者（先 `onResponderGrant`，再收到移动）；失败时滚动视图照常滚动。`PanResponder` 的 `panHandlers` 也可以一起用：和这个属性放在同一个视图上，`onPanResponderGrant` 在激活时运行。

> [!NOTE]
> 不同 API 的速度单位不同：`PanResponder` 的 `vx` / `vy` 是每毫秒的点数，而 `onPanEnd` 的 `velocityX` / `velocityY` 以及 `spring` / `decay` 的 `velocity` 选项是每秒的点数。把 `vx` 传给弹簧前要乘以 1000。

## 为什么有些答案是属性

主线程从不等待 JS。当系统需要立即得到答案时（这个拖动手势能否开始、这个点是否命中视图、这次点击是否应该收起键盘），NSH 根据事先挂载好的属性在原生端直接回答：`pan`（它的轴）、`pinch`、`hitSlop`、`pointerEvents`、`keyboardShouldPersistTaps`、`panActivation`。你的 JS 处理函数事后才得知发生了什么。所以这些是声明式的属性而不是回调；也正因如此，会调用 JS 的响应者协商只在 JS 响应者之间做决定，从不阻塞原生手势。

## 相关内容

- [`View`](../elements/view.md)：基础元素上的所有触摸属性。
- [动画](./animation.md)：用于手势驱动运动的原生值、弹簧和衰减。
- [滚动](./scrolling.md)：`keyboardShouldPersistTaps`、滚动视图与嵌套手势。
- [`nsh/gesture`](../modules/nsh-gesture.md)：`PanResponder` 参考。
- [无障碍](./accessibility.md)：可按压视图是无障碍元素；给它们设置角色和标签。

---
title: nsh/gesture
description: PanResponder：基于手势响应者系统的拖动手势。
order: 7
---

# nsh/gesture

`nsh/gesture` 导出 `PanResponder`：构建在手势响应者系统之上的拖动手势，通过 `onPanResponder…` 回调接收触摸事件和手势状态。它会帮你跟踪各触点的中心、移动距离和速度。

```ts
import { PanResponder, type PanResponderGestureState } from "nsh/gesture";
```

> [!TIP]
> `PanResponder` 会把每一次移动都交给 JS，适合会改变应用状态的手势（拖动排序、判断是否滑动删除）。如果只是让视图跟随手指移动，`pan={{ x, y }}`、`pinch` 和 `rotation` 属性更快：拖动在 UI 线程上直接写入[动画值](./nsh-animated.md)，JS 只会收到 `onPanStart` / `onPanEnd`。见[触摸与手势](../docs/touch-and-gestures.md)。

## PanResponder.create

```ts
const PanResponder: { create(config: PanResponderCallbacks): PanResponderInstance };

interface PanResponderInstance {
  panHandlers: TouchProps;
}
```

创建一个 pan responder。把它的 `panHandlers` 展开到需要接收手势的视图上：

```tsx
import { mount, signal, View } from "nsh";
import { PanResponder } from "nsh/gesture";

function App() {
  const pos = signal({ x: 0, y: 0 });
  let start = { x: 0, y: 0 };
  const pan = PanResponder.create({
    onStartShouldSetPanResponder: () => true,
    onPanResponderGrant: () => (start = pos()),
    onPanResponderMove: (_, g) => pos.set({ x: start.x + g.dx, y: start.y + g.dy }),
    onPanResponderRelease: (_, g) => console.log(`released at ${g.vx.toFixed(2)} pt/ms`),
  });
  return (
    <View style={{ flex: 1 }}>
      <View
        {...pan.panHandlers}
        style={{
          width: 64,
          height: 64,
          borderRadius: 32,
          backgroundColor: "#6366f1",
          transform: [{ translateX: pos().x }, { translateY: pos().y }],
        }}
      />
    </View>
  );
}

mount(App);
```

## 回调

每个回调都会收到触摸事件（`GestureResponderEvent`，来自 [`nsh`](./nsh.md#类型)）和手势状态。

```ts
type PanHandler<R = void> = (e: GestureResponderEvent, gestureState: PanResponderGestureState) => R;
```

| 回调 | 返回值 | 调用时机 |
|---|---|---|
| `onStartShouldSetPanResponder` | `boolean` | 手指按下：返回 `true` 以成为响应者。从被触摸的视图向外依次询问。 |
| `onStartShouldSetPanResponderCapture` | `boolean` | 同上，但在冒泡询问之前从外向内询问（父视图从子视图手中接管手势）。 |
| `onMoveShouldSetPanResponder` | `boolean` | 手指移动：返回 `true` 以成为响应者（例如仅当 `Math.abs(g.dx) > 10` 时）。 |
| `onMoveShouldSetPanResponderCapture` | `boolean` | 同上，捕获阶段。 |
| `onPanResponderGrant` | | 该视图成为响应者：手势开始。 |
| `onPanResponderReject` | | 另一个视图是响应者且不肯放手。 |
| `onPanResponderStart` | | 响应期间有手指按下。 |
| `onPanResponderMove` | | 手指移动。 |
| `onPanResponderEnd` | | 响应期间有手指抬起。 |
| `onPanResponderRelease` | | 最后一根手指抬起：手势结束。 |
| `onPanResponderTerminate` | | 另一个视图（或原生手势，比如滚动视图）接管了触摸。 |
| `onPanResponderTerminationRequest` | `boolean` | 另一个视图想要接管触摸：返回 `false` 以保留（默认 `true`）。 |
| `onShouldBlockNativeResponder` | `boolean` | 响应期间是否阻止原生滚动视图接管触摸（默认 `true`）。 |

## PanResponderGestureState

```ts
interface PanResponderGestureState {
  stateID: number;
  moveX: number;               // 触点中心的最新位置（界面坐标，点）
  moveY: number;
  x0: number;                  // 成为响应者时触点中心的位置
  y0: number;
  dx: number;                  // 成为响应者以来移动的距离
  dy: number;
  vx: number;                  // 速度，点/毫秒
  vy: number;
  numberActiveTouches: number;
}
```

| 字段 | 说明 |
|---|---|
| `stateID` | 标识这次手势。 |
| `moveX`、`moveY` | 触点中心的最新位置，界面坐标（点）。 |
| `x0`、`y0` | 成为响应者时触点中心的位置。 |
| `dx`、`dy` | 成为响应者以来移动的距离。 |
| `vx`、`vy` | 速度，单位为点/毫秒。 |
| `numberActiveTouches` | 按下的手指数。 |

## 在滚动视图中

当 pan responder 作为响应者时，外层的原生滚动视图不会滚动（`onShouldBlockNativeResponder` 默认为 `true`）。如果要让横向滑动与纵向滚动共存，只在横向移动时成为响应者：

```ts
const swipe = PanResponder.create({
  onMoveShouldSetPanResponder: (_, g) => Math.abs(g.dx) > 10 && Math.abs(g.dx) > Math.abs(g.dy),
  onPanResponderMove: (_, g) => offset.set(g.dx),
  onPanResponderRelease: (_, g) => (Math.abs(g.dx) > 120 ? remove() : offset.set(0)),
  onPanResponderTerminate: () => offset.set(0),
});
```

底层的响应者属性（`onStartShouldSetResponder`、`onResponderGrant`、`onTouchStart` 等）也可以直接用在任何元素上；见[元素](../elements/index.md)。

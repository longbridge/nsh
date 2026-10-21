---
title: nsh/gesture
description: PanResponder：基于手势响应者系统的拖动手势。
order: 7
---

# nsh/gesture

Pan responder 是构建在手势响应者系统之上的拖动手势：元素认领一次手势的触摸，然后独自接收这些触摸和手势状态，手势状态会帮你跟踪各触点的中心、移动距离和速度。Rust 代码用 `.claim_touches(TouchClaim::…)` 认领触摸，用 `.on_responder_*` 监听，其 `ResponderEvent` 带有 `PanGesture`，都在 `nsh_core::ui::prelude` 中；TSX 从 `nsh/gesture` 导入 `PanResponder`，它的 `onPanResponder…` 回调接收触摸事件和手势状态。

```rust tabs
use nsh_core::ui::prelude::*; // TouchClaim、ClaimRequest、ResponderEvent、PanGesture
```

```tsx tabs
import { PanResponder, type PanResponderGestureState } from "nsh/gesture";
```

> [!TIP]
> Pan responder 会把每一次移动都交给应用的代码，适合会改变应用状态的手势（拖动排序、判断是否滑动删除）。如果只是让视图跟随手指移动，原生手势值更快：Rust 用 `.pan_values(x, y)`、`.pinch_value(scale)` 和 `.rotation_value(angle)`，TSX 用 `pan={{ x, y }}`、`pinch` 和 `rotation` 属性。拖动在 UI 线程上直接写入[动画值](./nsh-animated.md)，应用只会收到开始和结束（`on_pan_start` / `on_pan_end`、`onPanStart` / `onPanEnd`）。见[触摸与手势](../docs/touch-and-gestures.md)。

## PanResponder.create

```ts
const PanResponder: { create(config: PanResponderCallbacks): PanResponderInstance };

interface PanResponderInstance {
  panHandlers: TouchProps;
}
```

创建一个 pan responder。把它的 `panHandlers` 展开到需要接收手势的视图上。在 Rust 中，由视图自己认领触摸并监听：

```rust tabs
use nsh_core::ui::prelude::*;

#[component]
pub fn App() -> impl IntoElement {
    let pos = signal((0.0_f32, 0.0_f32));
    let start = stored((0.0_f32, 0.0_f32));
    view().flex_1().child(
        view()
            .size(px(64.0))
            .rounded(px(32.0))
            .bg(rgb(0x6366f1))
            .style_with(move |s| {
                let (x, y) = pos.get();
                s.translate_x(px(x)).translate_y(px(y))
            })
            .claim_touches(TouchClaim::on_start())
            .on_responder_grant(move |_: &ResponderEvent| {
                start.replace(pos.get_untracked());
            })
            .on_responder_move(move |e: &ResponderEvent| {
                let (x, y) = start.get();
                pos.set((x + e.gesture.delta.x.0, y + e.gesture.delta.y.0));
            })
            .on_responder_release(move |e: &ResponderEvent| {
                log::info!("released at {:.2} pt/ms", e.gesture.vx);
            }),
    )
}
```

```tsx tabs
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

每个回调都会收到触摸事件（`GestureResponderEvent`，来自 [`nsh`](./nsh.md#类型)）和手势状态。在 Rust 中，认领用 `.ask(|request: &ClaimRequest| …)` 决定，调用 `request.claim()` 接管触摸（没有 `ask` 时总是接管），每个监听器收到一个 `ResponderEvent`，其 `gesture` 就是手势状态。

```ts
type PanHandler<R = void> = (e: GestureResponderEvent, gestureState: PanResponderGestureState) => R;
```

| 回调 | Rust | 返回值 | 调用时机 |
|---|---|---|---|
| `onStartShouldSetPanResponder` | `.claim_touches(TouchClaim::on_start())` | `boolean` | 手指按下：返回 `true` 以成为响应者。从被触摸的视图向外依次询问。 |
| `onStartShouldSetPanResponderCapture` | `TouchClaim::on_start().capture()` | `boolean` | 同上，但在冒泡询问之前从外向内询问（父视图从子视图手中接管手势）。 |
| `onMoveShouldSetPanResponder` | `TouchClaim::on_move().ask(…)` | `boolean` | 手指移动：返回 `true` 以成为响应者（例如仅当 `Math.abs(g.dx) > 10` 时）。 |
| `onMoveShouldSetPanResponderCapture` | `TouchClaim::on_move().capture()` | `boolean` | 同上，捕获阶段。 |
| `onPanResponderGrant` | `.on_responder_grant(…)` | | 该视图成为响应者：手势开始。 |
| `onPanResponderReject` | `.on_responder_reject(…)` | | 另一个视图是响应者且不肯放手。 |
| `onPanResponderStart` | `.on_responder_start(…)` | | 响应期间有手指按下。 |
| `onPanResponderMove` | `.on_responder_move(…)` | | 手指移动。 |
| `onPanResponderEnd` | `.on_responder_end(…)` | | 响应期间有手指抬起。 |
| `onPanResponderRelease` | `.on_responder_release(…)` | | 最后一根手指抬起：手势结束。 |
| `onPanResponderTerminate` | `.on_responder_terminate(…)` | | 另一个视图（或原生手势，比如滚动视图）接管了触摸。 |
| `onPanResponderTerminationRequest` | `.keep_touches(true)` 保留触摸 | `boolean` | 另一个视图想要接管触摸：返回 `false` 以保留（默认 `true`）。 |
| `onShouldBlockNativeResponder` | `TouchClaim::block_native(…)` | `boolean` | 响应期间是否阻止原生滚动视图接管触摸（默认 `true`）。 |

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

Rust：`PanGesture`，位置为 `Point<Points>`。

| 字段 | Rust | 说明 |
|---|---|---|
| `stateID` |  | 标识这次手势。 |
| `moveX`、`moveY` | `position` | 触点中心的最新位置，界面坐标（点）。 |
| `x0`、`y0` | `origin` | 成为响应者时触点中心的位置。 |
| `dx`、`dy` | `delta` | 成为响应者以来移动的距离。 |
| `vx`、`vy` | `vx`、`vy` | 速度，单位为点/毫秒。 |
| `numberActiveTouches` | `active_touches` | 按下的手指数。 |

## 在滚动视图中

当 pan responder 作为响应者时，外层的原生滚动视图不会滚动（`onShouldBlockNativeResponder` 默认为 `true`）。如果要让横向滑动与纵向滚动共存，只在横向移动时成为响应者：

```rust tabs
view()
    .claim_touches(TouchClaim::on_move().ask(|request: &ClaimRequest| {
        let d = request.event.gesture.delta;
        if d.x.0.abs() > 10.0 && d.x.0.abs() > d.y.0.abs() {
            request.claim();
        }
    }))
    .on_responder_move(move |e: &ResponderEvent| offset.set(e.gesture.delta.x.0))
    .on_responder_release(move |e: &ResponderEvent| {
        if e.gesture.delta.x.0.abs() > 120.0 { remove() } else { offset.set(0.0) }
    })
    .on_responder_terminate(move |_: &ResponderEvent| offset.set(0.0))
```

```tsx tabs
const swipe = PanResponder.create({
  onMoveShouldSetPanResponder: (_, g) => Math.abs(g.dx) > 10 && Math.abs(g.dx) > Math.abs(g.dy),
  onPanResponderMove: (_, g) => offset.set(g.dx),
  onPanResponderRelease: (_, g) => (Math.abs(g.dx) > 120 ? remove() : offset.set(0)),
  onPanResponderTerminate: () => offset.set(0),
});
```

对于由平台识别的拖动，`TouchClaim::on_pan(PanActivation::horizontal(10.0, 10.0))` 在手指移动时于主线程上决定。底层的响应者属性（`onStartShouldSetResponder`、`onResponderGrant`、`onTouchStart` 等）也可以直接用在任何元素上；见[元素](../elements/index.md)。

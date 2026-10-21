---
title: nsh/gesture
description: PanResponder, pan gestures built on the gesture responder system.
order: 7
---

# nsh/gesture

Pan responders are drag gestures on top of the gesture responder system: the element claims the touches of a gesture, then hears them alone with the gesture state, which tracks the touches' centroid, the distance moved and the velocity for you. Rust code claims them with `.claim_touches(TouchClaim::…)` and hears them with `.on_responder_*` listeners whose `ResponderEvent` carries a `PanGesture`, all in `nsh_core::ui::prelude`; TSX imports `PanResponder` from `nsh/gesture`, whose `onPanResponder…` callbacks receive the touch event and the gesture state.

```rust tabs
use nsh_core::ui::prelude::*; // TouchClaim, ClaimRequest, ResponderEvent, PanGesture
```

```tsx tabs
import { PanResponder, type PanResponderGestureState } from "nsh/gesture";
```

> [!TIP]
> A pan responder delivers every move to the app's code, so it suits gestures that change app state (reordering, swipe-to-delete decisions). To move a view with the finger, the native gesture values are faster: `.pan_values(x, y)`, `.pinch_value(scale)` and `.rotation_value(angle)` in Rust, the `pan={{ x, y }}`, `pinch` and `rotation` props in TSX. The drag writes [animated values](./nsh-animated.md) on the UI thread and the app only hears when it starts and ends (`on_pan_start` / `on_pan_end`, `onPanStart` / `onPanEnd`). See [Touch and gestures](../docs/touch-and-gestures.md).

## PanResponder.create

```ts
const PanResponder: { create(config: PanResponderCallbacks): PanResponderInstance };

interface PanResponderInstance {
  panHandlers: TouchProps;
}
```

Creates a pan responder. Spread its `panHandlers` on the view that should receive the gesture. In Rust, the view claims the touches and listens itself:

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

## Callbacks

Every callback receives the touch event (`GestureResponderEvent`, from [`nsh`](./nsh.md#types)) and the gesture state. In Rust, a claim decides with `.ask(|request: &ClaimRequest| …)`, calling `request.claim()` to take the touches (without `ask` it takes them always), and each listener gets a `ResponderEvent` whose `gesture` is the state.

```ts
type PanHandler<R = void> = (e: GestureResponderEvent, gestureState: PanResponderGestureState) => R;
```

| Callback | Rust | Returns | Called when |
|---|---|---|---|
| `onStartShouldSetPanResponder` | `.claim_touches(TouchClaim::on_start())` | `boolean` | A finger goes down: return `true` to become the responder. Asked from the touched view outwards. |
| `onStartShouldSetPanResponderCapture` | `TouchClaim::on_start().capture()` | `boolean` | The same, asked from the outside in before the bubbling one (a parent taking the gesture from its children). |
| `onMoveShouldSetPanResponder` | `TouchClaim::on_move().ask(…)` | `boolean` | A finger moves: return `true` to become the responder (e.g. only once `Math.abs(g.dx) > 10`). |
| `onMoveShouldSetPanResponderCapture` | `TouchClaim::on_move().capture()` | `boolean` | The same, capture phase. |
| `onPanResponderGrant` | `.on_responder_grant(…)` | | This view is the responder now: the gesture starts. |
| `onPanResponderReject` | `.on_responder_reject(…)` | | Another view is the responder and would not let go. |
| `onPanResponderStart` | `.on_responder_start(…)` | | A finger went down while responding. |
| `onPanResponderMove` | `.on_responder_move(…)` | | The fingers moved. |
| `onPanResponderEnd` | `.on_responder_end(…)` | | A finger lifted while responding. |
| `onPanResponderRelease` | `.on_responder_release(…)` | | The last finger lifted: the gesture ended. |
| `onPanResponderTerminate` | `.on_responder_terminate(…)` | | Another view (or a native gesture, like a scroll view) took the touches. |
| `onPanResponderTerminationRequest` | `.keep_touches(true)` keeps them | `boolean` | Another view wants the touches: return `false` to keep them (default `true`). |
| `onShouldBlockNativeResponder` | `TouchClaim::block_native(…)` | `boolean` | Whether to keep native scroll views from taking the touches while responding (default `true`). |

## PanResponderGestureState

```ts
interface PanResponderGestureState {
  stateID: number;
  moveX: number;               // latest centroid of the touches (surface points)
  moveY: number;
  x0: number;                  // the centroid when the responder was granted
  y0: number;
  dx: number;                  // distance moved since the grant
  dy: number;
  vx: number;                  // velocity, points per millisecond
  vy: number;
  numberActiveTouches: number;
}
```

Rust: `PanGesture`, with points as `Point<Points>`.

| Field | Rust | Description |
|---|---|---|
| `stateID` |  | Identifies the gesture. |
| `moveX`, `moveY` | `position` | Latest position of the touches' centroid, in surface points. |
| `x0`, `y0` | `origin` | Where the centroid was when the responder was granted. |
| `dx`, `dy` | `delta` | Distance moved since the grant. |
| `vx`, `vy` | `vx`, `vy` | Velocity in points per millisecond. |
| `numberActiveTouches` | `active_touches` | Fingers down. |

## Inside scroll views

While a pan responder is the responder, native scroll views around it do not scroll (`onShouldBlockNativeResponder` defaults to `true`). To let a horizontal swipe coexist with vertical scrolling, become the responder only on horizontal moves:

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

For a pan the platform recognizes, `TouchClaim::on_pan(PanActivation::horizontal(10.0, 10.0))` decides on the main thread as the fingers move. The underlying responder props (`onStartShouldSetResponder`, `onResponderGrant`, `onTouchStart`, ...) can also be used directly on any element; see [Elements](../elements/index.md).

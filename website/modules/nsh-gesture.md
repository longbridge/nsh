---
title: nsh/gesture
description: PanResponder, pan gestures built on the gesture responder system.
order: 7
---

# nsh/gesture

`nsh/gesture` exports `PanResponder`: drag gestures on top of the gesture responder system, with `onPanResponder…` callbacks that receive the touch event and a gesture state. It tracks the touches' centroid, the distance moved and the velocity for you.

```ts
import { PanResponder, type PanResponderGestureState } from "nsh/gesture";
```

> [!TIP]
> `PanResponder` delivers every move to JS, so it suits gestures that change app state (reordering, swipe-to-delete decisions). To move a view with the finger, the `pan={{ x, y }}`, `pinch` and `rotation` props are faster: the drag writes [animated values](./nsh-animated.md) on the UI thread and JS only hears `onPanStart` / `onPanEnd`. See [Touch and gestures](../docs/touch-and-gestures.md).

## PanResponder.create

```ts
const PanResponder: { create(config: PanResponderCallbacks): PanResponderInstance };

interface PanResponderInstance {
  panHandlers: TouchProps;
}
```

Creates a pan responder. Spread its `panHandlers` on the view that should receive the gesture:

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

## Callbacks

Every callback receives the touch event (`GestureResponderEvent`, from [`nsh`](./nsh.md#types)) and the gesture state.

```ts
type PanHandler<R = void> = (e: GestureResponderEvent, gestureState: PanResponderGestureState) => R;
```

| Callback | Returns | Called when |
|---|---|---|
| `onStartShouldSetPanResponder` | `boolean` | A finger goes down: return `true` to become the responder. Asked from the touched view outwards. |
| `onStartShouldSetPanResponderCapture` | `boolean` | The same, asked from the outside in before the bubbling one (a parent taking the gesture from its children). |
| `onMoveShouldSetPanResponder` | `boolean` | A finger moves: return `true` to become the responder (e.g. only once `Math.abs(g.dx) > 10`). |
| `onMoveShouldSetPanResponderCapture` | `boolean` | The same, capture phase. |
| `onPanResponderGrant` | | This view is the responder now: the gesture starts. |
| `onPanResponderReject` | | Another view is the responder and would not let go. |
| `onPanResponderStart` | | A finger went down while responding. |
| `onPanResponderMove` | | The fingers moved. |
| `onPanResponderEnd` | | A finger lifted while responding. |
| `onPanResponderRelease` | | The last finger lifted: the gesture ended. |
| `onPanResponderTerminate` | | Another view (or a native gesture, like a scroll view) took the touches. |
| `onPanResponderTerminationRequest` | `boolean` | Another view wants the touches: return `false` to keep them (default `true`). |
| `onShouldBlockNativeResponder` | `boolean` | Whether to keep native scroll views from taking the touches while responding (default `true`). |

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

| Field | Description |
|---|---|
| `stateID` | Identifies the gesture. |
| `moveX`, `moveY` | Latest position of the touches' centroid, in surface points. |
| `x0`, `y0` | Where the centroid was when the responder was granted. |
| `dx`, `dy` | Distance moved since the grant. |
| `vx`, `vy` | Velocity in points per millisecond. |
| `numberActiveTouches` | Fingers down. |

## Inside scroll views

While a pan responder is the responder, native scroll views around it do not scroll (`onShouldBlockNativeResponder` defaults to `true`). To let a horizontal swipe coexist with vertical scrolling, become the responder only on horizontal moves:

```ts
const swipe = PanResponder.create({
  onMoveShouldSetPanResponder: (_, g) => Math.abs(g.dx) > 10 && Math.abs(g.dx) > Math.abs(g.dy),
  onPanResponderMove: (_, g) => offset.set(g.dx),
  onPanResponderRelease: (_, g) => (Math.abs(g.dx) > 120 ? remove() : offset.set(0)),
  onPanResponderTerminate: () => offset.set(0),
});
```

The underlying responder props (`onStartShouldSetResponder`, `onResponderGrant`, `onTouchStart`, ...) can also be used directly on any element; see [Elements](../elements/index.md).

---
title: Touch and gestures
description: Presses and press feedback, hit areas, pointerEvents, raw touches, the responder system, PanResponder, and native pan, pinch and rotation gestures.
order: 38
group: ui
demo: guide-gestures
---

# Touch and gestures

NSH has three levels of touch handling. **Presses** (`onPress` and friends) cover buttons, rows and links, with press feedback drawn natively. **Native gestures** (`pan`, `pinch`, `rotation`) move [native values](./animation.md#native-values) on the UI thread, so a dragged card follows the finger even while JS is busy. **Raw touches and the responder system** (`onTouchStart`, `onResponderMove`, `PanResponder`) give you every finger position in JS for anything the other two cannot express, such as drawing.

Pick the highest level that does the job: the higher the level, the less work crosses from the UI thread to JS.

## Presses

Every element accepts the press handlers; there is no separate "touchable" component. Put `onPress` on a `<View>`, a `<Text>`, an `<Image>` or anything else.

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

| Prop | Type | Default | Description |
|---|---|---|---|
| `onPress` | `() => void` | | The finger lifted inside the view (after a press, not a long press). |
| `onLongPress` | `() => void` | | The finger stayed down for `delayLongPress`. `onPress` does not follow. |
| `onPressIn` | `() => void` | | The press started. |
| `onPressOut` | `() => void` | | The press ended: lifted, slid off, or taken over by a scroll view. |
| `delayLongPress` | `number` | `500` | Milliseconds before a press becomes a long press. |
| `disabled` | `boolean` | `false` | Takes no presses and shows no feedback; applies `disabled:` classes. |

How presses behave:

- **Only the innermost pressable view responds.** A pressable row containing a pressable button gets no press when the button is pressed.
- **Inside a scroll view, the press waits about 130 ms** before it starts (and shows feedback), so a drag that begins on a button scrolls instead of pressing it.
- **Sliding off cancels.** Moving the finger out of the view ends the press with `onPressOut` and no `onPress`.
- **`disabled` removes the press listeners** from what is mounted, so the view shows no feedback at all, and `disabled:` classes apply (`class="disabled:opacity-40"`).
- **Nested `<Text>` spans with `onPress` are links.** The paragraph is one native view; the press goes to the span under the finger. See [Text](./text.md).

## Press feedback

Press feedback is drawn by the platform the moment the finger touches down, with no round trip to JS. Choose one per view:

| Prop | Type | Default | Description |
|---|---|---|---|
| `activeOpacity` | `number` | | The view's opacity while pressed. |
| `underlayColor` | `ColorValue` | | The background while pressed; the children fade to `underlayOpacity`. |
| `underlayOpacity` | `number` | `0.85` | The children's opacity while `underlayColor` shows. |
| `android={{ ripple }}` | `{ color?, borderless?, radius?, foreground? }` | | Android's material ripple. Ignored on iOS. |

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

A borderless ripple spreads outside the view, which suits icon buttons: `android={{ ripple: { borderless: true, radius: 24 } }}`. Combining `activeOpacity` with a ripple is common: iOS uses the opacity, Android the ripple. See [Platform-specific code](./platform-specific.md).

The `active:` Tailwind variant is the third option. It applies classes while the view is pressed, and with a `transition` class the change animates:

```tsx
<View
  onPress={save}
  class="rounded-xl bg-emerald-600 p-4 transition active:scale-95 active:bg-emerald-800"
>
  <Text class="font-semibold text-white">Save</Text>
</View>
```

## Hit areas: hitSlop

`hitSlop` extends the area that takes touches beyond the view's bounds, without changing its layout. Use it for small targets such as close buttons and toggles.

```tsx
<View hitSlop={12} onPress={close} style={{ width: 20, height: 20 }}>…</View>
<View hitSlop={{ top: 10, bottom: 10, left: 4, right: 4 }} onPress={next}>…</View>
```

| Prop | Type | Description |
|---|---|---|
| `hitSlop` | `number \| { top?, right?, bottom?, left? }` | Points added around the bounds. A number applies to all four sides. |

Keep the extended area inside the parent's bounds (give the small view some margin or padding around it): hit testing reaches a child through its parent, so a touch outside the parent never gets to the child's slop.

## Children outside their parent

A child placed outside its parent's bounds (`position: "absolute"` with `overflow` left visible, such as a dropdown under its field) takes touches where it is drawn, like any other view: NSH hit-tests the descendants that reach outside a view natively. A parent with `overflow: "hidden"` clips them, and then they neither show nor take touches there. The area is computed from the layout, so a child moved out of its parent by a `transform` only takes touches inside the parent's bounds.

Mind the drawing order: a dropdown is drawn over the siblings of its field only if the field comes after them or has a higher `zIndex`, and touches go to what is drawn on top.

## pointerEvents

`pointerEvents` decides which views of a subtree can be the target of a touch. It is evaluated natively during hit testing, so it costs nothing per touch.

| Value | The view | Its children |
|---|---|---|
| `"auto"` (default) | Takes touches | Take touches |
| `"none"` | Never the target | Never the target: touches pass through to what is behind |
| `"box-none"` | Never the target | Take touches |
| `"box-only"` | Takes touches | Never the target |

The typical uses: an overlay that must not block what is under it (`"none"`), a full-screen container whose empty space should let touches through to the content behind while its buttons still work (`"box-none"`), and a row that should take a press even when the finger lands on a child (`"box-only"`).

```tsx
<View style={{ flex: 1 }}>
  <Map />
  {/* A floating toolbar: the empty area passes touches to the map. */}
  <View pointerEvents="box-none" style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, justifyContent: "flex-end", padding: 16 }}>
    <View onPress={locate} style={{ alignSelf: "flex-end", padding: 12, borderRadius: 24, backgroundColor: "#ffffff" }}>
      <Text>Locate</Text>
    </View>
  </View>
</View>
```

The Tailwind classes `pointer-events-none` and `pointer-events-auto` set the same thing; an explicit prop wins over the class.

## Native gestures: pan, pinch and rotation

For drags, pinches and rotations that move something on screen, bind [native values](./animation.md#native-values) to the gesture. The platform's gesture recognizer writes the values on the UI thread and the views bound to them move in the same frame; JS is told when the gesture starts and ends.

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
          // Spring back, starting at the speed the finger let go with.
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

| Prop | Type | Description |
|---|---|---|
| `pan` | `{ x?: NativeValue; y?: NativeValue }` | One-finger drags move these values, in points. |
| `pinch` | `{ scale?: NativeValue }` | Two-finger pinches drive `scale` (a factor). |
| `rotation` | `{ angle?: NativeValue }` | Two-finger rotations drive `angle`, in degrees. |
| `onPanStart` / `onPanEnd` | `(e: PanEvent) => void` | The drag started / ended. |
| `onPinchStart` / `onPinchEnd` | `(e: { scale; velocity }) => void` | The pinch started / ended. |
| `onRotationStart` / `onRotationEnd` | `(e: { rotation; velocity }) => void` | The rotation started / ended. |

`PanEvent` has `translationX` / `translationY` (points since the start), `velocityX` / `velocityY` (points per **second**) and `valueX` / `valueY` (the bound values at that moment).

What the native side does for you:

- **A drag continues from where the value is.** At the start the values are read, and the translation is added to them; grabbing a value that is still animating (a spring on its way home) stops the animation where it is.
- **One axis only lets the other scroll.** With only `pan={{ x }}` the drag begins only when the finger moves more horizontally than vertically, so a horizontal swipe row inside a vertical scroll view still scrolls vertically. This decision is made natively when the system asks whether the gesture may begin.
- **Pan, pinch and rotation run together** on the same view: one card can be dragged, pinched and rotated at once.
- **Release velocity for physics.** `onPanEnd`'s velocity feeds `spring({ velocity })` or `decay({ velocity })` directly; `valueX` / `valueY` let you pick a snap point.

```tsx
// A card that snaps to the left or right edge.
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
> Use `pan` rather than `PanResponder` whenever the drag only moves things. With `pan`, a busy JS thread cannot make the drag stutter; with `PanResponder`, every move is an event to JS and a re-render.

## Raw touches

The touch handlers report every finger as it goes down, moves and lifts. They are useful for logging, custom hit testing and reacting to multi-touch; they do not claim the touch (presses and scroll views keep working).

| Prop | Description |
|---|---|
| `onTouchStart` | A finger went down on the view or inside it. |
| `onTouchMove` | A finger moved. Moves are merged per engine turn. |
| `onTouchEnd` | A finger lifted. |
| `onTouchCancel` | The system took the touch (a scroll view started scrolling, a call came in). |
| `onTouchStartCapture`, `onTouchMoveCapture`, `onTouchEndCapture`, `onTouchCancelCapture` | The same, called from the outside in, before the bubbling handlers. |

Each handler gets a `GestureResponderEvent`:

| Field | Description |
|---|---|
| `identifier` | Identifies the finger while it is down. |
| `locationX`, `locationY` | Position in the view the finger went down on (`target`). |
| `pageX`, `pageY` | Position in the surface. |
| `target` | The id of the view the finger went down on. |
| `timestamp` | Milliseconds. |
| `touches` | Every finger that is down. |
| `changedTouches` | The fingers this event is about. |
| `nativeEvent` | The event itself, so `e.nativeEvent.locationX` works too. |
| `stopPropagation()` | Views further out do not see this touch event. |

```tsx
<View
  onTouchStart={(e) => console.log("down", e.locationX, e.locationY, e.touches.length)}
  onTouchMove={(e) => console.log("move", e.pageX, e.pageY)}
  onTouchEnd={() => console.log("up")}
/>
```

### Where a view is: measure

`locationX` / `locationY` are relative to the view the finger went down on, which may be a child of the view you care about (a slider's knob). `ref.measure()` tells you where a view is, so you can use `pageX` / `pageY` instead:

```tsx
let track!: YNode;
let origin = 0;
<View
  ref={track}
  onStartShouldSetResponder={() => true}
  onResponderGrant={async (e) => {
    const x = e.pageX;
    origin = (await track.measure())?.pageX ?? 0;
    seek(x - origin);
  }}
  onResponderMove={(e) => seek(e.pageX - origin)}
/>;
```

It resolves once the changes made so far are laid out, to `{ x, y, width, height, pageX, pageY }`: the frame in the parent (as `onLayout` reports it) and the top left corner in the surface, or in the modal the view is in, the same coordinates as touches' `pageX` / `pageY`. The offsets of the scroll views and lists around it are taken into account; transforms (animations included) are not. It resolves to `null` for a view that is not on screen.

## The responder system

When JS has to own a touch sequence (finger drawing, a custom slider, a gesture the native recognizers do not offer), use the gesture responder system. One view at a time is the **responder** and receives the moves; views negotiate who that is.

Becoming the responder:

| Prop | Return | Description |
|---|---|---|
| `onStartShouldSetResponder` | `boolean` | Asked when a finger goes down, from the target outwards. Return `true` to become the responder. |
| `onMoveShouldSetResponder` | `boolean` | Asked on every move while there is no responder (or to take it over). |

These four get a second argument, `gestureState`: the gesture since the first finger went down, with the same fields as [PanResponder's](#panresponder) (`dx`, `dy`, `vx`, `vy`, `moveX`, `moveY`, `x0`, `y0`, `numberActiveTouches`). A swipe that claims only horizontal moves: `onMoveShouldSetResponder={(e, g) => Math.abs(g.dx) > 10 && Math.abs(g.dx) > Math.abs(g.dy)}`. That decision still reaches JS a move late, so a scroll view around may start scrolling first; for drags inside scroll views use [`panActivation`](#pans-inside-scroll-views-panactivation).
| `onStartShouldSetResponderCapture` | `boolean` | As above, asked from the outside in, before the bubbling ones. A parent uses it to take a touch from its children. |
| `onMoveShouldSetResponderCapture` | `boolean` | The move version. |

While responding:

| Prop | Description |
|---|---|
| `onResponderGrant` | This view is now the responder. Return `true` to keep native scroll views from taking the touches. |
| `onResponderReject` | Another view is the responder and would not let go. |
| `onResponderStart` | Another finger went down. |
| `onResponderMove` | A finger moved. |
| `onResponderEnd` | A finger lifted. |
| `onResponderRelease` | The last finger lifted: the gesture is over. |
| `onResponderTerminationRequest` | Another view wants the touches. Return `false` to keep them (default `true`). |
| `onResponderTerminate` | Another view, or a native gesture, took the touches. |

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
        return true; // the scroll view around must not take the touches
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

How it runs: a view with any touch or responder handler listens to touches natively; the platform reports the touches on it (and inside it) with the path of listening views, and the negotiation runs in the runtime's Rust code, calling your handlers in order: the `…Capture` handlers from the outermost view inwards, then the others from the target outwards. Two consequences:

- **Native scroll views and JS responders compete.** If `onResponderGrant` returns `true`, the scroll views around the responder stop taking the touches until it ends. If a native scroll view starts dragging first, it wins: the responder gets `onResponderTerminate` and the touch handlers `onTouchCancel`.
- **Each move is an event to JS.** Responder-driven drags are as smooth as your JS is fast; for drags that only move views, prefer [`pan`](#native-gestures-pan-pinch-and-rotation).

## PanResponder

`PanResponder` from `nsh/gesture` wraps the responder system and adds the gesture state: distance from the start, velocity, the number of fingers. Spread its `panHandlers` on a view.

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

The callbacks get `(event, gestureState)`:

| `gestureState` field | Description |
|---|---|
| `stateID` | Identifies the gesture. |
| `moveX`, `moveY` | The latest position of the touches' centroid, in surface points. |
| `x0`, `y0` | Where the centroid was when the responder was granted. |
| `dx`, `dy` | Distance moved since the grant. |
| `vx`, `vy` | Velocity, in points per **millisecond**. |
| `numberActiveTouches` | Fingers down. |

The callbacks are `onStartShouldSetPanResponder(Capture)`, `onMoveShouldSetPanResponder(Capture)`, `onPanResponderGrant`, `onPanResponderReject`, `onPanResponderStart`, `onPanResponderMove`, `onPanResponderEnd`, `onPanResponderRelease`, `onPanResponderTerminate`, `onPanResponderTerminationRequest` and `onShouldBlockNativeResponder`. The last one defaults to `true`: a granted PanResponder keeps native scroll views from taking its touches.

## Pans inside scroll views: panActivation

A chart scrubbed sideways inside a vertical scroll view has to take horizontal drags and leave vertical ones to the scroll view. Claiming the touch on start blocks scrolling from the first pixel, and deciding in `onMoveShouldSetResponder` is too late: by the time JS answers, the scroll view may have taken the touch. `panActivation` lets the platform decide on the main thread, as the finger moves:

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

| Field | Description |
|---|---|
| `activeOffsetX`, `activeOffsetY` | The pan activates once the finger has moved further than this from where it went down, along that axis (either direction, points). |
| `failOffsetX`, `failOffsetY` | The pan fails once the finger has moved further than this first: the scroll views keep the touches. |

Without active offsets the pan activates after 10 points in any direction. Until it activates, the scroll views around wait for it (iOS) and the view is not asked to become the responder: its `…ShouldSetResponder` handlers are skipped. When it activates, the scroll views can no longer take these touches and the view becomes the responder (`onResponderGrant`, then the moves); when it fails, the scroll view scrolls as usual. `PanResponder`'s `panHandlers` work with it too: spread them next to the prop, and `onPanResponderGrant` runs at the activation.

> [!NOTE]
> Velocities differ by API: `PanResponder`'s `vx` / `vy` are points per millisecond, while `onPanEnd`'s `velocityX` / `velocityY` and the `velocity` option of `spring` / `decay` are points per second. Multiply `vx` by 1000 before passing it to a spring.

## Why some answers are props

The main thread never waits for JS. When the system needs an answer on the spot (may this pan gesture begin, does this point hit the view, should this tap dismiss the keyboard), NSH answers natively from props mounted beforehand: `pan` (its axes), `pinch`, `hitSlop`, `pointerEvents`, `keyboardShouldPersistTaps`, `panActivation`. Your JS handlers hear about what happened afterwards. That is why these are declarative props rather than callbacks, and why the responder system's negotiation, which does call JS, only decides between JS responders and never blocks a native gesture.

## Related

- [`View`](../elements/view.md): every touch prop on the basic element.
- [Animation](./animation.md): native values, springs and decay for gesture-driven motion.
- [Scrolling](./scrolling.md): `keyboardShouldPersistTaps`, scroll views and nested gestures.
- [`nsh/gesture`](../modules/nsh-gesture.md): the `PanResponder` reference.
- [Accessibility](./accessibility.md): pressable views are accessibility elements; give them a role and a label.

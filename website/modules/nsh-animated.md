---
title: nsh/animated
description: Animated values, animation drivers, composed animations, easings and layout animation.
order: 3
demo: mod-animated
---

# nsh/animated

`nsh/animated` animates on the UI thread. You create a **animated value**, bind it (or values derived from it) to a style property, and start an animation; from then on every frame is computed natively, without running JavaScript. A busy JS thread cannot make the animation stutter, and an animation costs no JS work per frame.

```ts
import { Animated, createAnimatedValue, Easing, LayoutAnimation, AnimatedNode, AnimatedValue } from "nsh/animated";
```

See [Animation](../docs/animation.md) for the guide.

## What an animated value can drive

| Where | How |
|---|---|
| `opacity` | `style={{ opacity: value }}` |
| `backgroundColor` | `style={{ backgroundColor: value.interpolate({ inputRange: [0, 1], outputRange: ["#fff", "#000"] }) }}` |
| `borderColor`, `color` | The same: a node of colors. `borderColor` colors every side; a `Text`'s `color` colors the whole text |
| `shadowColor`, `shadowOpacity`, `shadowRadius` | `style={{ shadowOpacity: value }}` (a node of colors for `shadowColor`) |
| `transform` | `translateX`, `translateY`, `scale`, `scaleX`, `scaleY`, `rotate`, `rotateX`, `rotateY`, `rotateZ` steps: `transform: [{ translateX: value }]` |
| Scroll position | `<Scroll scrollValue={value}>` and `<List scrollValue={value}>` write the scroll offset into it each frame |
| Gestures | `pan={{ x, y }}`, `pinch={{ scale }}`, `rotation={{ angle }}` on any element: the gesture writes the values directly (see [Touch and gestures](../docs/touch-and-gestures.md)) |
| Canvas | Any number or color argument of a [canvas](./nsh-canvas.md) drawing call |

Other style properties take plain values; to animate the layout itself (sizes, positions in the flow), use [`LayoutAnimation`](#layoutanimation).

## createAnimatedValue

```ts
function createAnimatedValue(value?: number): AnimatedValue;
```

Creates an animated value starting at `value` (default `0`).

```tsx
import { mount, Text, View } from "nsh";
import { createAnimatedValue } from "nsh/animated";

function App() {
  const x = createAnimatedValue(0);
  let right = false;
  const toggle = () => {
    right = !right;
    x.spring(right ? 200 : 0);
  };
  return (
    <View style={{ flex: 1, padding: 24, paddingTop: 48, gap: 16 }}>
      <View style={{ width: 60, height: 60, borderRadius: 12, backgroundColor: "#6366f1", transform: [{ translateX: x }] }} />
      <Text onPress={toggle}>Move</Text>
    </View>
  );
}

mount(App);
```

> [!NOTE]
> An animated value lives on the UI thread and changes there every frame, so JS reads it asynchronously: `await value.getValue()` (or the promise `stop()` returns) gives the value at that moment. Keep the target in a variable or a signal when your code needs it right away, as above. Gesture events report the values at their end (`PanEvent.valueX` / `valueY`).

## AnimatedValue

```ts
class AnimatedValue extends AnimatedNode {
  constructor(value: number);
  set(value: number): void;
  stop(): Promise<number>;
  getValue(): Promise<number>;
  setOffset(offset: number): void;
  flattenOffset(): void;
  extractOffset(): void;
  timing(to: number, options?: { duration?: number; easing?: Easing; delay?: number }): Promise<boolean>;
  spring(to: number, options?: { stiffness?: number; damping?: number; mass?: number; velocity?: number }): Promise<boolean>;
  decay(options?: { velocity?: number; deceleration?: number; clamp?: [number, number]; bounce?: number }): Promise<boolean>;
}
```

| Method | Description |
|---|---|
| `set(value)` | Jumps to `value`, stopping any running animation. |
| `stop()` | Stops the running animation where it is; resolves with the value there. |
| `getValue()` | Resolves with the current value (with its offset), read on the UI thread. |
| `setOffset(offset)` | An offset added to the value: bindings see value + offset; `set`, animations and drags move the value. |
| `flattenOffset()` | Adds the offset to the value and makes it 0. |
| `extractOffset()` | Moves the value into the offset and makes the value 0 (a drag then starts from where the last one ended). |
| `timing(to, options)` | Animates to `to` along an easing curve. `duration` in ms (default 300), `easing` (default `"easeInOut"`), `delay` in ms (default 0). |
| `spring(to, options)` | Springs to `to`. `stiffness` (default 170), `damping` (default 26), `mass` (default 1), `velocity` (initial velocity, units per second). |
| `decay(options)` | Coasts from `velocity` (units per second) and slows down by `deceleration` per millisecond (default 0.998). Use it after a fling, with the velocity from `onPanEnd`. With `clamp: [min, max]` it stops at a bound, or bounces back from it keeping `bounce` (0..1, default 0) of its speed. |

Each animation method returns a promise that resolves with `true` when the animation finished and `false` when it was stopped (by `stop`, `set` or another animation of the same value).

```ts
const opacity = createAnimatedValue(1);
const finished = await opacity.timing(0, { duration: 200 });
if (finished) visible.set(false);
```

## AnimatedNode

```ts
class AnimatedNode {
  interpolate(config: {
    inputRange: number[];
    outputRange: number[] | string[];
    extrapolate?: Extrapolation;
    extrapolateLeft?: Extrapolation;
    extrapolateRight?: Extrapolation;
    easing?: Easing;
  }): AnimatedNode;
  diffClamp(min: number, max: number): AnimatedNode;
  add(b: AnimatedNode | number): AnimatedNode;
  sub(b: AnimatedNode | number): AnimatedNode;
  mul(b: AnimatedNode | number): AnimatedNode;
  div(b: AnimatedNode | number): AnimatedNode;
  mod(b: AnimatedNode | number): AnimatedNode;
}

type Extrapolation = "extend" | "clamp" | "identity";
```

A node of the native animation graph: a `AnimatedValue`, or a value derived from others. Derived nodes are evaluated on the UI thread whenever their inputs change.

| Method | Description |
|---|---|
| `interpolate(config)` | Maps the input through piecewise ranges. `outputRange` holds numbers, colors (`"#ff0000"`, `"rgba(…)"`) or angles (`"45deg"`, `"1rad"`). Outside `inputRange` it clamps by default; `extrapolate` (or `extrapolateLeft` / `extrapolateRight`) can be `"extend"` (continue the line), `"clamp"` or `"identity"` (pass the input through). `easing` shapes each segment. |
| `diffClamp(min, max)` | Follows the input's changes but keeps the result within `min..max`: a header that hides while content scrolls up and comes back as soon as it scrolls down. |
| `add`, `sub`, `mul`, `div` | Arithmetic with another node or a constant. |
| `mod(b)` | Modulo, never negative for a positive `b` (keeping angles in 0..360). |

```tsx
const scroll = createAnimatedValue(0);
const header = scroll.diffClamp(0, 44);

<Scroll scrollValue={scroll} style={{ flex: 1 }}>{/* rows */}</Scroll>
<View style={{ height: 44, transform: [{ translateY: header.mul(-1) }] }} />
```

```ts
const progress = createAnimatedValue(0);
const rotate = progress.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "360deg"] });
const color = progress.interpolate({ inputRange: [0, 0.5, 1], outputRange: ["#22c55e", "#eab308", "#ef4444"] });
```

## Animated

```ts
const Animated: {
  timing(value: AnimatedValue, config: { toValue: number; duration?: number; easing?: Easing; delay?: number }): CompositeAnimation;
  spring(value: AnimatedValue, config: { toValue: number; stiffness?: number; damping?: number; mass?: number; velocity?: number }): CompositeAnimation;
  decay(value: AnimatedValue, config?: { velocity?: number; deceleration?: number; clamp?: [number, number]; bounce?: number }): CompositeAnimation;
  sequence(list: CompositeAnimation[]): CompositeAnimation;
  parallel(list: CompositeAnimation[], config?: { stopTogether?: boolean }): CompositeAnimation;
  stagger(ms: number, list: CompositeAnimation[]): CompositeAnimation;
  delay(ms: number): CompositeAnimation;
  loop(animation: CompositeAnimation, config?: { iterations?: number; resetBeforeIteration?: boolean }): CompositeAnimation;
  add(a: AnimatedNode, b: AnimatedNode | number): AnimatedNode;
  subtract(a: AnimatedNode, b: AnimatedNode | number): AnimatedNode;
  multiply(a: AnimatedNode, b: AnimatedNode | number): AnimatedNode;
  divide(a: AnimatedNode, b: AnimatedNode | number): AnimatedNode;
  modulo(a: AnimatedNode, b: AnimatedNode | number): AnimatedNode;
  diffClamp(a: AnimatedNode, min: number, max: number): AnimatedNode;
  sampleSpring(config: SpringConfig & { from?: number }, fps?: number): number[];
  sampleDecay(config: DecayConfig & { from?: number }, fps?: number): number[];
};

interface CompositeAnimation {
  start(callback?: (result: { finished: boolean }) => void): Promise<boolean>;
  stop(): void;
  reset(): void;
}
```

`Animated` builds animations out of steps (`timing`, `spring`, `decay`, `sequence`, `parallel`, ...). Nothing runs until `start()`; then the whole tree runs natively, and each step starts on the UI thread at the exact moment the previous one ends (no frame gap, no JS round trip between steps).

| Function | Description |
|---|---|
| `timing`, `spring`, `decay` | One animation of one value; same options and defaults as the [`AnimatedValue`](#animatedvalue) methods, with the target as `toValue`. |
| `sequence(list)` | Runs the animations one after another. |
| `parallel(list, { stopTogether })` | Runs them at the same time. With `stopTogether` (default `true`), stopping one stops all. |
| `stagger(ms, list)` | Runs them in parallel, each starting `ms` after the previous one. |
| `delay(ms)` | Waits; useful inside `sequence`. |
| `loop(animation, { iterations, resetBeforeIteration })` | Repeats `animation` `iterations` times (forever by default), putting its values back before each run unless `resetBeforeIteration` is `false`. |
| `add`, `subtract`, `multiply`, `divide`, `modulo`, `diffClamp` | The same as the [`AnimatedNode`](#animatednode) methods, as functions. |

`CompositeAnimation`:

| Method | Description |
|---|---|
| `start(callback?)` | Runs it (again). Resolves, and calls `callback`, once for the whole tree with whether it finished rather than being stopped. |
| `stop()` | Stops it where it is. |
| `reset()` | Stops it and puts its values back where they were when it started. |

```ts
const ys = [0, 1, 2, 3].map(() => createAnimatedValue(0));
const hop = Animated.stagger(
  80,
  ys.map((y) =>
    Animated.sequence([
      Animated.timing(y, { toValue: -36, duration: 220, easing: Easing.out(Easing.quad) }),
      Animated.spring(y, { toValue: 0, stiffness: 300, damping: 12 }),
    ]),
  ),
);
hop.start(({ finished }) => console.log(finished ? "done" : "stopped"));
```

## Easing

```ts
type Easing =
  | "linear" | "ease" | "easeIn" | "easeOut" | "easeInOut"
  | [number, number, number, number]   // cubic Bézier x1, y1, x2, y2
  | EasingCurve;

const Easing: {
  linear: Easing;
  ease: Easing;
  bezier(x1: number, y1: number, x2: number, y2: number): Easing;
  in(e: Easing): Easing;
  out(e: Easing): Easing;
  inOut(e: Easing): Easing;
  sample(easing: Easing, count: number): number[];
  quad: EasingCurve;
  cubic: EasingCurve;
  poly(n?: number): EasingCurve;
  sin: EasingCurve;
  circle: EasingCurve;
  exp: EasingCurve;
  elastic(bounciness?: number): EasingCurve;
  back(s?: number): EasingCurve;
  bounce: EasingCurve;
};
```

An easing is data, not a JS function: the runtime evaluates it natively. To draw one, `Easing.sample(easing, count)` gives its values at `count` evenly spaced points from 0 to 1; `Animated.sampleSpring` and `Animated.sampleDecay` give a spring's or decay's values one per frame (at `fps`, 60 by default) from `from` (0) until it ends. All three come from the native code the animations run. Wherever an `Easing` is accepted you can pass a name (`"easeOut"`), a cubic Bézier as four numbers, or a curve from the `Easing` object.

| Curve | Shape |
|---|---|
| `quad` | `t²` |
| `cubic` | `t³` |
| `poly(n)` | `tⁿ` (`n` defaults to 2) |
| `sin` | Sinusoidal |
| `circle` | Circular |
| `exp` | Exponential |
| `elastic(bounciness)` | Springy overshoot (`bounciness` defaults to 1) |
| `back(s)` | Pulls back before going; `s` is the overshoot (default 1.70158) |
| `bounce` | Bounces at the end |

The curves are in their "in" form (slow at the start). `Easing.out(curve)` reverses one, `Easing.inOut(curve)` makes it symmetric, `Easing.in(curve)` keeps it as is.

```ts
value.timing(1, { duration: 600, easing: Easing.out(Easing.back()) });
value.timing(1, { easing: Easing.bezier(0.2, 0, 0, 1) });
value.timing(1, { easing: "easeOut" });
```

## LayoutAnimation

```ts
const LayoutAnimation: {
  configureNext(config: LayoutAnimationConfig, onEnd?: () => void): void;
  create(duration: number, type?: LayoutAnimationType, property?: LayoutAnimationProperty): LayoutAnimationConfig;
  Types: { easeInEaseOut; spring; linear; easeIn; easeOut };
  Properties: { opacity; scaleXY };
  Presets: { easeInEaseOut: LayoutAnimationConfig; linear: LayoutAnimationConfig; spring: LayoutAnimationConfig };
  easeInEaseOut(onEnd?: () => void): void;
  linear(onEnd?: () => void): void;
  spring(onEnd?: () => void): void;
};

interface LayoutAnimationConfig {
  duration: number;                // ms
  create?: LayoutAnimationAnimation;    // views inserted
  update?: LayoutAnimationAnimation;    // views moved or resized
  delete?: LayoutAnimationAnimation;    // views removed
}

interface LayoutAnimationAnimation {
  type?: "easeInEaseOut" | "spring" | "linear" | "easeIn" | "easeOut";
  property?: "opacity" | "scaleXY" | "scale";
  duration?: number;
  delay?: number;
  springDamping?: number;          // 0..1, default 0.7
}
```

`LayoutAnimation.configureNext(config)` animates the layout changes of the next commit natively: views that move or resize glide to their new frames, new views appear (fading or scaling in, by `property`) and removed views disappear. Call it right before the state change that changes the layout.

| Member | Description |
|---|---|
| `configureNext(config, onEnd?)` | Animates the next commit's changes; `onEnd` runs when done. |
| `create(duration, type?, property?)` | A config using the same curve for all three phases. |
| `Presets.easeInEaseOut` | 300 ms, views fading in and out. |
| `Presets.linear` | 500 ms at constant speed. |
| `Presets.spring` | 700 ms; frames spring (damping 0.4), views fade linearly. |
| `easeInEaseOut(onEnd?)`, `linear(onEnd?)`, `spring(onEnd?)` | Shorthands for `configureNext(Presets.…)`. |
| `Types`, `Properties` | The names, as constants. |

```tsx
const open = signal(false);
const toggle = () => {
  LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
  open.update((o) => !o);
};

<View>
  <Text onPress={toggle}>Details</Text>
  <If when={open()}>
    <Text>Shown with a spring.</Text>
  </If>
</View>
```

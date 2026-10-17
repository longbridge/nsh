---
title: Animation
description: Style transitions and keyframe animations, native values with timing, spring and decay, interpolation, composite animations, Easing, LayoutAnimation, and what runs on the UI thread.
order: 43
group: ui
demo: guide-animation
---

# Animation

Every animation in NSH runs on the UI thread. You describe it from JS, once, and the platform plays it frame by frame (Core Animation and a display link on iOS, the UI-thread animator on Android) without calling back into JS. A busy JS thread cannot make an animation stutter, and an animation costs no JS while it runs.

There are four tools, from the simplest to the most flexible:

| Tool | Use it for |
|---|---|
| [Transitions](#transitions) | A style change that should animate instead of jump: a color, an opacity, a transform. |
| [Keyframe animations](#keyframe-animations) | A spinner, a pulsing placeholder, a bouncing hint. |
| [Native values](#native-values) | Anything driven by a value over time: springs, gestures, scroll position, sequences. |
| [LayoutAnimation](#layoutanimation) | Size and position changes after a re-render: items inserted, removed, reordered, a panel expanding. |

## Transitions

Give a view `transitionProperty` and `transitionDuration`, and later changes to those properties animate natively from what is on screen to the new value.

```tsx
import { mount, signal, View } from "nsh";

function App() {
  const on = signal(false);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
      <View
        onPress={() => on.set(!on())}
        style={{
          width: 100,
          height: 100,
          borderRadius: on() ? 50 : 16,
          backgroundColor: on() ? "#8250df" : "#0969da",
          transform: [{ rotate: on() ? "45deg" : "0deg" }, { scale: on() ? 1.2 : 1 }],
          transitionProperty: "all",
          transitionDuration: 400,
          transitionTimingFunction: "ease-in-out",
        }}
      />
    </View>
  );
}

mount(App);
```

| Style | Type | Description |
|---|---|---|
| `transitionProperty` | `string` | What animates, comma-separated: `"all"`, `"colors"`, `"opacity"`, `"transform"`, `"box-shadow"`, `"border-radius"`, ... |
| `transitionDuration` | `number \| "150ms" \| "0.2s"` | Milliseconds as a number, or a CSS time. |
| `transitionDelay` | `number \| "150ms" \| "0.2s"` | Before it starts. |
| `transitionTimingFunction` | `"linear" \| "ease" \| "ease-in" \| "ease-out" \| "ease-in-out" \| "cubic-bezier(…)"` | The curve. |

Properties that transition, by group: **colors** (background, border, outline, shadow and text colors, gradient stops), **opacity**, **transform**, **shadows** (`box-shadow`) and **shape** (border radius and width). CSS names and their camel-case forms both work (`background-color`, `backgroundColor`, `transform`, `scale`, ...). Layout does not transition: for size and position changes use [LayoutAnimation](#layoutanimation).

With Tailwind, the same thing is `transition` (colors, opacity, shadows and transforms), `transition-all`, `transition-colors`, `transition-opacity`, `transition-transform`, `transition-shadow`, with `duration-*`, `delay-*` and `ease-*`. Combined with the `active:` variant it gives pressed states that animate:

```tsx
<View onPress={save} class="rounded-xl bg-blue-600 px-5 py-3 transition duration-150 active:scale-95 active:bg-blue-800">
  <Text class="font-semibold text-white">Save</Text>
</View>
```

A transition starts from the value currently on screen, so changing the target again mid-way turns smoothly.

## Keyframe animations

Four built-in keyframe animations run natively, forever by default: `spin` (one turn clockwise), `ping` (scales up to 2× and fades out), `pulse` (fades to half opacity and back) and `bounce` (jumps up by a quarter of its height and lands).

```tsx
<View class="size-8 rounded-full border-4 border-zinc-300 border-t-blue-600 animate-spin" />
<View class="h-4 w-40 rounded bg-zinc-200 animate-pulse" />
```

| Style | Type | Description |
|---|---|---|
| `animationName` | `"none" \| "spin" \| "ping" \| "pulse" \| "bounce"` | The animation. |
| `animationDuration` | `number \| "1s" \| …` | One iteration. |
| `animationDelay` | `number \| "200ms" \| …` | Before it starts. |
| `animationTimingFunction` | as `transitionTimingFunction` | The curve. |
| `animationIterationCount` | `number \| "infinite"` | Default `"infinite"`. |

Tailwind: `animate-spin`, `animate-ping`, `animate-pulse`, `animate-bounce`, `animate-none`.

## Native values

A native value is a number that lives on the UI thread. Create one with `createNativeValue` from `nsh/animated`, bind it to styles, and animate it; the bound views follow it every frame without re-rendering.

```tsx
import { mount, Text, View } from "nsh";
import { createNativeValue, Easing } from "nsh/animated";

function App() {
  const x = createNativeValue(0);
  const opacity = createNativeValue(1);
  return (
    <View style={{ flex: 1, justifyContent: "center", padding: 24, gap: 24 }}>
      <View style={{ width: 60, height: 60, borderRadius: 12, backgroundColor: "#0969da", opacity, transform: [{ translateX: x }] }} />
      <View
        onPress={async () => {
          await x.timing(240, { duration: 600, easing: Easing.out(Easing.cubic) });
          await opacity.timing(0.3, { duration: 200 });
          x.spring(0);
          opacity.timing(1);
        }}
      >
        <Text style={{ color: "#0969da" }}>Play</Text>
      </View>
    </View>
  );
}

mount(App);
```

**What can be bound:** `opacity`; `backgroundColor`, `borderColor` (every side) and a `Text`'s `color` (the whole text), through an interpolation to colors; `shadowColor`, `shadowOpacity` and `shadowRadius`; and the `translateX`, `translateY`, `scale`, `scaleX`, `scaleY`, `rotate`, `rotateX`, `rotateY` and `rotateZ` steps of `transform`. A focus ring or a price that flashes green is a color interpolation bound to `borderColor` or `color`:

```tsx
const flash = createNativeValue(0);
const color = flash.interpolate({ inputRange: [0, 1], outputRange: ["#1f2328", "#1a7f37"] });

<Text style={{ color }}>{price()}</Text>
// On a new price: flash.set(1); flash.timing(0, { duration: 600 });
``` Native values also drive [canvas drawings](./canvas.md#animating-with-native-values), receive scroll offsets ([`scrollValue`](./scrolling.md#scroll-linked-effects-scrollvalue)) and gestures ([`pan`, `pinch`, `rotation`](./touch-and-gestures.md#native-gestures-pan-pinch-and-rotation)).

The value is kept alive by the views bound to it, so a binding never breaks because a JS variable went out of scope. Create native values in the component that owns them (not at module level when every instance needs its own).

### NativeValue methods

| Method | Description |
|---|---|
| `set(value)` | Jumps to `value`, stopping any animation. |
| `stop()` | Stops the running animation where it is; resolves with the value there. |
| `getValue()` | A promise of the current value, read on the UI thread where it moves (it settles within a frame). |
| `setOffset(offset)` | Adds `offset` to what the value shows: bindings see value + offset, while `set`, animations and drags move the value. |
| `flattenOffset()` | Adds the offset to the value and makes the offset 0. |
| `extractOffset()` | Moves the value into the offset and makes the value 0: a drag then starts from 0, on top of where the last one left it. |
| `timing(to, { duration?, easing?, delay? })` | Animates to `to` over `duration` ms (default 300, easing `"easeInOut"`). |
| `spring(to, { stiffness?, damping?, mass?, velocity? })` | Springs to `to` (defaults: stiffness 170, damping 26, mass 1). |
| `decay({ velocity?, deceleration?, clamp?, bounce? })` | Coasts from `velocity` (units per second) and slows down (deceleration 0.998 per ms by default). With `clamp: [min, max]` it stops at a bound, or bounces back from it keeping `bounce` (0..1) of its speed. |

Each animation returns a `Promise<boolean>`: `true` when it finished, `false` when something stopped it (`stop()`, `set()`, or a new animation on the same value). Starting a new animation on a moving value interrupts the old one smoothly: a `spring` or `decay` without `velocity` continues with the value's current velocity.

```tsx
// Spring settings: lower damping overshoots more, higher stiffness is faster.
x.spring(200, { stiffness: 180, damping: 8 });   // wobbly
x.spring(200, { stiffness: 400, damping: 30 });  // stiff, no overshoot

// A fling that coasts to a stop, from a pan's release velocity.
<View pan={{ x }} onPanEnd={(e) => x.decay({ velocity: e.velocityX })} />

// A fling that stays on its track and bounces softly off its ends.
x.decay({ velocity: e.velocityX, clamp: [0, 300], bounce: 0.3 });

// Where a value is now (it moves on the UI thread, so the answer is async).
const where = await x.getValue();
```

### Interpolation

`value.interpolate({ inputRange, outputRange })` maps a value through piecewise linear segments to numbers, colors or angles, and returns a new node you can bind. One value can drive many properties.

```tsx
const t = createNativeValue(0);

<View
  style={{
    backgroundColor: t.interpolate({ inputRange: [0, 1, 2], outputRange: ["#54aeff", "#8250df", "#cf222e"] }),
    opacity: t.interpolate({ inputRange: [0, 1], outputRange: [0.4, 1] }),
    transform: [
      { rotate: t.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "180deg"] }) },
      { scale: t.interpolate({ inputRange: [0, 1], outputRange: [0.6, 1.2], easing: Easing.out(Easing.back()) }) },
    ],
  }}
/>
```

| Option | Description |
|---|---|
| `inputRange` | Increasing input stops. |
| `outputRange` | One output per stop: numbers, colors (`"#rrggbb"`, `"rgba(…)"`, named) or angles (`"45deg"`, `"1rad"`). |
| `extrapolate` | Outside the input range: `"clamp"` (default), `"extend"` (continue the end segments) or `"identity"` (pass the input through). |
| `extrapolateLeft`, `extrapolateRight` | The same, for one side. |
| `easing` | Shapes each segment. |

> [!IMPORTANT]
> Interpolations **clamp** by default: outside the input range, the output stays at the nearest end. Pulling a scroll view past its top therefore does not push a scroll-linked header beyond its end state. Pass `extrapolate: "extend"` when you want the line to continue.

### Arithmetic and diffClamp

Nodes combine with arithmetic, also natively: `a.add(b)`, `a.sub(b)`, `a.mul(b)`, `a.div(b)`, `a.mod(b)` (never negative for a positive `b`, e.g. angles in 0..360), where `b` is a node or a number. `Animated.add`, `subtract`, `multiply`, `divide` and `modulo` are the same as functions.

`node.diffClamp(min, max)` follows the node's *changes* while staying within `min..max`. Applied to a scroll offset, it makes a header that hides as the content scrolls up and comes back as soon as it scrolls down:

```tsx
const scroll = createNativeValue(0);
const header = scroll.diffClamp(0, 56);

<Scroll style={{ flex: 1 }} scrollValue={scroll}>…</Scroll>
<View style={{ position: "absolute", top: 0, left: 0, right: 0, height: 56, transform: [{ translateY: header.mul(-1) }] }}>…</View>
```

## Composite animations

`Animated` builds animations out of steps and runs the whole tree natively: each step starts on the UI thread the moment the previous one ends, with no gap and no JS in between.

```tsx
import { Animated, createNativeValue, Easing } from "nsh/animated";

const ys = [0, 1, 2, 3, 4].map(() => createNativeValue(0));
const fade = createNativeValue(1);

const hop = Animated.stagger(
  80,
  ys.map((y) =>
    Animated.sequence([
      Animated.timing(y, { toValue: -30, duration: 200, easing: Easing.out(Easing.quad) }),
      Animated.spring(y, { toValue: 0, stiffness: 300, damping: 12 }),
    ]),
  ),
);

const breathe = Animated.loop(
  Animated.sequence([
    Animated.timing(fade, { toValue: 0.3, duration: 400 }),
    Animated.timing(fade, { toValue: 1, duration: 400 }),
  ]),
);

hop.start(({ finished }) => console.log(finished ? "landed" : "stopped"));
```

| Function | Description |
|---|---|
| `Animated.timing(value, { toValue, duration?, easing?, delay? })` | A timing step. |
| `Animated.spring(value, { toValue, stiffness?, damping?, mass?, velocity? })` | A spring step. |
| `Animated.decay(value, { velocity?, deceleration?, clamp?, bounce? })` | A decay step. |
| `Animated.sequence(list)` | One after another. |
| `Animated.parallel(list, { stopTogether? })` | All at once. |
| `Animated.stagger(ms, list)` | All at once, each starting `ms` after the previous one. |
| `Animated.delay(ms)` | Waits. |
| `Animated.loop(animation, { iterations?, resetBeforeIteration? })` | Repeats (forever by default). |

A `CompositeAnimation` has `start(callback?)` (also a promise of `finished`; calling it again runs it again), `stop()` (stops where it is) and `reset()` (stops and puts its values back where they were when it started). The callback runs once for the whole tree.

A running animation does not keep its values alive: when the component that owns them goes (its views and variables), an endless `Animated.loop` stops with them, and its promise resolves with `false`.

## Easing

`Easing` from `nsh/animated` gives the standard easing curves. They are data, evaluated natively, not JS functions.

| Easing | Description |
|---|---|
| `Easing.linear`, `Easing.ease` | Constant speed; CSS's `ease`. |
| `Easing.quad`, `Easing.cubic`, `Easing.poly(n)` | `t²`, `t³`, `tⁿ`. |
| `Easing.sin`, `Easing.circle`, `Easing.exp` | Sinusoidal, circular, exponential. |
| `Easing.back(s?)` | Pulls back before going (overshoot `s`, 1.70158 by default). |
| `Easing.elastic(bounciness?)` | Springy overshoot. |
| `Easing.bounce` | Bounces at the end. |
| `Easing.bezier(x1, y1, x2, y2)` | A cubic Bézier, as in CSS. |
| `Easing.in(e)`, `Easing.out(e)`, `Easing.inOut(e)` | The curves are "in" curves; `out` and `inOut` turn them around. |

Anywhere an easing is accepted you can also pass a name (`"linear"`, `"ease"`, `"easeIn"`, `"easeOut"`, `"easeInOut"`) or a Bézier array `[x1, y1, x2, y2]`.

```tsx
x.timing(1, { easing: Easing.out(Easing.back()) });
x.timing(1, { easing: "easeOut" });
x.timing(1, { easing: [0.2, 0.8, 0.2, 1] });
```

`Easing.sample(easing, count)` returns the curve at `count` evenly spaced points from 0 to 1, and `Animated.sampleSpring(config, fps?)` / `Animated.sampleDecay(config, fps?)` the values a spring or decay moves through, one per frame (60 per second by default) from `config.from` (0) until it ends. They are computed by the same native code the animations run, so a chart of a curve shows exactly what the animation does:

```ts
const curve = Easing.sample(Easing.out(Easing.back()), 50); // 50 progress values
const spring = Animated.sampleSpring({ toValue: 1, stiffness: 180, damping: 8 });
```

## LayoutAnimation

`LayoutAnimation.configureNext(config)` animates the frame changes of the **next** update natively: views that move or resize glide to their new frames, new views fade (or scale) in, removed views fade out. Call it right before the state change.

```tsx
import { signal } from "nsh";
import { LayoutAnimation } from "nsh/animated";

const items = signal([1, 2, 3]);
let next = 4;

const add = () => {
  LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
  items.set([next++, ...items()]);
};
const remove = () => {
  LayoutAnimation.easeInEaseOut();
  items.set(items().slice(1));
};
```

| Member | Description |
|---|---|
| `configureNext(config, onEnd?)` | Animates the next commit's changes; `onEnd` runs when done. |
| `create(duration, type?, property?)` | A config with one curve for all three phases. |
| `Presets.easeInEaseOut`, `Presets.linear`, `Presets.spring` | Ready-made configs. |
| `easeInEaseOut(onEnd?)`, `linear(onEnd?)`, `spring(onEnd?)` | `configureNext` with a preset. |
| `Types`, `Properties` | The names, for building configs. |

A config is `{ duration, create?, update?, delete? }`: `create` for inserted views, `update` for moved or resized ones, `delete` for removed ones. Each phase is `{ type?, property?, duration?, delay?, springDamping? }`, where `type` is `"easeInEaseOut"`, `"spring"`, `"linear"`, `"easeIn"` or `"easeOut"`, `property` is what appearing and disappearing views animate (`"opacity"`, `"scaleXY"`, `"scale"`), and `springDamping` (0..1, 0.7 by default) sets the spring's overshoot.

```tsx
LayoutAnimation.configureNext({
  duration: 300,
  create: { type: "easeOut", property: "opacity" },
  update: { type: "spring", springDamping: 0.8 },
  delete: { type: "easeIn", property: "scaleXY" },
});
```

## What runs where

| | Runs on | JS per frame |
|---|---|---|
| Transitions, keyframe animations | UI thread | none |
| `timing` / `spring` / `decay`, composites | UI thread | none; JS hears when they end |
| `interpolate`, arithmetic, `diffClamp` | UI thread | none |
| `scrollValue`, `pan` / `pinch` / `rotation` | UI thread | none; JS hears start and end |
| LayoutAnimation | UI thread | none |
| `requestAnimationFrame` loops, `onScroll` handlers, `PanResponder` | JS, then a re-render | every frame |

Reach for `requestAnimationFrame` (a global, following the display) only for what cannot be described ahead of time, such as a physics simulation in JS. Anything that can be a native value should be one.

## Reduced motion

When the user asks for less motion (iOS "Reduce Motion", Android "Remove animations"), `reduceMotion()` from `nsh/accessibility` is `true` and the `motion-reduce:` / `motion-safe:` Tailwind variants apply. NSH does not turn your animations off by itself; use them to choose a calmer alternative:

```tsx
import { View } from "nsh";
import { reduceMotion } from "nsh/accessibility";

const open = () => (reduceMotion() ? panel.set(0) : panel.spring(0));

<View class="motion-safe:animate-bounce" />
```

See [Accessibility](./accessibility.md#reduced-motion).

## Related

- [`nsh/animated`](../modules/nsh-animated.md): the reference.
- [Touch and gestures](./touch-and-gestures.md): driving native values with the finger.
- [Scrolling](./scrolling.md#scroll-linked-effects-scrollvalue): `scrollValue`.
- [Canvas](./canvas.md#animating-with-native-values): native values in drawings.
- [Style reference](../modules/style-reference.md): the `transition*` and `animation*` keys.

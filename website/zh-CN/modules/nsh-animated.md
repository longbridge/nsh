---
title: nsh/animated
description: 动画值、动画驱动、组合动画、缓动曲线和布局动画。
order: 3
demo: mod-animated
---

# nsh/animated

动画 API 在 UI 线程上执行动画。你创建一个**动画值**（animated value），把它（或由它派生的值）绑定到样式属性上，然后启动动画；此后每一帧都在原生侧计算，不需要运行应用的代码。引擎线程或 JS 线程再忙也不会让动画卡顿，动画的每一帧也不消耗应用任何开销。Rust 代码使用 `nsh_core::ui::prelude` 中的 `animated_value()`、`Animation`、`Spring`、`Timing` 和 `Easing`，用元素的 `.bind_*` 方法绑定动画值；TSX 导入 `nsh/animated`。

```rust tabs
use nsh_core::ui::{LayoutAnimation, prelude::*}; // animated_value、Animation、Spring、Timing、Easing、animate_layout
```

```tsx tabs
import { Animated, createAnimatedValue, Easing, LayoutAnimation, AnimatedNode, AnimatedValue } from "nsh/animated";
```

教程见[动画](../docs/animation.md)。

## 动画值可以驱动什么

| 位置 | Rust | TSX |
|---|---|---|
| `opacity` | `.bind_opacity(&value)` | `style={{ opacity: value }}` |
| `backgroundColor` | `.bind_background(&value.interpolate_colors(&[0.0, 1.0], &[white(), black()]))` | `style={{ backgroundColor: value.interpolate({ inputRange: [0, 1], outputRange: ["#fff", "#000"] }) }}` |
| `borderColor`、`color` | `.bind_border_color(&node)`、`.bind_color(&node)` | 同上：一个颜色节点。`borderColor` 为所有边着色；`Text` 的 `color` 为整段文本着色 |
| `shadowColor`、`shadowOpacity`、`shadowRadius` | `.bind_shadow_color(&node)`、`.bind_shadow_opacity(&value)`、`.bind_shadow_radius(&value)` | `style={{ shadowOpacity: value }}`（`shadowColor` 用颜色节点） |
| `transform` | `.bind_translate_x(&value)`、`.bind_translate_y`、`.bind_scale`、`.bind_rotate`（角度） | `translateX`、`translateY`、`scale`、`scaleX`、`scaleY`、`rotate`、`rotateX`、`rotateY`、`rotateZ`：`transform: [{ translateX: value }]` |
| 滚动位置 | 滚动视图或列表上的 `.scroll_value(&value)` | `<Scroll scrollValue={value}>` 和 `<List scrollValue={value}>` 每帧把滚动偏移写入该值 |
| 手势 | `.pan_values(Some(&x), Some(&y))`、`.pinch_value(&scale)`、`.rotation_value(&angle)` | 任意元素上的 `pan={{ x, y }}`、`pinch={{ scale }}`、`rotation={{ angle }}`：手势直接写入这些值（见[触摸与手势](../docs/touch-and-gestures.md)） |
| 画布 | 绘制调用中的动画参数 | [画布](./nsh-canvas.md)绘制调用中的任意数字或颜色参数 |

其他样式属性只接受普通值；如果要为布局本身（尺寸、在布局流中的位置）添加动画，请使用 [`LayoutAnimation`](#layoutanimation)。

## createAnimatedValue

```ts
function createAnimatedValue(value?: number): AnimatedValue;
```

创建一个初始值为 `value`（默认 `0`）的动画值。Rust：`animated_value(value)`，一个由创建它的组件持有的 `Copy` 句柄。

```rust tabs
use nsh_core::ui::prelude::*;

#[component]
pub fn App() -> impl IntoElement {
    let x = animated_value(0.0);
    let right = stored(false);
    let toggle = move || {
        let to_right = !right.get();
        right.replace(to_right);
        let _ = x.spring(if to_right { 200.0 } else { 0.0 }, Spring::default());
    };
    view()
        .flex_1()
        .p_6()
        .pt_12()
        .gap_4()
        .child(view().size(px(60.0)).rounded(px(12.0)).bg(rgb(0x6366f1)).bind_translate_x(&x))
        .child(text("Move").on_press(toggle))
}
```

```tsx tabs
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
> 动画值存在于 UI 线程，每帧都在那里变化，所以应用异步读取它：`await value.getValue()`（或 `stop()` 返回的 Promise）给出那一刻的值，Rust 中在任务里 `value.value().await`。如果代码需要立刻知道目标值，请像上例一样用变量或 signal 自己记录。手势事件会在结束时报告这些值（`PanEvent.valueX` / `valueY`）。

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

| 方法 | Rust | 说明 |
|---|---|---|
| `set(value)` | `set(value)` | 直接跳到 `value`，并停止正在运行的动画。 |
| `stop()` | `stop()`，然后 `value().await` | 让正在运行的动画停在当前位置；resolve 为停下处的值。 |
| `getValue()` | `value().await` | resolve 为当前值（含偏移），在 UI 线程上读取。 |
| `setOffset(offset)` | `set_offset(offset)` | 加到值上的偏移：绑定看到的是值 + 偏移；`set`、动画和拖动改变的是值本身。 |
| `flattenOffset()` | `flatten_offset()` | 把偏移加到值上，并把偏移归 0。 |
| `extractOffset()` | `extract_offset()` | 把值移入偏移，并把值归 0（这样下一次拖动从上一次结束的位置开始）。 |
| `timing(to, options)` | `timing(to, ms, easing)`、`timing_with(to, Timing { … })` | 沿缓动曲线动画到 `to`。`duration` 单位为毫秒（默认 300），`easing`（默认 `"easeInOut"`），`delay` 单位为毫秒（默认 0）。 |
| `spring(to, options)` | `spring(to, Spring { … })` | 以弹簧效果运动到 `to`。`stiffness`（默认 170）、`damping`（默认 26）、`mass`（默认 1）、`velocity`（初速度，单位/秒）。 |
| `decay(options)` | `decay(velocity, deceleration)`、`decay_within(velocity, deceleration, (min, max), bounce)` | 以 `velocity`（单位/秒）为初速度滑行并逐渐减速，每毫秒保留 `deceleration`（默认 0.998）的速度。适合在快速滑动后使用 `onPanEnd` 给出的速度。设置 `clamp: [min, max]` 时，它在边界处停下，或以 `bounce`（0..1，默认 0）比例的速度反弹。 |

每个动画方法都返回一个 Promise：动画完成时 resolve 为 `true`，被停止（`stop`、`set` 或同一个值上的另一个动画）时为 `false`。在 Rust 中返回 `AnimationEnd`，在任务中 await 它得到同样的 `bool`。

```rust tabs
let opacity = animated_value(1.0);
spawn(async move {
    if opacity.timing(0.0, 200.0, Easing::EaseInOut).await {
        visible.set(false);
    }
});
```

```tsx tabs
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

原生动画图中的一个节点：一个 `AnimatedValue`，或由其他节点派生的值。派生节点在其输入变化时在 UI 线程上求值。

| 方法 | Rust | 说明 |
|---|---|---|
| `interpolate(config)` | `interpolate(&input, &output)`、`interpolate_colors(&input, &colors)`、`interpolate_with(&input, output, Interpolation { … })` | 按分段区间映射输入。`outputRange` 可以是数字、颜色（`"#ff0000"`、`"rgba(…)"`）或角度（`"45deg"`、`"1rad"`）。超出 `inputRange` 时默认钳制（clamp）；`extrapolate`（或 `extrapolateLeft` / `extrapolateRight`）可以是 `"extend"`（沿直线延伸）、`"clamp"` 或 `"identity"`（直接输出输入值）。`easing` 决定每一段的曲线。Rust 接受数字、主题颜色（跟随外观）以及供 `bind_rotate` 使用的角度（度）。 |
| `diffClamp(min, max)` | `diff_clamp(min, max)` | 跟随输入的变化量，但把结果限制在 `min..max` 内：内容向上滚动时隐藏、一旦向下滚动就重新出现的标题栏。 |
| `add`、`sub`、`mul`、`div` | `add`、`sub`、`mul`、`div` | 与另一个节点或常量做算术运算。 |
| `mod(b)` | `modulo(b)` | 取模，`b` 为正时结果不会为负（比如把角度保持在 0..360）。 |

```rust tabs
let scroll_y = animated_value(0.0);
let header = scroll_y.diff_clamp(0.0, 44.0);

view()
    .flex_1()
    .child(scroll().scroll_value(&scroll_y).flex_1() /* rows */)
    .child(view().h(px(44.0)).bind_translate_y(&header.mul(-1.0)))
```

```tsx tabs
const scroll = createAnimatedValue(0);
const header = scroll.diffClamp(0, 44);

<Scroll scrollValue={scroll} style={{ flex: 1 }}>{/* rows */}</Scroll>
<View style={{ height: 44, transform: [{ translateY: header.mul(-1) }] }} />
```

```rust tabs
let progress = animated_value(0.0);
let rotate = progress.interpolate(&[0.0, 1.0], &[0.0, 360.0]); // 角度，供 bind_rotate 使用
let color = progress.interpolate_colors(&[0.0, 0.5, 1.0], &[rgb(0x22c55e), rgb(0xeab308), rgb(0xef4444)]);
```

```tsx tabs
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

`Animated` 用一个个步骤（`timing`、`spring`、`decay`、`sequence`、`parallel` 等）组合出动画。调用 `start()` 之前什么都不会运行；启动后整棵动画树都在原生侧运行，每一步在上一步结束的那一刻于 UI 线程上开始（步骤之间没有帧间隙，也不需要回到应用的代码）。Rust 用 `Animation` 组合同样的动画树。

| 函数 | Rust | 说明 |
|---|---|---|
| `timing`、`spring`、`decay` | `Animation::timing(&value, to, Timing)`、`Animation::spring(&value, to, Spring)`、`Animation::decay(&value, velocity, deceleration)` | 单个值的单个动画；选项和默认值与 [`AnimatedValue`](#animatedvalue) 的同名方法相同，目标值写作 `toValue`。 |
| `sequence(list)` | `Animation::sequence(list)` | 依次运行这些动画。 |
| `parallel(list, { stopTogether })` | `Animation::parallel(list).stop_together(…)` | 同时运行。`stopTogether`（默认 `true`）时，停止其中一个会停止全部。 |
| `stagger(ms, list)` | `Animation::stagger(ms, list)` | 并行运行，每个比前一个晚 `ms` 毫秒开始。 |
| `delay(ms)` | `Animation::delay(ms)` | 等待；常用于 `sequence` 中。 |
| `loop(animation, { iterations, resetBeforeIteration })` | `animation.looped(iterations).reset_before_iteration(…)` | 重复 `animation` `iterations` 次（默认无限次），每次运行前把值恢复到起点，除非 `resetBeforeIteration` 为 `false`。 |
| `add`、`subtract`、`multiply`、`divide`、`modulo`、`diffClamp` | 节点的方法 | 与 [`AnimatedNode`](#animatednode) 的对应方法相同，只是写成函数形式。 |
| `sampleSpring`、`sampleDecay` | `nsh_core::ui::sample_spring`、`sample_decay` | 弹簧或衰减动画逐帧的值（见 [Easing](#easing)）。 |

`CompositeAnimation`：

| 方法 | Rust（`Animation`） | 说明 |
|---|---|---|
| `start(callback?)` | `start()`，返回可 await 的 `AnimationEnd` | 运行（或再次运行）。整棵动画树只会 resolve 一次（并调用 `callback`），结果表示它是正常完成还是被停止。 |
| `stop()` | `stop()` | 停在当前位置。 |
| `reset()` | `reset()` | 停止，并把各个值恢复到启动时的值。 |

```rust tabs
let ys: Vec<AnimatedValue> = (0..4).map(|_| animated_value(0.0)).collect();
let hop = Animation::stagger(
    80.0,
    ys.iter().map(|y| {
        Animation::sequence([
            Animation::timing(y, -36.0, Timing::new(220.0, Easing::Curve(Curve::Quad, EaseMode::Out))),
            Animation::spring(y, 0.0, Spring { stiffness: 300.0, damping: 12.0, ..Spring::default() }),
        ])
    }),
);
let run = hop.start();
spawn(async move { log::info!("{}", if run.await { "done" } else { "stopped" }) });
```

```tsx tabs
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
  | [number, number, number, number]   // 三次贝塞尔曲线 x1, y1, x2, y2
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

缓动是数据而不是应用的函数：运行时在原生侧对它求值。在 Rust 中它是枚举 `Easing`：`Linear`、`EaseIn`、`EaseOut`、`EaseInOut`、`Bezier(x1, y1, x2, y2)` 和 `Curve(curve, mode)`，其中 `Curve` 为 `Quad`、`Cubic`、`Poly(n)`、`Sin`、`Circle`、`Exp`、`Elastic(bounciness)`、`Back(s)`、`Bounce`，`EaseMode` 为 `In`、`Out`、`InOut`。要绘制一条缓动曲线，`Easing.sample(easing, count)`（Rust：`nsh_core::ui::sample_easing`）给出它在 0 到 1 之间 `count` 个等距点上的值；`Animated.sampleSpring` 和 `Animated.sampleDecay`（`sample_spring`、`sample_decay`）从 `from`（0）开始逐帧给出弹簧或衰减动画的值（按 `fps`，默认 60），直到结束。三者都来自动画实际运行的原生代码。在 TSX 中凡是接受 `Easing` 的地方，都可以传入名称（`"easeOut"`）、由四个数字表示的三次贝塞尔曲线，或 `Easing` 对象中的曲线。

| 曲线 | Rust | 形状 |
|---|---|---|
| `quad` | `Curve::Quad` | `t²` |
| `cubic` | `Curve::Cubic` | `t³` |
| `poly(n)` | `Curve::Poly(n)` | `tⁿ`（`n` 默认为 2） |
| `sin` | `Curve::Sin` | 正弦 |
| `circle` | `Curve::Circle` | 圆形 |
| `exp` | `Curve::Exp` | 指数 |
| `elastic(bounciness)` | `Curve::Elastic(bounciness)` | 弹性过冲（`bounciness` 默认为 1） |
| `back(s)` | `Curve::Back(s)` | 先回拉再前进；`s` 为过冲量（默认 1.70158） |
| `bounce` | `Curve::Bounce` | 在结尾弹跳 |

这些曲线都是“in”形式（开始时慢）。`Easing.out(curve)` 把它反转，`Easing.inOut(curve)` 让它对称，`Easing.in(curve)` 保持原样；在 Rust 中这就是 `Easing::Curve` 的 `EaseMode`。

```rust tabs
let _ = value.timing(1.0, 600.0, Easing::Curve(Curve::Back(1.70158), EaseMode::Out));
let _ = value.timing_with(1.0, Timing { easing: Easing::Bezier(0.2, 0.0, 0.0, 1.0), ..Timing::default() });
let _ = value.timing_with(1.0, Timing { easing: Easing::EaseOut, ..Timing::default() });
```

```tsx tabs
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
  duration: number;                // 毫秒
  create?: LayoutAnimationAnimation;    // 插入的视图
  update?: LayoutAnimationAnimation;    // 移动或改变尺寸的视图
  delete?: LayoutAnimationAnimation;    // 移除的视图
}

interface LayoutAnimationAnimation {
  type?: "easeInEaseOut" | "spring" | "linear" | "easeIn" | "easeOut";
  property?: "opacity" | "scaleXY" | "scale";
  duration?: number;
  delay?: number;
  springDamping?: number;          // 0..1，默认 0.7
}
```

`LayoutAnimation.configureNext(config)` 在原生侧为下一次提交中的布局变化添加动画：移动或改变尺寸的视图平滑过渡到新位置，新视图出现（按 `property` 淡入或缩放进入），被移除的视图消失。在会改变布局的状态变化之前调用它。Rust：`animate_layout(LayoutAnimation)`，使用 `nsh_core::ui::LayoutAnimation` 的预设或它的 `create`、`update`、`delete` 过渡。

| 成员 | Rust | 说明 |
|---|---|---|
| `configureNext(config, onEnd?)` | `animate_layout(config)` | 为下一次提交的变化添加动画；结束时调用 `onEnd`。 |
| `create(duration, type?, property?)` | `LayoutAnimation::all(LayoutTransition::new(curve, duration))` | 三个阶段使用同一条曲线的配置。 |
| `Presets.easeInEaseOut` | `LayoutAnimation::EASE_IN_EASE_OUT` | 300 毫秒，视图淡入淡出。 |
| `Presets.linear` | `LayoutAnimation::LINEAR` | 500 毫秒，匀速。 |
| `Presets.spring` | `LayoutAnimation::SPRING` | 700 毫秒；位置以弹簧效果变化（阻尼 0.4），视图线性淡入淡出。 |
| `easeInEaseOut(onEnd?)`、`linear(onEnd?)`、`spring(onEnd?)` | `animate_layout(LayoutAnimation::…)` | `configureNext(Presets.…)` 的简写。 |
| `Types`、`Properties` | `LayoutCurve`、`LayoutProperty` | 这些名称的常量形式。 |

```rust tabs
let open = signal(false);
let toggle = move || {
    animate_layout(LayoutAnimation::SPRING);
    open.update(|o| *o = !*o);
};

view()
    .child(text("Details").on_press(toggle))
    .child(when(move || open.get(), || text("Shown with a spring.")))
```

```tsx tabs
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

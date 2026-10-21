---
title: 动画
description: 样式过渡与关键帧动画，动画值的 timing、spring 和 decay，插值，组合动画，缓动，布局动画，以及哪些部分运行在 UI 线程上，用 Rust 和 TSX 编写。
order: 43
group: ui
demo: guide-animation
---

# 动画

NSH 中的所有动画都运行在 UI 线程上。你用 Rust 或 TSX 描述一次，平台就逐帧播放（iOS 上是 Core Animation 和 display link，Android 上是 UI 线程的动画器），不再回调你的代码。繁忙的引擎线程不会让动画卡顿，动画运行期间也不消耗你的任何代码。

共有四种工具，从最简单到最灵活：

| 工具 | 适用场景 |
|---|---|
| [过渡](#过渡) | 样式变化时以动画代替跳变：颜色、不透明度、变换。 |
| [关键帧动画](#关键帧动画) | 加载指示器、闪烁的占位块、跳动的提示。 |
| [动画值](#动画值) | 任何由随时间变化的值驱动的效果：弹簧、手势、滚动位置、动画序列。 |
| [布局动画](#layoutanimation) | 更新后尺寸与位置的变化：条目插入、删除、重新排序，面板展开。 |

## 过渡

指明要动画的属性和时长（Rust 中的 `.transition_all().duration(400.0)`，TSX 中的 `transitionProperty` 和 `transitionDuration`），之后这些属性的变化就会从屏幕上的当前值以原生动画过渡到新值。

```rust tabs
#[component]
pub fn App() -> impl IntoElement {
    let on = signal(false);
    view().flex_1().items_center().justify_center().child(
        view()
            .on_press(move || on.update(|on| *on = !*on))
            .size(px(100.0))
            .rounded(move || px(if on.get() { 50.0 } else { 16.0 }))
            .bg(move || if on.get() { rgb(0x8250df) } else { rgb(0x0969da) })
            .rotate(move || deg(if on.get() { 45.0 } else { 0.0 }))
            .scale(move || if on.get() { 1.2_f32 } else { 1.0 })
            .transition_all()
            .duration(400.0)
            .ease_in_out(),
    )
}
```

```tsx tabs
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

| 样式 | Rust | 类型 | 说明 |
|---|---|---|---|
| `transitionProperty` | `.transition()`、`.transition_all()`、`.transition_colors()`、`.transition_opacity()`、`.transition_transform()`、`.transition_shadow()` | `string` | 参与动画的属性，用逗号分隔：`"all"`、`"colors"`、`"opacity"`、`"transform"`、`"box-shadow"`、`"border-radius"` 等。 |
| `transitionDuration` | `.duration(ms)` | `number \| "150ms" \| "0.2s"` | 数字表示毫秒，或 CSS 时间。 |
| `transitionDelay` | `.delay(ms)` | `number \| "150ms" \| "0.2s"` | 开始前的延迟。 |
| `transitionTimingFunction` | `.ease_linear()`、`.ease_in()`、`.ease_out()`、`.ease_in_out()`、`.ease_bezier(..)` | `"linear" \| "ease" \| "ease-in" \| "ease-out" \| "ease-in-out" \| "cubic-bezier(…)"` | 缓动曲线。 |

可过渡的属性按组划分：**颜色**（背景、边框、轮廓、阴影和文字的颜色，渐变色标）、**不透明度**、**变换**、**阴影**（`box-shadow`）以及**形状**（圆角和边框宽度）。CSS 名称及其驼峰形式都可以用（`background-color`、`backgroundColor`、`transform`、`scale` 等）。布局的变化用[布局动画](#layoutanimation)来呈现。

用 Tailwind 时对应的是 `transition`（颜色、不透明度、阴影和变换）、`transition-all`、`transition-colors`、`transition-opacity`、`transition-transform`、`transition-shadow`，再配合 `duration-*`、`delay-*` 和 `ease-*`。与按下状态（`.active(..)`、`active:`）结合，就得到带动画的按下状态：

```rust tabs
view()
    .on_press(save)
    .rounded_xl()
    .bg(BLUE.shade(600))
    .px_5()
    .py_3()
    .transition()
    .duration(150.0)
    .active(|s| s.scale(0.95).bg(BLUE.shade(800)))
    .child(text("Save").font_weight(FontWeight::SEMIBOLD).text_color(white()))
```

```tsx tabs
<View onPress={save} class="rounded-xl bg-blue-600 px-5 py-3 transition duration-150 active:scale-95 active:bg-blue-800">
  <Text class="font-semibold text-white">Save</Text>
</View>
```

过渡从屏幕上的当前值开始，所以中途再次改变目标时，动画会平滑地转向。

## 关键帧动画

内置四种以原生方式运行、默认无限循环的关键帧动画：`spin`（顺时针转一圈）、`ping`（放大到 2 倍并淡出）、`pulse`（淡到一半不透明度再恢复）和 `bounce`（向上跳起自身高度的四分之一再落下）。

```rust tabs
view()
    .size_8()
    .rounded_full()
    .border(px(4.0))
    .border_color(ZINC.shade(300))
    .border_t_color(BLUE.shade(600))
    .animate_spin()
view().h_4().w_40().rounded(px(4.0)).bg(ZINC.shade(200)).animate_pulse()
```

```tsx tabs
<View class="size-8 rounded-full border-4 border-zinc-300 border-t-blue-600 animate-spin" />
<View class="h-4 w-40 rounded bg-zinc-200 animate-pulse" />
```

| 样式 | Rust | 类型 | 说明 |
|---|---|---|---|
| `animationName` | `.animate_spin()`、`.animate_ping()`、`.animate_pulse()`、`.animate_bounce()`、`.animate_none()`，或 `.animation(KeyframeAnimation::new(Keyframes::Spin))` | `"none" \| "spin" \| "ping" \| "pulse" \| "bounce"` | 动画名称。 |
| `animationDuration` | `KeyframeAnimation::duration(ms)` | `number \| "1s" \| …` | 一次迭代的时长。 |
| `animationDelay` | `KeyframeAnimation::delay(ms)` | `number \| "200ms" \| …` | 开始前的延迟。 |
| `animationTimingFunction` | `KeyframeAnimation::easing([x1, y1, x2, y2])` | 同 `transitionTimingFunction` | 缓动曲线。 |
| `animationIterationCount` | `KeyframeAnimation::iterations(n)` | `number \| "infinite"` | 默认 `"infinite"`。 |

Tailwind：`animate-spin`、`animate-ping`、`animate-pulse`、`animate-bounce`、`animate-none`。

## 动画值

动画值是一个存在于 UI 线程上的数字。创建它（Rust 中用 `animated_value(..)`，TSX 中用 `nsh/animated` 的 `createAnimatedValue`），把它绑定到样式，再让它动起来；绑定它的视图每一帧都跟随它，而无需更新。

```rust tabs
#[component]
pub fn App() -> impl IntoElement {
    let x = animated_value(0.0);
    let opacity = animated_value(1.0);
    let play = move || {
        spawn(async move {
            x.timing(240.0, 600.0, Easing::Curve(Curve::Cubic, EaseMode::Out)).await;
            opacity.timing(0.3, 200.0, Easing::EaseInOut).await;
            x.spring(0.0, Spring::default());
            opacity.timing_with(1.0, Timing::default());
        });
    };
    view()
        .flex_1()
        .justify_center()
        .p_6()
        .gap_6()
        .child(
            view()
                .size(px(60.0))
                .rounded(px(12.0))
                .bg(rgb(0x0969da))
                .bind_opacity(&opacity)
                .bind_translate_x(&x),
        )
        .child(view().on_press(play).child(text("Play").text_color(rgb(0x0969da))))
}
```

```tsx tabs
import { mount, Text, View } from "nsh";
import { createAnimatedValue, Easing } from "nsh/animated";

function App() {
  const x = createAnimatedValue(0);
  const opacity = createAnimatedValue(1);
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

**可以绑定的属性：** 不透明度（`.bind_opacity(..)`）；背景色、边框颜色（所有边）和文字颜色（整段文字），通过插值得到颜色（`.bind_background(..)`、`.bind_border_color(..)`、`.bind_color(..)`）；阴影的颜色、不透明度和半径（`.bind_shadow_color(..)` 等）；以及变换步骤 `translateX`、`translateY`、`scale`、`scaleX`、`scaleY`、`rotate`、`rotateX`、`rotateY` 和 `rotateZ`（`.bind_translate_x(..)`、`.bind_translate_y(..)`、`.bind_scale(..)`、以度为单位的 `.bind_rotate(..)`、`.bind_transform(kind, ..)`）。聚焦环或闪绿的价格就是绑定到边框或文字颜色的颜色插值：

```rust tabs
let flash = animated_value(0.0);
let color = flash.interpolate_colors(&[0.0, 1.0], &[rgb(0x1f2328), rgb(0x1a7f37)]);

text(move || price.get().to_string()).bind_color(&color)
// 价格更新时：flash.set(1.0); flash.timing(0.0, 600.0, Easing::EaseInOut);
```

```tsx tabs
const flash = createAnimatedValue(0);
const color = flash.interpolate({ inputRange: [0, 1], outputRange: ["#1f2328", "#1a7f37"] });

<Text style={{ color }}>{price()}</Text>
// 价格更新时：flash.set(1); flash.timing(0, { duration: 600 });
```

动画值还可以驱动[画布绘制](./canvas.md#用动画值驱动绘制)，接收滚动偏移（[`scrollValue`](./scrolling.md#滚动联动效果scrollvalue)）和手势（[`pan`、`pinch`、`rotation`](./touch-and-gestures.md#原生手势拖动捏合与旋转)）。

动画值由绑定它的视图和创建它的组件保持存活，所以无论你的变量如何出入作用域，绑定都保持连接。在拥有它的组件中创建动画值（如果每个实例都需要自己的值，就不要放在模块顶层）。

### AnimatedValue 的方法

| TSX | Rust | 说明 |
|---|---|---|
| `set(value)` | `set(value)` | 跳到 `value`，停止正在进行的动画。 |
| `stop()` | `stop()` | 让正在运行的动画停在原处；在 TSX 中会以停下时的值 resolve。 |
| `getValue()` | `value()` | 当前值，在它运动所在的 UI 线程上读取（一帧之内给出结果）：TSX 中是 promise，Rust 中是可 await 的结果。 |
| `setOffset(offset)` | `set_offset(offset)` | 在显示值上加上 `offset`：绑定看到的是值 + 偏移，而 `set`、动画和拖动改变的是值本身。 |
| `flattenOffset()` | `flatten_offset()` | 把偏移加到值上，并把偏移设为 0。 |
| `extractOffset()` | `extract_offset()` | 把值移入偏移并把值设为 0：下一次拖动就从 0 开始，叠加在上一次停下的位置上。 |
| `timing(to, { duration?, easing?, delay? })` | `timing(to, ms, easing)`、`timing_with(to, Timing { .. })` | 在 `duration` 毫秒内动画到 `to`（默认 300 毫秒，缓动 `"easeInOut"`）。 |
| `spring(to, { stiffness?, damping?, mass?, velocity? })` | `spring(to, Spring { .. })` | 以弹簧运动到 `to`（默认 stiffness 170、damping 26、mass 1）。 |
| `decay({ velocity?, deceleration?, clamp?, bounce? })` | `decay(velocity, deceleration)`、`decay_within(velocity, deceleration, (min, max), bounce)` | 以 `velocity`（每秒单位数）开始滑行并逐渐减速（默认每毫秒 deceleration 0.998）。设置范围 `[min, max]` 时，它停在边界上，或以 `bounce`（0..1）保留的速度从边界反弹。 |

每个动画都给出一个可以 await 的结果：完成时为 `true`，被打断时（`stop()`、`set()`，或同一个值上启动了新动画）为 `false`。在 TSX 中它是 `Promise<boolean>`，在 Rust 中是 `AnimationEnd`。在运动中的值上启动新动画会平滑地接替旧动画：没有指定速度的弹簧或衰减会沿用该值当前的速度。

```rust tabs
// 弹簧参数：damping 越小越容易过冲，stiffness 越大越快。
x.spring(200.0, Spring { stiffness: 180.0, damping: 8.0, ..Spring::default() });   // 晃动
x.spring(200.0, Spring { stiffness: 400.0, damping: 30.0, ..Spring::default() });  // 干脆，不过冲

// 以拖动的松手速度滑行到停止。
view().pan_values(Some(&x), None).on_pan_end(move |e: &PanEvent| {
    x.decay(Some(e.velocity_x), 0.998);
})

// 留在轨道上、在两端柔和反弹的滑行。
x.decay_within(Some(e.velocity_x), 0.998, (0.0, 300.0), 0.3);

// 值现在在哪里（它在 UI 线程上运动，所以结果是异步的）。
let at = x.value().await;
```

```tsx tabs
// 弹簧参数：damping 越小越容易过冲，stiffness 越大越快。
x.spring(200, { stiffness: 180, damping: 8 });   // 晃动
x.spring(200, { stiffness: 400, damping: 30 });  // 干脆，不过冲

// 以拖动的松手速度滑行到停止。
<View pan={{ x }} onPanEnd={(e) => x.decay({ velocity: e.velocityX })} />

// 留在轨道上、在两端柔和反弹的滑行。
x.decay({ velocity: e.velocityX, clamp: [0, 300], bounce: 0.3 });

// 值现在在哪里（它在 UI 线程上运动，所以结果是异步的）。
const where = await x.getValue();
```

### 插值

`interpolate` 通过分段线性映射把值转换为数字、颜色或角度，并返回一个可以绑定的新节点。一个值可以驱动多个属性。在 Rust 中，`interpolate(&input, &output)` 映射为数字，`interpolate_colors(&input, &colors)` 映射为颜色，`interpolate_with(&input, output, Interpolation { .. })` 接受外推和缓动；在 TSX 中，`value.interpolate({ inputRange, outputRange })` 接受全部选项。

```rust tabs
let t = animated_value(0.0);

view()
    .bind_background(&t.interpolate_colors(&[0.0, 1.0, 2.0], &[rgb(0x54aeff), rgb(0x8250df), rgb(0xcf222e)]))
    .bind_opacity(&t.interpolate(&[0.0, 1.0], &[0.4, 1.0]))
    .bind_rotate(&t.interpolate(&[0.0, 1.0], &[0.0, 180.0]))
    .bind_scale(&t.interpolate_with(
        &[0.0, 1.0],
        AnimationOutput::Numbers(vec![0.6, 1.2]),
        Interpolation {
            easing: Easing::Curve(Curve::Back(1.70158), EaseMode::Out),
            ..Interpolation::default()
        },
    ))
```

```tsx tabs
const t = createAnimatedValue(0);

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

| 选项 | Rust | 说明 |
|---|---|---|
| `inputRange` | 输入切片 | 递增的输入节点。 |
| `outputRange` | 输出切片，或 `AnimationOutput` | 每个节点对应一个输出：数字、颜色（`"#rrggbb"`、`"rgba(…)"`、颜色名）或角度（`"45deg"`、`"1rad"`）。 |
| `extrapolate` | `Interpolation::extrapolate(Extrapolate::Extend)` | 输入范围之外：`"clamp"`（默认，截断）、`"extend"`（延续两端的线段）或 `"identity"`（直接输出输入值）。 |
| `extrapolateLeft`、`extrapolateRight` | `extrapolate_left`、`extrapolate_right` | 同上，只作用于一侧。 |
| `easing` | `easing` | 塑造每一段的曲线。 |

> [!IMPORTANT]
> 插值默认会**截断**：超出输入范围时，输出停留在最近的端点。因此把滚动视图拉过顶部时，与滚动联动的标题栏停在它的终止状态。想让曲线继续延伸时，传入 `extrapolate: "extend"`（`Extrapolate::Extend`）。

### 算术运算与 diffClamp

节点之间也可以做算术运算，同样在原生端执行：`a.add(b)`、`a.sub(b)`、`a.mul(b)`、`a.div(b)`、`a.mod(b)`（Rust 中为 `a.modulo(b)`；当 `b` 为正时结果不为负，例如 0..360 的角度），其中 `b` 可以是节点或数字。在 TSX 中，`Animated.add`、`subtract`、`multiply`、`divide` 和 `modulo` 是对应的函数形式。

`diffClamp(min, max)`（`diff_clamp`）跟随节点的*变化量*，同时把结果限制在 `min..max` 之内。把它用在滚动偏移上，就得到内容上滑时隐藏、一往下滑就重新出现的标题栏：

```rust tabs
let offset = animated_value(0.0);
let header = offset.diff_clamp(0.0, 56.0);

scroll().flex_1().scroll_value(&offset)
view().absolute().top_0().left_0().right_0().h(px(56.0)).bind_translate_y(&header.mul(-1.0))
```

```tsx tabs
const scroll = createAnimatedValue(0);
const header = scroll.diffClamp(0, 56);

<Scroll style={{ flex: 1 }} scrollValue={scroll}>…</Scroll>
<View style={{ position: "absolute", top: 0, left: 0, right: 0, height: 56, transform: [{ translateY: header.mul(-1) }] }}>…</View>
```

## 组合动画

动画由步骤搭建（Rust 中的 `Animation`，TSX 中 `nsh/animated` 的 `Animated`），整棵动画树在原生端运行：每个步骤在上一步结束的那一刻就在 UI 线程上开始，没有间隙，中间也不运行你的代码。

```rust tabs
let ys: Vec<AnimatedValue> = (0..5).map(|_| animated_value(0.0)).collect();
let fade = animated_value(1.0);

let hop = Animation::stagger(
    80.0,
    ys.iter().map(|y| {
        Animation::sequence([
            Animation::timing(y, -30.0, Timing::new(200.0, Easing::Curve(Curve::Quad, EaseMode::Out))),
            Animation::spring(y, 0.0, Spring { stiffness: 300.0, damping: 12.0, ..Spring::default() }),
        ])
    }),
);

let breathe = Animation::sequence([
    Animation::timing(&fade, 0.3, Timing::new(400.0, Easing::EaseInOut)),
    Animation::timing(&fade, 1.0, Timing::new(400.0, Easing::EaseInOut)),
])
.looped(None);

let landed = hop.start();
spawn(async move {
    status.set(if landed.await { "landed" } else { "stopped" });
});
```

```tsx tabs
import { Animated, createAnimatedValue, Easing } from "nsh/animated";

const ys = [0, 1, 2, 3, 4].map(() => createAnimatedValue(0));
const fade = createAnimatedValue(1);

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

| TSX | Rust | 说明 |
|---|---|---|
| `Animated.timing(value, { toValue, duration?, easing?, delay? })` | `Animation::timing(&value, to, Timing { .. })` | 一个 timing 步骤。 |
| `Animated.spring(value, { toValue, stiffness?, damping?, mass?, velocity? })` | `Animation::spring(&value, to, Spring { .. })` | 一个弹簧步骤。 |
| `Animated.decay(value, { velocity?, deceleration?, clamp?, bounce? })` | `Animation::decay(&value, velocity, deceleration)`、`Animation::decay_within(..)` | 一个衰减步骤。 |
| `Animated.sequence(list)` | `Animation::sequence(list)` | 依次执行。 |
| `Animated.parallel(list, { stopTogether? })` | `Animation::parallel(list)`、`.stop_together(..)` | 同时执行。 |
| `Animated.stagger(ms, list)` | `Animation::stagger(ms, list)` | 同时执行，但每个比前一个晚 `ms` 毫秒开始。 |
| `Animated.delay(ms)` | `Animation::delay(ms)` | 等待。 |
| `Animated.loop(animation, { iterations?, resetBeforeIteration? })` | `.looped(iterations)`、`.reset_before_iteration(..)` | 重复（默认无限次；Rust 中为 `None`）。 |

组合动画提供 `start()`（在 TSX 中也可以带回调，并返回表示是否完成的 promise；在 Rust 中返回可 await 的 `bool`；再次调用会重新运行）、`stop()`（停在原处）和 `reset()`（停止，并把各个值恢复到开始时的状态）。回调对整棵动画树只调用一次。

正在运行的动画不会让它的值保持存活：拥有这些值的组件（它的视图和变量）消失时，无限循环的动画随之停止，结果为 `false`。

## 缓动

缓动曲线是在原生端求值的数据：TSX 中是 `nsh/animated` 的 `Easing`，Rust 中是 `Easing` 枚举。

| TSX | Rust | 说明 |
|---|---|---|
| `Easing.linear`、`Easing.ease` | `Easing::Linear`、`Easing::Bezier(0.25, 0.1, 0.25, 1.0)` | 匀速；CSS 的 `ease`。 |
| `Easing.quad`、`Easing.cubic`、`Easing.poly(n)` | `Curve::Quad`、`Curve::Cubic`、`Curve::Poly(n)` | `t²`、`t³`、`tⁿ`。 |
| `Easing.sin`、`Easing.circle`、`Easing.exp` | `Curve::Sin`、`Curve::Circle`、`Curve::Exp` | 正弦、圆形、指数。 |
| `Easing.back(s?)` | `Curve::Back(s)` | 先回拉再前进（过冲量 `s`，默认 1.70158）。 |
| `Easing.elastic(bounciness?)` | `Curve::Elastic(bounciness)` | 弹性过冲。 |
| `Easing.bounce` | `Curve::Bounce` | 结尾处弹跳。 |
| `Easing.bezier(x1, y1, x2, y2)` | `Easing::Bezier(x1, y1, x2, y2)` | 三次贝塞尔曲线，与 CSS 相同。 |
| `Easing.in(e)`、`Easing.out(e)`、`Easing.inOut(e)` | `Easing::Curve(curve, EaseMode::In)`、`EaseMode::Out`、`EaseMode::InOut` | 曲线本身都是 "in" 形式；`out` 和 `inOut` 把它们反转。 |

在 TSX 中，凡是接受缓动的地方，也可以传入名称（`"linear"`、`"ease"`、`"easeIn"`、`"easeOut"`、`"easeInOut"`）或贝塞尔数组 `[x1, y1, x2, y2]`；在 Rust 中，`Easing::EaseIn`、`Easing::EaseOut` 和 `Easing::EaseInOut` 就是 CSS 的对应曲线。

```rust tabs
x.timing(1.0, 300.0, Easing::Curve(Curve::Back(1.70158), EaseMode::Out));
x.timing(1.0, 300.0, Easing::EaseOut);
x.timing(1.0, 300.0, Easing::Bezier(0.2, 0.8, 0.2, 1.0));
```

```tsx tabs
x.timing(1, { easing: Easing.out(Easing.back()) });
x.timing(1, { easing: "easeOut" });
x.timing(1, { easing: [0.2, 0.8, 0.2, 1] });
```

`Easing.sample(easing, count)`（Rust 中的 `sample_easing`）返回曲线在 0 到 1 之间 `count` 个等距点上的值，`Animated.sampleSpring(config, fps?)` / `Animated.sampleDecay(config, fps?)`（`sample_spring`、`sample_decay`）返回弹簧或衰减经过的值，每帧一个（默认每秒 60 帧），从 `config.from`（0）开始直到结束。它们由运行动画的同一套原生代码计算，所以画出的曲线与动画的实际运动完全一致：

```rust tabs
use nsh_core::ui::{sample_easing, sample_spring};

let curve = sample_easing(Easing::Curve(Curve::Back(1.70158), EaseMode::Out), 50); // 50 个进度值
let spring = sample_spring(0.0, 1.0, Spring { stiffness: 180.0, damping: 8.0, ..Spring::default() }, 60.0);
```

```ts tabs
const curve = Easing.sample(Easing.out(Easing.back()), 50); // 50 个进度值
const spring = Animated.sampleSpring({ toValue: 1, stiffness: 180, damping: 8 });
```

## LayoutAnimation

布局动画以原生动画呈现**下一次**更新中的布局变化：移动或改变尺寸的视图平滑地到达新位置，新视图淡入（或缩放）出现，被删除的视图淡出。在改变状态之前调用它：Rust 中是 `animate_layout(..)`，TSX 中是 `LayoutAnimation.configureNext(config)`。

```rust tabs
use nsh_core::ui::LayoutAnimation;

let items = signal(vec![1, 2, 3]);
let next = stored(4);

let add = move || {
    animate_layout(LayoutAnimation::SPRING);
    let n = next.with(|n| {
        *n += 1;
        *n - 1
    });
    items.update(|i| i.insert(0, n));
};
let remove = move || {
    animate_layout(LayoutAnimation::EASE_IN_EASE_OUT);
    items.update(|i| {
        i.remove(0);
    });
};
```

```tsx tabs
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

| TSX | Rust | 说明 |
|---|---|---|
| `configureNext(config, onEnd?)` | `animate_layout(config)` | 为下一次提交的变化添加动画；在 TSX 中，完成后调用 `onEnd`。 |
| `create(duration, type?, property?)` | `LayoutAnimation::all(LayoutTransition::new(curve, ms))` | 三个阶段使用同一曲线的配置。 |
| `Presets.easeInEaseOut`、`Presets.linear`、`Presets.spring` | `LayoutAnimation::EASE_IN_EASE_OUT`、`LINEAR`、`SPRING` | 现成的配置。 |
| `easeInEaseOut(onEnd?)`、`linear(onEnd?)`、`spring(onEnd?)` | 用预设调用 `animate_layout` | 使用预设调用 `configureNext`。 |
| `Types`、`Properties` | `LayoutCurve`、`LayoutProperty` | 构造配置时用的名称。 |

配置为 `{ duration, create?, update?, delete? }`（Rust 中为 `LayoutAnimation { create, update, delete }`）：`create` 用于插入的视图，`update` 用于移动或改变尺寸的视图，`delete` 用于删除的视图。每个阶段为 `{ type?, property?, duration?, delay?, springDamping? }`（`LayoutTransition { curve, property, duration_ms, delay_ms }`），其中 `type` 为 `"easeInEaseOut"`、`"spring"`、`"linear"`、`"easeIn"` 或 `"easeOut"`；`property` 是出现和消失的视图做什么动画（`"opacity"`、`"scaleXY"`、`"scale"`）；`springDamping`（0..1，默认 0.7；`LayoutCurve::Spring { damping }`）决定弹簧的过冲程度。

```rust tabs
use nsh_core::ui::{LayoutAnimation, LayoutCurve, LayoutProperty, LayoutTransition};

animate_layout(LayoutAnimation {
    create: Some(LayoutTransition::new(LayoutCurve::EaseOut, 300.0)),
    update: Some(LayoutTransition::new(LayoutCurve::Spring { damping: 0.8 }, 300.0)),
    delete: Some(LayoutTransition {
        property: LayoutProperty::Scale,
        ..LayoutTransition::new(LayoutCurve::EaseIn, 300.0)
    }),
});
```

```tsx tabs
LayoutAnimation.configureNext({
  duration: 300,
  create: { type: "easeOut", property: "opacity" },
  update: { type: "spring", springDamping: 0.8 },
  delete: { type: "easeIn", property: "scaleXY" },
});
```

## 各部分运行在哪里

| | 运行于 | 每帧运行你的代码 |
|---|---|---|
| 过渡、关键帧动画 | UI 线程 | 无 |
| `timing` / `spring` / `decay`、组合动画 | UI 线程 | 无；结束时通知你的代码 |
| `interpolate`、算术运算、`diffClamp` | UI 线程 | 无 |
| `scrollValue`、`pan` / `pinch` / `rotation` | UI 线程 | 无；开始和结束时通知你的代码 |
| 布局动画 | UI 线程 | 无 |
| `requestAnimationFrame` 循环（Rust 中的 `request_animation_frame`）、`onScroll` 处理函数、`PanResponder` | 你的代码，然后更新 | 每一帧 |

`requestAnimationFrame`（TSX 中跟随屏幕刷新的全局函数）只用于无法事先描述的情况，例如在你的代码中运行的物理模拟。凡是能用动画值表达的，都应该用动画值。

## 减少动态效果

当用户要求减少动态效果时（iOS 的"减弱动态效果"、Android 的"移除动画"），Rust 中的 `reduce_motion()` 和 TSX 中 `nsh/accessibility` 的 `reduceMotion()` 为 `true`，Tailwind 的 `motion-reduce:` / `motion-safe:` 变体随之生效。你的动画按原样播放；用它们来选择更平和的替代方案：

```rust tabs
let open = move || {
    if reduce_motion() {
        panel.set(0.0);
    } else {
        panel.spring(0.0, Spring::default());
    }
};

view().style_when(|| !reduce_motion(), |s| s.animate_bounce())
```

```tsx tabs
import { View } from "nsh";
import { reduceMotion } from "nsh/accessibility";

const open = () => (reduceMotion() ? panel.set(0) : panel.spring(0));

<View class="motion-safe:animate-bounce" />
```

见[无障碍](./accessibility.md#减少动态效果)。

## 相关内容

- [`nsh/animated`](../modules/nsh-animated.md)：TSX 参考。
- [触摸与手势](./touch-and-gestures.md)：用手指驱动动画值。
- [滚动](./scrolling.md#滚动联动效果scrollvalue)：`scrollValue`。
- [画布](./canvas.md#用动画值驱动绘制)：在绘制中使用动画值。
- [样式参考](../modules/style-reference.md)：`transition*` 与 `animation*` 键。

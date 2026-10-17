---
title: 动画
description: 样式过渡与关键帧动画，原生值的 timing、spring 和 decay，插值，组合动画，Easing，LayoutAnimation，以及哪些部分运行在 UI 线程上。
order: 43
group: ui
demo: guide-animation
---

# 动画

NSH 中的所有动画都运行在 UI 线程上。你在 JS 中描述一次，平台就逐帧播放（iOS 上是 Core Animation 和 display link，Android 上是 UI 线程的动画器），不再回调 JS。繁忙的 JS 线程不会让动画卡顿，动画运行期间也不消耗任何 JS。

共有四种工具，从最简单到最灵活：

| 工具 | 适用场景 |
|---|---|
| [过渡](#过渡) | 样式变化时以动画代替跳变：颜色、不透明度、变换。 |
| [关键帧动画](#关键帧动画) | 加载指示器、闪烁的占位块、跳动的提示。 |
| [原生值](#原生值) | 任何由随时间变化的值驱动的效果：弹簧、手势、滚动位置、动画序列。 |
| [LayoutAnimation](#layoutanimation) | 重新渲染后尺寸与位置的变化：条目插入、删除、重新排序，面板展开。 |

## 过渡

给视图设置 `transitionProperty` 和 `transitionDuration`，之后这些属性的变化就会从屏幕上的当前值以原生动画过渡到新值。

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

| 样式 | 类型 | 说明 |
|---|---|---|
| `transitionProperty` | `string` | 参与动画的属性，用逗号分隔：`"all"`、`"colors"`、`"opacity"`、`"transform"`、`"box-shadow"`、`"border-radius"` 等。 |
| `transitionDuration` | `number \| "150ms" \| "0.2s"` | 数字表示毫秒，或 CSS 时间。 |
| `transitionDelay` | `number \| "150ms" \| "0.2s"` | 开始前的延迟。 |
| `transitionTimingFunction` | `"linear" \| "ease" \| "ease-in" \| "ease-out" \| "ease-in-out" \| "cubic-bezier(…)"` | 缓动曲线。 |

可过渡的属性按组划分：**颜色**（背景、边框、轮廓、阴影和文字的颜色，渐变色标）、**不透明度**、**变换**、**阴影**（`box-shadow`）以及**形状**（圆角和边框宽度）。CSS 名称及其驼峰形式都可以用（`background-color`、`backgroundColor`、`transform`、`scale` 等）。布局不会过渡：尺寸与位置的变化请用 [LayoutAnimation](#layoutanimation)。

用 Tailwind 时对应的是 `transition`（颜色、不透明度、阴影和变换）、`transition-all`、`transition-colors`、`transition-opacity`、`transition-transform`、`transition-shadow`，再配合 `duration-*`、`delay-*` 和 `ease-*`。与 `active:` 变体结合，就得到带动画的按下状态：

```tsx
<View onPress={save} class="rounded-xl bg-blue-600 px-5 py-3 transition duration-150 active:scale-95 active:bg-blue-800">
  <Text class="font-semibold text-white">Save</Text>
</View>
```

过渡从屏幕上的当前值开始，所以中途再次改变目标时，动画会平滑地转向。

## 关键帧动画

内置四种以原生方式运行、默认无限循环的关键帧动画：`spin`（顺时针转一圈）、`ping`（放大到 2 倍并淡出）、`pulse`（淡到一半不透明度再恢复）和 `bounce`（向上跳起自身高度的四分之一再落下）。

```tsx
<View class="size-8 rounded-full border-4 border-zinc-300 border-t-blue-600 animate-spin" />
<View class="h-4 w-40 rounded bg-zinc-200 animate-pulse" />
```

| 样式 | 类型 | 说明 |
|---|---|---|
| `animationName` | `"none" \| "spin" \| "ping" \| "pulse" \| "bounce"` | 动画名称。 |
| `animationDuration` | `number \| "1s" \| …` | 一次迭代的时长。 |
| `animationDelay` | `number \| "200ms" \| …` | 开始前的延迟。 |
| `animationTimingFunction` | 同 `transitionTimingFunction` | 缓动曲线。 |
| `animationIterationCount` | `number \| "infinite"` | 默认 `"infinite"`。 |

Tailwind：`animate-spin`、`animate-ping`、`animate-pulse`、`animate-bounce`、`animate-none`。

## 原生值

原生值是一个存在于 UI 线程上的数字。用 `nsh/animated` 中的 `createNativeValue` 创建，把它绑定到样式，再让它动起来；绑定它的视图每一帧都跟随它，而无需重新渲染。

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

**可以绑定的属性：** `opacity`、`backgroundColor`（通过插值得到颜色），以及 `transform` 中的 `translateX`、`translateY`、`scale`、`scaleX`、`scaleY`、`rotate`、`rotateX`、`rotateY` 和 `rotateZ`。原生值还可以驱动[画布绘制](./canvas.md#用原生值驱动动画)，接收滚动偏移（[`scrollValue`](./scrolling.md#滚动联动效果scrollvalue)）和手势（[`pan`、`pinch`、`rotation`](./touch-and-gestures.md#原生手势拖动捏合与旋转)）。

原生值由绑定它的视图保持存活，所以不会因为某个 JS 变量超出作用域而解除绑定。在拥有它的组件中创建原生值（如果每个实例都需要自己的值，就不要放在模块顶层）。

### NativeValue 的方法

| 方法 | 说明 |
|---|---|
| `set(value)` | 跳到 `value`，停止正在进行的动画。 |
| `stop()` | 让正在运行的动画停在原处。 |
| `timing(to, { duration?, easing?, delay? })` | 在 `duration` 毫秒内动画到 `to`（默认 300 毫秒，缓动 `"easeInOut"`）。 |
| `spring(to, { stiffness?, damping?, mass?, velocity? })` | 以弹簧运动到 `to`（默认 stiffness 170、damping 26、mass 1）。 |
| `decay({ velocity?, deceleration? })` | 以 `velocity`（每秒单位数）开始滑行并逐渐减速（默认每毫秒 deceleration 0.998）。 |

每个动画都返回 `Promise<boolean>`：完成时为 `true`，被打断时（`stop()`、`set()`，或同一个值上启动了新动画）为 `false`。在运动中的值上启动新动画会平滑地接替旧动画：没有指定 `velocity` 的 `spring` 或 `decay` 会沿用该值当前的速度。

```tsx
// 弹簧参数：damping 越小越容易过冲，stiffness 越大越快。
x.spring(200, { stiffness: 180, damping: 8 });   // 晃动
x.spring(200, { stiffness: 400, damping: 30 });  // 干脆，不过冲

// 以拖动的松手速度滑行到停止。
<View pan={{ x }} onPanEnd={(e) => x.decay({ velocity: e.velocityX })} />
```

### 插值

`value.interpolate({ inputRange, outputRange })` 通过分段线性映射把值转换为数字、颜色或角度，并返回一个可以绑定的新节点。一个值可以驱动多个属性。

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

| 选项 | 说明 |
|---|---|
| `inputRange` | 递增的输入节点。 |
| `outputRange` | 每个节点对应一个输出：数字、颜色（`"#rrggbb"`、`"rgba(…)"`、颜色名）或角度（`"45deg"`、`"1rad"`）。 |
| `extrapolate` | 输入范围之外：`"clamp"`（默认，截断）、`"extend"`（延续两端的线段）或 `"identity"`（直接输出输入值）。 |
| `extrapolateLeft`、`extrapolateRight` | 同上，只作用于一侧。 |
| `easing` | 塑造每一段的曲线。 |

> [!IMPORTANT]
> 插值默认会**截断**：超出输入范围时，输出停留在最近的端点。因此把滚动视图拉过顶部时，与滚动联动的标题栏不会超出它的终止状态。想让曲线继续延伸时，传入 `extrapolate: "extend"`。

### 算术运算与 diffClamp

节点之间也可以做算术运算，同样在原生端执行：`a.add(b)`、`a.sub(b)`、`a.mul(b)`、`a.div(b)`、`a.mod(b)`（当 `b` 为正时结果不为负，例如 0..360 的角度），其中 `b` 可以是节点或数字。`Animated.add`、`subtract`、`multiply`、`divide` 和 `modulo` 是对应的函数形式。

`node.diffClamp(min, max)` 跟随节点的*变化量*，同时把结果限制在 `min..max` 之内。把它用在滚动偏移上，就得到内容上滑时隐藏、一往下滑就重新出现的标题栏：

```tsx
const scroll = createNativeValue(0);
const header = scroll.diffClamp(0, 56);

<Scroll style={{ flex: 1 }} scrollValue={scroll}>…</Scroll>
<View style={{ position: "absolute", top: 0, left: 0, right: 0, height: 56, transform: [{ translateY: header.mul(-1) }] }}>…</View>
```

## 组合动画

`Animated` 用步骤搭建动画，并在原生端运行整棵动画树：每个步骤在上一步结束的那一刻就在 UI 线程上开始，没有间隙，中间也没有 JS。

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

| 函数 | 说明 |
|---|---|
| `Animated.timing(value, { toValue, duration?, easing?, delay? })` | 一个 timing 步骤。 |
| `Animated.spring(value, { toValue, stiffness?, damping?, mass?, velocity? })` | 一个弹簧步骤。 |
| `Animated.decay(value, { velocity?, deceleration? })` | 一个衰减步骤。 |
| `Animated.sequence(list)` | 依次执行。 |
| `Animated.parallel(list, { stopTogether? })` | 同时执行。 |
| `Animated.stagger(ms, list)` | 同时执行，但每个比前一个晚 `ms` 毫秒开始。 |
| `Animated.delay(ms)` | 等待。 |
| `Animated.loop(animation, { iterations?, resetBeforeIteration? })` | 重复（默认无限次）。 |

`CompositeAnimation` 提供 `start(callback?)`（同时返回表示是否完成的 promise；再次调用会重新运行）、`stop()`（停在原处）和 `reset()`（停止，并把各个值恢复到开始时的状态）。回调对整棵动画树只调用一次。

## Easing

`nsh/animated` 中的 `Easing` 提供常用的缓动曲线。它们是在原生端求值的数据，而不是 JS 函数。

| Easing | 说明 |
|---|---|
| `Easing.linear`、`Easing.ease` | 匀速；CSS 的 `ease`。 |
| `Easing.quad`、`Easing.cubic`、`Easing.poly(n)` | `t²`、`t³`、`tⁿ`。 |
| `Easing.sin`、`Easing.circle`、`Easing.exp` | 正弦、圆形、指数。 |
| `Easing.back(s?)` | 先回拉再前进（过冲量 `s`，默认 1.70158）。 |
| `Easing.elastic(bounciness?)` | 弹性过冲。 |
| `Easing.bounce` | 结尾处弹跳。 |
| `Easing.bezier(x1, y1, x2, y2)` | 三次贝塞尔曲线，与 CSS 相同。 |
| `Easing.in(e)`、`Easing.out(e)`、`Easing.inOut(e)` | 曲线本身都是 "in" 形式；`out` 和 `inOut` 把它们反转。 |

凡是接受缓动的地方，也可以传入名称（`"linear"`、`"ease"`、`"easeIn"`、`"easeOut"`、`"easeInOut"`）或贝塞尔数组 `[x1, y1, x2, y2]`。

```tsx
x.timing(1, { easing: Easing.out(Easing.back()) });
x.timing(1, { easing: "easeOut" });
x.timing(1, { easing: [0.2, 0.8, 0.2, 1] });
```

## LayoutAnimation

`LayoutAnimation.configureNext(config)` 以原生动画呈现**下一次**更新中的布局变化：移动或改变尺寸的视图平滑地到达新位置，新视图淡入（或缩放）出现，被删除的视图淡出。在改变状态之前调用它。

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

| 成员 | 说明 |
|---|---|
| `configureNext(config, onEnd?)` | 为下一次提交的变化添加动画；完成后调用 `onEnd`。 |
| `create(duration, type?, property?)` | 三个阶段使用同一曲线的配置。 |
| `Presets.easeInEaseOut`、`Presets.linear`、`Presets.spring` | 现成的配置。 |
| `easeInEaseOut(onEnd?)`、`linear(onEnd?)`、`spring(onEnd?)` | 使用预设调用 `configureNext`。 |
| `Types`、`Properties` | 构造配置时用的名称。 |

配置为 `{ duration, create?, update?, delete? }`：`create` 用于插入的视图，`update` 用于移动或改变尺寸的视图，`delete` 用于删除的视图。每个阶段为 `{ type?, property?, duration?, delay?, springDamping? }`，其中 `type` 为 `"easeInEaseOut"`、`"spring"`、`"linear"`、`"easeIn"` 或 `"easeOut"`；`property` 是出现和消失的视图做什么动画（`"opacity"`、`"scaleXY"`、`"scale"`）；`springDamping`（0..1，默认 0.7）决定弹簧的过冲程度。

```tsx
LayoutAnimation.configureNext({
  duration: 300,
  create: { type: "easeOut", property: "opacity" },
  update: { type: "spring", springDamping: 0.8 },
  delete: { type: "easeIn", property: "scaleXY" },
});
```

## 各部分运行在哪里

| | 运行于 | 每帧的 JS |
|---|---|---|
| 过渡、关键帧动画 | UI 线程 | 无 |
| `timing` / `spring` / `decay`、组合动画 | UI 线程 | 无；结束时通知 JS |
| `interpolate`、算术运算、`diffClamp` | UI 线程 | 无 |
| `scrollValue`、`pan` / `pinch` / `rotation` | UI 线程 | 无；开始和结束时通知 JS |
| LayoutAnimation | UI 线程 | 无 |
| `requestAnimationFrame` 循环、`onScroll` 处理函数、`PanResponder` | JS，然后重新渲染 | 每一帧 |

`requestAnimationFrame`（跟随屏幕刷新的全局函数）只用于无法事先描述的情况，例如在 JS 中运行的物理模拟。凡是能用原生值表达的，都应该用原生值。

## 减少动态效果

当用户要求减少动态效果时（iOS 的"减弱动态效果"、Android 的"移除动画"），`nsh/accessibility` 中的 `reduceMotion()` 为 `true`，Tailwind 的 `motion-reduce:` / `motion-safe:` 变体随之生效。NSH 不会自行关闭你的动画；用它们来选择更平和的替代方案：

```tsx
import { View } from "nsh";
import { reduceMotion } from "nsh/accessibility";

const open = () => (reduceMotion() ? panel.set(0) : panel.spring(0));

<View class="motion-safe:animate-bounce" />
```

见[无障碍](./accessibility.md#减少动态效果)。

## 相关内容

- [`nsh/animated`](../modules/nsh-animated.md)：参考。
- [触摸与手势](./touch-and-gestures.md)：用手指驱动原生值。
- [滚动](./scrolling.md#滚动联动效果scrollvalue)：`scrollValue`。
- [画布](./canvas.md#用原生值驱动动画)：在绘制中使用原生值。
- [样式参考](../modules/style-reference.md)：`transition*` 与 `animation*` 键。

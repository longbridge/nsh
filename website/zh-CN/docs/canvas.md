---
title: 画布
description: 用 nsh/canvas 的 Canvas 绘图：2D 上下文子集、路径与 Path2D、渐变、文字、图片、根据 signal 重绘，以及用动画值驱动绘制动画。
order: 44
group: ui
demo: guide-canvas
---

# 画布

`nsh/canvas` 中的 `Canvas` 是一个用 HTML Canvas 2D API 绘制的视图：图表、进度环、仪表盘、签名、自定义图标，凡是画出来比用视图拼出来更容易的东西都适合。你的 `draw` 函数一次性录制整幅绘制；平台以原生方式回放（iOS 上是 Core Graphics，Android 上是 `android.graphics.Canvas`），按屏幕分辨率绘制，所以在任何像素密度下都很清晰。

```tsx
import { mount, View } from "nsh";
import { Canvas } from "nsh/canvas";

function App() {
  return (
    <View style={{ flex: 1, padding: 24, paddingTop: 60 }}>
      <Canvas
        style={{ height: 200, borderRadius: 12, backgroundColor: "#ffffff" }}
        draw={(ctx, { width, height }) => {
          ctx.fillStyle = "#0969da";
          ctx.beginPath();
          ctx.arc(width / 2, height / 2, 60, 0, Math.PI * 2);
          ctx.fill();
          ctx.strokeStyle = "#1f2328";
          ctx.lineWidth = 4;
          ctx.strokeRect(20, 20, width - 40, height - 40);
        }}
      />
    </View>
  );
}

mount(App);
```

`Canvas` 接受 [`View`](../elements/view.md) 的所有属性（样式、类、触摸事件、无障碍、`ref`），再加上 `draw`。它需要从样式或布局中得到尺寸；它的内容就是绘制结果。坐标以点为单位，原点在左上角。

## draw 何时运行

`draw(ctx, { width, height })` 会在以下时机运行：

- 画布尺寸确定后（第一次布局之后）；
- 画布尺寸改变时；
- 它读取过的 [signal](./reactivity.md) 改变时。

每次运行都会替换整幅绘制。在 `draw` 中读取 signal，画布就能跟随你的数据：

```tsx
const values = signal([12, 30, 22, 48, 36, 60]);

<Canvas
  style={{ height: 160 }}
  draw={(ctx, { width, height }) => {
    const v = values();
    const x = (i: number) => (i / (v.length - 1)) * width;
    const y = (n: number) => height - (n / 100) * height;
    ctx.beginPath();
    v.forEach((n, i) => (i === 0 ? ctx.moveTo(x(i), y(n)) : ctx.lineTo(x(i), y(n))));
    ctx.strokeStyle = "#3b82f6";
    ctx.lineWidth = 2.5;
    ctx.lineJoin = "round";
    ctx.stroke();
  }}
/>
```

上下文的调用只是录制：`draw` 运行期间什么也不会画出来，所以无法读回像素。`draw` 返回后，录制结果作为一个操作发送给平台。

## 2D 上下文

`CanvasRenderingContext2D` 是 HTML Canvas 2D API 的一个子集。支持的内容：

**状态**

| 成员 | 说明 |
|---|---|
| `save()`、`restore()` | 压入和弹出绘制状态（样式、变换、裁剪）。 |
| `fillStyle`、`strokeStyle` | CSS 颜色、渐变，或插值为颜色的动画值。 |
| `lineWidth`、`lineCap`、`lineJoin`、`miterLimit` | 描边形状。 |
| `setLineDash(segments)`、`getLineDash()`、`lineDashOffset` | 虚线。 |
| `globalAlpha` | 后续绘制的不透明度，0..1。 |
| `globalCompositeOperation` | 合成方式：`"source-over"`、`"destination-out"`、`"multiply"`、`"screen"` 等。 |
| `shadowColor`、`shadowBlur`、`shadowOffsetX`、`shadowOffsetY` | 阴影。 |
| `font` | CSS `font` 简写：`"600 14px Inter"`（尺寸可用 px、pt 或 rem）。 |
| `textAlign`、`textBaseline` | 文字位置。 |

**变换**：`translate(x, y)`、`scale(x, y)`、`rotate(angle)`（弧度，顺时针）、`transform(a, b, c, d, e, f)`、`setTransform(a, b, c, d, e, f)`、`resetTransform()`。

**路径**：`beginPath()`、`moveTo`、`lineTo`、`quadraticCurveTo`、`bezierCurveTo`、`arc`、`arcTo`、`ellipse`、`rect`、`roundRect(x, y, w, h, radii)`、`closePath()`；然后是 `fill(rule?)`、`stroke()` 和 `clip(rule?)`，它们也都接受 `Path2D`。填充规则为 `"nonzero"` 和 `"evenodd"`。

**矩形**：`fillRect`、`strokeRect`、`clearRect`。

**文字**：`fillText(text, x, y, maxWidth?)`、`strokeText(...)`、`measureText(text)`（由平台的文字引擎按当前 `font` 同步测量；只有 `width`）。

**渐变**：`createLinearGradient`、`createRadialGradient`、`createConicGradient`，都通过 `addColorStop(offset, color)` 添加色标。

**图片**：`drawImage(image, dx, dy)`、`drawImage(image, dx, dy, dw, dh)`、`drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh)`。

`ctx.canvas.width` 和 `ctx.canvas.height` 给出画布以点为单位的尺寸。

**不支持**：`getImageData` / `putImageData` 等像素访问、`filter`、`isPointInPath`、在 `drawImage` 中使用 SVG 图片。与 HTML Canvas 一样，无效的颜色或字体只会让那一个操作被丢弃。

## 路径与 Path2D

`Path2D` 保存一条构建一次、可多次填充、描边或裁剪的路径。它接受 SVG 路径数据，在原生端解析，用来画图标很方便：

```tsx
import { Canvas, Path2D } from "nsh/canvas";

const heart = new Path2D("M12 21s-7-4.35-9.5-8.5C.5 8.5 3 4 7 4c2 0 3.5 1 5 3 1.5-2 3-3 5-3 4 0 6.5 4.5 4.5 8.5C19 16.65 12 21 12 21z");

function Heart(props: { size: number; color: string }) {
  return (
    <Canvas
      style={{ width: props.size, height: props.size }}
      draw={(ctx) => {
        ctx.scale(props.size / 24, props.size / 24); // 路径画在 24 点的网格上
        ctx.fillStyle = props.color;
        ctx.fill(heart);
      }}
    />
  );
}
```

`Path2D` 拥有与上下文相同的路径方法（`moveTo`、`arc`、`roundRect` 等）以及 `addPath(other)`。`new Path2D(other)` 复制一条路径。

## 渐变

```tsx
<Canvas
  style={{ height: 120 }}
  draw={(ctx, { width, height }) => {
    const linear = ctx.createLinearGradient(0, 0, width, 0);
    linear.addColorStop(0, "#22d3ee");
    linear.addColorStop(1, "#6366f1");
    ctx.fillStyle = linear;
    ctx.fillRect(0, 0, width, height / 2);

    const radial = ctx.createRadialGradient(width / 2, height * 0.75, 4, width / 2, height * 0.75, 40);
    radial.addColorStop(0, "#fde68a");
    radial.addColorStop(1, "#f97316");
    ctx.fillStyle = radial;
    ctx.beginPath();
    ctx.arc(width / 2, height * 0.75, 28, 0, Math.PI * 2);
    ctx.fill();
  }}
/>
```

`createConicGradient(startAngle, x, y)` 让颜色绕一个点旋转过渡；起始角以弧度表示，从 x 轴顺时针计算。把锥形渐变用在描边的圆弧上，就是渐变色的进度环。

## 文字

```tsx
ctx.font = "600 20px system-ui";
ctx.fillStyle = "#1f2328";
ctx.textAlign = "center";
ctx.textBaseline = "middle";
ctx.fillText("72%", width / 2, height / 2);

ctx.font = "11px system-ui";
const label = "100";
ctx.fillText(label, 32 - 6 - ctx.measureText(label).width, 16);
```

字体族与样式中的相同：系统字体族以及应用导入的字体名称（见[字体](./fonts.md)）。

## 图片

`drawImage` 接受 URL、导入的资源（`import logo from "./logo.png"`）或 `{ uri }`。图片在原生端加载，加载完成后画布会重新绘制，你不需要自己跟踪加载状态。远程 URL 受应用的[网络能力声明](./sandbox-and-capabilities.md)约束。

```tsx
import { Canvas } from "nsh/canvas";
import logo from "./logo.png";

<Canvas
  style={{ width: 120, height: 120 }}
  draw={(ctx) => {
    ctx.save();
    ctx.beginPath();
    ctx.arc(60, 60, 56, 0, Math.PI * 2);
    ctx.clip(); // 圆形头像
    ctx.drawImage(logo, 4, 4, 112, 112);
    ctx.restore();
  }}
/>
```

九个参数的形式中，源矩形以图片的像素为单位。

## 用动画值驱动绘制

绘制调用中的任何数字参数，以及 `fillStyle` / `strokeStyle`，都可以是[动画值](./animation.md#动画值)或由它派生的节点。这类参数在平台绘制时才被读取；当动画值在动画中时，平台会自行逐帧重绘，不运行 `draw`，也不运行任何其他 JS。

```tsx
import { mount, signal, Text, View } from "nsh";
import { createAnimatedValue, Easing } from "nsh/animated";
import { Canvas } from "nsh/canvas";

function Ring() {
  const progress = createAnimatedValue(0.3);
  const label = signal(30);
  const go = (to: number) => {
    label.set(Math.round(to * 100));
    progress.timing(to, { duration: 900, easing: Easing.inOut(Easing.cubic) });
  };
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <Canvas
        style={{ width: 140, height: 140 }}
        draw={(ctx, { width, height }) => {
          const cx = width / 2, cy = height / 2, r = width / 2 - 10;
          ctx.lineWidth = 12;
          ctx.lineCap = "round";
          ctx.strokeStyle = "#e5e7eb";
          ctx.beginPath();
          ctx.arc(cx, cy, r, 0, Math.PI * 2);
          ctx.stroke();
          ctx.strokeStyle = "#6366f1";
          ctx.beginPath();
          // 终止角在每一帧读取动画值。
          ctx.arc(cx, cy, r, -Math.PI / 2, progress.mul(Math.PI * 2).sub(Math.PI / 2));
          ctx.stroke();
          ctx.fillStyle = "#1f2328";
          ctx.font = "600 22px system-ui";
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillText(`${label()}%`, cx, cy);
        }}
      />
      <View onPress={() => go(Math.random())}>
        <Text style={{ color: "#0969da" }}>Animate</Text>
      </View>
    </View>
  );
}

mount(Ring);
```

圆弧的终止角是 `progress.mul(2π).sub(π/2)`：它是一个节点，所以进度环会在 900 毫秒内以原生方式扫过，而 `draw` 只运行了一次（标签读取了 signal，因此再运行一次）。时钟的秒针也是同样的做法：`ctx.rotate(seconds.mul(Math.PI / 30))`，其中 `seconds` 做线性动画。

颜色也可以这样动起来：`ctx.fillStyle = t.interpolate({ inputRange: [0, 1], outputRange: ["#54aeff", "#cf222e"] })`。

## 画布上的触摸

画布是一个视图，因此接受按压事件和[手势响应者系统](./touch-and-gestures.md#手势响应者系统)。手写绘图就是把点记录到 signal 中再画出来：

```tsx
const strokes = signal<[number, number][][]>([]);

<Canvas
  style={{ height: 240 }}
  onStartShouldSetResponder={() => true}
  onResponderGrant={(e) => {
    strokes.set([...strokes(), [[e.locationX, e.locationY]]]);
    return true; // 外层的滚动视图不能接管触摸
  }}
  onResponderMove={(e) => {
    const all = strokes();
    strokes.set([...all.slice(0, -1), [...all[all.length - 1], [e.locationX, e.locationY]]]);
  }}
  draw={(ctx) => {
    ctx.lineWidth = 3;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const s of strokes()) {
      ctx.beginPath();
      s.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
      ctx.stroke();
    }
  }}
/>
```

## 性能

- JS 调用上下文时，绘制在 Rust 中录制，并作为一个操作发送给平台；不存在每次调用都跨越一次桥的开销。
- `draw` 每次都重新录制全部内容。让 `draw` 中的工作量与显示的内容相称，把每帧都变化的数据放在动画值而不是 signal 中。
- 如果一幅绘制既有静态部分又有频繁变化的部分，把它拆成上下叠放的两个画布，这样只有一个需要重绘。
- 在可缩放的滚动视图中，缩放结束后画布会按缩放后的分辨率重新绘制。

## 相关内容

- [`Canvas` 元素](../elements/canvas.md) 与 [`nsh/canvas`](../modules/nsh-canvas.md)：参考。
- [动画](./animation.md)：动画值、`interpolate` 和 `Easing`。
- [图片与资源](./images-and-assets.md)：导入图片。

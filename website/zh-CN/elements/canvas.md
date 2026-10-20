---
title: Canvas
description: Canvas 组件：用 HTML Canvas 2D 上下文自定义绘制，并在原生侧回放。
order: 8
---

# Canvas

canvas 是一个由你自己用 HTML Canvas 2D API 绘制的视图：图表、仪表盘、圆环、签名、自定义形状都可以用它来画。绘制函数只负责记录操作，平台在原生侧以屏幕分辨率回放这些操作（iOS 和 Mac Catalyst 上是 Core Graphics，Android 上是 `android.graphics.Canvas`）。

```tsx
import { mount } from "nsh";
import { Canvas } from "nsh/canvas";

function Ring(props: { progress: number }) {
  return (
    <Canvas
      style={{ width: 120, height: 120 }}
      draw={(cx, { width, height }) => {
        const centerX = width / 2, cy = height / 2, r = width / 2 - 10;
        cx.lineWidth = 12;
        cx.lineCap = "round";
        cx.strokeStyle = "#e5e7eb";
        cx.beginPath();
        cx.arc(centerX, cy, r, 0, Math.PI * 2);
        cx.stroke();
        cx.strokeStyle = "#6366f1";
        cx.beginPath();
        cx.arc(centerX, cy, r, -Math.PI / 2, -Math.PI / 2 + props.progress * Math.PI * 2);
        cx.stroke();
      }}
    />
  );
}

mount(() => <Ring progress={0.7} />);
```

`Canvas` 从 [`nsh/canvas`](../modules/nsh-canvas.md) 导入。它会创建原生画布视图、测量它的尺寸，再把上下文和尺寸传给你的 `draw` 函数。

## 属性

`Canvas` 接受 [view](./view.md) 的属性（`style`、`class`、点按与手势属性、`onLayout`、无障碍属性、`ref` 等），另外还有：

| 属性 | 类型 | 说明 |
|---|---|---|
| `draw` | `(cx: CanvasRenderingContext2D, size: { width: number; height: number }) => void` | 绘制画布。尺寸确定后执行一次，画布尺寸变化时、读取的 signal 变化时再次执行。 |

请通过样式给画布一个尺寸（`width` / `height`、`flex`、`aspectRatio`）：它自身没有内容尺寸。

## 何时重绘

`draw` 会像 effect 一样被追踪：在里面读取某个 signal，绘制就会订阅它。

```tsx
const values = signal([12, 40, 28, 64]);

<Canvas
  style={{ height: 160 }}
  draw={(cx, { width, height }) => {
    const data = values(); // a change of values() redraws
    const step = width / data.length;
    cx.fillStyle = "#0969da";
    data.forEach((v, i) => cx.fillRect(i * step + 4, height - v, step - 8, v));
  }}
/>;
```

数字和颜色也可以是[原生动画值](../modules/nsh-animated.md)（`CanvasNumber`、`CanvasPaint`）：这些值动画时，平台会自行重绘，不需要执行 `draw`，每帧也不运行任何 JS。

```tsx
import { Animated, createAnimatedValue } from "nsh/animated";
import { Canvas } from "nsh/canvas";

const angle = createAnimatedValue(0);
Animated.timing(angle, { toValue: Math.PI * 2, duration: 1000 }).start();

<Canvas
  style={{ width: 80, height: 80 }}
  draw={(cx) => {
    cx.translate(40, 40);
    cx.rotate(angle); // animates natively
    cx.fillStyle = "#cf222e";
    cx.fillRect(-20, -4, 40, 8);
  }}
/>;
```

## 上下文

`CanvasRenderingContext2D` 是浏览器版本的一个子集：状态栈（`save`、`restore`）、变换、路径（`moveTo`、`lineTo`、`arc`、`arcTo`、`ellipse`、`rect`、`roundRect`、贝塞尔曲线）、`fill` / `stroke` / `clip`（也可作用于 `Path2D`，后者接受 SVG 路径数据）、矩形、文字（`fillText`、`strokeText`、`measureText`）、线性 / 径向 / 锥形渐变、虚线、阴影、`globalAlpha`、`globalCompositeOperation` 以及 `drawImage`（图片可以是 URL 或导入的资源）。所有成员见 [`nsh/canvas` 参考](../modules/nsh-canvas.md)；[画布指南](../docs/canvas.md)介绍了如何绘制图表和动画图形。

坐标单位是点，原点在左上角。`measureText` 是同步的，只返回 `width`。

不支持：`getImageData` / `putImageData`、`filter`、`isPointInPath` 以及 SVG 图片。

## 事件

与 [view 的事件](./view.md)相同：`onPress`、`onLayout`、手势和触摸。如果要处理相对于画面的点击，可以在 `onTouchEnd` 中读取 `locationX` / `locationY`，或用 `onResponderMove` 跟踪拖动。

## Ref 方法

没有自己的方法。

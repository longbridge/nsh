---
title: Canvas
description: "The Canvas component: custom drawing with an HTML Canvas 2D context, replayed natively."
order: 8
---

# Canvas

A canvas is a view you draw yourself with the HTML Canvas 2D API: charts, gauges, rings, signatures, custom shapes.
The drawing function only records operations; the platform replays them natively (Core Graphics on iOS and Mac
Catalyst, `android.graphics.Canvas` on Android) at the screen's resolution.

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

`Canvas` comes from [`nsh/canvas`](../modules/nsh-canvas.md). It creates the native canvas view, measures it, and runs
your `draw` function with a context and the canvas's size.

## Props

`Canvas` takes the props of a [view](./view.md) (`style`, `class`, press and gesture props, `onLayout`, accessibility,
`ref`, …) plus:

| Prop | Type | Description |
|---|---|---|
| `draw` | `(cx: CanvasRenderingContext2D, size: { width: number; height: number }) => void` | Draws the canvas. Runs once the size is known, again when the canvas resizes, and again when signals it reads change. |

Give the canvas a size through its style (`width` / `height`, `flex`, `aspectRatio`): it has no content size of its own.

## When it redraws

`draw` is tracked like an effect: reading a signal inside it subscribes the drawing to that signal.

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

Numbers and colors may also be [native animated values](../modules/nsh-animated.md) (`CanvasNumber`,
`CanvasPaint`): the platform then redraws on its own while they animate, without running `draw` or any JS per frame.

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

## The context

`CanvasRenderingContext2D` is a subset of the browser's: the state stack (`save`, `restore`), transforms, paths
(`moveTo`, `lineTo`, `arc`, `arcTo`, `ellipse`, `rect`, `roundRect`, Bézier curves), `fill` / `stroke` / `clip` (also of
a `Path2D`, which accepts SVG path data), rectangles, text (`fillText`, `strokeText`, `measureText`), linear / radial /
conic gradients, line dashes, shadows, `globalAlpha`, `globalCompositeOperation` and `drawImage` (images by URL or
imported asset). Every member is listed in the [`nsh/canvas` reference](../modules/nsh-canvas.md); the
[canvas guide](../docs/canvas.md) walks through drawing charts and animated graphics.

Coordinates are in points with the origin at the top left. `measureText` is synchronous and returns only `width`.

Not supported: `getImageData` / `putImageData`, `filter`, `isPointInPath` and SVG images.

## Events

The [view events](./view.md#events): `onPress`, `onLayout`, gestures and touches. To handle drawing-relative taps, read
`locationX` / `locationY` in `onTouchEnd`, or track a drag with `onResponderMove`.

## Ref methods

None of its own.

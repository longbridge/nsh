---
title: nsh/canvas
description: Canvas, the 2D drawing context, Path2D and gradients, with the parts of HTML Canvas 2D that NSH supports.
order: 5
demo: mod-canvas
---

# nsh/canvas

`nsh/canvas` draws with an HTML Canvas 2D–style context. Your `draw` function **records** the drawing; the platform replays it natively (Core Graphics on iOS, `android.graphics.Canvas` on Android) at the screen's resolution. Numbers and colors in a drawing may be [native values](./nsh-animated.md): when they animate, the platform redraws on its own, without running `draw` again.

```ts
import { Canvas, CanvasGradient, CanvasRenderingContext2D, Path2D } from "nsh/canvas";
```

See [Canvas](../docs/canvas.md) for the guide and [`<Canvas>`](../elements/canvas.md) for its props.

## Canvas

```ts
function Canvas(props: CanvasProps): YNode;

interface CanvasProps extends ViewProps {
  draw: (ctx: CanvasRenderingContext2D, size: CanvasSize) => void;
}

interface CanvasSize {
  width: number;   // points
  height: number;
}
```

A view drawn with a 2D context. It takes every [view prop](../elements/view.md) (style, classes, touch handlers, accessibility, `ref`), plus:

| Prop | Type | Description |
|---|---|---|
| `draw` | `(ctx, { width, height }) => void` | Draws the canvas. Runs once the size is known, again when the signals it reads change, and again when the canvas resizes. Native values passed as numbers or colors animate without running it. |

Give the canvas a size through its style (`width` / `height`, `flex: 1`, `aspectRatio`, ...). Coordinates are in points, the origin at the top left.

```tsx
import { mount, signal, Text, View } from "nsh";
import { Canvas } from "nsh/canvas";

function App() {
  const values = signal([12, 30, 22, 48, 36, 60, 52, 75]);
  return (
    <View style={{ flex: 1, padding: 24 }}>
      <Canvas
        style={{ height: 160 }}
        draw={(ctx, { width, height }) => {
          const v = values();
          const step = width / (v.length - 1);
          ctx.strokeStyle = "#6366f1";
          ctx.lineWidth = 3;
          ctx.lineJoin = "round";
          ctx.beginPath();
          v.forEach((n, i) => (i ? ctx.lineTo(i * step, height - n * 2) : ctx.moveTo(0, height - n * 2)));
          ctx.stroke();
        }}
      />
      <Text onPress={() => values.update((v) => [...v.slice(1), Math.random() * 80])}>Next</Text>
    </View>
  );
}

mount(App);
```

## Animating without redrawing

Any argument typed `CanvasNumber` and any paint (`fillStyle`, `strokeStyle`) accepts a `NativeNode`. The canvas then reads the node while it draws, and redraws natively on every frame of its animation:

```tsx
import { createNativeValue, Easing } from "nsh/animated";
import { Canvas } from "nsh/canvas";

const progress = createNativeValue(0);

<Canvas
  style={{ width: 120, height: 120 }}
  draw={(ctx, { width }) => {
    const c = width / 2;
    ctx.lineWidth = 12;
    ctx.lineCap = "round";
    ctx.strokeStyle = "#6366f1";
    ctx.beginPath();
    // The end angle is a native node: animating `progress` redraws natively.
    ctx.arc(c, c, c - 10, -Math.PI / 2, progress.mul(Math.PI * 2).sub(Math.PI / 2));
    ctx.stroke();
  }}
/>;

progress.timing(0.75, { duration: 900, easing: Easing.inOut(Easing.cubic) });
```

```ts
type CanvasNumber = number | NativeNode;
type CanvasPaint = string | CanvasGradient | NativeNode;
```

## CanvasRenderingContext2D

The context `draw` receives. Calls only record operations; the platform draws them after `draw` returns.

### State and transforms

```ts
readonly canvas: { readonly width: number; readonly height: number };
save(): void;
restore(): void;
translate(x: CanvasNumber, y: CanvasNumber): void;
scale(x: CanvasNumber, y: CanvasNumber): void;
rotate(angle: CanvasNumber): void;
transform(a: CanvasNumber, b: CanvasNumber, c: CanvasNumber, d: CanvasNumber, e: CanvasNumber, f: CanvasNumber): void;
setTransform(a: CanvasNumber, b: CanvasNumber, c: CanvasNumber, d: CanvasNumber, e: CanvasNumber, f: CanvasNumber): void;
resetTransform(): void;
```

| Member | Description |
|---|---|
| `canvas` | The canvas's size in points. |
| `save()` / `restore()` | Pushes / pops the drawing state (transform, clip, styles). |
| `translate(x, y)` | Moves the origin. |
| `scale(x, y)` | Scales the axes. |
| `rotate(angle)` | Rotates the axes; radians, clockwise. |
| `transform(a, b, c, d, e, f)` | Multiplies the transform by the matrix `[a c e; b d f]`. |
| `setTransform(a, b, c, d, e, f)` | Replaces the transform. |
| `resetTransform()` | Back to the identity. |

### Properties

| Property | Type | Description |
|---|---|---|
| `fillStyle` | `CanvasPaint` | Color, gradient or native color node used by `fill`, `fillRect`, `fillText`. |
| `strokeStyle` | `CanvasPaint` | The same for strokes. |
| `lineWidth` | `CanvasNumber` | Stroke width in points. |
| `lineCap` | `"butt" \| "round" \| "square"` | How strokes end. |
| `lineJoin` | `"miter" \| "round" \| "bevel"` | How stroke segments join. |
| `miterLimit` | `number` | Miter joins longer than this (in line widths) are beveled. |
| `lineDashOffset` | `CanvasNumber` | Where the dash pattern starts. |
| `globalAlpha` | `CanvasNumber` | Opacity of what is drawn next, 0..1. |
| `globalCompositeOperation` | `string` | How what is drawn next combines with the canvas (`"source-over"`, `"multiply"`, ...). |
| `shadowColor` | `string` | The color of shadows (transparent: none). |
| `shadowBlur` | `number` | Shadow blur in points. |
| `shadowOffsetX`, `shadowOffsetY` | `number` | Shadow offset in points. |
| `font` | `string` | CSS `font` shorthand, e.g. `"600 14px Inter"` (sizes in `px`, `pt` or `rem`). |
| `textAlign` | `"start" \| "end" \| "left" \| "right" \| "center"` | Where text sits horizontally relative to its x. |
| `textBaseline` | `"alphabetic" \| "top" \| "hanging" \| "middle" \| "ideographic" \| "bottom"` | Where text sits vertically relative to its y. |

Dashes: `setLineDash(segments: number[])` and `getLineDash(): number[]`.

### Paths

The context has a current path, built with the same methods as [`Path2D`](#path2d):

```ts
beginPath(): void;
moveTo(x, y): void;
lineTo(x, y): void;
quadraticCurveTo(cpx, cpy, x, y): void;
bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y): void;
arc(x, y, radius, startAngle, endAngle, counterclockwise?: boolean): void;
arcTo(x1, y1, x2, y2, radius): void;
ellipse(x, y, radiusX, radiusY, rotation, startAngle, endAngle, counterclockwise?: boolean): void;
rect(x, y, w, h): void;
roundRect(x, y, w, h, radii?: CanvasNumber | CanvasNumber[]): void;
closePath(): void;
```

All coordinates are `CanvasNumber`s. Angles are in radians, clockwise from the x axis. `roundRect`'s `radii` is one radius or 2–4 as in CSS `border-radius` (top left first).

### Drawing

```ts
fill(path?: Path2D | "nonzero" | "evenodd", rule?: "nonzero" | "evenodd"): void;
stroke(path?: Path2D): void;
clip(path?: Path2D | "nonzero" | "evenodd", rule?: "nonzero" | "evenodd"): void;
fillRect(x, y, w, h): void;
strokeRect(x, y, w, h): void;
clearRect(x, y, w, h): void;
```

| Method | Description |
|---|---|
| `fill(path?, rule?)` | Fills the current path (or `path`) with `fillStyle`, by the nonzero (default) or even-odd rule. |
| `stroke(path?)` | Strokes the current path (or `path`) with `strokeStyle`. |
| `clip(path?, rule?)` | Restricts what is drawn next to the path. |
| `fillRect`, `strokeRect` | A rectangle, without touching the current path. |
| `clearRect` | Makes a rectangle transparent. |

### Text

```ts
fillText(text: string, x: CanvasNumber, y: CanvasNumber, maxWidth?: number): void;
strokeText(text: string, x: CanvasNumber, y: CanvasNumber, maxWidth?: number): void;
measureText(text: string): TextMetrics; // { width: number }
```

`measureText` is synchronous: it measures with the platform's text engine and the current `font`. Only `width` is reported.

```ts
ctx.font = "11px system-ui";
const label = "100";
ctx.fillText(label, 40 - ctx.measureText(label).width, 20);
```

### Gradients

```ts
createLinearGradient(x0: number, y0: number, x1: number, y1: number): CanvasGradient;
createRadialGradient(x0: number, y0: number, r0: number, x1: number, y1: number, r1: number): CanvasGradient;
createConicGradient(startAngle: number, x: number, y: number): CanvasGradient;

class CanvasGradient {
  addColorStop(offset: number, color: string): void; // offset 0..1, any CSS color
}
```

```ts
const g = ctx.createLinearGradient(0, 0, 0, height);
g.addColorStop(0, "rgba(99, 102, 241, 0.4)");
g.addColorStop(1, "rgba(99, 102, 241, 0)");
ctx.fillStyle = g;
ctx.fillRect(0, 0, width, height);
```

### Images

```ts
drawImage(image: string | ImageAsset | { uri: string }, ...args: CanvasNumber[]): void;
```

Draws an image given as a URL, an imported asset (`import logo from "./logo.png"`) or `{ uri }`. It is loaded natively; the canvas draws again once it has loaded. Three forms, as in HTML:

- `drawImage(image, dx, dy)`: at its natural size.
- `drawImage(image, dx, dy, dw, dh)`: scaled into a rectangle.
- `drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh)`: part of it (source in image pixels).

## Path2D

```ts
class Path2D {
  constructor(path?: Path2D | string);
  addPath(path: Path2D): void;
  // and every path method: moveTo, lineTo, quadraticCurveTo, bezierCurveTo,
  // arc, arcTo, ellipse, rect, roundRect, closePath
}
```

A reusable path to fill, stroke or clip. The constructor copies another `Path2D` or parses SVG path data natively:

```ts
const heart = new Path2D("M12 21s-7-4.5-9.5-9A5.5 5.5 0 0 1 12 6a5.5 5.5 0 0 1 9.5 6c-2.5 4.5-9.5 9-9.5 9z");
ctx.save();
ctx.scale(4, 4);
ctx.fillStyle = "#ef4444";
ctx.fill(heart);
ctx.restore();
```

## Not supported

Pixel access (`getImageData`, `putImageData`), `filter`, `isPointInPath` and SVG images are not available. An invalid color or font drops that one operation, as in HTML Canvas.

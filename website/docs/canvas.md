---
title: Canvas
description: Drawing with Canvas from nsh/canvas - the 2D context subset, paths and Path2D, gradients, text, images, redrawing from signals, and animating drawings with native values.
order: 44
group: ui
demo: guide-canvas
---

# Canvas

`Canvas` from `nsh/canvas` is a view you draw into with the HTML Canvas 2D API: charts, progress rings, gauges, signatures, custom icons, anything that is easier to draw than to build out of views. Your `draw` function records the drawing in one go; the platform replays it natively (Core Graphics on iOS, `android.graphics.Canvas` on Android) at the screen's resolution, so drawings stay sharp at any pixel density.

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

A `Canvas` takes every prop of a [`View`](../elements/view.md) (style, classes, touch handlers, accessibility, `ref`) plus `draw`. It needs a size from its style or layout; its content is the drawing. Coordinates are in points, with the origin at the top left.

## When draw runs

`draw(ctx, { width, height })` runs:

- once the canvas's size is known (after its first layout);
- again when the canvas is resized;
- again when a [signal](./reactivity.md) it read changes.

Each run replaces the whole drawing. Reading signals inside `draw` is how a canvas follows your data:

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

Context calls only record: nothing is drawn while `draw` runs, so you cannot read pixels back. The recording is sent to the platform as one operation after `draw` returns.

## The 2D context

`CanvasRenderingContext2D` is a subset of the HTML Canvas 2D API. What is supported:

**State**

| Member | Description |
|---|---|
| `save()`, `restore()` | Push and pop the drawing state (styles, transform, clip). |
| `fillStyle`, `strokeStyle` | A CSS color, a gradient, or a native value interpolating colors. |
| `lineWidth`, `lineCap`, `lineJoin`, `miterLimit` | Stroke shape. |
| `setLineDash(segments)`, `getLineDash()`, `lineDashOffset` | Dashes. |
| `globalAlpha` | Opacity of what is drawn next, 0..1. |
| `globalCompositeOperation` | How it combines: `"source-over"`, `"destination-out"`, `"multiply"`, `"screen"`, ... |
| `shadowColor`, `shadowBlur`, `shadowOffsetX`, `shadowOffsetY` | Shadows. |
| `font` | CSS `font` shorthand: `"600 14px Inter"` (sizes in px, pt or rem); `small-caps` and the number forms `tabular-nums`, `oldstyle-nums`, ... may come before the size. |
| `fontVariantNumeric` | `"tabular-nums"` makes every figure the same width, so changing numbers (counters, prices, axis labels) do not jitter; also `"proportional-nums"`, `"oldstyle-nums"`, `"lining-nums"`, `"normal"`. An NSH addition (CSS's `font-variant-numeric`); setting `font` keeps it. Browsers ignore it. |
| `textAlign`, `textBaseline` | Text placement. |

**Transforms**: `translate(x, y)`, `scale(x, y)`, `rotate(angle)` (radians, clockwise), `transform(a, b, c, d, e, f)`, `setTransform(a, b, c, d, e, f)`, `resetTransform()`.

**Paths**: `beginPath()`, `moveTo`, `lineTo`, `quadraticCurveTo`, `bezierCurveTo`, `arc`, `arcTo`, `ellipse`, `rect`, `roundRect(x, y, w, h, radii)`, `closePath()`; then `fill(rule?)`, `stroke()` and `clip(rule?)`, each also taking a `Path2D`. Fill rules are `"nonzero"` and `"evenodd"`.

**Rectangles**: `fillRect`, `strokeRect`, `clearRect`.

**Text**: `fillText(text, x, y, maxWidth?)`, `strokeText(...)`, `measureText(text)` (measured synchronously by the platform's text engine with the current `font`; only `width`).

**Hit testing**: `isPointInPath(path?, x, y, rule?)` and `isPointInStroke(path?, x, y)` answer synchronously, computed from the recorded geometry (the current transform, path, `lineWidth` and `lineCap`). Keep the context of the last `draw` and the `Path2D`s you drew to find the shape under a tap:

```tsx
let ctx: CanvasRenderingContext2D | undefined;
const slices = [new Path2D("M50 50 L100 50 A50 50 0 0 1 50 100 Z"), /* ... */];

<Canvas
  style={{ width: 100, height: 100 }}
  onStartShouldSetResponder={() => true}
  onResponderRelease={(e) => {
    const hit = slices.findIndex((p) => ctx?.isPointInPath(p, e.locationX, e.locationY));
    if (hit >= 0) select(hit);
  }}
  draw={(c) => {
    ctx = c;
    slices.forEach((p) => c.fill(p));
  }}
/>
```

Strokes are tested with round joins and without dashes; coordinates given as native values count as 0.

**Gradients**: `createLinearGradient`, `createRadialGradient`, `createConicGradient`, each with `addColorStop(offset, color)`.

**Images**: `drawImage(image, dx, dy)`, `drawImage(image, dx, dy, dw, dh)`, `drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh)`.

`ctx.canvas.width` and `ctx.canvas.height` give the canvas's size in points.

**Not supported**: `getImageData` / `putImageData` and other pixel access, `filter`, SVG images in `drawImage`. An invalid color or font drops that one operation, as in HTML Canvas.

## Paths and Path2D

`Path2D` holds a path you build once and fill, stroke or clip many times. It takes SVG path data, parsed natively, which makes icons easy:

```tsx
import { Canvas, Path2D } from "nsh/canvas";

const heart = new Path2D("M12 21s-7-4.35-9.5-8.5C.5 8.5 3 4 7 4c2 0 3.5 1 5 3 1.5-2 3-3 5-3 4 0 6.5 4.5 4.5 8.5C19 16.65 12 21 12 21z");

function Heart(props: { size: number; color: string }) {
  return (
    <Canvas
      style={{ width: props.size, height: props.size }}
      draw={(ctx) => {
        ctx.scale(props.size / 24, props.size / 24); // the path is drawn on a 24-point grid
        ctx.fillStyle = props.color;
        ctx.fill(heart);
      }}
    />
  );
}
```

A `Path2D` has the same path methods as the context (`moveTo`, `arc`, `roundRect`, ...) and `addPath(other)`. `new Path2D(other)` copies a path.

## Gradients

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

`createConicGradient(startAngle, x, y)` sweeps colors around a point; its start angle is in radians, clockwise from the x axis. A conic gradient on a stroked arc makes a gradient progress ring.

## Text

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

Font families are the same as in styles: system families and the names of fonts your app imports (see [Fonts](./fonts.md)).

## Images

`drawImage` takes a URL, an imported asset (`import logo from "./logo.png"`) or `{ uri }`. The image is loaded natively, and the canvas draws again once it has loaded, so you do not need to track loading yourself. Remote URLs are subject to the app's [network capabilities](./sandbox-and-capabilities.md).

```tsx
import { Canvas } from "nsh/canvas";
import logo from "./logo.png";

<Canvas
  style={{ width: 120, height: 120 }}
  draw={(ctx) => {
    ctx.save();
    ctx.beginPath();
    ctx.arc(60, 60, 56, 0, Math.PI * 2);
    ctx.clip(); // a round avatar
    ctx.drawImage(logo, 4, 4, 112, 112);
    ctx.restore();
  }}
/>
```

The nine-argument form takes the source rectangle in the image's pixels.

## Animating with native values

Any number argument of a drawing call, and `fillStyle` / `strokeStyle`, can be a [native value](./animation.md#native-values) or a node derived from it. Such arguments are read while the platform draws, and when the value animates, the platform redraws on its own, every frame, without running `draw` or any other JS.

```tsx
import { mount, signal, Text, View } from "nsh";
import { createNativeValue, Easing } from "nsh/animated";
import { Canvas } from "nsh/canvas";

function Ring() {
  const progress = createNativeValue(0.3);
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
          // The end angle reads the native value on every frame.
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

The arc's end angle is `progress.mul(2π).sub(π/2)`: a node, so the ring sweeps natively for 900 ms while `draw` ran only once (and once more for the label, which reads a signal). A clock's second hand works the same way: `ctx.rotate(seconds.mul(Math.PI / 30))` with `seconds` animated linearly.

Colors animate the same way: `ctx.fillStyle = t.interpolate({ inputRange: [0, 1], outputRange: ["#54aeff", "#cf222e"] })`.

## Touch on a canvas

A canvas is a view, so it takes presses and the [responder system](./touch-and-gestures.md#the-responder-system). Finger drawing records points in a signal and draws them:

```tsx
const strokes = signal<[number, number][][]>([]);

<Canvas
  style={{ height: 240 }}
  onStartShouldSetResponder={() => true}
  onResponderGrant={(e) => {
    strokes.set([...strokes(), [[e.locationX, e.locationY]]]);
    return true; // a scroll view around must not take the touches
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

## Performance

- A drawing is recorded in Rust as JS calls the context, and travels to the platform as one operation; there is no per-call bridge crossing.
- `draw` re-records everything. Keep the work in `draw` proportional to what is shown, and keep data that changes every frame in native values rather than signals.
- Split a drawing that has a static part and a frequently changing part into two canvases stacked on top of each other, so only one redraws.
- Inside a zooming scroll view, canvases are drawn again at the zoomed resolution when the zoom ends.

## Related

- [`Canvas` element](../elements/canvas.md) and [`nsh/canvas`](../modules/nsh-canvas.md): the reference.
- [Animation](./animation.md): native values, `interpolate` and `Easing`.
- [Images and assets](./images-and-assets.md): importing images.

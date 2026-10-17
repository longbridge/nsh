# Canvas

- **Canvas** (`Canvas` from `nsh/canvas`, the `canvas` host element): an HTML Canvas 2D subset, recorded and replayed natively
  (`nsh_core::canvas`: `Drawing { ops, slots }`). `draw={(ctx, { width, height }) => ...}` runs again when the signals it reads
  change (sizes come from `onLayout`). `CanvasRenderingContext2D`: state (`save` / `restore`, `globalAlpha`,
  `globalCompositeOperation`, line styles and dashes, shadows, `font` (CSS shorthand; besides `small-caps` it takes the
  number forms `tabular-nums` / `proportional-nums` / `oldstyle-nums` / `lining-nums`), `fontVariantNumeric` (CSS's
  `font-variant-numeric`, which HTML Canvas lacks: figures of equal width for changing values; kept apart from `font`,
  which does not reset it; CoreText feature settings on iOS, `Paint.setFontFeatureSettings` on Android, ignored by
  browsers), `textAlign` / `textBaseline`),
  transforms, paths (`arc`, `arcTo`, `ellipse`, curves, `rect`, `roundRect`), `fill` / `stroke` / `clip` (also of a `Path2D`,
  which takes SVG path data; arcs become cubics), `fillRect` / `strokeRect` / `clearRect`, `fillText` / `strokeText` /
  `measureText` (synchronous, through the engine's text engine; only `width`), `isPointInPath` / `isPointInStroke`
  (synchronous too, answered in Rust without the platform: `nsh_core::canvas::point_in_path` replays what was recorded
  for its transform, current path and line style and tests the flattened path; after `draw` returned a kept context
  answers from everything it drew, so press handlers can hit-test the shapes; strokes take joins as round and ignore
  dashes; native values count as 0, since JS does not see their current values), linear / radial / conic
  gradients, `drawImage` (all three forms, images by URI or asset). Any number argument and colors may be native values
  (`createNativeValue`, `interpolate`, ...): those become slots bound like style values (`AnimationProp::CanvasArg`) and the
  platform redraws on its own while they animate, without JS (only what they change on iOS and macOS: "Partial
  redraws" below). Deriving a node again (`interpolate`, `mul`, ... in a `draw`
  that runs again) gets the live node made by the same operation (`nsh-js` `animated.rs` `derived`, keyed by the
  decoded `AnimationOp`, as `nsh_core::ui` does for Rust components): a redraw sends no new graph nodes and keeps its slots. A drawing travels in its own op (`Op::Draw`), recorded into
  a Rust list of its own (`nsh-js` `canvas.rs`): creating native values while drawing pushes ops of its own. Each
  operation is one variant of `CanvasOp` / `PathOp` with `#[derive(JsOps)]` (`nsh_core::js::ops`): `#[op(code = CV_…)]` reads its fields in order
  (`OpArg`: numbers or native values, paints, `Path2D` lists, string unions; an invalid color or font drops the
  operation, as in HTML Canvas), and `method = …, params = …` / `property = …, initial = …` describe the methods and
  properties that only record (`moveTo`, `fillRect`, `lineWidth`, ...): `nsh-js` makes them natively from these specs
  (`CanvasOp::methods` / `properties`) and their declarations are generated (`canvas::declarations`); what has logic
  (overloads, `roundRect`'s radii, `save` / `restore`, gradients, images) is written in `canvas.rs`. The core's decoder
  (`nsh_core::js::canvas::decode`) reads the recorded list. Recording costs: `docs/dev/performance.md`. iOS replays
  into the `drawRect:` context, keeping its own transform and path in view space; Android `CanvasBox.java` interprets an `int[]` (`encode.rs`). Not
  supported: `getImageData` / `putImageData`, filters, SVG images.

**Partial redraws.** While native values animate, iOS (`setNeedsDisplayInRect:`) and macOS (`NshView::redraw_rect`:
`setNeedsDisplayInRect:` on the content layer) redraw only what changes: `nsh_core::canvas::DamageScan` replays the
operations for their geometry (transform with `save` / `restore`, bounds of the current path and of `Path2D`s, line
width / joins / caps, shadows) and adds up the bounds of the painting operations that read a slot whose value changed,
through their own numbers or the state they paint with (transform, current path, `globalAlpha`, paint, line width,
dash offset). The platform scans with the old values and the new ones and invalidates the union, inflated for
anti-aliasing (2 pt) and rounded out to whole points; the platform then replays the whole drawing clipped to it
(`drawRect:` on iOS, the content layer's `drawLayer:inContext:` on macOS; Core Graphics skips what falls outside), so
the pixels are those of a full redraw (on macOS also with `drawsAsynchronously`: checked against a full redraw by
screen capture). Bounds are conservative: curves by their control points, arcs by their circle, strokes by half the
width under the transform's largest scale times the miter limit (miter joins) or √2 (square caps), shadows by twice
their blur past their offset, text by 1.5 em per character and 1.5 em above and below the baseline (no text engine at
hand). The scan falls back to a full redraw (`Damage::All`) for a clip that reads a changed value, a painting
operation that reads one under a composite operation other than `source-over` (`copy`, `source-in`, ... change pixels
outside the shape), an image at its natural size and bounds that are not finite. A scan allocates nothing (its save
stack is kept); two per changed frame cost microseconds next to the replay. Linux (GTK 4 redraws whole widgets:
`queue_draw` takes no area), Android, the web runtime and the GPUI backend (which rasterizes the drawing, text outlines
included, with tiny-skia into a pixmap in device pixels: `nsh-platform-gpui` `canvas.rs`) still redraw the
whole canvas. Numbers:
`docs/dev/performance.md`.

Rust components record drawings with `ui::Painter` (the same operations; `docs/dev/components.md`), measure text
with `RenderCx::measure_text`, on the core's text engine, and hit-test what a painter recorded with the same
`point_in_path` / `point_in_stroke` (`Painter::is_point_in_path` / `is_point_in_stroke` for the current path,
`path_contains` / `stroke_contains` for a `Path`; native values count as 0 there too).

The drawing format is documented in `crates/nsh-core/src/canvas.rs`.

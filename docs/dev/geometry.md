# Units and geometry

NSH's Rust code types its units and shapes as GPUI's `gpui::geometry` does, in `nsh_core::geometry`
(`crates/nsh-core/src/geometry/`). A length is never a bare `f32`: it is `Points`, `Rems`, a `Percentage` or one of the
lengths styles give; an angle is `Radians`; a rectangle is `Bounds<Points>`. Mixing points with fractions, or degrees
with radians, is a type error instead of a bug found on a device. The convention is held by a test (["Typed
API"](#typed-api)).

## The types

| NSH | GPUI | What |
|---|---|---|
| `Points(pub f32)`, `px(f32)` | `Pixels`, `px` | A length in points, the unit NSH lays out in: iOS / macOS points, Android dp, CSS `px` on the web, DIPs on Windows. A style string's `px` and bare numbers are points |
| `Rems(pub f32)`, `rems(f32)`, `REM_SIZE` | `Rems`, `rems` | Multiples of a root font size; style strings and Tailwind's default theme resolve a rem at 16 points (`REM_SIZE`) |
| `Percentage(pub f32)`, `percentage(f32)` | `Percentage`, `percentage` | A fraction of a whole: `0.5` is 50% (CSS percentages are kept as fractions everywhere) |
| `Radians(pub f32)`, `radians(f32)`, `deg`, `turn`, `grad` | `Radians`, `radians` | An angle; `deg` / `turn` / `grad` convert CSS's other angle units. `Radians::from(Percentage)` is that fraction of a turn |
| `DevicePixels(pub i32)`, `ScaledPixels(pub f32)` | the same | Physical pixels (rounded / unrounded), only where a platform draws or reports pixels |
| `AbsoluteLength { Points, Rems }` | `AbsoluteLength { Pixels, Rems }` | A length known without layout; `to_points(rem_size)` |
| `DefiniteLength { Absolute, Fraction(Percentage) }`, `relative(f32)` | `DefiniteLength { Absolute, Fraction(f32) }`, `relative` | Not `auto`: absolute or a fraction of a reference (the parent for a box, the gradient line for a color stop); `to_points(base_size, rem_size)` |
| `Length { Definite, Auto }`, `auto()` | `Length`, `auto` | What a style gives a box |
| `Point<T>`, `Size<T>`, `Bounds<T>`, `Edges<T>`, `Corners<T>`, `point`, `size`, `bounds` | the same | Shapes generic over their unit, with GPUI's methods: `map`, `along` / `apply_along` (`Axis`, `Along`), `max` / `min` / `clamp`, `center`, `contains`, `intersects` / `intersect` / `union`, `dilate` / `inset` / `extend`, `space_within`, `localize`, `scale` / `to_device_pixels`, `Corners::clamp_radii_for_quad_size`, and the arithmetic operators |
| `Half`, `IsZero` | the same | What `center` and the zero checks need |

The units are `#[repr(transparent)]` newtypes and the shapes `#[repr(C)]` structs of them: they cost nothing on hot
paths (`nsh trace` timings are unchanged, docs/dev/performance.md). Every unit has the operators of its kind (`+`, `-`,
`*` / `/` by `f32`, a ratio `Points / Points -> f32`, `Sum`), `ZERO`, `as_f32()`, `abs`, `min` / `max` / `clamp`,
`floor` / `round` / `ceil`, and `Eq` / `Hash` on the bits (`0.0` and `-0.0` hash alike) so lengths can sit in hashed
declarations. They serialize as numbers.

Rust components style with them: `Styled`'s length methods take `impl IntoReactive<Length>`, so `.w(px(120.0))`,
`.w(relative(0.5))`, `.p(rems(1.0))`, `.h(Length::Auto)` and closures returning any of them all work.

### Deviations from GPUI, and why

- **Points, not pixels.** NSH lays out in the platform's density-independent unit on every backend; GPUI's `Pixels`
  are the same idea under another name. `px()` keeps GPUI's (and CSS's) name.
- **`f32`'s comparisons.** `PartialOrd` and `min` / `max` are IEEE's, not GPUI's total order: a new view's frame starts
  as NaN (`Bounds::UNSET`) so that it never equals the first real frame, and `-0.0 == 0.0` as layout expects. There is no
  `Ord`.
- **A public `.0`.** GPUI keeps `Pixels`' field to its crate; NSH's platform backends are crates of their own and hand
  values to their toolkits, so the field is public (`as_f32()` reads it too).
- **`DefiniteLength::Fraction` holds a `Percentage`** rather than a bare `f32`, so a fraction stays typed; `relative()`
  takes the `f32` as GPUI's does.
- **No range check on `percentage()`.** CSS takes percentages outside `0..=1` (a stop at 150%, `translate(-50%)`).
- **CSS angle constructors** (`deg`, `turn`, `grad`) beside GPUI's `radians`.
- **No bounds on the shapes' type parameter** (GPUI requires `Clone + Debug + Default + PartialEq` on the struct); the
  methods ask for what they use.
- **Left out** until something needs them: GPUI's `Anchor` / `Corner`, `GridLocation`, `phi()`, and `JsonSchema`.

## Where conversion happens

Inside NSH values stay typed; they become bare numbers only at a boundary, in one place each:

- **taffy** (`geometry/taffy.rs`): `From` impls between `Length` / `DefiniteLength` / `AbsoluteLength` and taffy's
  `Dimension` / `LengthPercentage(Auto)`, `Size<Points>` and taffy's `Size<f32>`, a laid out node's `taffy::Layout` and
  `Bounds<Points>`. The core's layout code computes in taffy's `f32` and wraps results (`geometry::rect`, `size_pt`,
  crate-private).
- **Platforms**: a mount item's frame is a `Bounds<Points>`, a scroll content size a `Size<Points>`, the environment's
  window, safe area, title bar and keyboard are `Size<Points>`, `Edges<Points>` and `Points`. The wire types are typed
  the same way: insets are `EdgeInsets` (`Edges<Points>`), touch, hover and context menu positions `Point<Points>`
  (`page`, `location`), `onLayout` a `Bounds<Points>`, scroll offsets and snap points `Points`, a scroll-to-index
  position a `Percentage`, and a loaded image's size `Size<DevicePixels>` (the pixels of its source). A backend reads
  `.0` (or `as_f32()`) where it hands a value to UIKit, AppKit, Android, GTK, Direct2D, GPUI or the DOM, and wraps what
  its toolkit reports (`Points(width)`). The encodings to Android and the web are unchanged: they write the same
  `f32`s. A value that reaches `format!` (the web's JSON) takes `.0`: `Points`' `Display` writes `10pt`.
- **JS** (`nsh-js`): numbers decoded from JS become typed values where they enter the core's types, and typed values
  become JS numbers where they leave (the environment, events).

## Typed API

nsh-core's `geometry::unit_types` test (`cargo test -p nsh-core unit_types`) parses `crates/nsh-core/src` with `syn` and
flags every bare `f32` / `f64` in public API: a `pub fn`'s parameters and return type (free functions, inherent
methods, public traits' methods), `pub` struct fields, public enums' variant fields, `pub` consts, statics and type
aliases; inside `Option`, arrays, tuples, references, generics and closures as well. A trait impl's methods are not
flagged (the trait decides them), nor are test modules, what macros generate, the `geometry` module (the units'
constructors take the bare value) and `color` (a color's components have no unit). `pub` is public API: the workspace's
`unreachable_pub` lint keeps every `pub` item reachable. Colors (`Color`, and GPUI's `Hsla` / `Rgba` if NSH adopts them)
are typed; a bare float or integer holding a color is not.

Each flagged item must be listed in one of two files next to the crate's `Cargo.toml`, by its path (`module::Type::method(param)`,
`module::Type::method ->` for a return type, `module::Struct.field`, `module::Enum::Variant.field`):

- `unitless.txt`: quantities without a unit, each with the reason (`# opacity, 0..1`): opacities, scale factors, flex
  grow / shrink, ratios, durations, velocities, font weights. A new entry needs a reason that holds.
- `untyped.txt`: lengths, angles and fractions not migrated yet, grouped as below. It only shrinks: the test fails when
  an entry is typed now (remove it) and when a flagged item is in neither file (new public API uses the geometry
  types).

## Migration

What `untyped.txt` lists, grouped so that agents can migrate in parallel without touching the same files. Each group is
one pull request; it removes its entries from `untyped.txt` and keeps behavior and wire formats.

Counts are the entries of `untyped.txt` when the check landed (379 in all; `unitless.txt` held 225).

| Group | Files (nsh-core) | Entries | What |
|---|---|---|---|
| A | `effects.rs`, `css.rs`, `transform.rs`, `shape.rs` | 24 | Box and drop shadows' offsets, blurs and spreads (`Points`), `Filter::Blur` / `HueRotate` (`Points`, `Radians`), outlines, `blur_radius`; `transform::Decomposed` (translate in `Points`, rotate in `Radians`), `Transform::resolve_fractions`; `shape::Segment` points (`Point<Points>`), `rounded_rect` |
| B | `style.rs`, `tailwind/` | 43 | `CornerRadii` (`Corners<Points>`), `Borders.widths` (`Edges<Points>`), shadows, `TextStyle`'s font size, letter spacing, line height, decoration (`Points`), `OriginLen` (`DefiniteLength`), `TransformOrigin`, `ViewStyle` radii and elevation. Another pull request reorders `style.rs`'s border / outline / animation precedence: land after it |
| C | `text.rs`, `canvas.rs`, `canvas/`, `ui/host.rs` (`Painter`) | 126 | Text metrics (`TextLine`, `LinePiece`, `BackgroundBox`, attachments: `Points`, `Bounds<Points>`), `TextEngine::measure(max_width)`; the canvas's ops, gradients, hit tests and `Painter`'s methods (`Point<Points>`, `Bounds<Points>`, `Radians` for arcs and rotations, `Percentage` for color stop offsets). The recorded canvas ops' wire encoding stays |
| D | `ui/` except `host.rs` | 100 | `Styled`'s remaining methods (`border(width)`, `rotate(degrees)` → `Radians`, `translate_x`, `blur`, shadows, `gradient_*_position` → `Percentage`), events (`ScrollEvent`, `PanEvent`, `ResponderEvent`, `VideoInfo` → `Size<DevicePixels>` for pixel sizes), `ElementRef::scroll_to*`, `Scrollable`, `ListLayout`, `Theme` lengths, `ambient::keyboard_height` / `measure_text` / `round_to_nearest_pixel`, image and clipboard sizes (`Size<DevicePixels>`) |
| E | `ops.rs`, `props.rs`, `mount.rs`, `core/`, `platform.rs`, `desktop.rs`, `js/`; `nsh-js`'s decoding; the backends that read them | 86 | The wire types: `EventPayload` (layout, scroll, pan, image), touch / hover / context menu locations (`Point<Points>`), `Prop` / `ScrollProp` lengths, `props::EdgeInsets` (→ `Edges<Points>`), `PanActivation`, `Command::ScrollTo` / `ZoomTo`, `ScrollBehavior` snapping, `StickyCell`, `ScreenConfig.header_height`, `WindowFrame` (`Bounds<Points>`), `Core::set_scroll_offset` and the header heights. JS numbers become typed where `nsh-js` decodes them; the backends unwrap them where they hand them to their toolkits |
| F | `nsh-js`, `nsh-engine` | – | Extend the check to their public API (the scan is a function of a source directory; each crate gets its own two lists) |

Groups A–E touch disjoint files in nsh-core; a group that changes a type other crates read (C's text metrics, E's wire
types) also updates those readers (`nsh-js`, `nsh-engine`, the backends, `crates/gpui`), so C and E should not land at
the same time as each other's backend edits without a rebase. `nsh-ui-mobile` is not scanned and not migrated (it is
being removed); it changes only where it would otherwise fail to compile.

Each group checks its backends as this migration did: `cargo test --workspace`, the GPUI workspace, and every backend
through `cargo check --target …` (iOS / Catalyst, macOS, Windows, Android, `wasm32`, Linux). Values that reach a
`format!` (CSS strings on the web, text the gallery shows) need `.0` or the typed `Display` (`10pt`), and an angle that
reaches a toolkit needs the unit that toolkit takes (`to_degrees()` for GSK, Android, CSS): the compiler does not catch
either, so review them by hand.

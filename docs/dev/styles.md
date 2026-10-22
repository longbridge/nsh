# Styles, classes and effects

- **CSS and Tailwind as in a browser**: CSS values (colors, lengths, gradients, filters, shadows, transforms,
  alignment keywords, ...) and Tailwind utilities and variants mean exactly what they mean in a browser and in Tailwind
  CSS (v4), whose generated CSS is the reference for each utility (the tests compare against it). What NSH cannot
  support is unsupported (an unknown class, a rejected value with a warning), never given a different meaning. Inline
  style objects keep React Native's defaults for unset keys (a column, `flexShrink: 0`, `alignContent: flex-start`
  outside grids). Colors (`crates/nsh-core/src/css.rs`) are hex, `rgb()`, `hsl()`, `oklch()`, `oklab()`, names and
  `color-mix()` of any of them (CSS Color 5: `in srgb`, `srgb-linear`, `xyz`, `oklab`, `oklch` or `hsl`, hue methods
  for the polar spaces, premultiplied alpha), packed to sRGB with clamped channels.
- **Tailwind** (`class` / `classList` on every element): JS only passes the strings; the core resolves them (how, and
  what is cached: `crates/nsh-core/src/tailwind/mod.rs`, `core/classes.rs`). The inline `style` always wins over classes; a key set to `undefined` sets nothing, so the class value (or the default) stays, while `null` sets the default.
  Later utilities win over earlier ones (components append a `class` prop, no `tailwind-merge`); variant rules apply
  after plain ones in Tailwind's variant order. Display follows CSS: `flex` / `inline-flex` are rows unless a direction
  utility says otherwise, wherever it is in the string (`flex-col flex` is a column; the implied row is a
  `Decl::Default`, applied before every rule), while views without them keep React Native's column; `block` /
  `flow-root` use Taffy's block layout; `inline` / `inline-block` lay out as views (there is no inline formatting
  context). Variants: `dark:`, `sm:`…`2xl:` / `max-*:` / `min-[..]:`, `portrait:` /
  `landscape:`, `ios:` / `android:` / `web:`, `rtl:` / `ltr:`, `motion-safe:` / `motion-reduce:`, `active:` / `hover:` /
  `focus:` / `focus-within:` / `disabled:` / `enabled:` / `selected:` (`accessibilityState.selected`, a `Button`'s
  `selected`) / `checked:` (`accessibilityState.checked`, not mixed) / `indeterminate:` (`accessibilityState.checked`
  mixed, CSS's `:indeterminate`), `group-*:` (named groups `group/name`), `first:` / `last:` /
  `odd:` / `even:` / `only:`, `placeholder:` / `selection:` (a color only: the `placeholderColor` / `selectionColor`
  style keys), `!` (important). The state classes have style props of their own (`activeStyle`, `hoverStyle`,
  `focusStyle`, `disabledStyle`, `checkedStyle`, `indeterminateStyle`, a `Button`'s `selectedStyle`), resolved the same
  way: later states win, `disabled` over all. `selected`, `checked` and `indeterminate` are value states the core sets
  from the accessibility state (never reported by the platform), below the interaction states: `checked` and
  `indeterminate` (one rank: a view is never both), then `selected`, then `hover`, `focus`, `active` (gpui-base's order
  of value states). `disabled` (any element) applies `disabled:` and drops
  the node's press listeners from what is mounted, so it shows no press feedback either. A part of a Rust component
  shows its control's `checked`, `indeterminate` and `disabled` to its own classes and state styles
  (`Interactive::project_checked`, `project_indeterminate`, `project_disabled`; `Prop::ProjectedStates`): a switch's thumb styled `checked:translate-x-4`, with no accessibility
  state or press change on the part, so screen readers hear the control alone. `dark:` follows the appearance
  shown: the app's own when it set one (`nsh.json` `"appearance"`, `setColorScheme`; `Core::set_appearance`), else the
  system's. The theme is Tailwind 4's
  extended by `nsh.json` `"tailwind"` (`colors`, `spacing`, `breakpoints`, `fontSize`, `radius`, `shadow`, ...; unknown
  keys are build errors). Rust components do not use classes: their typed styles (`ui/style.rs`, every utility group has its
  methods, tested against the classes) lower into the same rules and their
  colors come from `nsh/base`'s theme, `nsh.json` `"theme"` (`docs/dev/components.md`); the Tailwind theme only
  shapes the app's own classes. The app's classes name the design tokens (`ui::Tokens`: shadcn/ui's and the app's
  own, `nsh.json` `"theme"`, by the names of `tailwind/theme.rs` `ClassTokens`) as Tailwind 4 names the variables of
  `@theme`: a color is a color utility's name (`bg-primary`, `text-muted-foreground/70`, `bg-up/30`) when the
  Tailwind palette has no color of that name (a palette color of the same name wins), and every token is a variable
  (`bg-(--primary)`, `rounded-(--radius)`, `text-(length:--x)`, `font-(family-name:--font-mono)`); the fonts
  `font-sans` / `font-mono`, when the theme sets them, are what the `font-sans` / `font-mono` utilities set
  (`ClassTokens::replaces`), and `font-sans` is the default font of every text (Tailwind's `--font-sans`, a page's
  preflight). Such a utility is parsed with the light and the dark values as arbitrary ones (two rules of its rank
  when they differ, so it follows the appearance with no `dark:`) and depends on `deps::THEME`. When the design
  tokens the app draws with change (`Core::set_tokens`, `set_host_tokens`; a host's keep the app's names they lack,
  `Tokens::over`), `Core::class_tokens_changed` drops the cached strings that name tokens and resolves
  their nodes (and the app's classes on Rust components) again; when the names change, every class. `nsh build` / `nsh dev` warn about unknown static classes in app code (`file:line:col:
  unknown class '...'`, target `nsh::build`); the core logs each unknown class once at run time. Class-only settings
  that are props in React Native (`line-clamp-*`, `truncate`, `whitespace-nowrap`, `pointer-events-*`, `object-*`,
  `caret-*`) lose to explicit props; `placeholder:text-*` / `selection:text-*` lose to the `placeholderColor` /
  `selectionColor` style keys. Not supported: `peer-*`, pseudo-elements, arbitrary CSS
  properties; `space-x/y-*` are gaps.
- **`position: "sticky"`** is laid out `relative` with its insets kept as where it sticks (`Node::sticky`): after
  layout the core gives it a range of travel in the offsets of the nearest scroll view or own list around it (from its
  place less its `top`, or `left` in a horizontal one, to where its end meets its parent's content box) and sends it
  with that view's sticky list rows (`core/sticky.rs`, `MountItem::StickyCells`), which the platforms pin on every
  scroll. It draws above its siblings of the same `zIndex` (`Node::stack_rank`), as above a page's in-flow boxes.
  `bottom` / `right` stickiness and a zoomable scroll view are not supported. The `sticky` class keeps the meaning
  v0.2.0 recorded (it sets nothing).
- **Text style inheritance** (CSS-like): a view's text properties (color, font, size, alignment, ...) pass to the texts and inputs
  inside it, so `class="text-white"` on a button styles its label. Inset padding / margin (set by Rust
  components: `SafeAreaView`, `KeyboardAvoidingView`) are added to the resolved padding / margin, so classes and
  styles keep their own.
- **Effects** (styles and classes; `effects.rs`): `boxShadow` (CSS syntax, several, `inset`; also `shadow-*` / `ring-*`),
  `backgroundImage` (linear / radial / conic gradients and their `repeating-` forms,
  drawn over `backgroundColor` as in CSS; stops at percentages, at lengths along linear and radial gradients
  (a `DefiniteLength::Absolute` position: `Gradient::resolve_stops` places them against `Gradient::line_length` of the box when it is
  drawn, so every native backend passes the view's size; Android's `Decor.Gradient.resolve` does the same for the
  gradients `encode.rs` sends as written, with their stop count negated; the web hands them to the browser as `px`), or
  at angles around conic ones (fractions of a turn), up to two per stop; transition hints are not parsed; a gradient
  that does not parse is logged (inline styles, `bg-[…]` classes, Rust components' `Style`); but press feedback must show, so a background color an interaction state sets (`active:` / `hover:` / `focus:` /
  `focus-within:` / `group-*:` classes, Rust components' `.active(..)`, on a nested component's element too, `activeStyle` / `hoverStyle` / `focusStyle` / `disabledStyle`) or a native
  animation drives (`Animated` `backgroundColor`) replaces the gradients, unless that state sets a gradient of its own:
  done in the core, so every platform shows it alike), `filter` (`brightness`, `contrast`,
  `grayscale`, `hue-rotate`, `invert`, `saturate`, `sepia`, `opacity`, `blur`, `drop-shadow`; color filters are composed into one
  matrix), `backdropFilter` (blur; iOS chooses its system material through the
  `ios={{ backdropMaterial }}` prop: `ultraThin` by default, then `thin`, `regular`, `thick`, `chrome`;
  the radius approximates strength as a fraction of the material, rather than a Gaussian radius), `glassEffect`
  (Apple's Liquid Glass, drawn in the `backdropFilter`'s place on iOS 26 and later and nowhere else, so give a glass
  view a blur to fall back to), `maskImage` (a gradient whose alpha decides how much of the view is drawn, its backdrop
  filter included; iOS and web, the topmost layer of several), `mixBlendMode`, `outline*`, `visibility`, `cursor`, `transition*` (CSS: property groups,
  duration, delay, easing) and `animation*` (Tailwind's `spin` / `ping` / `pulse` / `bounce`), `display: grid` with
  `gridTemplateColumns/Rows` (line names: `grid.rs` turns them into taffy's `grid_template_*_names`, one set per
  line between components, a `repeat()` counting as one), `gridAutoFlow/Columns/Rows`, `gridColumn/Row(Start/End)` (by
  number or name, `span name`), `justifyItems/Self`; CSS's logical `start` / `end` / `self-start` / `self-end` (taffy's,
  right to left aware) besides `flex-start` / `flex-end`; an unset `alignContent` / `justifyContent` is taffy's `normal`,
  which `style::resolve_normal_alignment` makes `flex-start` outside grids when a style reaches the tree, so grids
  stretch their `auto` tracks as CSS does while flex keeps React Native's default (the alignment and grid classes set
  exactly what Tailwind CSS generates for them: `content-start` is `flex-start`, `place-content-start` /
  `place-items-*` / `place-self-*` `start` / `end`, `justify-items-*` `start` / `end` / `normal`, `justify-normal`
  `normal`, `justify-self-*` / `items-*` / `self-*` `flex-start` / `flex-end`; `content-normal` sets `stretch`, which
  is what CSS's `normal` does in flex and grid containers, since the style's `normal` is the unset value; a type hint
  is dropped from an arbitrary value (`col-[number:2]` is line 2), classes Tailwind has no negative of
  (`-grid-cols-*`, `-col-span-*`, `-auto-cols-*`) are unknown, and an invalid arbitrary `col-[…]` / `row-[…]` /
  `auto-cols-[…]` / `auto-rows-[…]` sets nothing, as a browser drops the declaration); text
  `textDecorationStyle` (`double` / `dotted` / `dashed` / `wavy`), `textDecorationThickness`, `textUnderlineOffset`, `em`
  letter spacing and unitless line heights (multiples). Translations by a fraction of the size (`translate-x-1/2`) are resolved by
  the core against the frame and sent again when the size changes.
  iOS: shadows are one `CALayer` per shadow with a `shadowPath` (outer shadows masked to outside the box, inset ones to the padding
  box), gradients `CAGradientLayer`, transitions explicit `CABasicAnimation`s from the presented values, keyframes
  `CAKeyframeAnimation`s, blend modes `compositingFilter`, `cursor` a `UIPointerInteraction`, backdrop blur a
  `UIVisualEffectView` (a system material at a fraction: an approximation), text decorations other than solid single lines are
  drawn by `TextView`. Android: `Decor` draws shadows, gradients and outlines; filters are `RenderEffect`s (API 31+; below, only
  color filters through a layer paint), blend modes a layer paint (API 29+), transitions / keyframes `ValueAnimator`s,
  backdrop blur (API 31+) a `RenderNode` recorded before each frame with what lies behind the view (`Backdrop.java`: each
  ancestor's background and the siblings drawn before the next ancestor, never the view itself) and blurred by a
  `RenderEffect`, drawn clipped under the view's background.
  Known limits: iOS has no CSS filters on views (only `opacity`, `brightness` ≤ 1 and `drop-shadow`; images take every color filter
  and `blur`, applied while decoding); iOS texts draw no gradients or inset shadows; Android has no backdrop blur, `blur` or
  `drop-shadow` before API 31.
- **View styles**: `backgroundColor`, `borderWidth` / `borderColor` (also per side: `borderTopWidth`, `borderLeftColor`, ...),
  `borderStyle` (`solid` / `dashed` / `dotted`), `borderRadius` + the four `borderTopLeftRadius`-style corners, `borderCurve`
  (`continuous`: corners easing into the edges, Figma's 60% corner smoothing, shrinking to circular where the box is too
  small, so pills stay pills; the outline geometry is `nsh_core::shape::rounded_rect`, ported to `Decor.java`; iOS layers that
  draw their own corners use `cornerCurve`), `overflow`, `opacity`,
  `transform` (translate / scale / rotate / rotateX / rotateY / skew / perspective, applied right to left like CSS; Android has no
  skew), `zIndex` (stable order of the native views), `shadowColor` / `shadowOffset` / `shadowOpacity` / `shadowRadius` (RN
  defaults: offset `(0, -3)`, radius 3, opacity 0), `tintColor` (images), `objectPosition` (images: where the fitted image sits, CSS's `object-position`; `images.md`), `android={{ elevation }}` (a prop, not a style: Android's material shadow, with
  `shadowColor` from API 28), `transformOrigin` (keywords / percentages / lengths; iOS conjugates the matrix around it,
  Android sets the pivot), `backfaceVisibility` (iOS `doubleSided`; Android hides the view while its rotations turn it away). `style` may be an array of styles merged left to right
  (falsy entries skipped), decoded in Rust. In one style object (an array is flattened into one, as React Native does) a
  longhand wins over its shorthand whatever the key order (`StylePatch::set`, `StyleKey::longhands`: border widths and
  colors per side, margin / padding sides and axes, `rowGap` / `columnGap`, `flexGrow` / `flexShrink` / `flexBasis`,
  grid start / end): a shorthand set after its longhands is recorded before them and they apply again, and a `null`
  longhand of a shorthand in the same object is dropped (unset, as in Yoga). Corner radii keep their own fields
  (`corner_radius` over `border_radius`), start / end properties are placed after the physical ones
  (`apply_direction`). Between layers (classes, then the inline style; a Rust component's own style, then the app's)
  order still decides, and classes keep "later wins". `outlineStyle: "none"` hides the outline whatever the width, and
  the `animation*` timing keys apply with or before `animationName` (`Style::pending` keeps them); `transition*` keys
  already combine in any order (`transitionProperty` defaults to `all`). Text adds `textShadowColor` / `textShadowOffset` / `textShadowRadius`,
  `fontVariant` (OpenType `smcp` / `onum` / `lnum` / `tnum` / `pnum`: CoreText feature settings, `Paint.setFontFeatureSettings`)
  and `writingDirection`.
  On Apple everything is drawn by Core Animation, escalating only as needed (`crates/nsh-platform-ios/src/decor.rs`
  describes the paths and their limits); Android draws in `Decor.java`.

## Colors

Colors in Rust follow GPUI's design (`nsh_core::color`; GPUI is a reference, not a dependency). **`Hsla`** is the
color Rust code holds, passes and computes with: theme colors (`AdaptiveColor { light: Hsla, dark: Hsla }`), the
`Styled` color methods (`.bg(..)`, `.text_color(..)`: anything `Into<AdaptiveColor>`, so an `Hsla`, an `Rgba` or a
theme color), host element setters (`.placeholder_color(..)`, `.header_tint_color(..)`), the canvas painter, the
palette (`palette::SKY.shade(500)`), `color(..)` (an adaptive color for the appearance now) and Rust components'
props (decoded from the same CSS strings as `Color`, so their TypeScript is unchanged). **`Rgba`** is for conversions,
as in GPUI; `rgb(0x2563eb)` and `rgba(0x2563eb80)` build one, `hsla(h, s, l, a)` (each `0..=1`, the hue in turns) and
`hsl(deg, %, %)` (gpui-kit's) build an `Hsla`, and `black()`, `white()`, `transparent_black()`, `opaque_grey(l, a)` are
GPUI's constants. `Rgba::to_hsla` and `Rgba::to_packed` are `const`, for colors in `const`s.

The packed **`Color`** (`0xRRGGBBAA`) is only the storage and wire form: what the style pipeline diffs and caches
(`Style`, `BoxShadow`, gradient stops, `Prop` values), mount items, the Android and web encodings and the fixtures'
`#rrggbbaa`. A color becomes packed where it enters a style, a prop or an operation (`Declaration::lower`, the host
setters), and nowhere earlier.

Adjusting a color is a method of `Hsla`, so state colors are derived rather than picked (a pressed shade is
`c.blend(opaque_grey(0.0, 0.12))` in light appearance and `opaque_grey(1.0, 0.12)` in dark):

| Method | Does |
|---|---|
| `opacity(f)`, `alpha(a)`, `fade_out(f)` | alpha times `f`; set to `a`; times `1 - f` (in place) |
| `lighten(f)`, `darken(f)` | lightness times `1 + f` (at most 1), times `1 - f` (gpui-kit's) |
| `saturate(f)`, `desaturate(f)`, `grayscale()` | the same for the saturation; no saturation |
| `spin(deg)`, `hue(h)`, `saturation(s)`, `lightness(l)`, `apply(base)` | rotate the hue; set one channel; take `base`'s hue and saturation |
| `invert()`, `invert_l()` | `1 - x` for hue, saturation and lightness; for the lightness only |
| `blend(over)` | `over` drawn on top by its alpha (source over, sRGB), keeping this alpha |
| `mix(other, f)`, `mix_oklab(other, f)` | `f` of this color, in HSL (the hue the short way) or in Oklab (CSS `color-mix(in oklab, …)`) |
| `luminance()`, `contrast_ratio(other)`, `readable_text()` | WCAG's relative luminance and contrast ratio; white or black, whichever reads better on it |
| `to_hex()`, `Hsla::parse_hex(..)`, `"…".parse::<Hsla>()` | `#RRGGBB(AA)`; hex; any CSS color NSH reads |

`AdaptiveColor::opacity(f)` and `AdaptiveColor::map(|c| ..)` apply one to both appearances.

**Rounding** is fixed so that TSX apps render exactly as before and every packed color survives a round trip: packed
to float is `byte / 255`; float to packed clamps to `0..=1`, scales by 255 and rounds halves away from zero (the rule
`rgb()`, `hsl()` and `oklch()` in CSS pack with); HSL to RGB is CSS Color 4's formula (`color::hsl_to_rgb`, shared
with `hsl()`), so `hsla(h, s, l, a)` packs as `hsl(h·360 s·100% l·100% / a)` does. Tests hold all of it: every 24-bit
color through `Hsla` and back, a table of CSS color strings with their packed values (`css.rs`), `opacity` against the packed alpha scaling it replaced (the same for
opaque colors; for translucent ones within one alpha step, where `alpha · factor` is exactly a half).

**Deviations from GPUI**, on purpose: packing rounds (GPUI's `From<Rgba> for u32` truncates, which loses colors on a
round trip); parsing takes every CSS color, not only hex, and fails with `ColorParseError` instead of `anyhow`;
gpui-kit's `Colorize` methods are inherent methods of `Hsla` (no trait to import), without `divide` (it sets the alpha:
`alpha`), and `lighten` clamps the lightness to 1; `saturate`, `desaturate`, `spin`, `luminance`, `contrast_ratio`,
`readable_text` and the `const` conversions are NSH's; no serde, schemars or proptest implementations.

## Android differences

- **Android differences**: shadows need a background (no content shadows, e.g. text with a transparent background); odd corner
  radii clip by path before API 33; pull to refresh is our own (stretch overscroll stays native); `contentInset` is padding and
  `decelerationRate` only shapes snapping glides.

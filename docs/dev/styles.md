# Styles, classes and effects

- **Tailwind** (`class` / `classList` on every element): JS only passes the strings; the core resolves them (how, and
  what is cached: `crates/nsh-core/src/tailwind/mod.rs`, `core/classes.rs`). The inline `style` always wins over classes; a key set to `undefined` sets nothing, so the class value (or the default) stays, while `null` sets the default.
  Later utilities win over earlier ones (components append a `class` prop, no `tailwind-merge`); variant rules apply
  after plain ones in Tailwind's variant order. Display follows CSS: `flex` / `inline-flex` are rows unless a direction
  utility says otherwise, wherever it is in the string (`flex-col flex` is a column; the implied row is a
  `Decl::Default`, applied before every rule), while views without them keep React Native's column; `block` /
  `flow-root` use Taffy's block layout; `inline` / `inline-block` lay out as views (there is no inline formatting
  context). Variants: `dark:`, `sm:`…`2xl:` / `max-*:` / `min-[..]:`, `portrait:` /
  `landscape:`, `ios:` / `android:` / `web:`, `rtl:` / `ltr:`, `motion-safe:` / `motion-reduce:`, `active:` / `hover:` /
  `focus:` / `focus-within:` / `disabled:` / `enabled:`, `group-*:` (named groups `group/name`), `first:` / `last:` /
  `odd:` / `even:` / `only:`, `placeholder:`, `!` (important). `disabled` (any element) applies `disabled:` and drops
  the node's press listeners from what is mounted, so it shows no press feedback either. `dark:` follows the appearance
  shown: the app's own when it set one (`nsh.json` `"appearance"`, `setColorScheme`; `Core::set_appearance`), else the
  system's. The theme is Tailwind 4's
  extended by `nsh.json` `"tailwind"` (`colors`, `spacing`, `breakpoints`, `fontSize`, `radius`, `shadow`, ...; unknown
  keys are build errors). Rust components do not use classes: their typed styles (`ui/style.rs`, every utility group has its
  methods, tested against the classes) lower into the same rules and their
  colors come from the component theme, `nsh.json` `"theme"` (`docs/dev/components.md`); the Tailwind theme only
  shapes the app's own classes. The app's classes name the component theme's tokens (`tailwind/theme.rs` `Tokens`:
  every color and number of the theme by its key, dots as dashes, `.background` optional; `bg-primary` when the
  Tailwind theme has no such color, `rounded-(--radius)`): such a utility is parsed with the light and the dark
  values as arbitrary ones (two rules of its rank when they differ) and depends on `deps::THEME`. When the theme the
  components draw with changes (`Core::set_theme`, `set_host_theme`; a host theme keeps the app's custom keys it
  lacks, `ui::Theme::inherit`), `Core::theme_tokens_changed` drops the cached strings that name tokens and resolves
  their nodes (and the app's classes on Rust components) again; when the names change, every class. `nsh build` / `nsh dev` warn about unknown static classes in app code (`file:line:col:
  unknown class '...'`, target `nsh::build`); the core logs each unknown class once at run time. Class-only settings
  that are props in React Native (`line-clamp-*`, `truncate`, `whitespace-nowrap`, `pointer-events-*`, `object-*`,
  `placeholder:text-*`, `caret-*`) lose to explicit props. Not supported: `peer-*`, pseudo-elements, arbitrary CSS
  properties; `space-x/y-*` are gaps.
- **Text style inheritance** (CSS-like): a view's text properties (color, font, size, alignment, ...) pass to the texts and inputs
  inside it, so `class="text-white"` on a button styles its label. Inset padding / margin (set by Rust
  components: `SafeAreaView`, `KeyboardAvoidingView`) are added to the resolved padding / margin, so classes and
  styles keep their own.
- **Effects** (styles and classes; `effects.rs`): `boxShadow` (CSS syntax, several, `inset`; also `shadow-*` / `ring-*`),
  `experimental_backgroundImage` / `backgroundImage` (linear / radial / conic gradients, drawn over `backgroundColor` as in
  CSS; but press feedback must show, so a background color an interaction state sets (`active:` / `hover:` / `focus:` /
  `focus-within:` / `group-*:` classes, Rust components' `.active(..)`, `underlayColor` while pressed) or a native
  animation drives (`Animated` `backgroundColor`) replaces the gradients, unless that state sets a gradient of its own:
  done in the core, so every platform shows it alike), `filter` (`brightness`, `contrast`,
  `grayscale`, `hue-rotate`, `invert`, `saturate`, `sepia`, `opacity`, `blur`, `drop-shadow`; color filters are composed into one
  matrix), `backdropFilter` (blur), `mixBlendMode`, `outline*`, `visibility`, `cursor`, `transition*` (CSS: property groups,
  duration, delay, easing) and `animation*` (Tailwind's `spin` / `ping` / `pulse` / `bounce`), `display: grid` with
  `gridTemplateColumns/Rows`, `gridAutoFlow/Columns/Rows`, `gridColumn/Row(Start/End)`, `justifyItems/Self`; text
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
  defaults: offset `(0, -3)`, radius 3, opacity 0), `tintColor` (images), `android={{ elevation }}` (a prop, not a style: Android's material shadow, with
  `shadowColor` from API 28), `transformOrigin` (keywords / percentages / lengths; iOS conjugates the matrix around it,
  Android sets the pivot), `backfaceVisibility` (iOS `doubleSided`; Android hides the view while its rotations turn it away). `style` may be an array of styles merged left to right
  (falsy entries skipped), decoded in Rust. Text adds `textShadowColor` / `textShadowOffset` / `textShadowRadius`,
  `fontVariant` (OpenType `smcp` / `onum` / `lnum` / `tnum` / `pnum`: CoreText feature settings, `Paint.setFontFeatureSettings`)
  and `writingDirection`.
  On Apple everything is drawn by Core Animation, escalating only as needed (`crates/nsh-platform-ios/src/decor.rs`
  describes the paths and their limits); Android draws in `Decor.java`.

## Android differences

- **Android differences**: shadows need a background (no content shadows, e.g. text with a transparent background); odd corner
  radii clip by path before API 33; pull to refresh is our own (stretch overscroll stays native); `contentInset` is padding and
  `decelerationRate` only shapes snapping glides.

---
title: Tailwind reference
description: Every Tailwind utility and variant NSH resolves natively, the arbitrary value syntax, and how to extend the theme in nsh.json.
order: 17
---

# Tailwind reference

Every element takes Tailwind CSS classes in its `class` prop (and conditional ones in `classList`). NSH resolves them in Rust against Tailwind 4's default theme: JS passes only the string, each distinct string is parsed once and cached, and variants such as `dark:` or `active:` are re-evaluated natively when the appearance, window size or press state changes, without running any JS.

This page lists what is supported. For an introduction see [Tailwind](../docs/tailwind.md); for the style keys the utilities set, see the [Style reference](./style-reference.md).

```tsx
<View class="flex-row items-center gap-3 rounded-2xl bg-white p-4 shadow-md active:scale-[0.98] transition dark:bg-zinc-800">
  <Text class="text-base font-semibold text-zinc-900 dark:text-white">Inbox</Text>
  <Text classList={{ "text-red-500": unread() > 0, "text-zinc-400": unread() === 0 }}>{unread()}</Text>
</View>
```

## How classes combine

- **Later wins.** Among utilities of the same variant level, the one later in the string wins: `"p-4 p-2"` is `p-2`. A component can take a `class` prop and append it to its own classes without `tailwind-merge`.
- **Variants apply after plain utilities**, in Tailwind's variant order: position (`first:` ...), `group-*:`, `focus-within:`, `hover:`, `focus:`, `active:`, `enabled:`, `disabled:`, motion, `max-*:` breakpoints, then breakpoints ascending, orientation, direction, `dark:`, then platform. So `"active:bg-red-500 bg-blue-500"` is red while pressed.
- **`style` wins over classes.** The inline `style` is applied on top of the resolved classes. An important utility (`!p-4` or `p-4!`) wins over the inline style too.
- **Text styles inherit.** Text utilities on a view (`text-white`, `font-bold`, `text-center`, ...) pass to the `<Text>` and `<Input>` elements inside it, as in CSS.
- **Display is CSS's.** `flex` and `inline-flex` make a row unless a direction utility says otherwise, wherever it is in the string (`flex-col flex` is a column). Views without `flex` keep NSH's column default.

## Units and the theme

Lengths are points (`1` = one point on iOS, one dp on Android). Tailwind's `1rem` is 16 points, so the theme matches the web at the same density: one spacing step is 4 points (`p-4` = 16 points), `text-base` is 16 points.

### Spacing values

Utilities that take a spacing value (padding, margin, gap, inset, width, height, `basis`, `translate`, `leading`) accept:

| Value | Example | Result |
|---|---|---|
| Steps (quarter steps allowed) | `p-4`, `gap-1.5`, `m-0.25` | step × 4 points |
| `px` | `p-px`, `-mt-px` | 1 point |
| `0` | `m-0` | 0 |
| Fraction | `w-1/2`, `left-1/3` | percentage of the parent |
| `full` | `w-full`, `inset-full` | 100% |
| `auto` (margin, inset, width, height, basis) | `mx-auto`, `w-auto` | `"auto"` |
| Arbitrary | `p-[13px]`, `w-[42%]`, `mt-[1.5rem]` | the given length |

Margins, insets, `translate`, `rotate`, `skew`, `scale`, `z`, `hue-rotate`, `tracking`, `underline-offset`, `outline-offset`, grid lines and gradient angles take a leading `-` for negative values: `-mt-2`, `-translate-x-1/2`, `-rotate-6`.

### Colors

The full Tailwind 4 palette is built in: `red`, `orange`, `amber`, `yellow`, `lime`, `green`, `emerald`, `teal`, `cyan`, `sky`, `blue`, `indigo`, `violet`, `purple`, `fuchsia`, `pink`, `rose`, `slate`, `gray`, `zinc`, `neutral`, `stone`, each in shades `50`, `100`–`900`, `950`; plus `black`, `white` and `transparent`. Your own colors come from `nsh.json` (see [Theme customization](#theme-customization)).

Every color utility takes:

| Form | Example |
|---|---|
| A theme color | `bg-sky-500`, `text-brand` |
| An opacity modifier (percent) | `bg-black/50`, `ring-black/5` |
| An arbitrary opacity | `text-black/[0.35]` |
| An arbitrary color | `bg-[#0ea5e9]`, `border-[rgb(0_0_0_/_0.1)]`, `text-[oklch(70%_0.2_50)]` |
| An arbitrary color with a modifier | `bg-[rgb(0_0_255)]/25` |

`currentColor` and `inherit` are not color values; rings default to the element's text color when no ring color is given.

## Arbitrary values

Square brackets pass a value the theme does not have: `w-[37px]`, `grid-cols-[200px_1fr]`, `bg-[linear-gradient(45deg,red,blue)]`.

- `_` stands for a space (`grid-cols-[1fr_2fr]`); write `\_` for a literal underscore.
- Lengths: a bare number or `px` (points), `pt`, `rem` (16 points), and `%` where percentages make sense.
- Angles (`rotate-[...]`, `hue-rotate-[...]`, `bg-linear-[...]`): `deg`, `rad`, `grad`, `turn`.
- Times (`duration-[...]`, `delay-[...]`): `ms` or `s`; a bare number is milliseconds.
- Type hints settle ambiguous values: `text-[length:22px]` is a size, `text-[color:#333]` a color; `border-[length:3px]` likewise.

Arbitrary CSS properties (`[mask-type:luminance]`) are not supported.

## Layout

| Class | Sets |
|---|---|
| `flex`, `inline-flex` | `display: "flex"`, and a row unless a direction utility is present |
| `grid`, `inline-grid` | `display: "grid"` |
| `block`, `flow-root` | `display: "block"` (block layout) |
| `inline`, `inline-block` | `display: "flex"` (there is no inline formatting context: they lay out as views) |
| `hidden` | `display: "none"` |
| `relative`, `static`, `sticky` | `position: "relative"` (`sticky` does not stick) |
| `absolute`, `fixed` | `position: "absolute"` (`fixed` is relative to the parent, not the screen) |
| `inset-<n>`, `inset-x-<n>`, `inset-y-<n>` | `top` / `right` / `bottom` / `left` |
| `top-<n>`, `right-<n>`, `bottom-<n>`, `left-<n>` | the matching offset |
| `start-<n>`, `end-<n>` | `start` / `end` (direction-aware) |
| `z-<n>`, `z-auto`, `z-[<n>]` | `zIndex` (`z-auto` is 0) |
| `visible` | `visibility: "visible"` |
| `invisible`, `collapse` | `visibility: "hidden"` (keeps its space) |
| `overflow-hidden`, `overflow-clip`, `overflow-x-hidden`, `overflow-y-hidden` | `overflow: "hidden"` |
| `overflow-visible` | `overflow: "visible"` |
| `aspect-square`, `aspect-video`, `aspect-<a>/<b>`, `aspect-[<a>/<b>]`, `aspect-auto` | `aspectRatio` (1, 16/9, a/b, none) |

Scrolling is an element, not a class: `overflow-auto` and `overflow-scroll` are not supported; use [scroll](../elements/scroll.md).

## Flexbox and grid

| Class | Sets |
|---|---|
| `flex-row`, `flex-row-reverse`, `flex-col`, `flex-col-reverse` | `flexDirection` |
| `flex-wrap`, `flex-wrap-reverse`, `flex-nowrap` | `flexWrap` |
| `flex-1` | grow 1, shrink 1, basis 0 |
| `flex-<n>` | grow n, shrink 1, basis 0 |
| `flex-auto` | grow 1, shrink 1, basis auto |
| `flex-initial` | grow 0, shrink 1, basis auto |
| `flex-none` | grow 0, shrink 0, basis auto |
| `flex-[<grow>]`, `flex-[<grow>_<shrink>_<basis>]` | the CSS shorthand: `flex-[2]`, `flex-[3_0_50%]` |
| `grow`, `grow-<n>`, `grow-[<n>]` | `flexGrow` (bare: 1) |
| `shrink`, `shrink-<n>`, `shrink-[<n>]` | `flexShrink` (bare: 1) |
| `basis-<n>` | `flexBasis` (spacing, fraction, `full`, `auto`, arbitrary) |
| `justify-start`, `justify-end`, `justify-center`, `justify-between`, `justify-around`, `justify-evenly`, `justify-stretch`, `justify-normal` | `justifyContent` |
| `items-start`, `items-end`, `items-center`, `items-baseline`, `items-stretch` | `alignItems` |
| `self-auto`, `self-start`, `self-end`, `self-center`, `self-stretch`, `self-baseline` | `alignSelf` |
| `content-start`, `content-end`, `content-center`, `content-between`, `content-around`, `content-evenly`, `content-stretch`, `content-normal` | `alignContent` |
| `justify-items-start`, `-end`, `-center`, `-stretch`, `-normal` | `justifyItems` |
| `justify-self-auto`, `-start`, `-end`, `-center`, `-stretch` | `justifySelf` |
| `place-content-center`, `-start`, `-end`, `-between`, `-stretch` | `alignContent` and `justifyContent` |
| `place-items-center`, `-start`, `-end`, `-stretch` | `alignItems` and `justifyItems` |
| `place-self-auto`, `-center`, `-start`, `-end`, `-stretch` | `alignSelf` and `justifySelf` |
| `gap-<n>`, `gap-x-<n>`, `gap-y-<n>` | `gap`, `columnGap`, `rowGap` |
| `space-x-<n>`, `space-y-<n>` | `columnGap`, `rowGap` (a gap, not margins on the children) |
| `grid-cols-<n>` | `gridTemplateColumns: "repeat(n, minmax(0, 1fr))"` |
| `grid-cols-[<tracks>]`, `grid-cols-none` | `gridTemplateColumns` (`grid-cols-[200px_1fr]`) |
| `grid-rows-<n>`, `grid-rows-[<tracks>]`, `grid-rows-none` | `gridTemplateRows` |
| `col-span-<n>`, `col-span-full`, `col-auto`, `col-<n>`, `col-[<value>]` | `gridColumn` |
| `col-start-<n>`, `col-end-<n>` (and `-auto`) | `gridColumnStart`, `gridColumnEnd` |
| `row-span-<n>`, `row-span-full`, `row-auto`, `row-<n>`, `row-[<value>]` | `gridRow` |
| `row-start-<n>`, `row-end-<n>` (and `-auto`) | `gridRowStart`, `gridRowEnd` |
| `grid-flow-row`, `grid-flow-col`, `grid-flow-dense`, `grid-flow-row-dense`, `grid-flow-col-dense` | `gridAutoFlow` |
| `auto-cols-auto`, `-min`, `-max`, `-fr`, `auto-cols-[<track>]` | `gridAutoColumns` |
| `auto-rows-auto`, `-min`, `-max`, `-fr`, `auto-rows-[<track>]` | `gridAutoRows` |

`order-*` is accepted but has no effect: children keep their order.

## Spacing

| Class | Sets |
|---|---|
| `p-<n>`, `px-<n>`, `py-<n>` | `padding`, `paddingHorizontal`, `paddingVertical` |
| `pt-<n>`, `pr-<n>`, `pb-<n>`, `pl-<n>` | `paddingTop` / `Right` / `Bottom` / `Left` |
| `ps-<n>`, `pe-<n>` | `paddingStart`, `paddingEnd` |
| `m-<n>`, `mx-<n>`, `my-<n>` | `margin`, `marginHorizontal`, `marginVertical` |
| `mt-<n>`, `mr-<n>`, `mb-<n>`, `ml-<n>` | `marginTop` / `Right` / `Bottom` / `Left` |
| `ms-<n>`, `me-<n>` | `marginStart`, `marginEnd` |

## Sizing

| Class | Sets |
|---|---|
| `w-<n>`, `h-<n>` | `width`, `height` (spacing, fraction, `full`, `auto`, arbitrary) |
| `size-<n>` | `width` and `height` |
| `min-w-<n>`, `max-w-<n>`, `min-h-<n>`, `max-h-<n>` | the matching limit |
| `w-screen`, `w-dvw`, `w-svw`, `w-lvw` | the window width (also on `min-w-` / `max-w-`) |
| `h-screen`, `h-dvh`, `h-svh`, `h-lvh` | the window height (also on `min-h-` / `max-h-`) |
| `w-min`, `w-max`, `w-fit` (and `h-`) | `auto` (sized by content) |
| `min-w-min`, `min-w-max`, `min-w-fit` (and `min-h-`) | 0 |
| `max-w-none`, `max-h-none`, `max-w-min`, `max-w-max`, `max-w-fit` | no limit |
| `w-<container>`, `min-w-<container>`, `max-w-<container>` | a container width (below) |

Container widths (points): `3xs` 256, `2xs` 288, `xs` 320, `sm` 384, `md` 448, `lg` 512, `xl` 576, `2xl` 672, `3xl` 768, `4xl` 896, `5xl` 1024, `6xl` 1152, `7xl` 1280, `prose` 520.

## Typography

### Font

| Class | Sets |
|---|---|
| `font-sans` | the system font |
| `font-serif`, `font-mono` | `fontFamily: "serif"` / `"monospace"` |
| `font-thin` (100), `font-extralight` (200), `font-light` (300), `font-normal` (400), `font-medium` (500), `font-semibold` (600), `font-bold` (700), `font-extrabold` (800), `font-black` (900) | `fontWeight` |
| `font-[<weight>]`, `font-[<family>]` | a numeric weight, otherwise a family: `font-[650]`, `font-[Inter]` |
| `italic`, `not-italic` | `fontStyle` |
| `normal-nums`, `small-caps`, `oldstyle-nums`, `lining-nums`, `tabular-nums`, `proportional-nums` | `fontVariant` (they combine) |

### Size, color and line height

`text-<size>` sets the font size and Tailwind's matching line height:

| Class | Size | Line height |
|---|---|---|
| `text-xs` | 12 | 16 |
| `text-sm` | 14 | 20 |
| `text-base` | 16 | 24 |
| `text-lg` | 18 | 28 |
| `text-xl` | 20 | 28 |
| `text-2xl` | 24 | 32 |
| `text-3xl` | 30 | 36 |
| `text-4xl` | 36 | 40 |
| `text-5xl` | 48 | 1× |
| `text-6xl` | 60 | 1× |
| `text-7xl` | 72 | 1× |
| `text-8xl` | 96 | 1× |
| `text-9xl` | 128 | 1× |

| Class | Sets |
|---|---|
| `text-<size>/<leading>` | size and line height: `text-sm/6` (24 points), `text-lg/tight` |
| `text-[<length>]` | `fontSize`: `text-[22px]`, `text-[length:22px]` |
| `text-<color>` | `color` |
| `leading-none` (1), `leading-tight` (1.25), `leading-snug` (1.375), `leading-normal` (1.5), `leading-relaxed` (1.625), `leading-loose` (2) | `lineHeight` as a multiple of the font size |
| `leading-<n>`, `leading-[<length>]` | `lineHeight` in points (`leading-6` = 24) |
| `tracking-tighter` (-0.05em), `tracking-tight` (-0.025em), `tracking-normal` (0), `tracking-wide` (0.025em), `tracking-wider` (0.05em), `tracking-widest` (0.1em) | `letterSpacing` relative to the font size |
| `tracking-[<value>]` | `letterSpacing`: `tracking-[0.2em]`, `tracking-[1px]` |

### Alignment, case and wrapping

| Class | Sets |
|---|---|
| `text-left`, `text-center`, `text-right`, `text-justify` | `textAlign` |
| `text-start` | `textAlign: "auto"` |
| `text-end` | `textAlign: "right"` |
| `uppercase`, `lowercase`, `capitalize`, `normal-case` | `textTransform` |
| `truncate` | one line, tail ellipsis, no wrapping, `overflow: hidden` |
| `line-clamp-<n>`, `line-clamp-none` | `numberOfLines` (`none` removes the limit) |
| `text-ellipsis`, `text-clip` | `ellipsizeMode` `"tail"` / `"clip"` |
| `whitespace-nowrap`, `text-nowrap` | lines never wrap |
| `whitespace-normal`, `text-wrap`, `text-balance`, `text-pretty` | lines wrap (balancing is not done) |
| `select-text`, `select-all`, `select-none` | `selectable` |

### Decoration and shadow

| Class | Sets |
|---|---|
| `underline`, `line-through`, `no-underline` | `textDecorationLine` |
| `decoration-solid`, `decoration-double`, `decoration-dotted`, `decoration-dashed`, `decoration-wavy` | `textDecorationStyle` |
| `decoration-<n>`, `decoration-[<length>]` | `textDecorationThickness` |
| `decoration-auto`, `decoration-from-font` | `textDecorationThickness: "auto"` |
| `decoration-<color>` | `textDecorationColor` |
| `underline-offset-<n>`, `underline-offset-[<length>]`, `underline-offset-auto` | `textUnderlineOffset` |
| `text-shadow-2xs`, `-xs`, `-sm`, `-md`, `-lg` | a text shadow from the theme |
| `text-shadow-<color>` | its color (keeps each layer's alpha unless the color has one) |
| `text-shadow-none` | removes it |

Tailwind's text shadows have several layers; natively one shadow is drawn, the widest layer.

### Inputs

| Class | Sets |
|---|---|
| `placeholder:text-<color>` | `placeholderTextColor` |
| `caret-<color>` | the cursor color |

## Backgrounds

| Class | Sets |
|---|---|
| `bg-<color>` | `backgroundColor` |
| `bg-[<gradient>]` | `backgroundImage`: `bg-[linear-gradient(45deg,red,blue)]`, `bg-[radial-gradient(...)]` |
| `bg-linear-to-t`, `-tr`, `-r`, `-br`, `-b`, `-bl`, `-l`, `-tl` | a linear gradient toward a side or corner (`bg-gradient-to-*` is the v3 name) |
| `bg-linear-<angle>`, `bg-linear-[<angle>]` | a linear gradient at an angle: `bg-linear-45`, `-bg-linear-12`, `bg-linear-[0.25turn]` |
| `bg-radial` | a radial gradient from the center |
| `bg-conic`, `bg-conic-<angle>` | a conic gradient, optionally starting at an angle |
| `from-<color>`, `via-<color>`, `to-<color>` | the gradient's stops (default transparent at the ends) |
| `from-<p>%`, `via-<p>%`, `to-<p>%` | stop positions: `via-40%`, `from-[15%]` |
| `bg-none` | removes the gradient |

```tsx
<View class="h-32 rounded-2xl bg-linear-to-br from-indigo-500 via-purple-500 via-40% to-pink-500" />
```

## Borders

| Class | Sets |
|---|---|
| `border`, `border-<n>`, `border-[<length>]` | `borderWidth` (bare: 1) |
| `border-x`, `border-y`, `border-t`, `border-r`, `border-b`, `border-l`, `border-s`, `border-e` (with `-<n>` or bare) | the matching sides' widths |
| `border-<color>`, `border-t-<color>`, ... | `borderColor` and the per-side colors |
| `border-solid`, `border-dashed`, `border-dotted` | `borderStyle` (`border-double` draws solid) |
| `border-none`, `border-hidden` | width 0 |
| `rounded`, `rounded-<size>`, `rounded-none`, `rounded-full`, `rounded-[<length>]` | `borderRadius` (bare: `sm`) |
| `rounded-t-`, `-r-`, `-b-`, `-l-`, `-tl-`, `-tr-`, `-br-`, `-bl-` | the matching corners |
| `rounded-s-`, `-e-`, `-ss-`, `-se-`, `-es-`, `-ee-` | start / end corners (direction-aware) |
| `outline`, `outline-<n>`, `outline-[<length>]` | `outlineWidth` and a solid `outlineStyle` (bare: 1) |
| `outline-<color>` | `outlineColor` |
| `outline-solid`, `outline-dashed`, `outline-dotted`, `outline-none`, `outline-hidden` | `outlineStyle` |
| `outline-offset-<n>`, `outline-offset-[<length>]` | `outlineOffset` |

Radius sizes (points): `xs` 2, `sm` 4, `md` 6, `lg` 8, `xl` 12, `2xl` 16, `3xl` 24, `4xl` 32, `full` 9999.

### Rings

Rings are box shadows with a spread, drawn outside the border without taking space.

| Class | Sets |
|---|---|
| `ring`, `ring-<n>`, `ring-[<length>]` | a ring of that width (bare: 1) |
| `ring-<color>` | its color (default the element's text color) |
| `ring-inset` | draws the ring inside |
| `ring-offset-<n>`, `ring-offset-<color>` | a gap between element and ring (default white) |
| `inset-ring`, `inset-ring-<n>`, `inset-ring-<color>` | an inner ring |

## Effects

| Class | Sets |
|---|---|
| `shadow-2xs`, `shadow-xs`, `shadow-sm`, `shadow`, `shadow-md`, `shadow-lg`, `shadow-xl`, `shadow-2xl` | `boxShadow` from the theme |
| `shadow-[<box-shadow>]` | an arbitrary shadow: `shadow-[0_8px_24px_rgb(0_0_0_/_0.12)]` |
| `shadow-<color>` | recolors the shadow layers: `shadow-indigo-500/40` |
| `shadow-none` | no shadow |
| `inset-shadow-2xs`, `-xs`, `-sm`, `inset-shadow`, `inset-shadow-[...]`, `inset-shadow-<color>`, `inset-shadow-none` | an inner shadow |
| `opacity-<n>`, `opacity-[<value>]` | `opacity`: `opacity-50` = 0.5 |
| `mix-blend-<mode>` | `mixBlendMode`: `normal`, `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`, `plus-darker`, `plus-lighter` |

Shadows, rings and inset shadows combine into one `boxShadow` list, as in CSS. A shadow color without its own alpha keeps each layer's alpha.

## Filters

| Class | Sets |
|---|---|
| `blur`, `blur-xs` (4), `-sm` (8), `-md` (12), `-lg` (16), `-xl` (24), `-2xl` (40), `-3xl` (64), `blur-none`, `blur-[<length>]` | `blur()` |
| `brightness-<n>`, `contrast-<n>`, `saturate-<n>` | percentage factors: `brightness-125` = 1.25 |
| `grayscale`, `grayscale-<n>`, `invert`, `invert-<n>`, `sepia`, `sepia-<n>` | bare: 100% |
| `hue-rotate-<deg>`, `-hue-rotate-<deg>` | `hue-rotate()` |
| `drop-shadow`, `drop-shadow-xs`, `-sm`, `-md`, `-lg`, `-xl`, `-2xl`, `drop-shadow-[...]` | `drop-shadow()` (one layer natively: the widest) |
| `filter-none` | removes all filters |
| `backdrop-blur`, `backdrop-blur-<size>`, `backdrop-blur-[<length>]` | blurs what is behind the view |
| `backdrop-brightness-<n>`, `backdrop-contrast-<n>`, `backdrop-grayscale`, `backdrop-hue-rotate-<deg>`, `backdrop-invert`, `backdrop-opacity-<n>`, `backdrop-saturate-<n>`, `backdrop-sepia` | backdrop filters |
| `backdrop-filter-none` | removes them |

> [!WARNING]
> iOS applies only `opacity`, `brightness` up to 1 and `drop-shadow` to views; `<Image>` takes every color filter and `blur`. Android needs API 31 for `blur`, `drop-shadow` and backdrop blur. See the [Style reference](./style-reference.md#filters-and-blending).

## Transforms

| Class | Sets |
|---|---|
| `translate-x-<n>`, `translate-y-<n>`, `translate-<n>` | translation (spacing, `full`, fractions of the element's own size: `-translate-x-1/2`, arbitrary) |
| `rotate-<deg>`, `rotate-z-<deg>`, `rotate-[<angle>]` | rotation |
| `rotate-x-<deg>`, `rotate-y-<deg>` | 3D rotation |
| `scale-<pct>`, `scale-x-<pct>`, `scale-y-<pct>`, `scale-[<factor>]` | scale: `scale-95` = 0.95, `-scale-x-100` mirrors |
| `skew-<deg>`, `skew-x-<deg>`, `skew-y-<deg>` | skew (not on Android) |
| `perspective-dramatic` (100), `-near` (300), `-normal` (500), `-midrange` (800), `-distant` (1200), `perspective-[<length>]` | perspective for 3D rotations |
| `origin-center`, `-top`, `-top-right`, `-right`, `-bottom-right`, `-bottom`, `-bottom-left`, `-left`, `-top-left`, `origin-[<value>]` | `transformOrigin` |
| `backface-hidden`, `backface-visible` | `backfaceVisibility` |
| `transform-none` | clears the transform |

Transform utilities compose like CSS: `translate-*`, `rotate-*` and `scale-*` combine regardless of order. `transform`, `transform-gpu`, `transform-cpu`, `transform-3d` and `transform-flat` are accepted and do nothing. Translations by a fraction of the size are resolved against the element's frame and follow it when it resizes.

## Transitions and animation

| Class | Sets |
|---|---|
| `transition` | colors, opacity, shadow and transform; 150 ms, `cubic-bezier(0.4, 0, 0.2, 1)` |
| `transition-all`, `transition-colors`, `transition-opacity`, `transition-shadow`, `transition-transform` | the animated properties |
| `transition-none` | no transition |
| `duration-<ms>`, `duration-[<time>]` | `transitionDuration`: `duration-300`, `duration-[0.4s]` |
| `delay-<ms>`, `delay-[<time>]` | `transitionDelay` |
| `ease-linear`, `ease-in`, `ease-out`, `ease-in-out`, `ease-initial`, `ease-[cubic-bezier(...)]` | `transitionTimingFunction` |
| `animate-spin`, `animate-ping`, `animate-pulse`, `animate-bounce` | a native keyframe animation, repeated forever |
| `animate-none` | stops it |

Transitions run on the UI thread when a value changes, including when a variant starts or stops matching, so `transition active:scale-95` animates the press without JS. Wrap animations in `motion-safe:` to respect the reduce-motion setting.

## Interactivity

| Class | Sets |
|---|---|
| `pointer-events-none`, `pointer-events-auto` | `pointerEvents` |
| `cursor-<name>`, `cursor-[<name>]` | `cursor`: `cursor-pointer`, `cursor-grab`, `cursor-not-allowed`, ... (every CSS cursor keyword) |
| `select-none`, `select-text`, `select-all`, `select-auto` | text selection (see Typography) |

## Images

| Class | Sets |
|---|---|
| `object-cover`, `object-contain`, `object-fill`, `object-none`, `object-scale-down` | `resizeMode` (`fill` stretches, `none` and `scale-down` center) |
| `tint-<color>` | `tintColor` (recolors template images) |

## Accepted without effect

These web utilities parse without a warning and do nothing natively: `antialiased`, `subpixel-antialiased`, `isolate`, `isolation-auto`, `box-border`, `box-content`, `order-*`, `select-auto`, `transform`, `transform-gpu`, `transform-cpu`, `transform-3d`, `transform-flat`, and every `will-change-*`, `touch-*`, `appearance-*`, `scroll-*`, `snap-*` and `overscroll-*` class. Scrolling, snapping and overscroll are props of [scroll](../elements/scroll.md).

## Not supported

`peer-*` variants, pseudo-elements (`before:`, `after:`), arbitrary properties (`[prop:value]`), `overflow-auto` / `overflow-scroll`, `divide-*`, `columns-*`, `float-*`, `container`, and container queries. `nsh build` and `nsh dev` warn about unknown classes they find in your source (`file:line:col: unknown class '...'`), and the runtime logs each unknown class once.

## Variants

Variants make a utility conditional: `dark:bg-zinc-900`, `md:flex-row`, `active:opacity-70`. They stack (`dark:active:bg-zinc-700`) and are evaluated natively whenever what they depend on changes.

### Appearance and environment

| Variant | Applies when |
|---|---|
| `dark:` | the appearance is dark |
| `ios:` | running on iOS or Mac Catalyst |
| `android:` | running on Android |
| `portrait:`, `landscape:` | the window is taller than wide / wider than tall |
| `ltr:`, `rtl:` | the layout direction |
| `motion-safe:`, `motion-reduce:` | the user has not / has asked for reduced motion |

### Breakpoints

Breakpoints compare the window width in points.

| Variant | Min width |
|---|---|
| `sm:` | 640 |
| `md:` | 768 |
| `lg:` | 1024 |
| `xl:` | 1280 |
| `2xl:` | 1536 |

| Form | Example | Applies when |
|---|---|---|
| `<bp>:` | `md:p-8` | width ≥ breakpoint |
| `max-<bp>:` | `max-md:p-1` | width < breakpoint |
| `min-[<length>]:` | `min-[600px]:grid-cols-3` | width ≥ the length |
| `max-[<length>]:` | `max-[400px]:text-sm` | width < the length |

A phone in portrait (390 points wide) matches none of the default breakpoints; a tablet or a phone in landscape matches several.

### Interaction states

States are tracked by the platform only for elements whose classes use them, and changes apply natively, with no JS round trip.

| Variant | Applies when |
|---|---|
| `active:` | the element is pressed |
| `hover:` | a pointer is over it (iPad pointer, Mac Catalyst, Android mouse) |
| `focus:`, `focus-visible:` | the input has focus |
| `focus-within:` | the element or a descendant has focus |
| `disabled:` | the element has the `disabled` prop |
| `enabled:` | the element does not have the `disabled` prop |

`disabled` works on any element: it applies `disabled:` and removes the element's press handlers, so it shows no press feedback either.

```tsx
<View
  onPress={save}
  disabled={saving()}
  class="rounded-lg bg-blue-600 px-4 py-2 transition active:scale-95 active:bg-blue-700 disabled:opacity-40"
>
  <Text class="font-medium text-white">Save</Text>
</View>
```

### Groups

Mark an ancestor with `group` and style descendants by its state with `group-<state>:`. Name groups to nest them: `group/card` and `group-hover/card:`.

| Variant | Applies when the group is |
|---|---|
| `group-active:` | pressed |
| `group-hover:` | hovered |
| `group-focus:`, `group-focus-visible:` | focused |
| `group-focus-within:` | focused or containing focus |
| `group-disabled:` | disabled |

```tsx
<View onPress={open} class="group/row flex-row items-center gap-3 p-3 active:bg-zinc-100">
  <Text class="flex-1 text-zinc-900 group-active/row:text-blue-600">Settings</Text>
  <Text class="text-zinc-400 group-active/row:translate-x-1 transition-transform">›</Text>
</View>
```

### Position among siblings

| Variant | Applies to |
|---|---|
| `first:` | the first child |
| `last:` | the last child |
| `only:` | the only child |
| `odd:` | the 1st, 3rd, ... child |
| `even:` | the 2nd, 4th, ... child |

```tsx
<Each of={items()}>{(item) => <Text class="border-b border-zinc-200 py-3 last:border-b-0">{item().name}</Text>}</Each>
```

### Placeholder

`placeholder:` applies to an `<Input>`'s placeholder and only takes a text color: `placeholder:text-zinc-400`.

### Important

`!` before or after a utility (`!p-4`, `p-4!`) makes it win over every other class and over the inline `style`.

## Theme customization

Extend the theme in the `"tailwind"` object of your app's `nsh.json`. Entries with a new name are added; entries with an existing name replace the default. Lengths are points. The theme is checked when the app is bundled: an unknown key or an invalid value is a build error.

```json
{
  "name": "My App",
  "id": "com.example.myapp",
  "tailwind": {
    "colors": {
      "brand": { "50": "#f5f3ff", "500": "#7c3aed", "600": "#6d28d9", "DEFAULT": "#7c3aed" },
      "accent": "#f59e0b"
    },
    "spacing": 4,
    "breakpoints": { "tablet": 600 },
    "fontFamily": { "display": "Inter" },
    "fontSize": { "huge": [48, 1.1] },
    "fontWeight": { "heavy": 850 },
    "radius": { "card": 14 },
    "shadow": { "card": "0 2px 8px rgb(0 0 0 / 0.12)" },
    "insetShadow": { "well": "inset 0 2px 6px rgb(0 0 0 / 0.1)" },
    "dropShadow": { "icon": "0 2px 4px rgb(0 0 0 / 0.2)" },
    "tracking": { "label": 0.08 },
    "leading": { "dense": 1.15 },
    "blur": { "glass": 20 },
    "perspective": { "card": 700 },
    "aspect": { "photo": 1.5 },
    "ease": { "snappy": [0.2, 0, 0, 1] },
    "container": { "content": 680 }
  }
}
```

With this theme, `bg-brand-500`, `bg-brand` (the `DEFAULT` shade), `text-accent`, `tablet:flex-row`, `font-display`, `text-huge`, `font-heavy`, `rounded-card`, `shadow-card`, `inset-shadow-well`, `drop-shadow-icon`, `tracking-label`, `leading-dense`, `blur-glass`, `perspective-card`, `aspect-photo`, `ease-snappy` and `max-w-content` all work.

| Key | Value | Used by |
|---|---|---|
| `colors` | `{ name: color }` or `{ name: { shade: color } }`; `DEFAULT` names the bare color. Any CSS color syntax. | every color utility |
| `spacing` | points per step (default 4) | `p-*`, `m-*`, `gap-*`, `w-*`, ... |
| `rem` | points per `rem` (default 16) | |
| `breakpoints` | `{ name: width }` | breakpoint variants (sorted by width) |
| `container` | `{ name: width }` | `w-*`, `min-w-*`, `max-w-*` |
| `fontFamily` | `{ name: family }` (a system family or the name of a bundled font) | `font-*` |
| `fontSize` | `{ name: size }` or `{ name: [size, lineHeightMultiple] }` | `text-*` |
| `fontWeight` | `{ name: 1–1000 }` | `font-*` |
| `tracking` | `{ name: em }` | `tracking-*` |
| `leading` | `{ name: multiple }` | `leading-*` |
| `radius` | `{ name: points }` | `rounded-*` |
| `shadow`, `insetShadow`, `dropShadow` | `{ name: "CSS box-shadow" }` | `shadow-*`, `inset-shadow-*`, `drop-shadow-*` |
| `blur` | `{ name: points }` | `blur-*`, `backdrop-blur-*` |
| `perspective` | `{ name: points }` | `perspective-*` |
| `aspect` | `{ name: ratio }` | `aspect-*` |
| `ease` | `{ name: [x1, y1, x2, y2] }` | `ease-*` |

> [!NOTE]
> The Tailwind theme shapes only your app's classes. The components of `nsh/ui/mobile` take their colors from the component theme, the `"theme"` key of `nsh.json`; see [Components](../components/index.md).

## Related

- [Tailwind](../docs/tailwind.md): the guide.
- [Styling](../docs/styling.md): `class`, `classList`, `style` and how they combine.
- [Style reference](./style-reference.md): the style keys utilities set.

---
title: Tailwind
description: Style elements with Tailwind CSS classes resolved natively - class and classList, variants for pressed states, dark mode and breakpoints, the theme in nsh.json, and the build-time class check.
order: 33
group: ui
demo: guide-tailwind
---

# Tailwind

Every element accepts Tailwind CSS classes in `class`, resolved natively against Tailwind 4's default theme:

```tsx
<View class="flex-row items-center gap-3 rounded-2xl bg-white p-4 shadow-lg active:scale-[0.98] dark:bg-zinc-800">
  <Text class="text-base font-semibold text-zinc-900 dark:text-white">Tailwind on native views</Text>
</View>
```

No CSS is generated and nothing runs in JavaScript: the string is passed to the core, which parses each distinct class string once (and caches it), turns it into style rules, and applies them. Variants that depend on state (pressed, dark mode, window width) are re-applied by the core when that state changes, and pressed and focused states are tracked natively, so `active:` feedback has no JavaScript round trip.

Classes and the [`style` prop](./styling.md) produce the same native styles and can be combined; this page covers what is specific to classes. The [Tailwind reference](../modules/tailwind-reference.md) lists every supported utility.

## class and classList

```tsx
<View
  class="rounded-lg px-4 py-2 transition-colors"
  classList={{ "bg-indigo-600": selected(), "bg-zinc-200": !selected(), "opacity-50": busy() }}
/>
```

- **`class`** is a string of classes. It can be dynamic: ``class={`h-6 rounded bg-${color()}-500`}`` works, because classes are resolved at run time.
- **`classList`** maps classes to conditions: a class applies while its value is truthy. It is applied after `class`.

How conflicts resolve:

1. **Later classes win** over earlier ones in the same string: `"p-2 p-4"` is `p-4`. There is no need for `tailwind-merge`: a component can append the `class` its caller passes.
2. **Variant classes apply after plain ones**, ordered by Tailwind's variant order (`hover:` before `active:`, breakpoints ascending, `dark:` last), whatever their position in the string. As in CSS, `odd:bg-a last:bg-b` gives an odd last row `bg-a`, because Tailwind sorts `odd` after `last`.
3. **`style` wins over classes.**
4. `!` makes a class important: `!p-0`.

Components from `nsh/ui/mobile` accept `class` and `classList` too; your classes are merged after the component's own styles and win at equal rank.

## Variants

| Variant | Applies when |
|---|---|
| `active:` | The view is pressed |
| `hover:` | A pointer is over the view (iPad and Mac pointers) |
| `focus:`, `focus-visible:` | An input has focus |
| `focus-within:` | The view or a descendant has focus |
| `disabled:`, `enabled:` | The element's `disabled` prop is true / false |
| `group-active:`, `group-hover:`, `group-focus:`, `group-focus-within:`, `group-disabled:` | The nearest ancestor with the `group` class is in that state |
| `group-active/name:` ... | The same, for an ancestor marked `group/name` |
| `first:`, `last:`, `odd:`, `even:`, `only:` | The element's own position among its siblings. To color a row's label by the row's position, put `last:text-*` on the row: the label inherits it |
| `placeholder:` | Styles an input's placeholder (`placeholder:text-zinc-400`) |
| `dark:` | The system appearance is dark |
| `sm:`, `md:`, `lg:`, `xl:`, `2xl:` | The window is at least 640, 768, 1024, 1280, 1536 points wide |
| `max-sm:`, `max-md:`, ... | The window is narrower than the breakpoint |
| `min-[600px]:`, `max-[600px]:` | Arbitrary breakpoints |
| `portrait:`, `landscape:` | The window's orientation |
| `ios:`, `android:` | The platform |
| `rtl:`, `ltr:` | The [layout direction](./layout.md#right-to-left) |
| `motion-safe:`, `motion-reduce:` | The user's reduce motion setting |

Variants combine: `dark:active:bg-zinc-700`, `md:landscape:flex-row`.

```tsx
<View class="group flex-row items-center gap-3 rounded-xl bg-white p-3 active:bg-zinc-50">
  <Text class="flex-1 text-zinc-900">Settings</Text>
  <Text class="text-zinc-300 transition-transform group-active:translate-x-1">›</Text>
</View>
```

`disabled` on any element applies `disabled:` classes and removes its press handlers from what is mounted, so it shows no press feedback either:

```tsx
<View disabled={saving()} onPress={save} class="rounded-lg bg-emerald-500 px-4 py-2 active:bg-emerald-600 disabled:opacity-40">
  <Text class="font-medium text-white">Save</Text>
</View>
```

Breakpoints compare against the window (the surface NSH renders into), not the device: in an iPad split view or a resizable window, they follow the window's width.

## What differs from Tailwind on the web

- **Units are points.** `p-4` is 16 points (1 spacing step = 4pt, 1rem = 16pt). Arbitrary values in `px` are points too: `p-[13px]` is 13pt.
- **`flex` is a row**, as in CSS: `class="flex"` lays out its children in a row unless a direction class says otherwise (anywhere in the string: `flex-col flex` is a column). A view without `flex` or `inline-flex` keeps NSH's default column. `block` and `flow-root` use block layout; `inline` and `inline-block` lay out as views (there is no inline formatting context).
- **Text classes on views are inherited.** `text-*`, `font-*`, `leading-*`, `tracking-*` and the other text utilities on a view pass to the `<Text>` and `<Input>` elements inside it, as CSS inheritance does: `<View class="text-white">` makes its labels white.
- **`space-x-*` / `space-y-*` are gaps** (`gap-x` / `gap-y`).
- **State backgrounds show over gradients.** In CSS a gradient covers `background-color`, so `active:bg-*` on a `bg-linear-*` surface would never be seen; in NSH a background color from `active:`, `hover:`, `focus:` or `group-*:` replaces the gradient while that state holds (unless the state sets a gradient of its own, such as `active:from-*`). So do `underlayColor` and animated background colors.
- **Classes for settings that are props in NSH**: `line-clamp-*`, `truncate`, `whitespace-nowrap`, `pointer-events-*`, `object-*` (image resize mode), `select-text`, `caret-*` and `placeholder:text-*` work, and lose to the explicit props (`numberOfLines`, `pointerEvents`, `resizeMode`, ...) when both are set.
- **Not supported**: `peer-*` variants, pseudo-elements (`before:`, `after:`), arbitrary properties (`[mask-type:luminance]`). Web-only utilities that need nothing natively (`antialiased`, `will-change-*`, `touch-*`, `scroll-*`, `snap-*`, `overscroll-*`, `box-border`, ...) are accepted and ignored.

Everything else works as documented by Tailwind 4: the color palette, opacity modifiers (`bg-black/40`), arbitrary values (`bg-[#34c759]`, `w-[72px]`, `grid-cols-[1fr_2fr]`), fractions (`w-1/2`, `translate-x-1/2`), shadows and rings, gradients (`bg-linear-to-r`, `bg-radial`, `bg-conic` with `from-` / `via-` / `to-`), filters, `backdrop-blur-*`, `mix-blend-*`, transforms, `transition-*` and `duration-*`, `animate-spin` / `ping` / `pulse` / `bounce`, and grid. The platform limits of [filters and blending](./styling.md#filters-and-blending) apply to classes too.

## Theme

Classes resolve against Tailwind 4's default theme. Extend it in `nsh.json` under `"tailwind"`; your entries are added to the defaults (or replace the default of the same name):

```json
{
  "tailwind": {
    "colors": {
      "brand": { "50": "#f5f3ff", "500": "#7c3aed", "600": "#6d28d9", "DEFAULT": "#7c3aed" },
      "ink": "#1f2328"
    },
    "radius": { "card": 14 },
    "fontFamily": { "display": "Pacifico" },
    "breakpoints": { "tablet": 600 }
  }
}
```

With this theme, `bg-brand-500`, `text-brand` (the `DEFAULT` shade), `border-ink`, `rounded-card`, `font-display` and `tablet:flex-row` are valid classes.

| Key | Values | Makes |
|---|---|---|
| `colors` | A CSS color, or an object of shades (`"DEFAULT"` for the bare name) | `bg-*`, `text-*`, `border-*`, `from-*`, `ring-*`, ... |
| `spacing` | Points per spacing step (default 4) | `p-*`, `m-*`, `gap-*`, `w-*`, ... |
| `rem` | Points per `rem` in arbitrary values (default 16) | |
| `breakpoints` | Points | `name:` variants |
| `container` | Points | `max-w-*` sizes |
| `fontSize` | Points, or `[size, lineHeight]` with the line height a multiple of the size | `text-*` |
| `fontWeight` | 1–1000 | `font-*` |
| `fontFamily` | A family name (a system family or an [imported font's](./fonts.md) name) | `font-*` |
| `tracking` | Ems | `tracking-*` |
| `leading` | Multiples of the font size | `leading-*` |
| `radius` | Points | `rounded-*` |
| `shadow`, `insetShadow`, `dropShadow` | A CSS `box-shadow` | `shadow-*`, `inset-shadow-*`, `drop-shadow-*` |
| `blur` | Points | `blur-*`, `backdrop-blur-*` |
| `perspective` | Points | `perspective-*` |
| `aspect` | A ratio (`1.5`) | `aspect-*` |
| `ease` | `[x1, y1, x2, y2]` | `ease-*` |

Unknown keys and invalid values are build errors (`nsh.json: tailwind: unknown theme key 'colours'`). The theme is compiled into the bundle and applied before the first frame.

The theme only shapes your own classes. The components of `nsh/ui/mobile` take their colors from the component theme, `"theme"` in `nsh.json` ([Components](../components/index.md)).

## The class check

`nsh build`, `nsh dev`, `nsh run` and `nsh trace` check the static class names in your JSX (string literals in `class` and the keys of `classList`) with the same parser and theme the runtime uses, and warn about classes that do not exist:

```text
 warn nsh::build App.tsx:12:36: unknown class `bg-brand-550`
```

Warnings do not fail the build. Classes built at run time (``bg-${color()}-500``) cannot be checked when building; the runtime logs each unknown class once when it first sees it (`unknown classes: ...`).

## Classes or styles?

Both are resolved natively and cost about the same. Classes are shorter and give you variants (pressed states, dark mode, breakpoints) without code. Inline styles take values computed in JavaScript and accept [native animated values](./animation.md) (`opacity: fade`, `transform: [{ translateX: x }]`). Most apps use classes for the static look and `style` for what is computed or animated.

## Related

- [Styling](./styling.md): the style prop and the platform limits of effects.
- [Tailwind reference](../modules/tailwind-reference.md): every utility and variant.
- [Layout](./layout.md): flexbox and grid, also available as classes.

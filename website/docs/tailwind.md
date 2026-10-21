---
title: Tailwind
description: Tailwind CSS classes resolved natively in TSX and their typed equivalents in Rust - class and classList, variants for pressed states, dark mode and breakpoints, the theme in nsh.json, and the build-time class check.
order: 33
group: ui
demo: guide-tailwind
---

# Tailwind

NSH's styles follow Tailwind CSS 4. In Rust, Tailwind's utilities are typed style methods with the same names and values (`.p_4()` is `p-4`, `.rounded_2xl()` is `rounded-2xl`, `ZINC.shade(900)` is `zinc-900`); in TSX, every element accepts the classes themselves in `class`, resolved natively against Tailwind 4's default theme. Each example below shows the typed methods first and the classes second:

```rust tabs
use nsh_core::ui::palette::ZINC;

view()
    .flex_row()
    .items_center()
    .gap_3()
    .rounded_2xl()
    .bg(AdaptiveColor::new(white(), ZINC.shade(800)))
    .p_4()
    .shadow_lg()
    .active(|s| s.scale(0.98))
    .child(
        text("Tailwind on native views")
            .text_base()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(AdaptiveColor::new(ZINC.shade(900), white())),
    )
```

```tsx tabs
<View class="flex-row items-center gap-3 rounded-2xl bg-white p-4 shadow-lg active:scale-[0.98] dark:bg-zinc-800">
  <Text class="text-base font-semibold text-zinc-900 dark:text-white">Tailwind on native views</Text>
</View>
```

Typed methods lower into the same style rules the classes compile to, with no string to parse. For classes, no CSS is generated and nothing runs in JavaScript: the string is passed to the core, which parses each distinct class string once (and caches it), turns it into style rules, and applies them. Variants that depend on state (pressed, dark mode, window width) are re-applied by the core when that state changes, and pressed and focused states are tracked natively, so `active:` feedback (`.active(..)` in Rust) has no round trip to your code.

Classes and the [`style` prop](./styling.md) produce the same native styles and can be combined; this page covers what is specific to classes and their typed equivalents. The [Tailwind reference](../modules/tailwind-reference.md) lists every supported utility.

## class and classList

```rust tabs
view()
    .rounded_lg()
    .px_4()
    .py_2()
    .transition_colors()
    .bg(move || if selected.get() { INDIGO.shade(600) } else { ZINC.shade(200) })
    .style_when(busy, |s| s.opacity(0.5))
```

```tsx tabs
<View
  class="rounded-lg px-4 py-2 transition-colors"
  classList={{ "bg-indigo-600": selected(), "bg-zinc-200": !selected(), "opacity-50": busy() }}
/>
```

- **`class`** is a string of classes. It can be dynamic: ``class={`h-6 rounded bg-${color()}-500`}`` works, because classes are resolved at run time.
- **`classList`** maps classes to conditions: a class applies while its value is truthy. It is applied after `class`.
- **In Rust**, a method given a closure or a signal follows it (`.bg(move || ..)`), `.style_when(condition, |s| ..)` applies styles while a condition holds, and `.when(condition, |v| ..)` decides once, when the element is made.

How conflicts resolve:

1. **Later classes win** over earlier ones in the same string: `"p-2 p-4"` is `p-4`. There is no need for `tailwind-merge`: a component can append the `class` its caller passes. Later methods win in Rust too.
2. **Variant classes apply after plain ones**, ordered by Tailwind's variant order (`hover:` before `active:`, breakpoints ascending, `dark:` last), whatever their position in the string. As in CSS, `odd:bg-a last:bg-b` gives an odd last row `bg-a`, because Tailwind sorts `odd` after `last`.
3. **`style` wins over classes.**
4. `!` makes a class important: `!p-0`.

Rust components (`SafeAreaView`, `KeyboardAvoidingView` and those of an app's own crate) accept `class` and `classList` from TSX too; your classes are merged after the component's own styles and win at equal rank.

## Variants

| Variant | Applies when | Rust |
|---|---|---|
| `active:` | The view is pressed | `.active(\|s\| ..)` |
| `hover:` | A pointer is over the view (iPad and Mac pointers) | `.hover(\|s\| ..)` |
| `focus:`, `focus-visible:` | An input has focus | `.focus(\|s\| ..)` |
| `focus-within:` | The view or a descendant has focus | |
| `disabled:`, `enabled:` | The element's `disabled` prop is true / false | `.disabled_style(\|s\| ..)` |
| `selected:` | The element is selected (a `Button`'s `selected`, `accessibilityState.selected`) | `.selected_style(..)` |
| `checked:` | The element is checked (`accessibilityState.checked` is `true`; `"mixed"` is not checked) | `.checked_style(..)` |
| `group-active:`, `group-hover:`, `group-focus:`, `group-focus-within:`, `group-disabled:`, `group-selected:`, `group-checked:` | The nearest ancestor with the `group` class is in that state | `.group()` on the ancestor; `.group_active(..)`, `.group_hover(..)`, `.group_focus_within(..)` inside |
| `group-active/name:` ... | The same, for an ancestor marked `group/name` | `.group_named("name")`; `.group_active_named("name", ..)`, ... |
| `first:`, `last:`, `odd:`, `even:`, `only:` | The element's own position among its siblings. To color a row's label by the row's position, put `last:text-*` on the row: the label inherits it | Plain Rust from the row's index (`at.is_first()`, `at.is_last()`, `at.index() % 2`) |
| `placeholder:` | Styles an input's placeholder (`placeholder:text-zinc-400`) | `.placeholder_color(..)` |
| `selection:` | Styles the selection of an input or a selectable text (`selection:text-violet-500`) | `.selection_color(..)` |
| `dark:` | The system appearance is dark | An `AdaptiveColor`, or `.dark(\|s\| ..)` |
| `sm:`, `md:`, `lg:`, `xl:`, `2xl:` | The window is at least 640, 768, 1024, 1280, 1536 points wide | `.style_when(\|\| window().width >= px(768.0), \|s\| ..)` |
| `max-sm:`, `max-md:`, ... | The window is narrower than the breakpoint | The same, with `<` |
| `min-[600px]:`, `max-[600px]:` | Arbitrary breakpoints | The same |
| `portrait:`, `landscape:` | The window's orientation | `window().width > window().height` |
| `ios:`, `android:`, `web:` | The platform | `.when(os() == Os::Ios, \|v\| ..)` |
| `rtl:`, `ltr:` | The [layout direction](./layout.md#right-to-left) | `rtl()` |
| `motion-safe:`, `motion-reduce:` | The user's reduce motion setting | `reduce_motion()` |

Variants combine: `dark:active:bg-zinc-700`, `md:landscape:flex-row`. In Rust, an `AdaptiveColor` in an `.active(..)` style is `dark:active:`.

```rust tabs
view()
    .group()
    .flex_row()
    .items_center()
    .gap_3()
    .rounded_xl()
    .bg(white())
    .p_3()
    .active(|s| s.bg(ZINC.shade(50)))
    .child(text("Settings").flex_1().text_color(ZINC.shade(900)))
    .child(
        text("›")
            .text_color(ZINC.shade(300))
            .transition_transform()
            .group_active(|s| s.translate_x(px(4.0))),
    )
```

```tsx tabs
<View class="group flex-row items-center gap-3 rounded-xl bg-white p-3 active:bg-zinc-50">
  <Text class="flex-1 text-zinc-900">Settings</Text>
  <Text class="text-zinc-300 transition-transform group-active:translate-x-1">›</Text>
</View>
```

`disabled` on any element (`.disabled(..)` in Rust) applies `disabled:` classes (`.disabled_style(..)`) and removes its press handlers from what is mounted, so it shows no press feedback either:

```rust tabs
view()
    .disabled(saving)
    .on_press(save)
    .rounded_lg()
    .bg(EMERALD.shade(500))
    .px_4()
    .py_2()
    .active(|s| s.bg(EMERALD.shade(600)))
    .disabled_style(|s| s.opacity(0.4))
    .child(text("Save").font_weight(FontWeight::MEDIUM).text_color(white()))
```

```tsx tabs
<View disabled={saving()} onPress={save} class="rounded-lg bg-emerald-500 px-4 py-2 active:bg-emerald-600 disabled:opacity-40">
  <Text class="font-medium text-white">Save</Text>
</View>
```

Breakpoints compare against the window (the surface NSH renders into), not the device: in an iPad split view or a resizable window, they follow the window's width, as `window()` does.

## What differs from Tailwind on the web

- **Units are points.** `p-4` (`.p_4()`) is 16 points (1 spacing step = 4pt, 1rem = 16pt). Arbitrary values in `px` are points too: `p-[13px]` (`.p(px(13.0))`) is 13pt.
- **`flex` is a row**, as in CSS: `class="flex"` lays out its children in a row unless a direction class says otherwise (anywhere in the string: `flex-col flex` is a column). A view without `flex` or `inline-flex` keeps NSH's default column. `block` and `flow-root` use block layout; `inline` and `inline-block` lay out as views (there is no inline formatting context). In Rust, `.flex_row()` and `.flex_col()` set the direction.
- **Text classes on views are inherited.** `text-*`, `font-*`, `leading-*`, `tracking-*` and the other text utilities on a view (`.text_color(..)`, `.font_weight(..)`, ... in Rust) pass to the text and input elements inside it, as CSS inheritance does: `<View class="text-white">` makes its labels white.
- **`space-x-*` / `space-y-*` are gaps** (`gap-x` / `gap-y`).
- **State backgrounds show over gradients.** In CSS a gradient covers `background-color`, so `active:bg-*` on a `bg-linear-*` surface would never be seen; in NSH a background color from `active:`, `hover:`, `focus:` or `group-*:` replaces the gradient while that state holds (unless the state sets a gradient of its own, such as `active:from-*`). So do `activeStyle` and the other state styles, and animated background colors.
- **Classes for settings that are props in NSH**: `line-clamp-*`, `truncate`, `whitespace-nowrap`, `pointer-events-*`, `object-*` (image resize mode), `select-text`, `caret-*` and `placeholder:text-*` work, and lose to the explicit props (`numberOfLines`, `pointerEvents`, `resizeMode`, ...) when both are set. In Rust these are the element's methods (`.number_of_lines(..)`, `.resize_mode(..)`, ...).
- **The supported set**: utilities and variants of Tailwind 4 that have a native meaning. `peer-*` variants, pseudo-elements (`before:`, `after:`) and arbitrary properties (`[mask-type:luminance]`) are reported as unknown classes. Web-only utilities that need nothing natively (`antialiased`, `will-change-*`, `touch-*`, `scroll-*`, `snap-*`, `overscroll-*`, `box-border`, ...) are accepted and ignored.

Everything else works as documented by Tailwind 4: the color palette, opacity modifiers (`bg-black/40`), arbitrary values (`bg-[#34c759]`, `w-[72px]`, `grid-cols-[1fr_2fr]`), fractions (`w-1/2`, `translate-x-1/2`), shadows and rings, gradients (`bg-linear-to-r`, `bg-radial`, `bg-conic` with `from-` / `via-` / `to-`), filters, `backdrop-blur-*`, `mix-blend-*`, transforms, `transition-*` and `duration-*`, `animate-spin` / `ping` / `pulse` / `bounce`, and grid, each with its typed method in Rust. The platform limits of [filters and blending](./styling.md#filters-and-blending) apply to classes too.

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

The Tailwind theme shapes your classes. Typed methods keep Tailwind's default scales, so a Rust component looks the same in every app, and take the app's look from the component theme, `"theme"` in `nsh.json` ([Rust](./rust.md#theme)).

## Theme tokens

Rust components draw with the component theme's colors and numbers (`theme()` from `nsh_base`); classes can name them too, so a TSX screen follows the same theme its Rust components do:

```rust tabs
use nsh_base::theme;

view()
    .rounded(move || theme().radius_lg())
    .border_1()
    .border_color(move || theme().border())
    .bg(move || theme().background())
    .p_4()
    .child(text("Balance").text_color(move || theme().foreground()))
    .child(text("Updated just now").text_color(move || theme().muted_foreground()))
    .child(
        view()
            .rounded(move || theme().radius())
            .bg(move || theme().primary())
            .px_4()
            .py_2()
            .active(|s| s.bg(move || theme().primary_active()))
            .child(
                text("Continue")
                    .text_color(move || theme().primary_foreground())
                    .text_size(move || theme().font_size()),
            ),
    )
```

```tsx tabs
<View class="rounded-(--radius-lg) border border-border bg-background p-4">
  <Text class="text-foreground">Balance</Text>
  <Text class="text-muted-foreground">Updated just now</Text>
  <View class="rounded-(--radius) bg-primary px-4 py-2 active:bg-primary-active">
    <Text class="text-primary-foreground text-(length:--font-size)">Continue</Text>
  </View>
</View>
```

A token is a key of the theme file with its dots written as dashes: `muted.foreground` is `muted-foreground`, `radius.lg` is `--radius-lg`. Keys ending in `.background` can leave it out: `bg-primary` is `primary.background`, and `bg-primary-background` works too. In Rust, the keys the components use are methods of `theme()` (`primary()`, `muted_foreground()`, `radius_lg()`), and every other key is `theme().color("chart.up")` or `theme().number("radius.card")`.

| Form | Takes | Example |
|---|---|---|
| A color name in any color utility | A theme color | `bg-primary`, `text-muted-foreground/70`, `border-t-border`, `ring-ring`, `from-accent`, `placeholder:text-muted-foreground` |
| `(--name)`, Tailwind 4's variable shorthand | A theme color or number | `bg-(--primary)`, `rounded-(--radius)`, `p-(--gap-card)` |
| `(color:--name)`, `(length:--name)`, `(number:--name)` | The same, typed where a utility takes several kinds | `text-(color:--link)`, `text-(length:--font-size)` |

- **Every key is a token.** The keys the components use (`primary.background`, `border`, `radius`, `font.size`, ...; the list is in [Rust](./rust.md#theme)), and any other key a theme file has: `"sidebar.background"` makes `bg-sidebar`, `"chart.up": "#16a34a"` makes `text-chart-up`, a number `"radius.card": 10` makes `rounded-(--radius-card)`. Numbers are points; `(number:--name)` passes the plain number.
- **Both appearances.** A token has the light and the dark value of the theme: `bg-primary` and `theme().primary()` switch with the appearance by themselves, with no `dark:` class. A color the theme sets in one mode only is used in both.
- **Your Tailwind colors win.** A name in `"tailwind"` `colors` or Tailwind's palette is that color, so `bg-primary` is a theme token only when the Tailwind theme has no color `primary`.
- **The theme can change while the app runs.** When an app embedded in a native app is given that app's theme, and whenever the theme changes, the classes naming tokens and the Rust bindings reading `theme()` are restyled natively, with no JavaScript running. Tokens the embedding app's theme does not have keep the values of the app's own `"theme"` (then the default theme's).
- **Checked like other classes.** A name that is no token of the default theme or of the app's `"theme"` is an unknown class when building (`unknown class 'bg-chart-up'`). To use a token only the embedding app provides, give it a value in the app's `"theme"` too: that value shows when the app runs on its own.

Classes naming tokens are resolved once per theme, like any other class: drawing them costs what a palette color does.

## The class check

`nsh build`, `nsh dev`, `nsh run` and `nsh test` check the static class names in your JSX (string literals in `class` and the keys of `classList`) with the same parser and theme the runtime uses, and warn about classes that do not exist:

```text
 warn nsh::build App.tsx:12:36: unknown class `bg-brand-550`
```

Warnings do not fail the build. Classes built at run time (``bg-${color()}-500``) are checked when the runtime first sees them, which logs each unknown class once (`unknown classes: ...`). Typed methods are checked by the Rust compiler.

## Classes or styles?

Both are resolved natively and cost about the same. Classes are shorter and give you variants (pressed states, dark mode, breakpoints) without code. Inline styles take values computed in JavaScript and accept [native animated values](./animation.md) (`opacity: fade`, `transform: [{ translateX: x }]`). Most TSX apps use classes for the static look and `style` for what is computed or animated. Rust components use typed methods for both, and bind animated values with `.bind_opacity(..)`, `.bind_translate_x(..)` and the other bindings.

## Related

- [Styling](./styling.md): the style prop, the typed methods and the platform limits of effects.
- [Tailwind reference](../modules/tailwind-reference.md): every utility and variant.
- [Layout](./layout.md): flexbox and grid, also available as classes.

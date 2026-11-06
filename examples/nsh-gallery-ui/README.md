# nsh-gallery-ui

The repository's internal UI layer for the TSX gallery and the website's runnable demos. It composes `nsh/base`
and NSH's elements; it is maintained with these examples, rather than published as an application API.

| File | Owns |
| --- | --- |
| `theme.json` | The canonical light/dark design tokens, referenced by both apps' `nsh.json` |
| `finishes.json` | Surface gradients, shadows and glass finishes used by NSH and HTML; `on` is what stays on (a toggle, a chosen tool), a fill that stands off its surface further than a hover in both appearances |
| `theme.ts` | Reactive palette roles, finish accessors, radii, focus/hover feedback, motion and `follow` (a value springing to a control's state) |
| `index.tsx` | Shared buttons, cards, insets, badges, rows, the gallery's `Toggle`, sheets and presentation helpers |
| `look/button.ts` | What a button looks like, spread on `nsh/base`'s `Button` (or a pressable `View`): `buttonLook` (kinds, sizes, the selected, hover, pressed, focus and disabled states, the press's native sink, a hit slop to 44 points on touch screens), `buttonLabel`, `buttonGlyph`, `buttonSizes`, and the icon-only and round buttons' `iconButtonLook` / `roundButtonLook`; the gallery's `Button`, `RoundButton` and icon buttons and the demos' `button()` use it |
| `look/switch.ts`, `look/switch-geometry.ts` | What a switch looks like, spread on `nsh/base`'s `Switch`: `switchLook` (the sunk track turning citrus, the knob springing across natively, white with an olive edge on (`finish.knobOn`)) and `switchHitSlop` (a hit slop to 44 points on touch screens, for a switch that stands alone, not one that is a row with its label); its sizes, the knob's travel (the same gap to the track's edge on and off) and the slop are plain arithmetic in `switch-geometry.ts`, which `website/tests/gallery-ui-switch.test.ts` checks; the gallery's `Toggle` and switch rows and every website demo's `Switch` use it |
| `look/press.ts` | A press's native feedback: `pressSink(scale)` (an `activeStyle` sinking the control, none with reduced motion, and the `style` transition that eases it), `pressTransition`, `pressEasing` (150 ms, ease-out); the button looks, `Press` and `tabPress` use it |
| `look/tool.ts` | What a toggle and a toolbar's tools look like, spread on `nsh/base`'s `Toggle` (or a toolbar's `Button` or popup trigger, with `on`): `toggleLook` (`ghost` or `outline`, sizes, the on fill `finish.on` as `pressedStyle`, hover, the press's native sink (`look/press.ts`), focus, disabled, a hit slop to 44 points on touch screens), `toggleLabel`, `toggleGlyph`, `toggleSizes`, and the inset track and hairline of a toolbar: `toolbarTrack`, `toolbarRule`; the gallery's Toggle and Toolbar demos and the demos' toggles use it |
| `popup.tsx` | The popup families' one surface and their enter motion (`PopIn`, `createPresence` for tooltips), and `MenuMark` / `menuIndicator`: a checkable menu item's check or dot and the indicator part it follows |
| `segmented.tsx` | The segmented control shared by the gallery and demo inspectors |
| `icons.tsx` | The common line icon set, `IconLabel` (an icon-only control's icon, its short label as text in the terminal), and `CheckMark`: the check of checkboxes, selects and menus as an SVG image |
| `data-table.ts` | Compact table spacing, resize separators, sort chevrons and selection feedback |
| `scroll-area.tsx` | The scroll bar's track and thumb, and `ScrollArea`: a scroll view with that bar in place of the platform's indicator |
| `command.tsx` | A `Command`'s parts: `commandClasses` (frame, search bar, rows with hover, highlight and disabled feedback, headings, separators), `CommandRow` (icon, label, detail, note, the shortcut's `CommandKeys`, a check), `CommandHeading`, `CommandEmpty` |
| `tree.tsx` | A `Tree`'s rows: `treeItemClass` (hover, selected, focus and disabled feedback) and `TreeRow` (the chevron and folder that open with the row, the label indented by depth) |
| `avatar.ts` | Avatars' identity colors (an OkLCH hue per name, as gpui-component's), offline SVG portraits and an image that never loads |
| `slider.ts` | The slider's rail, gradient fill, pill thumb, value column and state feedback |
| `palette.ts`, `demo.ts` | The documentation demos' palette adapter, `button()` / `buttonText()` (`buttonLook` with the demos' options), the field state helper, and stage, inspector, panel and log components |
| `css.js` | CSS variables for the website, generated from the same theme and finish data |

The gallery imports through `examples/gallery/ui.tsx`; that file keeps its navigation, sidebar state, branded
header and lazy demo loading. Website demos import through `website/demos/palette.ts`. Those adapters keep
application state outside this layer while existing demo imports keep working.
The website's Vite plugin consumes `css.js`; its page CSS aliases these variables rather than keeping another
palette and another copy of the shared surface finishes.

Shared looks live in `look/`, plain `.ts` modules per control family that import only `../theme`, each other and `nsh` / `nsh/*`, so the playground can include them. A look returns the props to spread on the `nsh/base` component (`style`, `hoverStyle`, `activeStyle`, `focusStyle`, `disabledStyle`, `hitSlop`), its colors getters that follow the theme; a state's border or fill goes after the finish it changes. Colors come from the theme's tokens and finishes: `website/tests/gallery-ui-colors.test.ts` rejects color literals in the looks and the demo adapters, and a website demo's `Switch` that styles its track or thumb in place instead of spreading `switchLook`.

Add or change a shared visual rule here. Keep layout, interaction and accessibility in `nsh/base`; colors, padding,
borders, surfaces and state feedback belong here. Preserve a component's iconography when changing spacing or
moving its styles. Apps can override the shared parts to demonstrate customization.

Normal builds follow these source imports with the NSH bundler. The website playground's single-module compiler
uses `website/src/lib/inline-gallery-ui.ts`: it includes the imported shared modules in isolated scopes and applies
this same theme before the first render. Changes to shared source should be checked in both the gallery and a
website demo, in light and dark appearance, including the playground.

## Visual bar

Every example must be beautiful and refined: theme tokens for color, photographs or restrained gradients rather than
saturated placeholder tints, nothing clipped, even spacing. See "Gallery examples must be beautiful and refined" in
`AGENTS.md`.

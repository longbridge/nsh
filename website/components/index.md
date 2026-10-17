---
title: Mobile UI
description: The nsh/ui/mobile UI set, its components and modules, and how to theme them from nsh.json.
order: 1
---

# Mobile UI

`nsh/ui/mobile` is NSH's UI set for phones and tablets: buttons, cards, lists, text fields, sheets, safe areas, the
keyboard, the status bar, system dialogs and navigation. Import what you need:

```tsx
import { Button, Card, Group, Sheet, Stack, Screen, createNavigator } from "nsh/ui/mobile";
```

Use it on top of the [host elements](../elements/index.md) (`<View>`, `<Text>`, `<Input>`, ...). The elements are
the building blocks every UI set uses. The components are a finished mobile look built from those elements.

## How the components work

The components are written in Rust, not in JavaScript. In your app a `<Button>` is still one JSX node. Its props go to
the native core, and the core renders the button from host elements in the same layout pass as the rest of your UI.
Its events come back to your handlers. This has a few practical consequences:

- **The same look on iOS and Android.** A component draws itself with NSH's own views, not UIKit or Android
  widgets, so a `Button` looks the same on both platforms. The exceptions are [Alert](./alert.md) and
  [ActionSheet](./action-sheet.md), which show the platform's system dialogs.
- **Interaction stays native.** Press feedback, the drag of a [Sheet](./sheet.md) and the repeat of a held
  [Stepper](./stepper.md) button run in the core or on the UI thread. JS only receives the events (`onPress`,
  `onChange`, `onClose`, ...).
- **Typed from the source.** The TypeScript declarations in `.nsh/types/nsh/ui/mobile.d.ts` are generated from the
  Rust props, so your editor shows each prop and its documentation.
- **Your content stays yours.** Slots (`header`, `footer`), children and list rows are your JSX. The component
  positions them but never creates or changes them, so their signals, handlers and refs work as usual.

Every component accepts `class`, `classList`, `style`, `testID`, `onLayout` and `ref` on its root view
(`ComponentHostProps`). Your `class` is applied after the component's own styles, so it overrides them:

```tsx
<Button title="Continue" class="rounded-full" onPress={next} />
<Card class="mx-4" variant="outlined">…</Card>
```

## Components

| Component | What it is |
|---|---|
| [Button](./button.md) | A press target with a label: `primary`, `secondary`, `danger` or `plain` |
| [Card](./card.md) | A rounded surface with an optional header and footer |
| [Group](./group.md) | Settings-style rows in a rounded box, divided by hairlines |
| [ListView](./list-view.md) | A virtualized list of your rows, plain or inset |
| [SafeAreaView](./safe-area-view.md) | A view padded by the safe area, a screen's header or a modal's insets |
| [SelectList](./select-list.md) | A scrolling list of options with a check mark on the chosen one |
| [Sheet](./sheet.md) | A bottom sheet over a dimmed backdrop, dragged down to close |
| [StatusBar](./status-bar.md) | The status bar's content color and visibility, rendered or set by function |
| [Stepper](./stepper.md) | A number with − and + buttons; holding a button repeats |
| [TextField](./text-field.md) | A labeled text input with a clear button |
| [KeyboardAvoidingView](./keyboard-avoiding-view.md) | Keeps content outside scroll views above the software keyboard |

Modules and navigation:

| Export | What it is |
|---|---|
| [Alert](./alert.md) | `Alert.alert(title, message?, buttons?, options?)`: a system alert, resolves with the button's index |
| [ActionSheet](./action-sheet.md) | `ActionSheet.show(options)`: a system action sheet, resolves with the action's index |
| [Keyboard](./keyboard.md) | `Keyboard.dismiss()` and the reactive `keyboard()` accessor |
| [Navigation](./navigation.md) | `createNavigator`, `Stack`, `Screen` and `Modal`: native navigation stacks and modals |

The set also re-exports three environment accessors from the runtime: `safeArea()`, `keyboard()` and
`useSafeAreaInsets()` (see [SafeAreaView](./safe-area-view.md) and [Keyboard](./keyboard.md)).

## Theming

All components use colors from one theme and never use fixed color values. Each color has a light and a dark value.
When the system switches between light and dark appearance, the components change color natively, with no render and
no JS.

The default theme is gpui-kit's default theme, a neutral black-and-white palette, with mobile corner radii. To use
your own theme, set `"theme"` in `nsh.json`. Its value is the path of a theme file, relative to `nsh.json`:

```json
{
  "name": "My App",
  "id": "com.example.myapp",
  "theme": "brand-theme.json"
}
```

You can also write the theme's content inline as the value. The format is gpui-kit's theme file format. Each entry in
`themes` sets the colors for one appearance (`mode`: `"light"` or `"dark"`; without `mode`, an entry is light):

```json
{
  "themes": [
    {
      "mode": "light",
      "radius": 8,
      "radius.lg": 14,
      "font.size": 16,
      "colors": {
        "primary.background": "#7c3aed",
        "primary.foreground": "white",
        "border": "neutral-200"
      }
    },
    {
      "mode": "dark",
      "colors": {
        "primary.background": "#a78bfa",
        "primary.foreground": "neutral-950"
      }
    }
  ]
}
```

- Your theme is applied over the default theme, so you only list what you change.
- Colors are CSS colors (`#7c3aed`, `rgb(…)`, `white`) or names from Tailwind's default palette (`neutral-900`,
  `red-500`). Custom colors from `nsh.json`'s `tailwind` key are not available here.
- `radius` is the corner radius of controls (buttons, inputs, stepper buttons). `radius.lg` is the radius of large
  surfaces (cards, groups, inset lists, sheets). `font.size` is the base font size of button labels, card headers and the stepper. All values are in points.
- If you leave out a pressed color (`primary.active.background`, `secondary.active.background`,
  `danger.active.background`, ...), NSH derives it from the base color: 12% darker in light mode, 12% lighter in
  dark mode.
- Keys that only gpui-kit's desktop components use (sidebars, title bars, ...) are ignored, so you can use a gpui-kit
  theme file as it is.
- The bundler checks the theme when it builds. A missing file, invalid JSON, an unknown `mode` or a value that is not
  a color fails the build with an `nsh.json: theme: …` error.

These are the theme keys the mobile components use, with their default light and dark values:

| Key | Light | Dark | Used by |
|---|---|---|---|
| `background` | `white` | `neutral-950` | ListView, SelectList and TextField backgrounds, the Sheet panel |
| `foreground` | `neutral-950` | `neutral-50` | Text in ListView, SelectList and Stepper; Plain buttons |
| `border` | `neutral-200` | `neutral-800` | Dividers, Card borders |
| `primary.background` / `.active.background` / `.foreground` | `neutral-900` / `neutral-950` / `neutral-50` | `neutral-50` / `neutral-200` / `neutral-900` | Primary buttons; SelectList check mark, TextField selection (`primary.background`) |
| `secondary.background` / `.active.background` / `.foreground` | `neutral-200` / `neutral-300` / `neutral-900` | `neutral-800` / `#212121` / `neutral-50` | Secondary buttons, Stepper buttons |
| `danger.background` / `.active.background` / `.foreground` | `red-500` / `red-600` / `neutral-50` | `red-400` / `red-500` / `red-600` | Danger buttons |
| `accent.background` | `neutral-100` | `neutral-800` | Plain buttons while pressed |
| `muted.background` | `neutral-100` | `neutral-800` | Inset ListView page, Sheet grabber, TextField clear button |
| `muted.foreground` | `neutral-500` | `neutral-400` | Group captions, TextField labels and placeholders |
| `group_box.background` / `.foreground` | `#f5f5f5` / `#171717` | `neutral-950` / `neutral-50` | Card and Group surfaces |
| `input.border` | `neutral-200` | `#2f2f2f` | TextField border |
| `ring` | `neutral-400` | `neutral-500` | TextField border while focused |
| `list.active.background` | `#bfdbfe33` | `#1e40af33` | SelectList row while pressed |
| `overlay` | `#0000000d` | `#00000033` | Sheet backdrop |

> [!TIP]
> To adjust one component instance, pass `class` or `style`; they apply to the component's root view and override
> its styles. To change the look of a component everywhere, change the theme.

## Related

- [Styling](../docs/styling.md) and [Tailwind](../docs/tailwind.md): styling your own views.
- [Navigation guide](../docs/navigation.md) and [Forms and keyboard](../docs/forms-and-keyboard.md).
- [Platform-specific code](../docs/platform-specific.md): `ios={{…}}` / `android={{…}}` props.

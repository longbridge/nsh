---
name: nsh
description: 'How to build apps with Native Shell (NSH), in TSX or Rust, on iOS, Android, macOS, Windows, Linux, the terminal and the web. Use when a project has an nsh.json or imports "nsh", "nsh/base", "nsh/mobile", "nsh/desktop" or nsh_core / nsh_base, and when creating, running, testing or packaging an app with the nsh command; choosing an element or a nsh/base component (View, Text, List, Button, Input, Select, Dialog, Sheet, NavStack, DataTable, Editor, TextView, etc.); signals, effects and resources; Tailwind classes and theme tokens; navigation, lists, scrolling, the keyboard and the safe area; platform-specific code, capabilities and signed packages. Holds the normative Coding Guides: read them before any architecture, state-ownership, public API, naming or testing decision. Stops the common mistake of writing React, React Native or web code that NSH does not run. Pairs with the nsh-design-guides skill.'
---

# Native Shell

NSH renders one app, written in TSX or Rust, through each platform's own views.
It looks like React and React Native and works like neither: **a component runs
once, and only the bindings that read a changed signal run again.** Code
translated from React compiles surprisingly often and then does not update. If
you know React, read [Coming from React](references/from-react.md) now.

For a complete tested app (store, pages, navigation, list, dialog, tests), read
[the application recipe](references/recipes.md).

## Read the Guides First

Two guides hold the rules this skill assumes. They are requirements, not
inspiration. Read the guide file itself; do not answer from this page, from a
similar file in the codebase, or from training data.

| Guide                                        | Read before                                                                                                                 |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Design Guides, skill `nsh-design-guides`     | Choosing components, layout, spacing, hierarchy, color, density, interaction states, overlays, motion, interface copy        |
| [Coding Guides](references/coding-guides.md) | App structure, what runs once, state ownership, keys, theme, events, focus, async, lists, public API, naming, testing        |

Read the Design Guides first when the change has a visible surface. If the design
skill is not installed, fetch `https://nsh.dev/docs/design-guides.md`. The coding
guide is a verbatim copy of `https://nsh.dev/docs/coding-guides.md`, so a link in
it such as `./reactivity.md` means `https://nsh.dev/docs/reactivity.md`, and
`../modules/nsh.md` means `https://nsh.dev/modules/nsh.md`.

### Coding Guides section map

Read the whole guide for a new app, module or feature. For a narrow change, read
"Understand what runs once and what runs again" and "Rules for coding agents"
first, then the section for the change (`grep -n '^## ' references/coding-guides.md`).

| Section                                       | Read when                                                     |
| --------------------------------------------- | ------------------------------------------------------------- |
| Architecture at a glance                      | Always; layers, an app's files, the store                     |
| Bootstrap and root ownership                  | `mount`, the root, what one surface is                        |
| Understand what runs once and what runs again | Always; components, bindings, owners, context                 |
| Choose the right unit                         | Plain function, component or Rust `#[component]`              |
| State ownership                               | Where state lives, controlled values, feedback loops          |
| Stable identity                               | Keys, routes, test ids                                        |
| Rendering and composition                     | Component bodies, `<If>`, using `nsh/base` before custom views |
| Behavior and presentation boundary            | What `nsh/base` owns, parts and slots                         |
| Theme and styling                             | Tokens, the Tailwind scale, state styles, text size           |
| Events, commands, and focus                   | Presses, shortcuts, decisions as props, focus, overlays       |
| Async work and side effects                   | `resource`, effects, stale results                            |
| Layout, measurement, and scrolling            | Flex defaults, `onLayout`, scroll owners, insets              |
| Lists, tables, and large data                 | `List`, why cells keep no state                               |
| Public API design                             | Anything other code imports                                   |
| Platform and capability boundaries            | `ios={{}}`, `nsh/mobile`, `nsh/desktop`, `capabilities`       |
| File and naming conventions                   | Names of components, props, events, states                    |
| Testing strategy                              | `nsh test`, selectors, what to cover                          |
| Performance rules                             | Before adding a cache; `nsh test --trace`                     |
| Common failure modes                          | Before finishing                                              |
| Rules for coding agents                       | Always when an agent writes code                              |
| Implementation checklist                      | Before finishing; run every item against the work             |

### Non-negotiables

A floor, not a substitute for the guides.

- **Never invent an API.** Look the name up in `.nsh/types` (TSX) or the crate's
  source (Rust), then its reference page. Do not translate a React, React Native,
  CSS or GPUI example by analogy.
- **Components run once.** Read props and signals where they are used, in JSX or
  a closure. Do not destructure props, and do not copy
  a signal into a variable in a component body.
- **The look is the app's.** `nsh/base` has behavior and no look, and NSH has no
  styled component library. Style with theme tokens and the Tailwind scale, not
  raw colors and arbitrary values. Build the app's components on `nsh/base`
  instead of asking NSH to add a look or a variant.
- **Use the semantic component.** `Button`, `Menu`, `Popover`, `Sheet`, `Dialog`,
  `Select` bring focus, keyboard, dismissal and accessibility. Do not rebuild them
  from `View`.
- **No DOM, no Node, no npm UI.** There is no `window`, `document` or
  `localStorage`, and no React. Standard modules are exactly `buffer`, `console`,
  `crypto`, `fs/promises`, `net`, `os`, `path`, `process`, `url`, `websocket`,
  `zlib`; `node:` prefixes are refused.
- **Long data goes in `List`,** with per-item state in data keyed by id, never in
  a signal created in a row.
- **A decision the system needs at once is a prop**, set beforehand (`popOnBack`,
  `closeOnEscape`, `disabled`), never a callback returning a boolean.
- **Do not change NSH to fit an app.** Solve it in the app first: its own
  components, a Rust crate, or a native component.

## The Loop

An app is checked by four commands; run them before calling work done.

```sh
nsh check          # the only one that type checks: fix every error
nsh build          # treat every `warn` as a finding (unknown class, props taken apart)
nsh test           # the app's *.test.tsx, headless, without a device
nsh run            # look at it: this machine's backend; --target ios, android, web, macos, linux, windows, gpui or tui
```

**A clean `nsh build` does not mean the code is right.** It accepts `className`,
`window` and other names only `nsh check` rejects, and an unknown class is only
a warning, with that class doing nothing.

| Also | Command |
| --- | --- |
| A new app (asks on a terminal) | `nsh init my-app --template ts\|rust\|mixed` |
| A platform's real input and screenshots | `nsh test --target macos`, `ios`, `android`, `web`, `linux`, `windows` |
| What a step mounts, and what it costs | `nsh test App.tsx --trace` |
| An installable app or a static site | `nsh package --target ios\|android\|macos\|windows\|linux\|web` |
| A run-time module for a host app | `nsh publish` ([Signed packages](https://nsh.dev/docs/signed-packages.md)) |

Declarations are in `.nsh/types`, kept to the `nsh` you run. They are the authority
for what exists in this version. The documentation below follows the latest
release, and may describe a newer API than the project uses.

## Documentation

- **Find a page**: `https://nsh.dev/llms.txt`, then load its Markdown.
- **Everything**: `https://nsh.dev/llms-full.txt`, when broad reference is needed.
- **An element or component**: `https://nsh.dev/elements/{name}.md`, such as
  `button.md`, `input.md`, `list.md`, `nav-stack.md`, `dialog.md`, `data-table.md`.
- **A module**: `https://nsh.dev/modules/{name}.md`: `nsh.md`, `nsh-base.md`,
  `nsh-mobile.md`, `nsh-desktop.md`, `style-reference.md`, `tailwind-reference.md`,
  `standard-modules.md`.
- **A guide**: `https://nsh.dev/docs/{name}.md`: `reactivity.md`, `layout.md`,
  `navigation.md`, `lists.md`, `forms-and-keyboard.md`, `platform-specific.md`,
  `rust.md`, `testing-and-trace.md`.

## Quick Reference

```tsx
import { mount, signal, Text, View } from "nsh";
import { Button } from "nsh/base";

function App() {
  const count = signal(0);
  return (
    <View class="flex-1 items-center justify-center gap-4 bg-background">
      <Text class="text-3xl font-semibold text-foreground">{count()}</Text>
      <Button onPress={() => count.update((c) => c + 1)} class="rounded-(--radius) bg-primary px-5 py-3 active:bg-primary/90">
        <Text class="text-primary-foreground">Tap</Text>
      </Button>
    </View>
  );
}

mount(App);
```

- **Tags**: PascalCase and imported. `If` and `Each` are globals, with no import:
  `<If when={open()}>`, `<Each of={items()} key={(i) => i.id}>{(item) => …}</Each>`,
  where `item` is a signal: `item().title`.
- **Text**: only inside `<Text>`; a string directly in a `<View>` is a build error.
- **Layout**: a `View` is a column by default, as in React Native. `class="flex"` or
  `flex-row` is a row. Text sizes and spacing use the Tailwind scale.
- **Theme tokens** are shadcn/ui's: `bg-background`, `text-foreground`, `bg-primary`,
  `text-primary-foreground`, `bg-muted`, `text-muted-foreground`, `border-border`,
  `border-input`, `text-destructive`, `rounded-(--radius)`. There is no
  `text-destructive-foreground`. Add roles in the app's theme file (`nsh.json` `"theme"`).
- **Safe area**: `pt-[env(safe-area-inset-top)]`, `pb-[env(safe-area-inset-bottom)]`.
- **Unstyled means unstyled**: a `Switch` without `trackClass` and `thumbClass` draws no
  switch at all, and a test still passes. Style every part, and look at the result.
- **Input**: `<Input value={v()} onChange={({ value }) => v.set(value)} />`; every event
  takes one object.
- **Pages**: a page gives `NavPage` both a `title` (the system header on iOS and
  Android) and a `header` (its own, shown on the desktops and the web, with its Back).
- **Network**: an app with `"capabilities"` in `nsh.json` may reach only what it lists.

In Rust (`nsh init --template rust` writes a working app; [Building with Rust](https://nsh.dev/docs/rust.md)):

```rust
use nsh_base::theme;
use nsh_core::prelude::*;

#[component]
pub fn App() -> impl IntoElement {
    let count = signal(0_u32);
    view()
        .flex_1()
        .bg(move || theme().background)
        .child(text(move || format!("Count: {}", count.get())))
}
```

A closure is a binding; a value read outside one is read once. `snake_case` Rust names
are `camelCase` in TSX (`on_open_change` is `onOpenChange`).

## Catalog

Exact props are in `.nsh/types` and `https://nsh.dev/elements/{name}.md`.

| Import from | What |
| --- | --- |
| `nsh`: elements | `View` `Text` `Scroll` `List` `Image` `Canvas` `Overlay` `Video` `WebView` `Shader` `ErrorBoundary` |
| `nsh`: reactivity | `signal` `computed` `effect` `resource` `batch` `untracked` `cleanup` `context` `split` `mount` `lifecycle` |
| `nsh`: animation, gestures | `Animated` `createAnimatedValue` `createMotion` `Easing` `LayoutAnimation` `PanResponder` |
| `nsh`: the environment | `platform` `colorScheme` `setColorScheme` `fontScale` `locale` `reduceMotion` `safeArea` `safeAreaInsets` `windowSize` `pixelRatio` `Keyboard` `keyboardFrame` `token` |
| `nsh`: services | `Storage` `Clipboard` `Picker` `Notifications` `Speech` `openUrl` `keymap` `host` `announce` |
| `nsh/base`: actions, forms | `Button` `Link` `Input` `NumberInput` `OtpInput` `Checkbox` `Radio` `RadioGroup` `Switch` `Slider` `Toggle` `ToggleGroup` `Select` `Combobox` `DatePicker` `TimeField` `Calendar` `ColorPicker` |
| `nsh/base`: navigation, structure | `NavStack` `NavPage` `Tabs` `Tab` `Toolbar` `Pagination` `Accordion` `Collapsible` `Carousel` `ResizablePanelGroup` `DockArea` `Scrollbar` |
| `nsh/base`: overlays | `Dialog` `AlertDialog` `Sheet` `Popover` `HoverCard` `Tooltip` `Menu` `MenuItem` `ContextMenu` `Submenu` `Command` `Toast` |
| `nsh/base`: data, documents | `Table` `DataTable` `Tree` `Avatar` `Progress` `TextView` `Editor` `DiffView` `RichEditor` |
| `nsh/base/plot` | `Plot` and its parts: charts |
| `nsh/mobile` | `TabBar` `BackHandler` `InputAccessoryView` |
| `nsh/desktop` | `Window` `Menu` (the application menu) |
| `nsh/updater` | App self-update |
| `nsh/test` | `test` `expect` `open` `beforeEach`, and the `App` steps: `press` `type` `waitFor` `back` `onDialog` `screenshot` |

## Working on NSH itself

This skill is for building apps. In a checkout of `longbridge/nsh`, follow its
`AGENTS.md` and `docs/dev/` as well.

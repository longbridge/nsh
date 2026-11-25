---
title: Coding Guides
description: "Architecture and coding conventions for maintainable Native Shell apps and component libraries, in TSX and Rust."
order: 29.5
group: ui
---

# Coding Guides

This guide shows how to structure a Native Shell (NSH) app and its code so it stays easy to change. It is for
engineers and coding agents who build NSH apps (TSX modules, Rust components, native apps that embed NSH) or component
libraries on `nsh/base`. Read the Design Guides first. Code structure should carry out the product design, not replace
it.

The rules use two words. **Must** marks a lifecycle, correctness or ecosystem constraint. **Should** marks the default
architecture: depart from it only for a concrete reason. For exact signatures, the source, the reference pages and the
declarations in `.nsh/types` win over this guide.

## Build the app's UI foundations before pages

For a new app, set up the design standards, theme tokens and the app's own styled components before you build
feature pages. For an existing app, find these first and reuse them. Keep the standards and shared UI code in the app's
repository. The [Design Guides](./design-guides.md#establish-the-apps-design-system-first) describe their contents.

Use this development order:

1. Identify the app's Desktop, Mobile, Web and TUI targets and their input, navigation and density requirements.
2. Define the app's semantic color roles and light/dark values in its theme file (`nsh.json` `"theme"`), together
   with the typography, spacing, radius and size scales the components use.
3. Create the app's `ui` module (`src/ui/`) and wrap and compose `nsh/base` into its core set of styled controls
   ([The app's `ui` layer](./design-guides.md#the-apps-ui-layer)). Each control takes `variant`, `size`, states and
   slots, never colors or radii. Build each one with its visible parts, variants and interaction states. Check a
   typical composition on the supported targets before you repeat it across pages.
4. Compose pages from that layer. If a token or shared variant is missing, add it to the layer that owns it first, then
   use it in pages. A change to a shared control should update every page that uses it.

NSH provides the primitives (`nsh-core`, `nsh` in TSX) and the unstyled `nsh-base` crate (`nsh/base` in TSX). The app
or a community library owns the styled layer above them. For ordinary UI, app code imports its own styled controls.
Assembling and styling base components happens in the shared layer. Adapt the look for each platform, but keep the
base behavior, focus and accessibility.

Read theme colors through semantic tokens: `theme().primary` or `theme().color("warning")` in Rust,
`bg-primary`, `text-warning` or `token("warning")` in TSX. A semantic token names a color by its role (primary,
warning), not by its value. Define raw hex, RGB/RGBA and HSL/HSLA values only in the theme source. This includes state
styles, gradients and shadows. A palette class such as `bg-blue-500`, or an arbitrary color class, also skips the
app's roles. Add the missing role instead.
Token opacity such as `bg-primary/90` or `theme().primary.opacity(0.9)` keeps the semantic source of the color.
User-authored colors and data values, such as an image's pixels or a color picker's selected value, remain data;
the surrounding controls still use theme tokens.

Before you finish, check new presentation code for raw color literals and palette classes. Check that it uses the
shared components. Try the feature's states in light and dark mode on each target platform. If you could not test a
platform, say so. One screenshot does not prove every target.

## Make the common path inexpensive

**Design principle: choose framework types and ownership so ordinary component use is inexpensive by default.**
You should be able to build and reuse UI without adding a cache at every call site. Two choices make this work.
A component runs once and builds its part of the tree. After that, only the bindings that read a changed
[signal](./reactivity.md) run again. A signal is a value that tracks who reads it. The reactive graph (the record of
which bindings read which signals), the renderer, layout and lists are native code. One turn of the app's code becomes
one batch of native operations. Small costs still add up across elements and updates. This matters most in TSX, which
runs in an interpreter. Measure a hot path before you add a cache ([Performance](./performance.md)). Neither choice
makes work free. The [state ownership rules](#state-ownership) and later rendering sections explain the mechanics.

## Architecture at a glance

<img class="diagram diagram-light" src="/diagrams/application-layers-light.svg" alt="NSH application architecture: the App Shell, Pages and App Components above a Component Library, nsh/base and nsh; dependencies point downward.">
<img class="diagram diagram-dark" src="/diagrams/application-layers-dark.svg" alt="NSH application architecture: the App Shell, Pages and App Components above a Component Library, nsh/base and nsh; dependencies point downward.">

Dependencies point downward. Higher layers own the domain logic and decide what happens when. Lower layers own
reusable presentation or behavior. Do not make a reusable component depend on an app page, or make a component library depend
on one app's theme ([Three layers](./architecture.md#three-layers)).

Use these boundaries:

- **app shell:** the entry (`App.tsx`, or `nsh.json` `"root"`) composes the root and the navigation stacks while
  keeping page logic out;
- **page:** one `NavPage` with its content, its dialogs and the work it starts;
- **app component:** a repeated domain-aware pattern;
- **component library:** styled, general-purpose UI, the app's own or the community's, built on `nsh/base`;
- **`nsh/base`:** reusable behavior without product presentation: navigation, dialogs, popovers, menus, sheets, a
  press target;
- **`nsh`:** the primitives, styles, layout, reactivity and the environment.

### Lay out an app's files

NSH finds an app's files by convention, so there is nothing to configure. `App.tsx` is the entry, and `nsh.json`
sits beside it.

```text
App.tsx            # the entry, by convention: mount(App)
nsh.json
Cargo.toml         # the app's crate, with Rust components ("rust": ".")
src/
  lib.rs
pages/
  Inbox.tsx
  Compose.tsx
  Settings.tsx
components/
  Card.tsx
store.ts
```

- `pages/` holds one file per `NavPage`. A page grows into more files only when it needs them.
- `components/` holds the app's reusable components, the ones several pages use.
- `store.ts` holds the app's state: its signals and the functions that change them. Pages read the signals and call
  the functions.
- Tests are `*.test.tsx` files beside what they test (`App.test.tsx`, `pages/Inbox.test.tsx`).
- `Cargo.toml` and `src/lib.rs` are the app's own crate, with its Rust components (`nsh.json` `"rust": "."`); its
  `#[native_module]` in `src/lib.rs` sets the name TSX imports. An app written only in Rust is the same Cargo project,
  with its root component in `src/lib.rs` ([Project structure](./project-structure.md)).

The app shell composes pages but holds little page logic. Pages read the store's signals and call its functions.
They never write another page's state. Keep the store's signals private, and export functions that read and change
them. The mutable state then stays behind those functions:

```tsx tabs
// store.ts
import { signal } from "nsh";

const name = signal("Untitled");

/** The workspace's name (tracked where it is read). */
export const workspaceName = () => name();

/** Renames the workspace. */
export function renameWorkspace(next: string) {
  name.set(next);
}
```

```rust tabs
/// The app's workspace: its state stays behind these functions.
#[derive(Clone, Copy)]
pub struct Workspace {
    name: Signal<SharedString>,
}

impl Workspace {
    /// A workspace named "Untitled".
    pub fn new() -> Self {
        Self { name: signal(SharedString::from("Untitled")) }
    }

    /// Its name (tracked where it is read).
    pub fn name(&self) -> SharedString {
        self.name.get()
    }

    /// Renames it.
    pub fn rename(&self, name: impl Into<SharedString>) {
        self.name.set(name.into());
    }
}
```

A page may hold the workspace and pass its id to search. Search does not need the workspace's whole collection or its
private signals. A simple acyclic dependency shape is:

```text
App.tsx ──▶ pages/ ──▶ store.ts
              └──────▶ components/ ──▶ nsh/base, nsh
```

Pages may depend on the store and on components, but not on the shell or on another page's private parts. If one page
must change what another shows, do it through the store. With these boundaries, engineers or AI agents can work on
separate pages in parallel with fewer file conflicts. Changes to the store still need coordination.

## Bootstrap and root ownership

Mount the app once. Its root declares the app's top-level windows. Each window holds a `flex: 1` view with a
background from the theme. A TSX entry calls `mount(App)`. A Rust app names its root component in `nsh.json`
(`"root": "App"`):

```tsx tabs
import { mount, View } from "nsh";
import { Window } from "nsh/desktop";

mount(() => (
  <Window id="main">
    <View class="flex-1 bg-background">
      <App />
    </View>
  </Window>
));
```

```rust tabs
use nsh_core::{element::desktop::window, prelude::*};

#[component]
pub fn App() -> impl IntoElement {
    provide_context(Workspace::new());
    window().id("main").content(
        view()
            .flex_1()
            .bg(move || theme().background)
            .child(inbox()),
    )
}
```

The app root coordinates what belongs to the whole app: the shared state, settings and the window registry. Each
window's content coordinates what belongs to that window: its navigation stack, dialogs, popovers, menus, sheets and
tooltips, and its safe area. Do not mount a second root for each page. One app is one engine. Each `NshAppView`,
`NshView` or `AppView` that a native app embeds runs its own VM, module state and reactive graph. So several NSH views
in one host app never share state by accident.

## Understand what runs once and what runs again

NSH follows Solid's model ([Reactivity](./reactivity.md)). You describe the UI declaratively, and NSH keeps its state
between updates. A signal survives across updates. A component body describes its part of the tree once, when it is
created. Keep that difference in mind:

| You write | Runs | Runs again when |
| --- | --- | --- |
| A component body | Once, when it is created | Never |
| A binding: a closure (Rust) or an expression that calls or reads a property (TSX) where a value goes | Once | A signal it read changes; only that prop or child updates |
| `computed(fn)` | When created | A signal it read changes |
| `effect(fn)` | After the elements of the pass exist | A signal it read changes |
| A row of `each` / `<Each>` | When its key first appears | Never; its item and index signals change |
| A cell of `virtual_list` / `<List>` | Once per cell | Never; the cell is given other rows |

- the component that runs is the **owner**. Effects, computed values, tasks, timers, key bindings and cleanups created
  while it runs belong to it. They are disposed with it;
- **context** (`provide_context` / `use_context`, `context()` and `<Ctx.Provider>`) gives a subtree a value without
  implying that the elements reading it own it;
- a Rust component's **`Scope`** (`cx`) emits its events and reads the insets it is given;
- **refs** (`element_ref()`, `ref`) name a host element for imperative calls (`focus()`, `scroll_to()`, `measure()`);
- layout and measurement are later, native phases. Use `onLayout` and `measure()` only when you need the resolved
  geometry.

Never read a context in an event handler or a timer. There is no component there to look it up from. Read it in the
body and keep the value. Keep typed handles (signals, resources, refs, domain ids). They are `Copy` in Rust, and they
stop doing anything once their owner is gone.

## Choose the right unit

### Use a plain function for value-like elements

**Default to a plain function for small, reusable pieces of UI.** The caller passes the current props and handlers.
The function returns elements, and only the bindings it made remain. Its arguments are its props. The label, value and
callbacks come from the caller's state. Like any component, it runs once. Afterwards only its bindings change.

```tsx tabs
function EmptyState(props: { title: string }) {
  return (
    <View class="items-center gap-2">
      <Text class="text-muted-foreground">{props.title}</Text>
    </View>
  );
}
```

```rust tabs
fn empty_state(title: impl Into<SharedString>) -> impl IntoElement {
    view()
        .gap_2()
        .items_center()
        .child(text(title.into()).text_color(move || theme().muted_foreground))
}
```

### Use a component for retained behavior

Use a `#[component]` (or a TSX component that creates signals) when a piece of UI owns persistent state or an
independent lifecycle: signals, effects, async work, timers, key bindings. Create that state once, in the component
body, never inside a binding.

```tsx tabs
function Search(props: { placeholder: string }) {
  const query = signal("");
  return <Input value={query()} onChange={({ value }) => query.set(value)} placeholder={props.placeholder} />;
}
```

```rust tabs
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SearchProps {
    /// Shown while the field is empty.
    pub placeholder: Prop<SharedString>,
}

#[component]
pub fn Search(props: SearchProps) -> impl IntoElement {
    let query = signal(String::new());
    input()
        .value(move || query.get())
        .on_change(move |text: &str| query.set(text.to_owned()))
        .placeholder(move || props.placeholder.get().to_string())
}
```

A `#[component]` has typed props (`#[derive(Props)]`) and events (`#[derive(Event)]`); TSX imports it with
declarations generated from them. Retained state that TSX creates with `new` (a ledger, a parser, a session) is a
`#[class]`. Plain functions are a `#[module]` ([Building with Rust](./rust.md#extending-a-rust-app-with-tsx)). Write the
parts an app uses most, component libraries and heavy logic in Rust, which runs as native code. Use TSX for what
changes often or ships at run time.

Do not turn every visual fragment into a component. Each component boundary adds lifecycle and coordination cost.
Use one where the piece must keep its identity across updates.

### Elements, views, and behavior systems are different

Do not force every component into one template. The ecosystem contains:

- semantic elements such as `Button`, `Switch` and `MenuItem`;
- compound behavior roots such as `Dialog`, `Popover`, `Menu`, `ContextMenu`, `Sheet` and `NavStack`;
- stateful systems such as `Input`, `List`, `TextView` and a navigation stack's state;
- infrastructure such as overlays and their placement, virtualization, scrolling, safe areas, the keyboard and
  animation.

An element can be complex inside and still look like a plain value to its caller. A stateful system can expose
render functions, so apps own the presentation without rewriting the behavior. Choose the public API from the
behavior, not from how many views it outputs.

## State ownership

### Own persistent UI text with `SharedString`

**In Rust, you must prefer `SharedString` for UI text kept across updates or in callback closures.** This includes
component labels, titles and placeholders. Props take it (`Prop<SharedString>`), and cloning it into a binding does not
copy the text. For an editing buffer or text you build with formatting, use `String` and convert it when it is ready.
For read-only access during one call, use `&str`. In TSX, strings are ordinary values. Keep text computed from state in
a `computed` or a closure, instead of rebuilding it in several places.

When the UI keeps response text from a server, keep the received value as it is. Put formatted or derived display
text in a separate value instead of rewriting the response.

Put each state in the narrowest owner that can keep it correct:

- domain state belongs to the store (`store.ts`) or the app's crate;
- transient view state belongs to the component that renders it, as a signal;
- reusable behavioral state belongs to the component designed for it (a `Menu` or `Popover` without `open` keeps its
  own);
- state a subtree shares is a signal provided through [context](./reactivity.md#context);
- what outlives a run belongs in [storage](../modules/nsh.md#storage) or on a server.

Prefer controlled values for ordinary selection and toggles. A controlled component gets its value from its owner.
Pass the current value in, receive the requested change, and update the owner. The bindings follow. A callback reports
what the user wants. It should not create a second, hidden copy of the state.

```tsx tabs
<Switch checked={showHidden()} onChange={({ checked }) => showHidden.set(checked)}>
  <Text>Show hidden files</Text>
</Switch>
```

```rust tabs
switch()
    .checked(show_hidden)
    .on_change(move |next: &bool| show_hidden.set(*next))
    .child(text("Show hidden files"))
```

There is nothing to notify. Writing a signal updates exactly the bindings that read it. A Rust component sends its
owner an event with `cx.emit(..)`. An effect follows a signal for as long as its owner lives. When several fields must
stay consistent with each other, write them in one [`batch`](./reactivity.md#batch). Effects and computed values then
see all the changes at once.

The app's own controlled components follow the same contract. Here is a quantity stepper written once in Rust and
used from TSX and Rust:

```tsx tabs
import { signal } from "nsh";
import { QuantityStepper } from "my-app";   // the Rust component: src/lib.rs has #[native_module("my-app")]

function CartRow() {
  const quantity = signal(1);
  return (
    <QuantityStepper
      value={quantity()}
      min={1}
      max={10}
      decrementLabel="Fewer"
      incrementLabel="More"
      onChange={({ value }) => quantity.set(value)}
    />
  );
}
```

```rust tabs
use nsh_base::{button, theme};
use nsh_core::prelude::*;

/// The props of [`QuantityStepper`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct QuantityStepperProps {
    /// The quantity: the app owns the value.
    #[prop(default)]
    pub value: Prop<u32>,
    /// The smallest quantity (0 when left out).
    #[prop(default)]
    pub min: Prop<u32>,
    /// The largest quantity (99 when left out).
    #[prop(default = 99)]
    pub max: Prop<u32>,
    /// What screen readers say for the button that takes one away.
    pub decrement_label: Prop<SharedString>,
    /// What screen readers say for the button that adds one.
    pub increment_label: Prop<SharedString>,
}

/// What a [`QuantityStepper`] reports.
#[derive(Debug, Clone, PartialEq, Event)]
pub enum QuantityStepperEvent {
    /// The user asked for another quantity; the owner sets `value`.
    Change(u32),
}

/// A quantity with a button to take one away and one to add one.
#[component]
pub fn QuantityStepper(props: QuantityStepperProps, cx: Scope<QuantityStepperEvent>) -> impl IntoElement {
    // Read untracked: the user's act, measured against the owner's current value.
    let step = move |delta: i64| {
        let value = props.value.get_untracked();
        let (min, max) = (props.min.get_untracked(), props.max.get_untracked());
        let next = (i64::from(value) + delta).clamp(i64::from(min), i64::from(max)) as u32;
        if next != value {
            cx.emit(QuantityStepperEvent::Change(next));
        }
    };
    view()
        .flex_row()
        .items_center()
        .gap_3()
        .child(step_button(
            "−",
            move || props.decrement_label.get(),
            move || props.value.get() <= props.min.get(),
            move || step(-1),
        ))
        .child(text(move || props.value.get().to_string()).text_color(move || theme().foreground))
        .child(step_button(
            "+",
            move || props.increment_label.get(),
            move || props.value.get() >= props.max.get(),
            move || step(1),
        ))
}

fn step_button(
    symbol: &'static str,
    label: impl Fn() -> SharedString + 'static,
    disabled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    button()
        .disabled(disabled)
        .accessibility_label(move || Some(label()))
        .on_press(on_press)
        .w(px(32.0))
        .h(px(32.0))
        .items_center()
        .justify_center()
        .rounded(move || theme().radius)
        .bg(move || theme().muted)
        .active(|s| s.opacity(0.6))
        .disabled_style(|s| s.opacity(0.4))
        .child(text(symbol).text_color(move || theme().foreground))
}

// The owner:
let quantity = signal(1_u32);
quantity_stepper()
    .value(quantity)
    .min(1)
    .max(10)
    .decrement_label("Fewer")
    .increment_label("More")
    .on_change(move |next: &u32| quantity.set(*next))
```

The same contract in TSX, a component in `components/`:

```tsx
import { Text, View } from "nsh";
import { Button } from "nsh/base";

export interface QuantityStepperProps {
  /** The quantity: the app owns the value. */
  value: number;
  /** The smallest quantity (0 when left out). */
  min?: number;
  /** The largest quantity (99 when left out). */
  max?: number;
  /** What screen readers say for the button that takes one away. */
  decrementLabel: string;
  /** What screen readers say for the button that adds one. */
  incrementLabel: string;
  /** The user asked for another quantity; the owner sets `value`. */
  onChange?: (e: { value: number }) => void;
}

export function QuantityStepper(props: QuantityStepperProps) {
  const min = () => props.min ?? 0;
  const max = () => props.max ?? 99;
  const step = (delta: number) => {
    const next = Math.min(max(), Math.max(min(), props.value + delta));
    if (next !== props.value) props.onChange?.({ value: next });
  };
  const buttonClass = "size-8 items-center justify-center rounded-(--radius) bg-muted active:opacity-60 disabled:opacity-40";
  return (
    <View class="flex-row items-center gap-3">
      <Button disabled={props.value <= min()} accessibilityLabel={props.decrementLabel} onPress={() => step(-1)} class={buttonClass}>
        <Text class="text-foreground">−</Text>
      </Button>
      <Text class="text-foreground">{props.value}</Text>
      <Button disabled={props.value >= max()} accessibilityLabel={props.incrementLabel} onPress={() => step(1)} class={buttonClass}>
        <Text class="text-foreground">+</Text>
      </Button>
    </View>
  );
}
```

### Keep ownership links and subscriptions bounded

A component owns what it creates. Everything it created goes away with it: when its `<If>` branch hides, its row is
removed or its page is popped. In Rust, tasks, timers, intervals (`spawn`, `set_timeout`, `set_interval`) and key
bindings (`on_key`) end with the owner they were made under. In TSX, register the stop with
[`cleanup`](./reactivity.md#cleanup) or return it from the effect. To listen, call `on(event, listener)`
(`host.on(name, listener)`, `BackHandler.on("press", listener)`). It returns a function that stops listening
(`() => void`, a `Stop` in Rust). A listener registered while a component renders goes away with the component. The
same holds for `keymap.bind` bindings.

When something keeps running after it should stop, check these first:

- effects and computed values created outside a component. In an event handler or a timer callback they have no
  owner and live forever;
- listeners registered without their stop;
- work started from a store that no component owns.

The list is not complete. A dropped `resource` load is aborted (`info.signal` in TSX). It does not keep its component
alive.

Do not write a signal just because a value was read or derived. Do not write a signal unconditionally from a binding
or effect that reads it. The binding then reruns itself until NSH stops it with an error. A value computed from other
state is a `computed`, not an effect that writes a second signal.

### Avoid state feedback loops

Text input, selection, filters and controlled popups often have two paths. The owner updates the value, and the
user asks for a new value. When the owner's value arrives, do not send it back through the user callback. When the
user acts, read the owner's current value untracked and compare it with the next one. Untracked means the read does
not subscribe to the signal. In Rust this is `props.value.get_untracked()`, as above. A TSX event handler is already
untracked. Each change is then reported once. A callback may close, replace or update the component that called it,
in the same call. In that case, make the callback safe to re-enter.

## Stable identity

A key is part of behavior. It gives a row, a page or a repeated control a stable identity. When the data changes,
the key decides which item keeps its state, focus and measurements.

- Use stable domain ids for rows, tabs, pages and repeated controls: `each(projects, |p| p.id, ..)`,
  `<Each of={projects()} key={(p) => p.id}>`, a list's `keyExtractor`.
- Namespace test ids with their owning object when the same control repeats (`delete-${project.id}`).
- Never derive identity from a translated label or a mutable list index when items can be inserted or reordered.
- Do not generate a fresh random key or id while rendering.

```tsx tabs
<Each of={projects()} key={(p) => p.id}>
  {(project) => (
    <Button onPress={() => remove(project().id)}>
      <Text testID={`delete-${project().id}`}>Delete</Text>
    </Button>
  )}
</Each>
```

```rust tabs
each(projects, |p| p.id, move |project, _index| {
    let id = project.get_untracked().id;
    button()
        .on_press(move || delete(id))
        .child(text("Delete").test_id(&format!("delete-{id}")))
})
```

A changed key means a changed UI identity. Treat that reset as deliberate.

The same rule applies to navigation routes, list keys, storage keys and test ids. A route that stays in `setRoutes`
keeps its page and its state. If two separate behaviors share a key, they can overwrite each other's state. If a
behavior changes its key on every update, it never keeps any state.

## Rendering and composition

Keep a component body declarative: read state, derive display values and compose elements. Move domain operations,
parsing and complex changes into named functions in the store or the app's crate.

```tsx tabs
function ProjectPage() {
  return (
    <View class="flex-1">
      <Toolbar />
      <Content />
    </View>
  );
}
```

```rust tabs
#[component]
pub fn ProjectPage() -> impl IntoElement {
    view()
        .flex_1()
        .child(toolbar())
        .child(content())
}
```

Extract a helper when it names a clear region and means the reader keeps less in mind at once. Extract a new
component when the region has its own reusable contract or lifecycle. A long builder chain or JSX block alone is not
a reason.

In Rust, use the typed style methods and the components' builders. In TSX, use classes. For small conditional
changes, use `.when(..)` / `.when_some(..)` (Rust) and `classList` (TSX). When the branches are very different UIs, use
`when` / `<If>`. A ternary in a closure rebuilds its branch whenever anything it reads changes.

Start from the standard component before you build a custom one. Do not rebuild a menu, a popover, a sheet or a
dialog from plain views just to match one screenshot. `Menu`, `ContextMenu`, `Popover`, `Sheet`,
`Tooltip` and `Dialog` from `nsh/base` preserve placement, focus transfer, keyboard navigation, selection, disabled
state, dismissal on Escape and back, and accessibility. If the standard component cannot express a valid pattern you
need often, improve its API. Do not style its inner views at each call site.

Render functions from app code should have no side effects. A list's row, a `TextView`'s `renderCodeBlock` or a
menu's items may run whenever their owner needs a cell, a part or an opening. They must not perform a business
operation, append data or start an unbounded subscription.

## Behavior and presentation boundary

The durable Base rule is:

> Base owns reusable behavior and the structure required to implement it. The presentation layer owns the product's
> visual language.

"Unstyled" does not mean "one empty view." Overlay placement, keyboard navigation, editing, virtualization, back
navigation, dismissal and safe areas need internal structure and state. Moving that work to every caller would not add
flexibility. It would copy fragile behavior into every app.

Base must not choose brand colors, typography, density, final icons, component variants, strings or how the app is
composed. Expose presentation through `class` / `style` on the main part, `<part>Class` /
`<part>Style` for every other part, typed state styles (`active`, `hover`, `focus`, `selected`, `checked`, `disabled`),
named slots (`trigger`, `header`, `footer`) and render functions. Do not search the children for titles, descriptions
or close buttons. Make each part explicit, and take every label as a prop or a child.

## Theme and styling

Read semantic values from the theme and apply layout with the typed style methods in Rust and Tailwind classes in TSX
([Tailwind](./tailwind.md), [theme tokens](./tailwind.md#theme-tokens)):

```tsx tabs
<View class="rounded-(--radius) border border-border bg-background text-foreground" />
```

```rust tabs
view()
    .bg(move || theme().background)
    .text_color(move || theme().foreground)
    .border_1()
    .border_color(move || theme().border)
    .rounded(move || theme().radius)
```

Rules:

- do not hard-code product colors, corner radii, spacing, or control geometry;
- app code must not introduce raw hex, `rgb` / `rgba` or `hsla` values; read a semantic color from `theme()` or a
  token class (`bg-primary`), or add the missing role to the app's theme file;
- app layout should use the spacing and type scales (`p-2`, `gap-3`, `w-64`, `text-sm`; `.p_2()`, `.gap_3()`,
  `.w_64()`, `.text_sm()`) instead of arbitrary values (`p-[13px]`, `px(13.0)`);
- classes mean exactly what they mean in Tailwind CSS v4, and style values what the
  [style reference](../modules/style-reference.md) gives them (CSS's, in native forms: numbers are points); an
  `unknown class` warning is a finding to fix;
- use semantic tokens for meaning, not palette position;
- keep state-independent geometry in the ordinary builder chain or class string;
- use `active:`, `hover:` and `focus-visible:` (`.active(..)`, `.hover(..)`, `.focus_visible(..)`) for interaction
  states. They apply natively, with no round trip to app code. `focus-visible:` is the keyboard focus ring. It shows
  when the keyboard moved the focus, not after a click or a tap. Use `focus:` for any focus (an input's edge, a list's
  highlighted row);
- leave `cursor` unset on buttons, rows, tabs, menu items and other controls. The platforms already show the default
  arrow there, the I-beam over inputs and selectable text, and the pointing hand over `Link` and text links.
  Set `cursor-pointer` only on content that behaves as a link (opens a URL or leaves the app), and resize, grab or
  `not-allowed` only while they describe the actual manipulation;
- make text selectable only where people copy it (message bodies, code, logs, values): put `selectable` on that
  `Text`/`TextView` or on the content view that holds it, never on a whole screen, sidebar or list. Every text in a
  selectable view shows the I-beam, so labels, headings, timestamps and badges there look editable. Give them
  `selectable={false}` (Rust `.selectable(false)`) or move them out. Text inside a control (button, tab, menu item,
  link) keeps the default arrow;
- use a component's semantic state styles (`selected:`, `checked:`, `disabled:`) for checked, selected, pressed, or
  disabled appearance;
- keep popup ownership in explicit state (`open` with `onOpenChange`) so its trigger can render the open or pressed
  appearance until dismissal;
- keep component variants few and meaningful rather than adding a variant for every call site;
- give the primary style to the action that the Return key triggers, the real default. Do not pick it by action
  count, frequency or toolbar position;
- use few badge and alert variants, each with a clear meaning. Ordinary metadata stays neutral. Do not give every
  enum case or section its own color just because a variant exists.

The effective precedence is the element's own style, then the state styles, later states winning, with `disabled`
over all. Later layers only replace fields they set.

Prefer the theme's semantic tokens for new presentation code. Every name in the app's theme file is a token, like a
variable in Tailwind's `@theme`. For example, `"up": "#16a34a"` gives `text-up`, `bg-up/30` and `token("up")` in TSX,
and `theme().color("up")` in Rust. The theme has light and dark values, so a token follows the appearance without a
`dark:` class. Name tokens after roles, not components. Otherwise every widget grows its own token.

Change the theme in the app's theme file (`nsh.json` `"theme"`), or let the embedding app supply its own. When the
theme changes, NSH restyles token classes and Rust bindings that read `theme()` natively, with no JavaScript running.
Code that copies a token's value into its own signal stops following the theme.

A focus ring drawn outside a control needs room. An ancestor with `overflow: hidden` (`overflow-hidden`) clips it.
Prefer layouts that leave room. If a design must clip, draw the focus state inside the control. Do not hide keyboard
focus without notice.

### The user's text size is the app's zoom

Text follows the user's text size setting (Dynamic Type, Android's font scale, the desktop settings). Read it with
`fontScale()` in TSX or `font_scale()` in Rust. Both are reactive. The type scale sets the base sizes (`text-sm`, and
`text-base` at 16 points for body text). The user's setting scales them. This follows Tailwind: named steps for type,
spacing and size, instead of unrelated point values.

Let text containers grow with their text: give them minimum heights and padding rather than fixed heights. A custom
component that combines scaled text with fixed geometry around it must document why that part should not grow.

In review, flag every arbitrary point value and raw color in app UI. Accept one only for a documented physical or
platform limit, geometry measured at run time, image or data colors, or the theme definition itself. Convenience and
matching a screenshot are not valid reasons.

Anything cached from resolved layout must be invalidated when the font scale changes. Put the scale in the cache key,
or a revision number that changes with it. This covers row heights given to a list, text measurements, popup geometry,
icon sizes based on text, and custom canvas metrics.

## Events, commands, and focus

Use presses for activation. `onPress`, `onLongPress`, `onPressDown` and `onPressUp` work the same with a finger, a
pen, a mouse or the keyboard. Use touch handlers and gestures only for what follows the finger (a pan, a drag). Use
keyboard shortcuts (`keymap.bind`, `on_key`) and menu items for commands that several inputs can trigger. Keep the
handlers close to the page that owns the command.

Write each command once. A `Button`, a `Menu` item, a `ContextMenu` item, an `nsh/desktop` application menu item and a
keyboard shortcut should all call the same store function. Do not copy the change five times. Where you can, derive
their label, icon, shortcut and disabled state from one place, so they cannot disagree. `key_label("mod+s")` writes a
shortcut in the platform's style. The menu owns navigation and dismissal. The page decides whether the command is
allowed and what it does.

```tsx tabs
keymap.bind("mod+s", save);

<View>
  <Button onPress={save}><Text>Save</Text></Button>
  <Menu trigger={<View class="px-3 py-2"><Text>File</Text></View>}>
    <MenuItem onSelect={save}><Text>Save  {keymap.label("mod+s")}</Text></MenuItem>
  </Menu>
</View>
```

```rust tabs
on_key("mod+s", move |_| save());
view()
    .child(button().on_press(save).child(text("Save")))
    .child(
        menu()
            .trigger(view().px_3().py_2().child(text("File")))
            .child(menu_item().on_select(save).child(text(format!("Save  {}", key_label("mod+s"))))),
    )
```

Pick the element by its role. Use `Button` for commands, even when it should look quiet. Give it a quiet style
instead of turning it into a link. NSH apps keep the `link` role for targets opened outside the app, such as a URL or
an email address. Use navigation for a place in the app, and `Button` for a command.

Stop propagation (`stopPropagation()` on a touch event) only when a nested control must keep its parent from handling
the same touch. Stopping it everywhere breaks scrolling, presses and gestures in ways that are hard to debug.

**The main thread never waits for app code.** Sometimes the system needs an answer at once: may this page pop, may
this sheet or popover close, may this text change. NSH answers natively from props set beforehand, then sends the app
an event. So a decision is a prop, set as soon as you know it. It is not a callback that returns true or false:
`popOnBack={!dirty()}` with `onBackPress` to confirm ([popOnBack](./navigation.md#keeping-the-user-on-a-page-poponback)),
`closeOnEscape` and `closeOnBackdropPress` with `onCancel` to hear the attempt, `maxLength` on an input, `disabled` on
a button.

A menu item's `shortcut` (`nsh/desktop`) binds its key while the menu is set. The menu shows it in the platform's
style. A `keymap` binding for the same keystroke takes priority. Give each keystroke one owner. Set the application
menu again when its items or shortcuts change.

Make focus ownership explicit:

- `Button`, `Switch` and menu items take keyboard focus on the desktops and the web. Build controls on them, not on
  bare views;
- move focus with a ref's `focus()` from an event or an effect that follows a state change, and move the screen
  reader's with `set_accessibility_focus()` (Rust) or `setAccessibilityFocus` from `nsh` (TSX);
- render a visible `focus-visible:` state;
- do not request focus unconditionally while rendering.

Composite widgets should implement full keyboard navigation, not a few separate shortcuts. That means arrow keys,
Home / End or page keys where they fit, confirm, cancel and Tab.

Overlays close from the top. Escape and the Android back button go to the topmost overlay that listens (`onCancel`).
Handle fast close-and-open sequences without acting on an overlay that is already closing.

## Async work and side effects

Start asynchronous work from an event, an effect or a [`resource`](./reactivity.md#resource), not as an unconditional
side effect of a component body. An effect tracks only what it reads synchronously, so read the signals first and
then start the work. When the work completes, update state through signals. Expect that the component may be gone.
Write related changes together.

Keep recurring work under an owner, so it stops when the owner goes away. Rust tasks and timers end with their owner.
TSX timers stop in `cleanup`. A one-off task that must finish on its own can start from the store, but then nothing
cancels it. Handle errors and failed requests explicitly.

Represent async operations with explicit states: a resource's `state()` is `unresolved`, `pending`, `ready`,
`refreshing` or `errored`. Keep the previous data visible during a refresh: a resource keeps its value while it loads
again. Block duplicate destructive submits. Show recoverable errors in the UI. Logs are not user feedback.

For ordinary, reversible settings (a radio choice, a switch, a stepper), update the local value in the event handler
before you start the save. Bind the visible checked or selected state to that value. Save asynchronously, so network
latency does not slow the control. Keep the last confirmed state and apply pending edits in order:

- an older load or response must not replace a newer choice;
- a failed save rolls back only its own change and keeps later edits;
- a server may accept a setting before it takes effect. Keep these two values apart, and do not replace the accepted
  choice with an old effective value.

Show a useful error on failure. If the action has serious consequences or its result is hard to predict, wait for the
server to confirm instead.

Test the visible control while a deliberately delayed request is pending. Then test failure, rapid changes, and old
or out-of-order responses. A headless test alone does not show touch-to-frame latency. When you can, check or measure
the interaction on the real target platform.

```tsx tabs
const results = resource(
  () => query() || undefined,
  async (q, { signal }) => {
    const response = await fetch(`https://api.example.com/search?q=${encodeURIComponent(q)}`, { signal });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return (await response.json()) as SearchHit[];
  },
);
```

```rust tabs
let results = resource(
    move || {
        let q = query.get();
        (!q.is_empty()).then_some(q)
    },
    |q, _info| async move {
        let response = fetch(format!("https://api.example.com/search?q={q}")).await?;
        if !response.ok() {
            return Err(format!("HTTP {}", response.status()));
        }
        Ok(response.text())
    },
);
```

Move expensive parsing or computation out of TSX handlers: into a `computed`, off the interaction path, or into Rust.
Results can arrive after the query, the page or the selection has changed. A resource drops outdated loads. For your
own async work, attach a revision or an id, and drop stale results instead of applying them to new state.

## Layout, measurement, and scrolling

A view lays out its children in a column, and by default they stretch across it. `class="flex"` (`.flex_row()`) makes
a row, as in CSS. A row of controls usually centers its children; say so with `items-center`. A row of full-height
columns keeps the default stretch. Each column then fills the row's height, and its header stays at the top:

```tsx tabs
<View class="flex flex-1">
  <Sidebar />
  <Content />
</View>
```

```rust tabs
view()
    .flex_row()
    .flex_1()
    .child(sidebar())
    .child(content())
```

Most UI should rely on layout, not measure itself ([Layout](./layout.md)). Measurement (`onLayout`, `measure()`) is for
overlays, virtualization, editors, charts and other components that need the final geometry to work correctly.

- Put measurement and geometry in the layer that owns the behavior.
- Observe layout only when ordinary layout cannot express the relationship.
- Never write unrelated app state on every layout event.
- Treat measured data as revision-scoped; it can become stale after text size, width, theme or content changes.
- Let `nsh/base`'s overlays place popups: their placement, flipping and safe-area margins follow one policy.

Build alignment in rather than correct it later. Sibling regions should use the same spacing token or shared inset,
not repeat the same number. For important repeated edges, columns and gaps, add geometry checks or screenshots on a
real platform (`nsh test --target`). Test more than the default window. A large text size or display scaling can turn
fractional coordinates into a one-pixel drift, even when the default screenshot looks aligned.

When you review precision, measure the result. Do not fix a measured gap with an arbitrary value. Trace it to its
cause, such as duplicated padding, nested insets, which view owns the border, font metrics or rounding. Then fix it
there.

Every scrollable region must have one owner. Avoid nested scrolling on the same axis by accident. Make the scroll
view the element that holds the whole page, panel or list, so its scroll indicator sits at the region's edge. Put the
content's padding inside it (`contentClass` / `contentStyle`), instead of wrapping the scroll view in a padded
container. An indicator floating between the content and the page edge usually means the wrong scroll owner, or
padding on the wrong layer. Keep clear of the safe area and the keyboard with `env(safe-area-inset-*)` and
`env(keyboard-inset-height)` in padding and margin. Read the values with their accessors. They follow rotation and the
keyboard natively.

## Lists, tables, and large data

Use [`List`](./lists.md) (`virtual_list` in Rust) for collections that can grow, such as chat timelines, feeds and
history. Use a scroll view with ordinary children only for small, bounded content. Keep a row's identity separate from
its position on screen. Do not copy the whole data set on every update. The list owns recycling, scrolling and the
visible range. Row functions own how a row looks.

Separate:

- source data and domain ids;
- filtering and sorting state;
- selection state;
- scroll state;
- row rendering.

This keeps updates local and prevents the view tree from becoming the data model.

Virtualization means the list creates views only for visible rows. It changes behavior, not just speed. A row
function runs once per cell, and cells are reused for other rows:

- **you must keep per-item state in data keyed by id.** A signal created inside the row moves with the cell to the
  next item;
- **you must read the item where it is used**, in bindings. A value copied at the top of the row function belongs to
  the first row the cell showed;
- update immutably in TSX (a new array, unchanged items reused, a changed item a new object), and give an
  `estimatedItemSize` close to the typical row;
- scrolling to an item (`scrollToIndex`) works in data coordinates even when most rows do not exist.

```tsx tabs
const liked = signal<Set<number>>(new Set());

<List each={posts()} estimatedItemSize={64} class="flex-1">
  {(post) => (
    <View class="flex-row p-4">
      <Text class="flex-1">{post().title}</Text>
      <Text onPress={() => toggle(post().id)}>{liked().has(post().id) ? "♥" : "♡"}</Text>
    </View>
  )}
</List>
```

```rust tabs
let liked = signal(HashSet::<u32>::new());

virtual_list(move || posts.with(Vec::len), move |at| {
    let post = computed(move || posts.with(|p| p[at.index()].clone()));
    view()
        .flex_row()
        .p_4()
        .child(text(move || post.with(|p| p.title.clone())).flex_1())
        .child(
            text(move || if liked.with(|s| s.contains(&post.with(|p| p.id))) { "♥" } else { "♡" })
                .on_press(move || toggle(post.with(|p| p.id))),
        )
})
.estimated_item_size(px(64.0))
```

## Public API design

For reusable components:

- props establish valid defaults: `#[prop(default)]` or `#[prop(default = …)]` in Rust, `??` where the prop is read in
  TSX;
- every prop, event and export has a doc comment. In Rust, the comment becomes the TypeScript declaration TSX sees;
- callbacks describe requested changes and include pointer details only when they are meaningful;
- explicit parts, slots and render functions are preferable to inspecting arbitrary descendants;
- adding reusable behavior must not force a product-level visual choice.

Apps reach host apps as signed packages and may never be rebuilt. Every later NSH release runs them as they are.
Apply the same care to what an app or component library exposes to others: its components' props and events, its
crate's exports, and the functions that a host app's native components and modules receive:

- inputs only gain optional members. Types only widen. Defaults and behavior stay, and new behavior is opt-in;
- a function whose parameters may grow takes its options as one object (`scrollTo({ x, y, animated })`), never another
  positional parameter. Parameters that will never grow, such as a range's start and end, stay positional;
- a mode is a string union, which can gain values, rather than a boolean that later needs a third state;
- a value has one type: `version: string`, never `string | number`;
- what a function returns is an object, which can gain a field, rather than a tuple or a bare value;
- a function that returns a Promise reports every failure by rejecting it. A failure the caller can handle carries a
  `code`, as NSH's own failures do. Callers branch on codes. Messages are for people and may change;
- a rename keeps the old name as a `@deprecated` alias of the same implementation.

In Rust, keep behavioral state in private fields, so it can change without breaking callers. Public fields suit plain
configuration records and serialized data. Every public struct with public fields must carry `#[non_exhaustive]` and
implement `Default`. Callers can then create values without listing every field, and you can add a field without
breaking them.

Keep public import paths stable when you reorganize the internals. Other code imports a crate's `#[native_module]`
name and its exports, and a TSX module's exports. Prefer the platforms' control names and NSH's existing names over
web-framework terms.

## Platform and capability boundaries

Do not assume every platform has the same features. Title bars and window controls, accessibility, the clipboard,
haptics, scroll physics, fonts and timing can differ. One bundle runs everywhere
([Platform-specific code](./platform-specific.md)). Put platform-specific code where NSH provides a place for it, and
define what happens on other platforms:

- what an element means on every platform is a top-level prop. A feature of one platform goes in `ios={{…}}`,
  `android={{…}}`, `macos={{…}}`, `windows={{…}}`, `desktop={{…}}` or `web={{…}}` (a builder method in Rust), for each
  platform that has it. Only the current platform's object applies;
- APIs of mobile platforms come from `nsh/mobile`, those of the desktops from `nsh/desktop`;
- styles per platform are `ios:`, `android:` and `web:` classes.

A platform branch must keep the same meaning even if it looks different. For example, a dialog is a sheet on iOS and
a layer over the window on the desktops, but the app still needs one consistent open state. Test the shared behavior
headless, and the platform's part with `nsh test --target`. Give every branch on a value NSH reports (`platform.os`, a
state, a reason) a `default`. Later releases add platforms and cases, and an app shipped today runs on them
([Compatibility](./compatibility.md)).

An app lists what it uses outside its VM in `nsh.json` `"capabilities"`. A capability is one such permission: the
hosts it fetches from, the pages a web view shows, the clipboard, the status bar
([Sandbox and Capabilities](./sandbox-and-capabilities.md)). An app delivered into other apps lists exactly what it
uses. A host app may grant less than the app declares, so handle a refused request like a failed one. A refused call
fails with `code` `"ERR_ACCESS_DENIED"`. Its `permission` names the capability, and its `resource` names what was
asked for.

## File and naming conventions

- Name pages and components after product concepts: `ProjectList`, `ProjectEditor`, `SettingsPage`.
- Name event handlers after intent: `confirmDelete`, `openProject`, `onQueryChange`.
- Give each module one main job. Split a file when you can no longer follow its state or lifecycle without reading
  unrelated code.
- Keep a component's module, props, events and focused tests together when they change together.
- Document invariants and surprising lifecycle constraints; do not narrate obvious builder calls.
- Use `rustfmt` and satisfy Clippy in Rust, and `nsh check` in TSX. Avoid broad `allow` attributes that conceal
  unrelated warnings.

### Vocabulary is part of the API

Use the same word for the same concept across components. Before you name a prop or an event, look for the term in
NSH's own API, then in the platforms' controls. Primitive content and global APIs take their names from HTML, the DOM
and CSS (`src`, `html`, `placeholder`). Rust names are `snake_case` and become `camelCase` in TSX (`on_open_change` is
`onOpenChange`). One word stays one word (`readonly`, `multiline`).

| Concept | Naming pattern | Example |
| --- | --- | --- |
| A component | A noun, `PascalCase` | `Button`, `Dialog`, `QuantityStepper` |
| Its props and events (Rust) | `<Component>Props`, `<Component>Event` | `ButtonProps`, `ButtonEvent::Press` → `onPress` |
| A behavior's state object | `<Control>State`, made by `create<Control>` | `NavStackState`, `createNavStack` |
| Why a value changed | `<Control>ChangeReason`, a string union | `DialogChangeReason`: `"cancel"`, `"backdropPress"` |
| A state | An adjective, positive, no `is` prefix | `open`, `hidden`, `disabled`, `selected`, `checked`, `readonly` |
| What the user may do | The adjective | `scrollable`, `zoomable`, `selectable`, `resizable` |
| A feature with no adjective | `<feature>Enabled` | `pagingEnabled` |
| Whether a part shows | A boolean named after the part, default `true` | `scrollbar`, `caret` (`scrollbar={false}`) |
| Whether a slot shows | `<slot>Visible`, default `true` | `headerVisible` (`header` is a page's slot) |
| A part's styles | `<part>Class`, `<part>Style` | `backdropClass`, `contentStyle` |
| A state's styles | `<state>Style`, `<state>:` classes | `activeStyle`, `disabled:` |
| A slot | What it is | `header`, `footer`, `trigger`, `content`, `empty` |
| Content of the app's | `render<Part>` | `renderCodeBlock` |
| A dismissal flag | `closeOn<Trigger>`, default `true` | `closeOnEscape`, `closeOnBackdropPress` |
| A value's change | `on<Value>Change({ <value>, ... })`, one object | `onChange({ value })`, `onOpenChange({ open, reason })` |
| Any other event | `on<Event>`, present tense, one object | `onPress()`, `onSubmit({ text })` |
| A phase | `<act>Start`, `<act>End` | `onPanStart`, `onLoadStart` |
| The mouse over a view | `Enter` / `Move` / `Leave`, the DOM's | `onMouseEnter`, `onMouseMove`, `onMouseLeave` |
| A mouse button's release | `Up`, the DOM's | `onMouseUp` |
| Starting to listen | `on(event, listener)`, returns `() => void` | `host.on(name, l)`, `BackHandler.on("press", l)` |
| A value of the environment | An accessor, `set<Value>` beside it | `colorScheme()`, `setColorScheme` |
| A read-only boolean | `is<Adjective>`, `has<Noun>` | `isAvailable` |

Props are never `is<Adjective>`. That form is for read-only booleans (a constant, a field of a payload). A part that
shows is named after the part, never `<part>Shown`, `<part>Hidden` or `shows<Part>`.

A Rust builder takes and returns `Self`. It is named after the prop it sets, with no `set_` (`.disabled(..)`,
`.checked(..)`, `.on_change(..)`). A method that changes a value in place is `set_<field>` (`setRoutes`,
`setColorScheme`). A boolean reader is `has<Noun>` when the value holds something, or `is<Adjective>` when it describes
a state or a permission.
Reach for the adjective whenever the action has one: `isClosable` over `canClose`, `isZoomable` over `canZoom`. When
the action is a verb phrase with no adjective form, name the thing it needs instead: `hasDefinition`, not
`canGoToDefinition`. Do not add new `can` readers.

### Let the enclosing name carry the context

A name is read inside something: a prop inside its component, a field inside its type. Neither repeats the enclosing
name (`contentStyle`, not `contentContainerStyle`; `Dialog`'s `backdropClass`, not `dialogBackdropClass`). Keep one type's
fields at the same level of abbreviation.

### Use precise domain words

- **selected** means membership or the current item. **checked** and **pressed** are the value of a checkbox or a
  toggle. **focused** is the keyboard's target. **hovered** means the pointer is over it. Never mix them up.
- **open / close** is an overlay's state. **show / hide** is a short-lived request. **expand / collapse** is about
  structure.
- **disabled** blocks interaction. **readonly** allows selecting and navigating but not editing. **loading** prevents
  duplicate work while an operation runs.
- **press** is activation by any input, and NSH's events say press (`onPress`). **click** is a mouse's.
- **index** is a position. **id** is identity. Do not store or key reorderable data by index.
- **value** is controlled domain data. **state** is retained behavior.
- **src** is the resource an element shows (an image, a video, a page); any other address is a **url**.
- **role** says what a choice means (`"cancel"`, `"destructive"`); a look is the app's.

Avoid vague names such as `data`, `item2`, `handleAction`, `updateUi`, `process`, `manager` or `config` when a narrower
domain term exists.

### Type and module style

- Rust types are `UpperCamelCase`; modules, functions, methods, fields and locals `snake_case`; constants
  `SCREAMING_SNAKE_CASE`. Name context bindings `cx`.
- TSX components and their tags are `PascalCase`; props, functions and locals `camelCase`. Primitives come from `nsh`,
  base components from `nsh/base`.
- A module named after a component owns its public API. Split its state, parts and tests into files without changing
  what others import.
- Use singular module names for one component concept and established names for families (`menu` for `Menu`,
  `MenuItem` and `Submenu`).
- Suffix identifiers with `Id`, and collections with meaningful plurals. Do not alternate `idx`, `index` and `ix` in
  one subsystem.
- Name predicates positively when you can. Positive states combine more easily than negatives. Keep established
  terms such as `disabled` where they match how controls work.

### Callback and event wording

- Every event passes one object, even with one field and even for a value change. Its fields use the names of the props
  they report: `onChange({ value })`, `onOpenChange({ open, reason })`, `onLinkPress({ url })`. A later field is then
  just an addition. An API data type is a field, never the whole payload (`onImagePress({ image })`). An event with
  nothing to report passes nothing.
- Name the act, not past tense or a request: `onPress`, `onClose`, `onCancel`, `onBackPress`.
- Name the intent rather than the geometry when the act has a name: `onLoadMore`, not "the end was reached".
- Lifecycle pairs are `onOpen` / `onClose`. Phases are `Start` / `End`, never `Begin`. The mouse over a view is
  `onMouseEnter` / `onMouseMove` / `onMouseLeave` (the DOM's names). A file dragged over it is `Enter` / `Leave`.
- Document whether a callback runs before or after the component's own state changes, and whether it may re-enter the
  component.

### Documentation and copy style

Public docs should start with what a component does and who owns its state. Examples must use current APIs and show
stable ids. Document defaults, platform differences, focus behavior, callback order, and which props the main thread
reads natively.

Follow the interface language rules of the Design Guides for labels, commands, confirmation dialogs, capitalization,
and ellipses. Keep one canonical term for each domain object, command, and state.

Translation keys describe stable intent (`dialog.deleteProject.title`), not a source-language sentence or a screen
coordinate. Never assemble a sentence from translated fragments or reuse one key for meanings that happen to share the
same English text.

Translate meaning, not sentence structure. Let every locale control its word order, plurals, punctuation and how much
context it needs. Tests should catch missing keys and inconsistent fixed terms. A person still decides whether a
repeated word is justified by context. Verify every string inside its component with realistic content, text expansion,
and a large text size.

## Testing strategy

Test at the lowest layer that can prove the behavior ([Testing](./testing-and-trace.md)):

1. pure tests for state transitions, formatting, parsing and ordering (`cargo test` of a crate's functions, a test
   file of the store's functions);
2. headless app tests for components, events and lifetimes: the real engine, renderer, layout and lists, with display
   frames at 60 Hz, in `nsh test` (`nsh/test`) or `cargo test` (`nsh_testing::Headless`);
3. interaction tests for focus, keyboard, pointer, layout and rendered state, with the steps a user takes (`press`,
   `type`, `scroll`, `drag`, `keyboard.press`), headless and on a real platform (`nsh test --target macos`, `ios`,
   `android`, `web`, ...);
4. example or app smoke tests for complete workflows, and `nsh test --trace` for what an interaction costs.

```tsx tabs
import { expect, open, test } from "nsh/test";

test("a tap counts", async () => {
  const app = await open("./App.tsx");
  await app.press("role:button[Tap]");
  expect(app.texts()).toContain("Count: 1");
});
```

```rust tabs
#[test]
fn a_tap_counts() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press("Tap").unwrap();
    app.wait_for("Count: 1").unwrap();
}
```

For an interactive component, test what it promises, not how it is built: activation by press and keyboard,
controlled value changes, disabled behavior, focus movement, event count and order, stable identity, and important
empty or failure states. Stub the network (`network` routes) and control time (a manual clock) instead of waiting on
real ones. If a bug reproduces reliably, add a regression test before you fix it.

Find views the way a user does: by role and name, label or text (`role:button[Save]`, `label:Close`). Use a stable
`testID` where nothing else names a view. Selectors are resolved again at every step, so a step never acts on a stale
snapshot. Use screenshots (`app.screenshot`, on a real platform) for visual facts that selectors cannot express. Use
coordinates only as a last resort. Report automated and manual results separately.

## Performance rules

- Do not write state unconditionally from a binding or an effect that reads it.
- Avoid creating signals, effects, key bindings and expensive data structures inside bindings or row functions.
- Read signals in bindings, so a coherent change updates the narrowest part of the tree.
- Use `List` for long collections and render only the visible range.
- Avoid cloning large strings or collections solely to satisfy a closure; capture `Copy` handles or shared data
  (`SharedString`, `Rc`).
- Keep TSX handlers and computed values light; move heavy work off the interaction path or into Rust.
- Measure with `nsh test --trace` before adding caches. A cache must have a clear invalidation owner.
- Keep animation work bounded: animated values on the UI thread rather than signals written every frame, `scrollValue`
  rather than a scroll listener ([Animation](./animation.md)), and honor reduced motion (`reduceMotion()`).

## Common failure modes

Avoid these patterns:

- one signal containing the app's unrelated state;
- business logic and network requests embedded in a long component body;
- random or index-based keys for reorderable content;
- per-row state in signals created inside a `List` row;
- reading a prop or signal in a component body and expecting it to update;
- literal colors and radii that break custom themes and dark mode;
- custom pressable views where a semantic component already supplies focus, keyboard, disabled, and accessibility
  behavior;
- duplicated local state that drifts from a controlled value;
- effects that copy one signal into another, and writes that loop through a binding;
- a decision the system needs on the spot left to a callback, instead of a prop set beforehand;
- nested scroll views without explicit ownership;
- a new component variant for a one-off screen;
- confirmation dialogs for reversible, low-risk actions;
- tests that call internal functions but never press, type or scroll.

## Rules for coding agents

Before editing, an agent must read the nearest implementation, its tests, the module's public exports and the
relevant reference page. It must look up signatures in the current source and `.nsh/types`. It must not translate a
React, React Native or CSS example by analogy. NSH components run once, and bindings follow the signals they read.

For each change, the agent should be able to name:

1. the behavior owner and presentation owner;
2. the retained identity and state lifecycle;
3. the press, keyboard, focus, and accessibility contract;
4. the layout and scrolling owner;
5. the theme tokens and intentional exceptions;
6. the test that would fail if the behavior regressed.

A person must review and test generated code. "It compiles" is not a UI quality bar. A broad refactor that only makes
generated code look tidy does not replace fitting the app's architecture.

## Implementation checklist

Before opening a change for review, confirm that:

- state and side-effect ownership are explicit;
- a plain function versus a component is chosen deliberately, and props and signals are read where they are used;
- repeated elements have stable domain-based keys and test ids;
- theme tokens and the Tailwind scale replace isolated visual literals;
- presses, keyboard shortcuts, focus, disabled state, and overlays work together;
- decisions the system needs on the spot are props, set beforehand;
- loading, empty, error, and cancellation paths are represented;
- long data sets use `List`, with per-item state in data;
- public API additions only widen, and preserve dependency direction and encapsulation;
- tests prove behavior at the appropriate layer, headless and on the platforms the change touches;
- formatting, Clippy, `nsh check`, targeted tests, and relevant examples pass.

See [Getting started](./getting-started.md) for app setup, and the [Elements](../elements/index.md) and
[API](../modules/index.md) references for current API details.

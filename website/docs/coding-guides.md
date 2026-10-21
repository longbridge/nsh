---
title: Coding Guides
description: "Architecture and coding conventions for Native Shell apps and component libraries: layers, state ownership, controlled components, identity, styling, the main thread, lists, naming, compatibility and testing, in Rust and TSX."
order: 29.5
group: ui
---

# Coding Guides

This guide describes the architecture and code patterns that keep Native Shell apps correct, fast and easy to change.
It is written for developers of NSH apps (TSX modules, Rust components in an app's own crates, native apps that embed
NSH) and for authors of component libraries built on `nsh/base`, and for the coding agents that work with them. Read
the Design Guides first: code structure carries out product intent, it does not replace it.

This is a normative guide. **Must** marks a rule of correctness, lifecycle or compatibility; **should** is the default
architecture, and departing from it needs a concrete reason. The reference pages and the generated declarations in
`.nsh/types` remain authoritative for exact signatures.

## Make the common path inexpensive

**Design principle: write ordinary UI so that it is inexpensive by default.** NSH gives every app the same fast path:
a component runs once, and afterwards only the bindings that read a changed signal run again; the reactive graph, the
renderer, layout, lists and every module an app imports are native; one turn of the app's code becomes one batch of
native operations. App code keeps that path when it reads signals where it uses them, keeps state in the narrowest
owner and leaves heavy work out of handlers. Measure before adding caches ([Performance](./performance.md)); the later
sections explain the mechanics.

## Architecture at a glance

An NSH app is built in layers, as GPUI Kit's is ([Three layers](./architecture.md#three-layers)). Dependencies point
downward: higher layers own domain meaning and orchestration, lower layers own reusable behavior and presentation.

| Layer | Where it lives | Owns |
| --- | --- | --- |
| App shell | The entry (`mount(App)`, or `nsh.json` `"root"`) | The root, the navigation stacks, the features it composes |
| Feature | A directory of TSX modules, or a crate of the app's | One capability's state, services, screens, dialogs and workflow |
| App component | The app's component library | A repeated, domain-aware pattern with the product's look |
| Component library | The app's, or one from the community | Styled, general-purpose components built on `nsh/base` |
| `nsh/base` (`nsh_base`) | Native Shell | Unstyled behavior: navigation, dialogs, a press target, safe areas, the keyboard, the theme's tokens |
| `nsh` (`nsh_core::ui::prelude`) | Native Shell | The primitives, styles, layout, reactivity, the environment |

A reusable component never depends on an app screen, and a component library never depends on a feature. `nsh/base`
draws nothing of its own: the look is always the app's or its library's.

### Organize large apps by capability

Keep one capability's state, services, screens, dialogs and commands together, behind one public boundary. A dialog
that renames a workspace belongs to the workspace feature; only the reusable dialog behavior belongs to the component
library. A small capability can stay one module until a boundary has a concrete benefit.

```text
App.tsx                      # mount(App): composes the features
features/
├── workspace/
│   ├── index.ts             # The feature's public boundary
│   ├── store.ts             # Its signals and the functions that change them
│   ├── WorkspaceScreen.tsx
│   └── RenameDialog.tsx
├── search/
│   ├── index.ts
│   ├── store.ts
│   ├── SearchScreen.tsx
│   └── filters.ts
└── settings/
    ├── index.ts
    └── SettingsScreen.tsx
ui/                          # The app's component library
├── Switch.tsx
└── Card.tsx
native/                      # Rust crates, listed in nsh.json "rust"
├── charts/                  # #[native_module("app/charts")]
└── ledger/                  # #[native_module("app/ledger")]
```

Do not invert this into global `components/`, `screens/`, `dialogs/` and `stores/` directories: they sort files by
their role and scatter every feature across the app. In Rust, a capability that has its own state, lifecycle and
public boundary merits a crate of its own ([Several crates](./rust.md#several-crates)); crates share code through path
dependencies, and only the crates listed in `"rust"` export to TSX.

Across a feature boundary, expose a deliberately public function, component, domain id or event, and keep the
feature's signals and modules private. Features depend on shared contracts below them, never on the shell or a
sibling's private modules; when one must call another, make the dependency explicit and one-way. These boundaries let
several engineers or agents work on separate capabilities in parallel with fewer conflicts.

## Bootstrap and root ownership

An app has one root. A TSX entry calls `mount(App)` once; a Rust app names its root component in `nsh.json`
(`"root": "App"`). The root fills the surface: give it `flex: 1` and a background from the theme.

```rust tabs
#[component]
pub fn App() -> impl IntoElement {
    view()
        .flex_1()
        .bg(move || theme().background())
        .child(home())
}
```

```tsx tabs
import { mount, View } from "nsh";

mount(() => (
  <View class="flex-1 bg-background">
    <App />
  </View>
));
```

One surface is one engine: each `NshAppView`, `NshView` or `AppView` a native app embeds runs its own VM, its own
module state and its own reactive graph. Code written for NSH keeps its state in the app's modules and components,
so several surfaces in one host app never share it by accident. A navigation flow has one `NavStack` and its state;
a `Dialog` presents over everything else, the stack's header included, so a dialog belongs to the feature that opens
it, not to a page's layout.

## What runs once and what runs again

NSH's model is Solid's, on a native reactive graph ([Reactivity](./reactivity.md)). Keep the distinction explicit:

| You write | Runs | Runs again when |
| --- | --- | --- |
| A component body | Once, when it is created | Never |
| A binding: a closure (Rust) or an expression that calls or reads a property (TSX) where a value goes | Once | A signal it read changes; only that prop or child updates |
| `computed(fn)` | When created | A signal it read changes |
| `effect(fn)` | After the elements of the pass exist | A signal it read changes |
| A row of `each` / `<Each>` | When its key first appears | Never; its item and index signals change |
| A cell of `virtual_list` / `<List>` | Once per cell | Never; the cell is given other rows |

- **Must read signals and props where they are used.** A value read in the body is read once; `const { title } = props`
  reads `title` once, and so does a parameter written as `({ title })`. Read `props.title` in the JSX, or use
  [`split`](./jsx-and-components.md#spreading-props) to pass the rest on.
- **Must create effects, computed values, timers and listeners while a component runs.** They belong to the
  component and go away with it. Created in an event handler or a timer callback, they have no owner and live forever.
- **Must not expect a component to render again.** A component builds its part of the tree once; what follows changes is a binding.

## Choose the right unit

### A function, a component, or a class

- **A plain function** returning elements (`fn row(..) -> impl IntoElement`, a TSX function component used only
  locally) is the default for a named region of UI. Its arguments are its props.
- **A `#[component]`** is a Rust component with typed props (`#[derive(Props)]`) and events (`#[derive(Event)]`). Write
  one when the UI has props or events of its own, when it is part of a component library, or when TSX imports it: its
  TypeScript face and declarations are generated from the structs.
- **A `#[class]`** keeps retained state in Rust that TSX uses with `new` (a ledger, a parser, a session), and a
  `#[module]` exports functions ([Building with Rust](./rust.md#extending-a-rust-app-with-tsx)).

Do not turn every visual fragment into a component: a component boundary is a contract, and it should name one.

### Rust or TSX

Rust components are native code from start to end; TSX runs its own code in QuickJS, an interpreter, while everything
it calls is native ([Performance](./performance.md#1-js-does-as-little-as-possible)). Write the parts an app uses most,
component libraries and demanding business logic in Rust; keep TSX for what changes often or is delivered at run time.
Both faces of one component share one definition: a component written in Rust is placed with `switch()` in Rust and
`<Switch>` in TSX, with the same props and events.

## State ownership

Put each state in the narrowest owner that can keep it correct:

- transient UI state (a field's draft, whether a disclosure is open) is a signal in the component that renders it;
- state a subtree shares is a signal provided through [context](./reactivity.md#context);
- a feature's domain state lives in the feature's store module (TSX) or its crate (Rust), changed through named
  functions;
- what outlives a run belongs in [storage](../modules/nsh-storage.md) or on a server;
- behavior a reusable component owns stays inside it; its value does not (below).

### Controlled values

**Components are controlled: the app owns the value, the component reports a requested change.** Pass the current
value in, receive the change, update the owner, and the bindings follow. A callback reports intent; it never creates a
second hidden source of truth. Every `nsh/base` component works this way (`Dialog`'s `open` and `onOpenChange`,
`Button`'s `selected`), and a component library on it should too.

A switch of an app's component library, written once in Rust and placed from Rust and TSX:

```rust tabs
use nsh_base::theme;
use nsh_core::{AccessibilityState, Checked, ui::prelude::*};

/// The props of [`Switch`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct SwitchProps {
    /// On or off: the app owns the value.
    #[prop(default)]
    pub checked: Prop<bool>,
    /// No presses; its disabled style applies.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// What screen readers say.
    pub accessibility_label: Prop<Option<SharedString>>,
}

/// What a [`Switch`] reports.
#[derive(Debug, Clone, PartialEq, Event)]
pub enum SwitchEvent {
    /// The user asked for the other value; the owner sets `checked`.
    Change(bool),
}

/// An on / off switch drawn with the app's theme.
#[component]
pub fn Switch(props: SwitchProps, cx: Scope<SwitchEvent>) -> impl IntoElement {
    view()
        .w(px(48.0))
        .h(px(28.0))
        .p(px(2.0))
        .rounded_full()
        .bg(move || if props.checked.get() { theme().primary() } else { theme().muted() })
        .disabled(props.disabled)
        .disabled_style(|s| s.opacity(0.5))
        .role(AccessibilityRole::Switch)
        .label(move || props.accessibility_label.get().unwrap_or_default())
        .accessibility_state(move || AccessibilityState {
            checked: Some(if props.checked.get() { Checked::True } else { Checked::False }),
            ..AccessibilityState::default()
        })
        .on_press(move || cx.emit(SwitchEvent::Change(!props.checked.get_untracked())))
        .child(
            view()
                .w(px(24.0))
                .h(px(24.0))
                .rounded_full()
                .bg(move || theme().background())
                .ml(move || if props.checked.get() { px(20.0) } else { px(0.0) }),
        )
}

// The owner:
let wifi = signal(true);
switch()
    .checked(wifi)
    .accessibility_label("Wi-Fi")
    .on_change(move |on: &bool| wifi.set(*on))
```

```tsx tabs
import { signal } from "nsh";
import { Switch } from "app/ui";   // the Rust component, from the crate's #[native_module("app/ui")]

function WifiRow() {
  const wifi = signal(true);
  return <Switch checked={wifi()} accessibilityLabel="Wi-Fi" onChange={(on) => wifi.set(on)} />;
}
```

The same contract in a TSX component library:

```tsx
import { View } from "nsh";

export interface SwitchProps {
  /** On or off: the app owns the value. */
  checked: boolean;
  /** No presses; `disabled:` styles apply. */
  disabled?: boolean;
  /** What screen readers say. */
  accessibilityLabel?: string;
  /** The user asked for the other value; the owner sets `checked`. */
  onChange?: (checked: boolean) => void;
}

export function Switch(props: SwitchProps) {
  return (
    <View
      role="switch"
      accessibilityLabel={props.accessibilityLabel}
      accessibilityState={{ checked: props.checked }}
      disabled={props.disabled}
      onPress={() => props.onChange?.(!props.checked)}
      class="h-7 w-12 rounded-full p-0.5 disabled:opacity-50"
      classList={{ "bg-primary": props.checked, "bg-muted": !props.checked }}
    >
      <View class="size-6 rounded-full bg-background" classList={{ "ml-5": props.checked }} />
    </View>
  );
}
```

Write a batch of related changes together: several fields that form one invariant are set in one
[`batch`](./reactivity.md#batch), so effects and computed values see them at once.

### Derive, do not synchronize

A value computed from other state is a `computed` (or a plain closure for a cheap value read once), never an effect
that writes a second signal. An effect that keeps `b` in sync with `a` leaves `b` stale between the write and the
effect, costs a second pass, and an effect that writes what it reads runs itself again until NSH stops it with an
error. Effects are for side effects: requests, timers, imperative calls on refs, logging.

### Avoid feedback loops

Inputs, selections, filters and dialogs have two paths: the owner sets the value, and the user asks for a new one. A
component must not report an owner-supplied value back through its change callback, and it reports each logical
change once. Read the current value untracked when handling the user's act (`props.checked.get_untracked()` in Rust, as
above); in TSX an event handler is already untracked. Make callbacks safe to re-enter: a change handler may
synchronously close, replace or remove the component that called it.

### Keep lifetimes bounded

- What a component starts ends with it. In Rust, tasks, timers and intervals (`spawn`, `set_timeout`,
  `set_interval`) end with the owner they were made under; in TSX, register the stop with
  [`cleanup`](./reactivity.md#cleanup) or return it from the effect.
- A function that starts listening returns a function that stops it (`() => void`); what is registered while a
  component renders goes away with the component, as `keymap.bind` does.
- A [`resource`](./reactivity.md#resource) keeps only the latest load: a new one drops the one running, and so does
  the component going away. In TSX, pass `info.signal` to `fetch` so the dropped request is aborted.

## Stable identity

A key is behavior. It decides which row keeps its state, its focus and its measurements when data changes.

- Key rows, pages and repeated controls by stable domain ids: `each(projects, |p| p.id, ..)`,
  `<Each of={projects()} key={(p) => p.id}>`, a list's `keyExtractor`.
- Navigation routes are values: a route that stays in `setRoutes` keeps its page and its state.
- Never key by a translated label, or by an index when items can be inserted or reordered.
- Never make a fresh random id or key while rendering: a changed key is a new identity, and its state starts over.
- `testID`s name what a test finds; keep them stable and derived from the domain (`delete-${project.id}`).

```rust tabs
each(projects, |p| p.id, |project, _index| {
    text(move || project.get().name)
})
```

```tsx tabs
<Each of={projects()} key={(p) => p.id}>
  {(project) => <Text testID={`project-${project().id}`}>{project().name}</Text>}
</Each>
```

## Rendering and composition

Keep a component declarative: read state, derive presentation values, compose elements. Move domain operations,
parsing and requests to named functions of the feature. Extract a helper when it names a meaningful region and
reduces what a reader holds at once; extract a component when the region has its own reusable contract, not merely
because a builder chain or a JSX block is long.

- Use `when` / `<If>` for branches of more than a few elements: a ternary in a closure builds its branch again
  whenever anything it reads changes.
- Use the semantic component before building a custom surface: `Button` from `nsh/base` for commands (presses, the
  disabled and selected states, the button role for screen readers), `Dialog` for modal surfaces, `NavStack` for
  navigation. A `View` with `onPress` carries the press and leaves the role and states to the app.
- Render callbacks the app passes (a list's row, a `TextView`'s `renderCodeBlock`, a component's render function)
  are free of side effects. They run whenever the owner needs a cell or a part: they never perform a business
  operation, append data or start a subscription.
- Read `props.children` once, where the children go: each read builds them again.

## Behavior and presentation boundary

The durable rule of `nsh/base`, and of any component library built on it:

> The base layer owns reusable behavior and the structure that behavior needs. The presentation layer owns the
> product's visual language.

"Unstyled" means the base owns no colors, sizes, radii, fonts, variants or strings, while it keeps the structure its
behavior needs. Navigation, dialogs, the keyboard, safe areas, selection and text documents need structure and state inside, and
moving that work to every caller would duplicate fragile behavior.

A component exposes its presentation explicitly:

- its own `class` / `style` style its main part, and every other part gets `<part>Class` / `<part>Style`
  (`Dialog`'s `backdropClass`);
- states take `<state>:` classes and `<state>Style` objects (`active`, `hover`, `focus`, `disabled`, `selected`);
- slots are named after what they are (`header`, `footer`, `trigger`, `content`, `empty`, `separator`);
- content of the app's for values the component has is a render function (`renderCodeBlock`; in Rust a
  `Renderer<T>` prop, [render functions](./rust.md#render-functions-and-the-apps-classes)).

Do not inspect arbitrary descendants to find a title, a description or a close button: make the part explicit. Strings
are always the app's: a component takes every label it shows as a prop or a child.

## Theme and styling

Read colors and numbers from the theme and apply layout with Tailwind's scale ([Tailwind](./tailwind.md),
[theme tokens](./tailwind.md#theme-tokens)):

```rust tabs
view()
    .rounded(move || theme().radius_lg())
    .border_1()
    .border_color(move || theme().border())
    .bg(move || theme().background())
    .p_4()
    .child(text("Balance").text_color(move || theme().foreground()))
```

```tsx tabs
<View class="rounded-(--radius-lg) border border-border bg-background p-4">
  <Text class="text-foreground">Balance</Text>
</View>
```

- **Must not hard-code product colors** in app components: read a token (`theme().primary()`, `bg-primary`) or add the
  role to the app's theme file. A token follows the appearance and an embedding app's theme natively.
- Use the spacing and type scales (`p-4`, `gap-3`, `text-sm`; `.p_4()`, `.gap_3()`, `.text_sm()`) rather than
  arbitrary point values; keep arbitrary values for measured, physical or platform geometry, and say why next to them.
- Classes mean exactly what they mean in Tailwind CSS v4, and CSS values what they mean in a browser. Treat an
  `unknown class` warning as a finding to fix, never as a class with an NSH-specific meaning.
- Press feedback is the app's: `active:` classes, `activeStyle`, `.active(|s| ..)` in Rust; the same for `hover`,
  `focus`, `disabled` and `selected`. These apply natively, with no round trip to app code.
- Keep variants few and semantic (primary, secondary, danger), not one per call site; use semantic tokens for meaning,
  not palette positions.
- Text follows the user's text size: leave text containers free to grow, and test with a large font scale.
- In Rust, styles are typed methods; the app's Tailwind classes reach a component's part through a prop of classes and
  `.app_class(..)`.

## Events, focus and the main thread

**The main thread never waits for app code.** When the system needs an answer on the spot (may this page pop, may
this sheet close, may this text change), NSH answers natively from props mounted beforehand, and an event tells the app
afterwards. So a decision is a prop, set as soon as it is known, not a callback that returns true or false:

- `popOnBack={!dirty()}` with `onBackPress` to confirm, rather than vetoing a back press ([popOnBack](./navigation.md#keeping-the-user-on-a-page-poponback));
- `closeOnEscape`, `closeOnBackdropPress` and `closeOnSwipe` on a `Dialog`, with `onCancel` to hear the attempt;
- `maxLength` on an input, `pointerEvents` on a view, `disabled` on a button.

```rust tabs
nav_page()
    .title("Edit")
    .pop_on_back(move || draft.with(String::is_empty))
    .on_back_press(move || {
        if !draft.with(String::is_empty) {
            confirm();
        }
    })
```

```tsx tabs
<NavPage title="Edit" popOnBack={!dirty()} onBackPress={() => dirty() && confirm()}>
  …
</NavPage>
```

Model one command once. A button, a menu item of `nsh/desktop` and a keyboard shortcut (`keymap.bind("mod+s", save)`)
call the same function of the feature, so their entry points cannot disagree; derive their labels and their disabled
state from one place.

Use presses for activation: `onPress`, `onLongPress`, `onPressDown` and `onPressUp` mean the same with a finger, a pen,
a mouse or the keyboard. Keep raw touch handlers and gestures for what follows the finger (a pan, a drag). Make focus
explicit: move it with a ref's `focus()` from an event or an effect that follows a state change, never unconditionally
while rendering, and give focused controls a visible `focus:` style.

## Async work and side effects

Start asynchronous work from an event, an effect or a [`resource`](./reactivity.md#resource), never as an
unconditional side effect of a component body. An effect tracks only what it reads synchronously: read the signals
first, then start the work.

- Represent loads with explicit states: a resource's `state()` is `unresolved`, `pending`, `ready`, `refreshing` or
  `errored`. Show the previous value while refreshing.
- Results can arrive after the query, the page or the selection changed. A resource drops superseded loads; work of
  your own carries an identity or a revision and is rejected when it is stale.
- Surface recoverable errors in the UI; logs are not user feedback.
- Prevent duplicate destructive submissions: disable the button while the request runs.
- Heavy work in TSX (parsing a large payload, sorting thousands of items on every keystroke) belongs in a `computed`,
  off the interaction path, or in Rust.

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

## Layout, measurement and scrolling

Most UI uses layout rather than measuring itself ([Layout](./layout.md)). Measurement (`measure()`, `onLayout`) is
for popups, overlays and components whose correctness depends on resolved geometry; treat what it returns as valid
until the width, the text size or the content changes.

- A view lays out its children in a column; `class="flex"` is a row, as in CSS. Say the direction where it matters.
- Every scrollable region has one owner. Avoid nested scroll views on the same axis; give the scroll view the whole
  region and put the content's padding inside it, so its indicator sits at the region's edge.
- For alignment, build rather than correct: sibling regions share one spacing token instead of repeating equal
  numbers, and a mismatch is traced to duplicated padding, nested insets or border ownership, not nudged with an
  arbitrary value.
- Read the safe area and the keyboard through `SafeAreaView`, `KeyboardAvoidingView` and their accessors, which follow
  rotation and the keyboard natively.

## Lists and large data

Use [`List`](./lists.md) (`virtual_list` in Rust) when data can grow beyond a small, bounded collection. It creates
views only for the rows on screen and recycles them, so its contract is:

- **Must keep per-item state in data keyed by id**, not in a signal created inside the row: a cell travels to other
  rows. "Expanded", "selected" or "liked" is a set of ids, or a field of the item.
- **Must read the item where it is used**, in bindings: a value copied at the top of the row function belongs to the
  first row the cell showed.
- Update immutably in TSX: a new array, unchanged items reused, a changed item a new object.
- Give an `estimatedItemSize` close to the typical row.
- Keep source data, filtering and sorting, selection, scroll position and row rendering apart, so an update stays
  local and the view tree never becomes the data model.

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

## Platform and capability boundaries

One bundle runs on every platform ([Platform-specific code](./platform-specific.md)). Most code needs no platform
checks; when something belongs to one platform, put it where NSH gives it a place:

- a prop two platforms have is a top-level prop; a prop of one platform goes in `ios={{…}}`, `android={{…}}`,
  `desktop={{…}}` or `web={{…}}` (a builder method in Rust), and only the current platform's object is applied;
- APIs of one platform come from `nsh/ios`, `nsh/android` and `nsh/desktop`;
- styles per platform are `ios:`, `android:` and `web:` classes.

A platform branch keeps the semantic contract even when the presentation differs: the app still knows whether a
dialog is open or a request went out. Keep platform checks for conventions users rely on (Android's back button, iOS
sheets, Android's ripple).

An app declares what it reaches outside its VM in `nsh.json` `"capabilities"`: the hosts it fetches from, the pages a
web view shows, the clipboard, the status bar ([Sandbox and Capabilities](./sandbox-and-capabilities.md)). An app
that will be delivered into other apps lists exactly what it uses, and handles a refused request as it handles a
failed one: a host app may grant less than the app declares. Files and processes are the standard modules
(`fs/promises`, `process`), under the same checks.

## Compatibility: apps delivered as plugins

NSH apps are delivered to host apps as signed packages and may never be built again: every later release of NSH runs
them as they are. Apply the same discipline to what an app or a component library exposes to others: the props and
events of its components, the exports of its Rust crates, the functions a host app's native components and modules
receive.

- Inputs gain optional members only; types only widen; defaults and behavior stay (new behavior is opt-in).
- Options are one object (`scrollTo({ x, y, animated })`), never a third positional parameter.
- A mode is a string union, which can gain values, rather than a boolean that later needs a third state.
- A value has one type: `version: string`, never `string | number`.
- What a function returns is an object, which can gain a field, rather than a tuple or a bare value.
- A rename keeps the old name as a `@deprecated` alias of the same implementation.

In Rust, a public struct of a library carries `#[non_exhaustive]` and implements `Default`, so a field can be added
without breaking its callers.

## Public API design

For reusable components:

- props establish valid defaults: `#[prop(default)]` or `#[prop(default = …)]` in Rust, `??` where the prop is read in
  TSX;
- every prop, event and export has a doc comment; in Rust, it becomes the TypeScript declaration TSX sees;
- callbacks describe a requested change or a completed act, in the app's terms; pointer details are passed only when
  they mean something to the app;
- explicit parts, slots and render functions are preferable to inspecting children;
- adding behavior never forces a visual choice on the app.

### Vocabulary is part of the API

Use the same word for the same concept across components. Before naming a prop or an event, look for the term in
NSH's own API, then in GPUI Kit's `gpui-base`, GPUI and `gpui-component`, then in the platforms' controls. What a
primitive shows and the global APIs take their names from HTML, the DOM and CSS (`src`, `html`, `placeholder`). Rust
names are `snake_case` and translate to `camelCase` in TSX (`on_open_change` is `onOpenChange`); one word stays one
word (`readonly`, `multiline`).

| Concept | Naming pattern | Example |
| --- | --- | --- |
| A component | A noun, `PascalCase` | `Button`, `Dialog`, `Switch` |
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
| A value's change | `on<Value>Change(next, details?)` | `onChange(text)`, `onOpenChange(open, reason)` |
| Any other event | `on<Event>`, present tense, one object | `onPress()`, `onSubmit({ text })` |
| A phase | `<act>Start`, `<act>End` | `onPanStart`, `onLoadStart` |
| The pointer over a view | `In` / `Out` | `onHoverIn`, `onHoverOut` |
| Starting to listen | Returns `() => void` | `keymap.bind(..)` |
| A value of the environment | An accessor, `set<Value>` beside it | `colorScheme()`, `setColorScheme` |
| A read-only boolean | `is<Adjective>`, `has<Noun>` | `isAvailable` |

Props are never `is<Adjective>`: that form is for what only reads a boolean (a constant, a field of a payload). A part
that shows is named after the part, never `<part>Shown`, `<part>Hidden` or `shows<Part>`.

### Use precise domain words

- **selected** is membership or the current item; **checked** and **pressed** are the value of a checkbox or a toggle;
  **focused** is the keyboard's target; **hovered** is the pointer's presence. Never use them interchangeably.
- **open / close** is an overlay's state; **show / hide** a transient request; **expand / collapse** structure.
- **disabled** blocks interaction; **readonly** allows selecting and navigating but not editing; **loading** prevents
  duplicate work while an operation runs.
- **press** is activation by any input, and NSH's events say press (`onPress`); **click** is a mouse's.
- **index** is a position; **id** is identity. Do not persist or key reorderable data by index.
- **value** is controlled domain data; **state** is retained behavior.
- **src** is the resource an element shows (an image, a video, a page); any other address is a **url**.
- **role** says what a choice means (`"cancel"`, `"destructive"`); a look is the app's.

Avoid vague names such as `data`, `item2`, `handleAction`, `updateUi`, `process`, `manager` or `config` when a narrower
domain term exists.

### Callback and event wording

- A value's change passes the value first and anything else after it: `onChange(next)`, `onOpenChange(open, reason)`.
  Every other event passes one object, even with one field (`onLinkPress({ url })`), so a later field is an addition;
  an event with nothing to report passes nothing.
- Name the act, not past tense or a request: `onPress`, `onClose`, `onCancel`, `onBackPress`.
- Name the intent rather than the geometry when the act has a name: `onLoadMore`, not "the end was reached".
- Lifecycle pairs are `onOpen` / `onClose`; phases are `Start` / `End`, never `Begin`; the pointer over a view is
  `onHoverIn` / `onHoverOut`, a file dragged over it `Enter` / `Leave`.
- Document whether a callback runs before or after the component's own state changes, and whether it may re-enter the
  component.

### Let the enclosing name carry the context

A name is read inside something: a prop inside its component, a field inside its type. Neither repeats what encloses
it (`contentStyle`, not `contentContainerStyle`; `Dialog`'s `backdropClass`, not `dialogBackdropClass`). Keep one type's
fields at the same level of abbreviation.

### Type and module style

- Rust types are `UpperCamelCase`; modules, functions, fields and locals `snake_case`; constants
  `SCREAMING_SNAKE_CASE`. Name context bindings `cx`.
- TSX components and their tags are `PascalCase`; props, functions and locals `camelCase`. Primitives come from `nsh`,
  base components from `nsh/base`.
- Name components after product concepts (`ProjectList`, `ProjectEditor`) and handlers after intent
  (`confirmDelete`, `openProject`).
- Keep a component's module, its props, its events and its tests together when they change together.

### Documentation and copy

Public docs begin with what a component does and who owns its state. Examples use current APIs and stable ids, and
document defaults, platform differences, focus behavior and callback order. Follow the Design Guides
for labels, commands and confirmations, and keep one term for each domain object, command and state.

Translation keys describe stable intent (`dialog.deleteProject.title`), not a source sentence or a place on screen.
Never assemble a sentence from translated fragments; give every locale its own word order, plurals and punctuation, and
check strings in their component with realistic content and a large text size.

## Testing strategy

Test at the lowest layer that proves the behavior ([Testing](./testing-and-trace.md)):

1. plain tests for state transitions, formatting, parsing and ordering (`cargo test` of a crate's functions, a test
   file of a TSX module's functions);
2. headless app tests: the real engine, renderer, layout and lists with display frames at 60 Hz, steps a user takes
   (`press`, `type`, `scroll`, `drag`) and checks of what is on screen, in `nsh test` (`nsh/test`) or in `cargo test`
   (`nsh_testing::Headless`);
3. the same tests on a real platform (`nsh test --target macos`, `ios`, `android`, `web`, ...), where the steps are
   the platform's own input and `app.screenshot` records what the platform drew;
4. `nsh test --trace` for what an interaction costs: mount items and timings per commit.

```rust tabs
#[test]
fn a_tap_counts() {
    let mut app = Headless::start(EngineConfig::new(Root::of::<my_app::App>())).unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press("Tap").unwrap();
    app.wait_for("Count: 1").unwrap();
}
```

```tsx tabs
import { expect, open, test } from "nsh/test";

test("a tap counts", async () => {
  const app = await open("./App.tsx");
  await app.press("role:button[Tap]");
  expect(app.texts()).toContain("Count: 1");
});
```

For an interactive component, cover the contract rather than its internals: activation, controlled value changes,
disabled behavior, focus, the count and order of events, stable identity, and the empty, loading and failure states.
Find views as a user does, by role and name, label or text (`role:button[Save]`, `label:Close`), and by a stable
`testID` where nothing else names them. Stub the network (`network` routes) and control time (a manual clock) instead
of waiting on real ones. Add a regression test before fixing a bug whenever the failure reproduces.

## Performance rules

- Keep handlers and computed values light in TSX; move heavy work to a `computed`, off the interaction path, or to
  Rust.
- Read signals in bindings, so a change updates one prop or one child.
- Never write a signal unconditionally from a binding or an effect that reads it.
- Use `List` for long collections, with per-item state in data.
- Animate with animated values on the UI thread, not signals written every frame; follow scrolling with
  `scrollValue`, not a scroll listener ([Animation](./animation.md)).
- Avoid cloning large strings or collections to satisfy a closure; share them (`Rc`, `SharedString`) or read them
  through a signal.
- Measure with `nsh test --trace` before adding a cache, and give every cache an owner that invalidates it.
- Honor reduced motion (`reduceMotion()`).

## Common failure modes

Avoid these patterns:

- reading a prop or signal in a component body and expecting it to update;
- one store holding the whole app's unrelated state;
- requests and business logic inside a long component body;
- index or random keys for reorderable rows;
- per-row state in signals created inside a `List` row;
- literal colors and radii that break the app's theme and dark mode;
- a `View` with `onPress` where `Button` already supplies the role, the states and their accessibility;
- local state that drifts from a controlled value;
- an effect that copies one signal into another;
- a decision the system needs on the spot left to a callback, instead of a prop set beforehand;
- nested scroll views on one axis without a clear owner;
- a new variant for a one-off screen;
- confirmation dialogs for reversible, low-risk actions;
- tests that call a module's internals but never press, type or scroll.

## Rules for coding agents

Before editing, an agent reads the nearest implementation, its tests, the module's public boundary and the relevant
reference page, and checks signatures in the code and in `.nsh/types` instead of translating a React, React Native or
web example by analogy. NSH's TSX follows Solid's model: components run once, and bindings follow the signals they read.

For each change, the agent can name:

1. the behavior owner and the presentation owner;
2. the state's owner and lifetime, and the keys that give rows their identity;
3. the press, keyboard, focus and accessibility contract;
4. the layout and scrolling owner;
5. the theme tokens used, and every intentional exception;
6. the test that would fail if the behavior regressed.

Generated code is reviewed and tested by a person. "It builds" is not a UI quality bar, and a broad refactor that only
makes generated code look tidy does not replace matching the app's architecture.

## Implementation checklist

Before opening a change for review, confirm that:

- state, side effects and their lifetimes have explicit owners;
- props and signals are read where they are used;
- components are controlled: the app owns each value, the component reports changes;
- repeated elements have stable, domain-based keys and test ids;
- theme tokens and the Tailwind scale replace isolated visual literals;
- decisions the system needs on the spot are props, set beforehand;
- presses, keyboard, focus, disabled states and dialogs work together;
- loading, empty, error and cancelled paths are represented;
- long collections use `List`, with per-item state in data;
- platform-only code sits in platform props and modules, and capabilities list what the app reaches;
- public API additions only widen, and keep the layers' dependency direction;
- tests prove the behavior at the right layer, headless and on the platforms it changes.

See [Getting started](./getting-started.md) to set up an app, and the [Elements](../elements/index.md) and
[API](../modules/index.md) references for each component and module.

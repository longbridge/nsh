# nsh-ui-mobile

The components and modules of `nsh/ui/mobile`: buttons, cards, lists, safe areas, the keyboard, the status bar,
system dialogs and the like, written in Rust.
One implementation draws the same on iOS and Android: a component renders NSH's elements with typed styles and theme
colors, the core lays it out in the same pass as everything else, and its interaction (press feedback, drags, state)
stays native. JS only gets the events the component emits.

- **One file per component** (`src/button.rs`), registered in `registry()` (`src/lib.rs`). Start with
  `use nsh_core::ui::prelude::*;`. The JS export is the type's name (`pub struct Button` is `Button`): no name is
  written by hand, and `the_exports_are_pinned` (`src/lib.rs`) lists them all.
- **Props are a struct** (`#[derive(Clone, PartialEq, Props)]`), **events an enum** (`#[derive(Event)]`, or `NoEvent`),
  choices an enum (`#[derive(PropEnum)]`). The bundler generates the `nsh/ui/mobile` export and its TypeScript
  declarations from them, doc comments included, so apps write `<Button title="Save" onPress={save} />` with full
  types; there is no hand-written glue. The props struct's doc comment documents the component in TypeScript.
- **`render(&self, props, cx) -> impl IntoElement`** builds the tree with `view()`, `text()`, `image()` and other
  components (`Card::element(props)`), styled with GPUI-style methods (`.px_4()`, `.rounded(px(theme.radius))`,
  `.bg(theme.primary)`, `.active(|s| s.bg(theme.primary_active))`). It runs again when a prop changes or a listener
  calls `cx.notify()`; keep it cheap and free of side effects. The root must be a `view()`: it is the component's host,
  so the app's `class` (merged last, it wins) and `style` apply to it.
- **Colors come from the theme** (`cx.theme()`: gpui-kit's semantic fields, a light and a dark value each), never
  literals: apps restyle every component with their `nsh.json` theme, and dark appearance switches natively.
- **What the render knows is plain Rust**: `.when(props.disabled, |v| v.opacity(0.5))`, `match props.variant`.
  Interaction states that change without a render are variants: `.active(..)`, `.hover(..)`, `.focus(..)`.
- **The app's content is props, placed by closures**; the component lays it out, never creates or changes it:
  `header: Option<Slot>` (`.slot(props.header, |h| h.px_4())`), `children: Children` (`.child(props.children)`, or
  `.children_each(props.children, |child, at| ..)` to wrap each), `items: Items` for virtualized rows
  (`.list(props.items, |l| l.estimated_item_size(48.0), |row, at| ..)`; in JS `each` and a row renderer, like `List`).
  `at` tells the index, the count, first and last. `#[prop(lazy)]` on `children` has JS make them only while the
  render places them (`Sheet`: fresh content on each opening).
  `Group` and `ListView` are the examples. A Rust component fills them the same way: `.child(..)`, `.fill("header",
  ..)`, and `ListView::element(props).items(virtual_list(n, row).header(..))` for the rows.
- **State** lives in the component's fields, set up from the first props in `setup` and adjusted in `props_changed`;
  listeners (`cx.listener(|this, event, cx| ..)`) get `&mut Self` and emit with `cx.emit(ButtonEvent::Press)`. Timers
  and async work are tasks (`cx.set_interval`, `cx.spawn` with tokio inside); keep the `Task` in a field and drop it to
  stop. Read the environment in the render (`cx.safe_area()`): it renders again when that changes. `Stepper` is the
  example, also of methods: `#[methods] impl Stepper { pub fn set_value(..), pub fn value(&self) -> f64 }` are
  `ref.setValue(5)` and `await ref.value()` in JS, and `r.update(|s, cx| s.set_value(5.0, cx))` on a
  `ComponentRef<Stepper>` in Rust.
- **Every host element is there**: `input()`, `scroll()`, `modal()`, `canvas(Painter::new()..)`, `web_view(url)`,
  `video(url)`, `native(name)`, with typed props and events (`on_change_text(cx.listener(|this, text: &str, cx| ..))`,
  `on_layout`, `on_pan_end`, `on_scroll`). Props that take a plain color take `cx.color(theme.x)`.
- **Element refs** send commands to what the component rendered: `cx.element_ref()` in `setup`, `.element_ref(&r)` on
  the element, `r.focus()` / `r.scroll_to(..)` from listeners and tasks (`TextField` focuses its input after
  clearing it).
- **Native values** move styles without renders: `cx.native_value(0.0)`, `.bind_translate_y(&v)`, `.pan_values(..)`
  (drags move it natively), `v.spring(..)` (await its `AnimationEnd` in a task). `Sheet` is the example.
- **The component's own rows**: `virtual_list(count, move |i, at| row)` with `.keys(ids)` (and `.header(..)`,
  `.footer(..)`, `.empty(..)`, `.separator(..)`, `.sticky_header_indices(..)`, the `Scrollable` behavior); rows'
  handlers are the component's (`listener.map(move |_: &()| i)` passes the index). `SelectList` is the example.
  Rows outlive the render: move what they read into them (copies of theme colors, an `Rc` of the data).
- **Shared state**: `cx.global::<G>()` in the render subscribes, `update_global` renders the readers; `cx.provide(v)` /
  `cx.context::<T>()` pass values down the tree. `catch_error` makes a component an error boundary.
- **Modules are functions without a component**: `#[module] impl Alert { pub async fn alert(title: String, ..,
  cx: AsyncCx) -> i32 }` is `await Alert.alert(title)` in JS (`system.rs`); a sync function takes `cx: &mut ModuleCx`.
  Option parameters at the end may be left out; options objects are `#[derive(JsObject)]` structs. A module named like
  a component adds static functions to it (`StatusBar.setHidden`, `status_bar.rs`). Register both in `registry()`.
- **System UI goes through the context**: `cx.dialog(..)` (await the index chosen), `cx.dismiss_keyboard()`,
  `cx.add_status_bar(..)` / `update_status_bar` / `remove_status_bar` for a rendered setting, `set_status_bar(patch)`
  for a function's; insets with `cx.insets()` in the render and `.inset_padding(..)` / `.inset_margin(..)` on the
  element (`SafeAreaView`, `KeyboardAvoidingView`).
- **Platform-only props** are an `android` / `ios` prop of a `JsObject` struct (`StatusBarAndroid`).
- **Primitives only**, never a platform widget: the look is ours and identical on both platforms (system dialogs are
  the platform's, for now).
- Changing a component needs a rebuild of the runtime (the dev hosts, `nsh-ios-app`, the Android runtime); apps using
  it reload as usual.

How the core runs components: `docs/dev/components.md`.

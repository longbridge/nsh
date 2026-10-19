# nsh-ui-mobile

The components and modules of `nsh/ui/mobile`: buttons, cards, lists, safe areas, the keyboard, the status bar,
system dialogs and the like, written in Rust.
One implementation draws the same on iOS and Android: a component makes its view from NSH's elements with typed styles
and theme colors, the core lays it out in the same pass as everything else, and its interaction (press feedback, drags,
state) stays native. JS only gets the events the component emits.

- **One file per component** (`src/button.rs`), registered in `registry()` (`src/lib.rs`). Start with
  `use nsh_core::ui::prelude::*;`. The JS export is the type's name (`#[component] pub fn Button` is `Button`): no
  name is written by hand, and `the_exports_are_pinned` (`src/lib.rs`) lists them all.
- **Props are a struct** (`#[derive(Clone, Copy, PartialEq, Props)]`, its fields `Prop<T>`), **events an enum**
  (`#[derive(Event)]`, or `NoEvent`), choices an enum (`#[derive(PropEnum)]`). The bundler generates the
  `nsh/ui/mobile` export and its TypeScript declarations from them, doc comments included, so apps write
  `<Button title="Save" onPress={save} />` with full types; there is no hand-written glue. The props struct's doc
  comment documents the component in TypeScript.
- **A component runs once**: `#[component] pub fn Button(props: ButtonProps, cx: Scope<ButtonEvent>) -> impl
  IntoElement` makes its view once, with `view()`, `text()`, `image()` and other components (`Card::element(props)`),
  styled with GPUI-style methods (`.px_4()`, `.rounded(px(16.0))`, `.active(|s| s.bg(..))`). What changes is a
  **binding**: pass a closure or a signal where the value goes (`text(props.title)`, `.bg(move || theme().primary)`,
  `.opacity(move || if props.disabled.get() { 0.5_f32 } else { 1.0 })`, `.style_when(props.disabled, |s| ..)`); when
  what it read changes, only that part is updated. A prop is read with `props.x.get()` in a binding and
  `get_untracked()` in a listener. The root can be any element; the app's `class` (merged last, it wins) and `style`
  apply to it.
- **Colors come from the theme** (`theme()`: gpui-kit's semantic fields, a light and a dark value each), never
  literals: apps restyle every component with their `nsh.json` theme, and dark appearance switches natively. Read
  `theme()` inside the bindings that use it, so a new theme reaches them.
- **What never changes is plain Rust**: `.when(cond, |v| ..)`, `match` at build time. Interaction states are variants:
  `.active(..)`, `.hover(..)`, `.focus(..)`.
- **What comes and goes is a region**: `If::new(cond, || el).otherwise(|| el)`, `dynamic(move || ..)` (an element, an
  `Option` or a `Vec`, made again when what it read changes) and `Each::new(items, key, |item, index| row)` (a row per
  key, kept while the key stays). `TextField`'s label and clear button are the examples.
- **The app's content is props, placed by closures**; the component lays it out, never creates or changes it:
  `header: Prop<Option<Slot>>` (`props.header.place(|h| h.px_4())`), `children: Children` (`.child(props.children)`,
  or `.children_each(props.children, |child, at| ..)` to wrap each), `items: Items` for virtualized rows
  (`.list(props.items, |l| l.estimated_item_size(48.0), |row, at| ..)`; in JS `each` and a row renderer, like `List`).
  `at` is a `Position` of signals: `at.index()`, `at.is_first()`, `at.is_last()`, read in bindings. `#[prop(lazy)]`
  on `children` has JS make them only while the view places them (`Sheet`: fresh content on each opening). `Group`
  and `ListView` are the examples. A Rust component fills them the same way: `.child(..)`, `.fill("header", ..)`, and
  `ListView::element(props).items(virtual_list(n, row).header(..))` for the rows.
- **State is signals** made in the body: `signal(v)` (equal writes are skipped), `signal_with(v, SignalEquals::Never)`,
  `computed(..)` for what derives from them, `stored(v)` for what nothing displays (a running task). Listeners are
  closures (`.on_press(move || count.update(|c| *c += 1))`) and emit with `cx.emit(ButtonEvent::Press)`. An
  `effect(..)` reacts to a prop. Timers and async work are tasks owned by the component (`set_interval(..)`,
  `spawn(async move { timer(d).await; value.set(..) })`, tokio inside); they end with it. `Stepper` is the example,
  also of methods: `#[methods] impl StepperApi { pub fn set_value(&self, ..), pub fn value(&self) -> f64 }`,
  registered with `cx.methods(StepperApi { .. })` and named by `#[component(methods = StepperApi)]`, are
  `ref.setValue(5)` and `await ref.value()` in JS, and `r.api::<StepperApi>()` on a `component_ref::<Stepper>()` in
  Rust.
- **The surface is free functions** whose reads subscribe: `window()`, `safe_area()`, `keyboard_height()`, `dark()`,
  `font_scale()`, `rtl()`, `locale()`, `cx.insets()` (the screen's insets), `color(..)` for props that take a plain
  color, `global::<G>()` (a signal of the surface), `provide_context(v)` / `use_context::<T>()`.
- **Every host element is there**: `input()`, `scroll()`, `modal()`, `canvas(move || painter)`, `web_view(url)`,
  `video(url)`, `native(name)`, with typed props and events (`.value(move || text.get())`, `.on_change_text(move
  |t: &str| ..)`, `on_layout`, `on_pan_end`, `on_scroll`); their setters take bindings too.
- **Element refs** send commands to what the view made: `element_ref()`, `.element_ref(&r)` on the element,
  `r.focus()` / `r.scroll_to(..)` from listeners and tasks (`TextField` focuses its input after clearing it). They are
  `Copy` handles kept by the view's owner, as signals are, and so are animated values.
- **Animated values** move styles with no binding per frame: `animated_value(0.0)`, `.bind_translate_y(&v)`,
  `.pan_values(..)` (drags move it natively), `v.spring(..)` (await its `AnimationEnd` in a task). `Sheet` is the
  example.
- **The component's own rows**: `virtual_list(count, move |at| row)` with `.keys(move || ids)` (and `.header(..)`,
  `.footer(..)`, `.empty(..)`, `.separator(..)`, `.sticky_header_indices(..)`, the `Scrollable` behavior). A cell's
  content is made once and follows the row it shows through `at`: read the row's data in bindings
  (`text(move || options.with(|o| o[at.index()].clone()))`). `SelectList` is the example.
- **Errors**: `catch_error(|| view, |error, reset| fallback)` is an error boundary.
- **Modules are functions without a component**: `#[module] impl Alert { pub async fn alert(title: String, ..) -> i32 }`
  is `await Alert.alert(title)` in JS (`system.rs`; an `async fn` uses the free functions, `dialog(..)`); one that
  is not `async` takes a last `cx: &mut ModuleCx` to reach the platform in order with the UI (a promise of its value),
  or none to run in the JS call and return its value. Option parameters at the end may be left out; options objects are
  `#[derive(JsObject)]` structs. A module named like a component adds static functions to it (`StatusBar.setHidden`,
  `status_bar.rs`). Register both in `registry()`.
- **System UI**: `dialog(..)` (await the index chosen), `dismiss_keyboard()`, `add_status_bar(..)` /
  `update_status_bar(..)` for a component's setting (removed with it), `ModuleCx::set_status_bar(patch)` for a
  function's; `.inset_padding(..)` / `.inset_margin(..)` on an element (`SafeAreaView`, `KeyboardAvoidingView`).
- **Platform-only props** are an `android` / `ios` prop of a `JsObject` struct (`StatusBarAndroid`).
- **Primitives only**, never a platform widget: the look is ours and identical on both platforms (system dialogs are
  the platform's, for now).
- Changing a component needs a rebuild of the runtime (the dev hosts, `nsh-ios-app`, the Android runtime); apps using
  it reload as usual.

How the core runs components: `docs/dev/components.md`.

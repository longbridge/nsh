# Components

The components of a UI set (`Button`, `Card`, ... of `nsh/ui/mobile`) are written in Rust and run by the core on the
engine thread. JS only describes the UI: a component is one node in the JSX tree, its props go to Rust, its events come
back. One implementation draws the same on iOS and Android, is laid out in the same Taffy pass as everything else
(no measuring round trip) and keeps its interaction native.

| Where | What |
|---|---|
| `crates/nsh-core/src/ui/` | The API (`nsh_core::ui`, `prelude`): `Component`, `Module`, elements, `Styled`, `Theme`, `Registry`, the status bar's layers (`status_bar.rs`), the declarations generator `typescript` |
| `crates/nsh-macros` | `#[component]`, `#[derive(Props)]`, `#[derive(PropEnum)]`, `#[derive(Event)]`, `#[derive(JsObject)]`; `#[methods]`, `#[module]`; `#[native_module("…")]` (an app crate's set: `docs/dev/tooling.md`); `style_helpers!` (the spacing methods of `Styled`) |
| `crates/nsh-core/src/core/components.rs` | The core side: instances, children and slots, events; `core/views.rs`: views, their bindings' patches and regions |
| `crates/nsh-ui-mobile` | The mobile set's components, one file each (`README.md`: how to write one) |
| `crates/nsh-js/src/components.rs` | The JS face of components and modules, made from their specs when a bundle imports the set |
| `crates/nsh-js/src/declarations.rs` | `nsh/ui/mobile.d.ts`: the declarations generated from `nsh_ui_mobile::registry()` (`nsh_core::ui::declarations`) with the re-exports and the navigation |
| `nsh_engine::EngineConfig::ui_sets` | The registries a surface serves (the platforms' `set_ui_sets`, by default `nsh_ui_mobile::registry()`; an app's own set: `docs/dev/tooling.md`) |

A desktop set is planned as a crate of its own (`nsh-ui-desktop`), not an abstraction over both; like every UI set it runs
on every backend (macOS, Windows, Linux, and GPUI, a separate platform backend in development, `gpui.md`). It does not exist
yet: on macOS, Windows and Linux apps use `nsh/ui/mobile`, whose components render there as on phones, or their own
layouts from the primitives.

## The API

```rust
/// A press target with a label.          // the TypeScript doc of `Button`
#[derive(Clone, Copy, PartialEq, Props)]
pub struct ButtonProps {
    /// The label.
    pub title: Prop<SharedString>,        // required in TS
    #[prop(default)]
    pub variant: Prop<ButtonVariant>,     // #[derive(PropEnum)]: "primary" | "secondary" | ...
    pub accessibility_label: Prop<Option<SharedString>>,   // `accessibilityLabel?`
}

#[derive(Event)]
pub enum ButtonEvent {
    /// The button was pressed.
    Press,                                // `onPress?: () => void`
}

#[component]                              // JS export: `Button`, the function's name
pub fn Button(props: ButtonProps, cx: Scope<ButtonEvent>) -> impl IntoElement {
    view()
        .px_4()
        .py_3()
        .rounded(move || px(theme().radius))
        .bg(move || theme().primary)
        .active(|s| s.bg(move || theme().primary_active))
        .on_press(move |_: &()| cx.emit(ButtonEvent::Press))
        .child(text(props.title).text_color(move || theme().primary_foreground))
}
```

A component is a function that **runs once** (`Component::view`; `#[component]` makes the unit struct named after
the function and its `Component` impl): it makes signals and effects, then its view, under an owner of its own (the
reactive graph of `crate::reactive`, instantiated for Rust in `ui/reactive.rs`). What changes is bound: a setter, a
style or a text given a closure, a signal, a memo or a `Prop` (anything `IntoReactive<T>`) is a binding, run again
when what it read changes, and the core applies only what that one binding produced. Nothing renders again and
nothing is diffed. Releasing the host disposes the owner: its bindings, effects, tasks, timers, back listeners and
child components go with it. The design and its reasons: `reactive-components.md`.

- **Names come from the type**: `Registry::register::<Button>()` registers it under `std::any::type_name`
  (`nsh_ui_mobile::button::Button`: the key the JS face sets as the host's `component` prop and the core finds it
  by, only compared within one binary) and exports it as the type's own name (`Button`; a generic type's without
  its arguments). A second registration of a type, two types of the same name (two versions of a crate) or two
  components of one set with the same export panic at registration (`Registry::merged` likewise for the sets a
  host serves, also for modules of the same name); components of different sets may share an export. A test
  pins `nsh/ui/mobile`'s exports (`nsh-ui-mobile` `the_exports_are_pinned`). A module's JS name is its type's
  too (`#[module] impl Alert`: `Alert`).
- **Props** are a struct, **events** an enum; the derives produce the decoding of `$name` values from JS, defaults,
  and the TypeScript types (doc comments included). Field and variant names become camel case in JS. Value types
  implement `JsValue` (numbers, `bool`, strings, `Color`, `Option`, `Vec`, `JsonValue`, `ui::Style` (an inline
  `StyleProp<ViewStyle>`), `PropEnum`s, `JsObject` structs). A field that can change is a `Prop<T>` (`Copy`): in the
  instance each is a memo of its own, so a new value from JS or from a parent reaches only what read it
  (`props.title.get()` in a binding, `get_untracked()` in a listener). A plain field is read once. A prop JS sends with
  the wrong type is refused with a warning; clearing a prop restores its default. A Rust parent passes a value
  (`3.into()`), a signal (`n.into()`) or a closure (`Prop::bind(move || ..)`). Platform-only props are an `android` /
  `ios` prop of a `JsObject` struct (`StatusBar`'s `android={{ backgroundColor }}`), as for host elements.
- **`Scope<E>`** (`Copy`) is what the component has of its own: `cx.emit(event)` (to the app's `on<Event>`, or to the
  parent's `.on_event(..)`), `cx.insets()` (its screen's or modal's insets, reading subscribes) and
  `cx.methods(api)` (below). Everything else is a free function of `ui::prelude`.
- **The app's content is props too, placed by closures**: the component never creates or touches the app's elements,
  it only lays them out.
  - `header: Prop<Option<Slot>>` is `Some` while the app passes non-empty JSX for it: `props.header.place(|h|
    h.px_4())` places it (a region; the closure styles the slot's own node).
  - `children: Prop<Children>`: `.child(props.children)` puts them all at one place among the view's own children;
    `.children_each(props.children, |child, at| view().py_3().child(child))` wraps each one (an `Each` keyed by
    the child's node: a child that stays keeps its wrapper; `at` is a `Position`: `index()`, `count()`,
    `is_first()`, `is_last()`, signals read in bindings).
  - `items: Items`: virtualized rows. In JS the component takes `List`'s data props (`each`, the row renderer as
    children, `keyExtractor`, `onEndReached`, `ListHeaderComponent`, ... : `ListItemsProps<T>`, so the component is
    generic over the row type). `.list(props.items, |l| l.flex_1().estimated_item_size(48.0), |row, at|
    view().px_4().style_when(move || at.is_last(), |v| v.rounded_b(px(16.0))).child(row))` places the list, lays it
    out (size, columns, padding, gaps, direction; bound layout values update it in place) and wraps each row; only
    rows near the viewport exist and cells are recycled as with `List`.
- **Elements** are `view()`, `text(..)` (its `.child(..)`ren are spans, as nested `<Text>`s: texts with styles and
  presses of their own, strings, inline views and images, regions; the core merges them into one paragraph),
  `image(..)`, the other host elements (`input()`, `scroll()`, `stack()` of `screen()`s, `modal()`,
  `canvas(painter)`, `web_view(url)` / `web_view_html(html, base_url)`, `video(url)`,
  `input_accessory_view(native_id)` (iOS's bar over the keyboard; empty on Android), `native(name)` for the
  embedder's components; `ui/host.rs`), strings (as texts) and other components (`Button::element(props)`, whose
  `.on_event(move |e: &ButtonEvent| ..)` handles their typed events instead of the app; `.child(..)` / `.children(..)`
  pass it content and `.fill("header", ..)` fills a slot, as JSX children and slot props do: the content is nodes of
  this view, placed where the nested component's view places the app's; regions in it join its children in place).
  Their abilities are traits: `Styled`, `Accessible`, `Interactive` (`active` / `hover` / `focus` styles, `disabled`,
  `hit_slop`, and the typed events every element has: `on_press`, `on_long_press`, `on_layout(Rect)`,
  `on_touch(TouchEvent)`, `on_pan_start`, ..., the touch responder: below), `ParentElement`, `FluentBuilder` (`when`,
  `when_some`, `map`: decided once, when the view is made). Each host element has its own props and typed events
  (`input().value(move || name.get()).on_change_text(move |text: &str| ..)`, `scroll().on_scroll(..)`,
  `modal().on_request_close(..)`; how scroll views and lists scroll is `Scrollable`, shared by `scroll()`,
  `virtual_list(..)` and a placed `ListElement`: snapping, `scroll_enabled`, `keyboard_dismiss_mode`,
  `scroll_value(&v)`, refresh, the scroll events); `on(kind, listener)` takes any native event with its raw
  payload. Host props are the core's `Prop`s, kept one per property and diffed by kind against what the binding
  applied last. A listener is a closure (`Listener<E>`: `impl Fn(&E)`) that writes the signals it captured.
- **What comes and goes is a region** (`ui/flow.rs`): `If::new(cond, || el).otherwise(|| el)` makes a branch when
  the condition flips to it and drops the other; `dynamic(move || ..)` (an element, an `Option` or a `Vec`) is made
  again whenever what it read changes; `Each::new(items, |item| key, |item, index| row)` makes a row when its key
  comes and keeps it, its `item` and `index` signals updated, while the key stays; `catch_error(|| view, |error,
  reset| fallback)` shows the fallback once something under it panicked (a binding, a listener, an effect, a
  component's view: the failed part is disposed; `reset.reset()` makes the view again; unhandled errors are logged,
  `js` target). A modal is presented while it is in the view (an `If` of it), a screen pushed while it is in its
  stack (an `Each` of the routes: the screens below keep their state; `on_dismiss` reports a native pop). The root
  can be any element: the host becomes it (a screen, a modal, a text, a list); a region or a component as the root
  goes in the host view.
- **Canvases** draw what a `Painter` recorded (the Canvas 2D operations: `.fill_style(..)` / `.stroke_style(..)` take
  colors, `CanvasGradient::linear` / `radial` / `conic` and native values; `.stroke_path(&canvas::svg_path(d))`;
  `measure_text(font, text)` is `measureText`; operations built by hand take `painter.num(&v)` / `.paint(..)` and go
  in with `.op(..)`; `isPointInPath` / `isPointInStroke` are `painter.is_point_in_path(x, y, rule)` /
  `.is_point_in_stroke(x, y)` and, with a `Path2D`, `.path_contains(&path, x, y, rule)` / `.stroke_contains(&path, x,
  y)`, on a painter made by a function the drawing and the press handler both call, to find the shape under a tap).
  `canvas(move || draw(..))` draws in a binding: again when what it read changes (a size kept from `on_layout`, the
  appearance through `color(..)`); a new drawing replaces the old one. Props that take a plain color (an input's
  placeholder color) take `move || color(theme().muted_foreground)`: the theme color for the appearance now.
- **What TS does, in Rust**:

  | TS | Rust component |
  |---|---|
  | the component's body (runs once) | the function's body |
  | `signal(v)`, `memo(f)`, `effect(f)`, `untracked`, `batch` | `signal(v)` (`PartialEq`: equal writes are skipped; `signal_with(v, SignalEquals::Never)` without), `computed(f)`, `effect(f)`, `untracked(f)`, `batch(f)`; `stored(v)` for state nothing displays; all `Copy` handles owned by the current owner |
  | event handlers | closures (`move \|e: &E\| ..`) that write signals; `cx.emit(..)` |
  | `setTimeout` / `setInterval`, `clear*` | `set_timeout(d, f)` / `set_interval(d, f)` return a `Spawned`; `.cancel()` clears it; it ends with the component either way |
  | async work | `spawn(async move { timer(d).await; value.set(..) })` on the engine thread, tokio inside; `background(fut)` / `blocking(f)` for `Send` work on the runtime's threads |
  | `cleanup` | `cleanup(f)` (runs before the owner runs again or goes) |
  | `windowSize()`, `safeArea()`, `keyboard()`, `colorScheme()`, ... | `window()`, `safe_area()`, `keyboard_height()`, `dark()`, `font_scale()`, `reduce_motion()`, `screen_reader()`, `rtl()`: reading subscribes |
  | `PixelRatio.get()`, `roundToNearestPixel`, `Platform.OS` / `Version` / `isPad` | `pixel_ratio()`, `round_to_nearest_pixel(v)`, `os()`, `os_version()`, `idiom()` |
  | `Intl.NumberFormat()`, `toLocaleString()` (the user's locale) | `locale()` (a `Locale`: `de-DE`; reading subscribes), formatted with a Rust crate: `Intl` and the locale-aware `toLocale*String`s are JS globals of TS apps, like `fetch` |
  | `requestAnimationFrame(cb)`, `cancelAnimationFrame` | `request_animation_frame(\|at\| ..)` returns a `Spawned`; in a task `next_frame().await` (an `Instant`), once per frame in a loop |
  | `I18nManager.forceRTL` / `allowRTL` | `force_rtl(bool)` / `allow_rtl(bool)`: the root's direction at once, `rtl()` readers follow |
  | `import logo from "./logo@2x.png"`, `import font from "./a.ttf"` | `const LOGO: ImageAsset = asset!("logo@2x.png");` (`image(LOGO)` lays out at its natural size), `const A: FontAsset = asset!("a.ttf");` (`.font_family(A.family)`), with `registry.assets(assets!())` (below) |
  | `Image.getSize(uri)`, `Image.prefetch(uri)` | `image_size(uri)` / `prefetch_image(uri)`: answers to await in a task |
  | `Clipboard.setString(text)`, `await Clipboard.getString()` / `hasString()` (`nsh/clipboard`) | `set_clipboard(text)`, `clipboard_string()` / `clipboard_has_string()`: answers to await in a task (the app's `capabilities.clipboard` applies as for JS) |
  | `AccessibilityInfo.announceForAccessibility`, `setAccessibilityFocus(ref)` | `announce_for_accessibility(msg)`, `note.set_accessibility_focus()` (an `ElementRef`) |
  | `Toast.show(msg, "long")` (`nsh/android`) | `toast(msg, ToastDuration::Long)` (nothing happens off Android) |
  | `useSafeAreaInsets()` | `cx.insets()`: the insets where the component is (the nearest screen's or modal's, else the surface's safe area) |
  | a signal or store at module level | `global::<G>()`: a `Signal<G>` of the surface, made with `Default` (shared by its components and modules) |
  | `context(default)` / `<Ctx.Provider value>` / `Ctx()` | `provide_context(value)` in the body; `use_context::<T>()` below it (through the app's elements too: a component JS made among another's JS children is under where that one places them); provide a signal to change what readers see |
  | `createNativeValue`, `Animated.spring`, `interpolate` | `native_value(0.0)` (`Clone`); `.bind_translate_y(&v)` / `.bind_opacity(..)` / `.bind_background(..)` / `.bind_border_color(..)` / `.bind_color(..)` / `.bind_shadow_color(..)` / `.bind_shadow_opacity(..)` / `.bind_shadow_radius(..)` / `.bind_scale(..)` / `.bind_rotate(..)`, `.pan_values(..)` (drags move it natively), `.pinch_value(&scale)` / `.rotation_value(&angle)` (TS `pinch={{ scale }}`, `rotation={{ angle }}`), `scroll().scroll_value(&v)`, canvas arguments (`Painter::new().fill_rect(0.0, 0.0, &v, 4.0)`); `v.spring(to, Spring::default())` / `.timing(to, ms, easing)` / `.timing_with(to, Timing { delay_ms, .. })` / `.decay(..)` / `.decay_within(.., (min, max), bounce)` return an `AnimationEnd` to await; `v.value()` (`getValue()`; after `v.stop()`, `stop()`'s promise) an `Answer<f32>`; `v.set_offset(..)` / `.flatten_offset()` / `.extract_offset()`; `ui::sample_easing` / `sample_spring` / `sample_decay` (`Easing.sample`, `Animated.sampleSpring` / `sampleDecay`) the curves, to draw them; `v.interpolate(..)`, `.interpolate_with(.., Interpolation { extrapolate_left, extrapolate_right, easing })`, `.add(..)`, `.diff_clamp(..)` derive nodes |
  | `Animated.sequence / parallel / stagger / delay / loop`, `.start(cb)` / `.stop()` / `.reset()` | `Animation::sequence([Animation::timing(&v, to, Timing::new(ms, easing)), Animation::delay(ms), Animation::spring(..)])`, `::parallel(..)` (`.stop_together(false)`), `::stagger(ms, ..)`, `.looped(iterations)` (`.reset_before_iteration(false)`); `a.start()` returns an `AnimationEnd`, `a.stop()`, `a.reset()` |
  | `BackHandler.addEventListener("hardwareBackPress", f)`, `.remove()` | `on_back_press(\|\| -> bool)`: registered while the current owner lives (only while a signal holds: inside an `effect`). After modals and before stacks, the views' listeners run newest first, then JS's; when none returns true the core sends `Command::Back`: the top stack pops, else the app exits (`ui/back.rs`) |
  | `<ErrorBoundary fallback>` | `catch_error(\|\| view, \|error, reset\| fallback)` |
  | a portal | none (TS has none either); overlays are `modal().transparent(true)` |
  | `ref={el}`, `el.focus()`, `await el.measure()` | `element_ref()`, `.element_ref(&r)` on the element, `r.focus()` / `.scroll_to(..)` / `.seek(..)` / `.command(..)` from listeners and tasks; `r.measure()` is an `Answer<Option<Measurement>>` (frame, `page_x` / `page_y`) to await in a task |
  | `onStartShouldSetResponder`, `onMoveShouldSetResponder(Capture)`, `PanResponder.create` | `.claim_touches(TouchClaim::on_start())` (`::on_move()`, `.capture()`, `.ask(\|request: &ClaimRequest\| request.claim())` to decide with state and the gesture, `.block_native(false)`; `TouchClaim::on_pan(PanActivation::horizontal(10.0, 10.0))` (`panActivation`) claims when the platform activates a pan, deciding on the main thread, `elements.md`), `.keep_touches(true)` (`onResponderTerminationRequest` → false); `.on_responder_grant` / `_move` / `_release` / `_terminate` / `_start` / `_end` / `_reject` get a `ResponderEvent` (the finger's `location_x`, `page_x`, ... and `gesture: PanGesture`: `dx`, `dy`, `vx`, `vy`, the centroid; `PanGesture::begin` / `update` follow one from `on_touch`) |
  | a ref to a component, `await stepper.value()`, `stepper.setValue(5)` | `component_ref::<Stepper>()`, `Stepper::element(..).component_ref(&r)`, `r.api::<StepperApi>()`: the API its view registered, its methods called directly |
  | `<List ListHeaderComponent ListFooterComponent ListEmptyComponent ItemSeparatorComponent stickyHeaderIndices viewabilityConfig onViewableItemsChanged onEndReachedThreshold initialScrollIndex>` | `virtual_list(count, row).header(..).footer(..).empty(..).separator(\|\| ..).sticky_header_indices(..).viewability(..).minimum_view_time(..).on_viewable_items_changed(..).end_reached_threshold(..).initial_scroll_index(..)`: the header, footer and empty view are part of the view, never recycled |
  | `<List maintainVisibleContentPosition={{ autoscrollToBottomThreshold: 40 }}>` | `.follow_end(40.0)` (`ListLayout`: on `virtual_list(..)` and on a placed `ListElement`) |
  | `<ListView each={rows} keyExtractor ListHeaderComponent={Header}>{row}</ListView>` | `ListView::element(props).items(virtual_list(n, row).keys(move \|\| ids).header(..))` |
  | `<Text>a <Text style={link} onPress={f}>b</Text> <View/></Text>` | `text("a ").child(link("b").on_press(..)).child(" ").child(view())`: one paragraph, links and inline views |
  | `class="group"`, `group-active:`, `dark:`, `disabled:` | `.group()`, `.group_active(\|s\| ..)`, `.dark(\|s\| ..)`, `.disabled_style(\|s\| ..)` (native, no binding) |
  | `bg-sky-500` (the palette) | `.bg(palette::SKY.shade(500))` |

- **Platform services** are free functions (`ui/ambient.rs`): `dialog(Dialog)` shows an alert or action sheet and
  returns a future of the index chosen (-1: dismissed; the answer is the root's `Dialog` event, which the core
  resolves; JS never sees it); `dismiss_keyboard()`, `animate_layout(..)`, `announce_for_accessibility(..)`,
  `toast(..)`, `force_rtl` / `allow_rtl`, `set_color_scheme(..)`, the clipboard (`set_clipboard`, `clipboard_string`,
  `clipboard_has_string`), the image information (`image_size`, `prefetch_image`), display frames
  (`request_animation_frame`, `next_frame`: the core keeps the waiting tasks' answers, the engine asks the platform
  for a frame while one waits, `MountSink::request_frame`, and delivers it at `EngineControl::frame`), and the status
  bar (below); they are actions the core applies in order with the view's other changes (`ui/services.rs`,
  `core/services.rs`). They work wherever the core runs Rust code: a view's body, its bindings, listeners and tasks,
  and modules.
- **Native values** (`ui/animated.rs`) are nodes of the platform's animation graph with odd ids (JS's are even,
  like node ids: the graph indexes nodes by id, so both must stay dense; ids from 2^31 are the graph's own
  composite steps); the last handle released frees the node. Deriving the same node again gets the live one. Colors
  interpolated from theme colors (`v.interpolate_colors(&[0.0, 1.0], &[colors::TRACK, colors::ACCENT])`, any
  `AdaptiveColor`) follow the appearance: a binding of the view gives the node the other appearance's colors in
  place when it changes (`AnimationOp::SetOutput`), so nothing is made again and its bindings stay. Bound
  style properties go into the element's inline style, before the app's `style` on a host (the core keeps the app's
  and composes the two). The animations' ends come back through `Core::animations_finished` (the engine hands JS
  the rest). An `Animation` is sent whole as one `AnimationOp::StartComposite` and runs in the graph step after step;
  its composite key (odd, JS's even) is what `reset()` restores by, and dropping the last handle releases it (a run
  going on goes on).
- **Element refs** (`ui/element_ref.rs`) hold the node of the element that carries them, set when the element is
  applied and cleared when it goes; their commands go out through the executor, and an event's handling applies them
  right after its own changes, so they keep their order. A measurement (`measure()`) waits in the core for the layout
  of the commit it goes out with, as JS's `Command::Measure` does, and is answered there (`Core::measure`); none goes
  to the platform.
- **Element attributes for system UI**: `.inset_padding(insets)` / `.inset_margin(insets)` add to the padding /
  margin the styles resolve to (what `SafeAreaView` and `KeyboardAvoidingView` use, so the app's own padding stays),
  `.inline_style(props.content_container_style)` applies a `ui::Style` prop (a value or a binding) (`KeyboardAvoidingView`'s `contentContainerStyle`).
- An app's own types become static values for setters with `nsh_core::static_reactive!(Tone: Tone);`. A closure
  returning a bare float literal for an `f32` setter needs its type: `move || if on { 0.5_f32 } else { 1.0 }`.

## Methods

The methods of a component's ref in JS are those of an API handle its view makes, holding the signals they read and
write: `#[methods] impl StepperApi { pub fn set_value(&self, v: f64) {..} pub fn value(&self) -> f64 {..} }`
implements `Api`, the view registers it with `cx.methods(StepperApi { .. })` for as long as it lives, and
`#[component(methods = StepperApi)]` (`Component::METHODS = Methods::of::<StepperApi>()`) names it in the spec, so the
JS face builds the ref (`StepperRef`: `node` and the methods, typed from the Rust signatures and docs) with no TS
written. A call is a `Command::Native { name, args: { c, a } }` op, so it runs in order with the props JS set before
it. A method returning nothing is called and forgotten (`c` 0); one returning a `JsValue` gives JS a `Promise`,
answered by a `$return` event (`{ c, v }` or `{ c, e }` for a refused argument) in the same frame's commit rounds. Rust
calls them directly: `ComponentRef::api::<StepperApi>()`.

## Modules

Functions a UI set exports without a component (`Alert.alert`, `Keyboard.dismiss`, `StatusBar.setHidden`) are a
`#[module]` `impl` block of a unit struct: `pub fn`s are the functions, typed from their signatures (`Option`
parameters at the end may be left out); a sync one may take a last `cx: &mut ModuleCx` (the platform's services,
applied in order right after the call). One returning a `JsValue` returns a `Promise` in JS; an `async fn` takes no
context and runs as a root task, reaching the platform with the same free functions views use (`Alert.alert` awaits
`dialog(..)`). `Registry::module::<M>()` registers it. A call is a root `Command::Native { name: "Alert.alert", args:
{ c, a } }` op (in order with the ops before it), answered like methods by a root `$return` event; `components.rs`
builds the object. A module with a component's name is merged into it (`StatusBar` is a component with static
functions). `Registry::reexport` re-exports runtime values (`safeArea`, `keyboard`, `useSafeAreaInsets`: `nsh-js`
`env.rs`) under the set's module, and `Registry::native_values` names the values `nsh-js` makes for the set (the
mobile UI's navigation: `navigation.rs`).

The runtime has modules of its own the same way (`crates/nsh-core/src/runtime/`, `runtime::registries()`): one
`Registry` per JS module (`Registry::runtime("nsh/android")`), whose functions JS calls as `nsh/android:Toast.show` so
they never clash with a UI set's; the core looks a call up in the set's registry, then the runtime's. Runtime modules
use the same services as components (`cx.toast`, `cx.announce_for_accessibility`, `image_size`), and the core's
internal actions for the rest (exiting the app, a ref's accessibility focus).
`#[module(flat)]` exports functions one by one (`setStatusBarBackgroundColor`); a function returning `Result` rejects
the promise with the error; a `NodeRef` parameter is a `YNode` in TS (sent as `{ $node: id }`).

## The status bar

`ui/status_bar.rs` layers the surface's settings (`StatusBarSetting`: style, hidden, animated, background, each
optional), one `StatusBars` per surface shared by its components, modules and tasks. At the bottom is what functions
set: `ModuleCx::set_status_bar(patch)` and JS's root `statusBar` command (`nsh/android`'s
`setStatusBarBackgroundColor`) both become `Command::SetStatusBar(patch)`. Over it, the settings views add
(`add_status_bar(setting)` → `StatusBarEntry`, removed when the current owner goes; `update_status_bar(entry, ..)`
from an effect on the props); the most recently added wins, field by field. Each change only marks the layers;
`commit` sends one `Command::StatusBar` with the merged result when it differs from the last one sent.

## Tasks

`ui/task.rs`: a small executor of the core runs futures on the engine thread, so they may hold signals and other
`!Send` handles. Each poll runs inside a tokio runtime's context (`Core::set_task_runtime`; the engine's
`EngineConfig::runtime`, else one runtime of two threads shared by the process), so the tokio ecosystem works in a
task: `tokio::time` timers (`timer(d)`, `timer_until(at)`), network and file IO, channels; `background(fut)` /
`blocking(f)` run `Send` work on the runtime's threads. A waker may fire on any thread: it sends the engine a `Wake`
message, and the loop runs the woken tasks (`Core::run_component_tasks`). A task changes what it wants by writing
signals, inside the graph like any other Rust code of the view; the patches that makes go out with the next commit.
`spawn`, `set_timeout`, `set_interval` and `request_animation_frame` return a `Spawned`: the task belongs to the owner
current when it was made and ends with it (or at `.cancel()`); awaiting a `Spawned` gives its output.

## Environment

The engine hands the core what views may read (`ui::Environment`: window, safe area, keyboard height, appearance,
font scale, reduced motion, screen reader, direction, pixel ratio, OS, its version, idiom, locale). The core keeps
each part in a signal of the surface (`ui/ambient.rs`), as it does the theme, the globals and each screen's or modal's
insets: `window()` and the like read them, so a binding that read a part runs again when it changes and nothing else
does. The direction views read is the surface's: its root's (`Prop::LayoutDirection`, which JS's `I18nManager`
sets), else the platform's; once a view sets a policy (`force_rtl` / `allow_rtl`), and from the start in a Rust app,
the core sets the root's direction from it and the language (in an app whose JS sets it too, the last one wins; JS's
`isRTL` does not hear Rust's).

## Assets

A crate ships files in its `assets/` directory, as TS imports them: `asset!("logo@2x.png")` (`nsh-macros`, the path
relative to `assets/`) reads the file at compile time and is an `ImageAsset { uri, width, height }` (the natural size
in points: `@2x` / `@3x` divide the pixels), a `FontAsset { uri, family }` (`.ttf`, `.otf`: the family from the
font's name table) or a `FileAsset { uri }`; a missing or unreadable file fails the build, a changed one builds the
crate again (`include_bytes!`). The URI is `asset:/@<package>/<path>`, apart from the app's own files. The registry
ships the directory: `registry.assets(assets!())` lists every file (`AssetFile`: URI, the file where the crate was
built, kind), which the app's command line (`rust_app.rs`, the registry is in-process) serves in development and
copies into packages next to the imported assets, and whose fonts the engine registers before the first view
(`EngineConfig::ui_sets`, read through `EngineConfig::assets`).

## Styles

`Styled` methods are named as in GPUI and Tailwind (`.flex_row()`, `.items_center()`, `.p_4()` = 16pt, `.gap(px(6.0))`,
`.rounded_xl()`, `.border_1()`, `.shadow_md()`, `.text_sm()`, `.font_weight(FontWeight::SEMIBOLD)`, ...); spacing steps
follow Tailwind's default scale, independent of the app's Tailwind theme. They cover what classes and styles do:
per-side borders and colors (`.border_t(6.0).border_t_color(..)`), `.border_dashed()`, per-corner and start / end
radii (`.rounded_tl(..)`, `.rounded_ss(..)`), `.border_curve(BorderCurve::Continuous)`; shadows of their own
(`.shadow([BoxShadow { .. }])`, React Native's `.shadow_opacity(..).shadow_radius(..).shadow_offset(..)`,
`.shadow_color(..)` for both and the presets), `.inset_shadow_sm()`, rings, outlines, text shadows; 3D transforms and
skews, `.transform_origin(..)` / `.origin_top_left()`, `.backface_hidden()`; decorations (`.underline().line_through()`
draw both, `.text_decoration_wavy()`, `.text_decoration_color(..)`), `.uppercase()`, `.tabular_nums()`,
`.writing_direction(..)`; gradients (`.bg_linear_to_br().gradient_from(..).gradient_via(..).gradient_to(..)`,
`.bg_radial()`, `.bg_conic(..)`), filters and backdrop filters (`.grayscale(1.0)`, `.backdrop_blur(12.0)`),
`.mix_blend_mode(..)`, transitions (`.transition_colors().duration(150.0).ease_out()`), keyframes (`.animate_spin()`; with TS's own timing keys
`.animation(KeyframeAnimation::new(Keyframes::Spin).duration(800.0))`),
grids (`.grid_cols(3)`, `.col_span(2)`), `.flex_grow(n)`. Tailwind's palette is typed too
(`ui::palette::SKY.shade(500)`). An element records declarations (`StyleRefinement`); the core lowers them into the
same rules Tailwind classes compile to (`ClassList`), cached and shared between elements with the same styles. There
is no string to parse and no `.class()`.

- A style method given a binding (`.bg(move || ..)`, `.opacity(props.dim)`, `.style_when(cond, |s| ..)`) is a bound
  part of the element's style: when one runs again, the core lowers the element's style with the new value and
  applies only the declarations that changed. A step after a bound one in an ordered list (transforms) is kept in
  the bound part too, so the order holds.
- Theme colors are `AdaptiveColor`s (a light and a dark value) and lower into one declaration that picks by
  appearance (`Decl::Adaptive`): the platform's appearance switch restyles natively, without a binding, and the app's
  plain classes still win over it. The appearance is the app's own when it set one (`nsh.json` `"appearance"`, TS
  `setColorScheme`, `set_color_scheme`): the core keeps the system's and resolves `dark` for classes and `dark()`
  from both (`Core::set_appearance`), and tells the platform (`MountItem::Appearance`: iOS
  `overrideUserInterfaceStyle` on the root view, presented modals and alerts, the status bar's default style; Android
  the status bar's default style and alert themes; native widgets elsewhere keep the system's).
- `.active(|s| ..)`, `.hover(..)`, `.focus(..)`, `.disabled_style(..)` lower into rules on the view's interaction
  states (Tailwind's ranks): press feedback is native. `.group()` makes a view a group and `.group_active(|s| ..)`
  (`_hover`, `_focus_within`, `_named` for `group/name`) styles what is inside it by the group's states;
  `.dark(|s| ..)` styles what is not a color in dark appearance. Nested variants all have to hold.
- What never changes is plain Rust, decided when the view is made: `.when(variant == Variant::Ghost, |v| ..)`.
- The app's `class` / `classList` on a component (Tailwind) is merged after the root's rules; at equal rank it wins.
  The styles a view gives a nested component element merge the same way (bound ones too: `Patch::Nested`).

## The theme

`Theme` (`ui/theme.rs`) has gpui-kit's semantic colors (`primary`, `primary_active`, `primary_foreground`,
`secondary`, `danger`, `muted`, `border`, `group_box`, `list_*`, `switch*`, `tab*`, ...: the fields mobile components
use) plus `radius`, `radius_lg` and `font_size`, and derefs to its colors (`theme.primary`). The default
(`default-theme.json`) is gpui-kit's default theme with mobile radii. An app sets its own in `nsh.json`: `"theme":
"brand.json"` (a gpui-kit theme file, relative to `nsh.json`) or the same JSON inline. Keys only desktop components
use are ignored; a pressed color the theme leaves out is derived from its base; null colors (gpui-kit serializes unset
ones so) are left as they are; colors are CSS colors or Tailwind palette names. The bundler checks the theme; the entry
hands it to the renderer (`nsh:rt` `setup`), which sends it to the root (`uiTheme`) before any view is made.
`theme()` reads it from a signal of the surface: a new theme runs again the bindings that read it. An embedding app
can give a surface its own theme (`Core::set_host_theme`, `EngineControl::set_theme`; sent from
`Engine::start_with`'s `setup` it is in the first frame): it wins over the app's until cleared, so an embedded app
looks like its host (`embedding.md`, "GPUI and GPUI Kit apps").

## The host node and ids

In JS a component is a view (the host element `view`) with a `component` prop naming it (its registered name,
`nsh_ui_mobile::button::Button`): its **host**. `Core::apply` sees the prop, creates an instance and from then on
intercepts the ops on the host (`intercept`): the host is the root of the component's view, the elements below the
root become nodes the core creates itself. Apps without components pay one check per op. The root can be any
element: at the view's first mount, before the host's native view is sent, the core gives the host the root's kind
(`Core::retag`: a text, a screen, a modal, a list), so the root's props and children apply to it; a root that is a
region or a component goes in the host view.

A view JS made (`<Button>` in TSX) is made when its host is first in the root's tree, so it is under the view that
placed it: it adopts the owner of the nearest component above it (where that one places its JS children, else its
own owner), and `use_context` and failures reach that far (`owner_above`, `adopt`). Until then it waits
(`components.waiting`).

JS allocates even node ids (from 2), the core odd ones (from 3), so both id spaces stay dense and never collide. JS
never sees the made nodes; their events are the component's (below).

## Views and bindings

`core/views.rs`. The view is expanded once, under the component's owner. An element's bound parts (its text, its
bound props, the bound parts of its style, a list's bound layout) are taken off it first: the static parts are
applied as they are, and each bound part becomes a render effect of the graph whose runs queue a `Patch` with the new
value (`Text`, `Attrs`, `Style`, `Layout`, `Nested`, a list's `Rows`). The core keeps what it applied last per node
(`BoundNode`) and applies a patch against it with the field diffs of host props and styles, so a change sends only
what changed.

- **Patches are taken at the start of each commit round**, with the events views emitted, until none are left:
  writes in one event, task or JS turn coalesce into one run of each binding (the graph's batch), and their changes
  go out in one `MountBatch`.
- **Regions** are effects too (`Patch::Region`): the core reconciles that region's nodes alone, in place among its
  parent's other children before the region's anchor (the next static node, or the next region's first node);
  keyed elements (an `Each`'s rows) match by key and stay. Each element of a region is made under an owner of its
  own (the branch, the row), disposed with what it made when the element leaves; a region in a nested component's
  content joins that component's children in place (`join_nested_at`).
- **Failures**: Rust code of the view (its body, a binding, a listener, an effect, a task) runs shielded. A panic
  disposes the owner it happened under and goes to the nearest `catch_error` above it (`boundary_of`), else it is
  logged (`js` target). A panic never unwinds through the graph.
- An unknown component name logs a denial and shows nothing.
- Cost: a binding that runs again and changes one text costs ~0.3µs in the graph; the commit around it what the
  tree's layout costs (`bench_view_tick`, `docs/dev/performance.md`).

## Children, slots and rows

- JS inserts a component's children into the host; the core places them where the view's `Children` is, before the
  node that follows it (or keeps them out of the tree while the view has none: a `Children` in a region that is not
  shown). The core sets the `Prop<Children>` when JS adds the first child or removes the last, so
  `props.children.get().is_empty()` in a binding follows it. With `children_each`, the children are an `Each` keyed by the child's node over a signal of the host's JS
  children: one wrapper per child, so a child added or removed leaves the others' wrappers in place.
- Named slots: the JS face creates one holder view per slot **the app passes** (`componentSlot`), filled with that
  prop's JSX. `Prop<Option<Slot>>` is `Some` while the holder has children (the core sets it when it fills or
  empties); `place(..)` is a region of it, and the slot's element *is* the holder: its styles and handlers apply to
  it, the core places it but never releases it (it is JS's).
- Rows: the JS face makes a `List` of the app's data and row renderer and hands it to the host as the slot of the
  `Items` prop. The view places it like a slot and sends its layout (`ListConfig`: estimated size, columns,
  direction, padding, gaps; its bound parts as `Patch::Layout`). Each item cell of that list gets the component's row
  wrapper (`Inner::Row`): made once per cell under an owner of its own, with a `Position` whose index the core sets
  when the list rebinds the cell to another row (`cellIndex`) and whose count when the number of rows changes, so
  only the wrapper's bindings that read them run. The row the app rendered goes into the wrapper. Binding,
  recycling, measuring and positioning stay the list's.
- The component's own rows (`virtual_list(count, |at| row)`, layout from `ListLayout`, `.keys(ids)` for splices):
  the core creates the list and binds its cells itself, within the commit that needs them (`Core::commit` binds and
  lays out again; JS is not involved). Each cell's content (`Inner::VirtualRow`) is made once, under an owner of its
  own, and shows the row its `Position` says: a memo of the list's count and keys moves the cells' index signals
  (`move_rows`) before the bindings that read them run, so a splice runs only the bindings of the rows that moved
  and never shows a row with another row's data. With keys, rows removed or inserted before others leave their
  cells and sizes alone. A cell past the end (the list shrank) disposes its content until the list binds it again.
  The header, footer and empty view are part of the view (their `CellSlot`s), outside the recycling; a separator
  goes into each row's cell, after the row (before it, inverted), while the row is not the last.
- Another component's rows (`ComponentElement::items(virtual_list(..))`): the list is content of the nested
  component, as `.child` is: nodes of this view (its rows, header and handlers are this component's), the slot of
  the nested component's `Items` prop, which places and lays it out as it places JS's list (its layout and styles,
  so the list's own are left out). Each cell is then the placing component's row wrapper (`Inner::Row`) around a
  node showing the row (`Inner::VirtualRow`), so each part's handlers run with its own component.
- Lazy children (`#[prop(lazy)] children: Children`, `Sheet`'s): the JS face makes the app's children only while the
  view places them, under a `cond` on a signal; the core tells it with a `$children` event (`true` / `false`) on the
  host when the view starts or stops placing them (`ComponentSpec::lazy_children`), so they are made again on each
  opening and released after. Until they are made, `Children::is_empty()` is true. The content a Rust view passes
  such a component (`Sheet::element(..).child(..)`) is lazy the same way: the core keeps it in a region of a signal it
  sets when the nested component starts or stops placing its children (slots and rows stay), instead of telling JS.

## Events

- Handlers sit on the elements; the native event of a made node, or of the host, goes to the component first
  (`Core::component_event`, called by the engine before JS). Events of made nodes never reach JS. A listener runs in
  the graph like any other Rust code of the view: the signals it writes run their bindings once it returns.
- A slot's node (JS's holder, the app's list of an `Items` prop, or another component's content) gets the
  handlers of the component it is a slot of, then its maker's: JS's (the app's `onEndReached` on a `ListView`)
  or the view's that passed it.
- A component's own list reports its viewable rows to it in the core, waiting for `minimum_view_time` when set
  (a task of the list's owner).
- `cx.emit(event)` is routed after the code that emitted it: to JS as a `NativeEvent` on the host (`{ name, data }`,
  from the `Event` derive; the JS face calls the app's `on<Name>(data)`), or, for a nested component, to the
  `on_event` listener of the view that made it, typed, whose own emits are routed in turn until none are left.
- The back button: `on_back_press` listeners of views run newest first, after modals' and before stacks', then JS's
  (`ui/back.rs`).

## The JS face and the declarations

`nsh-js` `components.rs` makes each component's and module's JS from its spec (`ComponentSpec`, `ModuleSpec`) when a
bundle first imports the set. `nsh_core::ui::declarations(registry, omit)` writes their declarations (into the set's
module file, `nsh/ui/mobile.d.ts`; `nsh-js` `declarations.rs` adds the re-exports and native values it makes): the
named types the props, events, methods and functions use (`export type
BarStyle = ...` for a `PropEnum`, `export interface AlertButton` for a `JsObject`, each once, with its doc comment;
`JsValue::ts_type(&mut TsTypes)` declares them), the props interfaces, the components and the module objects (each
of type `<Name>Module`). The JS face runs for every instance, so it is kept minimal: **one render effect reading every prop
directly** and sending the ones that changed, no `split` / `merge` / `spread` (their proxies made the first version, in
the former JS runtime, cost more JS than the TSX component it replaced), no node for an absent slot, no event listener
when no handler is passed. Benchmarks against the same Button in TSX: `docs/dev/performance.md` (`examples/bench`,
`component_bench`).

## Touches and the responder

The platform reports touches on the root, each finger with the listening nodes around it (`docs/dev/elements.md`).
The core takes the components' nodes on that path (`core/touch.rs`): their `on_touch` handlers hear the touches from
the finger outwards, then the responder is negotiated among their `claim_touches` as `nsh-js` does among JS's
handlers (captures from the outermost in, then from the finger outwards; with a responder, only the nodes around it;
`keep_touches` refuses to hand over), and the responder's listeners get the events with the gesture. Claims without
`ask` are decided from the element, running no component code. The event goes on to JS for its own nodes; each side
negotiates its own responder, and the core sends both sides' `SetJsResponder` / `ClearJsResponder` (the one that
blocked last wins while both do, and the other's block comes back when it lets go).

## Native components of the embedder

`nsh/native`'s `nativeComponent` is the other kind: a platform view the embedding app registered (Swift / Java
`NshComponent`, `docs/dev/embedding.md`). Those are views of their own on the main thread; Rust components are
compositions of primitives in the core.

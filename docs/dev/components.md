# Components

The components of a UI set (`Button`, `Card`, ... of `nsh/ui/mobile`) are written in Rust and run by the core on the
engine thread. JS only describes the UI: a component is one node in the JSX tree, its props go to Rust, its events come
back. One implementation draws the same on iOS and Android, is laid out in the same Taffy pass as everything else
(no measuring round trip) and keeps its interaction native.

| Where | What |
|---|---|
| `crates/nsh-core/src/ui/` | The API (`nsh_core::ui`, `prelude`): `Component`, `Module`, elements, `Styled`, `Theme`, `Registry`, the status bar's layers (`status_bar.rs`), the declarations generator `typescript` |
| `crates/nsh-macros` | `#[derive(Props)]`, `#[derive(PropEnum)]`, `#[derive(Event)]`, `#[derive(JsObject)]`; `#[methods]`, `#[module]`; `#[native_module("…")]` (an app crate's set: `docs/dev/tooling.md`); `style_helpers!` (the spacing methods of `Styled`) |
| `crates/nsh-core/src/core/components.rs` | The core side: instances, expansion, diffing, children and slots, events |
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
#[derive(Default)]
pub struct Button;                       // the component's state

/// A press target with a label.         // the TypeScript doc of `Button`
#[derive(Clone, PartialEq, Props)]
pub struct ButtonProps {
    /// The label.
    pub title: SharedString,             // required in TS
    #[prop(default)]
    pub variant: ButtonVariant,          // #[derive(PropEnum)]: "primary" | "secondary" | ...
    pub accessibility_label: Option<SharedString>,   // `accessibilityLabel?`
}

#[derive(Event)]
pub enum ButtonEvent {
    /// The button was pressed.
    Press,                               // `onPress?: () => void`
}

impl Component for Button {                // JS export: `Button`, the type's name
    type Props = ButtonProps;
    type Event = ButtonEvent;

    fn render(&self, props: &ButtonProps, cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        view()
            .px_4()
            .py_3()
            .rounded(px(theme.radius))
            .bg(theme.primary)
            .active(|s| s.bg(theme.primary_active))
            .on_press(cx.listener(|_this, _, cx| cx.emit(ButtonEvent::Press)))
            .child(text(props.title.clone()).text_color(theme.primary_foreground))
    }
}
```

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
  `StyleProp<ViewStyle>`), `PropEnum`s, `JsObject` structs). A prop JS sends with the wrong type is refused with a
  warning and nothing renders; clearing a prop restores its default. Platform-only props are an `android` / `ios`
  prop of a `JsObject` struct (`StatusBar`'s `android={{ backgroundColor }}`), as for host elements.
- **The app's content is props too, placed by closures**: the component never creates or touches the app's elements,
  it only lays them out.
  - `header: Option<Slot>` is `Some` while the app passes non-empty JSX for it: `.slot(props.header, |h| h.px_4())`
    places it (the closure styles the slot's own node).
  - `children: Children`: `.child(props.children)` puts them all at one place among the render's own children;
    `.children_each(props.children, |child, at| view().py_3().child(child))` wraps each one (it runs again when
    the children change; `at` is a `Position`: `index`, `count`, `is_first()`, `is_last()`, what an `<Each>` row knows
    from its index and the data).
  - `items: Items`: virtualized rows. In JS the component takes `List`'s data props (`each`, the row renderer as
    children, `keyExtractor`, `onEndReached`, `ListHeaderComponent`, ... : `ListItemsProps<T>`, so the component is
    generic over the row type). `.list(props.items, |l| l.flex_1().estimated_item_size(48.0), |row, at|
    view().px_4().when(at.is_last(), |v| v.rounded_b(px(16.0))).child(row))` places the list, lays it out (size, columns, padding, gaps, direction) and wraps each
    row; only rows near the viewport exist and cells are recycled as with `List`.
- **Elements** are `view()`, `text(..)` (its `.child(..)`ren are spans, as nested `<Text>`s: texts with styles and
  presses of their own, strings, inline views and images; the core merges them into one paragraph), `image(..)`, the
  other host elements (`input()`, `scroll()`, `stack()` of
  `screen()`s, `modal()`,
  `canvas(drawing)`, `web_view(url)` / `web_view_html(html, base_url)`, `video(url)`,
  `input_accessory_view(native_id)` (iOS's bar over the keyboard; empty on Android), `native(name)` for the
  embedder's components; `ui/host.rs`),
  strings (as texts) and other components (`Button::element(props)`, whose `.on_event(cx.listener(..))` handles their
  typed events instead of the app; `.child(..)` / `.children(..)` pass it content and `.fill("header", ..)` fills a
  slot, as JSX children and slot props do: the content is nodes of this render, placed where the nested component's
  render places the app's). Their abilities are traits: `Styled`, `Accessible`, `Interactive` (`active` /
  `hover` / `focus` styles, `disabled`, `hit_slop`, and the typed events every element has: `on_press`,
  `on_long_press`, `on_layout(Rect)`, `on_touch(TouchEvent)`, `on_pan_start`, ..., the touch responder: below),
  `ParentElement`, `FluentBuilder`
  (`when`, `when_some`, `map`). Each host element has its own props and typed events (`input().value(..)
  .on_change_text(cx.listener(|this, text: &str, cx| ..))`, `scroll().on_scroll(..)`, `modal().on_request_close(..)`;
  how scroll views and lists scroll is `Scrollable`, shared by `scroll()`, `virtual_list(..)` and a placed
  `ListElement`: snapping, `scroll_enabled`, `keyboard_dismiss_mode`, `scroll_value(&v)`, refresh, the scroll events);
  `on(kind, listener)` takes any native event with its raw payload. Host props are the core's `Prop`s, kept one per
  property and diffed by kind: one a render stops setting goes back to its default, as when JS clears it. A modal
  is presented while it is in the render, a screen pushed while it is in its stack (keyed by its route, so the
  screens below keep their state; `on_dismiss` reports a native pop, and the render leaves the screen out). A
  component's root is a view (its host node), so a component cannot render its own screen: the screen's options
  belong to whoever renders the stack (state they depend on goes in a global or a context; `examples/gallery-rs`). Canvases draw a `Drawing`, recorded with `Painter` (the Canvas 2D
  operations: `.fill_style(..)` / `.stroke_style(..)` take colors, `CanvasGradient::linear` / `radial` / `conic`
  and native values; `.stroke_path(&canvas::svg_path(d))`; `cx.measure_text(font, text)` in the render is
  `measureText`; operations built by hand take `painter.num(&v)` / `.paint(..)` and go in with `.op(..)`;
  `isPointInPath` / `isPointInStroke` are `painter.is_point_in_path(x, y, rule)` / `.is_point_in_stroke(x, y)` and,
  with a `Path2D`, `.path_contains(&path, x, y, rule)` / `.stroke_contains(&path, x, y)`, on the render's
  painter: made by a method the render and the press handler both call, to find the shape under a tap); a new drawing replaces the old one. Props that take a plain color (an input's placeholder color)
  take `cx.color(theme.muted_foreground)`: the theme color for the appearance now.
- **State** is `Self`, written like a TSX component where Rust allows:

  | TS | Rust component |
  |---|---|
  | the component's body (runs once) | `fn setup(&mut self, props, cx)` |
  | an `effect` reading a prop | `fn props_changed(&mut self, old, new, cx)` (before the next render) |
  | event handlers | `cx.listener(|this, event, cx| ..)`: `&mut Self`, `cx.emit(..)`, `cx.notify()` renders again |
  | `setTimeout` / `setInterval`, `clear*` | `cx.set_timeout(d, ..)` / `cx.set_interval(d, ..)` return a `Task`; dropping it clears it |
  | async work | `cx.spawn(|this, cx| async move { .. this.update(..).await })`; the component's tasks end with it |
  | `cleanup` | `fn cleanup(&mut self, props, cx)` (with a `Context`: it can still use the platform's services), and `Drop` |
  | `windowSize()`, `safeArea()`, `keyboard()`, `colorScheme()`, ... | `cx.window()`, `cx.safe_area()`, `cx.keyboard_height()`, `cx.dark()`, ... in the render: reading subscribes |
  | `PixelRatio.get()`, `roundToNearestPixel`, `Platform.OS` / `Version` / `isPad` | `cx.pixel_ratio()`, `cx.round_to_nearest_pixel(v)`, `cx.os()`, `cx.os_version()`, `cx.idiom()` in the render |
  | `Intl.NumberFormat()`, `toLocaleString()` (the user's locale) | `cx.locale()` in the render (a `Locale`: `de-DE`; reading subscribes), formatted with a Rust crate: `Intl` and the locale-aware `toLocale*String`s are JS globals of TS apps, like `fetch` |
  | `requestAnimationFrame(cb)`, `cancelAnimationFrame` | `cx.request_animation_frame(\|this, at, cx\| ..)` returns a `Task` (dropping it cancels); in a task `cx.next_frame().await` (an `Instant`), once per frame in a loop |
  | `I18nManager.forceRTL` / `allowRTL` | `cx.force_rtl(bool)` / `cx.allow_rtl(bool)`: the root's direction at once, `cx.rtl()` readers render again |
  | `import logo from "./logo@2x.png"`, `import font from "./a.ttf"` | `const LOGO: ImageAsset = asset!("logo@2x.png");` (`image(LOGO)` lays out at its natural size), `const A: FontAsset = asset!("a.ttf");` (`.font_family(A.family)`), with `registry.assets(assets!())` (below) |
  | `Image.getSize(uri)`, `Image.prefetch(uri)` | `cx.image_size(uri)` / `cx.prefetch_image(uri)`: answers to await in a task |
  | `Clipboard.setString(text)`, `await Clipboard.getString()` / `hasString()` (`nsh/clipboard`) | `cx.set_clipboard(text)`, `cx.clipboard_string()` / `cx.clipboard_has_string()`: answers to await in a task (the app's `capabilities.clipboard` applies as for JS) |
  | `AccessibilityInfo.announceForAccessibility`, `setAccessibilityFocus(ref)` | `cx.announce_for_accessibility(msg)`, `self.note.set_accessibility_focus()` (an `ElementRef`) |
  | `Toast.show(msg, "long")` (`nsh/android`) | `cx.toast(msg, ToastDuration::Long)` (nothing happens off Android) |
  | an effect on the environment | `fn environment_changed(&mut self, props, old, new, cx)` |
  | `useSafeAreaInsets()` | `cx.insets()` in the render: the insets where the component renders (the nearest screen's or modal's, else the surface's safe area) |
  | a signal or store at module level | `cx.global::<G>()` in the render (subscribes); `cx.update_global::<G, _>(..)` in listeners, tasks' updates and modules renders the readers again |
  | `context(default)` / `<Ctx.Provider value>` / `Ctx()` | `cx.provide(value)` in the render; `cx.context::<T>()` below it (the nearest provider up the node tree, through the app's elements too; subscribes) |
  | `createNativeValue`, `Animated.spring`, `interpolate` | `cx.native_value(0.0)` (in `setup`, kept in a field); `.bind_translate_y(&v)` / `.bind_opacity(..)` / `.bind_background(..)` / `.bind_border_color(..)` / `.bind_color(..)` / `.bind_shadow_color(..)` / `.bind_shadow_opacity(..)` / `.bind_shadow_radius(..)` / `.bind_scale(..)` / `.bind_rotate(..)`, `.pan_values(..)` (drags move it natively), `.pinch_value(&scale)` / `.rotation_value(&angle)` (TS `pinch={{ scale }}`, `rotation={{ angle }}`), `scroll().scroll_value(&v)`, canvas arguments (`Painter::new().fill_rect(0.0, 0.0, &v, 4.0)`); `v.spring(to, Spring::default())` / `.timing(to, ms, easing)` / `.timing_with(to, Timing { delay_ms, .. })` / `.decay(..)` / `.decay_within(.., (min, max), bounce)` return an `AnimationEnd` to await; `v.value()` (`getValue()`; after `v.stop()`, `stop()`'s promise) an `Answer<f32>`; `v.set_offset(..)` / `.flatten_offset()` / `.extract_offset()`; `ui::sample_easing` / `sample_spring` / `sample_decay` (`Easing.sample`, `Animated.sampleSpring` / `sampleDecay`) the curves, to draw them; `v.interpolate(..)`, `.interpolate_with(.., Interpolation { extrapolate_left, extrapolate_right, easing })`, `.add(..)`, `.diff_clamp(..)` derive nodes |
  | `Animated.sequence / parallel / stagger / delay / loop`, `.start(cb)` / `.stop()` / `.reset()` | `Animation::sequence([Animation::timing(&v, to, Timing::new(ms, easing)), Animation::delay(ms), Animation::spring(..)])`, `::parallel(..)` (`.stop_together(false)`), `::stagger(ms, ..)`, `.looped(iterations)` (`.reset_before_iteration(false)`), built in `setup` and kept in a field; `a.start()` returns an `AnimationEnd`, `a.stop()`, `a.reset()` |
  | `BackHandler.addEventListener("hardwareBackPress", f)`, `.remove()` | `cx.on_back_press(|this, cx| -> bool)` (in `setup`, kept in a field) returns a `BackSubscription`; dropping it removes the listener. After modals and before stacks, the components' listeners run newest first, then JS's; when none returns true the core sends `Command::Back`: the top stack pops, else the app exits (`ui/back.rs`) |
  | an error boundary (TS has none) | `fn catch_error(&mut self, error, cx) -> bool`: the nearest component above one whose code panicked (render, listener, method, task) handles it, typically keeping `error`, rendering a fallback and clearing it to try again; the failed one renders nothing until it renders again; unhandled errors are logged (`js` target) |
  | a portal | none (TS has none either); overlays are `modal().transparent(true)` |
  | `ref={el}`, `el.focus()`, `await el.measure()` | `cx.element_ref()` (in `setup`, kept in a field), `.element_ref(&self.field)` on the element each render, `self.field.focus()` / `.scroll_to(..)` / `.seek(..)` / `.command(..)` from listeners and tasks; `self.field.measure()` is an `Answer<Option<Measurement>>` (frame, `page_x` / `page_y`) to await in a task |
  | `onStartShouldSetResponder`, `onMoveShouldSetResponder(Capture)`, `PanResponder.create` | `.claim_touches(TouchClaim::on_start())` (`::on_move()`, `.capture()`, `.ask(cx.listener(\|this, request: &ClaimRequest, cx\| request.claim()))` to decide with state and the gesture, `.block_native(false)`; `TouchClaim::on_pan(PanActivation::horizontal(10.0, 10.0))` (`panActivation`) claims when the platform activates a pan, deciding on the main thread, `elements.md`), `.keep_touches(true)` (`onResponderTerminationRequest` → false); `.on_responder_grant` / `_move` / `_release` / `_terminate` / `_start` / `_end` / `_reject` get a `ResponderEvent` (the finger's `location_x`, `page_x`, ... and `gesture: PanGesture`: `dx`, `dy`, `vx`, `vy`, the centroid; `PanGesture::begin` / `update` follow one from `on_touch`) |
  | an object with methods handed elsewhere (`nav.push(() => <Detail market={market} />)`, `market.stop()`) | `cx.weak()` (a `WeakComponent<Self>`, what its tasks get) kept by the other side (in a global, say), which calls `owner.update(\|this, cx\| this.stop(cx))`: applied before the next render, `None` once it is gone |
  | a ref to a component, `await stepper.value()`, `stepper.setValue(5)` | `cx.component_ref::<Stepper>()` (in `setup`, kept in a field), `Stepper::element(..).component_ref(&self.field)`, `self.field.update(\|s, cx\| s.set_value(5.0, cx))`: its `#[methods]` called directly, right after the listener; await the `Updated` for the result |
  | `<List ListHeaderComponent ListFooterComponent ListEmptyComponent ItemSeparatorComponent stickyHeaderIndices viewabilityConfig onViewableItemsChanged onEndReachedThreshold initialScrollIndex>` | `virtual_list(n, row).header(..).footer(..).empty(..).separator(\|\| ..).sticky_header_indices(..).viewability(..).minimum_view_time(..).on_viewable_items_changed(..).end_reached_threshold(..).initial_scroll_index(..)`: the header, footer and empty view are part of the render (the component's state and handlers), never recycled |
  | `<List maintainVisibleContentPosition={{ autoscrollToBottomThreshold: 40 }}>` | `.follow_end(40.0)` (`ListLayout`: on `virtual_list(..)` and on a placed `ListElement`) |
  | `<ListView each={rows} keyExtractor ListHeaderComponent={Header}>{row}</ListView>` | `ListView::element(props).items(virtual_list(n, row).keys(ids).header(..))` |
  | `<Text>a <Text style={link} onPress={f}>b</Text> <View/></Text>` | `text("a ").child(link("b").on_press(..)).child(" ").child(view())`: one paragraph, links and inline views |
  | `class="group"`, `group-active:`, `dark:`, `disabled:` | `.group()`, `.group_active(\|s\| ..)`, `.dark(\|s\| ..)`, `.disabled_style(\|s\| ..)` (native, no render) |
  | `bg-sky-500` (the palette) | `.bg(palette::SKY.shade(500))` |

  `render(&self)` cannot change state. Timers are tasks: a `tokio::time` sleep, then an update.
- **Platform services** are on `Context` (setup, listeners, updates, methods), `ModuleCx` and `AsyncCx`:
  `cx.dialog(Dialog)` shows an alert or action sheet and returns a future of the index chosen (-1: dismissed; the
  answer is the root's `Dialog` event, which the core resolves; JS never sees it);
  `dismiss_keyboard()`, `animate_layout(..)`, `announce_for_accessibility(..)`, `toast(..)`, `force_rtl` /
  `allow_rtl`, the clipboard (`set_clipboard`; reads `clipboard_string` / `clipboard_has_string` on `Context` and
  `AsyncCx`), the image information (`image_size`, `prefetch_image`: on `Context` and `AsyncCx`), display frames
  (`Context::request_animation_frame`, `AsyncCx::next_frame`: the core keeps the waiting tasks' answers, the engine
  asks the platform for a frame while one waits, `MountSink::request_frame`, and delivers it at
  `EngineControl::frame`), and the status bar (below); they are actions the core applies in order with the
  component's other changes (`ui/services.rs`, `core/services.rs`). `cx.update_global::<G, _>(|g, cx| ..)` is
  state the surface's components and modules share.
- **Native values** (`ui/animated.rs`) are nodes of the platform's animation graph with odd ids (JS's are even,
  like node ids: the graph indexes nodes by id, so both must stay dense; ids from 2^31 are the graph's own
  composite steps); the last handle released frees the node. Deriving the same node again (a render calling
  `interpolate` each time) gets the live one, so bindings stay as they are. Bound style properties go into the
  element's inline style, before the app's `style` on a host (the core keeps the app's and composes the two). The
  animations' ends come back through `Core::animations_finished` (the engine hands JS the rest). An `Animation` is
  sent whole as one `AnimationOp::StartComposite` and runs in the graph step after step; its composite key (odd, JS's
  even) is what `reset()` restores by, and dropping the last handle releases it (a run going on goes on).
- **Element refs** (`ui/element_ref.rs`) hold the node of the element that carries them, set when the render is
  applied and cleared when the element goes; their commands go out through the executor, and an event's handling
  applies them right after its own changes, so they keep their order. A measurement (`measure()`) waits in the core
  for the layout of the commit it goes out with, as JS's `Command::Measure` does, and is answered there
  (`Core::measure`); none goes to the platform.
- **Element attributes for system UI**: `.inset_padding(insets)` / `.inset_margin(insets)` add to the padding /
  margin the styles resolve to (what `SafeAreaView` and `KeyboardAvoidingView` use, so the app's own padding stays),
  `.inline_style(&style)` applies a `ui::Style` prop (`KeyboardAvoidingView`'s `contentContainerStyle`).

## Methods

`#[methods]` on an `impl` block makes its functions methods of the component's ref in JS, with no TS written:
`const METHODS: &'static [Method<Self>] = Self::JS_METHODS;` lists them, the JS face builds the ref (`StepperRef`: `node`
and the methods, typed from the Rust signatures and docs). A call is a `Command::Native { name, args: { c, a } }` op,
so it runs in order with the props JS set before it. A method returning nothing is called and forgotten (`c` 0); one
returning a `JsValue` gives JS a `Promise`, answered by a `$return` event (`{ c, v }` or `{ c, e }` for a refused
argument) in the same frame's commit rounds. Methods (like listeners, timers and tasks' updates) read the current props
with `cx.props()`.

## Modules

Functions a UI set exports without a component (`Alert.alert`, `Keyboard.dismiss`, `StatusBar.setHidden`) are a
`#[module]` `impl` block of a unit struct: `pub fn`s are the functions, typed from their signatures (`Option`
parameters at the end may be left out), with an optional last `cx: &mut ModuleCx` parameter. One returning a
`JsValue` returns a `Promise` in JS; an `async fn` runs as a root task with an `AsyncCx` (`Alert.alert` awaits
`cx.dialog(..)`). `Registry::module::<M>()` registers it. A call is a root `Command::Native { name: "Alert.alert", args:
{ c, a } }` op (in order with the ops before it), answered like methods by a root `$return` event; `components.rs`
builds the object. A module with a component's name is merged into it (`StatusBar` is a component with static
functions). `Registry::reexport` re-exports runtime values (`safeArea`, `keyboard`, `useSafeAreaInsets`: `nsh-js`
`env.rs`) under the set's module, and `Registry::native_values` names the values `nsh-js` makes for the set (the
mobile UI's navigation: `navigation.rs`).

The runtime has modules of its own the same way (`crates/nsh-core/src/runtime/`, `runtime::registries()`): one
`Registry` per JS module (`Registry::runtime("nsh/android")`), whose functions JS calls as `nsh/android:Toast.show` so
they never clash with a UI set's; the core looks a call up in the set's registry, then the runtime's. Runtime modules
use the same services as components (`cx.toast`, `cx.announce_for_accessibility`, `cx.image_size`), and the core's
internal actions for the rest (exiting the app, a ref's accessibility focus).
`#[module(flat)]` exports functions one by one (`setStatusBarBackgroundColor`); a function returning `Result` rejects
the promise with the error; a `NodeRef` parameter is a `YNode` in TS (sent as `{ $node: id }`).

## The status bar

`ui/status_bar.rs` layers the surface's settings (`StatusBarSetting`: style, hidden, animated, background, each
optional). At the bottom is what functions set: `ModuleCx::set_status_bar(patch)`, `AsyncCx::set_status_bar`, and JS's
root `statusBar` command (`nsh/android`'s `setStatusBarBackgroundColor`) all become `Command::SetStatusBar(patch)`.
Over it, the settings components add (`cx.add_status_bar(setting)` → `StatusBarEntry`, `update_status_bar`,
`remove_status_bar`); the most recently added wins, field by field. Each change only marks the layers; `commit` sends
one `Command::StatusBar` with the merged result when it differs from the last one sent.

## Tasks

`ui/task.rs`: a small executor of the core runs components' futures on the engine thread, so they may hold the
component's `!Send` state. Each poll runs inside a tokio runtime's context (`Core::set_task_runtime`; the engine's
`EngineConfig::runtime`, else one runtime of two threads shared by the process), so the tokio ecosystem works in a
task: `tokio::time` timers, network and file IO, channels; `cx.background(fut)` / `cx.blocking(f)` run `Send` work on
the runtime's threads. A waker may fire on any thread: it sends the engine a `Wake` message, and the loop runs the woken
tasks (`Core::run_component_tasks`). `this.update(f)` does not touch the component during the poll: the core applies
the queued updates after it (with a `Context`, so they can `notify`, emit, spawn), then the tasks waiting for their
results run on; it resolves to `None` when the component is gone. A `Task` dropped cancels its future; a released
component cancels all of its own.

## Environment

The engine hands the core what components may read (`ui::Environment`: window, safe area, keyboard height, appearance,
font scale, reduced motion, screen reader, direction, pixel ratio, OS, its version, idiom, locale). A render records what it
read (`RenderCx::window()` and the like); when the environment changes, only the components that read a changed part
render again. The direction components read is the surface's: its root's (`Prop::LayoutDirection`, which JS's
`I18nManager` sets), else the platform's; once a component sets a policy (`force_rtl` / `allow_rtl`), and from the
start in a Rust app, the core sets the root's direction from it and the language (in an app whose JS sets it too,
the last one wins; JS's `isRTL` does not hear Rust's).

## Assets

A crate ships files in its `assets/` directory, as TS imports them: `asset!("logo@2x.png")` (`nsh-macros`, the path
relative to `assets/`) reads the file at compile time and is an `ImageAsset { uri, width, height }` (the natural size
in points: `@2x` / `@3x` divide the pixels), a `FontAsset { uri, family }` (`.ttf`, `.otf`: the family from the
font's name table) or a `FileAsset { uri }`; a missing or unreadable file fails the build, a changed one builds the
crate again (`include_bytes!`). The URI is `asset:/@<package>/<path>`, apart from the app's own files. The registry
ships the directory: `registry.assets(assets!())` lists every file (`AssetFile`: URI, the file where the crate was
built, kind), which the app's command line (`rust_app.rs`, the registry is in-process) serves in development and
copies into packages next to the imported assets, and whose fonts the engine registers before the first render
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
(`ui::palette::SKY.shade(500)`). A render records declarations
(`StyleRefinement`); the core lowers them into the same rules Tailwind classes compile to (`ClassList`), cached and
shared between elements with the same styles, and only when an element's styles changed. There is no string to
parse and no `.class()`.

- Theme colors are `AdaptiveColor`s (a light and a dark value) and lower into one declaration that picks by
  appearance (`Decl::Adaptive`): the platform's appearance switch restyles natively, without a render, and the app's
  plain classes still win over it. The appearance is the app's own when it set one (`nsh.json` `"appearance"`, TS
  `setColorScheme`, `cx.set_color_scheme`): the core keeps the system's and resolves `dark` for classes and
  `RenderCx::dark` from both (`Core::set_appearance`), and tells the platform (`MountItem::Appearance`: iOS
  `overrideUserInterfaceStyle` on the root view, presented modals and alerts, the status bar's default style; Android
  the status bar's default style and alert themes; native widgets elsewhere keep the system's).
- `.active(|s| ..)`, `.hover(..)`, `.focus(..)`, `.disabled_style(..)` lower into rules on the view's interaction
  states (Tailwind's ranks): press feedback is native. `.group()` makes a view a group and `.group_active(|s| ..)`
  (`_hover`, `_focus_within`, `_named` for `group/name`) styles what is inside it by the group's states;
  `.dark(|s| ..)` styles what is not a color in dark appearance. Nested variants all have to hold.
- What a render knows (a variant, `disabled`) is plain Rust: `.when(props.disabled, |v| v.opacity(0.5))`.
- The app's `class` / `classList` on a component (Tailwind) is merged after the root's rules; at equal rank it wins.
  The styles of a nested component element merge the same way.

## The theme

`Theme` (`ui/theme.rs`) has gpui-kit's semantic colors (`primary`, `primary_active`, `primary_foreground`,
`secondary`, `danger`, `muted`, `border`, `group_box`, `list_*`, `switch*`, `tab*`, ...: the fields mobile components
use) plus `radius`, `radius_lg` and `font_size`, and derefs to its colors (`theme.primary`). The default
(`default-theme.json`) is gpui-kit's default theme with mobile radii. An app sets its own in `nsh.json`: `"theme":
"brand.json"` (a gpui-kit theme file, relative to `nsh.json`) or the same JSON inline. Keys only desktop components
use are ignored; a pressed color the theme leaves out is derived from its base; null colors (gpui-kit serializes unset
ones so) are left as they are; colors are CSS colors or Tailwind palette names. The bundler checks the theme; the entry
hands it to the renderer (`nsh:rt` `setup`), which sends it to the root (`uiTheme`) before anything renders; a new
theme renders every component again. An embedding app can give a surface its own theme (`Core::set_host_theme`,
`EngineControl::set_theme`; sent from `Engine::start_with`'s `setup` it is in the first frame): it wins over the app's
until cleared, so an embedded app looks like its host (`embedding.md`, "GPUI and GPUI Kit apps").

## The host node and ids

In JS a component is a view (the host element `view`) with a `component` prop naming it (its registered name,
`nsh_ui_mobile::button::Button`): its **host**. `Core::apply` sees the
prop, creates an instance and from then on intercepts the ops on the host (`intercept`): the host is the root of the
component's expansion, the elements its render returns below the root become nodes the core creates itself. Apps
without components pay one check per op.

JS allocates even node ids (from 2), the core odd ones (from 3), so both id spaces stay dense and never collide. JS
never sees the expanded nodes; their events are the component's (below).

## Renders and diffing

- A prop change marks the instance; **renders run at the start of `commit`**, so props set in one JS turn render once.
  A component that keeps asking for renders is cut off after 16 per commit.
- The render is diffed against the previous one: keyed elements match by key; unkeyed ones take the next unused old
  element of the same kind in order, so an element that comes and goes (`.children(option)`) does not move its
  siblings onto new nodes. Attributes are compared field by field (`Attrs`), styles as a whole; native children are
  moved only when their order changed.
- An unknown name logs a denial and renders nothing.

## Children, slots and rows

- JS inserts a component's children into the host; the core places them where the render's `Children` is, before the
  rendered node that follows it (or keeps them out of the tree while the render has none). With `children_each`, a
  change of the children renders the component again: one wrapper per child, keyed by the child's node, so a child
  added or removed leaves the others' wrappers in place.
- Rows: the JS face makes a `List` of the app's data and row renderer and hands it to the host as the slot of the
  `Items` prop. The render places it like a slot and sends its layout (`ListConfig`: estimated size, columns,
  direction, padding, gaps). Each item cell of that list becomes a small instance whose render is the component's row
  function (`Inner::Row`): the wrapper goes into the cell, the row the app rendered into the wrapper. When the list
  rebinds a cell to another row (`cellIndex`), its wrapper renders again with the new position; so do all live cells
  when the number of rows changes (the last row moves), and when the component renders again. Binding, recycling, measuring and positioning stay the list's: a rebinding frame
  of `ListView` costs what one of `List` does (5 ops / 4 mount items in the gallery's Mobile UI demo).
- The component's own rows (`virtual_list(count, |index, at| row)`, layout from `ListLayout`, `.keys(ids)` for
  splices): the core creates the list and binds its cells itself, within the commit that needs them (`Core::commit`
  binds and lays out again; JS is not involved); each cell is a small instance (`Inner::VirtualRow`) rendering the
  row function, which runs again when the cell shows another row and when the component renders. With keys, rows
  removed or inserted before others leave their cells and sizes alone. Rows' handlers run with the component's
  state (also for the wrappers of the app's rows); a row passes its index with `listener.map(move |_: &()| index)`.
  The header, footer and empty view are cells of the render (their `CellSlot`s), outside the recycling; a
  separator goes into each row's cell, after the row (before it, inverted).
- Another component's rows (`ComponentElement::items(virtual_list(..))`): the list is content of the nested
  component, as `.child` is: a node of this render (its rows, header and handlers are this component's), the
  slot of the nested component's `Items` prop, which places and lays it out as it places JS's list (its layout
  and styles, so the list's own are left out). Each cell is then the placing component's row wrapper
  (`Inner::Row`) around a node rendering the row (`Inner::VirtualRow`), so each part's handlers run with its own
  component.
- Lazy children (`#[prop(lazy)] children: Children`, `Sheet`'s): the JS face makes the app's children only while the
  render places them, under a `cond` on a signal; the core tells it with a `$children` event (`true` / `false`) on
  the host when the render starts or stops placing them (`ComponentSpec::lazy_children`), so they are made again on
  each opening and released after. Until they are made, `Children::is_empty()` is true: a render that places them
  only when they are present never asks for them. The content a Rust render passes such a component
  (`Sheet::element(..).child(..)`) is lazy the same way: the core leaves it out (slots and rows stay) while the
  nested component does not place its children, and renders the passing component again when that changes, instead
  of telling JS.
- Named slots: the JS face creates one holder view per slot **the app passes** (`componentSlot`), filled with that prop's
  JSX. The render's `Slot` element *is* that holder: its styles and handlers apply to it, the core places it but never
  releases it (it is JS's). Filling or emptying a holder renders the component again.

## Events

- Handlers sit on the elements; the native event of an expanded node, or of the host, goes to the component first
  (`Core::component_event`, called by the engine before JS). Events of expanded nodes never reach JS.
- A slot's node (JS's holder, the app's list of an `Items` prop, or another component's content) gets the
  handlers of the component it is a slot of, then its maker's: JS's (the app's `onEndReached` on a `ListView`)
  or the rendering component's.
- A component's own list reports its viewable rows to it in the core, waiting for `minimum_view_time` when set
  (a task of the list's owner).
- `cx.emit(event)` sends a `NativeEvent` on the host (`{ name, data }`, from the `Event` derive); the JS face calls
  the app's `on<Name>(data)`. A nested component's events go to the `on_event` listener of the component that rendered it,
  typed, instead.

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

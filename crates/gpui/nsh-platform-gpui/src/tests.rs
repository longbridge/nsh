//! Tests in windows of GPUI's test platform (`TestAppContext`; feature
//! `shot`, which turns on GPUI's test support): a surface drawn by GPUI
//! and given GPUI's input, with the engine running on its own thread.

use std::{
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    AnyElement, App as GpuiApp, Entity, Focusable, IntoElement, Styled, TestAppContext,
    VisualTestContext, Window, div,
};
use nsh_core::ui::Root;

use crate::{App, AppOptions, AppView, Bundle, NativeComponent};

/// A native component that only draws a box.
struct Square;

impl NativeComponent for Square {
    fn render(&mut self, _: &mut Window, _: &mut GpuiApp) -> Option<AnyElement> {
        Some(div().size_full().into_any_element())
    }

    fn set_prop(
        &mut self,
        _: &str,
        _: Option<&serde_json::Value>,
        _: &mut Window,
        _: &mut GpuiApp,
    ) {
    }
}

/// A component and, after it, a focused field.
const FIELD_AFTER_COMPONENT: &str = r#"
import { Input, mount, signal, View } from "nsh";
import { nativeComponent } from "nsh/native";

const Square = nativeComponent<{}, {}>("test.square", { props: [], events: [] });

function App() {
  const text = signal("abcd");
  return (
    <View style={{ flex: 1, padding: 8, gap: 8 }}>
      <Square style={{ width: 40, height: 20 }} />
      <Input autoFocus value={text()} onChange={(t) => text.set(t)} style={{ height: 30 }} />
    </View>
  );
}
mount(App);
"#;

/// A focused field that logs its input method's compositions and sets
/// values while they are in progress: "held" on "ni" (applied once the
/// composition is cancelled), "stale" on "hao" (dropped, as "好" is
/// committed after it); the log becomes the text when "hao" ends.
const COMPOSING_FIELD: &str = r#"
import { Input, mount, signal, View } from "nsh";

function App() {
  const text = signal("abcd");
  const log: string[] = [];
  return (
    <View style={{ flex: 1, padding: 8 }}>
      <Input
        autoFocus
        value={text()}
        onChange={(t) => {
          log.push("change " + t);
          text.set(t);
        }}
        onCompositionChange={(e) => {
          log.push(e.composing ? "compose " + e.text : "end");
          if (e.text === "ni") text.set("held");
          if (e.text === "hao") text.set("stale");
          if (!e.composing && log.length > 2) text.set(log.join("|"));
        }}
        style={{ height: 30 }}
      />
    </View>
  );
}
mount(App);
"#;

/// A host view around a surface that holds the window's focus itself,
/// as an embedding app's own widgets do.
struct Host {
    view: Entity<AppView>,
    focus: gpui::FocusHandle,
}

impl gpui::Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        use gpui::{InteractiveElement as _, ParentElement as _};
        div()
            .size_full()
            .track_focus(&self.focus)
            .child(self.view.clone())
    }
}

/// The focused field's text.
fn field_text(view: &Entity<AppView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| {
        crate::text_input::focused(view.surface())
            .and_then(|field| crate::text_input::accessible_value(&field))
            .map(|value| value.text)
    })
}

/// The value the focused field holds until its composition ends.
fn field_pending(view: &Entity<AppView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, _| {
        crate::text_input::focused(view.surface())
            .and_then(|field| crate::text_input::pending_value(&field))
    })
}

/// Runs the window until the focused field holds `text` for after its
/// composition.
fn wait_for_pending(view: &Entity<AppView>, cx: &mut VisualTestContext, text: &str) {
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        let now = field_pending(view, cx);
        if now.as_deref() == Some(text) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the field holds {now:?}, not {text:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// An input method's step on the focused field.
fn ime(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&Rc<crate::surface::Surface>),
) {
    view.update(cx, |view, _| f(view.surface()));
}

/// Runs the window (and waits for the engine's thread) until the focused
/// field reads `text`.
fn wait_for(view: &Entity<AppView>, cx: &mut VisualTestContext, text: &str) {
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        let now = field_text(view, cx);
        if now.as_deref() == Some(text) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the field reads {now:?}, not {text:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The tests in windows, one after the other in one app: the main
/// thread's queue (`crate::rt`) is made once per process, for the first
/// app, so every window's engine must post to that app.
#[gpui::test]
fn windows(cx: &mut TestAppContext) {
    // The engine (and the settings portal) run on threads of their own.
    cx.executor().allow_parking();
    keys_reach_the_fields_of_a_surface_showing_a_native_component(cx);
    compositions_are_reported_and_hold_values_until_they_end(cx);
    presses_carry_where_the_pointer_was(cx);
    an_embedded_surface_takes_the_keyboard_when_its_app_focuses_a_field(cx);
    the_app_s_shortcuts_reach_it_while_a_native_component_has_the_keyboard(cx);
    js_and_rust_view_shortcuts_fire_together_in_either_order(cx);
    each_view_s_functions_read_the_context_its_options_gave_its_engine(cx);
}

fn keys_reach_the_fields_of_a_surface_showing_a_native_component(cx: &mut TestAppContext) {
    let code = nsh_bundler::compile_module("App.tsx", FIELD_AFTER_COMPONENT, &[]).unwrap();
    let (view, cx) =
        cx.add_window_view(|window, cx| AppView::new(AppOptions::default(), window, cx));
    view.update(cx, |view, _| {
        view.register_component("test.square", Rc::new(|_| Box::new(Square)));
        view.load(App::Js(Bundle::source(code)));
    });
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    wait_for(&view, cx, "abcd");
    // Painting the component's element must not take the surface's key
    // listeners with it: deletions and moves come as key events.
    cx.simulate_keystrokes("end backspace");
    wait_for(&view, cx, "abc");
    cx.simulate_keystrokes("home delete");
    wait_for(&view, cx, "bc");
    cx.simulate_keystrokes("right shift-right backspace");
    wait_for(&view, cx, "b");
}

fn compositions_are_reported_and_hold_values_until_they_end(cx: &mut TestAppContext) {
    let code = nsh_bundler::compile_module("App.tsx", COMPOSING_FIELD, &[]).unwrap();
    let (view, cx) =
        cx.add_window_view(|window, cx| AppView::new(AppOptions::default(), window, cx));
    view.update(cx, |view, _| view.load(App::Js(Bundle::source(code))));
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    wait_for(&view, cx, "abcd");
    cx.simulate_keystrokes("end");
    // The same composition twice is reported once; the value JS sets
    // meanwhile waits, the text keeps what the input method composes in.
    ime(&view, cx, |s| crate::text_input::mark(s, "ni"));
    ime(&view, cx, |s| crate::text_input::mark(s, "ni"));
    wait_for_pending(&view, cx, "held");
    assert_eq!(field_text(&view, cx).as_deref(), Some("abcd"));
    // Cancelled: the value held is applied as the composition ends.
    ime(&view, cx, |s| crate::text_input::mark(s, ""));
    assert_eq!(field_text(&view, cx).as_deref(), Some("held"));
    assert_eq!(field_pending(&view, cx), None);
    // Committed: the commit's `onChange` comes before the end, and the
    // value set before the commit is stale.
    ime(&view, cx, |s| crate::text_input::mark(s, "hao"));
    wait_for_pending(&view, cx, "stale");
    ime(&view, cx, |s| crate::text_input::char_input(s, "好"));
    assert_eq!(field_text(&view, cx).as_deref(), Some("held好"));
    wait_for(&view, cx, "compose ni|end|compose hao|change held好|end");
}

fn an_embedded_surface_takes_the_keyboard_when_its_app_focuses_a_field(cx: &mut TestAppContext) {
    let code = nsh_bundler::compile_module("App.tsx", FIELD_AFTER_COMPONENT, &[]).unwrap();
    use gpui::AppContext as _;
    let (host, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| AppView::new(AppOptions::default(), window, cx));
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Host { view, focus }
    });
    let view = host.read_with(cx, |host, _| host.view.clone());
    view.update(cx, |view, _| {
        view.register_component("test.square", Rc::new(|_| Box::new(Square)));
        view.load(App::Js(Bundle::source(code)));
    });
    // The app's autoFocus field. The window's focus stays on the host's
    // own element until the surface takes it.
    wait_for(&view, cx, "abcd");
    cx.simulate_keystrokes("end backspace");
    wait_for(&view, cx, "abc");
    // The host can take the keyboard while the surface keeps its focused field.
    let host_focus = host.read_with(cx, |host, _| host.focus.clone());
    cx.update(|window, cx| window.focus(&host_focus, cx));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        let field = crate::text_input::focused(view.surface()).unwrap();
        crate::text_input::focus(&field);
    });
    cx.run_until_parked();
    cx.update(|window, _| {
        assert!(
            !host_focus.is_focused(window),
            "focus() left the keyboard on the embedding host"
        );
    });
    cx.simulate_keystrokes("end backspace");
    wait_for(&view, cx, "ab");
}

struct KeyLog {
    keys: Rc<std::cell::RefCell<Vec<String>>>,
    focus: Rc<std::cell::RefCell<Option<gpui::FocusHandle>>>,
}

impl NativeComponent for KeyLog {
    fn render(&mut self, _: &mut Window, cx: &mut GpuiApp) -> Option<AnyElement> {
        use gpui::InteractiveElement as _;
        let focus = self
            .focus
            .borrow_mut()
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let keys = self.keys.clone();
        Some(
            div()
                .size_full()
                .track_focus(&focus)
                .on_key_down(move |e, _, _| keys.borrow_mut().push(e.keystroke.unparse()))
                .into_any_element(),
        )
    }

    fn focus_handle(&self) -> Option<gpui::FocusHandle> {
        self.focus.borrow().clone()
    }

    fn set_prop(
        &mut self,
        _: &str,
        _: Option<&serde_json::Value>,
        _: &mut Window,
        _: &mut GpuiApp,
    ) {
    }
}

/// A field showing which of the app's shortcuts fired, above a component
/// that takes the keyboard. `mod+k` fires while text is edited. `j` does
/// not. The menu's `mod+z` and `mod+shift+n` still choose their items.
const SHORTCUTS_OVER_COMPONENT: &str = r#"
import { Input, mount, signal, View } from "nsh";
import { Menu } from "nsh/desktop";
import { keymap } from "nsh/keymap";
import { nativeComponent } from "nsh/native";

const KeyLog = nativeComponent<{}, {}>("test.keylog", { props: [], events: [] });

function App() {
  const fired = signal("fired");
  const fire = (name: string) => () => fired.set(`${fired()} ${name}`);
  keymap.bind("mod+k", fire("bound"), { when: "always" });
  keymap.bind("j", fire("j"));
  Menu.setApplicationMenu([
    {
      label: "Edit",
      submenu: [
        { label: "Revert", shortcut: "mod+z", onSelect: fire("revert") },
        { label: "New", shortcut: "mod+shift+n", onSelect: fire("new") },
      ],
    },
  ]);
  return (
    <View style={{ flex: 1 }}>
      <Input value={fired()} style={{ height: 30 }} />
      <KeyLog style={{ height: 40 }} />
    </View>
  );
}
mount(App);
"#;

fn the_app_s_shortcuts_reach_it_while_a_native_component_has_the_keyboard(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    let code = nsh_bundler::compile_module("App.tsx", SHORTCUTS_OVER_COMPONENT, &[]).unwrap();
    let keys = Rc::new(RefCell::new(Vec::new()));
    let focus = Rc::new(RefCell::new(None));
    let (component_keys, component_focus) = (keys.clone(), focus.clone());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = AppView::new(AppOptions::default(), window, cx);
        let title = window.window_title();
        crate::chrome::set_standalone(view.surface(), &title, window.window_handle());
        view
    });
    view.update(cx, |view, _| {
        view.register_component(
            "test.keylog",
            Rc::new(move |_| {
                Box::new(KeyLog {
                    keys: component_keys.clone(),
                    focus: component_focus.clone(),
                })
            }),
        );
        view.load(App::Js(Bundle::source(code)));
    });
    let start = Instant::now();
    let handle = loop {
        cx.run_until_parked();
        if let Some(handle) = focus.borrow().clone() {
            break handle;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the component is not drawn"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    cx.simulate_keystrokes(&format!("a j {primary}-z {primary}-k"));
    // GPUI's test window sends no releases. K's release reaches the keymap
    // while the component has the focus, so the next Ctrl+K is not a repeat.
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse(&format!("{primary}-k")).unwrap(),
    });
    cx.simulate_keystrokes(&format!("{primary}-k shift-{primary}-n"));
    assert_eq!(
        keys.borrow()[..4],
        ["a", "j", &format!("{primary}-z"), &format!("{primary}-k")],
    );
    cx.update(|window, _| assert!(handle.is_focused(window), "the component lost the focus"));
    // The surface starts below the platform's title and menu bars.
    let (x, y) = view.read_with(cx, |view, _| view.surface().origin.get());
    cx.simulate_click(
        gpui::point(gpui::px(x + 20.0), gpui::px(y + 15.0)),
        gpui::Modifiers::default(),
    );
    wait_for(&view, cx, "fired revert bound bound new");
}
/// The part of the embedding app a view reaches: its engine's context.
struct Desk(&'static str);

/// Which desk the engine running the call was given.
#[nsh_core::ui::module]
struct Whose;

#[nsh_core::ui::module]
impl Whose {
    /// In the JS call.
    pub(crate) fn now() -> String {
        desk()
    }

    /// As a task, after it waited.
    pub(crate) async fn later() -> String {
        nsh_core::ui::prelude::timer(Duration::from_millis(5)).await;
        desk()
    }
}

fn desk() -> String {
    nsh_core::ui::engine_context::<Desk>()
        .map_or_else(|| "none".to_owned(), |desk| desk.0.to_owned())
}

/// A field showing which desk `Whose` names, in the call and in a task.
const WHOSE: &str = r#"
import { Input, mount, signal, View } from "nsh";
import { nativeModule } from "nsh/native";

interface WhoseApi {
  now(): string;
  later(): Promise<string>;
}
const { Whose } = nativeModule<{ Whose: WhoseApi }>("test/context");

function App() {
  const text = signal("waiting");
  setTimeout(async () => text.set(`${Whose.now()} ${await Whose.later()}`), 0);
  return (
    <View style={{ flex: 1 }}>
      <Input autoFocus value={text()} onChange={(t) => text.set(t)} style={{ height: 30 }} />
    </View>
  );
}
mount(App);
"#;

/// Two surfaces side by side in one window.
struct Pair {
    a: Entity<AppView>,
    b: Entity<AppView>,
}

impl gpui::Render for Pair {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        use gpui::ParentElement as _;
        div()
            .size_full()
            .flex()
            .child(div().flex_1().child(self.a.clone()))
            .child(div().flex_1().child(self.b.clone()))
    }
}

fn each_view_s_functions_read_the_context_its_options_gave_its_engine(cx: &mut TestAppContext) {
    let mut context = nsh_core::ui::Registry::ui_set("test/context");
    context.module::<Whose>();
    crate::set_ui_sets(vec![nsh_base::registry(), context]);
    let code = nsh_bundler::compile_module("App.tsx", WHOSE, &[]).unwrap();
    use gpui::AppContext as _;
    let (pair, cx) = cx.add_window_view(|window, cx| {
        let view = |desk: &'static str, window: &mut Window, cx: &mut gpui::Context<Pair>| {
            let options = AppOptions {
                context: nsh_engine::EngineContext::default().with(Desk(desk)),
                ..AppOptions::default()
            };
            cx.new(|cx| AppView::new(options, window, cx))
        };
        Pair {
            a: view("A", window, cx),
            b: view("B", window, cx),
        }
    });
    let (a, b) = pair.read_with(cx, |pair, _| (pair.a.clone(), pair.b.clone()));
    for view in [&a, &b] {
        view.update(cx, |view, _| {
            view.load(App::Js(Bundle::source(code.clone())))
        });
    }
    wait_for(&a, cx, "A A");
    wait_for(&b, cx, "B B");
}

/// A Rust view (`test/keys`) that binds a keystroke of its own.
mod rust_keys {
    use nsh_core::ui::{Registry, prelude::*};

    /// The set the app imports `Shortcut` from.
    pub(super) fn registry() -> Registry {
        let mut registry = Registry::ui_set("test/keys");
        registry.register::<Shortcut>();
        registry
    }

    #[derive(Debug, Clone, PartialEq, Props)]
    pub(super) struct ShortcutProps {
        /// The keystroke, as `on_key` takes it.
        pub(super) keys: Prop<SharedString>,
    }

    #[derive(Debug, Clone, PartialEq, Event)]
    pub(super) enum ShortcutEvent {
        /// The keystroke was pressed.
        Fired,
    }

    /// Binds `keys` (in text fields too) and reports each press.
    #[component]
    pub(super) fn Shortcut(props: ShortcutProps, cx: Scope<ShortcutEvent>) -> impl IntoElement {
        let options = KeyOptions {
            when: nsh_core::keymap::When::Always,
            ..KeyOptions::default()
        };
        on_key_with(&props.keys.get(), options, move |_| {
            cx.emit(ShortcutEvent::Fired)
        });
        view()
    }
}

/// A field logging Escape (JS's binding), Shift+Enter (a Rust view's) and
/// Mod+T (JS's, which shows and hides the view). `view_first` shows the
/// view from the start and binds Escape once it mounted. Otherwise Escape
/// is bound before any Rust view exists.
fn js_and_rust_view(view_first: bool) -> String {
    format!(
        r#"
import {{ Input, mount, signal, View }} from "nsh";
import {{ keymap }} from "nsh/keymap";
import {{ Shortcut }} from "test/keys";

const VIEW_FIRST = {view_first};

function App() {{
  const fired = signal("fired");
  const shown = signal(VIEW_FIRST);
  const fire = (name: string) => fired.set(`${{fired()}} ${{name}}`);
  const escape = () => keymap.bind("escape", () => fire("escape"), {{ when: "always" }});
  keymap.bind("mod+t", () => {{ shown.set(!shown()); fire(shown() ? "shown" : "hidden"); }}, {{ when: "always" }});
  if (VIEW_FIRST) setTimeout(() => {{ escape(); fire("bound"); }}, 0); else escape();
  return (
    <View style={{{{ flex: 1 }}}}>
      <Input autoFocus value={{fired()}} style={{{{ height: 30 }}}} />
      <If when={{shown()}}>
        <Shortcut keys="shift+enter" onFired={{() => fire("shift")}} style={{{{ height: 10 }}}} />
      </If>
    </View>
  );
}}
mount(App);
"#
    )
}

fn js_and_rust_view_shortcuts_fire_together_in_either_order(cx: &mut TestAppContext) {
    crate::set_ui_sets(vec![nsh_base::registry(), rust_keys::registry()]);
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let sets = [rust_keys::registry().into()];

    let code = nsh_bundler::compile_module("App.tsx", &js_and_rust_view(false), &sets).unwrap();
    let (view, window) =
        cx.add_window_view(|window, cx| AppView::new(AppOptions::default(), window, cx));
    view.update(window, |view, _| view.load(App::Js(Bundle::source(code))));
    window.update(|w, cx| w.focus(&view.focus_handle(cx), cx));
    wait_for(&view, window, "fired");
    window.simulate_keystrokes(&format!("{primary}-t"));
    wait_for(&view, window, "fired shown");
    window.simulate_keystrokes("escape shift-enter");
    wait_for(&view, window, "fired shown escape shift");
    // The view goes, and only its binding with it.
    window.simulate_keystrokes(&format!("{primary}-t"));
    wait_for(&view, window, "fired shown escape shift hidden");
    window.simulate_keystrokes("escape");
    wait_for(&view, window, "fired shown escape shift hidden escape");

    let code = nsh_bundler::compile_module("App.tsx", &js_and_rust_view(true), &sets).unwrap();
    let (view, window) =
        cx.add_window_view(|window, cx| AppView::new(AppOptions::default(), window, cx));
    view.update(window, |view, _| view.load(App::Js(Bundle::source(code))));
    window.update(|w, cx| w.focus(&view.focus_handle(cx), cx));
    wait_for(&view, window, "fired bound");
    window.simulate_keystrokes("escape shift-enter");
    wait_for(&view, window, "fired bound escape shift");
    window.simulate_keystrokes(&format!("{primary}-t"));
    wait_for(&view, window, "fired bound escape shift hidden");
    window.simulate_keystrokes("escape");
    wait_for(&view, window, "fired bound escape shift hidden escape");
}

/// A pad inside 8 points of padding. A press writes where it was in the
/// pad and in the window.
mod press_point {
    use nsh_core::ui::prelude::*;

    #[component]
    pub(super) fn PressPoint() -> impl IntoElement {
        let report = signal(String::from("none"));
        view()
            .p(px(8.0))
            .child(
                view()
                    .w(px(80.0))
                    .h(px(40.0))
                    .on(EventKind::Press, move |p: &EventPayload| {
                        let EventPayload::Press(e) = p else {
                            report.set("no position".into());
                            return;
                        };
                        report.set(format!(
                            "at {:.0} {:.0} in {:.0} {:.0}",
                            e.position.x.0,
                            e.position.y.0,
                            e.page_position.x.0,
                            e.page_position.y.0
                        ));
                    }),
            )
            .child(
                input()
                    .auto_focus(true)
                    .value(move || report.get())
                    .h(px(30.0)),
            )
    }
}

fn presses_carry_where_the_pointer_was(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|window, cx| AppView::new(AppOptions::default(), window, cx));
    view.update(cx, |view, _| {
        view.load(App::Rust(Root::of::<press_point::PressPoint>()))
    });
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    wait_for(&view, cx, "none");
    cx.simulate_click(
        gpui::point(gpui::px(18.0), gpui::px(20.0)),
        gpui::Modifiers::default(),
    );
    wait_for(&view, cx, "at 10 12 in 18 20");
}

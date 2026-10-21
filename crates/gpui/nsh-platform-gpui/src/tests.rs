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
    js_and_rust_view_shortcuts_fire_together_in_either_order(cx);
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

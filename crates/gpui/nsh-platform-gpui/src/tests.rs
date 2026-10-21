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

use crate::{App, AppOptions, AppView, Bundle, NativeComponent, Root};

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
    presses_carry_where_the_pointer_was(cx);
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

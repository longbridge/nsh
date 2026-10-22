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
    an_embedded_view_that_owns_its_window_sets_its_title_and_menu(cx);
    quit_and_close_requests_go_to_the_handler_instead_of_acting(cx);
    an_auto_hidden_menu_bar_shows_on_alt_alone_and_while_a_menu_is_open(cx);
    asset_uris_resolve_in_the_first_directory_that_has_the_file(cx);
    releasing_shift_before_alt_keeps_the_auto_hidden_menu_bar_hidden(cx);
}

/// Runs the window until the focused field's text satisfies `done`, and
/// returns it.
fn wait_until(
    view: &Entity<AppView>,
    cx: &mut VisualTestContext,
    done: impl Fn(&str) -> bool,
) -> String {
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        let now = field_text(view, cx).unwrap_or_default();
        if done(&now) {
            return now;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the field reads {now:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
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
/// A view the window's root holds, as an embedding app's frame does.
struct Framed(Entity<AppView>);

impl gpui::Render for Framed {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        use gpui::ParentElement as _;
        div().size_full().child(self.0.clone())
    }
}

/// Opens a window whose root holds an [`AppView`] that owns the window,
/// running `code`, with the keyboard in the view.
fn owned_view(
    cx: &mut TestAppContext,
    code: String,
    setup: impl FnOnce(&AppView),
) -> (Entity<AppView>, &mut VisualTestContext) {
    use gpui::AppContext as _;
    let (framed, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let view = AppView::new(AppOptions::default(), window, cx);
            view.own_window(window, cx);
            view
        });
        Framed(view)
    });
    let view = framed.read_with(cx, |framed, _| framed.0.clone());
    view.update(cx, |view, _| {
        setup(view);
        view.load(App::Js(Bundle::source(code)));
    });
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    (view, cx)
}

/// Runs the window until `done` holds.
fn wait_until_window(
    cx: &mut VisualTestContext,
    what: &str,
    done: impl Fn(&mut VisualTestContext) -> bool,
) {
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "never: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The window's content height, in points.
fn viewport_height(cx: &mut VisualTestContext) -> f32 {
    cx.update(|window, _| f32::from(window.viewport_size().height))
}

/// A field that reads the app's height (`h <points>`, the window less the
/// chrome around the app) and ` new` once New was chosen, under a title
/// and an application menu: File with New, Quit (the `quit` role, with a
/// shortcut of its own) and Close (the `close` role).
const OWNED_WINDOW: &str = r#"
import { Input, mount, signal, View } from "nsh";
import { Menu, Window } from "nsh/desktop";

function App() {
  const height = signal(-1);
  const chosen = signal("");
  Window.setTitle("Owned");
  Menu.setApplicationMenu([
    {
      label: "File",
      submenu: [
        { label: "New", shortcut: "mod+shift+n", onSelect: () => chosen.set(" new") },
        { label: "Quit", role: "quit", shortcut: "mod+shift+q" },
        { label: "Close", role: "close", shortcut: "mod+shift+w" },
      ],
    },
  ]);
  return (
    <View style={{ flex: 1 }} onLayout={(e) => height.set(Math.round(e.size.height))}>
      <Input autoFocus value={`h ${height()}${chosen()}`} style={{ height: 30 }} />
    </View>
  );
}
mount(App);
"#;

/// The app's height the field reads.
fn app_height(view: &Entity<AppView>, cx: &mut VisualTestContext) -> f32 {
    let text = wait_until(view, cx, |t| t.starts_with("h ") && !t.starts_with("h -1"));
    text.split(' ').nth(1).and_then(|h| h.parse().ok()).unwrap()
}

fn an_embedded_view_that_owns_its_window_sets_its_title_and_menu(cx: &mut TestAppContext) {
    let code = nsh_bundler::compile_module("App.tsx", OWNED_WINDOW, &[]).unwrap();
    let (view, cx) = owned_view(cx, code, |_| {});
    wait_until_window(cx, "the app's title on the window", |cx| {
        cx.window_title().as_deref() == Some("Owned")
    });
    if cfg!(target_os = "linux") {
        // The menu bar is drawn above the app, 28 points tall.
        let window = viewport_height(cx);
        wait_until(&view, cx, |t| t == format!("h {}", window - 28.0));
    }
}

fn quit_and_close_requests_go_to_the_handler_instead_of_acting(cx: &mut TestAppContext) {
    use crate::CloseRequest::{Close, Quit};
    let code = nsh_bundler::compile_module("App.tsx", OWNED_WINDOW, &[]).unwrap();
    let asked = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = asked.clone();
    let (view, cx) = owned_view(cx, code, move |view| {
        view.on_close_request(move |request, _, _| log.borrow_mut().push(request))
    });
    app_height(&view, cx);
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let mut expected = Vec::new();
    for (keys, request) in [
        // The reserved quit key, Ctrl+W, and the menu's roles by their
        // shortcuts.
        (format!("{primary}-q"), Quit),
        (format!("{primary}-w"), Close),
        (format!("shift-{primary}-q"), Quit),
        (format!("shift-{primary}-w"), Close),
    ] {
        cx.simulate_keystrokes(&keys);
        expected.push(request);
        wait_until_window(cx, &format!("{keys} asks {request:?}"), |_| {
            *asked.borrow() == expected
        });
    }
    // The window's close button: the window stays.
    assert!(!cx.simulate_close(), "the close button closed the window");
    expected.push(Close);
    wait_until_window(cx, "the close button asks Close", |_| {
        *asked.borrow() == expected
    });
    // Still open, still running: the menu's other items work.
    let height = app_height(&view, cx);
    cx.simulate_keystrokes(&format!("shift-{primary}-n"));
    wait_for(&view, cx, &format!("h {height} new"));
}

/// The app's height under an auto-hidden menu bar (and ` new` once File
/// > New was chosen).
const AUTO_HIDDEN_BAR: &str = r#"
import { Input, mount, signal, View } from "nsh";
import { Menu, Window } from "nsh/desktop";

function App() {
  const height = signal(-1);
  const chosen = signal("");
  Window.setMenuBar("autoHide");
  Menu.setApplicationMenu([
    { label: "File", submenu: [{ label: "New", onSelect: () => chosen.set(" new") }] },
  ]);
  return (
    <View style={{ flex: 1 }} onLayout={(e) => height.set(Math.round(e.size.height))}>
      <Input autoFocus value={`h ${height()}${chosen()}`} style={{ height: 30 }} />
    </View>
  );
}
mount(App);
"#;

fn an_auto_hidden_menu_bar_shows_on_alt_alone_and_while_a_menu_is_open(cx: &mut TestAppContext) {
    // macOS's menu bar is the system's.
    if !cfg!(target_os = "linux") && !cfg!(windows) {
        return;
    }
    let code = nsh_bundler::compile_module("App.tsx", AUTO_HIDDEN_BAR, &[]).unwrap();
    let (view, cx) = owned_view(cx, code, |_| {});
    let full = viewport_height(cx);
    let (hidden, shown) = (format!("h {full}"), format!("h {}", full - 28.0));
    wait_for(&view, cx, &hidden);
    let alt_alone = |cx: &mut VisualTestContext| {
        cx.simulate_modifiers_change(gpui::Modifiers::alt());
        cx.simulate_modifiers_change(gpui::Modifiers::none());
    };
    alt_alone(cx);
    wait_for(&view, cx, &shown);
    alt_alone(cx);
    wait_for(&view, cx, &hidden);
    // Alt with a key or another modifier is a shortcut: Alt + F opens File
    // (the bar shows while it is open), and the bar hides again as it
    // closes.
    cx.simulate_modifiers_change(gpui::Modifiers::alt());
    cx.simulate_keystrokes("alt-f");
    cx.simulate_modifiers_change(gpui::Modifiers::none());
    wait_for(&view, cx, &shown);
    cx.simulate_keystrokes("escape");
    wait_for(&view, cx, &hidden);
    cx.simulate_modifiers_change(gpui::Modifiers::alt());
    cx.simulate_modifiers_change(gpui::Modifiers {
        alt: true,
        shift: true,
        ..Default::default()
    });
    cx.simulate_modifiers_change(gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        field_text(&view, cx),
        Some(hidden.clone()),
        "Alt + Shift showed the bar"
    );
    // F10 opens the first menu; Return chooses its first item, and the bar
    // hides again.
    cx.simulate_keystrokes("f10");
    wait_for(&view, cx, &shown);
    cx.simulate_keystrokes("enter");
    wait_for(&view, cx, &format!("{hidden} new"));
}

/// Two images from two asset directories: each reports its size, or why
/// it did not load.
const TWO_ASSET_ROOTS: &str = r#"
import { Input, mount, signal, View } from "nsh";
import { Image } from "nsh/image";

function App() {
  const text = signal("waiting");
  const seen: string[] = [];
  const report = (name: string, what: string) => {
    seen.push(`${name} ${what}`);
    if (seen.length === 2) text.set(seen.sort().join("|"));
  };
  return (
    <View style={{ flex: 1 }}>
      <Image
        src="asset:/app.png"
        onLoad={(e) => report("app", `${e.width}x${e.height}`)}
        onError={(e) => report("app", e.message)}
        style={{ width: 10, height: 10 }}
      />
      <Image
        src="asset:/bytes/cover.png"
        onLoad={(e) => report("cover", `${e.width}x${e.height}`)}
        onError={(e) => report("cover", e.message)}
        style={{ width: 10, height: 10 }}
      />
      <Input autoFocus value={text()} onChange={(t) => text.set(t)} style={{ height: 30 }} />
    </View>
  );
}
mount(App);
"#;

fn asset_uris_resolve_in_the_first_directory_that_has_the_file(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("nsh-asset-roots-{}", std::process::id()));
    let (app, archive) = (root.join("app"), root.join("archive"));
    std::fs::create_dir_all(archive.join("bytes")).unwrap();
    std::fs::create_dir_all(&app).unwrap();
    let png = |path: std::path::PathBuf, w: u32, h: u32| {
        image::RgbaImage::new(w, h).save(path).unwrap();
    };
    png(app.join("app.png"), 2, 3);
    png(archive.join("bytes/cover.png"), 4, 5);
    // The archive's own `app.png` is not the one shown: the app's comes
    // first.
    png(archive.join("app.png"), 7, 7);
    let code = nsh_bundler::compile_module("App.tsx", TWO_ASSET_ROOTS, &[]).unwrap();
    let (view, cx) =
        cx.add_window_view(|window, cx| AppView::new(AppOptions::default(), window, cx));
    view.update(cx, |view, _| {
        view.set_asset_base(Some(crate::AssetBase::Directories(vec![app, archive])));
        view.load(App::Js(Bundle::source(code)));
    });
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    let report = wait_until(&view, cx, |t| t.contains('|'));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(report, "app 2x3|cover 4x5");
}

fn releasing_shift_before_alt_keeps_the_auto_hidden_menu_bar_hidden(cx: &mut TestAppContext) {
    if !cfg!(target_os = "linux") && !cfg!(windows) {
        return;
    }
    let code = nsh_bundler::compile_module("App.tsx", AUTO_HIDDEN_BAR, &[]).unwrap();
    let (view, cx) = owned_view(cx, code, |_| {});
    let full = viewport_height(cx);
    wait_for(&view, cx, &format!("h {full}"));
    for shift_first in [false, true] {
        cx.simulate_modifiers_change(if shift_first {
            gpui::Modifiers {
                shift: true,
                ..Default::default()
            }
        } else {
            gpui::Modifiers::alt()
        });
        cx.simulate_modifiers_change(gpui::Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        });
        cx.simulate_modifiers_change(gpui::Modifiers::alt());
        cx.simulate_modifiers_change(gpui::Modifiers::none());
        cx.run_until_parked();
        let height = view.read_with(cx, |view, _| view.surface().size().1);
        assert_eq!(
            height, full,
            "Alt+Shift showed the bar after Shift was released first (shift_first={shift_first})"
        );
    }
}

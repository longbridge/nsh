//! The OS the platform reports decides `platform` and which platform-only
//! props (`ios={{…}}`, `android={{…}}`, `desktop={{…}}`, `web={{…}}`)
//! apply; on the web, the idiom decides between mobile and desktop, and
//! the terminal idiom behaves as a desktop. `platform.desktop`, the
//! `desktop` idiom, `select`'s `desktop` and `desktop={{…}}` all follow the
//! one definition of desktop (`Environment::desktop`), whatever idiom the
//! platform set.

use std::time::Duration;

use nsh_core::{ApproxTextEngine, MountItem, Size, geometry::px};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment, Idiom, Os};

use crate::ChannelSink;

const APP: &str = r#"import { mount, Text, View } from "nsh";
import { platform } from "nsh";

mount(
  () => (
    <View
      ios={{ accessibilityLabel: "ios" }}
      android={{ accessibilityLabel: "android" }}
      desktop={{ accessibilityLabel: "desktop" }}
      web={{ accessibilityHint: "web" }}
    >
      <Text>{`${platform.os} ${platform.idiom} ${platform.desktop} ${platform.catalyst} ${platform.select({ ios: "i", mobile: "m", desktop: "d" })} ${platform.select({ native: "n", default: "-" })}`}</Text>
    </View>
  ),
);
"#;

/// The first frame's texts and accessibility labels and hints on `os`.
fn first_frame(os: Os, idiom: Idiom) -> (Vec<String>, Vec<String>) {
    let dir = std::env::temp_dir().join(format!(
        "nsh-platform-{}-{}-{idiom:?}",
        std::process::id(),
        os.name()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: vec![nsh_base::registry().into()],
    })
    .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: nsh_engine::App::Js(Bundle::source(bundle.code)),
        viewport: Size::new(px(320.0), px(480.0)),
        trace: false,
        network: None,
        websocket: None,
        refuse_connections: None,
        filesystem: None,
        assets: None,
        environment: Environment {
            os,
            idiom,
            ..Environment::default()
        },
        header_heights: (nsh_core::Points(44.0), nsh_core::Points(96.0)),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: None,
        cache: nsh_engine::CacheOptions::off(),
        storage: None,
        clock: Default::default(),
        context: Default::default(),
        pinned: Default::default(),
        clipboard: None,
        host: Default::default(),
        picker: None,
        speech: None,
    };
    let _engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let batch = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let mut texts = Vec::new();
    let mut labels = Vec::new();
    for item in &batch.items {
        match item {
            MountItem::Text { text, .. } => texts.push(text.text.clone()),
            MountItem::ViewProps { props, .. } => {
                labels.extend(props.accessibility.label.as_deref().map(str::to_owned));
                labels.extend(props.accessibility.hint.as_deref().map(str::to_owned));
            }
            _ => {}
        }
    }
    (texts, labels)
}

#[test]
fn platform_props_and_select_follow_the_os() {
    let frame = |os, idiom, text: &str, labels: &[&str]| {
        assert_eq!(
            first_frame(os, idiom),
            (
                vec![text.to_owned()],
                labels.iter().map(|l| (*l).to_owned()).collect()
            ),
            "{os:?} {idiom:?}"
        );
    };
    frame(Os::Ios, Idiom::Phone, "ios phone false false i n", &["ios"]);
    frame(
        Os::Android,
        Idiom::Phone,
        "android phone false false m n",
        &["android"],
    );
    frame(
        Os::MacOs,
        Idiom::Desktop,
        "macos desktop true false d n",
        &["desktop"],
    );
    // A desktop OS is desktop whatever idiom it was given.
    frame(
        Os::Linux,
        Idiom::Phone,
        "linux desktop true false d n",
        &["desktop"],
    );
    // UIKit's Mac idiom (Mac Catalyst) is mobile: the iOS backend. This
    // build is not Catalyst's, so `catalyst` is false.
    frame(Os::Ios, Idiom::Mac, "ios mac false false i n", &["ios"]);
    // No mobile OS reports "desktop".
    frame(
        Os::Ios,
        Idiom::Desktop,
        "ios phone false false i n",
        &["ios"],
    );
    // `native` is not the web, as in React Native.
    frame(Os::Web, Idiom::Phone, "web phone false false m -", &["web"]);
    frame(Os::Web, Idiom::Pad, "web pad false false m -", &["web"]);
    // A desktop browser behaves as a desktop.
    frame(
        Os::Web,
        Idiom::Desktop,
        "web desktop true false d -",
        &["desktop", "web"],
    );
    // A terminal reports the real OS and behaves as a desktop, in an idiom
    // of its own.
    frame(
        Os::Linux,
        Idiom::Terminal,
        "linux terminal true false d n",
        &["desktop"],
    );
}

const WINDOW_APP: &str = r#"import { mount, Text } from "nsh";
import { Window, displays, windowState } from "nsh/desktop";

// Bounds saved on a display that is gone, given back before the first
// render: NSH fits them on the display there is.
Window.setBounds({
  mode: "maximized",
  bounds: { origin: { x: -3000, y: 0 }, size: { width: 800, height: 600 } },
});

// Read in the binding, which runs again when they change (the component's
// body runs once).
const shown = () => {
  const state = windowState();
  return state
    ? `${state.windowBounds.mode} ${state.bounds.origin.x} ${state.minimized} ${state.hidden} ${state.display?.id}`
    : "none";
};
const listed = () =>
  displays().length
    ? displays().map((d) => `${d.id} ${d.primary} ${d.visibleBounds.size.width}`).join(",")
    : "no displays";

mount(() => <Text>{`${shown()} | ${listed()}`}</Text>);
"#;

#[test]
fn js_reads_the_window_and_the_displays_and_restores_bounds() {
    use nsh_core::{
        Command,
        desktop::{DesktopCommand, Display, DisplayId, WindowBounds, WindowState},
        geometry::{Bounds, point, size},
    };
    let dir = std::env::temp_dir().join(format!("nsh-window-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, WINDOW_APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: vec![nsh_base::registry().into()],
    })
    .unwrap();
    let rect = |x: f32, w: f32, h: f32| Bounds::new(point(px(x), px(0.0)), size(px(w), px(h)));
    let display = Display::new(
        DisplayId(0xab),
        rect(0.0, 1440.0, 900.0),
        rect(0.0, 1440.0, 875.0),
        true,
    );
    let window = rect(100.0, 800.0, 600.0);
    let mut state = WindowState::new(window, WindowBounds::Windowed(window));
    state.display = Some(display);
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        environment: Environment {
            os: Os::MacOs,
            idiom: Idiom::Desktop,
            window_state: Some(state),
            ..Environment::default()
        },
        ..EngineConfig::new(nsh_engine::App::Js(Bundle::source(bundle.code)))
    };
    let engine = Engine::start_with(
        config,
        || ApproxTextEngine,
        ChannelSink(tx),
        |control| {
            control.set_displays(vec![display]);
        },
    )
    .unwrap();
    // The last text starting with `until`, and the bounds the app gave the
    // window until then.
    let mut texts = Vec::new();
    let mut read = |until: &str| -> (String, Vec<WindowBounds>) {
        let mut restored = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while let Ok(batch) = rx.recv_deadline(deadline) {
            for item in batch.items {
                match item {
                    MountItem::Text { text, .. } => texts.push(text.text.to_string()),
                    MountItem::Command {
                        command: Command::Desktop(command),
                        ..
                    } => {
                        if let DesktopCommand::SetBounds(bounds) = *command {
                            restored.push(bounds);
                        }
                    }
                    _ => {}
                }
            }
            if let Some(text) = texts.last().filter(|t| t.starts_with(until)) {
                return (text.clone(), restored);
            }
        }
        panic!("no text {until:?} in {texts:?}");
    };
    // The bounds are fitted on the display there is (centered, the mode
    // kept), and the window and displays are there as the app first
    // renders.
    let (text, restored) = read("windowed 100 false false 00000000000000ab");
    assert_eq!(
        text,
        "windowed 100 false false 00000000000000ab | 00000000000000ab true 1440"
    );
    assert_eq!(
        restored,
        vec![WindowBounds::Maximized(Bounds::new(
            point(px(320.0), px(137.5)),
            size(px(800.0), px(600.0))
        ))]
    );
    // Maximized: at once.
    let mut maximized = state;
    maximized.window_bounds = WindowBounds::Maximized(window);
    engine.control().set_environment(Environment {
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        window_state: Some(maximized),
        ..Environment::default()
    });
    read("maximized 100");
    // Off desktops: none.
    engine.control().set_environment(Environment {
        os: Os::MacOs,
        idiom: Idiom::Desktop,
        ..Environment::default()
    });
    engine.control().set_displays(Vec::new());
    read("none | no displays");
}

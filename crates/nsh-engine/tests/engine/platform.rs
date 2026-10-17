//! The OS the platform reports decides `Platform` and which platform-only
//! props (`ios={{…}}`, `android={{…}}`, `desktop={{…}}`) apply.

use std::time::Duration;

use nsh_core::{ApproxTextEngine, MountItem, Size};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment, Os};

use crate::ChannelSink;

const APP: &str = r#"import { mount, Text, View } from "nsh";
import { Platform } from "nsh/device";

mount(
  () => (
    <View
      ios={{ accessibilityLabel: "ios" }}
      android={{ accessibilityLabel: "android" }}
      desktop={{ accessibilityLabel: "desktop" }}
    >
      <Text>{`${Platform.OS} ${Platform.isDesktop} ${Platform.select({ ios: "i", mobile: "m", desktop: "d" })}`}</Text>
    </View>
  ),
);
"#;

/// The first frame's texts and accessibility labels on `os`.
fn first_frame(os: Os) -> (Vec<String>, Vec<String>) {
    let dir = std::env::temp_dir().join(format!("nsh-platform-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
    })
    .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: nsh_engine::App::Js(Bundle::source(bundle.code)),
        viewport: Size::new(320.0, 480.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment {
            os,
            ..Environment::default()
        },
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
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
            }
            _ => {}
        }
    }
    (texts, labels)
}

#[test]
fn platform_props_and_select_follow_the_os() {
    assert_eq!(
        first_frame(Os::Ios),
        (vec!["ios false i".to_owned()], vec!["ios".to_owned()])
    );
    assert_eq!(
        first_frame(Os::Android),
        (
            vec!["android false m".to_owned()],
            vec!["android".to_owned()]
        )
    );
    assert_eq!(
        first_frame(Os::MacOs),
        (vec!["macos true d".to_owned()], vec!["desktop".to_owned()])
    );
}

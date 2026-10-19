//! Image variants follow the pixel ratio: a surface at 3 mounts the `@3x`
//! file of an image with scales, and moving to 2 sends that image again
//! as `@2x`, alone, in one batch; images without scales are not sent.
//! `Image.getSize` and `Image.prefetch` ask for the variant the views load
//! at the surface's pixel ratio; `getSize` gives the imported file's size.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, MountBatch, MountItem, NodeId,
    PayloadValue, ROOT_ID,
};
use nsh_engine::{Bundle, Engine, EngineConfig, Environment};

use crate::ChannelSink;

/// How long the engine may take to send what a test waits for: the first
/// frame starts QuickJS, which is slow in a debug build on a busy machine.
const WAIT: Duration = Duration::from_secs(5);

/// How long the engine must send nothing to be done with a change.
const QUIET: Duration = Duration::from_millis(300);

/// `app` (TSX), bundled, compiled and started on a surface at
/// `pixel_ratio`.
fn start(app: &str, pixel_ratio: f32) -> (Engine, Receiver<MountBatch>) {
    static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nsh-variants-{}-{}",
        std::process::id(),
        APPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, app).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
        split: false,
        ui_sets: Vec::new(),
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode_with(&bundle.code, false, &[]).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        environment: Environment {
            pixel_ratio,
            ..Environment::default()
        },
        // SAFETY: compiled just now by this runtime.
        ..EngineConfig::new(nsh_engine::App::Js(unsafe { Bundle::bytecode(bytecode) }))
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// The first batch that comes within `wait`, and those that follow it
/// until the engine is quiet.
fn batches_within(rx: &Receiver<MountBatch>, wait: Duration) -> Vec<MountBatch> {
    let mut out = Vec::new();
    if let Ok(first) = rx.recv_timeout(wait) {
        out.push(first);
        while let Ok(batch) = rx.recv_timeout(QUIET) {
            out.push(batch);
        }
    }
    out
}

/// What the engine sends for a start or a change that sends something.
fn sent(rx: &Receiver<MountBatch>) -> Vec<MountBatch> {
    let batches = batches_within(rx, WAIT);
    assert!(!batches.is_empty(), "nothing sent within {WAIT:?}");
    batches
}

/// The image items of `batch`: node and source URI.
fn images(batch: &MountBatch) -> Vec<(NodeId, String)> {
    batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Image { id, source, .. } => Some((
                *id,
                source.as_ref().map_or(String::new(), |s| s.uri.to_string()),
            )),
            _ => None,
        })
        .collect()
}

const APP: &str = r#"import { mount, View } from "nsh";
import { Image } from "nsh/image";

const icon = { uri: "asset:/img.png", width: 40, height: 30, scales: [1, 2, 3] };
const single = { uri: "asset:/single.png", width: 10, height: 10 };
const big = { uri: "asset:/big.png", width: 20, height: 20, scales: [1, 2, 4] };

mount(() => (
  <View>
    <Image src={icon} />
    <Image src={single} />
    <Image src={big} />
  </View>
));
"#;

#[test]
fn variant_repick_sends_only_the_images_whose_variant_changes() {
    let mut environment = Environment {
        pixel_ratio: 3.0,
        ..Environment::default()
    };
    let (engine, rx) = start(APP, environment.pixel_ratio);
    let first = sent(&rx);
    let mounted: Vec<_> = first.iter().flat_map(images).collect();
    let uris: Vec<&str> = mounted.iter().map(|(_, u)| u.as_str()).collect();
    assert_eq!(
        uris,
        [
            "asset:/img@3x.png",
            "asset:/single.png",
            "asset:/big@4x.png"
        ]
    );
    let icon = mounted[0].0;

    // 3 → 2: both scaled images go to `@2x`; `single` is not sent.
    environment.pixel_ratio = 2.0;
    engine.control().set_environment(environment);
    let after = sent(&rx);
    let changed: Vec<_> = after.iter().map(images).filter(|i| !i.is_empty()).collect();
    assert_eq!(changed.len(), 1, "one batch: {changed:?}");
    let mut sent_now = changed[0].clone();
    sent_now.sort();
    let big = sent_now[1].0;
    assert_eq!(
        sent_now,
        [
            (icon, "asset:/img@2x.png".to_owned()),
            (big, "asset:/big@2x.png".to_owned())
        ]
    );
    // No relayout: no frames moved.
    assert!(
        !after
            .iter()
            .flat_map(|b| &b.items)
            .any(|i| matches!(i, MountItem::Frame { .. })),
        "no relayout"
    );

    // 2 → 2.5: the smallest scale at least 2.5: `@3x` and `@4x`.
    environment.pixel_ratio = 2.5;
    engine.control().set_environment(environment);
    let mut sent_now: Vec<_> = sent(&rx).iter().flat_map(images).collect();
    sent_now.sort();
    assert_eq!(
        sent_now,
        [
            (icon, "asset:/img@3x.png".to_owned()),
            (big, "asset:/big@4x.png".to_owned())
        ]
    );

    // 2.5 → 3: neither pick changes, nothing is sent.
    environment.pixel_ratio = 3.0;
    engine.control().set_environment(environment);
    let sent_now: Vec<_> = batches_within(&rx, QUIET).iter().flat_map(images).collect();
    assert!(sent_now.is_empty(), "{sent_now:?}");
}

/// Many ratio changes keep the item count bounded: one image item per
/// image whose pick changed, each time.
#[test]
fn variant_repick_item_counts_stay_bounded() {
    let mut environment = Environment {
        pixel_ratio: 1.0,
        ..Environment::default()
    };
    let (engine, rx) = start(APP, environment.pixel_ratio);
    sent(&rx);
    for ratio in [2.0, 1.0, 2.0, 1.0, 3.0, 1.0] {
        environment.pixel_ratio = ratio;
        engine.control().set_environment(environment);
        let after = sent(&rx);
        let items: usize = after.iter().map(|b| b.items.len()).sum();
        let images = after.iter().flat_map(images).count();
        assert_eq!(images, 2, "at {ratio}: the icon and big");
        assert!(items <= 4, "at {ratio}: {items} items");
    }
}

const INFO_APP: &str = r#"import { mount, signal, Text } from "nsh";
import { Image } from "nsh/image";

const icon = { uri: "asset:/img.png", width: 40, height: 30, scales: [1, 2, 3] };
const shown = signal("waiting");
setTimeout(async () => {
  const size = await Image.getSize(icon);
  const plain = await Image.getSize({ uri: "asset:/single.png", width: 10, height: 10 });
  const ready = await Image.prefetch(icon);
  shown.set(`${size.width}x${size.height} ${plain.width}x${plain.height} ${ready}`);
}, 0);

mount(() => <Text>{shown()}</Text>);
"#;

/// Answers every image request with its file's pixel size (the variant's
/// scale times 40x30 for `img`, 10x10 for `single`), until the app shows
/// its result: the URIs asked for and the text shown.
fn info(pixel_ratio: f32) -> (Vec<String>, String) {
    let (engine, rx) = start(INFO_APP, pixel_ratio);
    let control = engine.control();
    let mut asked = Vec::new();
    let mut shown = String::new();
    while shown.is_empty() || shown == "waiting" {
        let Ok(batch) = rx.recv_timeout(WAIT) else {
            break;
        };
        for item in &batch.items {
            match item {
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::ImageInfo { id, uri },
                } => {
                    asked.push(uri.clone());
                    let scale = match uri.as_str() {
                        "asset:/img@2x.png" => 2.0,
                        "asset:/img@3x.png" => 3.0,
                        _ => 1.0,
                    };
                    let (w, h) = if uri.contains("single") {
                        (10.0, 10.0)
                    } else {
                        (40.0 * scale, 30.0 * scale)
                    };
                    control.dispatch_event(
                        ROOT_ID,
                        EventKind::ImageInfo,
                        EventPayload::Map(vec![
                            ("id", PayloadValue::Number(*id as f64)),
                            ("width", PayloadValue::Number(w)),
                            ("height", PayloadValue::Number(h)),
                        ]),
                    );
                }
                MountItem::Text { text, .. } => shown = text.text.clone(),
                _ => {}
            }
        }
    }
    (asked, shown)
}

#[test]
fn variant_info_get_size_and_prefetch_ask_for_the_picked_variant() {
    let (asked, shown) = info(3.0);
    assert_eq!(
        asked,
        [
            "asset:/img@3x.png",
            "asset:/single.png",
            "asset:/img@3x.png"
        ]
    );
    // The imported file's size, whatever variant was asked for.
    assert_eq!(shown, "40x30 10x10 true");

    let (asked, shown) = info(2.0);
    assert_eq!(asked[0], "asset:/img@2x.png");
    assert_eq!(shown, "40x30 10x10 true");

    let (asked, shown) = info(1.0);
    assert_eq!(asked[0], "asset:/img.png");
    assert_eq!(shown, "40x30 10x10 true");
}

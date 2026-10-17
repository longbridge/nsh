//! The Web and video demo: two web views and a video, the buttons driving
//! them and what they tell the app; the inline page follows the
//! appearance.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    Command, EventKind, EventPayload, MountBatch, MountItem, NativeProps, NodeId, PayloadValue,
    ViewKind, WebSource, platform::ColorScheme,
};
use nsh_engine::{Engine, Environment};

use crate::common::shows;

/// What the gallery shows, the nodes of its web views and video, and the
/// commands sent to nodes.
#[derive(Default)]
struct Media {
    screen: crate::common::Screen,
    web_views: Vec<NodeId>,
    videos: Vec<NodeId>,
    commands: Vec<(NodeId, Command)>,
    /// The HTML of each inline page, as last sent.
    html: Vec<(NodeId, String)>,
    /// Views by accessibility label (round buttons have no text).
    labels: Vec<(String, NodeId)>,
}

impl Media {
    fn drain(&mut self, rx: &Receiver<MountBatch>) {
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
            self.screen.apply(&batch);
            for item in &batch.items {
                match item {
                    MountItem::Create {
                        id,
                        kind: ViewKind::WebView,
                    } => self.web_views.push(*id),
                    MountItem::Create {
                        id,
                        kind: ViewKind::Video,
                    } => self.videos.push(*id),
                    MountItem::Command { id, command } => {
                        self.commands.push((*id, command.clone()))
                    }
                    MountItem::ViewProps { id, props } => {
                        self.labels.retain(|l| l.1 != *id);
                        if let Some(label) = &props.accessibility.label {
                            self.labels.push((label.to_string(), *id));
                        }
                    }
                    MountItem::NativeProps {
                        id,
                        props: NativeProps::WebView(web),
                        ..
                    } => {
                        if let WebSource::Html { html, .. } = &web.source {
                            self.html.retain(|h| h.0 != *id);
                            self.html.push((*id, html.to_string()));
                        }
                    }
                    MountItem::AccessibilityLabel { id, label } => {
                        self.labels.retain(|l| l.1 != *id);
                        if let Some(label) = label {
                            self.labels.push((label.to_string(), *id));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn press(&mut self, engine: &Engine, rx: &Receiver<MountBatch>, label: &str) {
        let text = self
            .screen
            .find(label)
            .unwrap_or_else(|| panic!("no {label}: {:?}", self.screen.texts));
        engine.control().dispatch_event(
            self.screen.press_target(text),
            EventKind::Press,
            EventPayload::None,
        );
        self.drain(rx);
    }

    /// Presses the view labelled `label` for assistive technologies.
    fn press_labelled(&mut self, engine: &Engine, rx: &Receiver<MountBatch>, label: &str) {
        let node = self
            .labels
            .iter()
            .rev()
            .find(|l| l.0 == label)
            .unwrap_or_else(|| panic!("no {label}: {:?}", self.labels))
            .1;
        engine
            .control()
            .dispatch_event(node, EventKind::Press, EventPayload::None);
        self.drain(rx);
    }

    fn event(
        &mut self,
        engine: &Engine,
        rx: &Receiver<MountBatch>,
        node: NodeId,
        kind: EventKind,
        payload: Vec<(&'static str, PayloadValue)>,
    ) {
        engine
            .control()
            .dispatch_event(node, kind, EventPayload::Map(payload));
        self.drain(rx);
    }
}

/// The gallery with the demo open: the index first, then the demo's
/// mount items, so the views it creates are seen.
fn open() -> (Engine, Receiver<MountBatch>, Media) {
    let (engine, rx) = crate::common::start();
    let mut media = Media::default();
    media.drain(&rx);
    media.press(&engine, &rx, "Web and video");
    assert!(
        media.screen.titles.iter().any(|t| t.1 == "Web and video"),
        "{:?}",
        media.screen.titles
    );
    (engine, rx, media)
}

#[test]
fn media_shows_its_sections_and_toggles_the_video() {
    let (engine, rx, mut media) = open();
    for text in [
        "Web view",
        "Inline HTML, with messages both ways",
        "From the page",
        "To the page",
        "A web site",
        "Navigation state, back and forward",
        "Loaded",
        "Video",
        "AVPlayer / MediaPlayer",
        "Paused",
        "Post to the page",
        "…",
        "Reload",
        "Restart",
        "0:00",
        "Own controls",
        "System controls",
    ] {
        assert!(
            shows(&media.screen, text),
            "{text}: {:?}",
            media.screen.texts
        );
    }
    assert_eq!(media.web_views.len(), 2);
    assert_eq!(media.videos.len(), 1);

    media.press_labelled(&engine, &rx, "Play");
    assert!(shows(&media.screen, "Playing"), "{:?}", media.screen.texts);
    assert!(
        media.labels.iter().any(|l| l.0 == "Pause"),
        "{:?}",
        media.labels
    );

    let video = media.videos[0];
    media.press(&engine, &rx, "Restart");
    assert!(
        media
            .commands
            .iter()
            .any(|(id, c)| *id == video && *c == Command::Seek(0.0)),
        "{:?}",
        media.commands
    );
    media.event(
        &engine,
        &rx,
        video,
        EventKind::Progress,
        vec![
            ("currentTime", PayloadValue::Number(1.5)),
            ("duration", PayloadValue::Number(5.0)),
        ],
    );
    assert!(shows(&media.screen, "0:01"), "{:?}", media.screen.texts);
    assert!(shows(&media.screen, "0:05"), "{:?}", media.screen.texts);
    media.event(
        &engine,
        &rx,
        video,
        EventKind::Buffer,
        vec![("isBuffering", PayloadValue::Bool(true))],
    );
    assert!(
        shows(&media.screen, "Buffering"),
        "{:?}",
        media.screen.texts
    );

    // The system's controls replace the demo's own.
    media.press(&engine, &rx, "System controls");
    assert!(!shows(&media.screen, "Restart"), "{:?}", media.screen.texts);
    media.press(&engine, &rx, "Own controls");
    assert!(shows(&media.screen, "Restart"), "{:?}", media.screen.texts);
}

#[test]
fn media_talks_to_its_web_views() {
    let (engine, rx, mut media) = open();
    // In the order of the render (the core may create them in another).
    media.web_views.sort();
    let (page, site) = (media.web_views[0], media.web_views[1]);

    media.event(
        &engine,
        &rx,
        page,
        EventKind::Message,
        vec![("data", PayloadValue::Str("hello from the page".into()))],
    );
    assert!(
        shows(&media.screen, "hello from the page"),
        "{:?}",
        media.screen.texts
    );

    media.press(&engine, &rx, "Post to the page");
    assert!(
        media.screen.texts.iter().any(|t| t.1.starts_with("hi #")),
        "{:?}",
        media.screen.texts
    );
    assert!(
        media
            .commands
            .iter()
            .any(|(id, c)| *id == page
                && matches!(c, Command::PostMessage(m) if m.starts_with("hi #"))),
        "{:?}",
        media.commands
    );

    media.event(
        &engine,
        &rx,
        site,
        EventKind::NavigationStateChange,
        vec![
            ("url", PayloadValue::Str("https://nsh.dev/docs/".into())),
            ("title", PayloadValue::Str("NSH".into())),
            ("loading", PayloadValue::Bool(true)),
            ("canGoBack", PayloadValue::Bool(true)),
            ("canGoForward", PayloadValue::Bool(false)),
        ],
    );
    for text in ["NSH", "nsh.dev", "Loading"] {
        assert!(
            shows(&media.screen, text),
            "{text}: {:?}",
            media.screen.texts
        );
    }
    media.press_labelled(&engine, &rx, "Back");
    assert!(
        media
            .commands
            .iter()
            .any(|(id, c)| *id == site && *c == Command::GoBack),
        "{:?}",
        media.commands
    );
}

#[test]
fn media_page_follows_the_appearance() {
    let (engine, rx, mut media) = open();
    let light = media.html.last().expect("the inline page").1.clone();
    assert!(light.contains("background: #f0f0ec"), "{light}");
    engine.control().set_environment(Environment {
        color_scheme: ColorScheme::Dark,
        ..Environment::default()
    });
    media.drain(&rx);
    let dark = &media.html.last().unwrap().1;
    assert!(dark.contains("background: #0c0d0f"), "{dark}");
    assert!(dark.contains("color: #eceef1"), "{dark}");
}

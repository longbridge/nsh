//! Views drawn by GPUI's renderer offscreen and read back as pixels
//! (feature `shot`: GPUI's headless renderer, Metal on macOS, wgpu on
//! Linux; skipped where the machine has none). A test binary of its own,
//! run on the main thread (no test harness): nsh's main-thread queue
//! belongs to the first app a process makes, and the macOS platform that
//! gives the text system is made on the main thread.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use futures_util::future::{Either, select};
use gpui::{AppContext as _, HeadlessAppContext, px, size};
use nsh_platform_gpui::{App, AppOptions, AppView, Bundle, Root};

/// A PNG of `w` × `h` pixels, its left half red and its right half green,
/// as a data URI.
fn halves(w: u32, h: u32) -> String {
    let image = image::RgbaImage::from_fn(w, h, |x, _| {
        if x < w / 2 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 255, 0, 255])
        }
    });
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    use base64::Engine as _;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// An SVG of 40 × 20 points, its left half `fill`, its right half empty.
fn half_icon(fill: &str) -> String {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="20" height="20" fill="{fill}"/></svg>"#
    );
    use base64::Engine as _;
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    )
}

/// Turned a quarter clockwise, each picture's left half is on top:
/// a photo turned itself (resampled), a tinted icon in a turned box and a
/// one-color icon (both turned by the GPU), and a photo in a turned,
/// rounded and clipping card (resampled, cut to the card's corners).
fn app() -> String {
    format!(
        r##"
import {{ mount, Image, View }} from "nsh";

const PHOTO = "{photo}";
const ICON = "{icon}";
const RED_ICON = "{red_icon}";

function App() {{
  return (
    <View style={{{{ flex: 1, backgroundColor: "#ffffff" }}}}>
      <Image src={{PHOTO}} style={{{{ position: "absolute", left: 20, top: 30, width: 40, height: 20, transform: [{{ rotate: "90deg" }}] }}}} />
      <View style={{{{ position: "absolute", left: 100, top: 30, width: 40, height: 20, transform: [{{ rotate: "90deg" }}] }}}}>
        <Image src={{ICON}} style={{{{ width: 40, height: 20, tintColor: "#0000ff" }}}} />
      </View>
      <Image src={{RED_ICON}} style={{{{ position: "absolute", left: 180, top: 30, width: 40, height: 20, transform: [{{ rotate: "90deg" }}] }}}} />
      <View style={{{{ position: "absolute", left: 20, top: 100, width: 60, height: 60, borderRadius: 30, overflow: "hidden", transform: [{{ rotate: "90deg" }}] }}}}>
        <Image src={{PHOTO}} style={{{{ width: 60, height: 60 }}}} />
      </View>
    </View>
  );
}}
mount(App);
"##,
        photo = halves(40, 20),
        icon = half_icon("#000000"),
        red_icon = half_icon("#ff0000"),
    )
}

/// Squares 40 × 40 turned by 0°, 30° and 180° (a row each), each with a
/// "T": a text view, one underlined, one with a text shadow, and a canvas's
/// `fillText` (longbridge/nsh#3255: the glyphs stayed upright, and at 180°
/// a text view's were not drawn).
fn turned_text_app() -> String {
    r##"
import { mount, Canvas, Text, View } from "nsh";

function Box(p: { left: number; top: number; angle: number; text?: object; canvas?: boolean }) {
  return (
    <View
      style={{ position: "absolute", left: p.left, top: p.top, width: 40, height: 40, alignItems: "center", justifyContent: "center", backgroundColor: "#38bdf8", transform: `rotate(${p.angle}deg)` }}
    >
      {p.canvas ? (
        <Canvas
          style={{ width: 40, height: 40 }}
          draw={(cx) => {
            cx.fillStyle = "#000000";
            cx.font = "bold 28px sans-serif";
            cx.textAlign = "center";
            cx.textBaseline = "middle";
            cx.fillText("T", 20, 20);
          }}
        />
      ) : (
        <Text style={{ color: "#000000", fontSize: 28, fontWeight: "bold", ...p.text }}>T</Text>
      )}
    </View>
  );
}

function App() {
  return (
    <View style={{ flex: 1, backgroundColor: "#ffffff" }}>
      {[0, 30, 180].flatMap((angle, row) => [
        <Box left={10} top={10 + row * 60} angle={angle} />,
        <Box left={65} top={10 + row * 60} angle={angle} text={{ textDecorationLine: "underline" }} />,
        <Box left={120} top={10 + row * 60} angle={angle} text={{ textShadow: "3px 3px rgb(255,255,255)" }} />,
        <Box left={175} top={10 + row * 60} angle={angle} canvas />,
      ])}
    </View>
  );
}
mount(App);
"##
    .into()
}

/// Dark pixels in the top and bottom halves of the 40 × 40 square at
/// `(x, y)` (window points).
fn ink(shot: &image::RgbaImage, scale: f32, (x, y): (f32, f32)) -> (u32, u32) {
    let (x0, y0, side) = (
        (x * scale) as u32,
        (y * scale) as u32,
        (40.0 * scale) as u32,
    );
    let (mut top, mut bottom) = (0, 0);
    for py in y0..y0 + side {
        for px in x0..x0 + side {
            let p = shot.get_pixel(px, py).0;
            if u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 150 {
                if py < y0 + side / 2 {
                    top += 1;
                } else {
                    bottom += 1;
                }
            }
        }
    }
    (top, bottom)
}

/// Text turns with its view: a "T" upright has its ink at the top, turned
/// by 180° at the bottom, in text views and canvases alike.
fn turned_text(cx: &mut HeadlessAppContext, output: Option<&Path>) {
    let window = open(cx, &turned_text_app());
    // (left, top, upright, what).
    let squares = [
        ((10.0, 10.0), true, "an upright text view"),
        ((175.0, 10.0), true, "an upright canvas"),
        // (Not the underlined one: its underline, turned, is at the top.)
        ((10.0, 130.0), false, "a text view turned by 180°"),
        ((175.0, 130.0), false, "a canvas turned by 180°"),
    ];
    let start = Instant::now();
    loop {
        draw(cx, window);
        let shot = cx.capture_screenshot(window.into()).unwrap();
        let scale = shot.width() as f32 / 240.0;
        let min = (20.0 * scale * scale) as u32;
        let wrong: Vec<String> = squares
            .iter()
            .filter_map(|&(at, upright, what)| {
                let (top, bottom) = ink(&shot, scale, at);
                let (head, foot) = if upright {
                    (top, bottom)
                } else {
                    (bottom, top)
                };
                // The bar of a "T" has more ink than its stem.
                (head < min || 2 * head < 3 * foot).then(|| {
                    format!("{what}: {top} dark pixels in the top half, {bottom} in the bottom")
                })
            })
            .collect();
        // The system's fonts (the glyphs' outlines) are found by a worker.
        if wrong.is_empty() || start.elapsed() > Duration::from_secs(15) {
            if let Some(output) = output {
                let path = output.join("turned-text.png");
                shot.save(&path).unwrap();
                println!("render: turned text screenshot: {}", path.display());
            }
            assert!(wrong.is_empty(), "{}", wrong.join("; "));
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The pixel at window point `(x, y)` of a screenshot `scale` pixels a
/// point.
fn pixel(shot: &image::RgbaImage, scale: f32, (x, y): (f32, f32)) -> [u8; 3] {
    let p = shot.get_pixel((x * scale) as u32, (y * scale) as u32).0;
    [p[0], p[1], p[2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(&b).all(|(a, b)| a.abs_diff(*b) <= 24)
}

const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// What each point should show: (point, color, what it shows).
const EXPECTED: &[((f32, f32), [u8; 3], &str)] = &[
    // The photo (center 40, 40): 20 wide, 40 tall.
    (
        (40.0, 25.0),
        RED,
        "the photo's left half, turned to the top",
    ),
    (
        (40.0, 55.0),
        GREEN,
        "the photo's right half, turned to the bottom",
    ),
    (
        (25.0, 40.0),
        WHITE,
        "left of the turned photo (its upright left half)",
    ),
    // The tinted icon (center 120, 40).
    (
        (120.0, 25.0),
        BLUE,
        "the tinted icon's half, turned to the top",
    ),
    ((120.0, 55.0), WHITE, "the tinted icon's empty half"),
    ((105.0, 40.0), WHITE, "left of the turned tinted icon"),
    // The one-color icon (center 200, 40).
    ((200.0, 25.0), RED, "the red icon's half, turned to the top"),
    ((185.0, 40.0), WHITE, "left of the turned red icon"),
    // The round card (center 50, 130, radius 30).
    (
        (50.0, 110.0),
        RED,
        "the card's photo, its left half turned to the top",
    ),
    (
        (50.0, 150.0),
        GREEN,
        "the card's photo, its right half at the bottom",
    ),
    ((23.0, 103.0), WHITE, "outside the card's round corner"),
];

/// A scroll view 200 × 160 at (20, 10) over 200 × 160 of content, the
/// photo 100 × 80 at its top left, zoomed to 2× around its middle: the
/// viewport shows the content from (50, 40), the photo's right half at
/// twice its size in its top left quarter. `text` adds 60 lines of text
/// under the photo, `steps` zooms by another step every 16 ms.
fn zoom_app(text: bool, steps: bool) -> String {
    let lines = if text {
        r#"<Each of={LINES}>{(i) => <Text style={{ fontSize: 9 }}>Line {i} of zoomed text</Text>}</Each>"#
    } else {
        ""
    };
    let steps = if steps {
        r#"let i = 0;
setInterval(() => {
  i += 1;
  scroll.zoomTo({ scale: 1 + (i % 60) / 20, animated: false });
}, 16);"#
    } else {
        ""
    };
    format!(
        r##"
import {{ mount, Image, Scroll, type ScrollRef, Text, View }} from "nsh";

const PHOTO = "{photo}";
const LINES = Array.from({{ length: 60 }}, (_, i) => i);
let scroll!: ScrollRef;

function App() {{
  return (
    <View style={{{{ flex: 1, backgroundColor: "#ffffff" }}}}>
      <Scroll ref={{scroll}} maximumZoomScale={{4}} zoomScale={{2}} style={{{{ position: "absolute", left: 20, top: 10, width: 200, height: 160 }}}}>
        <View style={{{{ width: 200, height: 160 }}}}>
          <Image src={{PHOTO}} style={{{{ width: 100, height: 80 }}}} />
          {lines}
        </View>
      </Scroll>
    </View>
  );
}}
mount(App);
{steps}
"##,
        photo = halves(100, 80),
    )
}

/// What the zoomed scroll view shows (window points).
const ZOOMED: &[((f32, f32), [u8; 3], &str)] = &[
    (
        (25.0, 15.0),
        GREEN,
        "the photo's right half at the viewport's top left (red unzoomed)",
    ),
    (
        (60.0, 40.0),
        GREEN,
        "content (70, 55), the photo's right half (red unzoomed)",
    ),
    ((115.0, 85.0), GREEN, "near the photo's zoomed bottom right"),
    ((125.0, 40.0), WHITE, "right of the zoomed photo"),
    ((60.0, 100.0), WHITE, "below the zoomed photo"),
    ((230.0, 100.0), WHITE, "outside the scroll view"),
];

fn main() {
    if !cfg!(feature = "js") {
        eprintln!("no JS engine here: skipped");
        return;
    }
    let probe = gpui_platform::current_headless_renderer();
    if !matches!(probe, Ok(Some(_))) {
        eprintln!("no headless renderer here: skipped");
        return;
    }
    drop(probe);
    let text = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(
        text,
        std::sync::Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    // The engine and the image decoders run on threads of their own.
    cx.allow_parking();
    let output = std::env::var_os("NSH_RENDER_OUTPUT_DIR").map(PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    if cfg!(feature = "svg") {
        shows(
            &mut cx,
            &app(),
            EXPECTED,
            "turned-images",
            output.as_deref(),
        );
        println!("render: turned images and icons turn with their views: ok");
    }
    turned_text(&mut cx, output.as_deref());
    println!("render: text turns with its view: ok");
    shows(
        &mut cx,
        &zoom_app(false, false),
        ZOOMED,
        "zoomed-scroll",
        output.as_deref(),
    );
    println!("render: zoomed scroll views draw their content scaled: ok");
    if let Some(output) = &output {
        embedding_screenshots(&mut cx, output);
    }
    if std::env::var_os("NSH_RENDER_BENCH").is_some() {
        zoom_frames(&mut cx);
    }
}

/// Render fixtures neither read nor write the host's persisted state.
struct RenderStorage;

impl nsh_engine::StorageBackend for RenderStorage {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(BTreeMap::new())
    }

    fn write(&self, _: nsh_engine::StorageChange) -> Result<(), String> {
        Err("render fixtures do not persist storage".into())
    }
}

fn render_options() -> AppOptions {
    AppOptions {
        cache: nsh_engine::CacheOptions::off(),
        storage: Some(Arc::new(RenderStorage)),
        data_dir: Some(std::env::temp_dir().join(format!("nsh-render-{}", std::process::id()))),
        ..Default::default()
    }
}

struct LaidOut(Mutex<Option<futures_channel::oneshot::Sender<()>>>);

/// Ordinary NSH text inherits the host's foreground; the last line opts out.
#[nsh_core::element::component]
fn EmbeddedTheme() -> impl nsh_core::element::IntoElement {
    use nsh_core::prelude::*;

    let ready = nsh_core::element::engine_context::<LaidOut>().unwrap();
    view()
        .size_full()
        .p(Points(20.0))
        .gap(Points(12.0))
        .child(
            text("Embedded section")
                .text_size(Points(20.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(text("Text follows the host theme."))
        .child(
            text("An explicit muted color stays muted.")
                .text_sm()
                .text_color(rgb(0x778899)),
        )
        .on_layout(move |_: &Bounds<Points>| {
            if let Some(ready) = ready.0.lock().unwrap().take() {
                // This acknowledgement follows the commit on GPUI's own queue.
                drop(nsh_core::element::run_on_main_thread(move || {
                    let _ = ready.send(());
                }));
            }
        })
}

/// Save actual NSH embeddings in both host appearances.
fn embedding_screenshots(cx: &mut HeadlessAppContext, output: &Path) {
    use nsh_core::platform::Appearance;

    let theme = r##"{ "cssVars": {
        "light": { "background": "#f8fafc", "foreground": "#172033" },
        "dark": { "background": "#141821", "foreground": "#edf2f7" }
    } }"##;
    for (name, appearance) in [("light", Appearance::Light), ("dark", Appearance::Dark)] {
        let (tx, rx) = futures_channel::oneshot::channel();
        let options = AppOptions {
            context: nsh_engine::EngineContext::default().with(LaidOut(Mutex::new(Some(tx)))),
            ..render_options()
        };
        let window = cx
            .open_window(size(px(360.0), px(180.0)), |window, cx| {
                cx.new(|cx| AppView::new(options, window, cx))
            })
            .unwrap();
        // Give the engine the host's viewport before loading the content.
        draw(cx, window);
        cx.update(|cx| {
            window
                .update(cx, |view, _, cx| {
                    view.set_theme_json(theme, cx).unwrap();
                    view.set_appearance(appearance, cx);
                    view.load(App::Rust(Root::of::<EmbeddedTheme>()));
                })
                .unwrap();
        });
        // Readiness comes only from layout; this timer just guards against a hang.
        let deadline = cx.background_executor().timer(Duration::from_secs(10));
        cx.foreground_executor().block_test(async {
            match select(rx, deadline).await {
                Either::Left((ready, _)) => ready.expect("layout acknowledgement dropped"),
                Either::Right(_) => panic!("timed out waiting for {name} embedding layout"),
            }
        });
        draw(cx, window);
        let path = output.join(format!("embedding-{name}.png"));
        cx.capture_screenshot(window.into())
            .unwrap()
            .save(&path)
            .unwrap();
        println!("render: embedding screenshot: {}", path.display());
        cx.update(|cx| window.update(cx, |view, _, _| view.stop()).unwrap());
    }
}

/// Opens a 240 × 180 window running `code`.
fn open(cx: &mut HeadlessAppContext, code: &str) -> gpui::WindowHandle<AppView> {
    let options = AppOptions {
        inline_root: true,
        ..render_options()
    };
    let window = cx
        .open_window(size(px(240.0), px(180.0)), |window, cx| {
            cx.new(|cx| AppView::new(options, window, cx))
        })
        .unwrap();
    let code = nsh_bundler::compile_module("App.tsx", code, &[]).unwrap();
    cx.update(|cx| {
        window
            .update(cx, |view, _, _| view.load(App::Js(Bundle::source(code))))
            .unwrap()
    });
    window
}

/// Draws a frame of `window`; how long drawing took.
fn draw(cx: &mut HeadlessAppContext, window: gpui::WindowHandle<AppView>) -> Duration {
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| {
        window.refresh();
        let start = Instant::now();
        window.draw(cx).clear(cx);
        start.elapsed()
    })
    .unwrap()
}

/// Runs `code` until a frame shows what `expected` says. Saves the last
/// screenshot on failure only when an output directory was requested.
fn shows(
    cx: &mut HeadlessAppContext,
    code: &str,
    expected: &[((f32, f32), [u8; 3], &str)],
    name: &str,
    output: Option<&Path>,
) {
    let window = open(cx, code);
    let start = Instant::now();
    loop {
        draw(cx, window);
        let shot = cx.capture_screenshot(window.into()).unwrap();
        let scale = shot.width() as f32 / 240.0;
        let wrong: Vec<String> = expected
            .iter()
            .filter_map(|&(at, want, what)| {
                let got = pixel(&shot, scale, at);
                (!near(got, want)).then(|| format!("{what} at {at:?}: {got:?}, not {want:?}"))
            })
            .collect();
        if wrong.is_empty() {
            return;
        }
        if start.elapsed() > Duration::from_secs(15) {
            if let Some(output) = output {
                let path = output.join(format!("nsh-{name}.png"));
                shot.save(&path).unwrap();
                panic!("{} (screenshot: {})", wrong.join("; "), path.display());
            }
            panic!("{}", wrong.join("; "));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What drawing costs while the zoom changes every 16 ms (a new scale,
/// so new glyph sizes, nearly every frame), against the same content at
/// rest: `NSH_RENDER_BENCH=1`.
fn zoom_frames(cx: &mut HeadlessAppContext) {
    for (steps, what) in [(false, "at rest, 2x"), (true, "zooming")] {
        let window = open(cx, &zoom_app(true, steps));
        let warm = Instant::now();
        while warm.elapsed() < Duration::from_secs(2) {
            draw(cx, window);
            std::thread::sleep(Duration::from_millis(8));
        }
        let mut times = Vec::new();
        let mut changed = 0;
        let mut last = cx.capture_screenshot(window.into()).unwrap();
        while times.len() < 240 {
            times.push(draw(cx, window).as_secs_f64() * 1e3);
            if times.len() % 20 == 0 {
                let shot = cx.capture_screenshot(window.into()).unwrap();
                changed += usize::from(shot != last);
                last = shot;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        times.sort_by(f64::total_cmp);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        println!(
            "render: {what}: mean {mean:.2} ms, median {:.2} ms, p95 {:.2} ms, max {:.2} ms a frame ({changed} of 12 samples changed)",
            times[times.len() / 2],
            times[times.len() * 95 / 100],
            times[times.len() - 1],
        );
    }
}

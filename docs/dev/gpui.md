# The GPUI backend (macOS, Windows, Linux)

`crates/gpui/nsh-platform-gpui` renders NSH apps with GPUI (GPUI Fast: crates.io `gpui-fast` 0.1.x, aliased as `gpui` in
its workspace), on macOS, Windows and Linux alike. `crates/gpui/nsh-gpui-host` is its development host (`nsh run --target
gpui`), `crates/gpui/nsh-gpui-app` the release runner of `nsh package --target gpui`. Its main purpose is embedding: an
`AppView` is a GPUI view, so GPUI and GPUI Kit apps (`gpui_component`) show NSH modules anywhere in their element tree.
The crate started as a port of the Windows backend (`windows.md`): one retained view per node, NSH draws every view
itself, and the input, scrolling, transition, keymap and editing logic is the Windows backend's, platform-free. Only
drawing, text, the main-thread queue, events and the window are GPUI's. User-facing page: `website/docs/gpui.md`.

Every area of the other desktop backends is ported; "Not implemented, or different" lists where GPUI's primitives make
it draw or behave differently.

## Building and checking

On Linux GPUI needs the X11, Wayland, xkbcommon, fontconfig and FreeType development packages and a Vulkan driver;
web views (feature `webview`, wry) need GTK 3 and WebKitGTK 4.1, video (feature `video`) GStreamer 1.20 or later with
its base plug-ins. On Debian / Ubuntu (`.github/workflows/gpui.yml` installs the same):

```sh
sudo apt-get install clang libclang-dev pkg-config libfontconfig-dev libfreetype-dev libwayland-dev libx11-xcb-dev \
  libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libzstd-dev libvulkan1 libgtk-3-dev libwebkit2gtk-4.1-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good
```

macOS needs Xcode (Metal), Windows the MSVC toolchain and LLVM as for the Windows backend (`RUST_MIN_STACK` included).
Off the three desktops the crates are empty.

The three crates are a workspace of their own, `crates/gpui` (its own `Cargo.toml`, `Cargo.lock` and the dependencies
only they use: GPUI, wry, GStreamer, tiny-skia, ...), excluded from the main workspace, so the main workspace's builds,
`cargo test --workspace`, clippy and CI never build GPUI. It builds into the repository's `target/`
(`crates/gpui/.cargo/config.toml`; the CLI and xtask pass `--manifest-path` and `--target-dir` from the root).

```sh
cd crates/gpui
cargo clippy --all-targets --features nsh-platform-gpui/dev -- -D warnings
cargo test                                        # needs no display: tests open no window
cargo +nightly fmt --all                          # the main workspace's fmt does not reach it
cargo build -p nsh-gpui-host                      # ../../target/debug/nsh-gpui-host
```

The GPUI binaries are built for the machine itself, so the CLI finds them in `target/<profile>` and builds them first
in a checkout; `cargo xtask dist` puts them in `dist/nsh/runtime/gpui/` on each of the three OSes (`tooling.md`,
`embedding.md`). CI: `.github/workflows/gpui.yml` (Ubuntu 24.04, macOS 15 and Windows: clippy with `-D warnings` and
tests of the GPUI workspace), run when `crates/gpui` changes and daily, which catches what changes to the crates it
shares with the main workspace (`nsh-core`, `nsh-engine`, ...) break.

Features:

| Feature | Does | Host | Runner |
|---|---|---|---|
| `js` (default) | JS apps; without it the crate only runs Rust apps and links no QuickJS | yes | yes |
| `webview` (default) | web views with wry (`native/web.rs`; GTK 3 and WebKitGTK on Linux) | yes | yes |
| `video` | videos with GStreamer (`native/video.rs`); without it a video is a placeholder that reports `onError` | Linux | Linux |
| `dev` | the dev server client, log forwarding, the error overlay (`dev.rs`) | yes | no |
| `shot` | `--script` shots on macOS and Windows through GPUI's `test-support` (`Window::render_to_image`) | macOS, Windows | no |

`shot` turns on GPUI's test support: never in the release runner. Cargo unifies features per build, so the
distribution builds the host and the runner in separate `cargo build` runs (`cargo xtask dist`), never together.

### Looking at a change on Linux

Run the host on a rootful Xwayland display of its own, so nothing touches the desktop, and take screenshots with
`--script` (`shot`) or ImageMagick:

```sh
Xwayland :170 -noreset -shm -geometry 1400x1000 & sleep 1.5
nsh build examples/gallery -o build
DISPLAY=:170 WAYLAND_DISPLAY= WEBKIT_DISABLE_DMABUF_RENDERER=1 target/debug/nsh-gpui-host --bundle build/bundle.nshc \
  --size 700x500 --script "wait 800; shot a.png; tap 100 200; shot b.png; quit"
import -window root -display :170 whole.png        # the display, if the window's own shot is not enough
```

- `WAYLAND_DISPLAY=` empty makes GPUI use X11: `shot` reads the window back from the X server (`GetImage`), and web
  views only embed in X11 windows.
- `-shm`: a hidden rootful Xwayland has one vertical blank a second, and a shot waits for the blanks the last frame
  waits for (about 3 s each); with `-shm` frames are copied in as GPUI presents them.
- `WEBKIT_DISABLE_DMABUF_RENDERER=1`: where WebKit cannot allocate GBM buffers ("Failed to create GBM buffer" in the
  log, common under Xwayland), pages stay black without it.
- A tiling window manager tiles the rootful Xwayland window and its root shrinks with it: make it float at its full
  size (Hyprland: `hyprctl dispatch` float, resize and move it to a special workspace).
- GPUI draws at the display's scale: on a scale 2 display screenshot pixels are twice the points the steps take
  (`shot a.png 2` keeps them).

## Architecture

- **One element per surface.** A surface (`surface.rs`) is drawn by one GPUI element, `render::SurfaceElement`, which
  fills the space its parent gives it, reports that size to the engine, takes the pointer and keys over it
  (`events.rs`) and paints the whole tree in its `paint` (`render::draw_view`): shadows and background (`decor.rs`),
  the content clipped to the bounds (text, image, canvas), the children (clipped when the view clips), the hosted
  items (`view::Hosted`: text editors, headers, dialogs) and the scroll indicator, then the border. Each node is an
  `NshView` (`view.rs`), a retained view the core placed at its frame, as on Windows.
- **`DrawCx`** (`render.rs`) is the drawing state GPUI does not keep: GPUI paints primitives (quads, paths, glyphs,
  images) in window coordinates (logical pixels, which are NSH's points) with no world transform and no group
  opacity, so `DrawCx` keeps the current transform (`geom::Mat4`) and applies opacity and color filters
  (`push_color_matrix`) to every color, as GPUI's own element opacity does. Axis-aligned rectangles are GPUI's quads,
  exact; under a rotation, skew or perspective they are filled as paths whose points are mapped one by one
  (`fill_contours`), so they turn and project with the view; glyphs are placed at their mapped origins, upright. Clips
  are rectangles in window coordinates (GPUI's content masks), intersected as they nest; a rounded or rotated clip is
  also kept as a convex polygon paths are cut to, and the innermost rounded clip rounds the corners of the quads and
  images reaching into its corners (`clip_rounded`). `paint(|window, app| …)` paints inside the current clip.
- **One hop per commit.** The engine runs on its own thread (`nsh_engine::Engine`); its sink queues each commit and
  sends one closure to GPUI's main thread (`rt::invoke`), which applies every queued batch in order (`mount.rs`).
  GPUI's executors are bound to the main thread, so `rt::init` spawns one foreground task per process that drains a
  channel of `Send` closures; worker threads (image decoders, network callbacks, the dev client) use it too and find
  their surface by key. `rt::after_ms` runs timers. Changes only mark a surface invalid: every main-thread entry point
  (a closure run there, a timer, an input event, a frame) ends with `rt::flush`, which notifies the GPUI views of the
  invalid surfaces. Views change outside GPUI's tracked state, so GPUI Fast's retained rendering needs that `notify`.
- **Text is shaped on the engine thread** (`text.rs`): `GpuiTextEngine` has a `gpui::WindowTextSystem` of its own
  over the app's `gpui::TextSystem` (thread-safe; the fonts are the app's, so the `FontId`s it hands out are the ones
  the main thread paints with). GPUI shapes one line of one font size at a time and leaves breaking, bidi, line
  heights and ellipses to its callers, so each paragraph is shaped once, in segments of one bidi level and font size,
  into clusters; measuring at another width only re-breaks them (UAX #14, `unicode-linebreak`), and the final layout
  orders each line's clusters by their bidi levels (`unicode-bidi`). Line heights follow Pango's model, as on Linux
  and Windows. The final layout is plain data (lines, cluster boxes, positioned glyphs); the main thread paints the
  glyphs (`paint_glyph`, `paint_emoji`) and reads colors and decorations from the spans, as the Linux backend does.
- **Frames**: native animations, JS frame requests, transitions, scroll glides and navigation transitions tick on the
  window's frames (`Surface::tick`, called while drawing; `window.request_animation_frame()` while something
  animates), and nothing is drawn otherwise.
- **Events** (`events.rs`): GPUI's mouse, scroll-wheel, pinch and key listeners last one frame, so they are installed
  in the element's paint: presses on the surface's hitbox, moves and releases while a press that began there lasts
  (GPUI captures nothing), wheel and touchpad scrolls the hitbox should handle, keys while the surface's focus handle
  is in the focus path. Positions become root coordinates and go to `input.rs` (hit testing, presses, the responder
  system, gestures) as on Windows. Keys become Windows virtual key codes (`keys.rs`), which the shared input,
  scrolling, keymap and editing code reads.
- **Embedding**: `AppView` (`app.rs`) is one surface as a GPUI view: `cx.new(|cx| AppView::new(options, window,
  cx))`, then `load(App::Js(bundle))` or `load(App::Rust(root))`; it renders a `SurfaceElement` and is `Focusable`.
  `open_app_window` opens one in a window of a running GPUI app; `run_app` runs the standalone app (one `AppView`
  filling a window, the process quits when the last window closes). `register_component` registers native
  components (GPUI elements); `set_theme` / `set_appearance` give the surface the host's theme and mode; `set_ui_sets`
  replaces the default `nsh/ui/mobile` set for apps with Rust components of their own. Several views are several
  engines; nothing is global but the `rt` queue and the key bindings, which are per process. Windows showing surfaces
  are opened with `inactive_frame_interval: None` (GPUI draws unfocused windows at 30 fps by default); a host must set
  it in its own `WindowOptions` too. Embedding in a GPUI Kit app (one GPUI, focus and Tab, overlays, themes, frame
  rate): `embedding.md` "GPUI and GPUI Kit apps", `examples/embed-gpui`.

## How each area maps to GPUI

| Area | Module | GPUI |
|---|---|---|
| Text | `text.rs` | GPUI's text system on the engine thread, lines broken and ordered by NSH, glyphs painted on the main thread (above). `selectable` text has the desktop range selection of Windows and Linux (Cmd instead of Ctrl on macOS) and a right-click menu (Copy, Select All). A blurred text shadow is the glyphs painted faintly around the blur radius. GPUI loads variable fonts at their default weight only: on Linux the UI font is the first of a list GPUI can set in bold (Adwaita Sans is variable; Noto Sans or DejaVu Sans take its place) |
| Decoration | `decor.rs` | CSS painting order. Axis-aligned views: GPUI's blurred rounded-box shadows (`paint_drop_shadows`, `paint_inset_shadows`), quads for the background and one-color borders (per-side widths, per-corner radii, dashes), gradients (`decor/gradient.rs`). Paths of polygons for what quads cannot draw: borders whose sides differ in color, dotted lines, and every decoration of a rotated, skewed or projected view (a rotated view's shadow is a stack of translucent outlines). React Native's content shadow is the shadows of the filled descendants' boxes and the glyphs. Color filters are color matrices on every color the subtree draws |
| Images | `image.rs` | Loaded and decoded (the `image` crate: PNG, JPEG, GIF, WebP, BMP, ICO, TIFF, EXIF orientation) on worker threads into `RenderImage`s (straight BGRA, a frame per animation frame), cached per location by bytes, drawn with `paint_image`. GPUI has no color matrix, blur, mipmaps or sprite opacity: a picture keeps variants made on the workers (halved when drawn small, blurred, filtered, tinted, opacity in 16 steps) and draws the closest ready one meanwhile. Animated GIF / WebP advance on timers at their delays, paused while not drawn |
| Canvas | `canvas.rs` | Rasterized with tiny-skia at the window's resolution of the view's whole transform (made again when its linear part changes, moved along with translations), uploaded as a GPUI image; canvas' model kept as on Windows (paths, strokes, anti-aliased clip masks, exact linear / radial / conic gradients, box-blurred shadows, composite operations in layers). Text is GPUI glyphs above the pixmap (GPUI gives no outlines), in the fonts `measureText` uses |
| Scroll views | `scroll.rs` | The Windows backend's: offsets of their own, UIKit-like deceleration, snapping, paging, sticky cells, rubber banding for touch drags (mouse drags never scroll: they are the app's), the keyboard, mirrored horizontal lists, the indicator dragged as a scroll bar, shown while the mouse moves over the view and kept on horizontal views with `showsIndicator={false}` (as on Linux); touchpad scrolls carry GPUI's phases, touchpad pinches go to pinch gestures |
| Input | `input.rs`, `events.rs` | The Windows backend's hit testing, presses, responder and gestures, fed by GPUI's mouse, wheel and pinch events; hover and pressed states and `onHoverIn` / `onHoverOut` (hover picked again after every batch for a still mouse); the cursor is set from the frame (`set_cursor_style`) |
| Text inputs | `text_input.rs` | The editor shared with the Windows backend (`text_input/model.rs`), laid out with GPUI's text system and hosted in the input's view; text arrives through GPUI's input handler (`Window::handle_input`, the `AppView` as `EntityInputHandler`), IME compositions underlined at the caret and the candidate window placed with `bounds_for_range`; the caret blinks on a timer; Cmd on macOS where Ctrl is used elsewhere |
| Navigation, modals | `navigation.rs` | As on Windows: screens slide in and out, headers drawn natively as hosted items with a fixed height reported to the core, back natively (Escape, Alt+Left, Cmd+[ on macOS, Browser Back, the mouse's back button), sheets as cards over a scrim. GPUI has no offscreen pictures, so a departing screen or modal keeps its views (their detaching deferred) in a layer drawn moving out |
| Dialogs, toasts | `system.rs` | Drawn in the surface as items the root hosts (the Windows backend's content dialogs), so they stay with an embedded surface and never block the main loop |
| Video | `native/video.rs` | Feature `video` (Linux): GStreamer `playbin` with an `appsink` of BGRA frames, each frame a `RenderImage` painted as view content (clipped, transformed at its bounds, covered like any view); `controls` draws NSH's own bar |
| Web views | `native/web.rs` | Feature `webview`: wry. Linux: WebKitGTK 4.1 on GTK 3 (X11 only: GPUI must run on X11 or Xwayland) in a GPUI Fast native composition surface (an X11 child window) sized to the visible part of its view, GTK's loop pumped from GPUI's; macOS / Windows: wry's `WKWebView` / WebView2 child at the view's box |
| Native components | `native.rs` | `AppView::register_component`; a component renders any GPUI or GPUI Kit element, laid out and prepainted at its box every frame, clipped to its clipping ancestors, painted where the tree draws its box; its hitboxes take the pointer there (`embedding.md`) |
| Accessibility | `access.rs` | AccessKit nodes in GPUI's accessibility tree (NSAccessibility, UI Automation, AT-SPI through AccessKit's adapters), rebuilt while an assistive technology is attached: the views as synthetic children of the surface element, folded as on Windows and Linux, with roles, names, states, values and actions; the accessibility focus is a real element tracking the view's focus handle; announcements are live nodes |
| Environment | `system.rs` | Color scheme from the app or the window's appearance (`observe_window_appearance`); locale, font scale, reduce motion, right to left, screen reader and OS version read as each native backend reads them (the settings portal and AT-SPI over D-Bus on Linux, `zbus`); pixel ratio the window's scale factor; safe area and keyboard height 0 |
| Window chrome (`nsh/desktop`) | `chrome.rs` | Windows open with a transparent title bar (macOS, Windows) so both styles can be drawn later: a title row drawn above the app, or the hidden style with the traffic lights / caption buttons over the app (`WindowControlArea` on Windows); Linux uses the window manager's decorations, or client-side ones when asked and granted. The application menu is GPUI's native menu bar on macOS, a menu bar drawn above the app elsewhere; context menus are popups drawn over the window with keyboard access |
| Clipboard | `system.rs` | GPUI's (`write_to_clipboard` / `read_from_clipboard`) |
| Networking | `net.rs` | ureq, as on Windows and Linux |
| Dev | `dev.rs` | The dev client on its own thread (reconnecting with a backoff); its events, runtime errors and the reload key drained by a foreground task of the view; the status line and the error overlay are GPUI elements over the surface |

## Frame statistics: `--fps`

`nsh-gpui-host --fps` (`AppOptions::fps`, `AppView::set_fps_monitor` for embedders) logs one `nsh::perf` line a
second (`stats.rs`): `fps 145 active 0/145 gap … tick … paint … canvas 0.04 paths 26.0 raf 145 notifies 0`. Frames
drawn and how many of them GPUI counted active (unfocused windows run at 30 fps unless `inactive_frame_interval` is
`None`), the gaps between frames, the animation tick (prepaint) and the tree's paint (milliseconds as
mean/p50/p95/max), the paint's milliseconds rasterizing canvases and its paths per frame (each batch of paths is a
viewport-sized MSAA pass for GPUI), the animation frames requested and the notifies sent (what made GPUI draw). GPUI's
own draw and present are not in it: add `ZED_MEASUREMENTS=1` (`frame duration: …` per frame). Measure GPU frames on a
real display: Xwayland started with `-shm` renders with llvmpipe, and a hidden window gets no frame callbacks.

## Checking a change: `--script`

`nsh-gpui-host --script "<steps>"` (or `AppOptions::script`) runs steps once the window shows, separated by `;` or
new lines: `wait`, `shot <file.png> [scale]`, `tap`, `drag`, `touch`, `wheel`, `pad`, `hover`, `type`, `mark`, `key`,
`selectall`, `dark` / `light`, `resize`, `quit`, as on Windows (`windows.md`; the reference is `script.rs`). The whole
script is parsed first; a step that fails, an app that shows nothing or an error the app reports makes the host exit
with a failure. Input and shots first wait for the app to settle (no batch for 100 ms, at most 2 s), so a shot after
a tap shows the response; timers, animations and network images still need a `wait`.

Shots: on macOS and Windows with feature `shot` (the host's), GPUI renders the window's last frame offscreen again,
whatever covers it (not yet tried on those systems). On Linux GPUI cannot read a frame back, so the window is read
from the X server: scripts run under X11 or Xwayland (see "Looking at a change on Linux"), and on a bare X server a
window covering this one shows in the shot. Under Wayland proper there are no shots. Web views are not in the offscreen
shots of macOS and Windows.

## Not implemented, or different

- **Transforms**: rotated, skewed and projected views turn as paths (backgrounds, borders, shadows), but glyphs stay
  upright at their mapped origins, and images, videos and canvases draw at their bounds (GPUI has no transform for
  sprites or glyphs).
- **Opacity** is applied to each color rather than to a group, so overlapping translucent children show through each
  other; an image's opacity (a view's, a fade-in) steps in sixteenths, each step a variant of its pixels.
- **Clips** are rectangles (GPUI's content masks): a rounded clip rounds the backgrounds and images that reach into its
  corners, but glyphs, borders, videos and native components are cut by its rectangle.
- **Effects GPUI cannot draw**: `filter: blur()`, `backdropFilter` and `mixBlendMode` are not drawn (no offscreen
  passes); shadows are not cut out of a translucent fill; `borderCurve: "continuous"` draws circular corners.
- **Canvas text** is approximated: clipped to its clip's bounds, upright under rotations, made smaller rather than
  narrower for `maxWidth`, gradients paint it in their middle color, `strokeText` fills, its shadows are not blurred,
  composite operations and `clearRect` do not apply to it.
- **Zooming scroll views**: `zoomTo` and the zoom props are ignored.
- **Video** needs feature `video`, which is built on Linux only (GStreamer); on macOS and Windows a video is a neutral
  placeholder reporting `onError`. Rounded clips do not cut the picture; audio-only sources show nothing.
- **Web views** sit above everything GPUI draws at their place (NSH content drawn over one is under it) and are
  missing from the offscreen shots of macOS and Windows. On Linux they need X11 (a Wayland window shows a placeholder,
  logged once) and may need `WEBKIT_DISABLE_DMABUF_RENDERER=1`; on macOS and Windows clipping ancestors do not cut
  them. `source.baseUrl` for HTML, `scrollEnabled` and `allowsBackForwardNavigationGestures` are not supported; failed
  loads report no `onError` on macOS and Windows.
- **Accessibility** (AccessKit 0.24, as GPUI Fast 0.1.9 uses it): the adapters pass on only `Click`, `Focus`,
  `SetValue` and `ScrollIntoView` everywhere, `Increment` / `Decrement` on macOS and `Expand` / `Collapse` on
  Windows; custom actions, `ShowContextMenu` and scroll actions are in the nodes but not called yet.
- **Text inputs**: input accessory views are never shown (no software keyboard).
- **No desktop equivalent**: the status bar; the safe area and the keyboard height are 0.
- **Not yet tried** on macOS and Windows: the backend was built and checked on Linux (X11 / Xwayland); CI builds
  and tests it on macOS and Windows (the text tests need the main thread on macOS and are skipped there), but `shot`'s
  offscreen rendering, wry's macOS / Windows web views and the chrome's macOS / Windows title bars have not run there.
  Windows CI machines list no fonts through DirectWrite, so font weights and registered fonts are untested on Windows
  (the tests that check them skip where no fonts are listed); the text threads initialize COM, which DirectWrite
  wants off the main thread.
- **Packaged apps** are laid out as the native backend's of the machine they are made on (a directory and `.tar.gz`
  on Linux, an ad hoc signed `.app` on macOS, a directory and `.zip` on Windows; `embedding.md`), with
  `nsh-gpui-app` as the executable (features `js` and `webview`, plus `video` on Linux). The Linux runner links
  WebKitGTK 4.1 (with GTK 3) and GStreamer dynamically: every machine a Linux package runs on needs them installed
  (`libwebkit2gtk-4.1-0`, `libgstreamer1.0-0`, `libgstreamer-plugins-base1.0-0`, and `gstreamer1.0-plugins-good` for
  common formats), besides a Vulkan driver and xkbcommon.
- **Dev logs** (feature `dev`): the log forwarder is per process (`dev.rs`' `CLIENT` / `CONNECTED`), so with several
  `AppView`s connected to dev servers every view's logs go to the most recent session.

### Programmatic window frame and level

The host chooses window bounds and level when opening a GPUI window.
`Window.setFrame` and `Window.setLevel` commands leave those host settings intact.

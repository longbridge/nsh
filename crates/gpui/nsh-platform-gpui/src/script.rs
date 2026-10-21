//! Scripted runs of the standalone app (`AppOptions::script`): steps
//! separated by `;` or new lines, run in order once the window shows, to
//! check the backend without touching the screen.
//!
//! - `wait <ms>`: waits.
//! - `shot <file.png> [scale]`: the window's content as a PNG, at `scale`
//!   pixels per point (default 1, so a shot's pixels are the points the
//!   other steps take; `2` keeps a Retina / 200 % window's pixels). Waits
//!   for the app to settle and the window to draw first (see below).
//!   Missing directories are made.
//! - `tap <x> <y>`: presses and releases at a point (points, root
//!   coordinates).
//! - `drag <x0> <y0> <x1> <y1> [ms]`: presses, moves in steps over `ms`
//!   (default 300), releases.
//! - `touch <x0> <y0> <x1> <y1> [ms]`: the same with a finger.
//! - `wheel <x> <y> <dx> <dy>`: a mouse wheel scroll (points; positive
//!   scrolls toward the end); `pad <x> <y> <dx> <dy>` a precise touchpad
//!   scroll.
//! - `hover <x> <y>`: moves the pointer.
//! - `rightclick <x> <y>`: presses and releases the secondary button
//!   (context menus).
//! - `type <text>`: types into the focused field.
//! - `mark <text>`: an IME's composition in the focused field (`mark`
//!   alone ends it).
//! - `key <name>`: presses a key: `Return`, `BackSpace`, `Tab`, `Escape`,
//!   `Left`, `Home`, `Page_Down`, `F5`, a letter, ... (GPUI's names,
//!   `enter`, `pagedown`, ..., work too); `ctrl+`, `shift+`, `alt+`,
//!   `meta+` prefixes add modifiers (`ctrl+a`).
//! - `selectall`: Ctrl+A.
//! - `dark` / `light`: switches the system's appearance (an app that
//!   forces one keeps it).
//! - `resize <w> <h>`: resizes the window's content (points; a tiling
//!   window manager keeps its own size).
//! - `quit`: closes the window.
//!
//! The whole script is parsed first: a step that does not parse is
//! reported, and the window closes without running any. A failed script
//! (a step that does not parse, an app that shows nothing, a shot not
//! written, the window closing before `quit`, an error the app reports
//! while the script runs) makes [`crate::run_app`] exit with a failure. The steps run on GPUI's main
//! thread, one after another, in one task of the window's app: input goes
//! through the handlers real input goes through ([`crate::input`],
//! [`crate::text_input`]), with times on the surface's clock, so presses,
//! gestures and flings behave as they do under a mouse. Running starts
//! once the app's first views are mounted, the app settled and the window
//! drew it. Input steps and shots first wait for the app to settle: no
//! batch for 100 ms (at most 2 s: an app that commits on every frame
//! never settles). So a `shot` right after a `tap` shows what the app did
//! in response, and a `tap` after a `tap` that pushed a screen lands on
//! that screen; timers, animations and network images still need a
//! `wait`.
//!
//! Shots: with feature `shot` (which turns on GPUI's `test-support`; the
//! development host turns it on there) on macOS and Windows, GPUI renders
//! the window's last frame offscreen again (`Window::render_to_image`),
//! whatever covers the window (not yet tried on those systems). GPUI
//! cannot read a frame back on Linux, so there the window is read from
//! the X server (`GetImage`): the host runs scripts under X11 (Xwayland on
//! a Wayland desktop), and on a bare X server another window covering this
//! one shows in the shot. GPUI's frames reach the X server through DRI3
//! and Present, shown on the window's vertical blanks, so a shot first
//! waits for the blanks the last frame waits for. A hidden rootful
//! Xwayland (one parked off screen for tests) has a blank a second, which
//! makes a shot take about 3 s there: start it with `-shm`, and frames are
//! copied in as GPUI presents them. Under Wayland proper there are no
//! shots.

use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use gpui::{App, AsyncApp, WindowHandle};

use crate::{
    app::AppView,
    input::{Modifiers, PointerKind, WheelEvent},
    surface::Surface,
    synth::{drawn, pointer, press, secondary, sleep, with_surface},
};

/// The time an input step leaves the app before the next step.
const STEP_MS: u64 = 16;

/// How long without a batch counts as settled.
const SETTLE_MS: u64 = 100;

/// The longest the script waits for the app to settle.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the script waits for the app's first views.
const START_TIMEOUT: Duration = Duration::from_secs(20);

/// One step.
#[derive(Debug, Clone, PartialEq)]
enum Step {
    Wait(u64),
    Shot {
        path: String,
        scale: f32,
    },
    Tap(f32, f32),
    RightClick(f32, f32),
    Drag {
        from: (f32, f32),
        to: (f32, f32),
        ms: f32,
        touch: bool,
    },
    Wheel {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        precise: bool,
    },
    Hover(f32, f32),
    Type(String),
    Mark(String),
    Key(u16, Modifiers),
    Dark(bool),
    Resize(f32, f32),
    Quit,
}

/// A script failed: the standalone app exits with a failure.
static FAILED: AtomicBool = AtomicBool::new(false);

/// True when a script failed: a step did not parse, the app showed
/// nothing, a shot was not written, the window closed before `quit` or the
/// app reported an error.
pub(crate) fn failed() -> bool {
    FAILED.load(Ordering::Relaxed)
}

fn fail(message: &str) {
    log::error!("script: {message}");
    FAILED.store(true, Ordering::Relaxed);
}

thread_local! {
    /// A script runs on this (main) thread.
    static RUNNING: Cell<bool> = const { Cell::new(false) };
}

/// The app reported an error: a script running fails.
pub(crate) fn app_error() {
    if RUNNING.with(Cell::get) {
        FAILED.store(true, Ordering::Relaxed);
    }
}

/// Runs `script`'s steps in `window` once it shows.
pub(crate) fn run(window: WindowHandle<AppView>, script: &str, cx: &mut App) {
    let steps = match parse(script) {
        Ok(steps) => steps,
        Err(err) => {
            fail(&err);
            // Whatever drives the script is not left waiting for a window
            // that never quits.
            cx.spawn(async move |cx| {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            })
            .detach();
            return;
        }
    };
    RUNNING.with(|r| r.set(true));
    cx.spawn(async move |cx| {
        if !started(window, cx).await {
            fail(&format!(
                "the app showed nothing in {START_TIMEOUT:?}; not running"
            ));
            let _ = window.update(cx, |_, window, _| window.remove_window());
            return;
        }
        for step in steps {
            log::debug!("script: {step:?}");
            let quit = step == Step::Quit;
            if !run_step(window, step, cx).await {
                if !quit {
                    fail("the window closed before the script ended");
                }
                break;
            }
        }
        RUNNING.with(|r| r.set(false));
    })
    .detach();
}

/// Waits for the first commit's views to be drawn. False when they do not
/// come (or the window closed).
async fn started(window: WindowHandle<AppView>, cx: &mut AsyncApp) -> bool {
    let start = std::time::Instant::now();
    loop {
        let Some(mounted) = with_surface(window, cx, |s, _, _| {
            s.root.with_children(|children| !children.is_empty())
        }) else {
            return false;
        };
        if mounted {
            return settled(window, cx).await && drawn(window, cx).await;
        }
        if start.elapsed() > START_TIMEOUT {
            return false;
        }
        sleep(cx, 10).await;
    }
}

/// Runs one step. False when the script ends (the window closed).
async fn run_step(window: WindowHandle<AppView>, step: Step, cx: &mut AsyncApp) -> bool {
    let input = |cx: &mut AsyncApp, f: &dyn Fn(&Rc<Surface>)| {
        with_surface(window, cx, |s, _, _| f(s)).is_some()
    };
    // Input goes to what the previous steps led to (a pushed screen, a
    // switched page), not to what was there a moment before.
    let is_input = !matches!(
        step,
        Step::Wait(_) | Step::Shot { .. } | Step::Dark(_) | Step::Resize(..) | Step::Quit
    );
    if is_input && !settled(window, cx).await {
        return false;
    }
    if matches!(step, Step::Tap(..) | Step::Drag { .. }) {
        // A press focuses the view, as a real one does ([`crate::events`]).
        let _ = window.update(cx, |view, window, cx| {
            let focus = gpui::Focusable::focus_handle(view, cx);
            if !focus.is_focused(window) {
                window.focus(&focus, cx);
            }
        });
    }
    let alive = match step {
        Step::Wait(ms) => {
            sleep(cx, ms).await;
            true
        }
        Step::Tap(x, y) => {
            if !input(cx, &|s| pointer(s, PointerKind::Down, x, y, false)) {
                return false;
            }
            sleep(cx, 60).await;
            input(cx, &|s| pointer(s, PointerKind::Up, x, y, false))
        }
        Step::Drag {
            from,
            to,
            ms,
            touch,
        } => {
            if !input(cx, &|s| {
                pointer(s, PointerKind::Down, from.0, from.1, touch)
            }) {
                return false;
            }
            let n = (ms / STEP_MS as f32).ceil().max(1.0) as u32;
            for i in 1..=n {
                sleep(cx, STEP_MS).await;
                let t = i as f32 / n as f32;
                let (x, y) = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
                if !input(cx, &|s| pointer(s, PointerKind::Move, x, y, touch)) {
                    return false;
                }
            }
            input(cx, &|s| pointer(s, PointerKind::Up, to.0, to.1, touch))
        }
        Step::Hover(x, y) => input(cx, &|s| pointer(s, PointerKind::Move, x, y, false)),
        Step::RightClick(x, y) => {
            if !input(cx, &|s| secondary(s, PointerKind::Down, x, y)) {
                return false;
            }
            sleep(cx, 60).await;
            input(cx, &|s| secondary(s, PointerKind::Up, x, y))
        }
        Step::Wheel {
            x,
            y,
            dx,
            dy,
            precise,
        } => input(cx, &|s| {
            crate::input::wheel(
                s,
                WheelEvent {
                    x,
                    y,
                    dx,
                    dy,
                    precise,
                    modifiers: Modifiers::default(),
                    time: s.clock(),
                },
            )
        }),
        Step::Type(text) => input(cx, &|s| crate::text_input::char_input(s, &text)),
        Step::Mark(text) => input(cx, &|s| crate::text_input::mark(s, &text)),
        Step::Key(vk, modifiers) => input(cx, &|s| press(s, vk, modifiers)),
        Step::Dark(dark) => with_surface(window, cx, |_, window, _| {
            crate::system::simulate_system_scheme(dark);
            window.refresh();
        })
        .is_some(),
        Step::Resize(w, h) => with_surface(window, cx, |_, window, _| {
            window.resize(gpui::size(gpui::px(w), gpui::px(h)))
        })
        .is_some(),
        Step::Shot { path, scale } => {
            if !settled(window, cx).await {
                return false;
            }
            let saved = crate::synth::frame(window, cx)
                .await
                .map(|f| f.and_then(|(frame, content)| save(frame, content, &path, scale)));
            match saved {
                Some(Ok((w, h))) => log::info!("script: wrote {path} ({w}x{h})"),
                Some(Err(err)) => fail(&format!("shot {path}: {err:#}")),
                None => return false,
            }
            return true;
        }
        Step::Quit => {
            let _ = with_surface(window, cx, |_, window, _| window.remove_window());
            return false;
        }
    };
    if alive {
        sleep(cx, STEP_MS).await;
    }
    alive
}

/// Waits for the app to settle (no batch for [`SETTLE_MS`], at most
/// [`SETTLE_TIMEOUT`]: an app that commits on every frame never does),
/// False when the window closed.
async fn settled(window: WindowHandle<AppView>, cx: &mut AsyncApp) -> bool {
    let start = std::time::Instant::now();
    let mut last = None;
    let mut quiet = start;
    loop {
        let Some(batches) = with_surface(window, cx, |s, _, _| s.batches.get()) else {
            return false;
        };
        if last != Some(batches) {
            last = Some(batches);
            quiet = std::time::Instant::now();
        } else if quiet.elapsed() >= Duration::from_millis(SETTLE_MS) {
            break;
        }
        if start.elapsed() > SETTLE_TIMEOUT {
            break;
        }
        sleep(cx, 5).await;
    }
    true
}

/// Parses a script into steps.
fn parse(script: &str) -> Result<Vec<Step>, String> {
    script
        .split([';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| parse_step(s).map_err(|err| format!("`{s}`: {err}")))
        .collect()
}

fn parse_step(step: &str) -> Result<Step, String> {
    let (cmd, rest) = step.split_once(char::is_whitespace).unwrap_or((step, ""));
    let rest = rest.trim();
    let nums = || -> Result<Vec<f32>, String> {
        rest.split_whitespace()
            .map(|v| {
                v.parse::<f32>()
                    .map_err(|_| format!("`{v}` is not a number"))
            })
            .collect()
    };
    let exactly = |n: usize| -> Result<Vec<f32>, String> {
        let v = nums()?;
        if v.len() == n {
            Ok(v)
        } else {
            Err(format!("takes {n} numbers"))
        }
    };
    Ok(match cmd {
        "wait" => Step::Wait(exactly(1)?[0].max(0.0) as u64),
        "shot" => {
            let mut words = rest.split_whitespace();
            let path = words.next().ok_or("needs a file name")?.to_owned();
            let scale = match words.next() {
                Some(v) => v
                    .parse::<f32>()
                    .ok()
                    .filter(|s| *s > 0.0)
                    .ok_or("the scale is a positive number")?,
                None => 1.0,
            };
            Step::Shot { path, scale }
        }
        "tap" => {
            let v = exactly(2)?;
            Step::Tap(v[0], v[1])
        }
        "rightclick" => {
            let v = exactly(2)?;
            Step::RightClick(v[0], v[1])
        }
        "hover" => {
            let v = exactly(2)?;
            Step::Hover(v[0], v[1])
        }
        "drag" | "touch" => {
            let v = nums()?;
            if !(4..=5).contains(&v.len()) {
                return Err("takes x0 y0 x1 y1 [ms]".into());
            }
            Step::Drag {
                from: (v[0], v[1]),
                to: (v[2], v[3]),
                ms: v.get(4).copied().unwrap_or(300.0),
                touch: cmd == "touch",
            }
        }
        "wheel" | "pad" => {
            let v = exactly(4)?;
            Step::Wheel {
                x: v[0],
                y: v[1],
                dx: v[2],
                dy: v[3],
                precise: cmd == "pad",
            }
        }
        "type" => Step::Type(rest.to_owned()),
        "mark" => Step::Mark(rest.to_owned()),
        "key" => {
            let (vk, modifiers) = parse_key(rest).ok_or("unknown key")?;
            Step::Key(vk, modifiers)
        }
        "selectall" => Step::Key(
            b'A' as u16,
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ),
        "dark" | "light" => Step::Dark(cmd == "dark"),
        "resize" => {
            let v = exactly(2)?;
            if v[0] < 1.0 || v[1] < 1.0 {
                return Err("the size is positive".into());
            }
            Step::Resize(v[0], v[1])
        }
        "quit" => Step::Quit,
        _ => return Err("unknown step".into()),
    })
}

/// A key name (with `ctrl+` / `shift+` / `alt+` / `meta+` prefixes) as a
/// virtual key and modifiers.
fn parse_key(name: &str) -> Option<(u16, Modifiers)> {
    let mut modifiers = Modifiers::default();
    let mut key = name;
    while let Some((prefix, rest)) = key.split_once('+').filter(|(_, rest)| !rest.is_empty()) {
        match prefix.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" => modifiers.alt = true,
            "meta" | "super" | "cmd" | "win" => modifiers.meta = true,
            _ => return None,
        }
        key = rest;
    }
    let vk = match key {
        "" => return None,
        "Return" | "Enter" => 0x0D,
        "BackSpace" => 0x08,
        "Tab" => 0x09,
        "Escape" => 0x1B,
        "Space" => 0x20,
        "Page_Up" | "PageUp" => 0x21,
        "Page_Down" | "PageDown" => 0x22,
        "End" => 0x23,
        "Home" => 0x24,
        "Left" => 0x25,
        "Up" => 0x26,
        "Right" => 0x27,
        "Down" => 0x28,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "BrowserBack" => 0xA6,
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() => {
            k.as_bytes()[0].to_ascii_uppercase() as u16
        }
        k if k.starts_with(['F', 'f'])
            && k[1..].parse::<u16>().is_ok_and(|n| (1..=24).contains(&n)) =>
        {
            0x70 + k[1..].parse::<u16>().ok()? - 1
        }
        k => crate::keys::vk_of(&k.to_ascii_lowercase())?,
    };
    Some((vk, modifiers))
}

/// Writes `frame` (a window `content` points big) to `path` at `scale`
/// pixels per point. Returns the image's size.
fn save(
    frame: ::image::RgbaImage,
    content: gpui::Size<gpui::Pixels>,
    path: &str,
    scale: f32,
) -> anyhow::Result<(u32, u32)> {
    let (w, h) = (
        (f32::from(content.width) * scale).round().max(1.0) as u32,
        (f32::from(content.height) * scale).round().max(1.0) as u32,
    );
    let image = if (frame.width(), frame.height()) == (w, h) {
        frame
    } else {
        ::image::imageops::resize(&frame, w, h, ::image::imageops::FilterType::Triangle)
    };
    if let Some(dir) = std::path::Path::new(path)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        std::fs::create_dir_all(dir)?;
    }
    image.save(path)?;
    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_parse_into_steps() {
        let steps =
            parse("wait 800; shot out/a.png\ntap 10 20;; drag 0 0 0 100 ; key ctrl+a ; quit")
                .unwrap();
        assert_eq!(steps[0], Step::Wait(800));
        assert_eq!(
            steps[1],
            Step::Shot {
                path: "out/a.png".into(),
                scale: 1.0
            }
        );
        assert_eq!(steps[2], Step::Tap(10.0, 20.0));
        assert_eq!(
            steps[3],
            Step::Drag {
                from: (0.0, 0.0),
                to: (0.0, 100.0),
                ms: 300.0,
                touch: false
            }
        );
        assert!(matches!(steps[4], Step::Key(0x41, m) if m.ctrl && !m.shift));
        assert_eq!(steps[5], Step::Quit);
        assert_eq!(
            parse("type hello world").unwrap(),
            vec![Step::Type("hello world".into())]
        );
        assert_eq!(parse("mark").unwrap(), vec![Step::Mark(String::new())]);
        assert_eq!(
            parse("rightclick 5 6").unwrap(),
            vec![Step::RightClick(5.0, 6.0)]
        );
        assert!(matches!(
            parse("pad 1 2 3 4").unwrap()[0],
            Step::Wheel { precise: true, .. }
        ));
        assert_eq!(
            parse("shot a.png 2").unwrap()[0],
            Step::Shot {
                path: "a.png".into(),
                scale: 2.0
            }
        );
    }

    #[test]
    fn bad_steps_are_errors() {
        assert!(parse("tap 10").is_err());
        assert!(parse("tap 10 x").is_err());
        assert!(parse("jump 1 2").is_err());
        assert!(parse("shot").is_err());
        assert!(parse("shot a.png 0").is_err());
        assert!(parse("key Nope").is_err());
        assert!(parse("drag 1 2 3").is_err());
    }

    #[test]
    fn key_names_map_to_virtual_keys() {
        assert_eq!(parse_key("Return").unwrap().0, 0x0D);
        assert_eq!(parse_key("enter").unwrap().0, 0x0D);
        assert_eq!(parse_key("pagedown").unwrap().0, 0x22);
        assert_eq!(parse_key("F5").unwrap().0, 0x74);
        let (vk, m) = parse_key("ctrl+shift+z").unwrap();
        assert_eq!(vk, b'Z' as u16);
        assert!(m.ctrl && m.shift && !m.alt);
        assert!(parse_key("ctrl+").is_none());
        assert!(parse_key("Nope").is_none());
    }
}

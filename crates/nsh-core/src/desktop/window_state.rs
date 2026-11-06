//! What the app reads of its window and the displays (TS `nsh/desktop`'s
//! `windowState()` and `displays()`), and the bounds it saves and restores
//! a window with ([`WindowBounds`], GPUI's).
//!
//! Every rectangle is in points, in screen coordinates: from the primary
//! display's top left corner, `y` growing down, as
//! [`super::WindowFrame`] takes them.

use serde::{Deserialize, Serialize};

use crate::{
    element::{JsHost, JsIn, JsKind, JsObject, JsOwned, JsValue, PropError, SharedString, TsTypes},
    geometry::{Bounds, Point, Points, Size},
};

/// A display, the same from one launch of the app to the next on the same
/// machine (an id to compare, not to read: the platform's own name for the
/// display, hashed). TS: a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DisplayId(pub u64);

impl DisplayId {
    /// The id of the display the platform names `name` (a display's UUID,
    /// a monitor's device name, a connector): FNV-1a, which gives the same
    /// id for the same name in every process. For the platform backends,
    /// which all name displays this way.
    #[doc(hidden)]
    pub fn from_name(name: &[u8]) -> DisplayId {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in name {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        DisplayId(hash)
    }
}

/// TS: `string`, sixteen hexadecimal digits.
impl JsValue for DisplayId {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        value
            .as_string()
            .and_then(|s| u64::from_str_radix(&s, 16).ok())
            .map(DisplayId)
            .ok_or_else(|| value.expected("a display id"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        format!("{:016x}", self.0).to_js(host)
    }
}

/// A display (GPUI's `PlatformDisplay`): its area and the part of it
/// windows use.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Default, JsObject)]
pub struct Display {
    /// Which display it is, the same in the app's next launch.
    pub id: DisplayId,
    /// Its whole area, in screen coordinates.
    pub bounds: Bounds<Points>,
    /// The part windows use: its area without the menu bar, the Dock or the
    /// taskbar (the whole area where the platform does not tell).
    pub visible_bounds: Bounds<Points>,
    /// The primary display, whose top left corner screen coordinates start
    /// from.
    pub primary: bool,
}

impl Display {
    /// A display with `id`, `bounds` and `visible_bounds`.
    pub fn new(
        id: DisplayId,
        bounds: Bounds<Points>,
        visible_bounds: Bounds<Points>,
        primary: bool,
    ) -> Display {
        Display {
            id,
            bounds,
            visible_bounds,
            primary,
        }
    }
}

/// A window's mode and its windowed bounds (GPUI's `WindowBounds`): what
/// an app saves to open its window as the user left it, and gives
/// [`crate::element::desktop::set_window_bounds`] to restore it. A
/// maximized or full screen window carries the bounds it returns to.
///
/// TS: `{ mode: "windowed" | "maximized" | "fullscreen"; bounds: Bounds }`,
/// which is also its serde form: what one entry saves `JSON.stringify`
/// reads back with `serde_json`, and the other way round.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "bounds", rename_all = "lowercase")]
pub enum WindowBounds {
    /// A window of its own size, at these bounds.
    Windowed(Bounds<Points>),
    /// Maximized (zoomed on macOS); these bounds when it is restored.
    Maximized(Bounds<Points>),
    /// In full screen; these bounds when it leaves it.
    Fullscreen(Bounds<Points>),
}

impl Default for WindowBounds {
    fn default() -> Self {
        WindowBounds::Windowed(Bounds::default())
    }
}

/// The least of a window's top strip ([`TITLE_STRIP`] tall) that stays
/// on a display's visible area for `fit_on_displays` to leave it where it
/// is: enough to take it by its title bar.
const GRAB: Size<Points> = Size {
    width: Points(100.0),
    height: Points(20.0),
};

/// The top of a window that holds its title bar.
const TITLE_STRIP: Points = Points(40.0);

impl WindowBounds {
    /// Its windowed bounds (GPUI's `get_bounds`): where the window is, or
    /// returns to.
    pub fn get_bounds(&self) -> Bounds<Points> {
        match self {
            WindowBounds::Windowed(b)
            | WindowBounds::Maximized(b)
            | WindowBounds::Fullscreen(b) => *b,
        }
    }

    /// The same mode with `bounds`.
    pub fn with_bounds(self, bounds: Bounds<Points>) -> WindowBounds {
        match self {
            WindowBounds::Windowed(_) => WindowBounds::Windowed(bounds),
            WindowBounds::Maximized(_) => WindowBounds::Maximized(bounds),
            WindowBounds::Fullscreen(_) => WindowBounds::Fullscreen(bounds),
        }
    }

    /// These bounds on the displays there are now (saved bounds of a
    /// display the user has since unplugged, or of another arrangement):
    /// unchanged when at least 100 × 20 points of the window's top 40
    /// points are on a display's visible area, where the user can take it
    /// by its title bar; otherwise centered on the primary display's
    /// visible area (the first display's without one), no larger than it.
    /// A size that is not finite and positive takes the visible area's.
    /// The mode stays. Without displays (off desktops) the bounds stay.
    /// What `set_window_bounds` does to the bounds it is given.
    pub(crate) fn fit_on_displays(self, displays: &[Display]) -> WindowBounds {
        let bounds = self.get_bounds();
        let valid = |v: Points| v.0.is_finite() && v.0 > 0.0;
        let sized = valid(bounds.size.width)
            && valid(bounds.size.height)
            && bounds.origin.x.0.is_finite()
            && bounds.origin.y.0.is_finite();
        if sized && displays.iter().any(|d| grabbable(bounds, d.visible_bounds)) {
            return self;
        }
        let Some(display) = displays
            .iter()
            .find(|d| d.primary)
            .or_else(|| displays.first())
        else {
            return self;
        };
        let area = display.visible_bounds;
        let fit = |v: Points, max: Points| if valid(v) { v.min(max) } else { max };
        let size = Size {
            width: fit(bounds.size.width, area.size.width),
            height: fit(bounds.size.height, area.size.height),
        };
        let origin = Point {
            x: area.origin.x + (area.size.width - size.width) / 2.0,
            y: area.origin.y + (area.size.height - size.height) / 2.0,
        };
        self.with_bounds(Bounds::new(origin, size))
    }
}

/// Whether enough of `bounds`' top strip is on `area` ([`GRAB`]).
fn grabbable(bounds: Bounds<Points>, area: Bounds<Points>) -> bool {
    let strip = Bounds::new(
        bounds.origin,
        Size {
            width: bounds.size.width,
            height: bounds.size.height.min(TITLE_STRIP),
        },
    );
    if !strip.intersects(&area) {
        return false;
    }
    let on = strip.intersect(&area);
    on.size.width >= GRAB.width.min(strip.size.width)
        && on.size.height >= GRAB.height.min(strip.size.height)
}

impl WindowBounds {
    fn mode(&self) -> &'static str {
        match self {
            WindowBounds::Windowed(_) => "windowed",
            WindowBounds::Maximized(_) => "maximized",
            WindowBounds::Fullscreen(_) => "fullscreen",
        }
    }
}

impl JsValue for WindowBounds {
    fn ts_type(types: &mut TsTypes) -> String {
        types.declare("WindowBounds", |_| {
            "/** A window's mode and its windowed bounds: what an app saves to open its window as the user left it (`Window.setBounds`). */\n\
             export interface WindowBounds {\n  \
             /** Windowed, maximized (zoomed on macOS) or in full screen. */\n  \
             mode: \"windowed\" | \"maximized\" | \"fullscreen\";\n  \
             /** Its bounds when windowed: where it is, or returns to, in points, in screen coordinates. */\n  \
             bounds: Bounds;\n}"
                .into()
        })
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.kind() != JsKind::Object {
            return Err(value.expected("an object with mode and bounds"));
        }
        let bounds = value.field("bounds")?;
        let bounds = Bounds::<Points>::from_js(bounds.get())
            .map_err(|e| PropError(format!("`bounds`: {e}")))?;
        let mode = value.field("mode")?;
        let mode = mode.get();
        Ok(match mode.as_string().as_deref() {
            None if mode.is_nullish() => WindowBounds::Windowed(bounds),
            Some("windowed") => WindowBounds::Windowed(bounds),
            Some("maximized") => WindowBounds::Maximized(bounds),
            Some("fullscreen") => WindowBounds::Fullscreen(bounds),
            _ => {
                return Err(PropError(
                    "`mode`: expected \"windowed\", \"maximized\" or \"fullscreen\"".into(),
                ));
            }
        })
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        let owned = JsOwned::new(host, host.new_object()?);
        let object = owned.get().raw();
        host.set_field(
            object,
            "mode",
            SharedString::new_static(self.mode())
                .to_js(host)?
                .into_raw(),
        )?;
        host.set_field(object, "bounds", self.get_bounds().to_js(host)?.into_raw())?;
        Ok(owned)
    }
}

/// The app's window as the platform reports it (TS `windowState()`,
/// Rust [`crate::element::desktop::window_state`]). A move or a resize is
/// reported once the window stops for a moment; a change of mode, of
/// display or of being minimized or hidden at once.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Default, JsObject)]
pub struct WindowState {
    /// Its outer frame now, window decorations included, in screen
    /// coordinates (a maximized or full screen window's as it shows).
    pub bounds: Bounds<Points>,
    /// Its mode and windowed bounds: what to save to open it the same way
    /// next time.
    pub window_bounds: WindowBounds,
    /// It is minimized (in the Dock, the taskbar).
    pub minimized: bool,
    /// It is off the screen without being minimized: the app is hidden
    /// (macOS's Hide), or the window is not shown yet or any more. A window
    /// on the screen is `!minimized && !hidden`; one there that
    /// `lifecycle()` reports in the background is covered by other windows.
    pub hidden: bool,
    /// The display it is on (the one with most of it).
    pub display: Option<Display>,
}

impl WindowState {
    /// A window at `bounds` in `window_bounds`' mode.
    pub fn new(bounds: Bounds<Points>, window_bounds: WindowBounds) -> WindowState {
        WindowState {
            bounds,
            window_bounds,
            ..WindowState::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{point, px, size};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Points> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn displays() -> Vec<Display> {
        vec![
            // A laptop's screen under a menu bar, and a display to its right.
            Display::new(
                DisplayId(1),
                rect(0.0, 0.0, 1440.0, 900.0),
                rect(0.0, 25.0, 1440.0, 875.0),
                true,
            ),
            Display::new(
                DisplayId(2),
                rect(1440.0, 0.0, 1920.0, 1080.0),
                rect(1440.0, 0.0, 1920.0, 1080.0),
                false,
            ),
        ]
    }

    #[test]
    fn display_ids_are_stable_hashes_of_names() {
        assert_eq!(DisplayId::from_name(b""), DisplayId(0xcbf2_9ce4_8422_2325));
        assert_eq!(DisplayId::from_name(b"a"), DisplayId(0xaf63_dc4c_8601_ec8c));
        assert_ne!(
            DisplayId::from_name(b"\\\\.\\DISPLAY1"),
            DisplayId::from_name(b"\\\\.\\DISPLAY2")
        );
    }

    #[test]
    fn bounds_on_a_display_stay() {
        let on_second = WindowBounds::Windowed(rect(2000.0, 100.0, 800.0, 600.0));
        assert_eq!(on_second.fit_on_displays(&displays()), on_second);
        // Across two displays.
        let across = WindowBounds::Maximized(rect(1000.0, 200.0, 900.0, 600.0));
        assert_eq!(across.fit_on_displays(&displays()), across);
        // Mostly off the screen, but its title bar can be taken.
        let corner = WindowBounds::Windowed(rect(-700.0, 860.0, 800.0, 600.0));
        assert_eq!(corner.fit_on_displays(&displays()), corner);
    }

    #[test]
    fn bounds_off_every_display_move_to_the_primary_one() {
        // The display on the left is gone.
        let gone = WindowBounds::Windowed(rect(-1500.0, 100.0, 800.0, 600.0));
        assert_eq!(
            gone.fit_on_displays(&displays()),
            WindowBounds::Windowed(rect(320.0, 162.5, 800.0, 600.0))
        );
        // Larger than the primary display: as large as its visible area.
        let large = WindowBounds::Fullscreen(rect(5000.0, 0.0, 2560.0, 1440.0));
        assert_eq!(
            large.fit_on_displays(&displays()),
            WindowBounds::Fullscreen(rect(0.0, 25.0, 1440.0, 875.0))
        );
        // Only its bottom edge shows: its title bar is above the screen.
        let above = WindowBounds::Windowed(rect(100.0, -590.0, 800.0, 600.0));
        assert_eq!(
            above.fit_on_displays(&displays()),
            WindowBounds::Windowed(rect(320.0, 162.5, 800.0, 600.0))
        );
        // Too little of its title bar shows to take it.
        let sliver = WindowBounds::Windowed(rect(-750.0, 100.0, 800.0, 600.0));
        assert_ne!(sliver.fit_on_displays(&displays()), sliver);
    }

    #[test]
    fn invalid_bounds_take_the_visible_area() {
        let nan = WindowBounds::Windowed(rect(f32::NAN, 0.0, 0.0, -1.0));
        assert_eq!(
            nan.fit_on_displays(&displays()),
            WindowBounds::Windowed(rect(0.0, 25.0, 1440.0, 875.0))
        );
    }

    #[test]
    fn without_displays_the_bounds_stay() {
        let gone = WindowBounds::Windowed(rect(-1500.0, 100.0, 800.0, 600.0));
        assert_eq!(gone.fit_on_displays(&[]), gone);
        // No primary display: the first one.
        let second = &displays()[1..];
        assert_eq!(
            gone.fit_on_displays(second),
            WindowBounds::Windowed(rect(2000.0, 240.0, 800.0, 600.0))
        );
    }

    #[test]
    fn bounds_save_as_the_ts_object() {
        let bounds = WindowBounds::Maximized(rect(10.0, 20.5, 800.0, 600.0));
        let json = serde_json::to_string(&bounds).unwrap();
        assert_eq!(
            json,
            r#"{"mode":"maximized","bounds":{"origin":{"x":10.0,"y":20.5},"size":{"width":800.0,"height":600.0}}}"#
        );
        assert_eq!(serde_json::from_str::<WindowBounds>(&json).unwrap(), bounds);
        // As `JSON.stringify` writes it.
        let ts = r#"{"mode":"fullscreen","bounds":{"origin":{"x":-1,"y":0},"size":{"width":800,"height":600}}}"#;
        assert_eq!(
            serde_json::from_str::<WindowBounds>(ts).unwrap(),
            WindowBounds::Fullscreen(rect(-1.0, 0.0, 800.0, 600.0))
        );
    }
}

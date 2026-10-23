//! What the popups of `nsh/base` share (`Popover`, `Menu`, `ContextMenu`,
//! `Tooltip`, `Sheet`): their open state, controlled or not, and why it
//! changed. They show their content in the core's overlays
//! (`nsh_core::ui::overlay`), which place it and report presses outside,
//! Escape and the back button; deciding to close is theirs.
//!
//! How a popup presents itself follows the device (`auto`, the only
//! presentation for now): on the desktops (and desktop browsers) a menu is
//! a popup at its trigger or the pointer, with cascading submenus, and a
//! press outside closes a popup and goes on to what is under it; on phones
//! and tablets a menu is a sheet at the bottom of the screen whose
//! submenus replace its items, and a press outside a popup only closes it
//! (a transparent backdrop takes it). A popover stays anchored to its
//! trigger everywhere: what it shows belongs next to it.

use nsh_core::ui::prelude::*;

/// Why a popup opened or closed (`onOpenChange`'s `reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PropEnum)]
pub enum OpenChangeReason {
    /// Its trigger was pressed.
    TriggerPress,
    /// A press went down outside it.
    PressOutside,
    /// Its backdrop was pressed (a sheet's).
    BackdropPress,
    /// Escape or the Android back button.
    Cancel,
    /// An item was chosen (a menu's).
    Select,
    /// The pointer rested on its trigger, or left or pressed it (a
    /// tooltip's).
    Hover,
    /// A finger was held on its trigger (a context menu's on touch
    /// screens).
    LongPress,
    /// The secondary button was pressed on its trigger (a context menu's).
    ContextMenu,
}

/// Whether a popup is open: the app's `open` when it sets one, else its own
/// state (from `defaultOpen`).
#[derive(Clone, Copy)]
pub(crate) struct OpenState {
    controlled: Prop<Option<bool>>,
    own: Signal<bool>,
}

impl OpenState {
    pub(crate) fn new(open: Prop<Option<bool>>, default_open: Prop<bool>) -> Self {
        Self {
            controlled: open,
            own: signal(default_open.get_untracked()),
        }
    }

    /// Whether it is open (tracked).
    pub(crate) fn get(self) -> bool {
        self.controlled.get().unwrap_or_else(|| self.own.get())
    }

    pub(crate) fn get_untracked(self) -> bool {
        self.controlled
            .get_untracked()
            .unwrap_or_else(|| self.own.get_untracked())
    }

    /// The user asks for it to be `open`: its own state follows when the
    /// app does not control it; whether it is a change to report.
    pub(crate) fn request(self, open: bool) -> bool {
        if self.get_untracked() == open {
            return false;
        }
        if self.controlled.get_untracked().is_none() {
            self.own.set(open);
        }
        true
    }
}

/// A popup's lifecycle, the same for every one (as `Dialog`'s): `open` once
/// it is all there (a sheet after it came in), `close` once it is all gone
/// (after it went), whoever opened or closed it. `shown` is tracked; its
/// first value only counts when it is shown.
pub(crate) fn lifecycle(
    shown: impl Fn() -> bool + 'static,
    open: impl Fn() + 'static,
    close: impl Fn() + 'static,
) {
    let was = stored(false);
    effect(move || {
        let now = shown();
        match (was.replace(now), now) {
            (false, true) => untracked(&open),
            (true, false) => untracked(&close),
            _ => {}
        }
    });
}

/// How long a sheet takes to come in and to go, in milliseconds.
const SHEET_IN_MS: f32 = 280.0;
const SHEET_OUT_MS: f32 = 200.0;

/// How a sheet comes and goes (`Sheet`, and menus presented as sheets): its
/// backdrop fades while its panel slides in from its edge, and out again
/// before they leave the view; at once when the user asked for less motion.
/// The motion is the sheet's behavior, not a look: native animated values,
/// bound to the parts the app styles.
#[derive(Clone, Copy)]
pub(crate) struct SheetMotion {
    /// The parts are in the view: while open, and while they go.
    pub(crate) shown: Signal<bool>,
    /// It is all there: it came in and has not started to go.
    pub(crate) settled: Signal<bool>,
    /// The backdrop's opacity.
    pub(crate) fade: AnimatedValue,
    /// The panel's translation.
    pub(crate) x: AnimatedValue,
    pub(crate) y: AnimatedValue,
    side: Stored<Placement>,
    /// Laid out once open: the panel's size, where it comes from.
    entering: Stored<bool>,
    /// The panel's size as last laid out, where it goes back to.
    panel: Stored<Option<Size<Points>>>,
    /// Going: it leaves the view once its motion's time is over.
    closing: Stored<bool>,
    /// The panel stays mounted while closed ([`Self::keeping_panel`]).
    kept: Stored<bool>,
}

impl SheetMotion {
    /// A motion that follows `open`, the panel on `side`.
    pub(crate) fn new(
        open: impl Fn() -> bool + 'static,
        side: impl Fn() -> Placement + 'static,
    ) -> Self {
        let motion = Self {
            shown: signal(false),
            settled: signal(false),
            fade: animated_value(0.0),
            x: animated_value(0.0),
            y: animated_value(0.0),
            side: stored(Placement::Bottom),
            entering: stored(false),
            panel: stored(None),
            closing: stored(false),
            kept: stored(false),
        };
        effect(move || {
            let open = open();
            let side = side();
            untracked(|| motion.follow(open, side));
        });
        motion
    }

    /// For a panel that stays mounted while closed (a dock's drawer, which
    /// keeps its panels' state): it keeps its frame, so no layout comes
    /// to say its size when it opens again, and it comes in from as far
    /// as it was last laid out.
    pub(crate) fn keeping_panel(self) -> Self {
        self.kept.replace(true);
        self
    }

    fn follow(self, open: bool, side: Placement) {
        self.side.replace(side);
        if open {
            if self.closing.replace(false) {
                // Opened again on its way out: back in from where it is.
                self.come_in();
                return;
            }
            if self.shown.get_untracked() {
                return;
            }
            self.fade.set(0.0);
            self.shown.set(true);
            // A kept panel laid out before says its size no more.
            let known = self.kept.get().then(|| self.panel.with(|p| *p)).flatten();
            if let Some(size) = known {
                let (x, y) = away(side, size);
                self.x.set(x);
                self.y.set(y);
                self.come_in();
                return;
            }
            // Out of sight until the panel is laid out: as far as the window.
            let (x, y) = away(side, window());
            self.x.set(x);
            self.y.set(y);
            self.entering.replace(true);
        } else if self.shown.get_untracked() && !self.closing.with(|c| *c) {
            self.entering.replace(false);
            if reduce_motion() {
                self.settled.set(false);
                self.shown.set(false);
                return;
            }
            // Going before it stops being settled: it is there until gone.
            self.closing.replace(true);
            self.settled.set(false);
            let (x, y) = away(side, self.size());
            self.x.timing(x, SHEET_OUT_MS, Easing::EaseIn);
            self.y.timing(y, SHEET_OUT_MS, Easing::EaseIn);
            let end = self.fade.timing(0.0, SHEET_OUT_MS, Easing::EaseIn);
            let (shown, closing) = (self.shown, self.closing);
            // Gone when the motion ends: the platform runs it and reports
            // its end, whatever clock the app's timers follow (a test's
            // manual one moves only as the test says).
            spawn(async move {
                if end.await && closing.replace(false) {
                    shown.set(false);
                }
            });
            // Or after the motion's time, whether or not its end is
            // reported (a platform that stops it, one that runs none).
            set_timeout(
                std::time::Duration::from_millis(SHEET_OUT_MS as u64),
                move || {
                    if closing.replace(false) {
                        shown.set(false);
                    }
                },
            );
        }
    }

    /// Whether it is there for its lifecycle (tracked): from when it came
    /// in until it is gone.
    pub(crate) fn there(self) -> bool {
        self.settled.get() || (self.shown.get() && self.closing.with(|c| *c))
    }

    /// The panel's size, as last laid out (the window's before).
    fn size(self) -> Size<Points> {
        self.panel.with(|p| *p).unwrap_or_else(window)
    }

    /// The panel was laid out at `size`: it comes in from as far.
    pub(crate) fn laid_out(self, size: Size<Points>) {
        self.panel.replace(Some(size));
        if !self.entering.replace(false) {
            return;
        }
        let (x, y) = away(self.side.with(|s| *s), size);
        self.x.set(x);
        self.y.set(y);
        self.come_in();
    }

    /// From where it is to its place, the backdrop to opaque; settled once
    /// it is there.
    fn come_in(self) {
        if reduce_motion() {
            self.x.set(0.0);
            self.y.set(0.0);
            self.fade.set(1.0);
            self.settled.set(true);
            return;
        }
        self.x.timing(0.0, SHEET_IN_MS, Easing::EaseOut);
        self.y.timing(0.0, SHEET_IN_MS, Easing::EaseOut);
        let end = self.fade.timing(1.0, SHEET_IN_MS, Easing::EaseOut);
        let (settled, closing) = (self.settled, self.closing);
        // There when the motion ends, or after its time (as it goes).
        spawn(async move {
            if end.await && !closing.with(|c| *c) {
                settled.set(true);
            }
        });
        set_timeout(
            std::time::Duration::from_millis(SHEET_IN_MS as u64),
            move || {
                if !closing.with(|c| *c) {
                    settled.set(true);
                }
            },
        );
    }
}

/// The translation that puts a panel of `size` out of sight past `side`.
fn away(side: Placement, size: Size<Points>) -> (f32, f32) {
    match side {
        Placement::Bottom => (0.0, size.height.0),
        Placement::Top => (0.0, -size.height.0),
        Placement::Left => (-size.width.0, 0.0),
        Placement::Right => (size.width.0, 0.0),
    }
}

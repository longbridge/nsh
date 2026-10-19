//! `nsh/desktop`'s window functions (macOS, Windows, Linux; nothing
//! happens elsewhere). Menus, which call the app's functions, are
//! `nsh-js`'s.

use crate::{
    Command, ROOT_ID,
    desktop::{DesktopCommand, TitleBarStyle, WindowFrame, WindowLevel},
    ui::{ModuleCx, module, task::Action},
};

fn desktop(cx: &mut ModuleCx, command: DesktopCommand) {
    cx.act(Action::Command(
        ROOT_ID,
        Command::Desktop(Box::new(command)),
    ));
}

/// The app's window.
pub(crate) struct Window;

/// The app's window (desktops; its functions do nothing elsewhere).
#[module]
impl Window {
    /// Sets the window's title (the title bar, the taskbar / Dock, the
    /// window switcher).
    pub(crate) fn set_title(title: String, cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::SetTitle(title));
    }

    /// Shows the system's title bar (`"default"`) or hides it
    /// (`"hidden"`): the app then fills the window up to its top edge and
    /// draws a title bar of its own, made draggable with
    /// `desktop={{ windowDrag: true }}`; the system's window controls stay,
    /// in the area `titleBar()` reports. `height` is the app's bar's height
    /// in points: the controls are centered in it (Windows' caption buttons
    /// fill it); the platform's own without it. On macOS, it selects compact
    /// chrome up to 44 points and regular chrome above; use `titleBar().height`
    /// to lay out the bar at AppKit's actual height. An embedding host's
    /// existing toolbar keeps its own style and height.
    pub(crate) fn set_title_bar(style: TitleBarStyle, height: Option<f32>, cx: &mut ModuleCx) {
        let height = crate::desktop::title_bar_height(height);
        desktop(cx, DesktopCommand::SetTitleBar { style, height });
    }

    /// Minimizes the window.
    pub(crate) fn minimize(cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::Minimize);
    }

    /// Maximizes the window, or restores it when it is maximized.
    pub(crate) fn toggle_maximize(cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::ToggleMaximize);
    }

    /// Enters or leaves full screen.
    pub(crate) fn toggle_full_screen(cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::ToggleFullScreen);
    }

    /// Moves and sizes the window in points; omitted fields keep their values.
    /// macOS and Windows use the outer frame, including window decorations;
    /// Linux uses GTK's default window size. `x` and `y` are the bottom left
    /// corner in macOS screen coordinates and the top left on Windows.
    /// `animated` enables macOS's resize animation (false by default).
    /// Sizing is supported on macOS, Windows and Linux; positioning and levels
    /// on macOS and Windows. A view embedded in another app's window leaves
    /// its bounds and level to that app.
    pub(crate) fn set_frame(frame: WindowFrame, cx: &mut ModuleCx) {
        let frame = WindowFrame {
            x: frame.x.filter(|v| v.is_finite()),
            y: frame.y.filter(|v| v.is_finite()),
            width: frame.width.filter(|v| v.is_finite() && *v > 0.0),
            height: frame.height.filter(|v| v.is_finite() && *v > 0.0),
            animated: frame.animated,
        };
        desktop(cx, DesktopCommand::SetFrame(frame));
    }

    /// Sets the window's level on macOS and Windows: `"normal"` restores the
    /// normal level; `"floating"` stays above normal windows. `"desktopIcon"`
    /// selects macOS's desktop icon level or moves a Windows window to the
    /// bottom of the current z-order, where later activation can raise it.
    /// Elsewhere the level is left to the host and the window manager.
    pub(crate) fn set_level(level: WindowLevel, cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::SetLevel(level));
    }

    /// Closes the window; the app ends with it.
    pub(crate) fn close(cx: &mut ModuleCx) {
        desktop(cx, DesktopCommand::Close);
    }
}

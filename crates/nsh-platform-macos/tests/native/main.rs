//! Native checks need AppKit on the process main thread and a desktop session.
//!
//! ```sh
//! cargo test -p nsh-platform-macos --test native -- --ignored
//! ```
//!
//! `mount.rs` also measures mounting rows that are not virtualized: with
//! `NSH_MOUNT_BENCH=1` after the checks, with `NSH_MOUNT_BENCH_ONLY=1`
//! alone (no other check runs first, so runs before and after a change
//! measure the same workload in the same process state):
//!
//! ```sh
//! NSH_MOUNT_BENCH_ONLY=1 cargo test --release -p nsh-platform-macos --test native -- --ignored
//! ```
#[cfg(target_os = "macos")]
mod embedded_scroll;
#[cfg(target_os = "macos")]
mod fps_collapse;
#[cfg(target_os = "macos")]
mod ime;
#[cfg(target_os = "macos")]
mod input_hidden;
#[cfg(target_os = "macos")]
mod keyboard;
#[cfg(target_os = "macos")]
mod menu_labels;
#[cfg(target_os = "macos")]
mod mount;
#[cfg(target_os = "macos")]
mod press_momentum;
#[cfg(target_os = "macos")]
mod title_bar;
#[cfg(target_os = "macos")]
mod window_alert;
#[cfg(target_os = "macos")]
mod window_dialog;
#[cfg(target_os = "macos")]
mod window_frames;
#[cfg(target_os = "macos")]
mod window_hover;
#[cfg(target_os = "macos")]
mod window_press;
#[cfg(target_os = "macos")]
mod zoom;
fn main() {
    if !std::env::args().any(|arg| arg == "--ignored") {
        println!("Native AppKit checks skipped; run with --ignored in a desktop session.");
    } else {
        #[cfg(target_os = "macos")]
        {
            if std::env::var_os("NSH_TITLE_BAR_CHECK_ONLY").is_some() {
                title_bar::run();
            } else if mount::bench_only() {
                mount::bench_alone();
            } else if std::env::var_os("NSH_MENU_LABELS_CHECK_ONLY").is_some() {
                window_press::run();
                menu_labels::run();
            } else if std::env::var_os("NSH_MOUNT_CHECK_ONLY").is_some() {
                mount::run();
            } else if std::env::var_os("NSH_WINDOW_DIALOG_CHECK_ONLY").is_some() {
                window_dialog::run();
            } else if std::env::var_os("NSH_WINDOW_FRAMES_CHECK_ONLY").is_some() {
                window_frames::run();
            } else if std::env::var_os("NSH_WINDOW_ALERT_CHECK_ONLY").is_some() {
                window_alert::run();
            } else if std::env::var_os("NSH_INPUT_HIDDEN_CHECK_ONLY").is_some() {
                input_hidden::run();
            } else if std::env::var_os("NSH_FPS_COLLAPSE_CHECK_ONLY").is_some() {
                fps_collapse::run();
            } else if std::env::var_os("NSH_ZOOM_CHECK_ONLY").is_some()
                || std::env::var_os("NSH_ZOOM_PINCH_CHECK_ONLY").is_some()
            {
                zoom::run();
            } else if std::env::var_os("NSH_IME_CHECK_ONLY").is_some() {
                ime::run();
            } else if std::env::var_os("NSH_KEYBOARD_CHECK_ONLY").is_some() {
                window_press::run();
                keyboard::run();
            } else if std::env::var_os("NSH_EMBEDDED_SCROLL_CHECK_ONLY").is_some() {
                embedded_scroll::run();
            } else if std::env::var_os("NSH_PRESS_MOMENTUM_CHECK_ONLY").is_some() {
                press_momentum::run();
            } else {
                window_press::run();
                window_hover::run();
                title_bar::run();
                window_dialog::run();
                menu_labels::run();
                window_frames::run();
                keyboard::run();
                ime::run();
                input_hidden::run();
                fps_collapse::run();
                zoom::run();
                mount::run();
                // Last: it makes the process an accessory app.
                embedded_scroll::run();
                // Last: it leaves the app an accessory, never activated.
                press_momentum::run();
            }
        }
    }
}

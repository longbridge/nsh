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
mod input_hidden;
#[cfg(target_os = "macos")]
mod mount;
#[cfg(target_os = "macos")]
mod title_bar;
#[cfg(target_os = "macos")]
mod window_dialog;
#[cfg(target_os = "macos")]
mod window_hover;
#[cfg(target_os = "macos")]
mod window_press;
fn main() {
    if !std::env::args().any(|arg| arg == "--ignored") {
        println!("Native AppKit checks skipped; run with --ignored in a desktop session.");
    } else {
        #[cfg(target_os = "macos")]
        {
            if mount::bench_only() {
                mount::bench_alone();
            } else if std::env::var_os("NSH_MOUNT_CHECK_ONLY").is_some() {
                mount::run();
            } else if std::env::var_os("NSH_WINDOW_DIALOG_CHECK_ONLY").is_some() {
                window_dialog::run();
            } else if std::env::var_os("NSH_INPUT_HIDDEN_CHECK_ONLY").is_some() {
                input_hidden::run();
            } else {
                window_press::run();
                window_hover::run();
                title_bar::run();
                window_dialog::run();
                input_hidden::run();
                mount::run();
            }
        }
    }
}

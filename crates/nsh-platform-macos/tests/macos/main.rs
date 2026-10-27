//! AppKit integration checks run on the process's real main thread.

#[cfg(target_os = "macos")]
mod window_background;

fn main() {
    #[cfg(target_os = "macos")]
    window_background::run();
}

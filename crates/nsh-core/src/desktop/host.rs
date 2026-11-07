//! What can go wrong when a host opens its own window (the platforms'
//! `HostWindow`), reported by [`HostWindowError`].

use std::fmt;

/// Why opening a [`HostWindow`](crate::platform) failed.
///
/// The platforms open a host's second window on the thread that runs the
/// app; the error says when a caller tried elsewhere.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostWindowError {
    /// The window was opened on a thread other than the one that runs the
    /// app (the main thread on macOS and Linux, the app's own thread on
    /// Windows).
    WrongThread,
    /// The window's options were invalid; the string says how.
    Invalid(&'static str),
}

impl fmt::Display for HostWindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongThread => {
                write!(
                    f,
                    "a host window must be opened on the thread that runs the app"
                )
            }
            Self::Invalid(what) => write!(f, "invalid host window options: {what}"),
        }
    }
}

impl std::error::Error for HostWindowError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_says_where_a_host_window_belongs() {
        assert_eq!(
            HostWindowError::WrongThread.to_string(),
            "a host window must be opened on the thread that runs the app"
        );
        assert_eq!(
            HostWindowError::Invalid("the size is negative").to_string(),
            "invalid host window options: the size is negative"
        );
    }
}

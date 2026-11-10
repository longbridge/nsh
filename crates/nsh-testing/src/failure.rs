//! The failures of steps a test may expect, and their `code`s: the
//! drivers (headless, a real platform's agent) fail with messages for
//! people, made here, and `nsh/test` gives the error of each a `code`
//! ([`code_of`]) a test matches instead of the message, which may change:
//! `await expect(app.press("Pay")).rejects.toMatchObject({ code:
//! "ERR_NSH_TEST_DISABLED" })`.

use std::time::Duration;

/// No view on screen matches the selector (a menu has no such item).
#[cfg(feature = "js")]
pub(crate) const NOT_FOUND: &str = "ERR_NSH_TEST_NOT_FOUND";
/// The view the selector names is disabled (a scroll view the user cannot
/// scroll, a disabled menu item).
#[cfg(feature = "js")]
pub(crate) const DISABLED: &str = "ERR_NSH_TEST_DISABLED";
/// Another view is drawn over the one the selector names and would take
/// a real press (an overlay over a button).
#[cfg(feature = "js")]
pub(crate) const COVERED: &str = "ERR_NSH_TEST_COVERED";
/// `back()` found nothing to go back from.
#[cfg(feature = "js")]
pub(crate) const NO_BACK: &str = "ERR_NSH_TEST_NO_BACK";
/// A wait ran out of time (`waitFor`, `waitUntil`, the app's first
/// render).
#[cfg(feature = "js")]
pub(crate) const TIMED_OUT: &str = nsh_core::element::codes::TIMED_OUT;
/// `open` was given an option the platform of `nsh test --target` cannot
/// honor (a size on a device's screen, a platform of another kind of
/// device, the embedding app's handlers), or a step it cannot take
/// (`forward` off the web).
#[cfg(feature = "js")]
pub(crate) const UNSUPPORTED: &str = "ERR_NSH_TEST_UNSUPPORTED";

/// No `what` (`"view"`, `"pressable view"`, `"input"`, ...) matches
/// `selector`.
pub(crate) fn not_found(what: &str, selector: impl std::fmt::Display) -> String {
    format!("no {what} matches `{selector}`")
}

/// The `what` `selector` names takes no step: it is disabled.
pub(crate) fn disabled(selector: impl std::fmt::Display, what: &str) -> String {
    if what == "scroll view" {
        format!("`{selector}` does not scroll: the user cannot scroll it (`scrollable` is false)")
    } else {
        format!("`{selector}` is disabled")
    }
}

/// The view `selector` names is under `cover`, which a press there
/// reaches instead.
pub(crate) fn covered(selector: impl std::fmt::Display, cover: &str) -> String {
    format!("`{selector}` is covered: a press there reaches {cover}")
}

/// The menu item `label` (a submenu's when `submenu`) is disabled.
pub(crate) fn menu_disabled(label: &str, submenu: bool) -> String {
    let what = if submenu { "submenu" } else { "menu item" };
    format!("the {what} {label:?} is disabled")
}

/// The open menu has no item `label`.
pub(crate) fn no_menu_item(label: &str) -> String {
    format!("the menu has no item {label:?}")
}

/// `what` (`` `Saved` showing ``) did not happen within `timeout`.
pub(crate) fn never_happened(what: &str, timeout: Duration) -> String {
    format!("{what} never happened in {} ms", timeout.as_millis())
}

/// The app showed nothing within `timeout` of its start.
pub(crate) fn rendered_nothing(timeout: Duration) -> String {
    format!("the app rendered nothing in {} s", timeout.as_secs())
}

/// `open(entry)` was given `option`, which `--target target` cannot
/// honor: `why`.
#[cfg(feature = "js")]
pub(crate) fn unsupported(entry: &str, option: &str, target: &str, why: &str) -> String {
    format!("open({entry:?}): `{option}` is unsupported on --target {target}: {why}")
}

/// What a test app's sockets and WebSockets fail with (`net.connect`,
/// `WebSocket.connect`; the global `WebSocket` fires `error` and `close`)
/// while its test keeps it off the network: `open`'s `network` other than
/// `"live"`, under which a request no route answers fails too.
#[cfg(any(feature = "js", feature = "agent"))]
pub(crate) const NETWORK: &str = "ERR_NSH_TEST_NETWORK";

/// Refuses a test app's connections with [`NETWORK`]
/// (`EngineConfig::refuse_connections`), naming what it connected to.
#[cfg(any(feature = "js", feature = "agent"))]
pub(crate) fn refuse_connections() -> nsh_core::element::RefuseConnections {
    std::sync::Arc::new(|target: &str| {
        nsh_core::element::JsError::Error(format!(
            "{target}: a test's app reaches the network only with `open(entry, {{ network: \"live\" }})`"
        ))
        .with_code(NETWORK)
    })
}

/// `back()` with nothing to go back from.
#[cfg(any(feature = "js", not(target_arch = "wasm32")))]
pub(crate) const NOTHING_TO_GO_BACK_FROM: &str = "nothing to go back from";

/// A step only the browser's history takes (`forward`), on another
/// platform: `ERR_NSH_TEST_UNSUPPORTED`.
#[cfg(any(feature = "js", feature = "agent"))]
pub(crate) fn web_only(what: &str) -> String {
    format!(
        "`{what}` is unsupported here: only the browser's history goes forward; run the test with `--target web`"
    )
}

/// How many characters of the texts on screen a failure shows: enough to
/// see what the screen held, not the whole of a long document, so a
/// failure cannot flood a terminal, a log or a tool's output. The CLI
/// shows all of a screen's texts with `-v`.
pub const SCREEN_TEXT_LIMIT: usize = 4096;

/// The texts on screen for a failure, as quoted strings joined by `, `
/// (`"Todos", "0 items left"`), cut to `limit` characters with
/// `… <n> more characters` after what is left, or all of them when
/// `limit` is `None`. A cut text keeps its beginning and an ellipsis, so
/// a long document cannot flood an error.
pub fn screen_texts<S: AsRef<str>>(texts: &[S], limit: Option<usize>) -> String {
    let total: usize = texts.iter().map(|t| t.as_ref().chars().count()).sum();
    let budget = limit.unwrap_or(total);
    let mut out = String::new();
    let mut used = 0usize;
    let mut cut = false;
    for text in texts {
        let text = text.as_ref();
        let room = budget.saturating_sub(used);
        if room == 0 {
            cut = true;
            break;
        }
        let length = text.chars().count();
        if length > room {
            let preview: String = text.chars().take(room).collect();
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(&format!("{:?}", format!("{preview}…")));
            used += room;
            cut = true;
            break;
        }
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(&format!("{text:?}"));
        used += length;
    }
    if cut {
        let hidden = total.saturating_sub(used);
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(&format!("… {hidden} more characters"));
    }
    out
}

/// The `code` of a step's failure `message`, made by this module's
/// functions (with what is on screen after them); `None` for any other
/// failure (the app threw, a selector that does not parse, ...).
#[cfg(feature = "js")]
pub(crate) fn code_of(message: &str) -> Option<&'static str> {
    // Each form starts in a way no other failure does: an app's error
    // reads "the app threw: ...", so its text never decides.
    let quoted = message.starts_with('`');
    if message.starts_with("no ") && message.contains(" matches `")
        || message.starts_with("the menu has no item ")
    {
        Some(NOT_FOUND)
    } else if quoted
        && (message.contains("` is disabled") || message.contains("` does not scroll: "))
        || (message.starts_with("the menu item ") || message.starts_with("the submenu "))
            && message.ends_with(" is disabled")
    {
        Some(DISABLED)
    } else if quoted && message.contains("` is covered: ") {
        Some(COVERED)
    } else if quoted && message.contains(" never happened in ")
        || message.starts_with("the app rendered nothing in ")
    {
        Some(TIMED_OUT)
    } else if message.starts_with(NOTHING_TO_GO_BACK_FROM) {
        Some(NO_BACK)
    } else if message.starts_with("open(") && message.contains("` is unsupported on --target ")
        || quoted
            && message.contains("` is unsupported here: only the browser's history goes forward")
    {
        Some(UNSUPPORTED)
    } else {
        None
    }
}

#[cfg(all(test, feature = "js"))]
mod tests {
    use super::*;

    #[test]
    fn each_failure_says_its_code() {
        let on_screen = "; on screen: [\"Pay\"]";
        let cases = [
            (
                not_found("pressable view", "Pay") + on_screen,
                Some(NOT_FOUND),
            ),
            (not_found("span", "id:x"), Some(NOT_FOUND)),
            (no_menu_item("Paste"), Some(NOT_FOUND)),
            (
                disabled("Pay", "pressable view") + on_screen,
                Some(DISABLED),
            ),
            (disabled("id:list", "scroll view"), Some(DISABLED)),
            (menu_disabled("Paste", false), Some(DISABLED)),
            (covered("Save", "the view `Overlay`"), Some(COVERED)),
            (menu_disabled("Share", true), Some(DISABLED)),
            (
                never_happened("`Saved` showing", Duration::from_millis(5)) + on_screen,
                Some(TIMED_OUT),
            ),
            (rendered_nothing(Duration::from_secs(30)), Some(TIMED_OUT)),
            (
                format!("{NOTHING_TO_GO_BACK_FROM}{on_screen}"),
                Some(NO_BACK),
            ),
            (
                unsupported(
                    "./App.tsx",
                    "width",
                    "ios",
                    "the device's screen has its own size",
                ),
                Some(UNSUPPORTED),
            ),
            (web_only("forward"), Some(UNSUPPORTED)),
            ("open(\"./App.tsx\"): the engine did not start".into(), None),
            ("`Pay` matches 2 pressable views".into(), None),
            (
                "the app threw: no view matches `x` is disabled".into(),
                None,
            ),
        ];
        for (message, code) in cases {
            assert_eq!(code_of(&message), code, "{message}");
        }
    }
}

#[cfg(test)]
mod screen_tests {
    use super::*;

    #[test]
    fn all_texts_when_they_fit() {
        assert_eq!(
            screen_texts(&["Count: 2", "Add"], Some(SCREEN_TEXT_LIMIT)),
            r#""Count: 2", "Add""#
        );
        assert_eq!(screen_texts::<&str>(&[], Some(SCREEN_TEXT_LIMIT)), "");
        assert_eq!(screen_texts(&["a"], None), r#""a""#);
    }

    #[test]
    fn a_long_text_is_cut_with_what_is_left_said() {
        let long = "x".repeat(100);
        let shown = screen_texts(&[long.as_str()], Some(10));
        assert!(shown.starts_with(r#""xxxxxxxxxx…""#), "{shown}");
        assert!(shown.ends_with("… 90 more characters"), "{shown}");
    }

    #[test]
    fn the_rest_of_the_screen_is_counted() {
        let texts = ["a".repeat(8), "b".repeat(100)];
        let shown = screen_texts(&texts, Some(10));
        assert!(shown.starts_with(r#""aaaaaaaa", "bb…""#), "{shown}");
        assert!(shown.ends_with("… 98 more characters"), "{shown}");
    }

    #[test]
    fn cutting_keeps_characters_whole() {
        let text = "é".repeat(20);
        let shown = screen_texts(&[text.as_str()], Some(5));
        assert!(shown.starts_with('"'), "{shown}");
        assert!(shown.ends_with("… 15 more characters"), "{shown}");
    }
}

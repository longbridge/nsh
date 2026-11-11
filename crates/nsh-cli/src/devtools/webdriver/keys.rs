//! WebDriver's special keys: the Private Use characters a client writes into
//! `sendKeys` text and key actions, and the `nsh/keymap` names they press.

use nsh_dev::protocol::KeyAction;

/// What a piece of `sendKeys` text is.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KeyPart {
    /// Text to type.
    Text(String),
    /// A key to press, by its `nsh/keymap` name. `"null"` is WebDriver's
    /// null key: it lets go of the modifiers held.
    Key(&'static str),
}

/// The key a W3C Private Use character names.
pub(crate) fn key_name(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{E000}' => "null",
        '\u{E003}' => "backspace",
        '\u{E004}' => "tab",
        '\u{E006}' | '\u{E007}' => "enter",
        '\u{E00C}' => "escape",
        '\u{E00D}' => "space",
        '\u{E012}' => "left",
        '\u{E013}' => "up",
        '\u{E014}' => "right",
        '\u{E015}' => "down",
        '\u{E017}' => "delete",
        '\u{E008}' => "shift",
        '\u{E009}' => "control",
        '\u{E00A}' => "alt",
        '\u{E03D}' => "meta",
        _ => return None,
    })
}

/// Whether the key is a modifier, held until the null key or the end.
fn is_modifier(name: &str) -> bool {
    matches!(name, "shift" | "control" | "alt" | "meta")
}

/// Cuts `text` into the text between special keys and the keys.
pub(crate) fn split_keys(text: &str) -> Vec<KeyPart> {
    let mut parts = Vec::new();
    let mut run = String::new();
    for c in text.chars() {
        match key_name(c) {
            Some(name) => {
                if !run.is_empty() {
                    parts.push(KeyPart::Text(std::mem::take(&mut run)));
                }
                parts.push(KeyPart::Key(name));
            }
            None => run.push(c),
        }
    }
    if !run.is_empty() {
        parts.push(KeyPart::Text(run));
    }
    parts
}

/// What `sendKeys` does in order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Sending {
    /// Types text into the element.
    Type(String),
    /// Takes key actions.
    Keys(Vec<KeyAction>),
}

fn down(key: &str) -> KeyAction {
    KeyAction::Down {
        key: key.to_owned(),
    }
}

fn up(key: &str) -> KeyAction {
    KeyAction::Up {
        key: key.to_owned(),
    }
}

/// Appends key actions to the last `Keys`, or starts one.
fn keys(plan: &mut Vec<Sending>, actions: impl IntoIterator<Item = KeyAction>) {
    if let Some(Sending::Keys(last)) = plan.last_mut() {
        last.extend(actions);
    } else {
        plan.push(Sending::Keys(actions.into_iter().collect()));
    }
}

/// The plan of a `sendKeys` text: plain text is typed; keys are pressed;
/// modifiers stay held, with the text that follows pressed key by key, until
/// the null key or the end.
pub(crate) fn plan(text: &str) -> Vec<Sending> {
    let mut plan = Vec::new();
    let mut held: Vec<&'static str> = Vec::new();
    for part in split_keys(text) {
        match part {
            KeyPart::Key("null") => {
                let ups: Vec<_> = held.drain(..).rev().map(up).collect();
                if !ups.is_empty() {
                    keys(&mut plan, ups);
                }
            }
            KeyPart::Key(name) if is_modifier(name) => {
                if !held.contains(&name) {
                    held.push(name);
                    keys(&mut plan, [down(name)]);
                }
            }
            KeyPart::Key(name) => keys(&mut plan, [down(name), up(name)]),
            KeyPart::Text(text) if held.is_empty() => plan.push(Sending::Type(text)),
            KeyPart::Text(text) => {
                for c in text.chars() {
                    let key = c.to_string();
                    keys(&mut plan, [down(&key), up(&key)]);
                }
            }
        }
    }
    if !held.is_empty() {
        keys(&mut plan, held.drain(..).rev().map(up));
    }
    plan
}

#[cfg(test)]
mod devtools_webdriver {
    use super::*;

    #[test]
    fn keys_split_into_text_and_named_keys() {
        assert_eq!(
            split_keys("ab\u{E007}c"),
            [
                KeyPart::Text("ab".to_owned()),
                KeyPart::Key("enter"),
                KeyPart::Text("c".to_owned()),
            ]
        );
        assert_eq!(
            split_keys("\u{E004}\u{E003}"),
            [KeyPart::Key("tab"), KeyPart::Key("backspace")]
        );
        assert_eq!(split_keys(""), []);
        // Emoji and CJK are text.
        assert_eq!(
            split_keys("héllo 你好 😀"),
            [KeyPart::Text("héllo 你好 😀".to_owned())]
        );
    }

    #[test]
    fn shift_is_held_until_null() {
        assert_eq!(
            plan("\u{E008}a\u{E000}b"),
            [
                Sending::Keys(vec![down("shift"), down("a"), up("a"), up("shift")]),
                Sending::Type("b".to_owned()),
            ]
        );
    }

    #[test]
    fn a_null_key_with_nothing_held_sends_nothing() {
        assert_eq!(
            plan("a\u{E000}b"),
            [Sending::Type("a".to_owned()), Sending::Type("b".to_owned())]
        );
        assert_eq!(plan("\u{E000}"), []);
    }

    #[test]
    fn modifiers_are_let_go_at_the_end() {
        assert_eq!(
            plan("\u{E009}\u{E008}x"),
            [Sending::Keys(vec![
                down("control"),
                down("shift"),
                down("x"),
                up("x"),
                up("shift"),
                up("control"),
            ])]
        );
    }

    #[test]
    fn text_and_keys_alternate() {
        assert_eq!(
            plan("ab\u{E007}c"),
            [
                Sending::Type("ab".to_owned()),
                Sending::Keys(vec![down("enter"), up("enter")]),
                Sending::Type("c".to_owned()),
            ]
        );
    }
}

//! The editor's default key bindings, as data in `nsh/keymap`'s keystroke
//! syntax (`nsh_core::keymap::Keystroke::parse_with`): `mod` is Command on
//! Apple platforms and Control elsewhere.
//!
//! Cut, copy and paste are not here: they go through the clipboard, which
//! the view owns.

use super::commands::Command;

/// One key binding: the keystroke and the command it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Binding {
    /// The keystroke, as `nsh/keymap` writes it (`"mod+shift+k"`).
    pub(crate) keys: &'static str,
    /// What it runs.
    pub(crate) command: Command,
    /// Whether a held key repeats it (moves and deletions do; toggles and
    /// one-shot commands do not).
    pub(crate) repeat: bool,
}

const fn bind(keys: &'static str, command: Command) -> Binding {
    Binding {
        keys,
        command,
        repeat: false,
    }
}

const fn repeat(keys: &'static str, command: Command) -> Binding {
    Binding {
        keys,
        command,
        repeat: true,
    }
}

/// What both conventions share: arrows, deletion, VS Code's line and
/// selection commands.
const COMMON: &[Binding] = {
    use Command::*;
    &[
        repeat("left", Left { select: false }),
        repeat("shift+left", Left { select: true }),
        repeat("right", Right { select: false }),
        repeat("shift+right", Right { select: true }),
        repeat("up", Up { select: false }),
        repeat("shift+up", Up { select: true }),
        repeat("down", Down { select: false }),
        repeat("shift+down", Down { select: true }),
        repeat("pageup", PageUp { select: false }),
        repeat("shift+pageup", PageUp { select: true }),
        repeat("pagedown", PageDown { select: false }),
        repeat("shift+pagedown", PageDown { select: true }),
        repeat("home", LineStart { select: false }),
        repeat("shift+home", LineStart { select: true }),
        repeat("end", LineEnd { select: false }),
        repeat("shift+end", LineEnd { select: true }),
        repeat("backspace", Backspace),
        repeat("shift+backspace", Backspace),
        repeat("delete", Delete),
        repeat("enter", Newline),
        repeat("shift+enter", Newline),
        repeat("mod+enter", NewlineBelow),
        repeat("mod+shift+enter", NewlineAbove),
        bind("tab", Indent),
        bind("shift+tab", Outdent),
        bind("mod+]", Indent),
        bind("mod+[", Outdent),
        repeat("mod+z", Undo),
        repeat("mod+shift+z", Redo),
        bind("mod+a", SelectAll),
        repeat("mod+l", SelectLine),
        repeat("mod+d", SelectNextOccurrence),
        bind("mod+shift+l", SelectAllOccurrences),
        repeat("alt+up", MoveLinesUp),
        repeat("alt+down", MoveLinesDown),
        repeat("alt+shift+up", DuplicateLinesUp),
        repeat("alt+shift+down", DuplicateLinesDown),
        repeat("mod+shift+k", DeleteLine),
        bind("mod+/", ToggleComment),
        bind("mod+shift+\\", MatchingBracket),
        bind("escape", SingleSelection),
        repeat("ctrl+j", JoinLines),
    ]
};

/// macOS and iOS: Option moves by words, Command to the ends of lines and
/// of the text, and Cocoa's Emacs keys.
const APPLE: &[Binding] = {
    use Command::*;
    &[
        repeat("alt+left", WordLeft { select: false }),
        repeat("alt+shift+left", WordLeft { select: true }),
        repeat("alt+right", WordRight { select: false }),
        repeat("alt+shift+right", WordRight { select: true }),
        repeat("cmd+left", LineStart { select: false }),
        repeat("cmd+shift+left", LineStart { select: true }),
        repeat("cmd+right", LineEnd { select: false }),
        repeat("cmd+shift+right", LineEnd { select: true }),
        bind("cmd+up", DocumentStart { select: false }),
        bind("cmd+shift+up", DocumentStart { select: true }),
        bind("cmd+down", DocumentEnd { select: false }),
        bind("cmd+shift+down", DocumentEnd { select: true }),
        repeat("alt+backspace", DeleteWordLeft),
        repeat("alt+delete", DeleteWordRight),
        repeat("cmd+backspace", DeleteToLineStart),
        repeat("cmd+delete", DeleteToLineEnd),
        repeat("cmd+alt+up", AddCaretAbove),
        repeat("cmd+alt+down", AddCaretBelow),
        repeat("ctrl+a", LineStart { select: false }),
        repeat("ctrl+shift+a", LineStart { select: true }),
        repeat("ctrl+e", LineEnd { select: false }),
        repeat("ctrl+shift+e", LineEnd { select: true }),
        repeat("ctrl+b", Left { select: false }),
        repeat("ctrl+f", Right { select: false }),
        repeat("ctrl+p", Up { select: false }),
        repeat("ctrl+n", Down { select: false }),
        repeat("ctrl+h", Backspace),
        repeat("ctrl+d", Delete),
        repeat("ctrl+k", DeleteToLineEnd),
        repeat("ctrl+t", TransposeChars),
    ]
};

/// Windows, Linux and the web elsewhere: Control moves by words and to
/// the ends of the text.
const OTHERS: &[Binding] = {
    use Command::*;
    &[
        repeat("ctrl+left", WordLeft { select: false }),
        repeat("ctrl+shift+left", WordLeft { select: true }),
        repeat("ctrl+right", WordRight { select: false }),
        repeat("ctrl+shift+right", WordRight { select: true }),
        bind("ctrl+home", DocumentStart { select: false }),
        bind("ctrl+shift+home", DocumentStart { select: true }),
        bind("ctrl+end", DocumentEnd { select: false }),
        bind("ctrl+shift+end", DocumentEnd { select: true }),
        repeat("ctrl+backspace", DeleteWordLeft),
        repeat("ctrl+delete", DeleteWordRight),
        repeat("ctrl+shift+backspace", DeleteToLineStart),
        repeat("ctrl+shift+delete", DeleteToLineEnd),
        repeat("ctrl+alt+up", AddCaretAbove),
        repeat("ctrl+alt+down", AddCaretBelow),
        repeat("ctrl+y", Redo),
    ]
};

/// The default bindings on Apple platforms (`apple`: Option+arrows move by
/// words, Command+arrows to line / text ends, Control+A/E/K as Cocoa's
/// Emacs keys) or elsewhere (Control+arrows by words, Home / End,
/// Control+Home / End).
pub(crate) fn default_bindings(apple: bool) -> Vec<Binding> {
    let own = if apple { APPLE } else { OTHERS };
    COMMON.iter().chain(own).copied().collect()
}

#[cfg(test)]
mod tests {
    use nsh_core::keymap::Keystroke;
    use rustc_hash::FxHashMap;

    use super::*;

    #[test]
    fn every_binding_parses_and_no_keystroke_is_bound_twice() {
        for apple in [true, false] {
            let mut seen: FxHashMap<Keystroke, &str> = FxHashMap::default();
            for binding in default_bindings(apple) {
                let keystroke =
                    Keystroke::parse_with(binding.keys, apple).unwrap_or_else(|e| panic!("{e}"));
                if let Some(other) = seen.insert(keystroke, binding.keys) {
                    panic!("{other:?} and {:?} are the same keystroke", binding.keys);
                }
            }
        }
    }

    #[test]
    fn the_conventions_differ_where_they_should() {
        let find = |apple: bool, keys: &str| {
            default_bindings(apple)
                .into_iter()
                .find(|b| b.keys == keys)
                .map(|b| b.command)
        };
        assert_eq!(
            find(true, "alt+left"),
            Some(Command::WordLeft { select: false })
        );
        assert_eq!(find(false, "alt+left"), None);
        assert_eq!(
            find(false, "ctrl+left"),
            Some(Command::WordLeft { select: false })
        );
        assert_eq!(find(false, "ctrl+y"), Some(Command::Redo));
        assert_eq!(find(true, "ctrl+t"), Some(Command::TransposeChars));
        assert_eq!(find(true, "mod+d"), Some(Command::SelectNextOccurrence));
    }
}

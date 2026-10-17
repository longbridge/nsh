//! App-level keyboard shortcuts (`nsh/keymap`): keystrokes parsed from
//! their text (`"mod+k"`, `"ctrl+shift+p"`, `"escape"`, `"f5"`) and the
//! table of a surface's bindings.
//!
//! The syntax is the one `nsh_core::desktop::Shortcut` (menus, `nsh/desktop`)
//! is moving to; the two will share this parser ([`Keystroke::parse`]).
//!
//! The main thread never waits for JS: JS sends the whole table to the
//! platform ([`crate::Prop::Keymap`] on the root, mounted as
//! [`crate::MountItem::Keymap`]), the platform matches each key against it
//! on the spot ([`Keymap::find`]) and consumes the keys it binds, then
//! tells JS which binding matched ([`crate::EventKind::KeyCommand`]).
//!
//! Keys are named by the character they type on the US layout without
//! Shift (`"/"`, `"["`, `"1"`); Shift is a modifier (`"shift+/"` is `?`).
//! Platforms match a key by the character it types without modifiers, and
//! on a layout that types no ASCII there (Cyrillic, Greek, ...) by the US
//! layout's character of the same key, as menus do, so shortcuts work on
//! any layout.

use std::fmt;

use crate::platform::Os;

/// The modifiers of a keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    /// Control.
    pub ctrl: bool,
    /// Option on Apple platforms, Alt elsewhere.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Command on Apple platforms; the Super (Windows) key elsewhere.
    pub cmd: bool,
}

/// A key, by what it is on the US layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A key typing a printable ASCII character without Shift: a lower
    /// case letter, a digit, punctuation, or `' '` (space).
    Char(char),
    /// Return / Enter (the keypad's too).
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// Backspace (the Mac's Delete).
    Backspace,
    /// Forward delete.
    Delete,
    /// Insert (Help on Apple keyboards).
    Insert,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// A function key, F1 to F24.
    F(u8),
}

/// One key with its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keystroke {
    /// Modifiers held.
    pub modifiers: Modifiers,
    /// The key.
    pub key: Key,
}

/// Why a keystroke's text is not one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeystrokeError(pub String);

impl fmt::Display for KeystrokeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KeystrokeError {}

/// Characters typed with Shift on the US layout, and the key typing them.
const SHIFTED: &[(char, char)] = &[
    ('~', '`'),
    ('!', '1'),
    ('@', '2'),
    ('#', '3'),
    ('$', '4'),
    ('%', '5'),
    ('^', '6'),
    ('&', '7'),
    ('*', '8'),
    ('(', '9'),
    (')', '0'),
    ('_', '-'),
    ('+', '='),
    ('{', '['),
    ('}', ']'),
    ('|', '\\'),
    (':', ';'),
    ('"', '\''),
    ('<', ','),
    ('>', '.'),
    ('?', '/'),
];

/// Named keys, by their names (lower case).
const NAMED: &[(&str, Key)] = &[
    ("enter", Key::Enter),
    ("return", Key::Enter),
    ("escape", Key::Escape),
    ("esc", Key::Escape),
    ("tab", Key::Tab),
    ("space", Key::Char(' ')),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("del", Key::Delete),
    ("insert", Key::Insert),
    ("up", Key::Up),
    ("arrowup", Key::Up),
    ("down", Key::Down),
    ("arrowdown", Key::Down),
    ("left", Key::Left),
    ("arrowleft", Key::Left),
    ("right", Key::Right),
    ("arrowright", Key::Right),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    // The plus key is the one typing `=`, as on the US layout.
    ("plus", Key::Char('=')),
    ("minus", Key::Char('-')),
];

fn error(text: &str, reason: impl fmt::Display) -> KeystrokeError {
    KeystrokeError(format!("invalid keystroke {text:?}: {reason}"))
}

impl Keystroke {
    /// Parses `text`: modifiers and a key joined by `+` (`"mod+shift+k"`,
    /// `"mod+-"` for Command and minus), in any case (lowercase is the
    /// style; a capital letter is the same key, not Shift). Modifiers:
    /// `ctrl`, `alt` (`option`), `shift`, `cmd` (`super`, `win`) and `mod`,
    /// which is Command on Apple platforms (`os`) and Control elsewhere.
    /// Keys: a character of the US layout typed without Shift (`k`, `1`,
    /// `/`, `[`, `=`: the plus key is `=`, `shift+=` with Shift), `space`,
    /// `enter`, `escape`, `tab`, `backspace`, `delete`, the arrows `up` /
    /// `down` / `left` / `right`, `home`, `end`, `pageup`, `pagedown`, `f1`
    /// to `f24`.
    pub fn parse(text: &str, os: Os) -> Result<Keystroke, KeystrokeError> {
        // Case does not matter (lowercase is the style); messages quote
        // the text as written and suggest lowercase.
        let lower = text.to_ascii_lowercase();
        let (mods, key) = split(text, &lower)?;
        let mut modifiers = Modifiers::default();
        for name in mods {
            let flag = modifier(&mut modifiers, name, os).map_err(|why| error(text, why))?;
            if *flag {
                return Err(error(text, format_args!("{name:?} twice")));
            }
            *flag = true;
        }
        Ok(Keystroke {
            modifiers,
            key: parse_key(text, key)?,
        })
    }

    /// Why the platform keeps this keystroke for itself on `os` (quitting,
    /// hiding, minimizing the app or making it full screen), if it does:
    /// such keystrokes cannot be bound.
    pub fn reserved(&self, os: Os) -> Option<&'static str> {
        let m = self.modifiers;
        let only = |ctrl, alt, cmd| m.ctrl == ctrl && m.alt == alt && m.cmd == cmd && !m.shift;
        match os {
            Os::MacOs | Os::Ios => match self.key {
                Key::Char('q') if only(false, false, true) => Some("it quits the app"),
                Key::Char('h') if only(false, false, true) => Some("it hides the app"),
                Key::Char('h') if only(false, true, true) => Some("it hides the other apps"),
                Key::Char('m') if only(false, false, true) => Some("it minimizes the window"),
                Key::Char('f') if only(true, false, true) => Some("it toggles full screen"),
                _ => None,
            },
            Os::Linux | Os::Windows | Os::Android => match self.key {
                Key::Char('q') if only(true, false, false) => Some("it quits the app"),
                Key::F(4) if only(false, true, false) => Some("it closes the window"),
                Key::F(11) if m == Modifiers::default() => Some("it toggles full screen"),
                _ => None,
            },
            // The browser keeps its own keys: the page never sees them.
            Os::Web => None,
        }
    }

    /// Whether this is the development hosts' reload key on `os` (`mod+r`:
    /// Cmd+R, Ctrl+R): built with their `dev` feature, platforms leave it
    /// to the dev server client, whatever the app binds.
    pub fn reloads(&self, os: Os) -> bool {
        let m = self.modifiers;
        let apple = matches!(os, Os::MacOs | Os::Ios);
        self.key == Key::Char('r') && !m.shift && !m.alt && m.cmd == apple && m.ctrl != apple
    }

    /// Whether a text input that has the keyboard focus uses this
    /// keystroke itself: keys that type, move the caret or delete, and the
    /// editing shortcuts (select all, cut, copy, paste, undo, redo).
    /// Bindings of these keystrokes do not fire while text is edited,
    /// unless they ask to ([`When::Always`]).
    pub fn edits_text(&self) -> bool {
        let m = self.modifiers;
        match self.key {
            Key::F(_) => false,
            Key::Char(c) => !(m.cmd || m.ctrl) || matches!(c, 'a' | 'c' | 'v' | 'x' | 'z'),
            _ => true,
        }
    }
}

/// The modifiers and the key of `text` (`"mod+shift+k"`).
fn split<'a>(text: &str, lower: &'a str) -> Result<(Vec<&'a str>, &'a str), KeystrokeError> {
    if text.is_empty() {
        return Err(error(text, "empty"));
    }
    if text.contains(char::is_whitespace) {
        return Err(error(
            text,
            "one keystroke per binding (key sequences such as \"mod+k mod+s\" are not supported)",
        ));
    }
    if lower == "+" || lower.ends_with("++") {
        let mods = lower.strip_suffix("++").unwrap_or("");
        let plus = if mods.is_empty() {
            "plus".to_owned()
        } else {
            format!("{mods}+plus")
        };
        return Err(error(
            text,
            format_args!("write the plus key as \"plus\": {plus:?}"),
        ));
    }
    let (mods, key) = match lower.rsplit_once('+') {
        Some((mods, key)) => (mods.split('+').collect(), key),
        None => (Vec::new(), lower),
    };
    if key.is_empty() {
        return Err(error(text, "no key after the modifiers"));
    }
    Ok((mods, key))
}

/// The flag of modifier `name` in `modifiers`, or why it is none.
fn modifier<'a>(modifiers: &'a mut Modifiers, name: &str, os: Os) -> Result<&'a mut bool, String> {
    let apple = matches!(os, Os::MacOs | Os::Ios);
    Ok(match name {
        "ctrl" | "control" => &mut modifiers.ctrl,
        "alt" | "option" | "opt" => &mut modifiers.alt,
        "shift" => &mut modifiers.shift,
        "cmd" | "command" | "meta" | "super" | "win" => &mut modifiers.cmd,
        "mod" | "cmdorctrl" | "commandorcontrol" if apple => &mut modifiers.cmd,
        "mod" | "cmdorctrl" | "commandorcontrol" => &mut modifiers.ctrl,
        "" => return Err("an empty modifier".into()),
        other => {
            return Err(format!(
                "unknown modifier {other:?} (ctrl, alt, shift, cmd or mod)"
            ));
        }
    })
}

fn parse_key(text: &str, key: &str) -> Result<Key, KeystrokeError> {
    if let Some(&(_, named)) = NAMED.iter().find(|(name, _)| *name == key) {
        return Ok(named);
    }
    if let Some(n) = key
        .strip_prefix('f')
        .filter(|n| !n.starts_with('0'))
        .and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        return Ok(Key::F(n));
    }
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        let hint = if let Some((first, _)) = key.split_once('-')
            && modifier(&mut Modifiers::default(), first, Os::Linux).is_ok()
        {
            // `mod--` is `mod+-`.
            let joined = match key.strip_suffix("--") {
                Some(mods) => format!("{}+-", mods.replace('-', "+")),
                None => key.replace('-', "+"),
            };
            format!(" (join modifiers and the key with \"+\": {joined:?})")
        } else {
            String::new()
        };
        return Err(error(text, format_args!("unknown key {key:?}{hint}")));
    };
    if let Some(&(_, base)) = SHIFTED.iter().find(|(shifted, _)| *shifted == c) {
        return Err(error(
            text,
            format_args!("{c:?} is typed with Shift: write \"shift+{base}\""),
        ));
    }
    if !c.is_ascii_graphic() {
        return Err(error(
            text,
            "keys are named by the US layout's characters (they match on any layout)",
        ));
    }
    Ok(Key::Char(c))
}

impl fmt::Display for Keystroke {
    /// The canonical text: `ctrl+alt+shift+cmd+<key>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.modifiers;
        for (on, name) in [
            (m.ctrl, "ctrl+"),
            (m.alt, "alt+"),
            (m.shift, "shift+"),
            (m.cmd, "cmd+"),
        ] {
            if on {
                f.write_str(name)?;
            }
        }
        match self.key {
            Key::Char(' ') => f.write_str("space"),
            Key::Char(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "f{n}"),
            key => {
                let name = NAMED.iter().find(|(_, k)| *k == key).map(|(n, _)| *n);
                f.write_str(name.unwrap_or("?"))
            }
        }
    }
}

/// When a binding fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum When {
    /// Unless a text input (or a hosted native view, such as a web view)
    /// has the keyboard focus and the keystroke is one it uses
    /// ([`Keystroke::edits_text`]): the default.
    #[default]
    Auto,
    /// Wherever the keyboard focus is, in text inputs too.
    Always,
    /// Only while no text input or hosted native view has the keyboard
    /// focus.
    NotEditing,
}

/// One binding of a [`Keymap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    /// What the platform reports when it fires
    /// ([`crate::EventKind::KeyCommand`]'s payload).
    pub id: u32,
    /// The keystroke.
    pub keystroke: Keystroke,
    /// It fires while a text input or hosted native view has the keyboard
    /// focus ([`When`], resolved).
    pub in_text: bool,
    /// It fires again on the auto-repeats of a held key; else once until
    /// the key is released.
    pub repeat: bool,
}

impl KeyBinding {
    /// A binding of `keystroke` firing as `when` says, on auto-repeats
    /// too when `repeat`.
    pub fn new(id: u32, keystroke: Keystroke, when: When, repeat: bool) -> KeyBinding {
        let in_text = match when {
            When::Auto => !keystroke.edits_text(),
            When::Always => true,
            When::NotEditing => false,
        };
        KeyBinding {
            id,
            keystroke,
            in_text,
            repeat,
        }
    }
}

/// What a key does with a [`Keymap`] ([`Keymap::find`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMatch {
    /// No binding: the key goes on as if there were no keymap.
    None,
    /// It fires binding `id`: the platform consumes the key and reports it
    /// ([`crate::EventKind::KeyCommand`]).
    Fire(u32),
    /// An auto-repeat of a held key whose binding fires once: the platform
    /// consumes it and reports nothing.
    Held,
}

impl KeyMatch {
    /// The platform consumes the key.
    pub fn consumed(self) -> bool {
        self != KeyMatch::None
    }
}

/// The bindings of a surface, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keymap {
    bindings: Vec<KeyBinding>,
}

impl Keymap {
    /// A table of `bindings`, oldest first.
    pub fn new(bindings: Vec<KeyBinding>) -> Keymap {
        Keymap { bindings }
    }

    /// No bindings: the platform need not look at keys.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// The bindings, oldest first.
    pub fn bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    /// What a key does: the newest binding of `keystroke` that fires while
    /// `editing` (a text input or hosted native view has the keyboard
    /// focus) takes it; `repeat` is an auto-repeat of a held key, which only
    /// fires bindings that ask for it.
    pub fn find(&self, keystroke: Keystroke, editing: bool, repeat: bool) -> KeyMatch {
        let found = self
            .bindings
            .iter()
            .rev()
            .find(|b| b.keystroke == keystroke && (b.in_text || !editing));
        match found {
            None => KeyMatch::None,
            Some(b) if repeat && !b.repeat => KeyMatch::Held,
            Some(b) => KeyMatch::Fire(b.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Keystroke, KeystrokeError> {
        Keystroke::parse(text, Os::MacOs)
    }

    fn stroke(ctrl: bool, alt: bool, shift: bool, cmd: bool, key: Key) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                cmd,
            },
            key,
        }
    }

    #[test]
    fn parses_keystrokes() {
        let c = |c| Key::Char(c);
        assert_eq!(parse("k"), Ok(stroke(false, false, false, false, c('k'))));
        assert_eq!(
            parse("cmd+shift+k"),
            Ok(stroke(false, false, true, true, c('k')))
        );
        assert_eq!(
            parse("ctrl+alt+enter"),
            Ok(stroke(true, true, false, false, Key::Enter))
        );
        assert_eq!(
            parse("cmd+-"),
            Ok(stroke(false, false, false, true, c('-')))
        );
        assert_eq!(parse("-"), Ok(stroke(false, false, false, false, c('-'))));
        assert_eq!(
            parse("cmd+="),
            Ok(stroke(false, false, false, true, c('=')))
        );
        assert_eq!(
            parse("space"),
            Ok(stroke(false, false, false, false, c(' ')))
        );
        assert_eq!(
            parse("f5"),
            Ok(stroke(false, false, false, false, Key::F(5)))
        );
        assert_eq!(
            parse("shift+/"),
            Ok(stroke(false, false, true, false, c('/')))
        );
        assert_eq!(
            parse("option+escape"),
            Ok(stroke(false, true, false, false, Key::Escape))
        );
        // `mod` is Command on Apple platforms, Control elsewhere.
        assert_eq!(parse("mod+k"), parse("cmd+k"));
        // Case does not matter, and a capital letter is not Shift.
        assert_eq!(parse("Mod+Shift+N"), parse("mod+shift+n"));
        assert_eq!(parse("CMD+K"), parse("cmd+k"));
        assert_eq!(parse("mod+K"), parse("mod+k"));
        assert_eq!(parse("Escape"), parse("escape"));
        assert_eq!(parse("F5"), parse("f5"));
        assert_eq!(parse("Ctrl+Plus"), parse("ctrl+="));
        assert_eq!(parse("cmdorctrl+k"), parse("meta+k"));
        assert_eq!(parse("mod+plus"), parse("mod+="));
        assert_eq!(parse("ctrl+minus"), parse("ctrl+-"));
        assert_eq!(parse("arrowup"), parse("up"));
        assert_eq!(parse("insert").unwrap().key, Key::Insert);
        assert_eq!(
            Keystroke::parse("mod+k", Os::Linux),
            Keystroke::parse("ctrl+k", Os::Linux)
        );
        assert_eq!(parse("mod+k").unwrap().to_string(), "cmd+k");
        assert_eq!(
            parse("shift+cmd+alt+ctrl+pagedown").unwrap().to_string(),
            "ctrl+alt+shift+cmd+pagedown"
        );
    }

    #[test]
    fn explains_invalid_keystrokes() {
        let err = |t| parse(t).unwrap_err().0;
        assert_eq!(err(""), "invalid keystroke \"\": empty");
        assert!(err("mod+k mod+s").contains("one keystroke per binding"));
        assert!(err("cmd+").contains("no key after the modifiers"));
        assert!(err("hyper+k").contains("unknown modifier \"hyper\""));
        assert!(err("cmd+cmd+k").contains("\"cmd\" twice"));
        assert!(err("cmd+kk").contains("unknown key \"kk\""));
        assert!(err("mod-k").contains("join modifiers and the key with \"+\": \"mod+k\""));
        assert!(err("ctrl-shift-p").contains("\"ctrl+shift+p\""));
        assert!(err("mod--").contains("\"mod+-\""));
        // Hints suggest lowercase, whatever case the text was written in.
        assert!(err("Mod-K").contains("join modifiers and the key with \"+\": \"mod+k\""));
        assert!(err("CTRL++").contains("\"ctrl+plus\""));
        assert!(err("cmd+?").contains("write \"shift+/\""));
        assert!(err("mod++").contains("write the plus key as \"plus\": \"mod+plus\""));
        assert!(err("+").contains("\"plus\""));
        assert!(err("cmd+é").contains("US layout"));
        assert!(err("f25").contains("unknown key"));
        assert!(err("f05").contains("unknown key"));
        assert!(err("cmd++k").contains("an empty modifier"));
    }

    #[test]
    fn reserves_what_the_platform_keeps() {
        let reserved = |t, os| Keystroke::parse(t, os).unwrap().reserved(os).is_some();
        assert!(reserved("mod+q", Os::MacOs));
        assert!(reserved("cmd+alt+h", Os::MacOs));
        assert!(reserved("ctrl+cmd+f", Os::MacOs));
        assert!(!reserved("cmd+shift+q", Os::MacOs));
        assert!(!reserved("ctrl+q", Os::MacOs));
        assert!(!reserved("cmd+w", Os::MacOs));
        assert!(reserved("mod+q", Os::Linux));
        assert!(reserved("f11", Os::Linux));
        assert!(reserved("alt+f4", Os::Windows));
        assert!(!reserved("cmd+q", Os::Linux));
    }

    #[test]
    fn the_dev_reload_key() {
        let reloads = |t, os| Keystroke::parse(t, os).unwrap().reloads(os);
        assert!(reloads("mod+r", Os::MacOs));
        assert!(reloads("ctrl+r", Os::Windows));
        assert!(!reloads("ctrl+r", Os::MacOs));
        assert!(!reloads("mod+shift+r", Os::Linux));
    }

    #[test]
    fn text_inputs_keep_their_keys() {
        let edits = |t| parse(t).unwrap().edits_text();
        assert!(edits("k"));
        assert!(edits("shift+k"));
        assert!(edits("alt+k"));
        assert!(edits("escape"));
        assert!(edits("cmd+left"));
        assert!(edits("cmd+c"));
        assert!(edits("cmd+shift+z"));
        assert!(!edits("cmd+k"));
        assert!(!edits("ctrl+shift+p"));
        assert!(!edits("f5"));
    }

    #[test]
    fn the_newest_binding_that_fires_wins() {
        let k = parse("cmd+k").unwrap();
        let e = parse("escape").unwrap();
        let map = Keymap::new(vec![
            KeyBinding::new(1, k, When::Auto, false),
            KeyBinding::new(2, e, When::Auto, true),
            KeyBinding::new(3, k, When::NotEditing, false),
        ]);
        assert_eq!(map.find(k, false, false), KeyMatch::Fire(3));
        assert_eq!(map.find(k, true, false), KeyMatch::Fire(1));
        assert_eq!(map.find(e, false, false), KeyMatch::Fire(2));
        assert_eq!(map.find(e, true, false), KeyMatch::None);
        assert_eq!(
            map.find(parse("cmd+j").unwrap(), false, false),
            KeyMatch::None
        );
        // A held key fires once, unless its binding repeats.
        assert_eq!(map.find(k, false, true), KeyMatch::Held);
        assert_eq!(map.find(e, false, true), KeyMatch::Fire(2));
        let always = Keymap::new(vec![KeyBinding::new(4, e, When::Always, false)]);
        assert_eq!(always.find(e, true, false), KeyMatch::Fire(4));
    }
}

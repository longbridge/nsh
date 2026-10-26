//! Key bindings, a port of `prosemirror-keymap`.
//!
//! A binding's name is a key (as [`KeyEvent::key`] names it: `"Enter"`,
//! `"a"`, `"ArrowUp"`, `"Space"` for the space bar) after modifiers
//! separated by `-`: `Shift-`, `Alt-` (`a-`), `Ctrl-` (`c-`, `Control-`),
//! `Meta-` (`Cmd-`, `m-`) and `Mod-`, which is `Meta-` on Apple platforms
//! and `Ctrl-` elsewhere. Character keys are matched with their case as
//! typed (`"Shift-a"` and `"A"` both name Shift with A held), as in
//! ProseMirror.

use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::rich_editor::{
    commands::BoxCommand,
    state::{EditorState, KeyEvent, Plugin, PluginKey, PluginSpec, Transaction},
};

/// Why a binding's name could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeymapError {
    /// A modifier no platform has (the binding's name and the modifier).
    UnknownModifier(String, String),
}

impl fmt::Display for KeymapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownModifier(name, modifier) => {
                write!(
                    f,
                    "unrecognized modifier `{modifier}` in key binding `{name}`"
                )
            }
        }
    }
}

impl std::error::Error for KeymapError {}

/// How names are read.
#[derive(Debug, Clone)]
#[non_exhaustive]
#[derive(Default)]
pub struct KeymapOptions {
    /// Whether `Mod-` is `Meta-` (Apple platforms) rather than `Ctrl-`;
    /// by default, whether this is macOS or iOS.
    pub mac: bool,
}

/// `name` in the normal form bindings are looked up by: modifiers in the
/// order `Shift-Meta-Ctrl-Alt-`, `Mod-` resolved, `Space` as `" "`.
pub fn normalize_key_name(name: &str, mac: bool) -> Result<String, KeymapError> {
    // A trailing `-` is the minus key, not a separator.
    let (mods, key) = match name
        .strip_suffix('-')
        .filter(|rest| rest.is_empty() || rest.ends_with('-'))
    {
        Some(rest) => (rest.strip_suffix('-').unwrap_or(rest), "-"),
        None => match name.rfind('-') {
            Some(i) => (&name[..i], &name[i + 1..]),
            None => ("", name),
        },
    };
    let mut key = if key == "Space" {
        " ".to_owned()
    } else {
        key.to_owned()
    };
    let (mut alt, mut ctrl, mut shift, mut meta) = (false, false, false, false);
    for modifier in mods.split('-').filter(|m| !m.is_empty()) {
        match modifier.to_ascii_lowercase().as_str() {
            "cmd" | "meta" | "m" => meta = true,
            "a" | "alt" => alt = true,
            "c" | "ctrl" | "control" => ctrl = true,
            "s" | "shift" => shift = true,
            "mod" => {
                if mac {
                    meta = true;
                } else {
                    ctrl = true;
                }
            }
            _ => {
                return Err(KeymapError::UnknownModifier(
                    name.to_owned(),
                    modifier.to_owned(),
                ));
            }
        }
    }
    if alt {
        key = format!("Alt-{key}");
    }
    if ctrl {
        key = format!("Ctrl-{key}");
    }
    if meta {
        key = format!("Meta-{key}");
    }
    if shift {
        key = format!("Shift-{key}");
    }
    Ok(key)
}

// The event's key with its modifiers, Shift only when `shift`.
fn modifiers(name: &str, event: &KeyEvent, shift: bool) -> String {
    let mut name = name.to_owned();
    if event.alt {
        name = format!("Alt-{name}");
    }
    if event.ctrl {
        name = format!("Ctrl-{name}");
    }
    if event.meta {
        name = format!("Meta-{name}");
    }
    if shift && event.shift {
        name = format!("Shift-{name}");
    }
    name
}

/// Bindings by normalized name, and the lookup of a key event in them.
#[derive(Clone, Default)]
pub struct Keymap {
    bindings: HashMap<String, BoxCommand>,
}

impl fmt::Debug for Keymap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keymap")
            .field("bindings", &self.bindings.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Keymap {
    /// The bindings, their names normalized with `options`. A later
    /// binding of a name replaces an earlier one.
    pub fn new(
        bindings: Vec<(String, BoxCommand)>,
        options: &KeymapOptions,
    ) -> Result<Self, KeymapError> {
        let mut map = HashMap::with_capacity(bindings.len());
        for (name, command) in bindings {
            map.insert(normalize_key_name(&name, options.mac)?, command);
        }
        Ok(Self { bindings: map })
    }

    /// The command bound to a normalized name.
    pub fn get(&self, name: &str) -> Option<&BoxCommand> {
        self.bindings.get(name)
    }

    /// Runs the command bound to `event`, as `prosemirror-keymap`'s
    /// `keydownHandler`: whether one applied.
    pub fn handle(
        &self,
        state: &EditorState,
        event: &KeyEvent,
        dispatch: &mut dyn FnMut(Transaction),
    ) -> bool {
        let name = if event.key == "Space" {
            " "
        } else {
            event.key.as_str()
        };
        let is_char = name.chars().count() == 1 && name != " ";
        // A character's Shift is in its case already.
        if let Some(command) = self.bindings.get(&modifiers(name, event, !is_char))
            && command(state, Some(&mut *dispatch))
        {
            return true;
        }
        if is_char && event.shift {
            // Also the binding with Shift spelled out, and of the base key
            // (`Shift-Mod-z` for a Z typed with Shift).
            if let Some(command) = self.bindings.get(&modifiers(name, event, true))
                && command(state, Some(&mut *dispatch))
            {
                return true;
            }
            let lower = name.to_lowercase();
            if lower != name
                && let Some(command) = self.bindings.get(&modifiers(&lower, event, true))
                && command(state, Some(&mut *dispatch))
            {
                return true;
            }
        }
        false
    }
}

// Makes each keymap plugin's key unique: an editor holds several.
static NEXT_KEYMAP: AtomicU64 = AtomicU64::new(0);

struct KeymapPlugin {
    key: PluginKey,
    keymap: Keymap,
}

impl PluginSpec for KeymapPlugin {
    fn key(&self) -> PluginKey {
        self.key.clone()
    }

    fn handle_key_down(
        &self,
        state: &EditorState,
        event: &KeyEvent,
        dispatch: &mut dyn FnMut(Transaction),
    ) -> bool {
        self.keymap.handle(state, event, dispatch)
    }
}

/// A plugin running the commands bound to keys (with the platform's
/// `Mod-`). Bindings with names that cannot be read are skipped, with a
/// warning; [`keymap_with`] reports them.
pub fn keymap(bindings: Vec<(String, BoxCommand)>) -> Plugin {
    let options = KeymapOptions::default();
    let mut valid = Vec::with_capacity(bindings.len());
    for (name, command) in bindings {
        match normalize_key_name(&name, options.mac) {
            Ok(_) => valid.push((name, command)),
            Err(error) => log::warn!("{error}"),
        }
    }
    keymap_plugin(Keymap::new(valid, &options).unwrap_or_default())
}

/// A plugin running the commands bound to keys, reading names with
/// `options`.
pub fn keymap_with(
    bindings: Vec<(String, BoxCommand)>,
    options: &KeymapOptions,
) -> Result<Plugin, KeymapError> {
    Ok(keymap_plugin(Keymap::new(bindings, options)?))
}

/// A plugin running `keymap`'s commands.
pub fn keymap_plugin(keymap: Keymap) -> Plugin {
    let id = NEXT_KEYMAP.fetch_add(1, Ordering::Relaxed);
    Arc::new(KeymapPlugin {
        key: PluginKey::new(&format!("keymap${id}")),
        keymap,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::rich_editor::{commands::command, extensions::starter, state::EditorStateConfig};

    #[test]
    fn names_normalize() {
        assert_eq!(normalize_key_name("Mod-b", false).unwrap(), "Ctrl-b");
        assert_eq!(normalize_key_name("Mod-b", true).unwrap(), "Meta-b");
        assert_eq!(
            normalize_key_name("Shift-Enter", false).unwrap(),
            "Shift-Enter"
        );
        assert_eq!(
            normalize_key_name("Ctrl-Alt-x", false).unwrap(),
            "Ctrl-Alt-x"
        );
        assert_eq!(
            normalize_key_name("alt-shift-cmd-ctrl-k", false).unwrap(),
            "Shift-Meta-Ctrl-Alt-k"
        );
        assert_eq!(
            normalize_key_name("Shift-Mod-z", true).unwrap(),
            "Shift-Meta-z"
        );
        assert_eq!(normalize_key_name("Space", false).unwrap(), " ");
        assert_eq!(normalize_key_name("Mod--", false).unwrap(), "Ctrl--");
        assert_eq!(normalize_key_name("-", false).unwrap(), "-");
        assert!(normalize_key_name("Hyper-x", false).is_err());
    }

    #[test]
    fn events_find_their_bindings() {
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = |hits: &Arc<AtomicUsize>, n: usize| {
            let hits = hits.clone();
            command(move |_, _| {
                hits.fetch_add(n, Ordering::Relaxed);
                true
            })
        };
        let keymap = Keymap::new(
            vec![
                ("Mod-b".into(), counter(&hits, 1)),
                ("Shift-Mod-z".into(), counter(&hits, 10)),
                ("Shift-Enter".into(), counter(&hits, 100)),
            ],
            &KeymapOptions { mac: false },
        )
        .unwrap();
        let state = EditorState::create(EditorStateConfig::new(starter::schema())).unwrap();
        let mut dispatch = |_| {};
        let event = |key: &str, ctrl: bool, shift: bool| KeyEvent {
            ctrl,
            shift,
            ..KeyEvent::new(key)
        };
        assert!(keymap.handle(&state, &event("b", true, false), &mut dispatch));
        assert!(keymap.handle(&state, &event("Z", true, true), &mut dispatch));
        assert!(keymap.handle(&state, &event("Enter", false, true), &mut dispatch));
        assert!(!keymap.handle(&state, &event("Enter", false, false), &mut dispatch));
        assert!(!keymap.handle(&state, &event("b", false, false), &mut dispatch));
        assert_eq!(hits.load(Ordering::Relaxed), 111);
    }
}

//! The app's keyboard shortcuts (`nsh/keymap`): the table the engine
//! mounts ([`MountItem::Keymap`](nsh_core::MountItem::Keymap)), matched on
//! every key down the window gets ([`crate::input::key`]): after the
//! window's shortcuts and the dialogs the surface shows, before the
//! application menu's shortcuts ([`crate::chrome::shortcut`]: a binding
//! wins over a menu item of the same keystroke), the focused field,
//! selectable text, navigation and scroll views. A key that fires a
//! binding is consumed and reported to JS with the root's `KeyCommand`
//! event; JS runs the handler afterwards.
//!
//! Modifiers are GPUI's: `cmd` is the platform key (Command on macOS, the
//! Super / Windows key elsewhere), so `mod` (Command on macOS, Control
//! elsewhere, resolved by the engine for [`OS`]) matches the key users
//! expect. Keys are matched by virtual key code ([`crate::keys`]): letters
//! and digits by the key GPUI names, punctuation by the US layout's keys.
//!
//! Auto-repeats: GPUI marks them on macOS, Windows and Wayland
//! (`KeyDownEvent::is_held`), not on X11, where they come as more key
//! downs of the same key with no release between (GPUI drops the
//! releases X11 makes up). So a key down of the key that went down last
//! and has not been released counts as a repeat too: a binding without
//! `repeat` fires once until the key is released, the modifiers change or
//! the window is deactivated ([`forget_held`]).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use nsh_core::{
    EventKind, EventPayload, PayloadValue, ROOT_ID,
    keymap::{Key, KeyMatch, Keymap, Keystroke, Modifiers},
    platform::Os,
};

use crate::{input::KeyEvent, keys, surface::Surface};

/// The platform bindings are read for.
const OS: Os = if cfg!(target_os = "macos") {
    Os::MacOs
} else if cfg!(windows) {
    Os::Windows
} else {
    Os::Linux
};

/// A surface's table, while it has bindings.
#[derive(Default)]
pub(crate) struct KeymapState {
    table: RefCell<Option<Arc<Keymap>>>,
    /// The key that went down last and is still down (not a modifier).
    held: Cell<Option<u16>>,
}

/// `MountItem::Keymap`: replaces the surface's table.
pub(crate) fn set(surface: &Surface, keymap: Arc<Keymap>) {
    *surface.keymap.table.borrow_mut() = (!keymap.is_empty()).then_some(keymap);
}

/// Drops the table (the app stopped).
pub(crate) fn clear(surface: &Surface) {
    surface.keymap.table.borrow_mut().take();
}

/// A key down: whether it fired (or, held, repeats) one of the bindings;
/// then it is consumed.
pub(crate) fn key(surface: &Rc<Surface>, event: &KeyEvent) -> bool {
    let repeat = repeats(&surface.keymap, event);
    let Some(table) = surface.keymap.table.borrow().clone() else {
        return false;
    };
    let Some(keystroke) = keystroke(event) else {
        return false;
    };
    // The dev host's Ctrl+R / Cmd+R reloads, whatever the app binds.
    if cfg!(feature = "dev") && keystroke.reloads(OS) {
        return false;
    }
    let editing = crate::text_input::focused(surface).is_some();
    let found = table.find(keystroke, editing, repeat);
    if let KeyMatch::Fire(id) = found {
        surface.event(
            ROOT_ID,
            EventKind::KeyCommand,
            EventPayload::Value(PayloadValue::Number(id as f64)),
        );
    }
    found.consumed()
}

/// Whether a key down is an auto-repeat (see the module docs), noting the
/// key held.
fn repeats(state: &KeymapState, event: &KeyEvent) -> bool {
    if modifier(event.vk) {
        return event.repeat;
    }
    state.held.replace(Some(event.vk)) == Some(event.vk) || event.repeat
}

/// A modifier key (which does not end another key's auto-repeat).
fn modifier(vk: u16) -> bool {
    matches!(
        vk,
        keys::VK_SHIFT | keys::VK_CONTROL | keys::VK_MENU | keys::VK_LWIN
    )
}

/// A key up: the key held is released.
pub(crate) fn released(surface: &Surface, event: &KeyEvent) {
    let held = &surface.keymap.held;
    if held.get() == Some(event.vk) {
        held.set(None);
    }
}

/// The window was deactivated (the key held may be released elsewhere),
/// or the modifiers changed: GPUI's X11 backend loses a key's release
/// when another release follows in the same batch of events (releasing
/// K and then Ctrl of Ctrl+K), so the next Ctrl+K must not count as a
/// repeat.
pub(crate) fn forget_held(surface: &Surface) {
    surface.keymap.held.set(None);
}

/// The keystroke of a key down.
fn keystroke(event: &KeyEvent) -> Option<Keystroke> {
    let m = event.modifiers;
    let modifiers = Modifiers {
        ctrl: m.ctrl,
        alt: m.alt,
        shift: m.shift,
        cmd: m.meta,
    };
    Some(Keystroke {
        modifiers,
        key: key_of(event.vk)?,
    })
}

/// The key of virtual key code `vk` (`VK_*`).
fn key_of(vk: u16) -> Option<Key> {
    Some(match vk {
        0x08 => Key::Backspace,
        0x09 => Key::Tab,
        0x0D => Key::Enter,
        0x1B => Key::Escape,
        0x20 => Key::Char(' '),
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x23 => Key::End,
        0x24 => Key::Home,
        0x25 => Key::Left,
        0x26 => Key::Up,
        0x27 => Key::Right,
        0x28 => Key::Down,
        0x2D => Key::Insert,
        0x2E => Key::Delete,
        // `0`-`9`, `A`-`Z`.
        0x30..=0x39 => Key::Char(char::from(vk as u8)),
        0x41..=0x5A => Key::Char(char::from(vk as u8).to_ascii_lowercase()),
        // The keypad's digits.
        0x60..=0x69 => Key::Char(char::from(b'0' + (vk - 0x60) as u8)),
        // F1 to F24.
        0x70..=0x87 => Key::F((vk - 0x6F) as u8),
        // `VK_OEM_*`: the US layout's punctuation keys.
        0xBA => Key::Char(';'),
        0xBB => Key::Char('='),
        0xBC => Key::Char(','),
        0xBD => Key::Char('-'),
        0xBE => Key::Char('.'),
        0xBF => Key::Char('/'),
        0xC0 => Key::Char('`'),
        0xDB => Key::Char('['),
        0xDC => Key::Char('\\'),
        0xDD => Key::Char(']'),
        0xDE => Key::Char('\''),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use nsh_core::keymap::Key;

    #[test]
    fn repeats_without_releases() {
        use super::{KeyEvent, KeymapState, repeats};
        let state = KeymapState::default();
        let down = |vk: u16| KeyEvent {
            vk,
            down: true,
            repeat: false,
            modifiers: Default::default(),
        };
        assert!(!repeats(&state, &down(0x4B)));
        // X11's auto-repeat: the same key again, unreleased.
        assert!(repeats(&state, &down(0x4B)));
        // Shift going down does not end it.
        assert!(!repeats(&state, &down(0x10)));
        assert!(repeats(&state, &down(0x4B)));
        // Another key does; so does a release.
        assert!(!repeats(&state, &down(0x4C)));
        state.held.set(None);
        assert!(!repeats(&state, &down(0x4C)));
    }

    #[test]
    fn virtual_keys() {
        assert_eq!(super::key_of(0x4B), Some(Key::Char('k')));
        assert_eq!(super::key_of(0x31), Some(Key::Char('1')));
        assert_eq!(super::key_of(0x74), Some(Key::F(5)));
        assert_eq!(super::key_of(0xBF), Some(Key::Char('/')));
        assert_eq!(super::key_of(0x10), None);
    }
}

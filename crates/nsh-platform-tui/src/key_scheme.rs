//! Terminal keys select actions; focus, navigation and scrolling execute them.

use nsh_core::keymap::{Key, Keystroke, Modifiers};

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Next,
    Previous,
    Right,
    Left,
    Activate,
    Back,
    PageDown,
    PageUp,
    Start,
    End,
    Quit,
}

pub(crate) fn keystroke(name: &str, mut modifiers: Modifiers) -> Option<Keystroke> {
    let mut chars = name.chars();
    let key = if let (Some(c), None) = (chars.next(), chars.next()) {
        const SHIFTED: &str = "~!@#$%^&*()_+{}|:\"<>?";
        const BASE: &str = "`1234567890-=[]\\;',./";
        if let Some((_, base)) = SHIFTED
            .chars()
            .zip(BASE.chars())
            .find(|(shifted, _)| *shifted == c)
        {
            modifiers.shift = true;
            Key::Char(base)
        } else {
            modifiers.shift |= c.is_ascii_uppercase();
            Key::Char(c.to_ascii_lowercase())
        }
    } else {
        Keystroke::parse(name, nsh_core::platform::Os::default())
            .ok()?
            .key
    };
    Some(Keystroke { key, modifiers })
}

pub(crate) fn terminal(stroke: Keystroke) -> Option<Action> {
    let Keystroke { key, modifiers: m } = stroke;
    if m.cmd {
        return None;
    }
    if m.ctrl && !m.alt && !m.shift {
        return Some(match key {
            Key::Char('n') => Action::Next,
            Key::Char('p') => Action::Previous,
            Key::Char('f') => Action::Right,
            Key::Char('b') => Action::Left,
            Key::Char('g') => Action::Back,
            Key::Char('v') => Action::PageDown,
            Key::Char('c') => Action::Quit,
            _ => return None,
        });
    }
    if m.alt && !m.ctrl {
        return Some(match (key, m.shift) {
            (Key::Char('v'), false) => Action::PageUp,
            (Key::Char(','), true) => Action::Start,
            (Key::Char('.'), true) => Action::End,
            _ => return None,
        });
    }
    if m.ctrl || m.alt {
        return None;
    }
    if key == Key::Tab {
        return Some(if m.shift {
            Action::Previous
        } else {
            Action::Next
        });
    }
    if m.shift {
        return None;
    }
    Some(match key {
        Key::Down => Action::Next,
        Key::Up => Action::Previous,
        Key::Right => Action::Right,
        Key::Left => Action::Left,
        Key::Enter => Action::Activate,
        Key::Escape => Action::Back,
        Key::PageDown => Action::PageDown,
        Key::PageUp => Action::PageUp,
        Key::Home => Action::Start,
        Key::End => Action::End,
        _ => return None,
    })
}

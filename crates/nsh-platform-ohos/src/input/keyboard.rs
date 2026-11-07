//! Native window filtering; mounted bindings decide consumption before JS runs.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

use nsh_core::{
    NodeId,
    keymap::{Key, KeyMatch, Keymap, Keystroke, Modifiers},
};
use ohos_arkui_binding::event::KeyCode as N;
use ohos_window_manager_binding::{Input_KeyEvent, KeyEventRef, Window};

impl crate::render::node::Node {
    pub(crate) fn dismiss_keyboard(&self) -> crate::render::node::Result<()> {
        let Some(id) = self
            .dispatcher
            .upgrade()
            .and_then(|route| route.keyboard.lock().unwrap().input)
        else {
            return Ok(());
        };
        let mut root = self.parent_node.borrow().upgrade();
        while let Some(parent) = root
            .as_ref()
            .and_then(|node| node.parent_node.borrow().upgrade())
        {
            root = Some(parent);
        }
        let input = root.as_deref().unwrap_or(self).find_input(id);
        if let Some(input) = input {
            input.focus(false)?;
        }
        Ok(())
    }
    fn find_input(&self, id: NodeId) -> Option<std::rc::Rc<crate::input::editor::Input>> {
        if self.id == id {
            return self.input.borrow().clone();
        }
        self.children
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .find_map(|node| node.find_input(id))
    }
}
#[derive(Default)]
pub(crate) struct Snapshot {
    pub(crate) map: Arc<Keymap>,
    pub(crate) focused: Option<NodeId>,
    pub(crate) input: Option<NodeId>,
    pub(crate) selection: Option<NodeId>,
}
#[derive(Default)]
struct Held {
    keys: Vec<(N, bool)>,
}
static LIFECYCLE: Mutex<()> = Mutex::new(());

pub(crate) struct KeyboardWindow {
    id: i32,
    route: Arc<WindowRoute>,
}
impl KeyboardWindow {
    pub(crate) fn id(&self) -> i32 {
        self.id
    }
    pub(crate) fn clear(&self) {
        self.route.held.lock().unwrap().keys.clear();
    }
    pub(crate) fn modifiers(&self) -> Modifiers {
        self.route.held.lock().unwrap().modifiers()
    }
    pub(crate) fn new(
        id: i32,
        host: std::sync::Weak<crate::app::session::Services>,
    ) -> napi_ohos::Result<Self> {
        // Native filter registration is serialized independently of callback routing.
        let _lifecycle = LIFECYCLE.lock().unwrap();
        let route = Arc::new(WindowRoute {
            host,
            held: Mutex::new(Held::default()),
        });
        let first = {
            let mut routes = routes().lock().unwrap();
            let entries = routes.entry(id).or_default();
            entries.retain(|entry| entry.strong_count() > 0);
            let first = entries.is_empty();
            entries.push(Arc::downgrade(&route));
            first
        };
        if first && let Err(error) = Window::from_id(id).register_key_event_filter(filter) {
            routes().lock().unwrap().remove(&id);
            return Err(napi_ohos::Error::from_reason(error.to_string()));
        }
        Ok(Self { id, route })
    }
}
struct WindowRoute {
    host: Weak<crate::app::session::Services>,
    held: Mutex<Held>,
}
static ROUTES: OnceLock<Mutex<BTreeMap<i32, Vec<Weak<WindowRoute>>>>> = OnceLock::new();
fn routes() -> &'static Mutex<BTreeMap<i32, Vec<Weak<WindowRoute>>>> {
    ROUTES.get_or_init(Mutex::default)
}
impl Drop for KeyboardWindow {
    fn drop(&mut self) {
        let _lifecycle = LIFECYCLE.lock().unwrap();
        let empty = {
            let mut routes = routes().lock().unwrap();
            if let Some(entries) = routes.get_mut(&self.id) {
                entries.retain(|entry| {
                    entry.strong_count() > 0 && !entry.ptr_eq(&Arc::downgrade(&self.route))
                });
                if entries.is_empty() {
                    routes.remove(&self.id);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if empty && let Err(error) = Window::from_id(self.id).unregister_key_event_filter() {
            log::error!("OHOS key filter retirement: {error}");
        }
    }
}
unsafe extern "C" fn filter(raw: *mut Input_KeyEvent) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: the SDK owns the key event until this synchronous callback returns.
        let Some(event) = (unsafe { KeyEventRef::from_raw(raw) }) else {
            return false;
        };
        let routes = {
            routes()
                .lock()
                .unwrap()
                .get(&event.window_id())
                .map(|entries| entries.iter().filter_map(Weak::upgrade).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let mut consumed = false;
        for route in routes.into_iter().rev() {
            consumed |= route.event(&event, !consumed);
        }
        consumed
    }))
    .unwrap_or(false)
}
impl WindowRoute {
    fn event(&self, event: &KeyEventRef<'_>, deliver: bool) -> bool {
        if self.held.lock().unwrap().cancel(event.action()) {
            return false;
        }
        let Some(code) = N::try_from_raw(event.key_code()) else {
            return false;
        };
        let Some(host) = self.host.upgrade() else {
            return false;
        };
        if !host.is_foreground() {
            self.held.lock().unwrap().keys.clear();
            return false;
        }
        let route = host.keyboard_route();
        let mut held = self.held.lock().unwrap();
        held.event(event.action(), code, |stroke, repeat| {
            if !deliver {
                return false;
            }
            let Some(route) = route.as_ref() else {
                return false;
            };
            let snapshot = route.keyboard.lock().unwrap();
            if snapshot.focused.is_none()
                && let Some(id) = snapshot.selection
            {
                use crate::text::selection::controller::Command;
                let command = match stroke.key {
                    Key::Escape if stroke.modifiers == Modifiers::default() => Some(Command::Clear),
                    Key::Char('c')
                        if stroke.modifiers.ctrl
                            && !stroke.modifiers.alt
                            && !stroke.modifiers.cmd =>
                    {
                        Some(Command::Copy)
                    }
                    Key::Char('a')
                        if stroke.modifiers.ctrl
                            && !stroke.modifiers.alt
                            && !stroke.modifiers.cmd =>
                    {
                        Some(Command::SelectAll)
                    }
                    _ => None,
                };
                if let Some(command) = command {
                    if !repeat {
                        route.send(crate::app::runtime::Message::TextSelection(id, command));
                    }
                    return true;
                }
            }
            let matched = snapshot
                .map
                .find(stroke, snapshot.focused.is_some(), repeat);
            if let KeyMatch::Fire(id) = matched {
                route.send(crate::app::runtime::Message::HostEvent(
                    nsh_core::EventKind::KeyCommand,
                    nsh_core::EventPayload::Value(nsh_core::PayloadValue::Number(id as f64)),
                ));
            }
            if matched.consumed() {
                return true;
            }
            if !repeat {
                if stroke.key == Key::Escape && stroke.modifiers == Modifiers::default() {
                    if snapshot.input.is_some() {
                        route.send(crate::app::runtime::Message::DismissKeyboard);
                        return true;
                    }
                    drop(snapshot);
                    return host.handle_back();
                }
                if snapshot.focused.is_none()
                    && ((stroke.key == Key::Left
                        && stroke.modifiers
                            == Modifiers {
                                alt: true,
                                ..Modifiers::default()
                            })
                        || (stroke.key == Key::Char('[')
                            && stroke.modifiers
                                == Modifiers {
                                    cmd: true,
                                    ..Modifiers::default()
                                }))
                {
                    drop(snapshot);
                    return host.handle_back();
                }
            }
            false
        })
    }
}

impl Held {
    fn modifiers(&self) -> Modifiers {
        Modifiers {
            ctrl: self.has(N::CtrlLeft, N::CtrlRight),
            alt: self.has(N::AltLeft, N::AltRight),
            shift: self.has(N::ShiftLeft, N::ShiftRight),
            cmd: self.has(N::MetaLeft, N::MetaRight),
        }
    }
    fn cancel(&mut self, action: i32) -> bool {
        if action == KeyEventRef::ACTION_CANCEL {
            self.keys.clear();
            true
        } else {
            false
        }
    }
    fn event(&mut self, action: i32, code: N, fire: impl FnOnce(Keystroke, bool) -> bool) -> bool {
        if self.cancel(action) {
            return false;
        }
        let old = self
            .keys
            .iter()
            .position(|(held_code, _)| *held_code == code);
        if action == KeyEventRef::ACTION_UP {
            return old.is_some_and(|index| self.keys.swap_remove(index).1);
        }
        if action != KeyEventRef::ACTION_DOWN {
            return false;
        }
        let modifiers = self.modifiers();
        let consumed =
            key(code).is_some_and(|key| fire(Keystroke { key, modifiers }, old.is_some()));
        if let Some(index) = old {
            self.keys[index].1 |= consumed;
        } else {
            self.keys.push((code, consumed));
        }
        consumed
    }
    fn has(&self, left: N, right: N) -> bool {
        self.keys
            .iter()
            .any(|(code, _)| *code == left || *code == right)
    }
}
fn key(code: N) -> Option<Key> {
    use Key as K;
    Some(match code {
        N::A => K::Char('a'),
        N::B => K::Char('b'),
        N::C => K::Char('c'),
        N::D => K::Char('d'),
        N::E => K::Char('e'),
        N::F => K::Char('f'),
        N::G => K::Char('g'),
        N::H => K::Char('h'),
        N::I => K::Char('i'),
        N::J => K::Char('j'),
        N::K => K::Char('k'),
        N::L => K::Char('l'),
        N::M => K::Char('m'),
        N::N => K::Char('n'),
        N::O => K::Char('o'),
        N::P => K::Char('p'),
        N::Q => K::Char('q'),
        N::R => K::Char('r'),
        N::S => K::Char('s'),
        N::T => K::Char('t'),
        N::U => K::Char('u'),
        N::V => K::Char('v'),
        N::W => K::Char('w'),
        N::X => K::Char('x'),
        N::Y => K::Char('y'),
        N::Z => K::Char('z'),
        N::Digit0 => K::Char('0'),
        N::Digit1 => K::Char('1'),
        N::Digit2 => K::Char('2'),
        N::Digit3 => K::Char('3'),
        N::Digit4 => K::Char('4'),
        N::Digit5 => K::Char('5'),
        N::Digit6 => K::Char('6'),
        N::Digit7 => K::Char('7'),
        N::Digit8 => K::Char('8'),
        N::Digit9 => K::Char('9'),
        N::F1 => K::F(1),
        N::F2 => K::F(2),
        N::F3 => K::F(3),
        N::F4 => K::F(4),
        N::F5 => K::F(5),
        N::F6 => K::F(6),
        N::F7 => K::F(7),
        N::F8 => K::F(8),
        N::F9 => K::F(9),
        N::F10 => K::F(10),
        N::F11 => K::F(11),
        N::F12 => K::F(12),
        N::Enter => K::Enter,
        N::Escape => K::Escape,
        N::Tab => K::Tab,
        N::Space => K::Char(' '),
        N::Del => K::Backspace,
        N::ForwardDel => K::Delete,
        N::Insert => K::Insert,
        N::DpadUp => K::Up,
        N::DpadDown => K::Down,
        N::DpadLeft => K::Left,
        N::DpadRight => K::Right,
        N::MoveHome => K::Home,
        N::MoveEnd => K::End,
        N::PageUp => K::PageUp,
        N::PageDown => K::PageDown,
        N::Comma => K::Char(','),
        N::Period => K::Char('.'),
        N::Grave => K::Char('`'),
        N::Minus => K::Char('-'),
        N::Equals => K::Char('='),
        N::LeftBracket => K::Char('['),
        N::RightBracket => K::Char(']'),
        N::Backslash => K::Char('\\'),
        N::Semicolon => K::Char(';'),
        N::Apostrophe => K::Char('\''),
        N::Slash => K::Char('/'),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeat_and_consumed_release_survive_callback_borrowing() {
        let mut held = Held::default();
        assert!(held.event(KeyEventRef::ACTION_DOWN, N::A, |_, repeat| {
            assert!(!repeat);
            true
        }));
        assert!(!held.event(KeyEventRef::ACTION_DOWN, N::A, |_, repeat| {
            assert!(repeat);
            false
        }));
        assert!(held.event(KeyEventRef::ACTION_UP, N::A, |_, _| panic!(
            "up must not dispatch"
        )));
        assert!(!held.event(KeyEventRef::ACTION_UP, N::A, |_, _| false));
    }
    #[test]
    fn cancellation_does_not_require_a_decodable_key_code() {
        let mut held = Held::default();
        held.event(KeyEventRef::ACTION_DOWN, N::CtrlLeft, |_, _| false);
        assert!(held.cancel(KeyEventRef::ACTION_CANCEL));
        assert!(held.keys.is_empty());
    }
    #[test]
    fn modifier_state_and_cancel_are_native_only() {
        let mut held = Held::default();
        held.event(KeyEventRef::ACTION_DOWN, N::CtrlLeft, |_, _| false);
        held.event(KeyEventRef::ACTION_DOWN, N::C, |stroke, _| {
            assert!(stroke.modifiers.ctrl);
            false
        });
        held.event(KeyEventRef::ACTION_CANCEL, N::C, |_, _| false);
        held.event(KeyEventRef::ACTION_DOWN, N::C, |stroke, _| {
            assert!(!stroke.modifiers.ctrl);
            false
        });
    }
}

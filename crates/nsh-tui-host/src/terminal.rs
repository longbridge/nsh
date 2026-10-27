use std::{
    io::{self, stdout},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nsh_platform_tui::{Input, Modifiers, MouseButton, MouseInput, MouseKind, RunEvent};
use ratatui_crossterm::crossterm::{cursor, event, execute, style::Print, terminal};

const ENABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007h";
const DISABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007l";

pub(crate) struct Session {
    active: Arc<AtomicBool>,
}

fn restore(active: &AtomicBool) {
    if !active.swap(false, Ordering::AcqRel) {
        return;
    }
    // Try every restoration even when a disconnected terminal rejects one.
    let _ = execute!(stdout(), event::DisableMouseCapture);
    let _ = execute!(stdout(), Print(DISABLE_ALTERNATE_SCROLL));
    let _ = execute!(stdout(), terminal::LeaveAlternateScreen);
    let _ = execute!(stdout(), cursor::Show);
    let _ = terminal::disable_raw_mode();
}

impl Session {
    pub(crate) fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub(crate) fn enter() -> io::Result<Self> {
        let session = Self {
            active: Arc::new(AtomicBool::new(true)),
        };
        let previous = std::panic::take_hook();
        let active = session.active.clone();
        std::panic::set_hook(Box::new(move |panic| {
            restore(&active);
            previous(panic);
        }));
        terminal::enable_raw_mode()?;
        execute!(
            stdout(),
            terminal::EnterAlternateScreen,
            Print(ENABLE_ALTERNATE_SCROLL),
            event::EnableMouseCapture,
            cursor::Hide
        )?;
        Ok(session)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        restore(&self.active);
    }
}

pub(crate) fn poll(timeout: Duration) -> io::Result<RunEvent> {
    if event::poll(timeout)? {
        Ok(convert(event::read()?))
    } else {
        Ok(RunEvent::Idle)
    }
}

fn modifiers(keys: event::KeyModifiers) -> Modifiers {
    Modifiers {
        ctrl: keys.contains(event::KeyModifiers::CONTROL),
        alt: keys.contains(event::KeyModifiers::ALT),
        shift: keys.contains(event::KeyModifiers::SHIFT),
        cmd: keys.contains(event::KeyModifiers::SUPER),
    }
}

fn button(button: event::MouseButton) -> MouseButton {
    match button {
        event::MouseButton::Left => MouseButton::Left,
        event::MouseButton::Right => MouseButton::Right,
        event::MouseButton::Middle => MouseButton::Middle,
    }
}

/// Converts host transport events; terminal resize is a redraw, never view input.
pub fn convert(event: event::Event) -> RunEvent {
    match event {
        event::Event::Resize(..) => RunEvent::Redraw,
        event::Event::FocusGained => RunEvent::Input(Input::Focus(true)),
        event::Event::FocusLost => RunEvent::Input(Input::Focus(false)),
        event::Event::Paste(text) => RunEvent::Input(Input::Paste(text)),
        event::Event::Mouse(mouse) => {
            let mut input = MouseInput::default();
            input.column = mouse.column;
            input.row = mouse.row;
            input.modifiers = modifiers(mouse.modifiers);
            input.kind = match mouse.kind {
                event::MouseEventKind::Down(b) => MouseKind::Down(button(b)),
                event::MouseEventKind::Up(b) => MouseKind::Up(button(b)),
                event::MouseEventKind::Moved | event::MouseEventKind::Drag(_) => MouseKind::Move,
                event::MouseEventKind::ScrollUp => MouseKind::Wheel { x: 0, y: -1 },
                event::MouseEventKind::ScrollDown => MouseKind::Wheel { x: 0, y: 1 },
                event::MouseEventKind::ScrollLeft => MouseKind::Wheel { x: -1, y: 0 },
                event::MouseEventKind::ScrollRight => MouseKind::Wheel { x: 1, y: 0 },
            };
            RunEvent::Input(Input::Mouse(input))
        }
        event::Event::Key(key) => {
            let release = key.kind == event::KeyEventKind::Release;
            if !release
                && key.code == event::KeyCode::Char('c')
                && key.modifiers == event::KeyModifiers::CONTROL
            {
                return RunEvent::Exit;
            }
            let name = match key.code {
                event::KeyCode::Char(c) => c.to_string(),
                event::KeyCode::Esc => "Escape".into(),
                event::KeyCode::BackTab => "Tab".into(),
                event::KeyCode::F(number) => format!("F{number}"),
                other => format!("{other:?}"),
            };
            let mut modifiers = modifiers(key.modifiers);
            if key.code == event::KeyCode::BackTab {
                modifiers.shift = true;
            }
            if key.kind == event::KeyEventKind::Repeat {
                RunEvent::Input(Input::KeyRepeat {
                    key: name,
                    modifiers,
                })
            } else {
                RunEvent::Input(Input::Key {
                    key: name,
                    modifiers,
                    release,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_conversion_keeps_absolute_coordinates_and_modifiers() {
        let event = event::Event::Mouse(event::MouseEvent {
            kind: event::MouseEventKind::Down(event::MouseButton::Left),
            column: 17,
            row: 9,
            modifiers: event::KeyModifiers::SHIFT,
        });
        let RunEvent::Input(Input::Mouse(mouse)) = convert(event) else {
            panic!("expected mouse");
        };
        assert_eq!((mouse.column, mouse.row), (17, 9));
        assert!(mouse.modifiers.shift);
        assert_eq!(mouse.kind, MouseKind::Down(MouseButton::Left));
    }

    #[test]
    fn resize_is_a_redraw_and_control_c_exits_only_on_press() {
        assert!(matches!(
            convert(event::Event::Resize(80, 24)),
            RunEvent::Redraw
        ));
        let mut key = event::KeyEvent::new(event::KeyCode::Char('c'), event::KeyModifiers::CONTROL);
        assert!(matches!(convert(event::Event::Key(key)), RunEvent::Exit));
        key.kind = event::KeyEventKind::Release;
        assert!(matches!(
            convert(event::Event::Key(key)),
            RunEvent::Input(Input::Key { release: true, .. })
        ));
    }

    #[test]
    fn control_meta_and_navigation_keys_keep_their_identity() {
        for (code, held, name) in [
            (event::KeyCode::Char('n'), event::KeyModifiers::CONTROL, "n"),
            (event::KeyCode::Char('v'), event::KeyModifiers::ALT, "v"),
            (event::KeyCode::Char('<'), event::KeyModifiers::ALT, "<"),
            (
                event::KeyCode::PageDown,
                event::KeyModifiers::NONE,
                "PageDown",
            ),
            (event::KeyCode::BackTab, event::KeyModifiers::NONE, "Tab"),
        ] {
            let RunEvent::Input(Input::Key {
                key,
                modifiers,
                release,
            }) = convert(event::Event::Key(event::KeyEvent::new(code, held)))
            else {
                panic!("key input");
            };
            assert_eq!(key, name);
            assert_eq!(modifiers.ctrl, held.contains(event::KeyModifiers::CONTROL));
            assert_eq!(modifiers.alt, held.contains(event::KeyModifiers::ALT));
            assert_eq!(modifiers.shift, code == event::KeyCode::BackTab);
            assert!(!release);
        }
        let key = event::KeyEvent::new_with_kind(
            event::KeyCode::Char('n'),
            event::KeyModifiers::CONTROL,
            event::KeyEventKind::Repeat,
        );
        assert!(matches!(
            convert(event::Event::Key(key)),
            RunEvent::Input(Input::KeyRepeat { .. })
        ));
    }

    #[test]
    #[ignore = "requires an interactive terminal"]
    fn normal_exit_restores_raw_mode() {
        assert!(!terminal::is_raw_mode_enabled().unwrap());
        let session = Session::enter().unwrap();
        assert!(terminal::is_raw_mode_enabled().unwrap());
        drop(session);
        assert!(!terminal::is_raw_mode_enabled().unwrap());
    }

    #[test]
    #[ignore = "requires an interactive terminal"]
    fn panic_restores_raw_mode() {
        assert!(!terminal::is_raw_mode_enabled().unwrap());
        let result = std::panic::catch_unwind(|| {
            let _session = Session::enter().unwrap();
            panic!("terminal restoration test");
        });
        assert!(result.is_err());
        assert!(!terminal::is_raw_mode_enabled().unwrap());
    }
}

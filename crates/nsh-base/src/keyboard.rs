//! The software keyboard: `Keyboard` (`KeyboardAvoidingView` is a
//! primitive of `nsh`, `nsh_core::element`).

use nsh_core::element::prelude::*;

/// The software keyboard.
#[module]
#[derive(Debug)]
pub struct Keyboard;

#[module]
impl Keyboard {
    /// Unfocuses the focused input, hiding the keyboard.
    pub fn dismiss(cx: &mut ModuleCx) {
        cx.dismiss_keyboard();
    }
}

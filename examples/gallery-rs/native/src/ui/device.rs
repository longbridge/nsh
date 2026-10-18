//! What depends on the device's input (as the TSX gallery's `hint`):
//! phones and tablets are touched, desktops pointed at.

use nsh_core::ui::prelude::*;

/// Instructions for the device's input: `touch` on phones and tablets,
/// `pointer` on desktops (a mouse, a touchpad, a keyboard).
pub(crate) fn hint(touch: &'static str, pointer: &'static str) -> &'static str {
    if is_desktop() { pointer } else { touch }
}

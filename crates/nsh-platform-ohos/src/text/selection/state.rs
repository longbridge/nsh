//! UI-owned selection state and menu generations, with no native handles.
use std::cell::{Cell, RefCell};

#[derive(Default)]
pub(crate) struct State {
    pub(crate) range: Cell<Option<(usize, usize)>>,
    pub(crate) menu: Cell<bool>,
    pub(crate) generation: Cell<u32>,
    pub(crate) geometry: RefCell<super::layout::Geometry>,
}
impl State {
    pub(crate) fn invalidate_menu(&self) {
        self.generation
            .set(self.generation.get().wrapping_add(1).max(1));
    }
    /// Lifting the long-press contact keeps the callbacks of its visible menu valid.
    pub(crate) fn open_menu(&self) -> bool {
        if self.range.get().is_none() || self.menu.replace(true) {
            return false;
        }
        self.invalidate_menu();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_visible_menu_keeps_its_generation_until_dismissed_or_selection_changes() {
        let state = State::default();
        assert!(!state.open_menu());
        state.range.set(Some((0, 5)));
        assert!(state.open_menu());
        let shown = state.generation.get();
        assert!(!state.open_menu());
        assert_eq!(state.generation.get(), shown);
        state.menu.set(false);
        assert!(state.open_menu());
        assert_ne!(state.generation.get(), shown);
        let reopened = state.generation.get();
        state.invalidate_menu();
        assert_ne!(state.generation.get(), reopened);
        state.generation.set(u32::MAX);
        state.invalidate_menu();
        assert_eq!(state.generation.get(), 1);
    }
}

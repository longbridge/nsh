//! The context menu steps: `rightClick(<selector>)`, the mouse's secondary
//! button over a view, and choosing from (or closing) the menu the app
//! then shows with `Menu.popup`, as a user does on a desktop.

use nsh_core::{ContextMenuEvent, EventKind, EventPayload, PayloadValue, ROOT_ID, geometry::point};

use super::Headless;
use crate::{menu, selector::Selector};

impl Headless {
    /// The step `rightClick(<selector>)`: the mouse's secondary button over
    /// the middle of the view `selector` names, on a desktop (`open`'s
    /// `platform`): the view, or the nearest around it listening, hears
    /// `onContextMenu`; its handler shows a menu (`Menu.popup`) or its own
    /// UI. A phone has no mouse: there it fails.
    pub fn right_click(&mut self, selector: &str) -> Result<(), String> {
        let parsed = Selector::parse(selector)?;
        self.step(format!("rightClick({selector})"));
        if !self.environment.desktop() {
            return Err(
                "a phone has no mouse to right-click with: open the app as a desktop (`platform`)"
                    .into(),
            );
        }
        let pick = self.screen.view(&parsed);
        let target = self.picked(pick, selector, "view")?;
        let at = self
            .screen
            .bounds_in_root(target)
            .ok_or_else(|| format!("`{selector}` is not on screen"))?
            .center();
        if let Some(id) = self.screen.context_menu_target(target) {
            let origin = self
                .screen
                .bounds_in_root(id)
                .map_or(Default::default(), |b| b.origin);
            let event = ContextMenuEvent {
                position: point(at.x - origin.x, at.y - origin.y),
                page_position: at,
                modifiers: Default::default(),
            };
            self.send(id, EventKind::ContextMenu, EventPayload::ContextMenu(event));
        }
        self.done()
    }

    /// The actions of the context menu the app shows (`Menu.popup`), `None`
    /// when none is open.
    pub fn menu(&self) -> Option<Vec<menu::Entry>> {
        self.screen.menu().map(menu::entries)
    }

    /// The step `chooseMenuItem(<label>)`: chooses the open context menu's
    /// action labelled `label` (a submenu's: `"Share > Mail"`), as a click
    /// on it does: the app hears `onMenu` with its id, then the menu closes.
    pub fn choose_menu_item(&mut self, label: &str) -> Result<(), String> {
        self.step(format!("chooseMenuItem({label})"));
        let items = self.screen.menu().ok_or("no context menu is open")?;
        let action = menu::find(items, label)?;
        let id = action.id;
        if id != 0 {
            self.menu_event(vec![("id", PayloadValue::Number(f64::from(id)))]);
        }
        self.close_open_menu();
        self.done()
    }

    /// The step `closeMenu()`: the open context menu closes with nothing
    /// chosen, as Escape or a click elsewhere closes it.
    pub fn close_menu(&mut self) -> Result<(), String> {
        self.step("closeMenu");
        if self.screen.menu().is_none() {
            return Err("no context menu is open".into());
        }
        self.close_open_menu();
        self.done()
    }

    fn close_open_menu(&mut self) {
        self.screen.menu_closed();
        // The app's `Menu.popup` promise settles.
        self.menu_event(vec![
            ("id", PayloadValue::Number(0.0)),
            ("closed", PayloadValue::Bool(true)),
        ]);
    }

    fn menu_event(&self, fields: Vec<(&'static str, PayloadValue)>) {
        self.engine
            .control()
            .dispatch_event(ROOT_ID, EventKind::Menu, EventPayload::Map(fields));
    }
}

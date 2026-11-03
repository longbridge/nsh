//! Context menus: `nsh/desktop`'s `Menu.popup`
//! ([`nsh_core::desktop::DesktopCommand::PopupMenu`]), drawn by the backend in the terminal's
//! own look as the desktop backends show the system's (`NSMenu`,
//! `TrackPopupMenuEx`, GTK's `PopoverMenu`): a box of rows at the pointer
//! (or the point the app gives), on the raised surface of an anchored popup
//! with the popover's line, a check mark column when an item is checkable,
//! shortcuts at the right and `›` on a submenu, whose box opens beside its
//! row. Disabled items are faint; separators are lines across the box.
//! Every label is the app's.
//!
//! An open menu takes every key and press until it closes. Up / Down and
//! Tab / Shift+Tab move between the items that can be chosen, round the
//! box; Home / End go to the first and last; a letter to the next item it
//! starts; Right, Enter or Space opens a submenu; Left or Escape closes
//! one; Enter or Space chooses an item, Escape at the top closes the menu.
//! The pointer moves the focus and opens submenus; a press on an item
//! chooses it, a press outside the boxes closes the menu and goes no
//! further. A chosen item is reported as [`EventKind::Menu`] with its id
//! (an item with a role closing the window ends the app), then the menu's
//! closing as `{ id: 0, closed: true }`, which settles the app's promise.

use nsh_core::{
    EventKind, EventPayload, PayloadValue, Point, Points, ROOT_ID,
    desktop::{MenuAction, MenuItem, MenuRole, Shortcut},
    keymap::{Key, Modifiers},
};
use ratatui_core::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    CellSize,
    dialog::{clear, put},
    key_scheme::{self, Action},
    palette::{RAISED, Ramp, SELECT},
};

/// Where a menu's top left corner goes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    /// The pointer's Cell.
    Cell(Position),
    /// The app's point (the surface's coordinates).
    Point(Point<Points>),
}

/// One box: the root menu or an open submenu.
struct Level {
    items: Vec<MenuItem>,
    /// The focused item (an index of `items`).
    focused: Option<usize>,
    /// The box, as last drawn.
    rect: Rect,
    /// Each item's row, as last drawn (separators included).
    rows: Vec<Rect>,
}

impl Level {
    fn new(items: Vec<MenuItem>, focus_first: bool) -> Self {
        let mut level = Self {
            items,
            focused: None,
            rect: Rect::default(),
            rows: Vec::new(),
        };
        if focus_first {
            level.focused = level.next(None, true);
        }
        level
    }

    fn choosable(&self, index: usize) -> bool {
        match &self.items[index] {
            MenuItem::Action(action) => action.enabled,
            MenuItem::Submenu { enabled, .. } => *enabled,
            MenuItem::Separator => false,
        }
    }

    /// The item that can be chosen after (`forward`) or before `from`,
    /// round the box.
    fn next(&self, from: Option<usize>, forward: bool) -> Option<usize> {
        let count = self.items.len();
        (1..=count)
            .map(|step| match (from, forward) {
                (Some(at), true) => (at + step) % count,
                (Some(at), false) => (at + count - step % count) % count,
                (None, true) => step - 1,
                (None, false) => count - step,
            })
            .find(|&i| self.choosable(i))
    }

    /// The next item after the focused one whose label starts with `c`.
    fn typed(&self, c: char) -> Option<usize> {
        let count = self.items.len();
        let from = self.focused.unwrap_or(count - 1);
        let c = c.to_lowercase().next()?;
        (1..=count).map(|step| (from + step) % count).find(|&i| {
            self.choosable(i)
                && label(&self.items[i])
                    .chars()
                    .next()
                    .and_then(|first| first.to_lowercase().next())
                    == Some(c)
        })
    }

    fn row_at(&self, position: Position) -> Option<usize> {
        self.rows.iter().position(|r| r.contains(position))
    }
}

/// What a key or press did while a menu is open.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// The menu took it and stays open.
    Taken,
    /// The menu closed, with the item chosen if any: its id and role.
    Closed(Option<(u32, Option<MenuRole>)>),
}

impl Outcome {
    /// Reports a closing to the app: the chosen item, then the closing.
    /// Returns whether the chosen item ends the app (its role closes the
    /// window).
    pub(crate) fn dispatch(&self, control: &nsh_engine::EngineControl) -> bool {
        let Outcome::Closed(chosen) = self else {
            return false;
        };
        if let Some((id, _)) = chosen
            && *id != 0
        {
            control.dispatch_event(
                ROOT_ID,
                EventKind::Menu,
                EventPayload::Map(vec![("id", PayloadValue::Number(f64::from(*id)))]),
            );
        }
        control.dispatch_event(
            ROOT_ID,
            EventKind::Menu,
            EventPayload::Map(vec![
                ("id", PayloadValue::Number(0.0)),
                ("closed", PayloadValue::Bool(true)),
            ]),
        );
        matches!(chosen, Some((_, Some(MenuRole::Quit | MenuRole::Close))))
    }
}

/// The context menu open, if any, and where the pointer last was.
#[derive(Default)]
pub(crate) struct Menus {
    anchor: Option<Anchor>,
    /// The root box first, then each open submenu.
    levels: Vec<Level>,
    /// The pointer's last Cell: where a menu without a point of its own
    /// opens.
    pub(crate) pointer: Option<Position>,
}

impl Menus {
    /// Opens `items` at `at` (the surface's points) or at the pointer, in
    /// place of a menu open before (its promise is settled by now).
    pub(crate) fn popup(&mut self, items: &[MenuItem], at: Option<Point<Points>>) {
        self.anchor = Some(match at {
            Some(at) => Anchor::Point(at),
            None => Anchor::Cell(self.pointer.unwrap_or_default()),
        });
        self.levels = vec![Level::new(items.to_vec(), false)];
    }

    /// Whether a menu is open (and takes all input).
    pub(crate) fn is_open(&self) -> bool {
        !self.levels.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.levels.clear();
        self.anchor = None;
    }

    fn close(&mut self, chosen: Option<(u32, Option<MenuRole>)>) -> Outcome {
        self.clear();
        Outcome::Closed(chosen)
    }

    /// Opens the submenu of the focused item of the innermost box, its
    /// first item focused when `focus`.
    fn open_submenu(&mut self, focus: bool) -> bool {
        let Some(level) = self.levels.last() else {
            return false;
        };
        let Some(MenuItem::Submenu {
            enabled: true,
            items,
            ..
        }) = level.focused.map(|i| &level.items[i])
        else {
            return false;
        };
        let items = items.clone();
        self.levels.push(Level::new(items, focus));
        true
    }

    /// Chooses item `index` of box `depth`: an action closes the menu, a
    /// submenu opens.
    fn choose(&mut self, depth: usize, index: usize) -> Outcome {
        self.levels.truncate(depth + 1);
        let level = &mut self.levels[depth];
        if !level.choosable(index) {
            return Outcome::Taken;
        }
        level.focused = Some(index);
        match &level.items[index] {
            MenuItem::Action(MenuAction { id, role, .. }) => {
                let chosen = (*id, *role);
                self.close(Some(chosen))
            }
            _ => {
                self.open_submenu(true);
                Outcome::Taken
            }
        }
    }

    /// Hands a key to the menu; `None` when none is open.
    pub(crate) fn key(&mut self, name: &str, modifiers: Modifiers) -> Option<Outcome> {
        if !self.is_open() {
            return None;
        }
        let Some(stroke) = key_scheme::keystroke(name, modifiers) else {
            return Some(Outcome::Taken);
        };
        // The terminal's aliases: Ctrl+N / Ctrl+P down and up, Ctrl+F /
        // Ctrl+B right and left, Ctrl+G as Escape.
        let alias = match key_scheme::terminal(stroke) {
            Some(Action::Next) => Some(Key::Down),
            Some(Action::Previous) => Some(Key::Up),
            Some(Action::Right) if stroke.modifiers.ctrl => Some(Key::Right),
            Some(Action::Left) if stroke.modifiers.ctrl => Some(Key::Left),
            Some(Action::Back) => Some(Key::Escape),
            _ => None,
        };
        let (key, m) = match alias {
            Some(key) => (key, Modifiers::default()),
            None => (stroke.key, stroke.modifiers),
        };
        if m.ctrl || m.alt || m.cmd {
            return Some(Outcome::Taken);
        }
        let depth = self.levels.len() - 1;
        let level = self.levels.last_mut()?;
        match key {
            Key::Down => level.focused = level.next(level.focused, true),
            Key::Up => level.focused = level.next(level.focused, false),
            Key::Tab => level.focused = level.next(level.focused, !m.shift),
            Key::Home => level.focused = level.next(None, true),
            Key::End => level.focused = level.next(None, false),
            Key::Right => {
                self.open_submenu(true);
            }
            Key::Left if depth > 0 => {
                self.levels.pop();
            }
            Key::Escape if depth > 0 => {
                self.levels.pop();
            }
            Key::Escape => return Some(self.close(None)),
            Key::Enter | Key::Char(' ') => {
                if let Some(index) = level.focused {
                    return Some(self.choose(depth, index));
                }
            }
            Key::Char(c) if !c.is_control() => {
                if let Some(index) = level.typed(c) {
                    level.focused = Some(index);
                }
            }
            _ => {}
        }
        Some(Outcome::Taken)
    }

    /// The box and row under `position`, innermost box first.
    fn hit(&self, position: Position) -> Option<(usize, Option<usize>)> {
        self.levels
            .iter()
            .enumerate()
            .rev()
            .find(|(_, level)| level.rect.contains(position))
            .map(|(depth, level)| (depth, level.row_at(position)))
    }

    /// The pointer moved to `position`: the item under it takes the focus,
    /// a submenu's opens, the boxes past it close.
    pub(crate) fn hover(&mut self, position: Position) -> Option<Outcome> {
        if !self.is_open() {
            return None;
        }
        self.pointer = Some(position);
        if let Some((depth, Some(index))) = self.hit(position) {
            let level = &self.levels[depth];
            // The item whose submenu is open keeps it.
            let opened = level.focused == Some(index) && self.levels.len() > depth + 1;
            if !opened && level.choosable(index) {
                self.levels.truncate(depth + 1);
                self.levels[depth].focused = Some(index);
                self.open_submenu(false);
            }
        }
        Some(Outcome::Taken)
    }

    /// Hands a press at `position` to the menu; `None` when none is open.
    pub(crate) fn press(&mut self, position: Position) -> Option<Outcome> {
        if !self.is_open() {
            return None;
        }
        Some(match self.hit(position) {
            Some((depth, Some(index))) => self.choose(depth, index),
            Some((_, None)) => Outcome::Taken,
            None => self.close(None),
        })
    }

    /// Draws the open menu over everything in `area`, on the terminal's
    /// surfaces `ramp` when it has them.
    pub(crate) fn draw(
        &mut self,
        buffer: &mut Buffer,
        area: Rect,
        cells: CellSize,
        ramp: Option<Ramp>,
    ) {
        let area = area.intersection(buffer.area);
        let Some(anchor) = self.anchor else {
            return;
        };
        let at = match anchor {
            Anchor::Cell(at) => at,
            Anchor::Point(at) => Position::new(
                area.x
                    .saturating_add((at.x / cells.width()).floor().max(0.0) as u16),
                area.y
                    .saturating_add((at.y / cells.height()).floor().max(0.0) as u16),
            ),
        };
        let mac = crate::app::OS == nsh_core::platform::Os::MacOs;
        let mut beside: Option<(Rect, u16)> = None;
        for level in &mut self.levels {
            let checks = level
                .items
                .iter()
                .any(|i| matches!(i, MenuItem::Action(a) if a.checked.is_some()));
            let mark = if checks { 2 } else { 0 };
            let rights: Vec<String> = level.items.iter().map(|i| right(i, mac)).collect();
            let labels = level
                .items
                .iter()
                .map(|i| width(label(i)))
                .max()
                .unwrap_or(0);
            let widest_right = rights.iter().map(|r| width(r)).max().unwrap_or(0);
            let gap = if widest_right > 0 { 3 } else { 0 };
            let inner = (mark + labels + gap + widest_right).max(8);
            let w = (inner + 4).min(area.width);
            let h = (level.items.len() as u16 + 2).min(area.height);
            let (x, y) = match beside {
                // A submenu beside its row, on the other side when it has
                // no room.
                Some((parent, row)) => {
                    let x = if parent.right() + w <= area.right() {
                        parent.right()
                    } else {
                        parent.x.saturating_sub(w).max(area.x)
                    };
                    (x, row.saturating_sub(1))
                }
                None => {
                    let x = if at.x + w <= area.right() {
                        at.x
                    } else {
                        area.right().saturating_sub(w)
                    };
                    // Above the pointer when there is no room below it.
                    let y = if at.y + h <= area.bottom() {
                        at.y
                    } else if at.y + 1 >= area.y + h {
                        at.y + 1 - h
                    } else {
                        area.bottom().saturating_sub(h)
                    };
                    (x, y)
                }
            };
            let y = y.min(area.bottom().saturating_sub(h)).max(area.y);
            let rect = Rect::new(x.max(area.x), y, w, h);
            level.rect = rect;
            clear(buffer, rect);
            let (line, surface, band) = match ramp {
                Some(ramp) => (
                    ramp.popover_line(),
                    ramp.level(RAISED),
                    Some(ramp.level(SELECT)),
                ),
                None => (Color::Reset, Color::Reset, None),
            };
            if ramp.is_some() {
                for y in rect.y..rect.bottom() {
                    for x in rect.x..rect.right() {
                        buffer[(x, y)].bg = surface;
                    }
                }
            }
            border(buffer, rect, line, ramp.is_none());
            level.rows.clear();
            let content = Rect::new(
                rect.x + 1,
                rect.y + 1,
                rect.width.saturating_sub(2),
                rect.height.saturating_sub(2),
            );
            for (index, item) in level.items.iter().enumerate() {
                let y = content.y + index as u16;
                if y >= content.bottom() {
                    break;
                }
                let row = Rect::new(content.x, y, content.width, 1);
                level.rows.push(row);
                if matches!(item, MenuItem::Separator) {
                    for x in rect.x..rect.right() {
                        let symbol = if x == rect.x {
                            "├"
                        } else if x + 1 == rect.right() {
                            "┤"
                        } else {
                            "─"
                        };
                        let cell = &mut buffer[(x, y)];
                        cell.set_symbol(symbol);
                        cell.fg = line;
                        cell.modifier = if ramp.is_none() {
                            Modifier::DIM
                        } else {
                            Modifier::empty()
                        };
                    }
                    continue;
                }
                let enabled = level.choosable(index);
                let (color, mut modifier) = match (enabled, ramp) {
                    (true, _) => (Color::Reset, Modifier::empty()),
                    (false, Some(ramp)) => (ramp.faint(), Modifier::empty()),
                    (false, None) => (Color::Reset, Modifier::DIM),
                };
                let focused = level.focused == Some(index);
                if focused && band.is_none() {
                    modifier |= Modifier::REVERSED;
                }
                let text_x = row.x + 1;
                let room = row.width.saturating_sub(2);
                if focused {
                    for x in row.x..row.right() {
                        let cell = &mut buffer[(x, y)];
                        cell.set_symbol(" ");
                        match band {
                            Some(band) => cell.bg = band,
                            None => cell.modifier = Modifier::REVERSED,
                        }
                    }
                }
                if let MenuItem::Action(MenuAction {
                    checked: Some(true),
                    ..
                }) = item
                {
                    put(buffer, text_x, y, "✓", room, color, modifier);
                }
                put(
                    buffer,
                    text_x + mark,
                    y,
                    label(item),
                    room.saturating_sub(mark),
                    color,
                    modifier,
                );
                let right = &rights[index];
                if !right.is_empty() {
                    let rw = width(right).min(room);
                    let muted = match (enabled, ramp) {
                        (true, Some(ramp)) => ramp.muted(),
                        _ => color,
                    };
                    put(buffer, text_x + room - rw, y, right, rw, muted, modifier);
                }
            }
            beside = level
                .focused
                .and_then(|i| level.rows.get(i))
                .map(|row| (rect, row.y));
        }
    }
}

/// What item `item` shows.
fn label(item: &MenuItem) -> &str {
    match item {
        MenuItem::Action(action) if action.label.is_empty() => {
            action.role.map_or("", MenuRole::label)
        }
        MenuItem::Action(action) => &action.label,
        MenuItem::Submenu { label, .. } => label,
        MenuItem::Separator => "",
    }
}

/// What an item shows at its right: its shortcut, or a submenu's `›`.
fn right(item: &MenuItem, mac: bool) -> String {
    match item {
        MenuItem::Action(action) => action
            .shortcut
            .or_else(|| action.role.and_then(|role| role.shortcut(mac)))
            .map(|s| shortcut(&s, mac))
            .unwrap_or_default(),
        MenuItem::Submenu { .. } => "›".into(),
        MenuItem::Separator => String::new(),
    }
}

/// A shortcut as the platform's menus write it: macOS's symbols, else
/// `Ctrl+Shift+N`.
fn shortcut(s: &Shortcut, mac: bool) -> String {
    if !mac {
        return s.label();
    }
    let mut out = String::new();
    for (on, symbol) in [(s.ctrl, "⌃"), (s.alt, "⌥"), (s.shift, "⇧"), (s.meta, "⌘")] {
        if on {
            out.push_str(symbol);
        }
    }
    out.push_str(&s.key.label());
    out
}

/// The rounded box on the edge Cells of `rect`, dim without surfaces.
fn border(buffer: &mut Buffer, rect: Rect, line: Color, dim: bool) {
    if rect.width < 2 || rect.height < 2 {
        return;
    }
    let modifier = if dim {
        Modifier::DIM
    } else {
        Modifier::empty()
    };
    let (right, bottom) = (rect.right() - 1, rect.bottom() - 1);
    let mut set = |x: u16, y: u16, symbol: &str| {
        let cell = &mut buffer[(x, y)];
        cell.set_symbol(symbol);
        cell.fg = line;
        cell.modifier = modifier;
    };
    for x in rect.x + 1..right {
        set(x, rect.y, "─");
        set(x, bottom, "─");
    }
    for y in rect.y + 1..bottom {
        set(rect.x, y, "│");
        set(right, y, "│");
    }
    set(rect.x, rect.y, "╭");
    set(right, rect.y, "╮");
    set(rect.x, bottom, "╰");
    set(right, bottom, "╯");
}

fn width(text: &str) -> u16 {
    text.width().min(usize::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use nsh_core::desktop::{MenuAction, MenuItem};
    use ratatui_core::{
        buffer::Buffer,
        layout::{Position, Rect},
    };

    use super::{Menus, Outcome};

    fn action(id: u32, label: &str, enabled: bool) -> MenuItem {
        MenuItem::Action(MenuAction {
            id,
            label: label.into(),
            shortcut: None,
            enabled,
            checked: None,
            role: None,
            when: Default::default(),
        })
    }

    fn items() -> Vec<MenuItem> {
        vec![
            action(1, "Copy", true),
            action(2, "Paste", false),
            MenuItem::Separator,
            MenuItem::Submenu {
                label: "Share".into(),
                enabled: true,
                items: vec![action(3, "Mail", true)],
            },
        ]
    }

    fn rows(buffer: &Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    fn draw(menus: &mut Menus, buffer: &mut Buffer) {
        let area = buffer.area;
        menus.draw(buffer, area, crate::CellSize::default(), None);
    }

    #[test]
    fn a_menu_opens_at_the_pointer_with_its_items_in_rows() {
        let mut menus = Menus {
            pointer: Some(Position::new(4, 2)),
            ..Menus::default()
        };
        menus.popup(&items(), None);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 12));
        draw(&mut menus, &mut buffer);
        let rows = rows(&buffer);
        assert!(rows[2][4..].starts_with('╭'), "{rows:#?}");
        assert!(rows[3].contains("│ Copy"), "{rows:#?}");
        assert!(rows[5].contains("├"), "{rows:#?}");
        assert!(
            rows[6].contains("Share") && rows[6].contains('›'),
            "{rows:#?}"
        );
    }

    #[test]
    fn keys_skip_disabled_items_open_submenus_and_choose() {
        let mut menus = Menus::default();
        menus.popup(&items(), None);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 12));
        let none = Default::default();
        assert_eq!(menus.key("ArrowDown", none), Some(Outcome::Taken));
        assert_eq!(menus.key("ArrowDown", none), Some(Outcome::Taken));
        draw(&mut menus, &mut buffer);
        assert_eq!(menus.key("ArrowRight", none), Some(Outcome::Taken));
        draw(&mut menus, &mut buffer);
        assert!(rows(&buffer).iter().any(|r| r.contains("Mail")));
        assert_eq!(
            menus.key("Enter", none),
            Some(Outcome::Closed(Some((3, None))))
        );
        assert!(!menus.is_open());
        menus.popup(&items(), None);
        // Tab goes round the items that can be chosen; Escape closes.
        for _ in 0..3 {
            menus.key("Tab", none);
        }
        assert_eq!(
            menus.key("Enter", none),
            Some(Outcome::Closed(Some((1, None))))
        );
        menus.popup(&items(), None);
        assert_eq!(menus.key("Escape", none), Some(Outcome::Closed(None)));
        assert_eq!(menus.key("Escape", none), None);
    }

    #[test]
    fn a_press_chooses_an_item_and_outside_closes() {
        let mut menus = Menus {
            pointer: Some(Position::new(0, 0)),
            ..Menus::default()
        };
        menus.popup(&items(), None);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 12));
        draw(&mut menus, &mut buffer);
        // A disabled item takes the press and the menu stays.
        assert_eq!(menus.press(Position::new(3, 2)), Some(Outcome::Taken));
        assert_eq!(
            menus.press(Position::new(3, 1)),
            Some(Outcome::Closed(Some((1, None))))
        );
        menus.popup(&items(), None);
        draw(&mut menus, &mut buffer);
        assert_eq!(
            menus.press(Position::new(30, 10)),
            Some(Outcome::Closed(None))
        );
    }

    #[test]
    fn a_menu_without_room_below_the_pointer_opens_above_it() {
        let mut menus = Menus {
            pointer: Some(Position::new(38, 11)),
            ..Menus::default()
        };
        menus.popup(&items(), None);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 12));
        draw(&mut menus, &mut buffer);
        let rows = rows(&buffer);
        let bottom = rows.iter().rposition(|r| r.contains('╰')).unwrap();
        assert_eq!(bottom, 11, "{rows:#?}");
        assert!(rows[bottom].ends_with('╯'), "{rows:#?}");
    }
}

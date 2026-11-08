//! Trailing iOS navigation-bar buttons (`ios={{ headerRightItems }}`).
//!
//! Rust apps build [`HeaderItem`]s with per-item [`HeaderItem::on_press`]
//! and menus of [`super::desktop::MenuItem`]s (their
//! [`super::desktop::MenuItem::on_select`]). [`register`] turns them into
//! the platform's [`crate::HeaderItem`]s with numeric ids, keeps the
//! handlers in [`super::menus::ViewMenus`], and a cleanup drops them when
//! the binding is replaced or the page goes. The platform reports a
//! chosen id as the root's [`crate::EventKind::Menu`].

use std::{fmt, rc::Rc, sync::Arc};

use super::{cleanup, desktop, menus::MenuHandler, reactive::Owner, view_menus};
use crate::{HeaderItemContent, HeaderSystemItem};

/// A trailing iOS bar button for [`super::host::Screen::header_right_items`]:
/// what it shows, an optional native menu of [`desktop::MenuItem`]s, and
/// handlers that run when the platform reports their ids as the root's
/// `Menu` event.
///
/// ```ignore
/// HeaderItem::system(HeaderSystemItem::Edit).on_press(edit)
/// HeaderItem::symbol("ellipsis.circle")
///     .label("More")
///     .menu([
///         desktop::MenuItem::new("Share").on_select(share),
///         desktop::MenuItem::submenu("Danger", [
///             desktop::MenuItem::new("Delete").destructive(true).on_select(remove),
///         ])
///         .inline(true),
///     ])
/// ```
#[derive(Clone)]
pub struct HeaderItem {
    key: Option<Arc<str>>,
    content: HeaderItemContent,
    label: Option<Arc<str>>,
    disabled: bool,
    on_press: Option<MenuHandler>,
    menu: Option<Vec<desktop::MenuItem>>,
}

impl HeaderItem {
    fn of(content: HeaderItemContent) -> Self {
        Self {
            key: None,
            content,
            label: None,
            disabled: false,
            on_press: None,
            menu: None,
        }
    }

    /// A system-item button.
    pub fn system(item: HeaderSystemItem) -> Self {
        Self::of(HeaderItemContent::SystemItem(item))
    }

    /// An SF Symbol button.
    pub fn symbol(name: impl Into<Arc<str>>) -> Self {
        Self::of(HeaderItemContent::Symbol(name.into()))
    }

    /// A titled button (`label` is what it shows).
    pub fn new(label: impl Into<Arc<str>>) -> Self {
        Self::of(HeaderItemContent::Label(label.into()))
    }

    /// Optional key for keeping a `UIBarButtonItem` across updates.
    pub fn key(mut self, key: impl Into<Arc<str>>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// VoiceOver's label when the content is a system item or symbol.
    pub fn label(mut self, label: impl Into<Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Disables the button.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Runs `f` when the button is pressed and it has no menu.
    pub fn on_press(mut self, f: impl Fn() + 'static) -> Self {
        self.on_press = Some(Rc::new(f));
        self
    }

    /// Opens this native menu on press (the button's [`Self::on_press`] is
    /// unused while a menu is set).
    pub fn menu(mut self, items: impl IntoIterator<Item = desktop::MenuItem>) -> Self {
        self.menu = Some(items.into_iter().collect());
        self
    }
}

impl fmt::Debug for HeaderItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeaderItem")
            .field("key", &self.key)
            .field("content", &self.content)
            .field("label", &self.label)
            .field("disabled", &self.disabled)
            .field("menu", &self.menu.as_ref().map(|m| m.len()))
            .finish_non_exhaustive()
    }
}

/// Decodes `items` into the platform's form, keeps their handlers under a
/// generation that a cleanup drops when this binding is replaced or the
/// owner goes, and returns what [`crate::ScreenConfig::right_items`] holds.
pub(crate) fn register(items: Vec<HeaderItem>) -> Vec<crate::HeaderItem> {
    let menus = view_menus();
    let owner = Owner::current();
    let mut handlers = Vec::new();
    let mac = super::os() == crate::platform::Os::MacOs;
    let decoded = items
        .into_iter()
        .map(|item| decode_item(item, mac, &owner, &mut handlers))
        .collect();
    let generation = menus.borrow_mut().push_header(handlers);
    let weak = Rc::downgrade(&menus);
    cleanup(move || {
        if let Some(menus) = weak.upgrade() {
            menus.borrow_mut().take_header(generation);
        }
    });
    decoded
}

fn decode_item(
    item: HeaderItem,
    mac: bool,
    owner: &Owner,
    handlers: &mut Vec<(u32, MenuHandler)>,
) -> crate::HeaderItem {
    let menus = view_menus();
    let press_id = match item.on_press {
        Some(f) if item.menu.is_none() => {
            let id = menus.borrow_mut().next_id();
            let owner = *owner;
            handlers.push((id, Rc::new(move || owner.run(&*f))));
            id
        }
        _ => 0,
    };
    let menu = item.menu.map(|items| {
        // Reuse the desktop menu decoder so item ids and handlers match.
        desktop::decode_for_header(items, mac, owner, handlers)
    });
    crate::HeaderItem {
        key: item.key,
        content: item.content,
        label: item.label,
        disabled: item.disabled,
        press_id,
        menu,
    }
}

/// Whether two lists describe the same trailing buttons for UIKit's
/// identity (count, keys or contents, and whether each has a menu). Menus'
/// contents, labels and `disabled` can change without rebuilding the bar
/// buttons.
pub fn same_right_buttons(a: &[crate::HeaderItem], b: &[crate::HeaderItem]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| identity(x) == identity(y) && x.menu.is_some() == y.menu.is_some())
}

#[derive(PartialEq, Eq)]
enum Identity<'a> {
    Key(&'a str),
    Content(&'a HeaderItemContent),
}

fn identity(item: &crate::HeaderItem) -> Identity<'_> {
    match item.key.as_deref() {
        Some(key) => Identity::Key(key),
        None => Identity::Content(&item.content),
    }
}

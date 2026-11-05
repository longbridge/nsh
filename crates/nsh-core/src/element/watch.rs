//! Views watching a scroll view they did not make ([`super::watch_scroll`]):
//! a scroll bar of the app's own following a scroll view or a list
//! elsewhere in the tree, as `gpui-base`'s `Scrollbar`
//! follows its scroll handle.
//!
//! The platforms report every scroll view's offset, whoever listens, and the
//! core lays the content out: a scroll watch costs nothing until the core,
//! at the end of a commit round, finds what it watches changed and runs its
//! handler (another round follows, so what the handler changes goes out in
//! the same batch). Targets are resolved again in every commit, so a
//! watch made before its target is mounted (a ref set later in the same
//! render) finds it when it is.
//!
//! Views also watch the order of elements they did not make
//! ([`super::watch_order`]): a toolbar's items, wherever they are in it.
//! Components render in the order the core makes them, not the tree's (an
//! item inside a group renders after the toolbar's later children), so a
//! group that moves the focus between its items asks the core for their
//! order. Such a watch costs nothing until it is invalidated (an item came)
//! or a node moves inside the subtree its elements share (their nearest
//! common ancestor when the core last looked); the core then puts its
//! elements in tree order at the end of the round. A move elsewhere (a
//! keyed list of rows reordered, each row with a group of its own) leaves
//! it alone: nothing outside that subtree orders its elements. The handler
//! hears the order only when it differs from the list's own, unless the
//! watch asked to hear it in any case ([`super::OrderWatch::request`]): a
//! menu opened from the keyboard focuses its first item then, which is
//! already first when it is the only one.
//!
//! Views also watch the keyboard focus come into an element and leave it
//! ([`super::ElementRef::on_focus_in`], `on_focus_out`, GPUI's
//! `on_focus_in` / `on_focus_out` of a focus handle): the core knows
//! which views are in `focus-within`, so such a watch costs nothing until
//! the focus moves, and its handler runs at the end of that commit round.

use std::{cell::Cell, rc::Rc};

use crate::{
    NodeId,
    element::{ElementRef, events::ScrollEvent},
};

/// A scroll watch's handler.
pub(crate) type ScrollHandler = Rc<dyn Fn(&ScrollEvent)>;

/// One view's watch of a node.
pub(crate) struct Watch {
    pub(crate) id: u32,
    /// The node watched, resolved again in every commit.
    pub(crate) target: Rc<dyn Fn() -> Option<NodeId>>,
    /// Hears the target's offset, content size or viewport change.
    pub(crate) handler: ScrollHandler,
    /// What it last reported.
    pub(crate) last: Option<ScrollEvent>,
}

/// An order watch's handler: the order of the elements, as indices into
/// the list it was given.
pub(crate) type OrderHandler = Rc<dyn Fn(&[usize])>;

/// What an order watch asks for its elements when the core looks.
pub(crate) type OrderElements = Rc<dyn Fn() -> Vec<ElementRef>>;

/// One view's watch of the order of elements.
pub(crate) struct Order {
    pub(crate) id: u32,
    /// The elements, asked when the core looks.
    pub(crate) elements: OrderElements,
    pub(crate) handler: OrderHandler,
    /// Whether the core looks at the end of the round, and whether it tells
    /// the handler then.
    pub(crate) flags: Rc<OrderFlags>,
    /// The nearest common ancestor of the elements in the tree when the
    /// core last looked: only a move inside it can reorder them. `None`
    /// before the first look and while none of them is in the tree (any
    /// move makes the core look again).
    pub(crate) container: Rc<Cell<Option<NodeId>>>,
}

/// What an order watch asks of the core's next look, shared with its
/// [`OrderWatch`].
#[derive(Default)]
pub(crate) struct OrderFlags {
    /// The core looks at the end of the round.
    pub(crate) dirty: Cell<bool>,
    /// The handler hears the order then, even when it is the list's own.
    pub(crate) requested: Cell<bool>,
}

/// A focus watch's handler: whether the focus is within its element now.
pub(crate) type FocusHandler = Rc<dyn Fn(bool)>;

/// One view's watch of the focus coming into an element and leaving it.
pub(crate) struct FocusWatch {
    pub(crate) id: u32,
    /// The element watched, resolved again whenever the core looks.
    pub(crate) target: Rc<dyn Fn() -> Option<NodeId>>,
    /// Hears the focus come in (`true`) or leave (`false`).
    pub(crate) handler: FocusHandler,
    /// Whether the focus was within it when the core last looked; `None`
    /// before the core found it, so a watch made while the focus is in its
    /// element hears it leave, not come.
    pub(crate) last: Option<bool>,
}

/// The views' watches, oldest first.
#[derive(Default)]
pub(crate) struct Watches {
    next: u32,
    pub(crate) list: Vec<Watch>,
    pub(crate) orders: Vec<Order>,
    pub(crate) focus: Vec<FocusWatch>,
}

impl Watches {
    /// Adds a watch; its id.
    pub(crate) fn add(
        &mut self,
        target: Rc<dyn Fn() -> Option<NodeId>>,
        handler: ScrollHandler,
    ) -> u32 {
        let id = self.next;
        self.next += 1;
        self.list.push(Watch {
            id,
            target,
            handler,
            last: None,
        });
        id
    }

    /// Removes watch `id`; returns it so it is dropped outside the borrow
    /// (its handler may own what removes others).
    pub(crate) fn remove(&mut self, id: u32) -> Option<Watch> {
        let i = self.list.iter().position(|w| w.id == id)?;
        Some(self.list.remove(i))
    }

    /// Adds an order watch, to look at in this round; its id and its flags.
    pub(crate) fn add_order(
        &mut self,
        elements: Rc<dyn Fn() -> Vec<ElementRef>>,
        handler: OrderHandler,
    ) -> (u32, Rc<OrderFlags>) {
        let id = self.next;
        self.next += 1;
        let flags = Rc::new(OrderFlags {
            dirty: Cell::new(true),
            ..Default::default()
        });
        self.orders.push(Order {
            id,
            elements,
            handler,
            flags: flags.clone(),
            container: Rc::new(Cell::new(None)),
        });
        (id, flags)
    }

    /// Removes order watch `id`; returns it so it is dropped outside the
    /// borrow.
    pub(crate) fn remove_order(&mut self, id: u32) -> Option<Order> {
        let i = self.orders.iter().position(|w| w.id == id)?;
        Some(self.orders.remove(i))
    }

    /// Whether an order watch waits to be looked at.
    pub(crate) fn orders_dirty(&self) -> bool {
        self.orders.iter().any(|o| o.flags.dirty.get())
    }

    /// Adds a focus watch; its id.
    pub(crate) fn add_focus(
        &mut self,
        target: Rc<dyn Fn() -> Option<NodeId>>,
        handler: FocusHandler,
    ) -> u32 {
        let id = self.next;
        self.next += 1;
        self.focus.push(FocusWatch {
            id,
            target,
            handler,
            last: None,
        });
        id
    }

    /// Removes focus watch `id`; returns it so it is dropped outside the
    /// borrow.
    pub(crate) fn remove_focus(&mut self, id: u32) -> Option<FocusWatch> {
        let i = self.focus.iter().position(|w| w.id == id)?;
        Some(self.focus.remove(i))
    }

    /// Whether a focus watch has not found its element yet.
    pub(crate) fn focus_unresolved(&self) -> bool {
        self.focus.iter().any(|w| w.last.is_none())
    }
}

/// A watch of the order of elements ([`super::watch_order`]): tells the
/// core to look at it again.
#[doc(hidden)]
#[derive(Clone)]
pub struct OrderWatch {
    flags: Rc<OrderFlags>,
}

impl OrderWatch {
    pub(crate) fn new(flags: Rc<OrderFlags>) -> Self {
        Self { flags }
    }

    /// The elements changed (one came): the core puts them in tree order at
    /// the end of this commit round, and the handler hears it in the same
    /// batch when it differs from the list's own.
    pub fn invalidate(&self) {
        self.flags.dirty.set(true);
    }

    /// As [`Self::invalidate`], and the handler hears the order even when
    /// it is the list's own: for a watch that acts on its elements in
    /// order once they are there rather than only sorting them (a menu
    /// opened from the keyboard focusing its first item, which is first
    /// already when it is the only one).
    pub fn request(&self) {
        self.flags.requested.set(true);
        self.flags.dirty.set(true);
    }
}

impl std::fmt::Debug for OrderWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderWatch")
            .field("dirty", &self.flags.dirty.get())
            .field("requested", &self.flags.requested.get())
            .finish()
    }
}

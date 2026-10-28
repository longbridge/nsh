//! Views watching an element they did not make ([`super::watch_scroll`],
//! [`super::watch_hover`]): a scroll bar of the app's own following a
//! scroll view or a list elsewhere in the tree, as `gpui-base`'s `Scrollbar`
//! follows its scroll handle.
//!
//! The platforms report every scroll view's offset, whoever listens, and the
//! core lays the content out: a scroll watch costs nothing until the core,
//! at the end of a commit round, finds what it watches changed and runs its
//! handler (another round follows, so what the handler changes goes out in
//! the same batch). A hover watch has the watched node listen to the mouse
//! entering and leaving it, as an overlay's hint has its anchor
//! (`Node::hover_watched`). Targets are resolved again in every commit, so a
//! watch made before its target is mounted (a ref set later in the same
//! render) finds it when it is.

use std::rc::Rc;

use crate::{NodeId, element::events::ScrollEvent};

/// A scroll watch's handler.
pub(crate) type ScrollHandler = Rc<dyn Fn(&ScrollEvent)>;

/// What a watch hears.
pub(crate) enum WatchHandler {
    /// The target's offset, content size or viewport changed.
    Scroll(ScrollHandler),
    /// The mouse entered (`true`) or left the target.
    Hover(Rc<dyn Fn(bool)>),
}

/// One view's watch of a node.
pub(crate) struct Watch {
    pub(crate) id: u32,
    /// The node watched, resolved again in every commit.
    pub(crate) target: Rc<dyn Fn() -> Option<NodeId>>,
    pub(crate) handler: WatchHandler,
    /// The node it watched when the core last resolved it (0: none).
    pub(crate) node: NodeId,
    /// What a scroll watch last reported.
    pub(crate) last: Option<ScrollEvent>,
}

/// The views' watches, oldest first.
#[derive(Default)]
pub(crate) struct Watches {
    next: u32,
    pub(crate) list: Vec<Watch>,
}

impl Watches {
    /// Adds a watch; its id.
    pub(crate) fn add(
        &mut self,
        target: Rc<dyn Fn() -> Option<NodeId>>,
        handler: WatchHandler,
    ) -> u32 {
        let id = self.next;
        self.next += 1;
        self.list.push(Watch {
            id,
            target,
            handler,
            node: 0,
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
}

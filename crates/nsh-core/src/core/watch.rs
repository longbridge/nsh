//! The views' watches of nodes they did not make (`element::watch`): their
//! targets resolved, scroll changes delivered and hover listening kept in
//! step, once per commit round.

use std::rc::Rc;

use super::{Core, Key, dirty};
use crate::{
    EventKind, NodeId, TextEngine,
    element::{
        ScrollEvent, caught, untracked,
        watch::{ScrollHandler, WatchHandler},
    },
};

/// A watch as one round sees it, taken out of the executor's list so its
/// target and handler run with no borrow held.
struct Seen {
    id: u32,
    target: Rc<dyn Fn() -> Option<NodeId>>,
    scroll: Option<ScrollHandler>,
    last: Option<ScrollEvent>,
}

impl<M: TextEngine> Core<M> {
    /// Resolves the watches' targets, runs the scroll watches whose target
    /// moved, was laid out again or was found, and has the hover watches'
    /// targets listen to the pointer; whether a handler ran (another round
    /// sends what it changed).
    pub(super) fn sync_watches(&mut self) -> bool {
        let watches = self.components.executor.watches.clone();
        let seen: Vec<Seen> = watches
            .borrow()
            .list
            .iter()
            .map(|w| Seen {
                id: w.id,
                target: w.target.clone(),
                scroll: match &w.handler {
                    WatchHandler::Scroll(f) => Some(f.clone()),
                    WatchHandler::Hover(_) => None,
                },
                last: w.last,
            })
            .collect();
        if seen.is_empty() && self.hover_watched.is_empty() {
            return false;
        }
        let _entered = self.components.runtime.enter();
        let mut ran = false;
        let mut hovered = Vec::new();
        for w in seen {
            let node = untracked(|| (w.target)()).unwrap_or(0);
            let Some(handler) = w.scroll else {
                if let Ok(key) = self.key(node) {
                    hovered.push((key, node));
                }
                Self::note_watch(&watches, w.id, node, None);
                continue;
            };
            let now = self.scroll_report(node);
            if now == w.last {
                continue;
            }
            Self::note_watch(&watches, w.id, node, now);
            if let Some(event) = now {
                if let Err(message) = caught(|| handler(&event)) {
                    log::error!(target: "js", "a scroll watch failed: {message}");
                }
                self.apply_actions();
                ran = true;
            }
        }
        self.rewatch_hover(hovered);
        ran
    }

    /// Records what watch `id` resolved and reported, if it is still there.
    fn note_watch(
        watches: &std::cell::RefCell<crate::element::watch::Watches>,
        id: u32,
        node: NodeId,
        last: Option<ScrollEvent>,
    ) {
        if let Some(w) = watches.borrow_mut().list.iter_mut().find(|w| w.id == id) {
            w.node = node;
            w.last = last;
        }
    }

    /// What `on_scroll` would report of scroll view (or list) `id` now;
    /// none for a node that is not one, or not made yet.
    fn scroll_report(&self, id: NodeId) -> Option<ScrollEvent> {
        let node = &self.nodes[self.key(id).ok()? as usize];
        let scroll = node.scroll.as_ref()?;
        if !node.created {
            return None;
        }
        let frame = node.frame.size;
        let content = scroll.content.unwrap_or(frame);
        let (x, y) = scroll.offset;
        Some(ScrollEvent {
            offset: crate::geometry::point(crate::geometry::px(x), crate::geometry::px(y)),
            content_size: content * scroll.zoom,
            size: frame,
            zoom_scale: scroll.zoom,
        })
    }

    /// The nodes hover watches name now listen to the pointer; those no
    /// watch names any more stop.
    fn rewatch_hover(&mut self, mut new: Vec<(Key, NodeId)>) {
        new.sort_unstable();
        new.dedup();
        if new == self.hover_watched {
            return;
        }
        let old = std::mem::replace(&mut self.hover_watched, new);
        for &(k, id) in &old {
            if !self.hover_watched.contains(&(k, id))
                && self.nodes.get(k as usize).is_some_and(|n| n.id == id)
            {
                self.nodes[k as usize].hover_watched = false;
                self.mark(k, dirty::EVENTS);
            }
        }
        for i in 0..self.hover_watched.len() {
            let (k, _) = self.hover_watched[i];
            if !self.nodes[k as usize].hover_watched {
                self.nodes[k as usize].hover_watched = true;
                self.mark(k, dirty::EVENTS);
            }
        }
    }

    /// The pointer entered or left node `id`: the hover watches naming it
    /// hear it.
    pub(super) fn watch_hover_event(&mut self, id: NodeId, kind: EventKind) {
        let Ok(key) = self.key(id) else { return };
        if !self.nodes[key as usize].hover_watched {
            return;
        }
        let inside = kind == EventKind::MouseEnter;
        let handlers: Vec<Rc<dyn Fn(bool)>> = self
            .components
            .executor
            .watches
            .borrow()
            .list
            .iter()
            .filter(|w| w.node == id)
            .filter_map(|w| match &w.handler {
                WatchHandler::Hover(f) => Some(f.clone()),
                WatchHandler::Scroll(_) => None,
            })
            .collect();
        for handler in handlers {
            if let Err(message) = caught(|| handler(inside)) {
                log::error!(target: "js", "a hover watch failed: {message}");
            }
            self.apply_actions();
        }
    }
}

//! Overlays ([`crate::overlay`]): an `<overlay>` node stays where the app
//! put it (its parent's child for the reactive graph, events, lifetime,
//! inherited text styles), while its native view is in the root view
//! around it (the surface, or the nearest modal), above that root view's
//! own children and the overlays shown before it.
//!
//! Each commit [`Core::sync_overlays`] works out which overlays are shown
//! (every view around them is, and the overlays around them are) and in
//! which root view, and moves their native views there: one opened later
//! goes on top. An overlay is a layout root of its own, laid out in a box
//! the size of its root view (`position: fixed`), and placed against its
//! anchor when it has one ([`crate::overlay::place`]); it follows the
//! anchor as it moves ([`Core::place_overlays`], after the lists and
//! scroll offsets of the commit). The core also routes what dismissing
//! takes: presses outside (`onPressOutside`: the overlays watching them,
//! their anchors and root views listen to touches while they are shown)
//! and Escape / the back button (`onCancel`, to the top overlay listening:
//! a binding of the root's table and the root's back listener while one
//! is shown). Closing is the app's (`nsh-base`'s components).

use taffy::NodeId as LayoutId;

use super::{Core, Key, Kind, NO_KEY, definite};
use crate::{
    EventKind, EventPayload, NodeId, PayloadValue, ROOT_ID, TextEngine, TouchEvent, TouchPhase,
    geometry::{Bounds, Point, Points, Size, rect},
    keymap::{KeyBinding, Keystroke, When},
    mount::MountItem,
    overlay::{OverlayAnchor, OverlayConfig, Placed, inset, place},
};

/// The binding id of Escape while an overlay listens to `onCancel`: above
/// any JS gives and any view's ([`crate::ui::keys::FIRST_ID`] counts up).
pub(super) const ESCAPE_BINDING: u32 = u32::MAX;

/// An overlay's own state.
#[derive(Debug)]
pub(super) struct OverlayState {
    /// How it is placed.
    pub(super) config: OverlayConfig,
    /// A layout node the size of the root view, holding the overlay's as
    /// an absolutely placed child: what its insets and percentages are
    /// against.
    pub(super) wrapper: LayoutId,
    /// The root view its native view is in, while it is shown.
    pub(super) host: Option<NodeId>,
    /// Where it was placed last, as `onPosition` reported it.
    pub(super) placed: Option<(Placed, Bounds<Points>)>,
}

/// What the overlays add to the native side: the nodes listening to
/// touches for `onPressOutside`, and whether Escape and back are taken.
#[derive(Debug, Default)]
pub(super) struct Overlays {
    /// The shown overlays and the other nodes listening to touches for
    /// them (their anchors and root views), checked by id (keys are
    /// reused).
    pub(super) watched: Vec<(Key, NodeId)>,
    /// A shown overlay listens to `onCancel`: Escape is bound and the back
    /// button listened to.
    pub(super) cancel: bool,
}

impl<M: TextEngine> Core<M> {
    /// A new overlay node `key` with layout node `layout`: its wrapper.
    pub(super) fn overlay_made(&mut self, key: Key, layout: LayoutId) {
        let wrapper = self
            .layout
            .new_with_children(taffy::Style::default(), &[layout])
            .expect("taffy wrapper");
        if let Some(root) = self.nodes[key as usize].root.as_mut() {
            root.overlay = Some(Box::new(OverlayState {
                config: OverlayConfig::default(),
                wrapper,
                host: None,
                placed: None,
            }));
        }
        self.overlays.push(key);
    }

    /// Overlay `key` is released: forgets it and drops its wrapper (its own
    /// layout node is gone already).
    pub(super) fn overlay_released(&mut self, key: Key, wrapper: LayoutId) {
        self.overlays.retain(|&k| k != key);
        let _ = self.layout.remove(wrapper);
    }

    pub(super) fn overlay(&self, key: Key) -> Option<&OverlayState> {
        self.nodes[key as usize]
            .root
            .as_ref()
            .and_then(|r| r.overlay.as_deref())
    }

    fn overlay_mut(&mut self, key: Key) -> Option<&mut OverlayState> {
        self.nodes[key as usize]
            .root
            .as_mut()
            .and_then(|r| r.overlay.as_deref_mut())
    }

    /// Changes how overlay `key` is placed.
    pub(super) fn set_overlay_config(&mut self, key: Key, f: impl FnOnce(&mut OverlayConfig)) {
        if let Some(state) = self.overlay_mut(key) {
            let before = state.config;
            f(&mut state.config);
            if state.config != before {
                state.placed = None;
                // Placed again with its size, or laid out again (no anchor:
                // its insets place it).
                if let Some(root) = self.nodes[key as usize].root.as_mut() {
                    root.laid_out = None;
                }
                self.needs_layout = true;
            }
        }
    }

    /// The first overlay shown in root view `parent`: the native sibling
    /// its own children go below.
    pub(super) fn first_overlay_in(&self, parent: Key) -> Option<NodeId> {
        let id = self.nodes[parent as usize].id;
        self.overlays
            .iter()
            .find_map(|&k| (self.overlay(k)?.host == Some(id)).then(|| self.nodes[k as usize].id))
    }

    /// The root view overlay `key` is shown in, or `None` while it is not
    /// shown: a view around it is not (out of the tree, its native view
    /// not made, `display: none`), or the overlay around it is not.
    fn overlay_host(&self, key: Key) -> Option<Key> {
        let mut modal = None;
        let mut k = self.nodes[key as usize].parent;
        while k != NO_KEY {
            if k == self.root {
                return Some(modal.unwrap_or(self.root));
            }
            let n = &self.nodes[k as usize];
            if let Some(layout) = n.layout {
                if !n.created {
                    return None;
                }
                let hidden = self
                    .layout
                    .style(layout)
                    .is_ok_and(|s| s.display == taffy::Display::None);
                if hidden && n.kind != Kind::Overlay {
                    return None;
                }
            }
            match n.kind {
                Kind::Modal if modal.is_none() => modal = Some(k),
                Kind::Overlay => {
                    // Shown in its root view, which is this one's unless a
                    // modal is between them.
                    let host = self.overlay(k)?.host?;
                    return Some(modal.unwrap_or(self.key(host).ok()?));
                }
                _ => {}
            }
            k = n.parent;
        }
        None
    }

    /// The overlays around `key`.
    fn overlay_depth(&self, key: Key) -> u32 {
        let mut depth = 0;
        let mut k = self.nodes[key as usize].parent;
        while k != NO_KEY {
            if self.nodes[k as usize].kind == Kind::Overlay {
                depth += 1;
            }
            k = self.nodes[k as usize].parent;
        }
        depth
    }

    /// Shows the overlays whose views around them are shown, in their root
    /// views, and hides the others; then what they listen to (once per
    /// commit, before the dirty views are flushed).
    pub(super) fn sync_overlays(&mut self) {
        if self.overlays.is_empty() && self.overlay_state.watched.is_empty() {
            return;
        }
        // The overlays around one first: theirs say where it goes.
        let mut order: Vec<(u32, Key)> = self
            .overlays
            .iter()
            .map(|&k| (self.overlay_depth(k), k))
            .collect();
        order.sort_by_key(|&(depth, _)| depth);
        let mut opened = Vec::new();
        for (_, key) in order {
            let target = self.overlay_host(key);
            let target_id = target.map(|h| self.nodes[h as usize].id);
            let Some(state) = self.overlay(key) else {
                continue;
            };
            let current = state.host;
            if current == target_id {
                continue;
            }
            let id = self.nodes[key as usize].id;
            if let Some(old) = current
                && self.key(old).is_ok_and(|h| self.nodes[h as usize].created)
                && self.nodes[key as usize].created
            {
                self.mount.push(MountItem::Remove { id });
            }
            let state = self.overlay_mut(key).expect("overlay");
            state.host = target_id;
            state.placed = None;
            let Some(host) = target_id else {
                continue;
            };
            self.ensure_created(key);
            self.mount.push(MountItem::Insert {
                parent: host,
                child: id,
                before: None,
            });
            if let Some(root) = self.nodes[key as usize].root.as_mut() {
                root.laid_out = None;
            }
            self.needs_layout = true;
            opened.push(key);
        }
        // Opened last, on top: in the order they were shown.
        if !opened.is_empty() {
            self.overlays.retain(|k| !opened.contains(k));
            self.overlays.extend(opened);
        }
        self.sync_overlay_listeners();
    }

    /// The touches and keys the shown overlays need.
    fn sync_overlay_listeners(&mut self) {
        let mut watched: Vec<(Key, NodeId)> = Vec::new();
        let mut cancel = false;
        let watching = self.overlays.iter().any(|&k| {
            self.overlay(k).is_some_and(|s| s.host.is_some())
                && self.nodes[k as usize]
                    .events
                    .contains(EventKind::PressOutside)
        });
        for &key in &self.overlays {
            let Some(state) = self.overlay(key) else {
                continue;
            };
            let Some(host) = state.host else { continue };
            let node = &self.nodes[key as usize];
            cancel |= node.events.contains(EventKind::Cancel);
            if !watching {
                continue;
            }
            // Every shown overlay, so a press in one inside a watching one
            // tells where it is.
            watched.push((key, node.id));
            if node.events.contains(EventKind::PressOutside) {
                for id in [Some(host), anchor_node(state)].into_iter().flatten() {
                    if let Ok(k) = self.key(id) {
                        watched.push((k, self.nodes[k as usize].id));
                    }
                }
            }
        }
        watched.sort_unstable();
        watched.dedup();
        let old = std::mem::take(&mut self.overlay_state.watched);
        for &(k, id) in &old {
            if !watched.contains(&(k, id)) && self.nodes.get(k as usize).is_some_and(|n| n.id == id)
            {
                self.nodes[k as usize].overlay_watched = false;
                self.mark(k, super::dirty::EVENTS);
            }
        }
        for &(k, id) in &watched {
            if !old.contains(&(k, id)) {
                self.nodes[k as usize].overlay_watched = true;
                self.mark(k, super::dirty::EVENTS);
            }
        }
        self.overlay_state.watched = watched;
        if cancel != self.overlay_state.cancel {
            self.overlay_state.cancel = cancel;
            // The root's table and back listener are made again.
            self.components.executor.keys.borrow_mut().changed = true;
            self.components.executor.back.borrow_mut().changed = true;
        }
    }

    /// Escape's binding, while an overlay listens to `onCancel`.
    pub(super) fn overlay_key_binding(&self) -> Option<KeyBinding> {
        if !self.overlay_state.cancel {
            return None;
        }
        let escape = Keystroke::parse_with("escape", false).ok()?;
        Some(KeyBinding::new(ESCAPE_BINDING, escape, When::Always, false))
    }

    /// Escape or back: `onCancel` of the top overlay listening to it;
    /// whether one took it.
    pub(super) fn overlay_cancel(&mut self) -> bool {
        let top = self.overlays.iter().rev().copied().find(|&k| {
            self.overlay(k).is_some_and(|s| s.host.is_some())
                && self.nodes[k as usize].events.contains(EventKind::Cancel)
        });
        let Some(key) = top else {
            return false;
        };
        let id = self.nodes[key as usize].id;
        self.events.push(crate::ui::JsEvent::Event(
            id,
            EventKind::Cancel,
            EventPayload::None,
        ));
        true
    }

    /// A touch on the surface: `onPressOutside` of the shown overlays
    /// watching presses that a finger went down outside of.
    pub(super) fn overlay_touches(&mut self, touch: &TouchEvent) {
        if touch.phase != TouchPhase::Start || self.overlay_state.watched.is_empty() {
            return;
        }
        for point in touch.points.iter().filter(|p| p.changed) {
            for i in 0..self.overlays.len() {
                let key = self.overlays[i];
                let node = &self.nodes[key as usize];
                let Some(state) = self.overlay(key) else {
                    continue;
                };
                if state.host.is_none() || !node.events.contains(EventKind::PressOutside) {
                    continue;
                }
                let id = node.id;
                let anchor = anchor_node(state);
                let inside = point.path.iter().any(|&p| {
                    p == id
                        || Some(p) == anchor
                        || self.key(p).is_ok_and(|k| {
                            self.nodes[k as usize].kind == Kind::Overlay && self.is_ancestor(key, k)
                        })
                });
                if inside {
                    continue;
                }
                self.events.push(crate::ui::JsEvent::Event(
                    id,
                    EventKind::PressOutside,
                    EventPayload::Map(vec![
                        ("pageX", PayloadValue::Number(point.page.x.0 as f64)),
                        ("pageY", PayloadValue::Number(point.page.y.0 as f64)),
                    ]),
                ));
            }
        }
    }

    /// The size of the root view overlay `key` is shown in.
    fn overlay_host_size(&self, host: NodeId) -> Option<(Size<Points>, crate::EdgeInsets)> {
        if host == ROOT_ID {
            return Some((self.viewport, self.safe_area));
        }
        let root = self.nodes[self.key(host).ok()? as usize].root.as_ref()?;
        Some((root.reported.unwrap_or(self.viewport), root.insets))
    }

    /// Lays out shown overlay `key` in a box the size of its root view,
    /// when that size or its content changed, and places it.
    pub(super) fn layout_overlay(&mut self, key: Key) {
        let Some(state) = self.overlay(key) else {
            return;
        };
        let (Some(host), wrapper) = (state.host, state.wrapper) else {
            return;
        };
        let Some((size, _)) = self.overlay_host_size(host) else {
            return;
        };
        let root = self.nodes[key as usize].root.as_ref().expect("root state");
        let changed = root.laid_out != Some((size, 0.0));
        if !changed && !self.layout.dirty(wrapper).unwrap_or(true) {
            return;
        }
        if changed {
            let style = taffy::Style {
                size: taffy::Size {
                    width: taffy::Dimension::length(size.width.0),
                    height: taffy::Dimension::length(size.height.0),
                },
                ..taffy::Style::default()
            };
            self.layout.set_style(wrapper, style).expect("taffy style");
        }
        self.nodes[key as usize]
            .root
            .as_mut()
            .expect("root state")
            .laid_out = Some((size, 0.0));
        self.layout_root(wrapper, definite(size));
        self.emit_frames(key);
    }

    /// Where overlay `key` of `frame` (as laid out in its root view) goes:
    /// against its anchor when it has one it can find. Reports a new
    /// placement to `onPosition`.
    pub(super) fn overlay_origin(&mut self, key: Key, frame: Bounds<Points>) -> Point<Points> {
        let Some(state) = self.overlay(key) else {
            return frame.origin;
        };
        let (config, Some(host)) = (state.config, state.host) else {
            return frame.origin;
        };
        let anchor = match config.anchor {
            OverlayAnchor::None => None,
            OverlayAnchor::Point(p) => Some(Bounds {
                origin: p,
                size: Size::default(),
            }),
            OverlayAnchor::Node(id) => self.measure(id).map(|m| Bounds {
                origin: m.page,
                size: m.frame.size,
            }),
            OverlayAnchor::Content(id) => self.content_bounds(id),
        };
        let Some(anchor) = anchor else {
            return frame.origin;
        };
        let Some((size, insets)) = self.overlay_host_size(host) else {
            return frame.origin;
        };
        let m = config.margin.0;
        let area = inset(
            rect(0.0, 0.0, size.width.0, size.height.0),
            [
                insets.top.0 + m,
                insets.right.0 + m,
                insets.bottom.0 + m,
                insets.left.0 + m,
            ],
        );
        let placed = place(
            anchor,
            frame.size,
            area,
            &config,
            self.nodes[key as usize].rtl,
        );
        let bounds = Bounds {
            origin: placed.origin,
            size: frame.size,
        };
        let node = &self.nodes[key as usize];
        let (id, listens) = (node.id, node.events.contains(EventKind::Position));
        let state = self.overlay_mut(key).expect("overlay");
        if state.placed != Some((placed, bounds)) {
            state.placed = Some((placed, bounds));
            if listens {
                let n = |v: Points| PayloadValue::Number(v.0 as f64);
                self.events.push(crate::ui::JsEvent::Event(
                    id,
                    EventKind::Position,
                    EventPayload::Map(vec![
                        (
                            "placement",
                            PayloadValue::Str(placed.placement.name().into()),
                        ),
                        ("x", n(bounds.origin.x)),
                        ("y", n(bounds.origin.y)),
                        ("width", n(bounds.size.width)),
                        ("height", n(bounds.size.height)),
                        ("availableWidth", n(placed.available.width)),
                        ("availableHeight", n(placed.available.height)),
                    ]),
                ));
            }
        }
        placed.origin
    }

    /// The box around what view `id` draws in its root view: around its
    /// children's frames, or its own while it has none (laid out ones only).
    fn content_bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
        let own = self.measure(id)?;
        let key = self.key(id).ok()?;
        let mut out: Option<(f32, f32, f32, f32)> = None;
        for &c in &self.nodes[key as usize].children {
            let n = &self.nodes[c as usize];
            if n.kind == Kind::Overlay || n.layout.is_none() {
                continue;
            }
            let Some(m) = self.measure(n.id) else {
                continue;
            };
            let (x0, y0) = (m.page.x.0, m.page.y.0);
            let (x1, y1) = (x0 + m.frame.size.width.0, y0 + m.frame.size.height.0);
            out = Some(match out {
                None => (x0, y0, x1, y1),
                Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
            });
        }
        Some(match out {
            Some((x0, y0, x1, y1)) => rect(x0, y0, x1 - x0, y1 - y0),
            None => Bounds {
                origin: own.page,
                size: own.frame.size,
            },
        })
    }

    /// Places the shown overlays with an anchor again, at the end of the
    /// commit: their anchors may have moved (the layout, the lists,
    /// scrolling).
    pub(super) fn place_overlays(&mut self) {
        for i in 0..self.overlays.len() {
            let key = self.overlays[i];
            let anchored = self
                .overlay(key)
                .is_some_and(|s| s.host.is_some() && s.config.anchor != OverlayAnchor::None);
            let frame = self.nodes[key as usize].frame;
            if !anchored || !self.nodes[key as usize].created || frame.size.width.0.is_nan() {
                continue;
            }
            let origin = self.overlay_origin(key, frame);
            if origin != frame.origin {
                self.set_frame(
                    key,
                    Bounds {
                        origin,
                        size: frame.size,
                    },
                );
            }
        }
    }
}

/// The view an overlay is anchored to.
fn anchor_node(state: &OverlayState) -> Option<NodeId> {
    match state.config.anchor {
        OverlayAnchor::Node(id) | OverlayAnchor::Content(id) => Some(id),
        _ => None,
    }
}

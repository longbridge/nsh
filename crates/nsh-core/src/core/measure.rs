//! Where views are: measuring a node for `ref.measure()`, scrolling one
//! into view (`ref.scrollIntoView()`), and the areas descendants cover
//! outside their parents' bounds, which platforms hit-test
//! ([`MountItem::Overflow`]).

use super::{Core, Key, Kind, NO_KEY};
use crate::{
    EventPayload, JsonValue, PayloadValue,
    geometry::{Bounds, Point, Points, point, rect},
    mount::MountItem,
    ops::{Command, NodeId, ScrollAlignment, ScrollIntoView},
    text::TextEngine,
};

/// The offset that places the range `start..start + len` of a scroll
/// view's content in its `viewport` as `align` says, from `offset`.
fn aligned(start: f32, len: f32, viewport: f32, offset: f32, align: ScrollAlignment) -> f32 {
    match align {
        ScrollAlignment::Start => start,
        ScrollAlignment::Center => start + (len - viewport) / 2.0,
        ScrollAlignment::End => start + len - viewport,
        // CSSOM View's "determine the scroll-into-view position".
        ScrollAlignment::Nearest => {
            let before = start < offset;
            let after = start + len > offset + viewport;
            if before == after {
                offset
            } else if before == (len <= viewport) {
                start
            } else {
                start + len - viewport
            }
        }
    }
}

/// Where a view is, from the last layout (`ref.measure()`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    /// The frame relative to the parent view, as `onLayout` reports it.
    pub frame: Bounds<Points>,
    /// The top left corner in the coordinates of the root view the view is
    /// in (the surface, or the modal it is shown in): those of touches'
    /// `pageX` / `pageY` (`pagePosition`). Scroll offsets and zoom scales of the scroll
    /// views and lists around it are applied; transforms are not.
    pub page: Point<Points>,
}

impl Measurement {
    /// The JS object `ref.measure()` resolves to.
    pub(crate) fn to_json(self) -> JsonValue {
        serde_json::json!({
            "bounds": {
                "origin": { "x": self.frame.origin.x.0, "y": self.frame.origin.y.0 },
                "size": { "width": self.frame.size.width.0, "height": self.frame.size.height.0 },
            },
            "pagePosition": { "x": self.page.x.0, "y": self.page.y.0 },
        })
    }
}

/// Descendants reaching less than this far out of a view do not count
/// (rounding).
const OVERFLOW_EPSILON: f32 = 0.5;

fn valid(r: &Bounds<Points>) -> bool {
    !(r.origin.x.0.is_nan()
        || r.origin.y.0.is_nan()
        || r.size.width.0.is_nan()
        || r.size.height.0.is_nan())
}

impl<M: TextEngine> Core<M> {
    /// Where view `id` is, from the last layout; `None` when it is not in
    /// the tree or was never laid out.
    pub fn measure(&self, id: NodeId) -> Option<Measurement> {
        let key = self.key(id).ok()?;
        let node = &self.nodes[key as usize];
        if !(node.created || node.culled) || node.layout.is_none() || !valid(&node.frame) {
            return None;
        }
        // Detached (not in the tree): no position.
        if key != self.root && !self.in_tree(key) {
            return None;
        }
        let (mut x, mut y) = (0.0f32, 0.0f32);
        let mut k = key;
        while k != NO_KEY {
            let n = &self.nodes[k as usize];
            // Modals (and the views UIKit places) are root views of their
            // own: positions are in them. A screen fills its stack.
            if matches!(
                n.kind,
                Kind::Modal | Kind::HeaderItem | Kind::InputAccessory
            ) {
                break;
            }
            // Spans and raw text have no view; text attachments are placed
            // in their text block.
            if n.layout.is_some() && valid(&n.frame) {
                x += n.frame.origin.x.0;
                y += n.frame.origin.y.0;
            }
            // An overlay is placed in its root view.
            if n.is_overlay() {
                break;
            }
            let parent = n.parent;
            if parent == NO_KEY {
                break;
            }
            // In a scroll view's content: where the content is now.
            if let Some(scroll) = self.nodes[parent as usize].scroll.as_deref() {
                x = x * scroll.zoom - scroll.offset.0;
                y = y * scroll.zoom - scroll.offset.1;
            }
            k = parent;
        }
        Some(Measurement {
            frame: node.frame,
            page: point(Points(x), Points(y)),
        })
    }

    /// The `ScrollTo` commands that bring view `id` into view, as
    /// `options` places it: one for each scroll view around it that has to
    /// move, innermost first, each from where the ones inside it end up.
    /// Lists and the root view's own scrolling are left alone. None when
    /// the view is not laid out. A scroll view in `projected` (the offsets
    /// earlier commands of the commit scroll it to, the last one last) is
    /// taken to be there.
    pub(super) fn scrolls_into_view(
        &self,
        id: NodeId,
        options: ScrollIntoView,
        projected: &[(NodeId, (f32, f32))],
    ) -> Vec<(NodeId, Command)> {
        let mut scrolls = Vec::new();
        if self.measure(id).is_none() {
            log::warn!("scrollIntoView: view {id} is not laid out");
            return scrolls;
        }
        let Ok(mut k) = self.key(id) else {
            return scrolls;
        };
        let size = self.nodes[k as usize].frame.size;
        let (mut x, mut y) = (0.0f32, 0.0f32);
        let (mut width, mut height) = (size.width.0, size.height.0);
        while k != NO_KEY {
            let n = &self.nodes[k as usize];
            if matches!(
                n.kind,
                Kind::Modal | Kind::HeaderItem | Kind::InputAccessory
            ) {
                break;
            }
            if n.layout.is_some() && valid(&n.frame) {
                x += n.frame.origin.x.0;
                y += n.frame.origin.y.0;
            }
            if n.is_overlay() || n.parent == NO_KEY {
                break;
            }
            let parent = &self.nodes[n.parent as usize];
            if let Some(scroll) = parent.scroll.as_deref() {
                (x, y) = (x * scroll.zoom, y * scroll.zoom);
                (width, height) = (width * scroll.zoom, height * scroll.zoom);
                let mut offset = scroll.offset;
                // A culled scroll view is made for its command.
                if parent.kind == Kind::Scroll && valid(&parent.frame) {
                    let viewport = parent.frame.size;
                    let most = |content: Option<Points>, viewport: Points| {
                        content.map_or(f32::INFINITY, |c| (c - viewport).0.max(0.0))
                    };
                    let most_x = most(scroll.content.map(|c| c.width), viewport.width);
                    let most_y = most(scroll.content.map(|c| c.height), viewport.height);
                    if let Some(&(_, (px, py))) = projected.iter().rfind(|(s, _)| *s == parent.id) {
                        offset = (px.clamp(0.0, most_x), py.clamp(0.0, most_y));
                    }
                    let to = if scroll.horizontal {
                        let to = aligned(x, width, viewport.width.0, offset.0, options.inline);
                        (to.clamp(0.0, most_x), offset.1)
                    } else {
                        let to = aligned(y, height, viewport.height.0, offset.1, options.block);
                        (offset.0, to.clamp(0.0, most_y))
                    };
                    if to != offset {
                        scrolls.push((
                            parent.id,
                            Command::ScrollTo {
                                x: Points(to.0),
                                y: Points(to.1),
                                animated: options.animated,
                            },
                        ));
                        offset = to;
                    }
                }
                (x, y) = (x - offset.0, y - offset.1);
            }
            k = n.parent;
        }
        scrolls
    }

    /// The offset `command` scrolls scroll view `id` to, when it does.
    pub(super) fn scrolled_to(&self, id: NodeId, command: &Command) -> Option<(f32, f32)> {
        let node = &self.nodes[self.key(id).ok()? as usize];
        if node.kind != Kind::Scroll {
            return None;
        }
        let scroll = node.scroll.as_deref()?;
        match *command {
            Command::ScrollTo { x, y, .. } => Some((x.0, y.0)),
            Command::ScrollToEnd { .. } => {
                let content = scroll.content?;
                let viewport = node.frame.size;
                Some(if scroll.horizontal {
                    ((content.width - viewport.width).0.max(0.0), 0.0)
                } else {
                    (0.0, (content.height - viewport.height).0.max(0.0))
                })
            }
            _ => None,
        }
    }

    /// Completes an input's `onCaretMove` payload from its platform: adds
    /// `pageBounds`, its `bounds` (in the input) moved by where the input is
    /// in its root view. The platforms know the caret in their text views,
    /// the core where the input is (`ref.measure()`'s `pagePosition`, with
    /// the scroll views around it).
    pub fn locate_caret(&self, id: NodeId, payload: &mut EventPayload) {
        if matches!(payload, EventPayload::None) {
            *payload = EventPayload::Map(Vec::new());
        }
        let EventPayload::Map(fields) = payload else {
            return;
        };
        if fields.iter().any(|(k, _)| *k == "pageBounds") {
            return;
        }
        // A platform that could not place the caret reports an empty one.
        let caret = match fields.iter().find(|(k, _)| *k == "bounds") {
            Some(&(_, PayloadValue::Bounds(caret))) => caret,
            _ => {
                fields.retain(|(k, _)| *k != "bounds");
                fields.push(("bounds", PayloadValue::Bounds(Bounds::default())));
                Bounds::default()
            }
        };
        let page = self.measure(id).map_or(Point::default(), |m| m.page);
        fields.push((
            "pageBounds",
            PayloadValue::Bounds(Bounds {
                origin: point(page.x + caret.origin.x, page.y + caret.origin.y),
                size: caret.size,
            }),
        ));
    }

    /// Whether `key` hangs from the main tree's root or a root view that
    /// does.
    pub(super) fn in_tree(&self, mut key: Key) -> bool {
        while key != NO_KEY {
            if key == self.root {
                return true;
            }
            key = self.nodes[key as usize].parent;
        }
        false
    }

    /// Answers `ref.measure()` call `call` on `id` (null when it is not
    /// laid out).
    pub(super) fn answer_measure(&mut self, id: NodeId, call: u32) {
        let value = self
            .measure(id)
            .map_or(JsonValue::Null, Measurement::to_json);
        self.settle(call, Ok(Box::new(value)));
    }

    /// Updates the area `key`'s descendants cover outside its bounds (its
    /// children's frames and areas are current), and mounts it when it
    /// changed. Clipping views have none: what is outside is not drawn.
    pub(super) fn update_overflow(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        if !node.created || key == self.root {
            return;
        }
        let clips = node.view_style.overflow_hidden
            || matches!(node.kind, Kind::Scroll | Kind::List | Kind::Stack);
        let extent = if clips || !valid(&node.frame) || node.children.is_empty() {
            None
        } else {
            let (w, h) = (node.frame.size.width.0, node.frame.size.height.0);
            let (mut x0, mut y0, mut x1, mut y1) = (0.0f32, 0.0f32, w, h);
            for &child in &node.children {
                let c = &self.nodes[child as usize];
                if c.is_layout_root() || c.layout.is_none() || !c.created || !valid(&c.frame) {
                    continue;
                }
                let f = c.frame;
                if f.size.width.0 > 0.0 && f.size.height.0 > 0.0 {
                    x0 = x0.min(f.origin.x.0);
                    y0 = y0.min(f.origin.y.0);
                    x1 = x1.max(f.origin.x.0 + f.size.width.0);
                    y1 = y1.max(f.origin.y.0 + f.size.height.0);
                }
                if let Some(e) = c.overflow {
                    x0 = x0.min(f.origin.x.0 + e.origin.x.0);
                    y0 = y0.min(f.origin.y.0 + e.origin.y.0);
                    x1 = x1.max(f.origin.x.0 + e.origin.x.0 + e.size.width.0);
                    y1 = y1.max(f.origin.y.0 + e.origin.y.0 + e.size.height.0);
                }
            }
            let out = x0 < -OVERFLOW_EPSILON
                || y0 < -OVERFLOW_EPSILON
                || x1 > w + OVERFLOW_EPSILON
                || y1 > h + OVERFLOW_EPSILON;
            out.then_some(rect(x0, y0, x1 - x0, y1 - y0))
        };
        let node = &mut self.nodes[key as usize];
        if node.overflow != extent {
            node.overflow = extent;
            let id = node.id;
            self.mount.push(MountItem::Overflow { id, extent });
        }
    }
}

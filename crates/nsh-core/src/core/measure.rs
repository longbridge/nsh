//! Where views are: measuring a node for `ref.measure()`, and the areas
//! descendants cover outside their parents' bounds, which platforms
//! hit-test ([`MountItem::Overflow`]).

use super::{Core, Key, Kind, NO_KEY};
use crate::{
    JsonValue,
    geometry::Rect,
    mount::MountItem,
    ops::{NodeId, ROOT_ID},
    text::TextEngine,
};

/// Where a view is, from the last layout (`ref.measure()`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    /// The frame relative to the parent view, as `onLayout` reports it.
    pub frame: Rect,
    /// The top left corner in the coordinates of the root view the view is
    /// in (the surface, or the modal it is shown in): those of touches'
    /// `pageX` / `pageY`. Scroll offsets and zoom scales of the scroll
    /// views and lists around it are applied; transforms are not.
    pub page_x: f32,
    /// See [`Measurement::page_x`].
    pub page_y: f32,
}

impl Measurement {
    /// The JS object `ref.measure()` resolves to.
    pub(crate) fn to_json(self) -> JsonValue {
        serde_json::json!({
            "x": self.frame.x,
            "y": self.frame.y,
            "width": self.frame.width,
            "height": self.frame.height,
            "pageX": self.page_x,
            "pageY": self.page_y,
        })
    }
}

/// Descendants reaching less than this far out of a view do not count
/// (rounding).
const OVERFLOW_EPSILON: f32 = 0.5;

fn valid(r: &Rect) -> bool {
    !(r.x.is_nan() || r.y.is_nan() || r.width.is_nan() || r.height.is_nan())
}

impl<M: TextEngine> Core<M> {
    /// Where view `id` is, from the last layout; `None` when it is not in
    /// the tree or was never laid out.
    pub fn measure(&self, id: NodeId) -> Option<Measurement> {
        let key = self.key(id).ok()?;
        let node = &self.nodes[key as usize];
        if !node.created || node.layout.is_none() || !valid(&node.frame) {
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
                x += n.frame.x;
                y += n.frame.y;
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
            page_x: x,
            page_y: y,
        })
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
        self.reply(ROOT_ID, call as u64, Ok(value));
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
            let (w, h) = (node.frame.width, node.frame.height);
            let (mut x0, mut y0, mut x1, mut y1) = (0.0f32, 0.0f32, w, h);
            for &child in &node.children {
                let c = &self.nodes[child as usize];
                if c.kind.is_root() || c.layout.is_none() || !c.created || !valid(&c.frame) {
                    continue;
                }
                let f = c.frame;
                if f.width > 0.0 && f.height > 0.0 {
                    x0 = x0.min(f.x);
                    y0 = y0.min(f.y);
                    x1 = x1.max(f.x + f.width);
                    y1 = y1.max(f.y + f.height);
                }
                if let Some(e) = c.overflow {
                    x0 = x0.min(f.x + e.x);
                    y0 = y0.min(f.y + e.y);
                    x1 = x1.max(f.x + e.x + e.width);
                    y1 = y1.max(f.y + e.y + e.height);
                }
            }
            let out = x0 < -OVERFLOW_EPSILON
                || y0 < -OVERFLOW_EPSILON
                || x1 > w + OVERFLOW_EPSILON
                || y1 > h + OVERFLOW_EPSILON;
            out.then_some(Rect {
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
            })
        };
        let node = &mut self.nodes[key as usize];
        if node.overflow != extent {
            node.overflow = extent;
            let id = node.id;
            self.mount.push(MountItem::Overflow { id, extent });
        }
    }
}

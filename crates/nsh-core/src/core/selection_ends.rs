//! Which list item a selectable view's selection starts and ends in
//! (`onSelectionChange`'s `anchor` and `focus`).
//!
//! A platform reports each end as the view node of its text and the UTF-16
//! offset in it ([`crate::selection::SelectionModel::ends`]). The core knows
//! the tree: the list cell around that text and the item the cell shows,
//! and lists the cell's texts as the platforms list an area's (document
//! order, hidden views and texts kept out of selection left out, a list
//! marker beside a text) to place the end in the copy of the whole item.
//! The runtime then names the item by the list's `keyExtractor`.

use std::{ops::Range, sync::Arc};

use super::{Core, Key, Kind, LeafContext, NO_KEY};
use crate::{
    AttributedText, CellSlot, PayloadValue,
    geometry::{Bounds, Point, Points, point},
    ops::NodeId,
    selection::{CELL_FIELDS, END_FIELDS, SelectionTexts, TextPosition, copy_offset, marker_of},
    text::TextEngine,
};

/// The texts of one list cell, in document order, as a selectable view's
/// backend lists them.
#[derive(Default)]
struct CellTexts {
    keys: Vec<u64>,
    texts: Vec<Arc<AttributedText>>,
    /// Content boxes in the cell's coordinates.
    frames: Vec<Bounds<Points>>,
    continues: Vec<bool>,
    markers: Vec<Option<String>>,
    /// The last text left out (`selectable={false}`): maybe the marker of
    /// the next.
    chrome: Option<(String, Bounds<Points>)>,
}

impl SelectionTexts for CellTexts {
    fn len(&self) -> usize {
        self.texts.len()
    }

    fn content(&self, i: usize) -> &str {
        &self.texts[i].text
    }

    fn copied(&self, i: usize, range: Range<u32>) -> String {
        self.texts[i].copied_text(range)
    }

    fn frame(&self, i: usize) -> Bounds<Points> {
        self.frames[i]
    }

    // Only copies are made of these texts: nothing hit-tests them.
    fn offset_at(&self, _: usize, _: Point<Points>) -> u32 {
        0
    }

    fn key(&self, i: usize) -> u64 {
        self.keys[i]
    }

    fn continues(&self, i: usize) -> bool {
        self.continues[i]
    }

    fn marker(&self, i: usize) -> Option<&str> {
        self.markers[i].as_deref()
    }
}

impl<M: TextEngine> Core<M> {
    /// Completes the ends of a selectable view's `onSelectionChange`
    /// payload: each end a platform reported as a text node and an offset
    /// in it ([`END_FIELDS`]) becomes the list, the cell's slot and the
    /// offset in the copy of the item the cell shows ([`CELL_FIELDS`]), or
    /// is left out when its text is in no list item.
    pub(super) fn locate_selection_ends(&self, fields: &mut Vec<(&'static str, PayloadValue)>) {
        let number = |fields: &[(&str, PayloadValue)], key: &str| {
            fields.iter().find_map(|(k, v)| match v {
                PayloadValue::Number(n) if *k == key && *n >= 0.0 => Some(*n),
                _ => None,
            })
        };
        let ends = END_FIELDS.map(|(node, offset)| {
            Some((
                number(fields, node)? as NodeId,
                number(fields, offset)? as u32,
            ))
        });
        fields.retain(|(k, _)| {
            !END_FIELDS
                .iter()
                .any(|(node, offset)| k == node || k == offset)
        });
        for (end, (list, slot, offset)) in ends.into_iter().zip(CELL_FIELDS) {
            let Some((cell_list, cell_slot, item_offset)) =
                end.and_then(|(node, at)| self.selection_end_in_item(node, at))
            else {
                continue;
            };
            fields.push((list, PayloadValue::Number(cell_list as f64)));
            fields.push((slot, PayloadValue::Number(f64::from(cell_slot))));
            fields.push((offset, PayloadValue::Number(f64::from(item_offset))));
        }
    }

    /// The list item around text node `text` and where its UTF-16 offset
    /// `offset` is in the copy of the item: the list, the slot of the
    /// item's cell and that offset. `None` outside a list item.
    fn selection_end_in_item(&self, text: NodeId, offset: u32) -> Option<(NodeId, u32, u32)> {
        let key = self.key(text).ok()?;
        let mut cell = key;
        let slot = loop {
            if cell == NO_KEY {
                return None;
            }
            let node = &self.nodes[cell as usize];
            let parent = node.parent;
            if let Some(CellSlot::Item(slot)) = node.slot
                && parent != NO_KEY
                && self.nodes[parent as usize].list.is_some()
            {
                break slot;
            }
            cell = parent;
        };
        let list = self.nodes[self.nodes[cell as usize].parent as usize].id;
        let mut texts = CellTexts::default();
        self.list_cell_texts(cell, point(Points::ZERO, Points::ZERO), &mut texts);
        let at = texts.keys.iter().position(|&k| k == u64::from(text))?;
        let len = utf16_len(&texts.texts[at].text);
        let item_offset = copy_offset(&texts, TextPosition::new(at, offset.min(len)));
        Some((list, slot, item_offset))
    }

    /// Lists the texts of `key`'s subtree into `out`, `key`'s top left at
    /// `origin` in the cell, as the platforms' selectable views do: hidden
    /// views are left out with their subtrees, texts kept out of selection
    /// (`selectable={false}`) are maybe the marker of the next.
    fn list_cell_texts(&self, key: Key, origin: Point<Points>, out: &mut CellTexts) {
        let node = &self.nodes[key as usize];
        if node.layout.is_none()
            || node.view_style.hidden
            || node.spare
            || node.hidden_in_text
            || !super::measure::valid(&node.frame)
        {
            return;
        }
        if node.kind == Kind::Text
            && let Some(LeafContext::Text(cx)) =
                node.layout.and_then(|l| self.layout.get_node_context(l))
        {
            let (text, content) = match &cx.laid_out {
                Some(laid) => (&laid.text, laid.content),
                None => (&cx.text, Bounds::new(Point::default(), node.frame.size)),
            };
            let frame = Bounds::new(
                point(origin.x + content.origin.x, origin.y + content.origin.y),
                content.size,
            );
            if text.selectable == Some(false) {
                out.chrome = Some((text.text.clone(), frame));
            } else {
                out.keys.push(u64::from(node.id));
                out.texts.push(Arc::clone(text));
                out.frames.push(frame);
                out.continues
                    .push(node.props.as_ref().is_some_and(|p| p.continues_text));
                out.markers
                    .push(marker_of(out.chrome.take().as_ref(), frame));
            }
        }
        // Children sit in the content, which a scroll view moves.
        let (scroll, zoom) = node
            .scroll
            .as_deref()
            .map_or(((0.0, 0.0), 1.0), |s| (s.offset, s.zoom));
        for &child in &node.children {
            let frame = self.nodes[child as usize].frame;
            let at = point(
                origin.x + Points(frame.origin.x.0 * zoom - scroll.0),
                origin.y + Points(frame.origin.y.0 * zoom - scroll.1),
            );
            self.list_cell_texts(child, at, out);
        }
    }
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

//! What the reader inserts into a Markdown part before parsing it (`cjk.rs`'s
//! marks, the escapes of prose `$`), and the parsed tree brought back to the
//! text given: its values without the marks, its positions offsets in that
//! text. So nothing inserted shows to plugins, which slice the source by
//! positions.

use markdown::{mdast::Node, unist::Point};

use super::cjk;

/// Insertions into a text, in order: where each starts in the text as
/// parsed (with every insertion) and how many bytes it is.
#[derive(Debug, Default)]
pub(super) struct Inserted {
    at: Vec<(usize, usize)>,
}

impl Inserted {
    /// Whether nothing is inserted.
    pub(super) fn is_empty(&self) -> bool {
        self.at.is_empty()
    }

    /// Insertions of `len` bytes each at offsets `at` (in order) of the
    /// text as it is now, which holds the insertions so far.
    pub(super) fn insert(&mut self, at: &[usize], len: usize) {
        debug_assert!(at.is_sorted());
        for (offset, _) in &mut self.at {
            // Text inserted where an insertion starts goes before it.
            let before = at.partition_point(|&a| a <= *offset);
            *offset += before * len;
        }
        self.at
            .extend(at.iter().enumerate().map(|(i, &a)| (a + i * len, len)));
        self.at.sort_unstable_by_key(|&(offset, _)| offset);
    }

    /// The offsets of the text as parsed mapped to the text given.
    fn map(&self) -> Map<'_> {
        let mut before = Vec::with_capacity(self.at.len());
        let mut sum = 0;
        for &(_, len) in &self.at {
            before.push(sum);
            sum += len;
        }
        Map {
            at: &self.at,
            before,
        }
    }
}

/// [`Inserted`] with the bytes inserted before each insertion.
struct Map<'a> {
    at: &'a [(usize, usize)],
    before: Vec<usize>,
}

impl Map<'_> {
    /// How many inserted bytes come before `offset` of the text as parsed;
    /// an offset inside an insertion counts as at its start.
    fn removed(&self, offset: usize) -> usize {
        let i = self.at.partition_point(|&(at, _)| at < offset);
        if i == 0 {
            return 0;
        }
        let (at, len) = self.at[i - 1];
        self.before[i - 1] + len.min(offset - at)
    }
}

/// The tree parsed from `parsed` (the text given with `inserted`) brought
/// back to the text given: positions mapped, `marked` values unmarked.
pub(super) fn restore(root: &mut Node, parsed: &str, inserted: &Inserted, marked: bool) {
    let map = inserted.map();
    // Insertions never hold a line break: a column moves by what was
    // inserted between its line's start and it.
    let lines: Vec<usize> = parsed
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'\n').then_some(i + 1))
        .collect();
    let point = |point: &mut Point| {
        let removed = map.removed(point.offset);
        if removed == 0 {
            return;
        }
        let line = lines
            .partition_point(|&start| start <= point.offset)
            .checked_sub(1)
            .map_or(0, |i| lines[i]);
        point.column -= removed - map.removed(line);
        point.offset -= removed;
    };
    // A stack, not recursion: the tree may be as deep as the input nests.
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if marked {
            cjk::unmark_node(node);
        }
        if let Some(position) = node.position_mut() {
            point(&mut position.start);
            point(&mut position.end);
        }
        if let Some(children) = node.children_mut() {
            stack.extend(children.iter_mut());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Inserted;

    #[test]
    fn offsets_map_back_past_insertions() {
        // "ab|c|d" from "abcd": marks of 3 bytes after `b` and `c`.
        let mut inserted = Inserted::default();
        inserted.insert(&[2, 3], 3);
        assert_eq!(inserted.at, vec![(2, 3), (6, 3)]);
        // An escape of 1 byte before `a`, and one at the second mark.
        inserted.insert(&[0, 6], 1);
        assert_eq!(inserted.at, vec![(0, 1), (3, 3), (7, 1), (8, 3)]);
        let map = inserted.map();
        // "\ab|c\|d": every offset of the text as parsed.
        let mapped: Vec<usize> = (0..=12).map(|o| o - map.removed(o)).collect();
        assert_eq!(mapped, vec![0, 0, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3, 4]);
    }
}

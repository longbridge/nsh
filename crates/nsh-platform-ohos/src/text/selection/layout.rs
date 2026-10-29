//! Owned selection geometry from the engine's original paragraph, in logical units.
use std::{ops::Range, sync::Arc};

use unicode_segmentation::UnicodeSegmentation;

pub(crate) const MAX_CLUSTERS: usize = 65_536;
pub(crate) const MAX_TEXT_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Region {
    pub(crate) bounds: [f32; 4],
    pub(crate) rtl: bool,
}
struct Cluster {
    byte: usize,
    word: Range<usize>,
    regions: Range<usize>,
}
pub(crate) struct Layout {
    text: Arc<str>,
    clusters: Vec<Cluster>,
    regions: Vec<Region>,
}
#[derive(Default)]
pub(crate) struct Geometry {
    pub(crate) rectangles: Vec<[f32; 4]>,
    pub(crate) handles: Option<[(f32, f32, f32); 2]>,
}
impl Layout {
    /// The callback runs only on the engine thread and returns owned native boxes.
    pub(crate) fn new(
        text: Arc<str>,
        width: f32,
        height: f32,
        mut boxes: impl FnMut(Range<usize>) -> Vec<Region>,
    ) -> Option<Self> {
        if text.len() > MAX_TEXT_BYTES {
            return None;
        }
        let mut clusters = Vec::new();
        let mut regions = Vec::new();
        let mut words = text.split_word_bound_indices();
        let mut word = 0..0;
        let mut utf16 = 0;
        for (byte, grapheme) in text.grapheme_indices(true) {
            if clusters.len() == MAX_CLUSTERS {
                return None;
            }
            while byte >= word.end {
                let (start, value) = words.next()?;
                word = start..start + value.len();
            }
            let start = regions.len();
            let end = utf16 + grapheme.encode_utf16().count();
            for mut region in boxes(utf16..end) {
                let r = &mut region.bounds;
                if !r.iter().all(|v| v.is_finite()) {
                    continue;
                }
                r[0] = r[0].max(0.0);
                r[1] = r[1].max(0.0);
                r[2] = r[2].min(width);
                r[3] = r[3].min(height);
                if r[2] > r[0] && r[3] > r[1] {
                    // A grapheme can span many bidi/font runs. Bound the whole
                    // interaction cache, not the number of boxes for one grapheme.
                    if regions.len() == MAX_CLUSTERS * 16 {
                        return None;
                    }
                    regions.push(region);
                }
            }
            clusters.push(Cluster {
                byte,
                word: word.clone(),
                regions: start..regions.len(),
            });
            utf16 = end;
        }
        Some(Self {
            text,
            clusters,
            regions,
        })
    }
    pub(crate) fn storage_bytes(&self) -> usize {
        self.text.len()
            + self.clusters.capacity() * std::mem::size_of::<Cluster>()
            + self.regions.capacity() * std::mem::size_of::<Region>()
    }
    pub(crate) fn len(&self) -> usize {
        self.clusters.len()
    }
    fn byte(&self, boundary: usize) -> usize {
        self.clusters
            .get(boundary)
            .map_or(self.text.len(), |cluster| cluster.byte)
    }
    pub(crate) fn copy(&self, range: Range<usize>) -> String {
        self.text[self.byte(range.start)..self.byte(range.end)]
            .chars()
            .filter(|&c| c != '\u{fffc}')
            .collect()
    }
    pub(crate) fn rectangles(&self, range: Range<usize>) -> impl Iterator<Item = Region> + '_ {
        self.clusters[range]
            .iter()
            .flat_map(|cluster| self.regions[cluster.regions.clone()].iter().copied())
    }
    pub(crate) fn geometry(&self, range: Range<usize>, geometry: &mut Geometry) {
        let rectangles = &mut geometry.rectangles;
        rectangles.clear();
        rectangles.extend(self.rectangles(range.clone()).map(|r| r.bounds));
        rectangles.sort_unstable_by(|a, b| {
            a[1].total_cmp(&b[1])
                .then(a[3].total_cmp(&b[3]))
                .then(a[0].total_cmp(&b[0]))
        });
        let mut write = 0;
        for read in 0..rectangles.len() {
            let r = rectangles[read];
            if write > 0 {
                let previous = &mut rectangles[write - 1];
                if previous[1] == r[1] && previous[3] == r[3] && previous[2] >= r[0] {
                    previous[2] = previous[2].max(r[2]);
                    continue;
                }
            }
            rectangles[write] = r;
            write += 1;
        }
        rectangles.truncate(write);
        geometry.handles = self.handles(range);
    }
    pub(crate) fn word(&self, point: (f32, f32)) -> Option<Range<usize>> {
        let (index, _) = self.nearest(point)?;
        let word = &self.clusters[index].word;
        Some(
            self.clusters.partition_point(|c| c.byte < word.start)
                ..self.clusters.partition_point(|c| c.byte < word.end),
        )
    }
    pub(crate) fn boundary(&self, point: (f32, f32)) -> Option<usize> {
        let (index, region) = self.nearest(point)?;
        let right = point.0 >= (region.bounds[0] + region.bounds[2]) * 0.5;
        Some(index + usize::from(right != region.rtl))
    }
    fn nearest(&self, point: (f32, f32)) -> Option<(usize, Region)> {
        if !point.0.is_finite() || !point.1.is_finite() {
            return None;
        }
        self.clusters
            .iter()
            .enumerate()
            .flat_map(|(i, cluster)| {
                self.regions[cluster.regions.clone()]
                    .iter()
                    .map(move |r| (i, *r))
            })
            .min_by(|(_, a), (_, b)| {
                let distance = |r: &Region| {
                    let r = r.bounds;
                    let dy = (r[1] - point.1).max(0.0).max(point.1 - r[3]);
                    let dx = (r[0] - point.0).max(0.0).max(point.0 - r[2]);
                    // Stay on the nearest visual line before choosing its character.
                    dy * 1_000_000.0 + dx
                };
                distance(a).total_cmp(&distance(b))
            })
    }
    pub(crate) fn handles(&self, range: Range<usize>) -> Option<[(f32, f32, f32); 2]> {
        let first = self
            .clusters
            .get(range.start..range.end)?
            .iter()
            .find_map(|c| self.regions.get(c.regions.clone())?.first())?;
        let last = self
            .clusters
            .get(range.start..range.end)?
            .iter()
            .rev()
            .find_map(|c| self.regions.get(c.regions.clone())?.last())?;
        let edge = |r: &Region, end: bool| {
            (
                r.bounds[if end != r.rtl { 2 } else { 0 }],
                r.bounds[1],
                r.bounds[3],
            )
        };
        Some([edge(first, false), edge(last, true)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout(text: &str, rtl: bool) -> Layout {
        Layout::new(Arc::from(text), 1000.0, 20.0, |range| {
            vec![Region {
                bounds: [
                    range.start as f32 * 10.0,
                    0.0,
                    range.end as f32 * 10.0,
                    20.0,
                ],
                rtl,
            }]
        })
        .unwrap()
    }
    #[test]
    fn one_grapheme_can_have_more_than_four_visual_boxes() {
        let layout = Layout::new(Arc::from("a"), 1000.0, 20.0, |_| {
            (0..8)
                .map(|i| Region {
                    bounds: [i as f32 * 10.0, 0.0, i as f32 * 10.0 + 5.0, 20.0],
                    rtl: false,
                })
                .collect()
        })
        .unwrap();
        assert_eq!(layout.rectangles(0..1).count(), 8);
        assert_eq!(layout.copy(0..1), "a");
        assert!(layout.storage_bytes() > 8 * std::mem::size_of::<Region>());
    }
    #[test]
    fn graphemes_and_copy_preserve_unicode_and_drop_attachments() {
        let l = layout("Hi 👩🏽‍💻 e\u{301}\u{fffc}!", false);
        assert_eq!(l.len(), 8);
        assert_eq!(l.copy(3..4), "👩🏽‍💻");
        assert_eq!(l.copy(5..8), "e\u{301}!");
        assert_eq!(l.word((5.0, 10.0)), Some(0..2));
    }
    #[test]
    fn bidi_edges_follow_visual_run_and_hidden_text_has_no_hit_boxes() {
        let l = layout("אב", true);
        assert_eq!(l.boundary((1.0, 10.0)), Some(1));
        assert_eq!(l.boundary((9.0, 10.0)), Some(0));
        assert_eq!(
            l.handles(0..2),
            Some([(10.0, 0.0, 20.0), (10.0, 0.0, 20.0)])
        );
        let clipped = Layout::new(Arc::from("ab"), 10.0, 20.0, |r| {
            vec![Region {
                bounds: [r.start as f32 * 10.0, 0.0, r.end as f32 * 10.0, 20.0],
                rtl: false,
            }]
        })
        .unwrap();
        assert_eq!(clipped.word((900.0, 10.0)), Some(0..2));
        assert_eq!(clipped.copy(0..2), "ab");
        assert_eq!(clipped.rectangles(0..2).count(), 1);
    }
    #[test]
    fn selection_limits_reject_unbounded_input() {
        assert!(
            Layout::new(
                Arc::from("a".repeat(MAX_TEXT_BYTES + 1)),
                10.0,
                10.0,
                |_| vec![]
            )
            .is_none()
        );
        assert!(
            Layout::new(
                Arc::from("a".repeat(MAX_CLUSTERS + 1)),
                10.0,
                10.0,
                |_| vec![]
            )
            .is_none()
        );
    }
    #[test]
    fn overlapping_ligature_boxes_are_painted_once() {
        let l = Layout::new(Arc::from("ffi"), 30.0, 20.0, |_| {
            vec![Region {
                bounds: [0.0, 0.0, 30.0, 20.0],
                rtl: false,
            }]
        })
        .unwrap();
        let mut geometry = Geometry::default();
        l.geometry(0..3, &mut geometry);
        assert_eq!(geometry.rectangles, vec![[0.0, 0.0, 30.0, 20.0]]);
        assert_eq!(
            geometry.handles,
            Some([(0.0, 0.0, 20.0), (30.0, 0.0, 20.0)])
        );
        let capacity = geometry.rectangles.capacity();
        l.geometry(0..1, &mut geometry);
        assert_eq!(geometry.rectangles.capacity(), capacity);
        assert_eq!(geometry.rectangles, vec![[0.0, 0.0, 30.0, 20.0]]);
    }
}

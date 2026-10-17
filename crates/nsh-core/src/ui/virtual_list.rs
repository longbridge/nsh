//! Lists of a component's own rows ([`virtual_list`]): virtualized and
//! recycled like `List`, the rows rendered in Rust. The core binds the
//! cells itself (no JS), within the commit that needs them. The layout
//! methods both kinds of list share are [`ListLayout`], how they scroll
//! [`super::Scrollable`].

use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    ops::RangeInclusive,
    rc::Rc,
    time::Duration,
};

use super::{
    Accessible, AnyElement, If, Interactive, IntoReactive, Position, Reactive,
    element::{BoundListConfig, ElKind, ElNode, Element, IntoElement, ListConfig, Listener},
    events,
};
use crate::{
    CellSlot, EventKind, EventPayload, ListContentStyle, PayloadValue, Prop, Tag, Viewability,
};

/// How a list lays out its rows: [`super::ListElement`] (the app's rows)
/// and [`VirtualList`] (the component's).
pub trait ListLayout: Sized {
    #[doc(hidden)]
    fn config(&mut self) -> &mut ListConfig;

    #[doc(hidden)]
    fn bound_config(&mut self) -> &mut Vec<BoundListConfig>;

    /// Sets a field of the layout with `set`: once for a value, in a
    /// binding (a component that runs once) for a signal or a closure.
    #[doc(hidden)]
    fn layout<T: 'static>(
        mut self,
        value: impl IntoReactive<T>,
        set: impl Fn(&mut ListConfig, T) + 'static,
    ) -> Self {
        match value.into_reactive() {
            Reactive::Static(v) => set(self.config(), v),
            Reactive::Bound(f) => self.bound_config().push(Rc::new(move |c| set(c, f()))),
        }
        self
    }

    /// The size of rows not measured yet, in points (default 50).
    fn estimated_item_size(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| c.estimated = Some(points))
    }

    /// Rows side by side (vertical lists).
    fn columns(self, columns: impl IntoReactive<u32>) -> Self {
        self.layout(columns, |c, columns| c.columns = Some(columns.max(1)))
    }

    /// Scrolls horizontally.
    fn horizontal(mut self) -> Self {
        self.config().horizontal = Some(true);
        self
    }

    /// Item 0 at the end (chats), starting scrolled to the end.
    fn inverted(mut self) -> Self {
        self.config().inverted = Some(true);
        self
    }

    /// Stays at the end of its content while it is within `threshold`
    /// points of it, as the content grows or the viewport shrinks (a reply
    /// streaming into the last row, the keyboard showing); scrolled further
    /// up, what is visible stays where it is (TS:
    /// `maintainVisibleContentPosition={{ autoscrollToBottomThreshold }}`).
    /// For lists that are not [`Self::inverted`], which keep their distance
    /// from the end anyway.
    fn follow_end(self, threshold: impl IntoReactive<f32>) -> Self {
        self.layout(threshold, |c, threshold| {
            c.follow_end = Some(threshold.max(0.0));
        })
    }

    /// Whether the scroll indicator shows (default yes).
    fn shows_indicator(self, shows: impl IntoReactive<bool>) -> Self {
        self.layout(shows, |c, shows| c.shows_indicator = Some(shows))
    }

    /// Padding around the rows, in points.
    fn content_padding(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| {
            content(c).padding = crate::EdgeInsets::all(points);
        })
    }

    /// Horizontal padding around the rows.
    fn content_padding_x(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| {
            let p = &mut content(c).padding;
            (p.left, p.right) = (points, points);
        })
    }

    /// Vertical padding around the rows.
    fn content_padding_y(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| {
            let p = &mut content(c).padding;
            (p.top, p.bottom) = (points, points);
        })
    }

    /// Padding above the rows.
    fn content_padding_top(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| content(c).padding.top = points)
    }

    /// Padding below the rows.
    fn content_padding_bottom(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| content(c).padding.bottom = points)
    }

    /// Space between rows (columns in horizontal lists).
    fn row_gap(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| content(c).row_gap = points)
    }

    /// Space between columns ([`Self::columns`]).
    fn column_gap(self, points: impl IntoReactive<f32>) -> Self {
        self.layout(points, |c, points| content(c).column_gap = points)
    }
}

/// The content style of a list's layout, made when first set.
fn content(config: &mut ListConfig) -> &mut ListContentStyle {
    config.content.get_or_insert_default()
}

/// Renders the row at a position.
pub(crate) type RowFn = Rc<dyn Fn(Position) -> AnyElement>;

/// Renders the separator between two rows.
pub(crate) type SeparatorFn = Rc<dyn Fn() -> AnyElement>;

/// The rows' identities, hashed, read in a binding.
pub(crate) type KeysFn = Rc<dyn Fn() -> Rc<[u64]>>;

/// A list of the component's own rows, as a view makes it.
#[derive(Clone)]
pub(crate) struct VirtualListEl {
    pub(crate) config: ListConfig,
    /// How many rows: bound, a binding that updates the list in place.
    pub(crate) count: Reactive<u32>,
    pub(crate) row: RowFn,
    /// The rows' identities, for splices (rows keep their cells and sizes
    /// when rows are added or removed before them).
    pub(crate) keys: Option<KeysFn>,
    /// Between rows (after each row but the last; before it, inverted).
    pub(crate) separator: Option<SeparatorFn>,
    /// The row scrolled to before the first frame.
    pub(crate) initial_index: Option<u32>,
    /// How long rows stay viewable before they are reported.
    pub(crate) min_view_time: Option<Duration>,
}

/// A virtualized, recycling list of `count` rows rendered by `row`: only
/// the rows near the viewport exist. `row(at)` builds the row at `at`
/// (`at.index()`) from the component's data.
///
/// Each cell's content is made once: when the cell shows another row its
/// position changes, and only the bindings that read it run again
/// (`text(move || rows.with(|r| r[at.index()].name.clone()))`). `count`
/// can be a signal or a closure: the list grows and shrinks in place. Give
/// the rows' identities with [`VirtualList::keys`] when rows come and go
/// other than at the end.
pub fn virtual_list<R: IntoElement>(
    count: impl IntoReactive<usize>,
    row: impl Fn(Position) -> R + 'static,
) -> VirtualList {
    let count = count.into_reactive().map(|n| n as u32);
    let mut node = ElNode::new(Tag::List);
    node.kind = ElKind::VirtualList(Box::new(VirtualListEl {
        config: ListConfig::default(),
        count,
        row: Rc::new(move |at| row(at).into_element()),
        keys: None,
        separator: None,
        initial_index: None,
        min_view_time: None,
    }));
    VirtualList(node)
}

/// One of a list's own cells (header, footer, empty) holding `content`.
fn extra_cell(slot: CellSlot, key: &'static str, content: AnyElement) -> ElNode {
    let mut cell = ElNode::new(Tag::Cell);
    cell.key = Some(key.into());
    super::host::set(&mut cell, Prop::Slot(slot));
    cell.children.push(content.0);
    cell
}

/// A list of the component's own rows ([`virtual_list`]).
#[derive(Clone)]
pub struct VirtualList(ElNode);

impl VirtualList {
    fn list(&mut self) -> &mut VirtualListEl {
        match &mut self.0.kind {
            ElKind::VirtualList(list) => list,
            _ => unreachable!("a VirtualList is a virtual list"),
        }
    }

    /// One of the list's own cells (header, footer, empty), outside the
    /// recycling: made with the view, kept while the view has it.
    fn extra(mut self, slot: CellSlot, key: &'static str, content: AnyElement) -> Self {
        self.0
            .children
            .retain(|c| c.key.as_ref().is_none_or(|k| k.as_str() != key));
        self.0.children.push(extra_cell(slot, key, content));
        self
    }

    /// The rows' identities, in order (any hashable id per row), read in a
    /// binding:
    /// `.keys(move || rows.with(|r| r.iter().map(|r| r.id).collect::<Vec<_>>()))`.
    pub fn keys<K: Hash, I: IntoIterator<Item = K>>(
        mut self,
        keys: impl Fn() -> I + 'static,
    ) -> Self {
        self.list().keys = Some(Rc::new(move || {
            keys()
                .into_iter()
                .map(|k| {
                    let mut h = rustc_hash::FxHasher::default();
                    k.hash(&mut h);
                    h.finish()
                })
                .collect()
        }));
        self
    }

    /// Above the rows, scrolling with them (TS: `ListHeaderComponent`):
    /// part of this view, so its state and handlers are the
    /// component's, and it is never recycled.
    pub fn header(self, header: impl IntoElement) -> Self {
        self.extra(CellSlot::Header, "\0header", header.into_element())
    }

    /// Below the rows, scrolling with them (TS: `ListFooterComponent`).
    pub fn footer(self, footer: impl IntoElement) -> Self {
        self.extra(CellSlot::Footer, "\0footer", footer.into_element())
    }

    /// Shown instead of the rows while there are none (TS:
    /// `ListEmptyComponent`).
    pub fn empty(mut self, empty: impl IntoElement) -> Self {
        let empty = empty.into_element();
        match self.list().count.clone() {
            Reactive::Static(0) => self.extra(CellSlot::Empty, "\0empty", empty),
            Reactive::Static(_) => self,
            // While the bound count is 0.
            Reactive::Bound(count) => {
                let cell = extra_cell(CellSlot::Empty, "\0empty", empty);
                let shown = If::new(move || count() == 0, move || AnyElement(cell.clone()));
                self.0.children.push(shown.into_element().0);
                self
            }
        }
    }

    /// Between rows (TS: `ItemSeparatorComponent`): `separator()` renders
    /// one after each row but the last (before it in an inverted list), in
    /// the row's cell.
    pub fn separator<R: IntoElement>(mut self, separator: impl Fn() -> R + 'static) -> Self {
        self.list().separator = Some(Rc::new(move || separator().into_element()));
        self
    }

    /// These rows stick to the start while scrolled past, until the next one
    /// pushes them out (section headers; one column, not inverted; TS:
    /// `stickyHeaderIndices`).
    pub fn sticky_header_indices(mut self, indices: impl IntoIterator<Item = usize>) -> Self {
        let indices = indices.into_iter().map(|i| i as u32).collect();
        super::host::set(&mut self.0, Prop::StickyIndices(indices));
        self
    }

    /// Scrolls to row `index` before the first frame (TS:
    /// `initialScrollIndex`).
    pub fn initial_scroll_index(mut self, index: usize) -> Self {
        self.list().initial_index = Some(index as u32);
        self
    }

    /// When rows count as viewable for
    /// [`Self::on_viewable_items_changed`] (TS: `viewabilityConfig`'s
    /// thresholds; without one, any visible pixel counts).
    pub fn viewability(mut self, config: Viewability) -> Self {
        super::host::set(&mut self.0, Prop::Viewability(config));
        self
    }

    /// Rows are reported viewable once they stayed so this long (TS:
    /// `viewabilityConfig.minimumViewTime`).
    pub fn minimum_view_time(mut self, time: Duration) -> Self {
        self.list().min_view_time = Some(time).filter(|t| !t.is_zero());
        self
    }

    /// The viewable rows changed (TS: `onViewableItemsChanged`).
    pub fn on_viewable_items_changed(self, listener: impl Into<Listener<ViewableItems>>) -> Self {
        self.on(
            EventKind::ViewableItemsChanged,
            events::typed(listener.into(), viewable),
        )
    }

    /// Scrolled close to the end (load more).
    pub fn on_end_reached(self, listener: impl Into<Listener<()>>) -> Self {
        self.on(
            EventKind::EndReached,
            events::typed(listener.into(), events::unit),
        )
    }

    /// How close to the end [`Self::on_end_reached`] fires, in viewports
    /// (default 1; TS: `onEndReachedThreshold`).
    pub fn end_reached_threshold(mut self, viewports: f32) -> Self {
        super::host::set(&mut self.0, Prop::EndReachedThreshold(viewports));
        self
    }

    /// This list as the rows of a component's `Items` prop named `name`
    /// ([`super::ComponentElement::items`]): the slot's node, laid out and
    /// styled by the component that places it.
    pub(crate) fn into_items(mut self, name: &'static str) -> ElNode {
        self.list().config = ListConfig::default();
        self.0.style = Default::default();
        self.0.key = Some(format!("slot:{name}").into());
        self.0.attrs.props.push(Prop::ComponentSlot(name.into()));
        self.0
    }
}

/// The rows a list shows ([`VirtualList::on_viewable_items_changed`]), by
/// index, and the ones it showed at the report before.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewableItems {
    /// The first and last viewable row (`None`: none is).
    pub viewable: Option<(usize, usize)>,
    /// The same at the report before.
    pub previous: Option<(usize, usize)>,
}

impl ViewableItems {
    fn range(rows: Option<(usize, usize)>) -> RangeInclusive<usize> {
        match rows {
            Some((first, last)) => first..=last,
            #[allow(clippy::reversed_empty_ranges)]
            None => 1..=0,
        }
    }

    /// The viewable rows (TS: `viewableItems`).
    pub fn indices(&self) -> RangeInclusive<usize> {
        Self::range(self.viewable)
    }

    /// The rows that became viewable (`true`) or stopped being viewable
    /// (`false`) since the report before (TS: `changed`).
    pub fn changed(&self) -> impl Iterator<Item = (usize, bool)> + '_ {
        let (now, before) = (self.indices(), Self::range(self.previous));
        let came = now
            .clone()
            .filter(move |i| !before.contains(i))
            .map(|i| (i, true));
        let (now, before) = (self.indices(), Self::range(self.previous));
        let went = before.filter(move |i| !now.contains(i)).map(|i| (i, false));
        came.chain(went)
    }
}

/// The payload of a viewability report: `first`, `last`, `previousFirst`,
/// `previousLast` (-1: none).
pub(crate) fn viewable_payload(
    now: Option<(u32, u32)>,
    before: Option<(u32, u32)>,
) -> EventPayload {
    let n = |v: Option<u32>| PayloadValue::Number(v.map_or(-1.0, f64::from));
    EventPayload::Map(vec![
        ("first", n(now.map(|r| r.0))),
        ("last", n(now.map(|r| r.1))),
        ("previousFirst", n(before.map(|r| r.0))),
        ("previousLast", n(before.map(|r| r.1))),
    ])
}

/// The range in a viewability payload under `first` / `last` (or the
/// `previous…` keys).
pub(crate) fn viewable_range(p: &EventPayload, first: &str, last: &str) -> Option<(u32, u32)> {
    let EventPayload::Map(entries) = p else {
        return None;
    };
    let get = |key: &str| {
        entries.iter().find_map(|(k, v)| match v {
            PayloadValue::Number(n) if *k == key && *n >= 0.0 => Some(*n as u32),
            _ => None,
        })
    };
    Some((get(first)?, get(last)?))
}

fn viewable(p: &EventPayload) -> Option<Cow<'_, ViewableItems>> {
    let range = |first, last| viewable_range(p, first, last).map(|(f, l)| (f as usize, l as usize));
    Some(Cow::Owned(ViewableItems {
        viewable: range("first", "last"),
        previous: range("previousFirst", "previousLast"),
    }))
}

impl ListLayout for VirtualList {
    fn config(&mut self) -> &mut ListConfig {
        &mut self.list().config
    }

    fn bound_config(&mut self) -> &mut Vec<BoundListConfig> {
        &mut self.0.bound_list
    }
}

impl Element for VirtualList {
    fn node(&mut self) -> &mut ElNode {
        &mut self.0
    }
}

impl IntoElement for VirtualList {
    fn into_element(self) -> AnyElement {
        AnyElement(self.0)
    }
}

impl Accessible for VirtualList {}
impl Interactive for VirtualList {}
impl super::Scrollable for VirtualList {}

/// How the rows changed from `old` to `new` keys: `(at, removed,
/// inserted)` by the common start and end (as `List` does), `None` when
/// they are the same.
pub(crate) fn splice(old: &[u64], new: &[u64]) -> Option<(u32, u32, u32)> {
    let max = old.len().min(new.len());
    let start = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take(max - start)
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old.len() - start - end;
    let inserted = new.len() - start - end;
    (removed > 0 || inserted > 0).then_some((start as u32, removed as u32, inserted as u32))
}

#[cfg(test)]
mod tests {
    use super::ViewableItems;

    #[test]
    fn viewable_rows_report_what_changed() {
        let report = ViewableItems {
            viewable: Some((3, 6)),
            previous: Some((1, 4)),
        };
        assert_eq!(report.indices(), 3..=6);
        let changed: Vec<_> = report.changed().collect();
        assert_eq!(changed, [(5, true), (6, true), (1, false), (2, false)]);
        let none = ViewableItems {
            viewable: None,
            previous: Some((0, 1)),
        };
        assert_eq!(none.indices().count(), 0);
        assert_eq!(none.changed().collect::<Vec<_>>(), [(0, false), (1, false)]);
    }

    #[test]
    fn splices_keep_the_common_ends() {
        use super::splice;
        assert_eq!(splice(&[1, 2, 3], &[1, 2, 3]), None);
        assert_eq!(splice(&[1, 2, 3], &[0, 1, 2, 3]), Some((0, 0, 1)));
        assert_eq!(splice(&[1, 2, 3], &[1, 3]), Some((1, 1, 0)));
        assert_eq!(splice(&[1, 2, 3], &[1, 2, 3, 4]), Some((3, 0, 1)));
        assert_eq!(splice(&[1, 1], &[1]), Some((1, 1, 0)));
    }
}

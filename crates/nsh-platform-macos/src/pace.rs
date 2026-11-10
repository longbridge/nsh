//! Spreads a batch that makes hundreds of views out of sight over frames.
//!
//! A list row that mounts out of sight (the overscan, a long reply staged
//! in) comes as one batch of hundreds of views; applied whole, the views'
//! creation and the Core Animation commit that sends their layers to the
//! render server land in one frame (90 ms for 420 views). What the batch
//! makes under views that exist already is split off as trees
//! ([`plan`]); after the rest is applied, a tree that shows is applied at
//! once, and the others wait ([`Pacer`]) and come a level at a time, each
//! view whole, within [`BUDGET`] a frame (nearest the view first). A tree
//! the scroll brings in before its turn comes at once; a later batch that
//! names a view of a waiting tree applies that tree first, so the views
//! change in the order the core sent.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use nsh_core::{MountItem, NodeId, ViewKind};
use rustc_hash::{FxHashMap, FxHashSet};

/// Batches that make fewer views are applied whole.
pub(crate) const MIN_VIEWS: usize = 64;

/// The time a frame spends on waiting views.
pub(crate) const BUDGET: Duration = Duration::from_millis(4);

/// The views a frame adds at most. Most of a view's cost comes after the
/// budget is measured, in the commit that sends its layer to the render
/// server: about 0.2 ms a view all told (420 views in a 87 ms frame), so
/// 32 keep a frame near 7 ms.
pub(crate) const MAX_VIEWS: usize = 32;

/// Views a batch makes under a view that exists already: a root, its
/// descendants, and every item of theirs.
#[derive(Debug)]
pub(crate) struct Tree {
    /// The view the root goes into.
    pub(crate) parent: NodeId,
    /// The root.
    pub(crate) root: NodeId,
    /// The root's frame in its parent.
    pub(crate) frame: nsh_core::Bounds<nsh_core::Points>,
    /// Each view's items, in the batch's order: parents before children,
    /// siblings in the order they were inserted.
    pub(crate) views: VecDeque<(NodeId, Vec<MountItem>)>,
}

/// What [`plan`] splits off a batch.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// Items that wait with a tree (by index into the batch).
    pub(crate) waits: Vec<bool>,
    pub(crate) trees: Vec<Tree>,
}

/// The ids an item names: the view it is about, and the others it refers
/// to (a parent, a sibling, sticky cells). `None` for items about no view.
fn named(item: &MountItem, mut f: impl FnMut(NodeId)) {
    use MountItem as M;
    match item {
        M::Insert {
            parent,
            child,
            before,
        } => {
            f(*parent);
            f(*child);
            if let Some(b) = before {
                f(*b);
            }
        }
        M::StickyCells { id, cells } => {
            f(*id);
            for c in cells {
                f(c.cell);
            }
        }
        // The logical order of mounted views: macOS keeps none.
        M::LayoutDirection { .. }
        | M::Appearance { .. }
        | M::Keymap(_)
        | M::TreeOrder(_)
        | M::LayoutAnimation(_)
        | M::Animation(_) => {}
        other => {
            if let Some(id) = own(other) {
                f(id);
            }
        }
    }
}

/// The view an item is about: an insert's child.
fn own(item: &MountItem) -> Option<NodeId> {
    use MountItem as M;
    Some(match item {
        M::Insert { child, .. } => *child,
        M::Create { id, .. }
        | M::TrackStates { id, .. }
        | M::Remove { id }
        | M::Delete { id }
        | M::Frame { id, .. }
        | M::Overflow { id, .. }
        | M::ViewStyle { id, .. }
        | M::Text { id, .. }
        | M::Events { id, .. }
        | M::Image { id, .. }
        | M::ScrollConfig { id, .. }
        | M::ScrollContent { id, .. }
        | M::ScrollBy { id, .. }
        | M::StickyCells { id, .. }
        | M::ScrollBehavior { id, .. }
        | M::Refreshing { id, .. }
        | M::Canvas { id, .. }
        | M::Shader { id, .. }
        | M::AnimationBindings { id, .. }
        | M::Gestures { id, .. }
        | M::ViewProps { id, .. }
        | M::AccessibilityLabel { id, .. }
        | M::Input { id, .. }
        | M::InputValue { id, .. }
        | M::InputContent { id, .. }
        | M::ScreenConfig { id, .. }
        | M::NativeProps { id, .. }
        | M::ModalConfig { id, .. }
        | M::Component { id, .. }
        | M::ComponentPropSlots { id, .. }
        | M::ComponentProps { id, .. }
        | M::Command { id, .. }
        | M::ProjectStates { id, .. }
        | M::ShaderTransition { id, .. }
        | M::NavShader { id, .. }
        | M::ShaderInput { id } => *id,
        M::LayoutDirection { .. }
        | M::Appearance { .. }
        | M::Keymap(_)
        | M::TreeOrder(_)
        | M::LayoutAnimation(_)
        | M::Animation(_) => return None,
    })
}

/// Whether a view of `kind` may wait: not what the navigation, the
/// keyboard or the window place themselves.
fn may_wait(kind: ViewKind) -> bool {
    !matches!(
        kind,
        ViewKind::Modal | ViewKind::HeaderItem | ViewKind::InputAccessory | ViewKind::Stack
    )
}

/// Splits off `items` the trees of views they make under views that
/// `exists`, when they make [`MIN_VIEWS`] or more. A tree stays in the
/// batch when anything else in it names one of its views (a sibling
/// inserted before it, a command, a removal), when it has no frame yet,
/// or when it holds a view that may not wait; nothing waits in a batch
/// that animates its layout.
pub(crate) fn plan(items: &[MountItem], exists: impl Fn(NodeId) -> bool) -> Option<Plan> {
    let mut made: FxHashMap<NodeId, ViewKind> = FxHashMap::default();
    for item in items {
        match item {
            MountItem::Create { id, kind } => {
                made.insert(*id, *kind);
            }
            MountItem::LayoutAnimation(_) => return None,
            _ => {}
        }
    }
    if made.len() < MIN_VIEWS {
        return None;
    }
    // Where each view made goes, and in which order.
    let mut parent: FxHashMap<NodeId, NodeId> = FxHashMap::default();
    let mut order: FxHashMap<NodeId, usize> = FxHashMap::default();
    let mut moved = FxHashSet::default();
    for (k, item) in items.iter().enumerate() {
        if let MountItem::Insert {
            parent: p, child, ..
        } = item
            && made.contains_key(child)
            && parent.insert(*child, *p).is_some()
        {
            moved.insert(*child);
        }
        if let MountItem::Insert { child, .. } = item {
            order.entry(*child).or_insert(k);
        }
    }
    // Each view's root: the view made under one that exists.
    let mut root_of: FxHashMap<NodeId, NodeId> = FxHashMap::default();
    for &id in made.keys() {
        let mut at = id;
        let mut path = Vec::new();
        let root = loop {
            if let Some(&r) = root_of.get(&at) {
                break Some(r);
            }
            path.push(at);
            match parent.get(&at) {
                Some(&p) if made.contains_key(&p) => at = p,
                Some(&p) if exists(p) => break Some(at),
                // detached, or under a view neither made nor there
                _ => break None,
            }
        };
        if let Some(root) = root {
            for v in path {
                root_of.insert(v, root);
            }
        }
    }
    // Trees that may wait: none of their views moved or of a kind that
    // may not wait, and nothing outside them names them.
    let mut ok: FxHashMap<NodeId, bool> = FxHashMap::default();
    for (&id, &root) in &root_of {
        let fine = may_wait(made[&id]) && !moved.contains(&id);
        *ok.entry(root).or_insert(true) &= fine;
    }
    let mut frame: FxHashMap<NodeId, nsh_core::Bounds<nsh_core::Points>> = FxHashMap::default();
    for item in items {
        let owner = own(item).and_then(|o| root_of.get(&o).copied());
        if let (MountItem::Frame { id, frame: f }, Some(_)) = (item, owner)
            && root_of.get(id) == Some(id)
        {
            frame.insert(*id, *f);
        }
        if matches!(
            item,
            MountItem::Command { .. }
                | MountItem::Remove { .. }
                | MountItem::Delete { .. }
                | MountItem::ScrollBy { .. }
        ) && let Some(o) = owner
        {
            ok.insert(o, false);
        }
        named(item, |n| {
            // (a root's insert names its parent, which is no member)
            if let Some(&r) = root_of.get(&n)
                && owner != Some(r)
            {
                ok.insert(r, false);
            }
        });
    }
    let mut trees: FxHashMap<NodeId, Tree> = FxHashMap::default();
    for (&root, &fine) in &ok {
        let Some(&f) = frame.get(&root).filter(|_| fine) else {
            continue;
        };
        trees.insert(
            root,
            Tree {
                parent: parent[&root],
                root,
                frame: f,
                views: VecDeque::new(),
            },
        );
    }
    if trees.is_empty() {
        return None;
    }
    let mut waits = vec![false; items.len()];
    let mut per_view: FxHashMap<NodeId, Vec<MountItem>> = FxHashMap::default();
    for (k, item) in items.iter().enumerate() {
        let Some(o) = own(item) else { continue };
        if root_of.get(&o).is_some_and(|r| trees.contains_key(r)) {
            waits[k] = true;
            per_view.entry(o).or_default().push(item.clone());
        }
    }
    // Parents before children, siblings in their insert order.
    let depth = |mut id: NodeId| {
        let mut d = 0;
        while let Some(&p) = parent.get(&id).filter(|_| root_of.get(&id) != Some(&id)) {
            id = p;
            d += 1;
        }
        d
    };
    let mut views: Vec<(usize, usize, NodeId)> = per_view
        .keys()
        .map(|&id| (depth(id), order.get(&id).copied().unwrap_or(usize::MAX), id))
        .collect();
    views.sort_unstable();
    for (_, _, id) in views {
        let root = root_of[&id];
        let items = per_view.remove(&id).unwrap_or_default();
        if let Some(t) = trees.get_mut(&root) {
            t.views.push_back((id, items));
        }
    }
    let mut trees: Vec<Tree> = trees.into_values().collect();
    trees.sort_by_key(|t| order.get(&t.root).copied().unwrap_or(usize::MAX));
    Some(Plan { waits, trees })
}

/// Trees waiting to be applied, in the order their batches came.
#[derive(Default)]
pub(crate) struct Pacer {
    trees: VecDeque<Tree>,
    /// Every view of a waiting tree not applied yet, and its root.
    waiting: FxHashMap<NodeId, NodeId>,
}

impl Pacer {
    pub(crate) fn is_empty(&self) -> bool {
        self.trees.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.trees.clear();
        self.waiting.clear();
    }

    pub(crate) fn push(&mut self, tree: Tree) {
        for (id, _) in &tree.views {
            self.waiting.insert(*id, tree.root);
        }
        self.trees.push_back(tree);
    }

    /// The roots of the waiting trees `items` name: what must be applied
    /// before them.
    pub(crate) fn named_by(&self, items: &[MountItem]) -> Vec<NodeId> {
        let mut roots = Vec::new();
        if self.waiting.is_empty() {
            return roots;
        }
        for item in items {
            named(item, |n| {
                if let Some(&r) = self.waiting.get(&n)
                    && !roots.contains(&r)
                {
                    roots.push(r);
                }
            });
        }
        roots
    }

    /// The waiting trees: root, parent, frame, and whether the root was
    /// applied.
    pub(crate) fn trees(&self) -> impl Iterator<Item = &Tree> {
        self.trees.iter()
    }

    /// Takes the next view of tree `root` to apply, and its items; `None`
    /// once none is left (the tree is dropped).
    pub(crate) fn next(&mut self, root: NodeId) -> Option<(NodeId, Vec<MountItem>)> {
        let at = self.trees.iter().position(|t| t.root == root)?;
        let view = self.trees[at].views.pop_front();
        match &view {
            Some((id, _)) => {
                self.waiting.remove(id);
            }
            None => {
                self.trees.remove(at);
            }
        }
        if self
            .trees
            .get(at)
            .is_some_and(|t| t.root == root && t.views.is_empty())
        {
            self.trees.remove(at);
        }
        view
    }

    /// Drops tree `root` (its parent went).
    pub(crate) fn drop_tree(&mut self, root: NodeId) {
        if let Some(at) = self.trees.iter().position(|t| t.root == root) {
            for (id, _) in &self.trees[at].views {
                self.waiting.remove(id);
            }
            self.trees.remove(at);
        }
    }
}

/// Applies views of waiting trees: every tree `shows` (a tree's root, or
/// where it goes) finds in view whole, then, with a `budget`, views of the
/// rest a level at a time, near ones first, until the budget or
/// [`MAX_VIEWS`] is spent (at least one view). `apply` applies one view's
/// items; `alive` tells whether a view exists (a tree whose parent went is
/// dropped).
pub(crate) fn run(
    pacer: &mut Pacer,
    budget: Option<Duration>,
    shows: impl Fn(&Tree) -> (bool, bool),
    alive: impl Fn(NodeId) -> bool,
    mut apply: impl FnMut(Vec<MountItem>),
) {
    let start = Instant::now();
    let gone: Vec<NodeId> = pacer
        .trees()
        .filter(|t| !alive(t.parent))
        .map(|t| t.root)
        .collect();
    for root in gone {
        pacer.drop_tree(root);
    }
    let seen: Vec<(NodeId, bool, bool)> = pacer
        .trees()
        .map(|t| {
            let (shown, near) = shows(t);
            (t.root, shown, near)
        })
        .collect();
    for &(root, shown, _) in &seen {
        if shown {
            while let Some((_, items)) = pacer.next(root) {
                apply(items);
            }
        }
    }
    let Some(budget) = budget else { return };
    let mut views = 0;
    let order = seen
        .iter()
        .filter(|s| !s.1 && s.2)
        .chain(seen.iter().filter(|s| !s.1 && !s.2));
    for &(root, ..) in order {
        while views == 0 || (views < MAX_VIEWS && start.elapsed() < budget) {
            let Some((_, items)) = pacer.next(root) else {
                break;
            };
            apply(items);
            views += 1;
        }
        if views >= MAX_VIEWS || start.elapsed() >= budget {
            break;
        }
    }
}

/// Applies the trees `roots` whole (a batch names them).
pub(crate) fn flush(pacer: &mut Pacer, roots: &[NodeId], mut apply: impl FnMut(Vec<MountItem>)) {
    for &root in roots {
        while let Some((_, items)) = pacer.next(root) {
            apply(items);
        }
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::{
        Bounds,
        prelude::{point, px, size},
    };

    use super::*;

    fn frame(y: f32) -> Bounds<nsh_core::Points> {
        Bounds {
            origin: point(px(0.0), px(y)),
            size: size(px(400.0), px(40.0)),
        }
    }

    /// A row of `n` texts under `parent` (`id` and up), as the core sends
    /// it: creates, inserts, then frames.
    fn row(parent: NodeId, id: NodeId, n: u32, y: f32) -> Vec<MountItem> {
        let mut items = vec![MountItem::Create {
            id,
            kind: ViewKind::View,
        }];
        for k in 1..=n {
            items.push(MountItem::Create {
                id: id + 2 * k,
                kind: ViewKind::Text,
            });
            items.push(MountItem::Insert {
                parent: id,
                child: id + 2 * k,
                before: None,
            });
        }
        items.push(MountItem::Insert {
            parent,
            child: id,
            before: None,
        });
        items.push(MountItem::Frame {
            id,
            frame: frame(y),
        });
        for k in 1..=n {
            items.push(MountItem::Frame {
                id: id + 2 * k,
                frame: frame(k as f32 * 20.0),
            });
        }
        items
    }

    #[test]
    fn a_small_batch_is_applied_whole() {
        assert!(plan(&row(1, 101, 10, 0.0), |id| id == 1).is_none());
    }

    #[test]
    fn a_big_row_waits_parents_first() {
        let items = row(1, 101, 100, 2000.0);
        let plan = plan(&items, |id| id == 1).expect("a plan");
        assert!(plan.waits.iter().all(|w| *w), "all of the row waits");
        let [tree] = &plan.trees[..] else {
            panic!("one tree")
        };
        assert_eq!(
            (tree.parent, tree.root, tree.frame),
            (1, 101, frame(2000.0))
        );
        assert_eq!(tree.views.len(), 101);
        // The root first, with its create, insert and frame in order; then
        // its texts in the order they went in.
        let (id, root_items) = &tree.views[0];
        assert_eq!(*id, 101);
        assert!(matches!(
            root_items[..],
            [
                MountItem::Create { .. },
                MountItem::Insert { .. },
                MountItem::Frame { .. }
            ]
        ));
        let texts: Vec<NodeId> = tree.views.iter().skip(1).map(|v| v.0).collect();
        assert_eq!(texts, (1..=100).map(|k| 101 + 2 * k).collect::<Vec<_>>());
    }

    #[test]
    fn a_row_another_view_names_does_not_wait() {
        let mut items = row(1, 101, 100, 2000.0);
        // a view that exists goes in before the row's root
        items.push(MountItem::Insert {
            parent: 1,
            child: 7,
            before: Some(101),
        });
        assert!(plan(&items, |id| id == 1 || id == 7).is_none());
        // a command at one of its views
        let mut items = row(1, 101, 100, 2000.0);
        items.push(MountItem::Command {
            id: 103,
            command: nsh_core::Command::Focus,
        });
        assert!(plan(&items, |id| id == 1).is_none());
    }

    #[test]
    fn a_layout_animation_keeps_the_batch_whole() {
        let mut items = vec![MountItem::LayoutAnimation(Default::default())];
        items.extend(row(1, 101, 100, 2000.0));
        assert!(plan(&items, |id| id == 1).is_none());
    }

    #[test]
    fn what_shows_comes_at_once_the_rest_a_view_at_a_time() {
        let mut pacer = Pacer::default();
        let mut items = row(1, 101, 100, 0.0);
        items.extend(row(1, 1001, 100, 2000.0));
        for t in plan(&items, |id| id == 1).expect("a plan").trees {
            pacer.push(t);
        }
        let mut applied = Vec::new();
        // the first row is in view: whole; the second gets one view (a
        // budget of nothing still applies one)
        run(
            &mut pacer,
            Some(Duration::ZERO),
            |t| (t.frame.origin.y.0 < 800.0, false),
            |_| true,
            |items| applied.push(own(&items[0]).unwrap()),
        );
        assert_eq!(applied.len(), 102);
        assert_eq!(applied[101], 1001);
        // a batch naming the second row's text applies the row first
        let roots = pacer.named_by(&[MountItem::Frame {
            id: 1003,
            frame: frame(10.0),
        }]);
        assert_eq!(roots, [1001]);
        flush(&mut pacer, &roots, |items| {
            applied.push(own(&items[0]).unwrap())
        });
        assert_eq!(applied.len(), 202);
        assert!(pacer.is_empty());
    }

    #[test]
    fn a_tree_whose_parent_went_is_dropped() {
        let mut pacer = Pacer::default();
        for t in plan(&row(1, 101, 100, 2000.0), |id| id == 1).unwrap().trees {
            pacer.push(t);
        }
        let mut n = 0;
        run(
            &mut pacer,
            Some(BUDGET),
            |_| (false, false),
            |_| false,
            |_| n += 1,
        );
        assert_eq!(n, 0);
        assert!(pacer.is_empty());
    }
}

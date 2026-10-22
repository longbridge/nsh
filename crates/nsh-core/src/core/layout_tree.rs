//! The core's layout tree: Taffy's algorithms (flexbox, grid, block, out of
//! flow) run over nodes the core keeps, through Taffy's low-level traits,
//! as `TaffyTree` runs them over its own (`TaffyView`, which this follows
//! call for call). What the core does differently is its own:
//!
//! - **dirty marking goes up to the root, always.** `TaffyTree::mark_dirty`
//!   stops at the first node whose cache is already empty, taking its
//!   ancestors for dirty too; a node of a `display: none` subtree or an
//!   absolutely positioned one always has an empty cache while the nodes
//!   above keep theirs, so a change there was never laid out.
//! - **no hoisted children.** Taffy records, on each containing block, the
//!   absolutely positioned nodes it laid out, for its rounding pass alone;
//!   the core rounds layouts itself (`core::rounded`), so nothing is kept,
//!   and nothing can go stale.
//! - **no rounding**: [`LayoutTree::unrounded_layout`] is what the
//!   algorithms computed.

use slab::Slab;
use taffy::{
    AvailableSpace, BlockContext, Cache, CacheTree, DetailedGridInfo, DetailedLayoutInfo, Display,
    Layout, LayoutBlockContainer, LayoutContainingBlock, LayoutFlexboxContainer,
    LayoutGridContainer, LayoutInput, LayoutOutput, LayoutPartialTree, NodeId, RunMode, Size,
    Style, TraversePartialTree, TraverseTree, compute_block_layout, compute_cached_layout,
    compute_flexbox_layout, compute_grid_layout, compute_hidden_layout, compute_oof_layout,
    compute_root_layout,
};

/// A node: its style, place in the tree, cache and layout, and what
/// measures it when it is a leaf.
struct LayoutNode<C> {
    style: Style,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    cache: Cache,
    unrounded: Layout,
    detailed: DetailedLayoutInfo,
    context: Option<C>,
}

impl<C> LayoutNode<C> {
    fn new(style: Style, context: Option<C>) -> Self {
        LayoutNode {
            style,
            parent: None,
            children: Vec::new(),
            cache: Cache::new(),
            unrounded: Layout::new(),
            detailed: DetailedLayoutInfo::None,
            context,
        }
    }
}

/// The layout nodes of a core, each with a context `C` its leaves are
/// measured with.
pub(crate) struct LayoutTree<C> {
    nodes: Slab<LayoutNode<C>>,
}

fn key(node: NodeId) -> usize {
    usize::from(node)
}

impl<C> LayoutTree<C> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        LayoutTree {
            nodes: Slab::with_capacity(capacity),
        }
    }

    /// A new detached node.
    pub(crate) fn new_leaf(&mut self, style: Style) -> NodeId {
        NodeId::from(self.nodes.insert(LayoutNode::new(style, None)))
    }

    /// A new detached node measured with `context`.
    pub(crate) fn new_leaf_with_context(&mut self, style: Style, context: C) -> NodeId {
        NodeId::from(self.nodes.insert(LayoutNode::new(style, Some(context))))
    }

    /// A new detached node holding `children` (detached nodes).
    pub(crate) fn new_with_children(&mut self, style: Style, children: &[NodeId]) -> NodeId {
        let id = NodeId::from(self.nodes.insert(LayoutNode::new(style, None)));
        for &child in children {
            self.nodes[key(child)].parent = Some(id);
        }
        self.nodes[key(id)].children = children.to_vec();
        id
    }

    /// Frees `node`, detaching it from its parent; its children are left
    /// without one.
    pub(crate) fn remove(&mut self, node: NodeId) {
        if let Some(parent) = self.nodes[key(node)].parent {
            self.nodes[key(parent)].children.retain(|&c| c != node);
            self.mark_dirty(parent);
        }
        let removed = self.nodes.remove(key(node));
        for child in removed.children {
            self.nodes[key(child)].parent = None;
        }
    }

    pub(crate) fn style(&self, node: NodeId) -> &Style {
        &self.nodes[key(node)].style
    }

    /// Replaces `node`'s style.
    pub(crate) fn set_style(&mut self, node: NodeId, style: Style) {
        self.nodes[key(node)].style = style;
        self.mark_dirty(node);
    }

    /// Replaces what measures `node`.
    pub(crate) fn set_node_context(&mut self, node: NodeId, context: Option<C>) {
        self.nodes[key(node)].context = context;
        self.mark_dirty(node);
    }

    pub(crate) fn get_node_context_mut(&mut self, node: NodeId) -> Option<&mut C> {
        self.nodes[key(node)].context.as_mut()
    }

    pub(crate) fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.nodes[key(node)].parent
    }

    #[cfg(test)]
    pub(crate) fn children(&self, node: NodeId) -> &[NodeId] {
        &self.nodes[key(node)].children
    }

    /// Appends `child` (detached) to `parent`'s children.
    pub(crate) fn add_child(&mut self, parent: NodeId, child: NodeId) {
        let index = self.nodes[key(parent)].children.len();
        self.insert_child_at_index(parent, index, child);
    }

    /// Inserts `child` (detached) into `parent` at `index`.
    pub(crate) fn insert_child_at_index(&mut self, parent: NodeId, index: usize, child: NodeId) {
        self.nodes[key(child)].parent = Some(parent);
        self.nodes[key(parent)].children.insert(index, child);
        self.mark_dirty(parent);
    }

    /// Detaches `child` from `parent`.
    pub(crate) fn remove_child(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[key(parent)].children.retain(|&c| c != child);
        self.nodes[key(child)].parent = None;
        self.mark_dirty(parent);
    }

    /// Clears the cached layouts of `node` and of every node above it, up
    /// to the root (never stopping at a node whose cache is already empty:
    /// see the module's docs).
    pub(crate) fn mark_dirty(&mut self, node: NodeId) {
        let mut at = Some(node);
        while let Some(node) = at {
            let n = &mut self.nodes[key(node)];
            n.cache.clear();
            at = n.parent;
        }
    }

    /// Whether `node` has no cached layout: it changed, or something in it
    /// did, since it was last laid out.
    pub(crate) fn dirty(&self, node: NodeId) -> bool {
        self.nodes[key(node)].cache.is_empty()
    }

    /// `node`'s layout as the algorithms computed it, relative to its
    /// parent, unrounded.
    pub(crate) fn unrounded_layout(&self, node: NodeId) -> &Layout {
        &self.nodes[key(node)].unrounded
    }

    /// Lays out the tree under `root` within `available`, measuring leaves
    /// with `measure` (`taffy::TaffyTree::compute_layout_with_measure`'s).
    pub(crate) fn compute_layout_with_measure<M>(
        &mut self,
        root: NodeId,
        available: Size<AvailableSpace>,
        measure: M,
    ) where
        M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
    {
        let mut view = View {
            tree: self,
            measure,
        };
        compute_root_layout(&mut view, root, available);
    }
}

/// The tree being laid out, with what measures its leaves: what Taffy's
/// algorithms run over.
struct View<'t, C, M> {
    tree: &'t mut LayoutTree<C>,
    measure: M,
}

impl<C, M> View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    fn node(&self, node: NodeId) -> &LayoutNode<C> {
        &self.tree.nodes[key(node)]
    }

    /// What both `compute_child_layout` and `compute_block_child_layout`
    /// run (`TaffyView::compute_child_layout`).
    fn compute_child_layout(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        // An ancestor is `display: none`: hidden whatever its own display.
        if inputs.run_mode == RunMode::PerformHiddenLayout {
            return compute_hidden_layout(self, node);
        }
        compute_cached_layout(self, node, inputs, |view, node, inputs| {
            let display = view.node(node).style.display;
            let has_children = !view.node(node).children.is_empty();
            let mut output = match (display, has_children) {
                (Display::None, _) => compute_hidden_layout(view, node),
                (Display::Block, true) => compute_block_layout(view, node, inputs, block),
                (Display::FlowRoot, true) => compute_block_layout(view, node, inputs, None),
                (Display::Flex, true) => compute_flexbox_layout(view, node, inputs),
                (Display::Grid, true) => compute_grid_layout(view, node, inputs),
                (_, false) => {
                    let n = &mut view.tree.nodes[key(node)];
                    (view.measure)(inputs, node, n.context.as_mut(), &n.style)
                }
            };
            // The out-of-flow nodes this one is the containing block of, in
            // full layout passes only, as Taffy's tree does.
            if inputs.run_mode == RunMode::PerformLayout {
                compute_oof_layout(view, node, &mut output);
            }
            output
        })
    }
}

impl<C, M> TraversePartialTree for View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    type ChildIter<'a>
        = std::iter::Copied<std::slice::Iter<'a, NodeId>>
    where
        Self: 'a;

    fn child_ids(&self, parent: NodeId) -> Self::ChildIter<'_> {
        self.node(parent).children.iter().copied()
    }

    fn child_count(&self, parent: NodeId) -> usize {
        self.node(parent).children.len()
    }

    fn get_child_id(&self, parent: NodeId, index: usize) -> NodeId {
        self.node(parent).children[index]
    }
}

impl<C, M> TraverseTree for View<'_, C, M> where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput
{
}

impl<C, M> LayoutPartialTree for View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;

    // `Style`'s default, Taffy's `DefaultCheapStr` with `std`.
    type CustomIdent = String;

    fn get_core_container_style(&self, node: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.node(node).style
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        self.tree.nodes[key(node)].unrounded = *layout;
    }

    fn resolve_calc_value(&self, _: *const (), _: f32) -> f32 {
        0.0
    }

    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
        View::compute_child_layout(self, node, inputs, None)
    }
}

impl<C, M> LayoutContainingBlock for View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    type OofItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_oof_item_style(&self, node: NodeId) -> Self::OofItemStyle<'_> {
        &self.node(node).style
    }

    // Kept for Taffy's rounding pass alone, which the core does not run.
    fn clear_hoisted_children(&mut self, _: NodeId) {}

    fn add_hoisted_children(&mut self, _: NodeId, _: &[NodeId]) {}

    fn get_detailed_layout_info(&self, node: NodeId) -> &DetailedLayoutInfo {
        &self.node(node).detailed
    }
}

impl<C, M> CacheTree for View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    fn cache_get(&mut self, node: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.tree.nodes[key(node)].cache.get(input)
    }

    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.tree.nodes[key(node)].cache.store(input, output);
    }

    fn cache_clear(&mut self, node: NodeId) {
        self.tree.nodes[key(node)].cache.clear();
    }
}

impl<C, M> LayoutBlockContainer for View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_block_container_style(&self, node: NodeId) -> Self::BlockContainerStyle<'_> {
        &self.node(node).style
    }

    fn get_block_child_style(&self, child: NodeId) -> Self::BlockItemStyle<'_> {
        &self.node(child).style
    }

    fn compute_block_child_layout(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        View::compute_child_layout(self, node, inputs, block)
    }
}

impl<C, M> LayoutFlexboxContainer for View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, node: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.node(node).style
    }

    fn get_flexbox_child_style(&self, child: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.node(child).style
    }
}

impl<C, M> LayoutGridContainer for View<'_, C, M>
where
    M: FnMut(LayoutInput, NodeId, Option<&mut C>, &Style) -> LayoutOutput,
{
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_grid_container_style(&self, node: NodeId) -> Self::GridContainerStyle<'_> {
        &self.node(node).style
    }

    fn get_grid_child_style(&self, child: NodeId) -> Self::GridItemStyle<'_> {
        &self.node(child).style
    }

    fn set_detailed_grid_info(&mut self, node: NodeId, info: DetailedGridInfo) {
        self.tree.nodes[key(node)].detailed = DetailedLayoutInfo::Grid(Box::new(info));
    }
}

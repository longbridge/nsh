//! Components that run once ([`crate::ui::Component::view`]) in the core:
//! their views are mounted with bindings, and what changes later arrives as
//! patches.
//!
//! The view is expanded once, under the component's owner; an element's
//! bound parts (a text, props or a style given as a signal or a closure)
//! become render effects. Each run of one queues a patch with the part's
//! new value; the core applies it against what it applied last (field by
//! field), so a change sends only what changed.
//! A region ([`crate::ui::dynamic`], `If`, `Each`) is an effect queueing its
//! new elements; the core reconciles that region's nodes alone, in place
//! among its parent's other children. Patches and the events components emit
//! are taken at the start of each commit round, until none is left.

use std::{cell::RefCell, rc::Rc};

use super::{
    Core,
    components::{Classes, Expanded, Instance, Part, Placement},
};
use crate::{
    NodeId, Op, Tag, TextEngine,
    ui::{
        Attrs, BoundListConfig, ElNode, ListConfig, SharedString, StyleRefinement,
        element::BoundAttrs,
        flow::{Region, RegionElement},
        reactive::render_effect,
    },
};

/// A change a binding of a view queued.
pub(super) enum Patch {
    /// A text's new content.
    Text {
        host: NodeId,
        id: NodeId,
        text: SharedString,
    },
    /// An element's props, the bound ones read again.
    Attrs {
        host: NodeId,
        id: NodeId,
        attrs: Box<Attrs>,
    },
    /// An element's style, the bound parts read again.
    Style {
        host: NodeId,
        id: NodeId,
        style: StyleRefinement,
    },
    /// A region's new elements.
    Region {
        region: u32,
        elements: Vec<RegionElement>,
    },
    /// A list's layout, its bound parts read again.
    Layout { list: NodeId, config: ListConfig },
    /// The styles a view gives a nested component (its host), the bound
    /// parts read again.
    Nested {
        host: NodeId,
        style: StyleRefinement,
    },
    /// A list's rows, its bound count or keys read again.
    Rows {
        list: NodeId,
        count: u32,
        keys: Option<Rc<[u64]>>,
    },
}

/// The queue the bindings of views push to.
pub(super) type Patches = Rc<RefCell<Vec<Patch>>>;

/// What was last applied to a node with bindings.
pub(super) struct BoundNode {
    attrs: Attrs,
    pub(super) style: StyleRefinement,
    pub(super) classes: Classes,
    /// Its text's content and raw text node.
    text: Option<(SharedString, NodeId)>,
}

/// A region of a parent's children.
pub(super) struct RegionState {
    host: NodeId,
    parent: NodeId,
    /// What its nodes go before.
    anchor: Anchor,
    /// Its elements, as applied.
    children: Vec<Expanded>,
}

/// What a region's nodes go before in their parent.
#[derive(Debug, Clone, Copy)]
pub(super) enum Anchor {
    /// The end.
    End,
    Node(NodeId),
    /// The next region's first node, or what it goes before.
    Region(u32),
}

/// The bound parts of an element, taken off it before its static parts are
/// applied.
pub(super) struct Bound {
    text: Option<Rc<dyn Fn() -> SharedString>>,
    attrs: Vec<BoundAttrs>,
    /// The style with its bound parts (none when it has none).
    style: Option<StyleRefinement>,
    /// A list's layout: its static part and its bound parts.
    list: (ListConfig, Vec<BoundListConfig>),
}

impl Bound {
    /// The bound parts of `el`, if it has any (they leave it: what remains
    /// is static).
    pub(super) fn take(el: &mut ElNode) -> Option<Self> {
        if !el.is_bound() {
            return None;
        }
        Some(Self {
            text: el.bound_text.take(),
            attrs: std::mem::take(&mut el.bound_attrs),
            style: el.style.is_bound().then(|| {
                let full = std::mem::take(&mut el.style);
                el.style = full.static_part();
                full
            }),
            list: (
                el.list_config().copied().unwrap_or_default(),
                std::mem::take(&mut el.bound_list),
            ),
        })
    }
}

/// A binding's runs: the first one's value, or a patch for each later one.
///
/// A render effect runs as it is made, so its first value is read while
/// the node is being made and applied with its static parts, once; only what
/// changes later goes through the patch queue.
struct Runs<T> {
    first: Rc<RefCell<Option<T>>>,
    started: std::cell::Cell<bool>,
}

impl<T> Runs<T> {
    fn new() -> (Self, Rc<RefCell<Option<T>>>) {
        let first = Rc::new(RefCell::new(None));
        (
            Self {
                first: first.clone(),
                started: std::cell::Cell::new(false),
            },
            first,
        )
    }

    /// The value of a run: kept if it is the first, else `patch`ed.
    fn run(&self, value: T, patch: impl FnOnce(T)) {
        if self.started.replace(true) {
            patch(value);
        } else {
            *self.first.borrow_mut() = Some(value);
        }
    }
}

impl<M: TextEngine> Core<M> {
    /// Binds the bound parts of node `id` (of `host`'s view) under the
    /// current owner and puts what they read now into `el`, whose parts are
    /// then applied together; their later runs queue patches against what
    /// [`Self::keep_bound`] records.
    pub(super) fn bind_parts(&mut self, host: NodeId, id: NodeId, bound: Bound, el: &mut ElNode) {
        let patches = self.components.patches.clone();
        if let Some(text) = bound.text {
            let patches = patches.clone();
            let (runs, first) = Runs::new();
            render_effect(move || {
                runs.run(text(), |text| {
                    patches.borrow_mut().push(Patch::Text { host, id, text });
                });
            });
            if let Some(text) = first.take() {
                el.text = Some(text);
            }
        }
        if !bound.attrs.is_empty() {
            let base = el.attrs.clone();
            let parts = bound.attrs;
            let patches = patches.clone();
            let (runs, first) = Runs::new();
            render_effect(move || {
                let mut attrs = base.clone();
                for part in &parts {
                    part(&mut attrs);
                }
                runs.run(attrs, |attrs| {
                    patches.borrow_mut().push(Patch::Attrs {
                        host,
                        id,
                        attrs: Box::new(attrs),
                    });
                });
            });
            if let Some(attrs) = first.take() {
                el.attrs = attrs;
            }
        }
        if let Some(style) = bound.style {
            let (runs, first) = Runs::new();
            render_effect(move || {
                runs.run(style.resolved(), |style| {
                    patches.borrow_mut().push(Patch::Style { host, id, style });
                });
            });
            if let Some(style) = first.take() {
                el.style = style;
            }
        }
        let (base, parts) = bound.list;
        if !parts.is_empty() {
            let config = self.bind_list_layout(id, base, parts);
            if let Some(c) = el.list_config() {
                *c = config;
            }
        }
    }

    /// Records what was applied to node `id` of a view with bindings (its
    /// parts with the bindings' first values): what their patches apply
    /// against.
    pub(super) fn keep_bound(
        inst: &mut Instance,
        id: NodeId,
        attrs: &Attrs,
        style: &StyleRefinement,
        classes: Classes,
        text: Option<(SharedString, NodeId)>,
    ) {
        inst.bound.insert(
            id,
            BoundNode {
                attrs: attrs.clone(),
                style: style.clone(),
                classes,
                text,
            },
        );
    }

    /// Binds the styles a view gives nested component `host` under the
    /// current owner: the styles now, to apply; later changes are patches.
    pub(super) fn bind_nested_style(
        &mut self,
        host: NodeId,
        style: StyleRefinement,
    ) -> StyleRefinement {
        let patches = self.components.patches.clone();
        let (runs, first) = Runs::new();
        render_effect(move || {
            runs.run(style.resolved(), |style| {
                patches.borrow_mut().push(Patch::Nested { host, style });
            });
        });
        first.take().unwrap_or_default()
    }

    /// Binds the bound parts of list `list`'s layout over `base` under the
    /// current owner: the layout now, to apply; later changes are patches.
    pub(super) fn bind_list_layout(
        &mut self,
        list: NodeId,
        base: ListConfig,
        parts: Vec<BoundListConfig>,
    ) -> ListConfig {
        let patches = self.components.patches.clone();
        let (runs, first) = Runs::new();
        render_effect(move || {
            let mut config = base;
            for part in &parts {
                part(&mut config);
            }
            runs.run(config, |config| {
                patches.borrow_mut().push(Patch::Layout { list, config });
            });
        });
        let config = first.take().unwrap_or(base);
        self.components.list_layouts.insert(list, config);
        config
    }

    /// Makes region `region` of `host`'s view in `parent` a binding under
    /// the current owner; returns its id (its nodes come with its first
    /// patch).
    pub(super) fn bind_region(&mut self, host: NodeId, parent: NodeId, region: Region) -> u32 {
        let id = self.components.next_region;
        self.components.next_region += 1;
        self.components.regions.insert(
            id,
            RegionState {
                host,
                parent,
                anchor: Anchor::End,
                children: Vec::new(),
            },
        );
        let patches = self.components.patches.clone();
        region.bind(Rc::new(move |elements| {
            patches.borrow_mut().push(Patch::Region {
                region: id,
                elements,
            });
        }));
        id
    }

    /// Sets what region `region`'s nodes go before.
    pub(super) fn set_region_anchor(&mut self, region: u32, anchor: Anchor) {
        if let Some(state) = self.components.regions.get_mut(&region) {
            state.anchor = anchor;
        }
    }

    /// The nodes of region `region` now, in order.
    pub(super) fn region_nodes(&self, region: u32) -> Vec<NodeId> {
        self.components
            .regions
            .get(&region)
            .map(|state| state.children.iter().flat_map(|e| self.placed(e)).collect())
            .unwrap_or_default()
    }

    /// The nodes an expanded element puts in its parent (a region its own).
    pub(super) fn placed(&self, e: &Expanded) -> Vec<NodeId> {
        match e.part {
            Part::Region(region) => self.region_nodes(region),
            _ => vec![e.id],
        }
    }

    /// The node a region's nodes go before (`None`: the end).
    fn anchor_node(&self, anchor: Anchor) -> Option<NodeId> {
        let mut anchor = anchor;
        loop {
            match anchor {
                Anchor::End => return None,
                Anchor::Node(id) => return Some(id),
                Anchor::Region(region) => {
                    let state = self.components.regions.get(&region)?;
                    if let Some(first) = self.region_nodes(region).first() {
                        return Some(*first);
                    }
                    anchor = state.anchor;
                }
            }
        }
    }

    /// Applies what the views' bindings queued and routes the events their
    /// components emitted, until nothing is left. Whether anything was.
    pub(super) fn drain_views(&mut self) -> bool {
        let mut any = false;
        loop {
            // The insets components read, as their screens have them now.
            for host in self.components.runtime.inset_hosts() {
                let insets = self.insets_of(host);
                self.components.runtime.set_insets(host, insets);
            }
            let emits = self.components.runtime.take_emits();
            let patches = std::mem::take(&mut *self.components.patches.borrow_mut());
            if emits.is_empty() && patches.is_empty() {
                return any;
            }
            any = true;
            for (host, event) in emits {
                self.route_emit(host, event);
            }
            for patch in patches {
                self.apply_patch(patch);
            }
        }
    }

    fn apply_patch(&mut self, patch: Patch) {
        match patch {
            Patch::Region { region, elements } => self.reconcile_region(region, elements),
            Patch::Rows { list, count, keys } => self.set_list_rows(list, count, keys),
            Patch::Nested { host, style } => {
                if self.is_host(host) {
                    let user = (!style.is_empty()).then(|| self.components.lowered(&style));
                    self.set_user(host, user);
                }
            }
            Patch::Layout { list, config } => {
                let Some(old) = self.components.list_layouts.get_mut(&list) else {
                    return;
                };
                let old = std::mem::replace(old, config);
                let mut props = Vec::new();
                config.changes(&old, &mut props);
                for prop in props {
                    let _ = self.apply_op(Op::SetProp { id: list, prop });
                }
            }
            Patch::Text { host, id, text } => {
                let Some(inst) = self.components.instances.get_mut(&host) else {
                    return;
                };
                let Some(node) = inst.bound.get_mut(&id) else {
                    return;
                };
                match &mut node.text {
                    Some((old, raw)) if *old != text => {
                        let raw = *raw;
                        *old = text.clone();
                        let _ = self.apply_op(Op::SetText {
                            id: raw,
                            text: text.as_str().to_owned(),
                        });
                    }
                    _ => {}
                }
            }
            Patch::Attrs { host, id, attrs } => {
                let Some(inst) = self.components.instances.get_mut(&host) else {
                    return;
                };
                let app = (id == host).then(|| inst.app_style.clone()).flatten();
                let Some(node) = inst.bound.get_mut(&id) else {
                    return;
                };
                let attrs = *attrs;
                let old = std::mem::replace(&mut node.attrs, attrs.clone());
                self.apply_attrs_over(id, &attrs, &old, app.as_ref());
            }
            Patch::Style { host, id, style } => {
                let Some(inst) = self.components.instances.get_mut(&host) else {
                    return;
                };
                let user = if id == host { inst.user.clone() } else { None };
                let Some(node) = inst.bound.get_mut(&id) else {
                    return;
                };
                let old_style = std::mem::replace(&mut node.style, style.clone());
                let old_classes = node.classes.clone();
                let classes = self.apply_styles(id, &style, &old_style, old_classes, user);
                if let Some(node) = self
                    .components
                    .instances
                    .get_mut(&host)
                    .and_then(|i| i.bound.get_mut(&id))
                {
                    node.classes = classes;
                }
            }
        }
    }

    /// Region `region`'s new elements, matched with the old ones as an
    /// element's children are (kept rows stay as they are; each element is
    /// applied under its owner), then put in place among the parent's other
    /// children, before the region's anchor.
    fn reconcile_region(&mut self, region: u32, elements: Vec<RegionElement>) {
        let Some(state) = self.components.regions.remove(&region) else {
            return;
        };
        let host = state.host;
        let Some(mut inst) = self.components.instances.remove(&host) else {
            return;
        };
        let parent = state.parent;
        // The JS children were placed by this region: placed again below.
        let old_place = inst.children_at;
        if state.children.iter().any(|e| e.part == Part::Children) {
            inst.children_at = Placement::None;
        }
        let new = elements
            .into_iter()
            .map(|e| (e.owner, e.element.0))
            .collect();
        let (out, placeholder) = self.match_children(parent, state.children, new, host, &mut inst);
        self.forget_released(host, &mut inst);
        // The JS children's place went with what the region released.
        if let Placement::At(at, _) = inst.children_at
            && at != host
            && !self.components.owners.contains_key(&at)
        {
            inst.children_at = Placement::None;
        }
        let anchor = self.anchor_node(state.anchor);
        if let Some(at) = placeholder {
            let after = out[at + 1..]
                .iter()
                .find(|e| e.id != 0)
                .map(|e| e.id)
                .or(anchor);
            inst.children_at = Placement::At(parent, after);
        }
        // In place: from the last, each before the next, from the anchor.
        let nodes: Vec<NodeId> = out
            .iter()
            .flat_map(|e| match e.part {
                Part::Children => inst.js_children.clone(),
                _ => self.placed(e),
            })
            .collect();
        self.children_placed(host, &mut inst, old_place);
        self.components.instances.insert(host, inst);
        self.components.regions.insert(
            region,
            RegionState {
                host,
                parent,
                anchor: state.anchor,
                children: out,
            },
        );
        // A nested component's content joins it as JS children do.
        let nested = parent != host && self.is_host(parent);
        let mut before = anchor;
        for &child in nodes.iter().rev() {
            if nested {
                if !self.nested_precedes(parent, child, before) {
                    self.join_nested_at(parent, child, before);
                }
            } else if !self.precedes(parent, child, before) {
                let _ = self.apply_op(Op::Insert {
                    parent,
                    child,
                    before,
                });
            }
            before = Some(child);
        }
    }

    /// Forgets what `host`'s view knew of the nodes released since: their
    /// handlers, responders and bindings.
    fn forget_released(&self, host: NodeId, inst: &mut Instance) {
        let live = |id: &NodeId| *id == host || self.components.owners.contains_key(id);
        inst.handlers.retain(|(id, _, _)| live(id));
        inst.event_handlers.retain(|(id, _)| live(id));
        inst.responders.retain(|(id, _)| live(id));
        inst.bound.retain(|id, _| live(id));
    }

    /// Releases the regions of `host`'s view (the host goes).
    pub(super) fn release_regions(&mut self, host: NodeId) {
        let regions: Vec<u32> = self
            .components
            .regions
            .iter()
            .filter(|(_, s)| s.host == host)
            .map(|(&id, _)| id)
            .collect();
        // Their nodes go too (a region inside another is released with it).
        for region in regions {
            self.release_region(region);
        }
    }

    /// Releases region `region`'s nodes (its placeholder goes).
    pub(super) fn release_region(&mut self, region: u32) {
        if let Some(state) = self.components.regions.remove(&region) {
            for e in state.children {
                self.release_expanded(e);
            }
        }
    }
}

/// A placeholder element for a region of `tag` (it has no node).
pub(super) fn region_placeholder(region: u32) -> Expanded {
    let mut e = Expanded::js(0, Part::Region(region));
    e.tag = Tag::View;
    e
}

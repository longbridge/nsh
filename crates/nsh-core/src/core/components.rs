//! Rust components in the core ([`crate::ui`]): creating instances,
//! expanding their renders into nodes, diffing renders, placing JS children
//! and slots, and routing events.
//!
//! A component's host node is the root of its expansion: the root element's
//! props and styles apply to it, its other elements become nodes with odd
//! ids (JS uses even ones) owned by the host. Ops from JS on a host are
//! intercepted ([`Core::apply`]): its `class` goes after the root's styles,
//! its children are placed where the render's `Children` is. Renders run at
//! the start of a commit, so props set in one JS turn render once.

use std::{
    any::{Any, TypeId},
    rc::Rc,
    sync::Arc,
};

use rustc_hash::FxHashMap;

use super::{Core, Kind, NO_KEY};
use crate::{
    ApplyError, Command, EdgeInsets, EventKind, EventMask, EventPayload, JsonValue, NodeId, Op,
    Prop, ROOT_ID, Tag, TextEngine,
    tailwind::ClassList,
    ui::{
        Action, AnswerSlot, AnyInstance, AsyncCx, Attrs, Child, ClipboardRead, ComponentError,
        ComponentSpec, ElKind, ElNode, Environment, ErasedCx, Executor, Function, FunctionCall,
        Globals, ImageSize, IntoElement, ListConfig, Listener, ModuleCx, ParentElement, PlaceFn,
        Position, PropError, Provided, Registry, RenderReads, RenderScope, Root, SharedString,
        StyleRefinement, Task, Theme, caught,
        responder::ResponderEl,
        view,
        virtual_list::{self, RowFn, SeparatorFn, VirtualListEl},
    },
};

/// Renders of one component per commit, at most (a component whose render
/// keeps asking for another one is cut off).
const MAX_RENDERS: u32 = 16;

/// Lowered styles kept for reuse (components render the same styles over
/// and over); cleared when full.
const STYLE_CACHE: usize = 4096;

/// Rounds of polls and updates per run of the tasks, at most.
const MAX_TASK_ROUNDS: u32 = 64;

/// What renders on a host node.
enum Inner {
    /// Nothing is registered under the name: renders nothing.
    Missing,
    Component(Box<dyn AnyInstance>),
    /// A list cell of a component's list: the component's row function
    /// wraps the row the app rendered.
    Row {
        place: PlaceFn,
        index: usize,
    },
    /// A cell of a component's own list ([`crate::ui::virtual_list`]): it
    /// renders row `index` with the list's row function.
    VirtualRow {
        list: NodeId,
        index: usize,
    },
}

/// A component's own list.
struct VirtualListState {
    /// The component whose render it is in (its rows' handlers are its).
    owner: NodeId,
    row: RowFn,
    count: u32,
    keys: Option<Rc<[u64]>>,
    separator: Option<SeparatorFn>,
    /// The cells, by recycling slot.
    cells: Vec<NodeId>,
    /// The rows of another component's `Items` prop
    /// ([`crate::ui::ComponentElement::items`]): each cell is that
    /// component's wrapper (`place`, its row function: an [`Inner::Row`])
    /// around a node rendering the row (an [`Inner::VirtualRow`], in
    /// `contents` by slot), so each part's handlers run with its own
    /// component.
    items: bool,
    place: Option<PlaceFn>,
    contents: Vec<NodeId>,
    /// Viewable rows: how long they must stay, the range reported last and
    /// the latest, and the report waiting for the time to pass.
    min_view: Option<std::time::Duration>,
    reported: Option<(u32, u32)>,
    latest: Option<(u32, u32)>,
    report: Option<Task<()>>,
}

/// Where a host's JS children are.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Placement {
    /// Out of the tree (the render has no place for them).
    None,
    /// In this node, before the rendered node after the render's
    /// `Children` (their anchor).
    At(NodeId, Option<NodeId>),
    /// Each in its own wrapper ([`ParentElement::children_each`]): placed
    /// by the render.
    Each,
}

/// A component instance on its host node.
pub(super) struct Instance {
    inner: Inner,
    spec: Option<Arc<ComponentSpec>>,
    /// The app's `class` and `classList` on the host.
    class: Box<str>,
    class_list: Vec<Box<str>>,
    /// The app's classes, or the styles of the component rendering this
    /// one: applied after the root's own.
    user: Option<Arc<ClassList>>,
    /// Events JS listens to on the host.
    js_events: EventMask,
    /// The last render, expanded (its root is the host).
    root: Option<Expanded>,
    /// The host's JS children, in order.
    js_children: Vec<NodeId>,
    children_at: Placement,
    /// Lazy children (`#[prop(lazy)]`): whether JS was told to make them.
    children_wanted: bool,
    /// The glue's node of each slot the app passes.
    slots: Vec<(&'static str, NodeId)>,
    /// The last render's handlers: native events by node, and the events of
    /// nested components by their host (a handful: lists beat maps).
    handlers: Vec<(NodeId, EventKind, Listener<EventPayload>)>,
    event_handlers: Vec<(NodeId, Listener<dyn Any>)>,
    /// The last render's parts in the touch responder, by node.
    responders: Vec<(NodeId, Rc<ResponderEl>)>,
    /// What the last render read of the environment.
    reads: RenderReads,
    /// The app's `style` on the host, applied after the root's own inline
    /// style and native values.
    app_style: Option<crate::StylePatch>,
    dirty: bool,
    renders: u32,
}

impl Instance {
    fn new(
        inner: Inner,
        spec: Option<Arc<ComponentSpec>>,
        js_children: Vec<NodeId>,
        app_style: Option<crate::StylePatch>,
    ) -> Self {
        Self {
            inner,
            spec,
            class: "".into(),
            class_list: Vec::new(),
            user: None,
            js_events: EventMask::default(),
            root: None,
            js_children,
            children_at: Placement::None,
            children_wanted: false,
            slots: Vec::new(),
            handlers: Vec::new(),
            event_handlers: Vec::new(),
            responders: Vec::new(),
            reads: RenderReads::default(),
            app_style,
            dirty: false,
            renders: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Node,
    /// A slot's node: JS's, never released by the core.
    Slot,
    /// Where the JS children are (no node of its own).
    Children,
    /// A nested component.
    Component(&'static str),
    /// A list of the component's own rows; its cells are the core's.
    VirtualList,
    /// One of the JS children (no node of its own).
    JsChild,
    /// A piece of a text's content: a raw text node.
    Raw,
}

/// The node's classes: its styles lowered, and what was merged after them.
type Classes = (Option<Arc<ClassList>>, Option<Arc<ClassList>>);

/// An element of a render, as it was applied.
struct Expanded {
    id: NodeId,
    tag: Tag,
    key: Option<SharedString>,
    /// A text's content and its raw text node.
    text: Option<(SharedString, NodeId)>,
    attrs: Attrs,
    style: StyleRefinement,
    classes: Classes,
    events: EventMask,
    /// A list's layout.
    list: ListConfig,
    part: Part,
    children: Vec<Expanded>,
}

impl Expanded {
    /// A node of JS's in the render (`id` 0: the place of the children).
    fn js(id: NodeId, part: Part) -> Self {
        Self {
            id,
            tag: Tag::View,
            key: None,
            text: None,
            attrs: Attrs::default(),
            style: StyleRefinement::default(),
            classes: (None, None),
            events: EventMask::default(),
            list: ListConfig::default(),
            part,
            children: Vec::new(),
        }
    }
}

/// What a component's task waits for from the animation graph.
enum Awaited {
    /// An animation's end: whether it finished.
    End(AnswerSlot<bool>),
    /// A value read ([`crate::ui::NativeValue::value`]).
    Value(AnswerSlot<f32>),
}

/// The core's component state.
pub(super) struct Components {
    registry: Registry,
    /// The specs of the components made by type (not by name from the
    /// registry), one per type.
    specs: FxHashMap<&'static str, Arc<ComponentSpec>>,
    theme: Arc<Theme>,
    /// The app's theme (`nsh.json`, a Rust app's root): rendered with while
    /// the embedding app sets none ([`Core::set_host_theme`]).
    app_theme: Arc<Theme>,
    /// The embedding app's theme: wins over [`Self::app_theme`].
    host_theme: Option<Arc<Theme>>,
    /// Boxed: a render takes its instance out of the map and back.
    pub(super) instances: FxHashMap<NodeId, Box<Instance>>,
    /// Expanded node -> the host of the component that rendered it.
    owners: FxHashMap<NodeId, NodeId>,
    /// Slot nodes (`componentSlot`) -> their slot's name.
    slot_names: FxHashMap<NodeId, Arc<str>>,
    /// Slot nodes in a host -> the host.
    slot_hosts: FxHashMap<NodeId, NodeId>,
    styles: FxHashMap<StyleRefinement, Arc<ClassList>>,
    /// The lists components place (the glue's, of `Items` props) -> the
    /// row function of the component's last render, and the number of rows
    /// its wrappers last saw.
    lists: FxHashMap<NodeId, (PlaceFn, u32)>,
    pub(super) environment: Environment,
    /// Runs the components' async tasks and timers.
    pub(super) executor: Rc<Executor>,
    /// State the components, modules and tasks share, by type (also the
    /// status bar's settings).
    pub(super) globals: Globals,
    /// Dialogs components show, by id: their answers.
    dialogs: FxHashMap<u32, AnswerSlot<i32>>,
    next_dialog: u32,
    /// Image sizes asked for (`Image.getSize`), by request id: their answers.
    image_infos: FxHashMap<u32, AnswerSlot<Result<ImageSize, String>>>,
    next_image_info: u32,
    /// Clipboard reads (`Clipboard.getString` / `hasString`), by request
    /// id: their answers.
    clipboard_reads: FxHashMap<u32, ClipboardRead>,
    next_clipboard_read: u32,
    /// The ends of the animations components started and the values they
    /// read, by id.
    animations: FxHashMap<nsh_animation::AnimationId, Awaited>,
    /// Elements to measure once this commit is laid out
    /// ([`crate::ui::ElementRef::measure`]).
    pub(super) measures: Vec<(NodeId, AnswerSlot<Option<crate::Measurement>>)>,
    /// The components' own lists, by node.
    virtual_lists: FxHashMap<NodeId, VirtualListState>,
    /// The insets screens and modals reported (`onInsets`).
    insets: FxHashMap<NodeId, EdgeInsets>,
    /// JS listens to the back button (`BackHandler`) on the root.
    js_back: bool,
    /// The touches the components hear and their responder.
    pub(super) touch: super::touch::TouchState,
    /// Display frames and the layout direction.
    pub(super) services: super::services::Services,
    next_id: NodeId,
    dirty: Vec<NodeId>,
    /// Reused buffers.
    rendered: Vec<NodeId>,
    props: Vec<Prop>,
}

impl Default for Components {
    fn default() -> Self {
        Self {
            registry: Registry::default(),
            specs: FxHashMap::default(),
            theme: Arc::new(Theme::default()),
            app_theme: Arc::new(Theme::default()),
            host_theme: None,
            instances: FxHashMap::default(),
            owners: FxHashMap::default(),
            slot_names: FxHashMap::default(),
            slot_hosts: FxHashMap::default(),
            styles: FxHashMap::default(),
            lists: FxHashMap::default(),
            environment: Environment::default(),
            executor: Rc::new(Executor::default()),
            globals: Globals::default(),
            dialogs: FxHashMap::default(),
            next_dialog: 1,
            image_infos: FxHashMap::default(),
            next_image_info: 1,
            clipboard_reads: FxHashMap::default(),
            next_clipboard_read: 1,
            animations: FxHashMap::default(),
            measures: Vec::new(),
            virtual_lists: FxHashMap::default(),
            insets: FxHashMap::default(),
            js_back: false,
            touch: super::touch::TouchState::default(),
            services: Default::default(),
            // Odd: JS allocates the even ids (1 is the root).
            next_id: 3,
            dirty: Vec::new(),
            rendered: Vec::new(),
            props: Vec::new(),
        }
    }
}

impl Components {
    pub(super) fn for_viewport(viewport: crate::Size) -> Self {
        let mut components = Self::default();
        components.environment.window = viewport;
        components
    }

    fn alloc(&mut self) -> NodeId {
        let id = self.next_id;
        self.next_id += 2;
        id
    }

    fn mark(&mut self, host: NodeId) {
        if let Some(inst) = self.instances.get_mut(&host)
            && !inst.dirty
        {
            inst.dirty = true;
            self.dirty.push(host);
        }
    }

    /// `style` as rules, shared between the elements that have it.
    fn lowered(&mut self, style: &StyleRefinement) -> Arc<ClassList> {
        if let Some(list) = self.styles.get(style) {
            return list.clone();
        }
        if self.styles.len() >= STYLE_CACHE {
            self.styles.clear();
        }
        let list = Arc::new(style.lower());
        self.styles.insert(style.clone(), list.clone());
        list
    }
}

/// The inline style of `id` from a JS style (an object or an array of
/// them; keys and values as in React Native). Values that are objects or
/// arrays (transforms, offsets) are not taken.
fn inline_style(
    id: NodeId,
    value: Option<&JsonValue>,
    animated: &[(crate::StyleDecl, crate::ui::NativeNode)],
    app: Option<&crate::StylePatch>,
) -> Op {
    fn walk(value: &JsonValue, style: &mut crate::Style, patch: &mut crate::StylePatch) {
        use crate::StyleValue as V;
        match value {
            JsonValue::Object(entries) => {
                for (key, value) in entries {
                    let v = match value {
                        JsonValue::Number(n) => V::Number(n.as_f64().unwrap_or(0.0) as f32),
                        JsonValue::String(s) => V::Str(s),
                        JsonValue::Null => V::Null,
                        _ => continue,
                    };
                    if patch.set(style, key, v).is_err() {
                        log::warn!(target: "js", "style `{key}`: not taken");
                    }
                }
            }
            JsonValue::Array(items) => {
                for item in items {
                    walk(item, style, patch);
                }
            }
            _ => {}
        }
    }
    let mut style = Box::<crate::Style>::default();
    let mut patch = crate::StylePatch::default();
    if let Some(value) = value {
        walk(value, &mut style, &mut patch);
    }
    // Native values over the rest, then (on a host) the app's style.
    let app = app.map(|p| p.decls()).unwrap_or_default();
    for decl in animated.iter().map(|(d, _)| d).chain(app) {
        let _ = patch.push(&mut style, decl.clone());
    }
    Op::SetStyle { id, style, patch }
}

fn same(a: &Option<Arc<ClassList>>, b: &Option<Arc<ClassList>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl<M: TextEngine> Core<M> {
    /// The components this core can render (from `nsh-ui-mobile` and the
    /// like); set before the first op.
    pub fn set_components(&mut self, registry: Registry) {
        self.components.registry = registry;
    }

    /// `uiTheme` on the root: the app's theme (`nsh.json`).
    pub(super) fn set_ui_theme(&mut self, json: &str) {
        match Theme::from_json(json) {
            Ok(theme) => self.set_theme(theme),
            Err(e) => log::warn!("theme: {e}"),
        }
    }

    /// The app's theme; components render with it unless the embedding
    /// app set one ([`Core::set_host_theme`]).
    pub fn set_theme(&mut self, theme: Theme) {
        self.components.app_theme = Arc::new(theme);
        self.theme_changed();
    }

    /// The embedding app's theme (`None`: the app's again): an app
    /// embedded in a native app looks like it, whatever theme the app
    /// itself sets.
    pub fn set_host_theme(&mut self, theme: Option<Theme>) {
        self.components.host_theme = theme.map(Arc::new);
        self.theme_changed();
    }

    /// The theme components render with now: every component renders
    /// again when it changed.
    fn theme_changed(&mut self) {
        let theme = self
            .components
            .host_theme
            .clone()
            .unwrap_or_else(|| self.components.app_theme.clone());
        if *theme == *self.components.theme {
            self.components.theme = theme;
            return;
        }
        self.components.theme = theme;
        let hosts: Vec<NodeId> = self.components.instances.keys().copied().collect();
        for host in hosts {
            self.components.mark(host);
        }
    }

    /// Mounts a Rust app's root component as the content of the root view;
    /// it renders in the next commit.
    pub fn mount_root(&mut self, root: &Root) {
        let mut el = root.element().0;
        let ElKind::Component(c) = std::mem::replace(&mut el.kind, ElKind::Plain) else {
            return log::error!("the root is not a component");
        };
        let id = self.components.alloc();
        let _ = self.apply_op(Op::Create { id, tag: el.tag });
        self.create_instance_of(id, c.name, c.make, c.spec);
        if let Some(inst) = self.components.instances.get_mut(&id)
            && let Inner::Component(inner) = &mut inst.inner
        {
            inner.set_props(c.props.as_any());
        }
        let _ = self.apply_op(Op::Insert {
            parent: ROOT_ID,
            child: id,
            before: None,
        });
        self.own_direction();
    }

    /// The environment components read ([`crate::ui::RenderCx::window`]
    /// and the like), as the platform reports it: the components that read
    /// what changed render again. `dark` is the system's: the app's
    /// appearance wins over it ([`Core::set_appearance`]).
    pub fn set_environment(&mut self, mut environment: Environment) {
        environment.rtl = self.surface_rtl(environment.rtl);
        self.system_dark = environment.dark;
        environment.dark = self.shown_dark();
        self.apply_environment(environment);
    }

    /// The environment components read is now `environment`.
    pub(super) fn apply_environment(&mut self, environment: Environment) {
        let old = self.components.environment;
        let changed = environment.changes(&old);
        self.components.environment = environment;
        if changed == 0 {
            return;
        }
        let hosts: Vec<NodeId> = self
            .components
            .instances
            .iter()
            .filter(|(_, i)| i.reads.env & changed != 0)
            .map(|(&h, _)| h)
            .collect();
        for host in hosts {
            let mut cx = self.erased_cx(host);
            let mut failed = None;
            if let Some(inst) = self.components.instances.get_mut(&host)
                && let Inner::Component(inner) = &mut inst.inner
            {
                failed = caught(|| inner.environment_changed(&old, &environment, &mut cx)).err();
            }
            self.finish_event(host, cx);
            self.components.mark(host);
            if let Some(message) = failed {
                self.component_failed(host, message);
            }
        }
    }

    /// The surface's size changed (the environment's window).
    pub(super) fn components_viewport(&mut self, size: crate::Size) {
        let environment = Environment {
            window: size,
            ..self.components.environment
        };
        self.apply_environment(environment);
    }

    /// Wakes the engine when a component's task is woken (from any
    /// thread): it then calls [`Self::run_component_tasks`].
    pub fn set_task_notify(&mut self, notify: Arc<dyn Fn() + Send + Sync>) {
        self.components.executor.set_notify(notify);
    }

    /// The tokio runtime the components' tasks are polled in the context
    /// of (their timers, IO and background work run on it).
    pub fn set_task_runtime(&mut self, runtime: tokio::runtime::Handle) {
        self.components.executor.set_runtime(runtime);
    }

    /// Runs the components' woken tasks, with the state changes they ask
    /// for, until they all wait.
    pub fn run_component_tasks(&mut self) {
        let executor = self.components.executor.clone();
        // A task that keeps waking itself cannot hold the engine.
        for _ in 0..MAX_TASK_ROUNDS {
            let polled = executor.poll_ready();
            let updates = executor.take_updates();
            let actions = executor.take_actions();
            if !polled && updates.is_empty() && actions.is_empty() {
                return;
            }
            for action in actions {
                self.apply_action(action);
            }
            for update in updates {
                let mut cx = self.erased_cx(update.host);
                let target = match self.components.instances.get_mut(&update.host) {
                    Some(inst) => match &mut inst.inner {
                        Inner::Component(inner) => Some(inner),
                        _ => None,
                    },
                    None => None,
                };
                match target {
                    Some(inner) => {
                        let (state, props) = inner.parts();
                        let failed = caught(|| (update.run)(Some((state, props, &mut cx)))).err();
                        self.finish_event(update.host, cx);
                        if let Some(message) = failed {
                            self.component_failed(update.host, message);
                        }
                    }
                    None => (update.run)(None),
                }
            }
            for (host, message) in executor.take_failures() {
                self.component_failed(host, message);
            }
        }
    }

    fn erased_cx(&self, host: NodeId) -> ErasedCx {
        ErasedCx::new(
            self.components.environment,
            host,
            self.components.executor.clone(),
            self.components.globals.clone(),
        )
    }

    /// Whether `id` is a component's host.
    fn is_host(&self, id: NodeId) -> bool {
        self.components.instances.contains_key(&id)
    }

    /// Ops from JS on a component's host take its place; others pass
    /// (`Some`).
    pub(super) fn intercept(&mut self, op: Op) -> Result<Option<Op>, ApplyError> {
        match op {
            // The app's style on a host goes after the root's own.
            Op::SetStyle { id, patch, .. } if self.is_host(id) => {
                let inst = self.components.instances.get_mut(&id).expect("host");
                inst.app_style = (!patch.is_empty()).then_some(patch);
                let (inline, animated) = match &inst.root {
                    Some(root) => (root.attrs.inline.clone(), root.attrs.animated.clone()),
                    None => (None, Vec::new()),
                };
                let app = inst.app_style.clone();
                Ok(Some(inline_style(
                    id,
                    inline.as_ref(),
                    &animated,
                    app.as_ref(),
                )))
            }
            // A slot's node joins its host: the render places it.
            Op::Insert { parent, child, .. }
                if self.is_host(parent) && self.components.slot_names.contains_key(&child) =>
            {
                let name = self.components.slot_names[&child].clone();
                let inst = self.components.instances.get_mut(&parent).expect("host");
                let Some(slot) = inst.spec.as_ref().and_then(|s| s.holder(&name)) else {
                    log::warn!(target: "js", "the component has no slot `{name}`");
                    self.detach_id(child);
                    return Ok(None);
                };
                inst.slots.retain(|(_, n)| *n != child);
                inst.slots.push((slot, child));
                self.components.slot_hosts.insert(child, parent);
                self.components.mark(parent);
                Ok(None)
            }
            Op::Remove { parent, child }
                if self.is_host(parent) && self.components.slot_names.contains_key(&child) =>
            {
                self.unslot(child);
                self.detach_id(child);
                Ok(None)
            }
            // A component's list gets a cell: the component wraps its rows.
            Op::Insert { parent, child, .. }
                if self.components.slot_hosts.contains_key(&parent) && self.is_list(parent) =>
            {
                self.apply_op(op)?;
                if let Some((place, _)) = self.components.lists.get(&parent).cloned() {
                    self.wrap_row(child, place);
                }
                Ok(None)
            }
            Op::Remove { parent, .. }
                if self.components.slot_hosts.contains_key(&parent) && self.is_list(parent) =>
            {
                Ok(Some(op))
            }
            // A slot filled or emptied renders its component again.
            Op::Insert { parent, .. } | Op::Remove { parent, .. }
                if self.components.slot_hosts.contains_key(&parent) =>
            {
                let empty = |core: &Self| {
                    core.key(parent)
                        .map_or(true, |k| core.nodes[k as usize].children.is_empty())
                };
                let before = empty(self);
                self.apply_op(op)?;
                if empty(self) != before {
                    let host = self.components.slot_hosts[&parent];
                    self.components.mark(host);
                }
                Ok(None)
            }
            Op::Release { id } if self.components.slot_names.contains_key(&id) => {
                self.unslot(id);
                self.components.slot_names.remove(&id);
                self.components.lists.remove(&id);
                Ok(Some(Op::Release { id }))
            }
            // The number of rows changed: the rows' wrappers render again
            // (they may depend on it: the last row).
            Op::SetProp {
                id,
                prop: Prop::Count(count),
            } if self.components.lists.contains_key(&id) => {
                let key = self.key(id)?;
                self.apply_op(Op::SetProp {
                    id,
                    prop: Prop::Count(count),
                })?;
                // A splice before may have changed the list's count already.
                let seen = &mut self.components.lists.get_mut(&id).expect("list").1;
                if std::mem::replace(seen, count) != count {
                    let cells: Vec<NodeId> = self.nodes[key as usize]
                        .children
                        .iter()
                        .map(|&c| self.nodes[c as usize].id)
                        .collect();
                    for cell in cells {
                        self.components.mark(cell);
                    }
                }
                Ok(None)
            }
            // A row cell shows another row: its wrapper renders again.
            Op::SetProp {
                id,
                prop: Prop::CellIndex(index),
            } if self.is_host(id) => {
                let inst = self.components.instances.get_mut(&id).expect("host");
                if let Inner::Row { index: current, .. } = &mut inst.inner
                    && *current != index as usize
                {
                    *current = index as usize;
                    self.components.mark(id);
                }
                Ok(Some(op))
            }
            Op::Insert {
                parent,
                child,
                before,
            } if self.is_host(parent) => {
                self.key(child)?;
                let inst = self.components.instances.get_mut(&parent).expect("host");
                let was_empty = inst.js_children.is_empty();
                inst.js_children.retain(|&c| c != child);
                let at = before.and_then(|b| inst.js_children.iter().position(|&c| c == b));
                inst.js_children
                    .insert(at.unwrap_or(inst.js_children.len()), child);
                let place = inst.children_at;
                if was_empty || place == Placement::Each {
                    self.components.mark(parent);
                }
                match place {
                    // Before its next JS sibling, else before what the
                    // render puts after the children.
                    Placement::At(p, anchor) => self.apply_op(Op::Insert {
                        parent: p,
                        child,
                        before: at.and(before).or(anchor),
                    })?,
                    // Its wrapper is the render's.
                    Placement::Each => self.detach_id(child),
                    Placement::None => self.detach_id(child),
                }
                Ok(None)
            }
            Op::Remove { parent, child } if self.is_host(parent) => {
                let inst = self.components.instances.get_mut(&parent).expect("host");
                inst.js_children.retain(|&c| c != child);
                if inst.js_children.is_empty() || inst.children_at == Placement::Each {
                    self.components.mark(parent);
                }
                self.detach_id(child);
                Ok(None)
            }
            Op::SetProp {
                id,
                prop: Prop::Class(class),
            } if self.is_host(id) => {
                let inst = self.components.instances.get_mut(&id).expect("host");
                inst.class = class;
                self.update_user_class(id);
                Ok(None)
            }
            Op::SetProp {
                id,
                prop: Prop::ClassList(list),
            } if self.is_host(id) => {
                let inst = self.components.instances.get_mut(&id).expect("host");
                inst.class_list = list;
                self.update_user_class(id);
                Ok(None)
            }
            // The root listens to the back button while JS or a component
            // does.
            Op::Listen {
                id: crate::ROOT_ID,
                event: EventKind::BackPress,
                enabled,
            } => {
                self.components.js_back = enabled;
                let rust = !self.components.executor.back.borrow().list.is_empty();
                Ok(Some(Op::Listen {
                    id: crate::ROOT_ID,
                    event: EventKind::BackPress,
                    enabled: enabled || rust,
                }))
            }
            // The component's events come from the core, not from the view.
            Op::Listen {
                id,
                event: EventKind::NativeEvent,
                ..
            } if self.is_host(id) => Ok(None),
            Op::Listen { id, event, enabled } if self.is_host(id) => {
                let inst = self.components.instances.get_mut(&id).expect("host");
                inst.js_events.set(event, enabled);
                let own = inst.root.as_ref().is_some_and(|r| r.events.contains(event));
                Ok(Some(Op::Listen {
                    id,
                    event,
                    enabled: enabled || own,
                }))
            }
            // A function of a module, on the root.
            Op::Command {
                id: crate::ROOT_ID,
                command: Command::Native { name, args },
            } if self.function(&name).is_some() => {
                self.call_function(&name, args);
                Ok(None)
            }
            // A method of the component's ref: `{ c, a }`, the call (0: no
            // reply) and the arguments.
            Op::Command {
                id,
                command: Command::Native { name, args },
            } if self.is_host(id) => {
                self.call_method(id, &name, args);
                Ok(None)
            }
            Op::Release { id } if self.is_host(id) => {
                self.release_instance(id);
                Ok(Some(Op::Release { id }))
            }
            op => Ok(Some(op)),
        }
    }

    /// The number of rows of the list cell `cell` is in.
    fn rows_of_cell(&self, cell: NodeId) -> usize {
        let Ok(key) = self.key(cell) else { return 0 };
        self.nodes
            .get(self.nodes[key as usize].parent as usize)
            .and_then(|list| list.list.as_ref())
            .map_or(0, |list| list.count as usize)
    }

    /// The number of rows of list `list`.
    fn rows_of_list(&self, list: NodeId) -> u32 {
        self.key(list)
            .ok()
            .and_then(|k| self.nodes[k as usize].list.as_ref())
            .map_or(0, |l| l.count)
    }

    /// Whether `id` is a list.
    fn is_list(&self, id: NodeId) -> bool {
        self.key(id)
            .is_ok_and(|k| self.nodes[k as usize].list.is_some())
    }

    /// Item cell `cell` of a component's list: its rows are wrapped by
    /// `place`, the row function of the component's last render.
    fn wrap_row(&mut self, cell: NodeId, place: PlaceFn) {
        let Ok(key) = self.key(cell) else { return };
        let node = &self.nodes[key as usize];
        if !matches!(node.slot, Some(crate::CellSlot::Item(_))) {
            return;
        }
        if let Some(inst) = self.components.instances.get_mut(&cell) {
            if let Inner::Row { place: current, .. } = &mut inst.inner {
                *current = place;
            }
            self.components.mark(cell);
            return;
        }
        // The row's content is already in the cell: it moves into the
        // wrapper.
        let js_children = node
            .children
            .iter()
            .map(|&c| self.nodes[c as usize].id)
            .collect();
        self.components.instances.insert(
            cell,
            Box::new(Instance::new(
                Inner::Row { place, index: 0 },
                None,
                js_children,
                None,
            )),
        );
        self.components.mark(cell);
    }

    /// Function `name` of a module: the UI set's (`Alert.alert`), else the
    /// runtime's (`nsh/android:Toast.show`).
    fn function(&self, name: &str) -> Option<&'static Function> {
        self.components
            .registry
            .function(name)
            .or_else(|| crate::runtime::function(name))
    }

    /// Calls module function `name` (`Alert.alert`); a call JS waits for
    /// gets its result as a `$return` event on the root.
    fn call_function(&mut self, name: &str, args: JsonValue) {
        let Some(function) = self.function(name) else {
            return;
        };
        let call = args.get("c").and_then(JsonValue::as_u64).unwrap_or(0);
        let args = match args.get("a") {
            Some(JsonValue::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        let root = crate::ROOT_ID;
        match &function.call {
            FunctionCall::Sync(run) => {
                let mut cx = self.erased_cx(root);
                let result = run(args, &mut ModuleCx::new(&mut cx));
                self.finish_event(root, cx);
                if let Err(e) = &result {
                    log::warn!(target: "js", "{name}(): {e}");
                }
                if call != 0 {
                    self.reply(root, call, result.map_err(|e| e.0));
                }
            }
            FunctionCall::Async(start) => {
                let executor = Rc::downgrade(&self.components.executor);
                match start(args, AsyncCx::new(executor.clone())) {
                    Ok(future) => {
                        // A task of the root: it never goes away.
                        crate::ui::task::spawn_raw(
                            &self.components.executor,
                            root,
                            Box::pin(async move {
                                let result = future.await;
                                if call == 0 {
                                    if let Err(e) = &result {
                                        log::warn!(target: "js", "{}(): {e}", function.name);
                                    }
                                } else if let Some(executor) = executor.upgrade() {
                                    executor.act(Action::Return(call, result));
                                }
                            }),
                        );
                    }
                    Err(e) => {
                        log::warn!(target: "js", "{name}(): {e}");
                        if call != 0 {
                            self.reply(root, call, Err(e.0));
                        }
                    }
                }
            }
        }
    }

    /// Calls method `name` of `host`'s component; a call JS waits for gets
    /// its result (or error) as a `$return` event.
    fn call_method(&mut self, host: NodeId, name: &str, args: JsonValue) {
        let call = args.get("c").and_then(JsonValue::as_u64).unwrap_or(0);
        let args = match args.get("a") {
            Some(JsonValue::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        let mut cx = self.erased_cx(host);
        let mut failed = None;
        let result = match self
            .components
            .instances
            .get_mut(&host)
            .map(|i| &mut i.inner)
        {
            Some(Inner::Component(inner)) => match caught(|| inner.call(name, args, &mut cx)) {
                Ok(result) => {
                    result.unwrap_or_else(|| Err(PropError(format!("no method `{name}`"))))
                }
                Err(message) => {
                    failed = Some(message.clone());
                    Err(PropError(message))
                }
            },
            _ => Err(PropError(format!("no method `{name}`"))),
        };
        self.finish_event(host, cx);
        if let Some(message) = failed {
            self.component_failed(host, message);
        }
        if let Err(e) = &result {
            log::warn!(target: "js", "{name}(): {e}");
        }
        if call != 0 {
            self.reply(host, call, result.map_err(|e| e.0));
        }
    }

    /// The app's classes on `host` changed.
    fn update_user_class(&mut self, host: NodeId) {
        let inst = &self.components.instances[&host];
        let mut combined = String::with_capacity(inst.class.len() + 16 * inst.class_list.len());
        combined.push_str(&inst.class);
        for name in &inst.class_list {
            combined.push(' ');
            combined.push_str(name);
        }
        let user = (!combined.trim().is_empty()).then(|| self.tailwind.get(&combined).0);
        self.set_user(host, user);
    }

    /// What goes after `host`'s own styles.
    fn set_user(&mut self, host: NodeId, user: Option<Arc<ClassList>>) {
        let inst = self.components.instances.get_mut(&host).expect("host");
        if !same(&inst.user, &user) {
            inst.user = user;
            self.components.mark(host);
        }
    }

    /// Slot node `id` leaves its host.
    fn unslot(&mut self, id: NodeId) {
        if let Some(host) = self.components.slot_hosts.remove(&id) {
            if let Some(inst) = self.components.instances.get_mut(&host) {
                inst.slots.retain(|(_, n)| *n != id);
            }
            self.components.mark(host);
        }
    }

    /// `componentSlot` on a view: it holds a slot's JSX.
    pub(super) fn set_slot_name(&mut self, id: NodeId, name: Arc<str>) {
        self.components.slot_names.insert(id, name);
    }

    /// `Prop::Component` on a view: it becomes the host of component `name`.
    pub(super) fn create_instance(&mut self, host: NodeId, name: Arc<str>) {
        if self.is_host(host) {
            return;
        }
        let (spec, inner) = match self.components.registry.make(&name) {
            Some((spec, inner)) => (Some(spec), Inner::Component(inner)),
            None => {
                self.deny(format!(
                    "no component is registered as `{name}`: it renders nothing"
                ));
                (None, Inner::Missing)
            }
        };
        self.insert_instance(host, spec, inner);
    }

    /// View `host` becomes the host of the component `make` creates (a
    /// component a Rust render or the app's root names by type).
    pub(super) fn create_instance_of(
        &mut self,
        host: NodeId,
        name: &'static str,
        make: fn() -> Box<dyn AnyInstance>,
        spec: fn() -> ComponentSpec,
    ) {
        if self.is_host(host) {
            return;
        }
        let spec = self
            .components
            .specs
            .entry(name)
            .or_insert_with(|| Arc::new(spec()))
            .clone();
        self.insert_instance(host, Some(spec), Inner::Component(make()));
    }

    fn insert_instance(&mut self, host: NodeId, spec: Option<Arc<ComponentSpec>>, inner: Inner) {
        // A style JS set before the component prop.
        let app_style = self
            .key(host)
            .ok()
            .map(|k| self.nodes[k as usize].inline_style.clone())
            .filter(|p| !p.is_empty());
        self.components.instances.insert(
            host,
            Box::new(Instance::new(inner, spec, Vec::new(), app_style)),
        );
        self.components.mark(host);
    }

    /// `$name` on a host: a prop of its component.
    pub(super) fn set_component_prop(
        &mut self,
        host: NodeId,
        name: Arc<str>,
        value: Option<JsonValue>,
    ) -> bool {
        let Some(inst) = self.components.instances.get_mut(&host) else {
            return false;
        };
        let Inner::Component(inner) = &mut inst.inner else {
            return true;
        };
        match inner.set_prop(&name, value) {
            Ok(true) => self.components.mark(host),
            Ok(false) => {}
            Err(e) => {
                let component = inst.spec.as_ref().map_or("", |s| s.name);
                log::warn!(target: "js", "{component}: {e}");
            }
        }
        true
    }

    /// Renders the components whose props or state changed.
    pub(super) fn render_components(&mut self) {
        // What handles asked for since (native values released, ...).
        for action in self.components.executor.take_actions() {
            self.apply_action(action);
        }
        if self.components.dirty.is_empty() {
            return;
        }
        let mut rendered = std::mem::take(&mut self.components.rendered);
        while let Some(host) = self.components.dirty.pop() {
            let Some(mut inst) = self.components.instances.remove(&host) else {
                continue;
            };
            inst.dirty = false;
            inst.renders += 1;
            if inst.renders > MAX_RENDERS {
                log::error!(target: "js", "component on node {host} keeps rendering again; stopped");
                self.components.instances.insert(host, inst);
                continue;
            }
            let failed = self.render(host, &mut inst);
            self.components.instances.insert(host, inst);
            rendered.push(host);
            if let Some(message) = failed {
                self.component_failed(host, message);
            }
        }
        for host in rendered.drain(..) {
            if let Some(inst) = self.components.instances.get_mut(&host) {
                inst.renders = 0;
            }
        }
        self.components.rendered = rendered;
    }

    /// Renders `inst` on `host`; what its code panicked with, if it did.
    fn render(&mut self, host: NodeId, inst: &mut Instance) -> Option<String> {
        let mut failure = None;
        let mut el = match &mut inst.inner {
            Inner::Component(inner) => {
                // What the app passed, as props.
                if let Some(spec) = &inst.spec {
                    for name in spec.slots() {
                        let filled = inst.slots.iter().any(|(s, node)| {
                            *s == name
                                && self
                                    .key(*node)
                                    .is_ok_and(|k| !self.nodes[k as usize].children.is_empty())
                        });
                        inner.set_slot(name, filled);
                    }
                }
                inner.set_children(!inst.js_children.is_empty());
                // The setup the first time, else what changed in the props.
                let mut cx = self.erased_cx(host);
                let mut failed = caught(|| inner.update(&mut cx)).err();
                let context = |type_id| self.provided_above(host, type_id);
                let measure = |font: &_, text: &str| {
                    crate::canvas::measure_line(&self.text_engine, font, text)
                };
                let scope = RenderScope {
                    theme: &self.components.theme,
                    environment: &self.components.environment,
                    insets: self.insets_of(host),
                    globals: &self.components.globals,
                    context: &context,
                    measure: &measure,
                };
                // A render that fails renders nothing.
                let (el, reads) = match failed {
                    Some(_) => (ElNode::empty_view(), RenderReads::default()),
                    None => caught(|| inner.render(&scope)).unwrap_or_else(|message| {
                        failed = Some(message);
                        (ElNode::empty_view(), RenderReads::default())
                    }),
                };
                let old = std::mem::replace(&mut inst.reads, reads);
                self.provided_changed(host, &old.provided, &inst.reads.provided);
                self.finish_event(host, cx);
                failure = failed;
                el
            }
            // The cell itself stays the list's: the wrapper goes inside.
            Inner::Row { place, index } => {
                let at = Position {
                    index: *index,
                    count: self.rows_of_cell(host),
                };
                match caught(|| view().child(place(Child::row(), at)).into_element().0) {
                    Ok(el) => el,
                    Err(message) => {
                        failure = Some(message);
                        ElNode::empty_view()
                    }
                }
            }
            Inner::VirtualRow { list, index } => {
                let list = *list;
                let index = *index;
                let inverted = self
                    .key(list)
                    .ok()
                    .and_then(|k| self.nodes[k as usize].list.as_ref())
                    .is_some_and(|l| l.inverted);
                match self.components.virtual_lists.get(&list) {
                    // The row is gone (the list shrank): the cell shows
                    // nothing until the list unbinds it or binds it to a
                    // row again; the row function is never asked for a
                    // row past the end.
                    Some(state) if index >= state.count as usize => ElNode::empty_view(),
                    Some(state) => {
                        let at = Position {
                            index,
                            count: state.count as usize,
                        };
                        let row = state.row.clone();
                        // Between rows: after each but the last (before it,
                        // inverted).
                        let separator = state.separator.clone().filter(|_| !at.is_last());
                        let build = || {
                            let row = row(index, at);
                            match &separator {
                                Some(s) if inverted => view().child(s()).child(row),
                                Some(s) => view().child(row).child(s()),
                                None => view().child(row),
                            }
                            .into_element()
                            .0
                        };
                        match caught(build) {
                            Ok(el) => el,
                            Err(message) => {
                                failure = Some(message);
                                ElNode::empty_view()
                            }
                        }
                    }
                    None => ElNode::empty_view(),
                }
            }
            Inner::Missing => ElNode::empty_view(),
        };
        if el.tag != Tag::View || !matches!(el.kind, ElKind::Plain) {
            log::error!(target: "js", "a component's root must be a view; it is rendered as one");
            el.tag = Tag::View;
            el.kind = ElKind::Plain;
            el.text = None;
        }
        inst.handlers.clear();
        inst.event_handlers.clear();
        inst.responders.clear();
        let old = inst.root.take();
        let old_place = std::mem::replace(&mut inst.children_at, Placement::None);
        // The root is the host itself.
        let (old_attrs, old_style, old_classes, old_events, old_children) = match old {
            Some(e) => (e.attrs, e.style, e.classes, e.events, e.children),
            None => (
                Attrs::default(),
                StyleRefinement::default(),
                (None, None),
                EventMask::default(),
                Vec::new(),
            ),
        };
        self.apply_attrs_over(host, &el.attrs, &old_attrs, inst.app_style.as_ref());
        let user = inst.user.clone();
        let classes = self.apply_styles(host, &el.style, &old_style, old_classes, user);
        // The host listens to its component's events and to JS's.
        let js = inst.js_events;
        self.apply_handlers(host, &el, old_events | js, js, inst);
        let mut events = EventMask::default();
        for (kind, _) in &el.handlers {
            events.set(*kind, true);
        }
        events.set(
            EventKind::Touch,
            events.contains(EventKind::Touch) || el.responder.is_some(),
        );
        let new_children = std::mem::take(&mut el.children);
        let children = self.reconcile_children(host, old_children, new_children, host, inst);
        inst.root = Some(Expanded {
            id: host,
            tag: Tag::View,
            key: None,
            text: None,
            attrs: el.attrs,
            style: el.style,
            classes,
            events,
            list: ListConfig::default(),
            part: Part::Node,
            children,
        });
        // No `Children` any more: the JS children leave the tree.
        if inst.children_at == Placement::None && old_place != Placement::None {
            for &child in &inst.js_children {
                self.detach_id(child);
            }
        }
        // Lazy children are made while the render places them: by JS, or
        // by the component that renders this one (it renders again to pass
        // them, or to drop them).
        let wanted = inst.children_at != Placement::None;
        if inst.children_wanted != wanted && inst.spec.as_ref().is_some_and(|s| s.lazy_children) {
            inst.children_wanted = wanted;
            match self.components.owners.get(&host).copied() {
                Some(owner) => self.components.mark(owner),
                None => self.events.push((
                    host,
                    EventKind::NativeEvent,
                    EventPayload::Native {
                        name: "$children".into(),
                        data: JsonValue::Bool(wanted),
                    },
                )),
            }
        }
        failure
    }

    /// Whether the component hosted by `nested` takes its children now:
    /// unless they are lazy and its render does not place them.
    fn wants_children(&self, nested: NodeId) -> bool {
        self.components
            .instances
            .get(&nested)
            .is_none_or(|i| i.children_wanted || !i.spec.as_ref().is_some_and(|s| s.lazy_children))
    }

    fn detach_id(&mut self, id: NodeId) {
        if let Ok(key) = self.key(id)
            && self.nodes[key as usize].parent != NO_KEY
        {
            self.detach(key);
        }
    }

    fn apply_attrs(&mut self, id: NodeId, new: &Attrs, old: &Attrs) {
        self.apply_attrs_over(id, new, old, None);
    }

    /// [`Self::apply_attrs`] for a host: the app's style goes after the
    /// root's (and is sent again with it).
    fn apply_attrs_over(
        &mut self,
        id: NodeId,
        new: &Attrs,
        old: &Attrs,
        app: Option<&crate::StylePatch>,
    ) {
        if old.element_ref != new.element_ref
            && let Some(r) = &old.element_ref
        {
            r.detach(id);
        }
        if let Some(r) = &new.element_ref {
            r.attach(id);
        }
        let mut props = std::mem::take(&mut self.components.props);
        new.changes(old, &mut props);
        for prop in props.drain(..) {
            let _ = self.apply_op(Op::SetProp { id, prop });
        }
        self.components.props = props;
        if new.inline != old.inline || new.animated != old.animated {
            let _ = self.apply_op(inline_style(id, new.inline.as_ref(), &new.animated, app));
        }
        if new.drawing != old.drawing
            && let Some(drawing) = &new.drawing
        {
            let _ = self.apply_op(Op::Draw {
                id,
                drawing: Box::new((**drawing).clone()),
            });
        }
    }

    /// Sets node `id`'s classes: `style` lowered, then `user` (a host's).
    fn apply_styles(
        &mut self,
        id: NodeId,
        style: &StyleRefinement,
        old_style: &StyleRefinement,
        old: Classes,
        user: Option<Arc<ClassList>>,
    ) -> Classes {
        let own = if style == old_style {
            old.0.clone()
        } else if style.is_empty() {
            None
        } else {
            Some(self.components.lowered(style))
        };
        if !same(&own, &old.0) || !same(&user, &old.1) {
            let list = match (&own, &user) {
                (Some(a), Some(b)) => Some(Arc::new(a.merged(b))),
                (Some(a), None) => Some(a.clone()),
                (None, Some(b)) => Some(b.clone()),
                (None, None) => None,
            };
            if let Ok(key) = self.key(id) {
                self.set_class_list(key, list);
            }
        }
        (own, user)
    }

    /// Registers `el`'s handlers for node `id` and listens to their events
    /// (plus `extra`, what JS listens to on a host); returns the mask.
    fn apply_handlers(
        &mut self,
        id: NodeId,
        el: &ElNode,
        old: EventMask,
        extra: EventMask,
        inst: &mut Instance,
    ) -> EventMask {
        let mut mask = extra;
        for (kind, handler) in &el.handlers {
            mask.set(*kind, true);
            inst.handlers.push((id, *kind, handler.clone()));
        }
        // The responder hears the touches the platform reports.
        if let Some(responder) = &el.responder {
            mask.set(EventKind::Touch, true);
            inst.responders.push((id, responder.clone()));
        }
        // Only the events that changed.
        let mut changed = mask.bits() ^ old.bits();
        while changed != 0 {
            let kind = EventKind::ALL[changed.trailing_zeros() as usize];
            changed &= changed - 1;
            let _ = self.apply_op(Op::Listen {
                id,
                event: kind,
                enabled: mask.contains(kind),
            });
        }
        mask
    }

    fn reconcile_children(
        &mut self,
        parent: NodeId,
        old: Vec<Expanded>,
        new: Vec<ElNode>,
        host: NodeId,
        inst: &mut Instance,
    ) -> Vec<Expanded> {
        if new.is_empty() && old.is_empty() {
            return Vec::new();
        }
        // `children_each`: one wrapper per JS child, keyed by it.
        let new = if new.iter().any(|e| matches!(e.kind, ElKind::EachChild(_))) {
            let mut each = Vec::with_capacity(new.len() + inst.js_children.len());
            for el in new {
                let ElKind::EachChild(place) = &el.kind else {
                    each.push(el);
                    continue;
                };
                if inst.children_at != Placement::None {
                    log::error!(target: "js", "a render places its children once");
                    continue;
                }
                inst.children_at = Placement::Each;
                let count = inst.js_children.len();
                for (index, &child) in inst.js_children.iter().enumerate() {
                    let mut wrapper = place(Child::js(child), Position { index, count }).0;
                    wrapper.key = Some(format!("\u{0}{child}").into());
                    each.push(wrapper);
                }
            }
            each
        } else {
            new
        };
        let mut old: Vec<Option<Expanded>> = old.into_iter().map(Some).collect();
        let mut out = Vec::with_capacity(new.len());
        let mut placeholder = None;
        // Unkeyed elements take the next unused old element of their kind,
        // in order: an element that comes or goes (`.children(option)`)
        // does not shift its siblings onto new nodes.
        let mut cursor = 0;
        for mut el in new {
            match std::mem::replace(&mut el.kind, ElKind::Plain) {
                ElKind::Slot(name) => {
                    // The slot's own node, while the app passes one.
                    let Some(&(_, node)) = inst.slots.iter().find(|(s, _)| *s == name) else {
                        continue;
                    };
                    let at = old.iter().position(|o| {
                        o.as_ref()
                            .is_some_and(|o| o.part == Part::Slot && o.id == node)
                    });
                    let prev = at.and_then(|j| old[j].take());
                    out.push(self.reconcile_slot(prev, node, el, inst));
                }
                ElKind::Children => {
                    if placeholder.is_some() || inst.children_at != Placement::None {
                        log::error!(target: "js", "a render places its children once");
                        continue;
                    }
                    if let Some(j) = old
                        .iter()
                        .position(|o| o.as_ref().is_some_and(|o| o.part == Part::Children))
                    {
                        old[j] = None;
                    }
                    placeholder = Some(out.len());
                    out.push(Expanded::js(0, Part::Children));
                }
                ElKind::JsChild(id) => {
                    // Taken over, not released (that would detach it).
                    if let Some(j) = old.iter().position(|o| {
                        o.as_ref()
                            .is_some_and(|o| o.part == Part::JsChild && o.id == id)
                    }) {
                        old[j] = None;
                    }
                    if inst.js_children.contains(&id) {
                        out.push(Expanded::js(id, Part::JsChild));
                    }
                }
                // Expanded above.
                ElKind::EachChild(_) => {}
                ElKind::List(list) => {
                    // The glue's list: placed and laid out like a slot,
                    // its rows wrapped by this render's row function.
                    let Some(&(_, node)) = inst.slots.iter().find(|(s, _)| *s == list.name) else {
                        continue;
                    };
                    let at = old.iter().position(|o| {
                        o.as_ref()
                            .is_some_and(|o| o.part == Part::Slot && o.id == node)
                    });
                    let prev = at.and_then(|j| old[j].take());
                    let old_config = prev.as_ref().map(|p| p.list).unwrap_or_default();
                    let mut props = std::mem::take(&mut self.components.props);
                    list.config.changes(&old_config, &mut props);
                    for prop in props.drain(..) {
                        let _ = self.apply_op(Op::SetProp { id: node, prop });
                    }
                    self.components.props = props;
                    let mut e = self.reconcile_slot(prev, node, el, inst);
                    e.list = list.config;
                    let rows = self.rows_of_list(node);
                    self.components.lists.insert(node, (list.row.clone(), rows));
                    // Rows of the component that rendered this one: its
                    // cells to come are wrapped too.
                    if let Some(state) = self.components.virtual_lists.get_mut(&node) {
                        state.place = Some(list.row.clone());
                    }
                    let cells: Vec<NodeId> = match self.key(node) {
                        Ok(k) => self.nodes[k as usize]
                            .children
                            .iter()
                            .map(|&c| self.nodes[c as usize].id)
                            .collect(),
                        Err(_) => Vec::new(),
                    };
                    for cell in cells {
                        self.wrap_row(cell, list.row.clone());
                    }
                    out.push(e);
                }
                kind => {
                    el.kind = kind;
                    let part = match &el.kind {
                        ElKind::Component(c) => Part::Component(c.name),
                        ElKind::VirtualList(_) => Part::VirtualList,
                        ElKind::Raw => Part::Raw,
                        _ => Part::Node,
                    };
                    let matches = |o: &Expanded| o.tag == el.tag && o.part == part;
                    let at = match &el.key {
                        Some(k) => old.iter().position(|o| {
                            o.as_ref()
                                .is_some_and(|o| o.key.as_ref() == Some(k) && matches(o))
                        }),
                        None => {
                            let at = old[cursor.min(old.len())..]
                                .iter()
                                .position(|o| {
                                    o.as_ref().is_some_and(|o| o.key.is_none() && matches(o))
                                })
                                .map(|j| j + cursor);
                            if let Some(j) = at {
                                cursor = j + 1;
                            }
                            at
                        }
                    };
                    let prev = at.and_then(|j| old[j].take());
                    out.push(self.reconcile_node(prev, el, part, host, inst));
                }
            }
        }
        // Content of a nested component (`.child`, `.fill`): it leaves and
        // joins the nested host as JS children and slots do, so the nested
        // render places it.
        let nested = parent != host && self.is_host(parent);
        for o in old.into_iter().flatten() {
            if nested {
                self.leave_nested(parent, o.id);
            }
            self.release_expanded(o);
        }
        if nested {
            self.join_nested(parent, &out);
            return out;
        }
        if let Some(at) = placeholder {
            inst.children_at = Placement::At(parent, out.get(at + 1).map(|e| e.id));
        }
        // Order the native children as rendered, the JS children in the
        // placeholder's place (moves only when it changed).
        let js = &inst.js_children;
        let wanted = || {
            out.iter().flat_map(move |e| {
                if e.part == Part::Children {
                    js.as_slice()
                } else {
                    std::slice::from_ref(&e.id)
                }
            })
        };
        // A list positions its cells itself: their order does not matter
        // (and the cells it binds are not the render's), new ones join it.
        if self.is_list(parent) {
            for e in &out {
                let joined = self
                    .key(e.id)
                    .is_ok_and(|k| self.nodes[k as usize].parent != NO_KEY);
                if !joined {
                    let _ = self.apply_op(Op::Insert {
                        parent,
                        child: e.id,
                        before: None,
                    });
                }
            }
            return out;
        }
        let in_order = self.key(parent).is_ok_and(|k| {
            let have = &self.nodes[k as usize].children;
            have.len() == wanted().count()
                && have
                    .iter()
                    .zip(wanted())
                    .all(|(&c, &id)| self.nodes[c as usize].id == id)
        });
        if !in_order {
            // From the end, each child before the one after it: only the
            // children out of place move. Moving a native view takes it
            // out of the window for a moment, which ends its touches and
            // its focus (Android: an input that gains a sibling would lose
            // the keyboard on each keystroke).
            let ids: Vec<NodeId> = wanted().copied().collect();
            let mut before = None;
            for &child in ids.iter().rev() {
                if !self.precedes(parent, child, before) {
                    let _ = self.apply_op(Op::Insert {
                        parent,
                        child,
                        before,
                    });
                }
                before = Some(child);
            }
        }
        out
    }

    /// Node `child` of this render is no longer content of the nested
    /// component hosted by `nested`.
    fn leave_nested(&mut self, nested: NodeId, child: NodeId) {
        if self.components.slot_names.remove(&child).is_some() {
            self.unslot(child);
        } else if let Ok(Some(op)) = self.intercept(Op::Remove {
            parent: nested,
            child,
        }) {
            let _ = self.apply_op(op);
        }
    }

    /// The content `out` of the nested component hosted by `nested`, in
    /// order: its children as the host's JS children, its slots' holders
    /// in their slots (inserted only when that changed).
    fn join_nested(&mut self, nested: NodeId, out: &[Expanded]) {
        let insert = |core: &mut Self, child: NodeId, before: Option<NodeId>| {
            if let Ok(Some(op)) = core.intercept(Op::Insert {
                parent: nested,
                child,
                before,
            }) {
                let _ = core.apply_op(op);
            }
        };
        let (slots, children): (Vec<NodeId>, Vec<NodeId>) = out
            .iter()
            .map(|e| e.id)
            .partition(|id| self.components.slot_names.contains_key(id));
        for slot in slots {
            if self.components.slot_hosts.get(&slot) != Some(&nested) {
                insert(self, slot, None);
            }
        }
        let same = self
            .components
            .instances
            .get(&nested)
            .is_some_and(|i| i.js_children == children);
        if !same {
            let mut before = None;
            for &child in children.iter().rev() {
                insert(self, child, before);
                before = Some(child);
            }
        }
    }

    /// Whether `child` is a child of `parent` right before `next` (the last
    /// one when `next` is `None`).
    fn precedes(&self, parent: NodeId, child: NodeId, next: Option<NodeId>) -> bool {
        let Ok(k) = self.key(parent) else {
            return false;
        };
        let children = &self.nodes[k as usize].children;
        let Some(at) = children
            .iter()
            .position(|&c| self.nodes[c as usize].id == child)
        else {
            return false;
        };
        children.get(at + 1).map(|&c| self.nodes[c as usize].id) == next
    }

    /// Slot node `node` as element `el`: its styles, props and handlers
    /// (its children are the app's JSX).
    fn reconcile_slot(
        &mut self,
        prev: Option<Expanded>,
        node: NodeId,
        el: ElNode,
        inst: &mut Instance,
    ) -> Expanded {
        let (old_attrs, old_style, old_classes, old_events) = match prev {
            Some(p) => (p.attrs, p.style, p.classes, p.events),
            None => (
                Attrs::default(),
                StyleRefinement::default(),
                (None, None),
                EventMask::default(),
            ),
        };
        self.apply_attrs(node, &el.attrs, &old_attrs);
        let classes = self.apply_styles(node, &el.style, &old_style, old_classes, None);
        // Its events are the component's first (`component_event` finds it
        // by its host); the node stays JS's or the rendering component's.
        let events = self.apply_handlers(node, &el, old_events, EventMask::default(), inst);
        Expanded {
            id: node,
            tag: Tag::View,
            key: None,
            text: None,
            attrs: el.attrs,
            style: el.style,
            classes,
            events,
            list: ListConfig::default(),
            part: Part::Slot,
            children: Vec::new(),
        }
    }

    fn reconcile_node(
        &mut self,
        prev: Option<Expanded>,
        mut el: ElNode,
        part: Part,
        host: NodeId,
        inst: &mut Instance,
    ) -> Expanded {
        if matches!(el.kind, ElKind::Raw) {
            return self.reconcile_raw(prev, el, host);
        }
        // A text with spans: its own content is the first piece, as JS
        // has it.
        if !el.children.is_empty()
            && let Some(content) = el.text.take()
            && !content.is_empty()
        {
            let mut raw = ElNode::new(Tag::Text);
            raw.text = Some(content);
            raw.kind = ElKind::Raw;
            el.children.insert(0, raw);
        }
        let old_list = prev.as_ref().map(|p| p.list).unwrap_or_default();
        let (id, old_attrs, old_style, old_classes, old_events, old_children, old_text) = match prev
        {
            Some(p) => (
                p.id, p.attrs, p.style, p.classes, p.events, p.children, p.text,
            ),
            None => {
                let id = self.components.alloc();
                let _ = self.apply_op(Op::Create { id, tag: el.tag });
                self.components.owners.insert(id, host);
                if let ElKind::Component(c) = &el.kind {
                    self.create_instance_of(id, c.name, c.make, c.spec);
                }
                (
                    id,
                    Attrs::default(),
                    StyleRefinement::default(),
                    (None, None),
                    EventMask::default(),
                    Vec::new(),
                    None,
                )
            }
        };
        let (component, virtual_list) = match std::mem::replace(&mut el.kind, ElKind::Plain) {
            ElKind::Component(c) => (Some(c), None),
            ElKind::VirtualList(list) => (None, Some(list)),
            _ => (None, None),
        };
        if let Some(c) = component {
            // A nested component: its props, and the styles as the app's
            // classes of its host.
            if let Some(nested) = self.components.instances.get_mut(&id)
                && let Inner::Component(inner) = &mut nested.inner
                && inner.set_props(c.props.as_any())
            {
                self.components.mark(id);
            }
            if self.is_host(id) {
                let user = (!el.style.is_empty()).then(|| self.components.lowered(&el.style));
                self.set_user(id, user);
            }
            if let Some(listener) = c.on_event {
                inst.event_handlers.push((id, listener));
            }
            // Its `ComponentRef`.
            let element_ref = el.attrs.element_ref.take();
            if old_attrs.element_ref != element_ref
                && let Some(r) = &old_attrs.element_ref
            {
                r.detach(id);
            }
            if let Some(r) = &element_ref {
                r.attach(id);
            }
            // The content this render passes it (`.child`, `.fill`): nodes of
            // this render, inserted into its host as JS inserts its children,
            // so the nested component places them where its render says.
            // Lazy children are passed only while it places them, as JS
            // makes the app's (`$children`); slots and rows always are.
            let mut content = std::mem::take(&mut el.children);
            if !self.wants_children(id) {
                content.retain(|c| {
                    c.attrs
                        .props
                        .iter()
                        .any(|p| matches!(p, Prop::ComponentSlot(_)))
                });
            }
            let children = self.reconcile_children(id, old_children, content, host, inst);
            return Expanded {
                id,
                tag: el.tag,
                key: el.key,
                text: None,
                attrs: Attrs {
                    element_ref,
                    ..Attrs::default()
                },
                style: el.style,
                classes: (None, None),
                events: EventMask::default(),
                list: ListConfig::default(),
                part,
                children,
            };
        }
        self.apply_attrs(id, &el.attrs, &old_attrs);
        let classes = self.apply_styles(id, &el.style, &old_style, old_classes, None);
        let text = match (el.text.take(), old_text) {
            (Some(content), Some((old, raw))) => {
                if content != old {
                    let _ = self.apply_op(Op::SetText {
                        id: raw,
                        text: content.as_str().to_owned(),
                    });
                }
                Some((content, raw))
            }
            (Some(content), None) => {
                let raw = self.components.alloc();
                let _ = self.apply_op(Op::CreateText {
                    id: raw,
                    text: content.as_str().to_owned(),
                });
                self.components.owners.insert(raw, host);
                let _ = self.apply_op(Op::Insert {
                    parent: id,
                    child: raw,
                    before: None,
                });
                Some((content, raw))
            }
            (None, Some((_, raw))) => {
                self.release_id(raw);
                None
            }
            (None, None) => None,
        };
        let events = self.apply_handlers(id, &el, old_events, EventMask::default(), inst);
        let new_children = std::mem::take(&mut el.children);
        let children = self.reconcile_children(id, old_children, new_children, host, inst);
        let list = match virtual_list {
            Some(list) => {
                let config = list.config;
                self.update_virtual_list(id, host, *list, &old_list);
                config
            }
            None => ListConfig::default(),
        };
        Expanded {
            id,
            tag: el.tag,
            key: el.key,
            text,
            attrs: el.attrs,
            style: el.style,
            classes,
            events,
            list,
            part,
            children,
        }
    }

    /// A piece `el` of a text's content: its raw text node, created or
    /// changed (the render's order puts it in place).
    fn reconcile_raw(&mut self, prev: Option<Expanded>, el: ElNode, host: NodeId) -> Expanded {
        let content = el.text.unwrap_or_else(|| "".into());
        let id = match prev {
            Some(p) => {
                if p.text.as_ref().is_none_or(|(old, _)| *old != content) {
                    let _ = self.apply_op(Op::SetText {
                        id: p.id,
                        text: content.as_str().to_owned(),
                    });
                }
                p.id
            }
            None => {
                let id = self.components.alloc();
                let _ = self.apply_op(Op::CreateText {
                    id,
                    text: content.as_str().to_owned(),
                });
                self.components.owners.insert(id, host);
                id
            }
        };
        Expanded {
            id,
            tag: Tag::Text,
            key: None,
            text: Some((content, id)),
            attrs: Attrs::default(),
            style: StyleRefinement::default(),
            classes: (None, None),
            events: EventMask::default(),
            list: ListConfig::default(),
            part: Part::Raw,
            children: Vec::new(),
        }
    }

    /// A render's list of its own rows: its layout, its rows (a splice
    /// when the keys say rows came or went, then the count), and its live
    /// cells render again with the new row function.
    fn update_virtual_list(
        &mut self,
        id: NodeId,
        owner: NodeId,
        list: VirtualListEl,
        old: &ListConfig,
    ) {
        let mut props = std::mem::take(&mut self.components.props);
        list.config.changes(old, &mut props);
        let (old_count, old_keys) = match self.components.virtual_lists.get(&id) {
            Some(state) => (Some(state.count), state.keys.clone()),
            None => (None, None),
        };
        if let (Some(old), Some(new)) = (&old_keys, &list.keys)
            && let Some((at, removed, inserted)) = crate::ui::virtual_list::splice(old, new)
        {
            props.push(Prop::Splice(at, removed, inserted));
        }
        if old_count != Some(list.count) {
            props.push(Prop::Count(list.count));
        }
        for prop in props.drain(..) {
            let _ = self.apply_op(Op::SetProp { id, prop });
        }
        self.components.props = props;
        if old_count.is_none()
            && let Some(index) = list.initial_index
        {
            let _ = self.apply_op(Op::Command {
                id,
                command: Command::ScrollToIndex {
                    index,
                    animated: false,
                    view_position: 0.0,
                    view_offset: 0.0,
                },
            });
        }
        // The rows of another component's `Items` prop (its slot's node).
        let items = self.components.slot_names.contains_key(&id);
        let state = self
            .components
            .virtual_lists
            .entry(id)
            .or_insert_with(|| VirtualListState {
                owner,
                row: list.row.clone(),
                count: 0,
                keys: None,
                separator: None,
                cells: Vec::new(),
                items,
                place: None,
                contents: Vec::new(),
                min_view: None,
                reported: None,
                latest: None,
                report: None,
            });
        state.row = list.row;
        state.count = list.count;
        state.keys = list.keys;
        state.separator = list.separator;
        state.min_view = list.min_view_time;
        let marked: Vec<NodeId> = state
            .cells
            .iter()
            .chain(&state.contents)
            .copied()
            .filter(|&c| c != 0)
            .collect();
        for cell in marked {
            self.components.mark(cell);
        }
    }

    /// Binds the cells of the components' own lists the last layout asked
    /// for (creating them as needed); false when there were none.
    pub(super) fn bind_virtual_lists(&mut self) -> bool {
        if self.components.virtual_lists.is_empty() {
            return false;
        }
        let mut binds = std::mem::take(&mut self.list_binds);
        let mut bound = false;
        binds.retain(|bind| {
            if !self.components.virtual_lists.contains_key(&bind.list) {
                return true;
            }
            for &(slot, index) in &bind.binds {
                self.bind_virtual_cell(bind.list, slot as usize, index as usize);
            }
            bound = true;
            false
        });
        self.list_binds = binds;
        bound
    }

    fn bind_virtual_cell(&mut self, list: NodeId, slot: usize, index: usize) {
        let state = &self.components.virtual_lists[&list];
        let owner = state.owner;
        let cell = match state.cells.get(slot).copied().filter(|&c| c != 0) {
            Some(cell) => {
                let content = state.contents.get(slot).copied().filter(|&c| c != 0);
                for part in [Some(cell), content].into_iter().flatten() {
                    if let Some(inst) = self.components.instances.get_mut(&part) {
                        match &mut inst.inner {
                            Inner::VirtualRow { index: current, .. }
                            | Inner::Row { index: current, .. } => *current = index,
                            _ => {}
                        }
                    }
                    self.components.mark(part);
                }
                cell
            }
            None => {
                let (items, place) = (state.items, state.place.clone());
                let cell = self.components.alloc();
                let _ = self.apply_op(Op::Create {
                    id: cell,
                    tag: Tag::Cell,
                });
                self.components.owners.insert(cell, owner);
                let _ = self.apply_op(Op::SetProp {
                    id: cell,
                    prop: Prop::Slot(crate::CellSlot::Item(slot as u32)),
                });
                let _ = self.apply_op(Op::Insert {
                    parent: list,
                    child: cell,
                    before: None,
                });
                let row = Inner::VirtualRow { list, index };
                let content = if items {
                    // The placing component's wrapper in the cell, around
                    // the row's own node (placed as a row's content is).
                    let content = self.components.alloc();
                    let _ = self.apply_op(Op::Create {
                        id: content,
                        tag: Tag::View,
                    });
                    self.components.owners.insert(content, owner);
                    let _ = self.apply_op(Op::Insert {
                        parent: cell,
                        child: content,
                        before: None,
                    });
                    self.components.instances.insert(
                        content,
                        Box::new(Instance::new(row, None, Vec::new(), None)),
                    );
                    self.components.mark(content);
                    let place = place.unwrap_or_else(|| Rc::new(|row, _| row.into_element()));
                    self.components.instances.insert(
                        cell,
                        Box::new(Instance::new(
                            Inner::Row { place, index },
                            None,
                            vec![content],
                            None,
                        )),
                    );
                    content
                } else {
                    self.components
                        .instances
                        .insert(cell, Box::new(Instance::new(row, None, Vec::new(), None)));
                    0
                };
                let state = self.components.virtual_lists.get_mut(&list).expect("list");
                if state.cells.len() <= slot {
                    state.cells.resize(slot + 1, 0);
                    state.contents.resize(slot + 1, 0);
                }
                state.cells[slot] = cell;
                state.contents[slot] = content;
                self.components.mark(cell);
                cell
            }
        };
        let _ = self.apply_op(Op::SetProp {
            id: cell,
            prop: Prop::CellIndex(index as u32),
        });
    }

    /// A component's own list reported its viewable rows: they are the
    /// owner's to handle now, or once they stayed so for the minimum view
    /// time (a change meanwhile starts the wait again).
    fn viewable_changed(&mut self, list: NodeId, payload: &EventPayload) {
        let executor = self.components.executor.clone();
        let Some(state) = self.components.virtual_lists.get_mut(&list) else {
            return;
        };
        state.latest = virtual_list::viewable_range(payload, "first", "last");
        match state.min_view {
            Some(delay) => {
                state.report = Some(crate::ui::task::act_after(
                    &executor,
                    state.owner,
                    delay,
                    Action::ReportViewable(list),
                ));
            }
            None => self.report_viewable(list),
        }
    }

    /// Hands list `list`'s owner the rows viewable now, unless they are the
    /// ones reported last.
    fn report_viewable(&mut self, list: NodeId) {
        let Some(state) = self.components.virtual_lists.get_mut(&list) else {
            return;
        };
        state.report = None;
        if state.latest == state.reported {
            return;
        }
        let payload = virtual_list::viewable_payload(state.latest, state.reported);
        state.reported = state.latest;
        let owner = state.owner;
        self.handle(owner, list, EventKind::ViewableItemsChanged, &payload);
    }

    fn release_id(&mut self, id: NodeId) {
        self.components.owners.remove(&id);
        let _ = self.apply_op(Op::Release { id });
    }

    /// Frees an expanded element and what it rendered (a slot's node is
    /// JS's: it only leaves the tree).
    fn release_expanded(&mut self, e: Expanded) {
        match e.part {
            Part::Slot => {
                self.detach_id(e.id);
                return;
            }
            // The JS children are placed (or detached) by the render.
            Part::Children => return,
            // A JS child leaves the wrapper being released.
            Part::JsChild => {
                self.detach_id(e.id);
                return;
            }
            Part::Raw => {
                self.release_id(e.id);
                return;
            }
            Part::Component(_) => self.release_instance(e.id),
            Part::VirtualList => {
                if let Some(state) = self.components.virtual_lists.remove(&e.id) {
                    let parts = state.contents.into_iter().chain(state.cells);
                    for part in parts.filter(|&c| c != 0) {
                        self.release_instance(part);
                        self.release_id(part);
                    }
                }
                // The rows of another component's `Items` prop: no longer
                // its slot.
                if self.components.slot_names.remove(&e.id).is_some() {
                    self.unslot(e.id);
                }
                self.components.lists.remove(&e.id);
            }
            Part::Node => {}
        }
        if let Some(r) = &e.attrs.element_ref {
            r.detach(e.id);
        }
        for c in e.children {
            self.release_expanded(c);
        }
        if let Some((_, raw)) = e.text {
            self.release_id(raw);
        }
        self.release_id(e.id);
    }

    /// Frees what a component rendered (not its host nor its JS children).
    fn release_instance(&mut self, host: NodeId) {
        let Some(mut inst) = self.components.instances.remove(&host) else {
            return;
        };
        if let Inner::Component(inner) = &mut inst.inner {
            let mut cx = self.erased_cx(host);
            if let Err(message) = caught(|| inner.cleanup(&mut cx)) {
                log::error!(target: "js", "{} failed in its cleanup: {message}", inner.name());
            }
            self.finish_event(host, cx);
        }
        // Its tasks and timers end with it.
        self.components.executor.cancel_host(host);
        let listeners = self.components.executor.back.borrow_mut().remove_host(host);
        drop(listeners);
        for &child in &inst.js_children {
            self.detach_id(child);
        }
        for (_, node) in inst.slots {
            self.components.slot_hosts.remove(&node);
            self.detach_id(node);
        }
        if let Some(root) = inst.root {
            if let Some(r) = &root.attrs.element_ref {
                r.detach(host);
            }
            for c in root.children {
                self.release_expanded(c);
            }
        }
    }

    /// A native event on node `id`: true when a component handled it (its
    /// own parts' events never reach JS, which does not know them).
    pub fn component_event(&mut self, id: NodeId, kind: EventKind, payload: &EventPayload) -> bool {
        // Touches come on the root with the listening nodes around each
        // finger: the components hear theirs, JS its own.
        if kind == EventKind::Touch && id == ROOT_ID {
            if let EventPayload::Touch(touch) = payload {
                self.touches(payload, touch);
            }
            return false;
        }
        if self.platform_event(id, kind, payload) {
            return true;
        }
        if self.is_host(id) {
            return self.handle(id, id, kind, payload);
        }
        // A component's own list reports its viewable rows as they settle.
        if kind == EventKind::ViewableItemsChanged
            && self.components.virtual_lists.contains_key(&id)
        {
            self.viewable_changed(id, payload);
            return true;
        }
        // A slot's node: the component it is a slot of handles its events,
        // then whoever made it: the component that rendered it, or JS (the
        // app's list of an `Items` prop has the app's handlers).
        let slot_of = self.components.slot_hosts.get(&id).copied();
        let owner = self.components.owners.get(&id).copied();
        if let Some(host) = slot_of {
            self.handle(host, id, kind, payload);
        }
        if let Some(host) = owner
            && Some(host) != slot_of
        {
            self.handle(host, id, kind, payload);
        }
        // The core's nodes (odd ids) are the components': JS does not know
        // them.
        (slot_of.is_some() || owner.is_some()) && id % 2 == 1
    }

    /// The host of the component whose render node `id` is (itself for a
    /// host).
    pub(super) fn host_of(&self, id: NodeId) -> Option<NodeId> {
        if self.is_host(id) {
            Some(id)
        } else {
            self.components.owners.get(&id).copied()
        }
    }

    /// Runs `host`'s component's handler of `kind` on its node `id`; false
    /// when it has none.
    fn handle(
        &mut self,
        host: NodeId,
        id: NodeId,
        kind: EventKind,
        payload: &EventPayload,
    ) -> bool {
        let Some(listener) = self.handler(host, id, kind) else {
            return false;
        };
        self.run_listener(host, &listener, payload)
    }

    /// `host`'s component's handler of `kind` on its node `id`.
    pub(super) fn handler(
        &self,
        host: NodeId,
        id: NodeId,
        kind: EventKind,
    ) -> Option<Listener<EventPayload>> {
        self.components
            .instances
            .get(&host)?
            .handlers
            .iter()
            .find(|(n, k, _)| *n == id && *k == kind)
            .map(|(_, _, l)| l.clone())
    }

    /// The part of `host`'s node `id` in the touch responder.
    pub(super) fn responder_el(&self, host: NodeId, id: NodeId) -> Option<Rc<ResponderEl>> {
        self.components
            .instances
            .get(&host)?
            .responders
            .iter()
            .find(|(n, _)| *n == id)
            .map(|(_, r)| r.clone())
    }

    /// Runs `listener` of a node `host`'s render placed with the state it
    /// changes (its component's, or a list row's owner's); false when that
    /// component is gone.
    pub(super) fn run_listener<E: ?Sized>(
        &mut self,
        host: NodeId,
        listener: &Listener<E>,
        event: &E,
    ) -> bool {
        let Some(owner) = self.state_owner(host) else {
            return false;
        };
        let mut cx = self.erased_cx(owner);
        let Some(inst) = self.components.instances.get_mut(&owner) else {
            return false;
        };
        let mut failed = None;
        if let Inner::Component(inner) = &mut inst.inner {
            let (state, props) = inner.parts();
            failed = caught(|| (listener.0)(state, props, event, &mut cx)).err();
        }
        self.finish_event(owner, cx);
        if let Some(message) = failed {
            self.component_failed(owner, message);
        }
        true
    }

    /// The component whose state `host`'s handlers change: its own, or for
    /// a list row the component's whose list it is.
    fn state_owner(&self, host: NodeId) -> Option<NodeId> {
        match &self.components.instances.get(&host)?.inner {
            Inner::Component(_) => Some(host),
            Inner::VirtualRow { list, .. } => {
                self.components.virtual_lists.get(list).map(|s| s.owner)
            }
            Inner::Row { .. } => self
                .list_of(host)
                .and_then(|l| self.components.slot_hosts.get(&l))
                .copied(),
            Inner::Missing => None,
        }
    }

    /// The code of `host`'s component panicked: the nearest component above
    /// that handles the error does ([`Component::catch_error`]); else it is
    /// reported.
    fn component_failed(&mut self, host: NodeId, message: String) {
        let component = match self.components.instances.get(&host).map(|i| &i.inner) {
            Some(Inner::Component(inner)) => inner.name(),
            _ => "a list row",
        };
        let error = ComponentError { component, message };
        let mut key = self.key(host).ok();
        while let Some(k) = key {
            let parent = self.nodes[k as usize].parent;
            if parent == NO_KEY {
                break;
            }
            key = Some(parent);
            let id = self.nodes[parent as usize].id;
            let mut cx = self.erased_cx(id);
            let Some(Inner::Component(inner)) =
                self.components.instances.get_mut(&id).map(|i| &mut i.inner)
            else {
                continue;
            };
            let handled = caught(|| inner.catch_error(&error, &mut cx)).unwrap_or(false);
            self.finish_event(id, cx);
            if handled {
                return;
            }
        }
        log::error!(target: "js", "{error}");
    }

    /// The list cell `cell` is in.
    fn list_of(&self, cell: NodeId) -> Option<NodeId> {
        let key = self.key(cell).ok()?;
        let parent = self.nodes[key as usize].parent;
        (parent != NO_KEY).then(|| self.nodes[parent as usize].id)
    }

    /// Applies what a handler did: a new render, events for JS (or for the
    /// component that rendered this one).
    /// What a component or a task asked of the platform.
    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Command(id, command) => {
                let _ = self.apply_op(Op::Command { id, command });
            }
            Action::Dialog(mut dialog, answer) => {
                dialog.id = self.components.next_dialog;
                self.components.next_dialog += 1;
                self.components.dialogs.insert(dialog.id, answer);
                let _ = self.apply_op(Op::Command {
                    id: crate::ROOT_ID,
                    command: Command::Dialog(Box::new(dialog)),
                });
            }
            Action::Return(call, result) => self.reply(crate::ROOT_ID, call, result),
            Action::Animation(op) => {
                let _ = self.apply_op(Op::Animation(op));
            }
            Action::Animate { op, animation, end } => {
                self.components
                    .animations
                    .insert(animation, Awaited::End(end));
                let _ = self.apply_op(Op::Animation(op));
            }
            Action::ReadValue { node, value } => {
                let request = self.components.executor.next_animation_id();
                self.components
                    .animations
                    .insert(request, Awaited::Value(value));
                let _ = self.apply_op(Op::Animation(nsh_animation::AnimationOp::Read {
                    node,
                    request,
                }));
            }
            Action::ReportViewable(list) => self.report_viewable(list),
            Action::Frame(answer) => self.wait_frame(answer),
            Action::Direction(change) => self.change_direction(change),
            Action::Appearance(appearance) => self.set_appearance(appearance),
            Action::Measure(id, answer) => self.components.measures.push((id, answer)),
            Action::ImageInfo(uri, answer) => {
                let id = self.components.next_image_info;
                self.components.next_image_info += 1;
                self.components.image_infos.insert(id, answer);
                let _ = self.apply_op(Op::Command {
                    id: crate::ROOT_ID,
                    command: Command::ImageInfo { id, uri },
                });
            }
            Action::ReadClipboard(read) => {
                let id = self.components.next_clipboard_read;
                self.components.next_clipboard_read += 1;
                let has_only = matches!(read, ClipboardRead::Has(_));
                self.components.clipboard_reads.insert(id, read);
                let _ = self.apply_op(Op::Command {
                    id: crate::ROOT_ID,
                    command: Command::ReadClipboard { id, has_only },
                });
            }
        }
    }

    /// Native animations that ended: the components' resolve their
    /// [`crate::ui::AnimationEnd`]s; JS's stay in `finished`.
    pub fn animations_finished(&mut self, finished: &mut Vec<nsh_animation::Finished>) {
        finished.retain(|f| match self.components.animations.remove(&f.animation) {
            Some(Awaited::End(end)) => {
                end.borrow_mut().deliver(f.finished);
                false
            }
            Some(Awaited::Value(value)) => {
                value.borrow_mut().deliver(f.value.unwrap_or_default());
                false
            }
            // JS's are even.
            None => f.animation % 2 == 0,
        });
    }

    /// The answer to image info request `id`.
    pub(super) fn answer_image_info(&mut self, id: u32, size: Result<ImageSize, String>) {
        if let Some(answer) = self.components.image_infos.remove(&id) {
            answer.borrow_mut().deliver(size);
        }
    }

    /// The answer to clipboard read `id`: the text, `None` when it holds
    /// none.
    pub(super) fn answer_clipboard(&mut self, id: u32, text: Option<String>, has: bool) {
        match self.components.clipboard_reads.remove(&id) {
            Some(ClipboardRead::Text(answer)) => {
                answer.borrow_mut().deliver(text.unwrap_or_default())
            }
            Some(ClipboardRead::Has(answer)) => answer.borrow_mut().deliver(has),
            None => {}
        }
    }

    /// The answer to a call JS waits for, as a `$return` event on `host`.
    pub(super) fn reply(&mut self, host: NodeId, call: u64, result: Result<JsonValue, String>) {
        let mut reply = serde_json::Map::new();
        reply.insert("c".into(), call.into());
        match result {
            Ok(value) => reply.insert("v".into(), value),
            Err(e) => reply.insert("e".into(), JsonValue::String(e)),
        };
        self.events.push((
            host,
            EventKind::NativeEvent,
            EventPayload::Native {
                name: "$return".into(),
                data: JsonValue::Object(reply),
            },
        ));
    }

    /// Events the components handle before anything else: dialog answers
    /// (theirs never reach JS), insets of screens and modals.
    fn platform_event(&mut self, id: NodeId, kind: EventKind, payload: &EventPayload) -> bool {
        let number = |key: &str| match payload {
            EventPayload::Map(entries) => entries.iter().find_map(|(k, v)| match v {
                crate::PayloadValue::Number(n) if *k == key => Some(*n),
                _ => None,
            }),
            _ => None,
        };
        match kind {
            EventKind::Dialog => {
                let Some(dialog) = number("id") else {
                    return false;
                };
                let Some(answer) = self.components.dialogs.remove(&(dialog as u32)) else {
                    return false;
                };
                answer
                    .borrow_mut()
                    .deliver(number("index").unwrap_or(-1.0) as i32);
                true
            }
            EventKind::BackPress if id == crate::ROOT_ID => {
                let listeners = self.components.executor.back.borrow().newest_first();
                for (host, listener) in listeners {
                    if self.back_pressed(host, &listener) {
                        return true;
                    }
                }
                // JS's listeners run next (and go back when none handles
                // it).
                if self.components.js_back {
                    return false;
                }
                let _ = self.apply_op(Op::Command {
                    id: crate::ROOT_ID,
                    command: Command::Back,
                });
                true
            }
            EventKind::ImageInfo if id == crate::ROOT_ID => {
                let Some(request) = number("id") else {
                    return false;
                };
                let error = match payload {
                    EventPayload::Map(entries) => entries.iter().find_map(|(k, v)| match v {
                        crate::PayloadValue::Str(e) if *k == "error" => Some(e.clone()),
                        _ => None,
                    }),
                    _ => None,
                };
                let size = match (number("width"), number("height"), error) {
                    (Some(width), Some(height), None) => Ok(ImageSize { width, height }),
                    (_, _, error) => Err(error.unwrap_or_else(|| "no size".into())),
                };
                self.answer_image_info(request as u32, size);
                true
            }
            EventKind::Clipboard if id == crate::ROOT_ID => {
                let Some(request) = number("id") else {
                    return false;
                };
                let (mut text, mut has) = (None, false);
                if let EventPayload::Map(entries) = payload {
                    for (k, v) in entries {
                        match (*k, v) {
                            ("text", crate::PayloadValue::Str(t)) => text = Some(t.clone()),
                            ("has", crate::PayloadValue::Bool(b)) => has = *b,
                            _ => {}
                        }
                    }
                }
                self.answer_clipboard(request as u32, text, has);
                true
            }
            EventKind::Insets => {
                let insets = EdgeInsets {
                    top: number("top").unwrap_or(0.0) as f32,
                    right: number("right").unwrap_or(0.0) as f32,
                    bottom: number("bottom").unwrap_or(0.0) as f32,
                    left: number("left").unwrap_or(0.0) as f32,
                };
                if self.components.insets.insert(id, insets) != Some(insets) {
                    self.mark_readers(crate::ui::env::INSETS);
                }
                // JS's screens keep them too.
                false
            }
            _ => false,
        }
    }

    /// Runs a back listener of component `host`: whether it handled the
    /// press.
    fn back_pressed(&mut self, host: NodeId, listener: &crate::ui::BackListener) -> bool {
        let mut cx = self.erased_cx(host);
        let Some(inst) = self.components.instances.get_mut(&host) else {
            return false;
        };
        let Inner::Component(inner) = &mut inst.inner else {
            return false;
        };
        let (state, props) = inner.parts();
        let result = caught(|| listener(state, props, &mut cx));
        self.finish_event(host, cx);
        match result {
            Ok(handled) => handled,
            Err(message) => {
                self.component_failed(host, message);
                false
            }
        }
    }

    /// The root listens to the back button while JS or a component does:
    /// follows the components' listeners coming and going.
    pub(super) fn sync_back(&mut self) {
        let rust = {
            let mut back = self.components.executor.back.borrow_mut();
            if !std::mem::take(&mut back.changed) {
                return;
            }
            !back.list.is_empty()
        };
        let _ = self.apply_op(Op::Listen {
            id: crate::ROOT_ID,
            event: EventKind::BackPress,
            enabled: rust || self.components.js_back,
        });
    }

    /// The nearest `type_id` provided by a component above `host` (through
    /// the app's nodes too).
    fn provided_above(&self, host: NodeId, type_id: TypeId) -> Option<Rc<dyn Any>> {
        let mut key = self.key(host).ok()?;
        loop {
            key = self.nodes[key as usize].parent;
            if key == NO_KEY {
                return None;
            }
            let id = self.nodes[key as usize].id;
            if let Some(p) = self
                .components
                .instances
                .get(&id)
                .and_then(|i| i.reads.provided.iter().find(|p| p.type_id == type_id))
            {
                return Some(p.value.clone());
            }
        }
    }

    /// `host` provides `new` instead of `old`: the components reading a
    /// value that changed render again.
    fn provided_changed(&mut self, host: NodeId, old: &[Provided], new: &[Provided]) {
        let mut changed: Vec<TypeId> = Vec::new();
        for p in new {
            if !old.iter().any(|o| o.same(p)) {
                changed.push(p.type_id);
            }
        }
        for o in old {
            if !new.iter().any(|p| p.type_id == o.type_id) {
                changed.push(o.type_id);
            }
        }
        if changed.is_empty() {
            return;
        }
        let readers: Vec<NodeId> = self
            .components
            .instances
            .iter()
            .filter(|(id, i)| **id != host && i.reads.contexts.iter().any(|t| changed.contains(t)))
            .map(|(&id, _)| id)
            .collect();
        for id in readers {
            self.components.mark(id);
        }
    }

    /// Renders again the components that read globals of these types.
    fn mark_global_readers(&mut self, changed: &[TypeId]) {
        let readers: Vec<NodeId> = self
            .components
            .instances
            .iter()
            .filter(|(_, i)| i.reads.globals.iter().any(|t| changed.contains(t)))
            .map(|(&id, _)| id)
            .collect();
        for id in readers {
            self.components.mark(id);
        }
    }

    /// Renders again the components that read `what` of the environment.
    fn mark_readers(&mut self, what: u16) {
        let hosts: Vec<NodeId> = self
            .components
            .instances
            .iter()
            .filter(|(_, i)| i.reads.env & what != 0)
            .map(|(&h, _)| h)
            .collect();
        for host in hosts {
            self.components.mark(host);
        }
    }

    /// The insets where `host` renders: its screen's or modal's, else the
    /// surface's safe area.
    fn insets_of(&self, host: NodeId) -> EdgeInsets {
        let safe = self.components.environment.safe_area;
        let mut key = self.key(host).ok();
        while let Some(k) = key {
            let node = &self.nodes[k as usize];
            if let Some(insets) = self.components.insets.get(&node.id) {
                return *insets;
            }
            match node.kind {
                // Not reported yet: a screen's header covers the top.
                Kind::Screen => return EdgeInsets { top: 0.0, ..safe },
                Kind::Modal => return safe,
                _ => {}
            }
            key = (node.parent != NO_KEY).then_some(node.parent);
        }
        safe
    }

    fn finish_event(&mut self, host: NodeId, cx: ErasedCx) {
        if cx.notify {
            self.components.mark(host);
        }
        if !cx.changed_globals.is_empty() {
            self.mark_global_readers(&cx.changed_globals);
        }
        for action in cx.actions {
            self.apply_action(action);
        }
        // What handles (element refs, native values) asked for meanwhile,
        // in order with the rest.
        for action in self.components.executor.take_actions() {
            self.apply_action(action);
        }
        for event in cx.emits {
            match self.components.owners.get(&host).copied() {
                // A nested component: the component that rendered it gets
                // the event, if it handles it (a list row's handlers run
                // with the component whose list it is).
                Some(parent) => {
                    let Some(listener) = self
                        .components
                        .instances
                        .get(&parent)
                        .and_then(|inst| inst.event_handlers.iter().find(|(n, _)| *n == host))
                        .map(|(_, l)| l.clone())
                    else {
                        continue;
                    };
                    let Some(parent) = self.state_owner(parent) else {
                        continue;
                    };
                    let mut cx = self.erased_cx(parent);
                    let Some(inst) = self.components.instances.get_mut(&parent) else {
                        continue;
                    };
                    let mut failed = None;
                    if let Inner::Component(inner) = &mut inst.inner {
                        let (state, props) = inner.parts();
                        failed =
                            caught(|| (listener.0)(state, props, event.as_any(), &mut cx)).err();
                    }
                    self.finish_event(parent, cx);
                    if let Some(message) = failed {
                        self.component_failed(parent, message);
                    }
                }
                None => self.events.push((
                    host,
                    EventKind::NativeEvent,
                    EventPayload::Native {
                        name: event.name().into(),
                        data: event.data(),
                    },
                )),
            }
        }
    }
}

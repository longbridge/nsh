//! The view's controller: the session it shows, the tree of rendered
//! blocks and their ids, the geometry, the revisions bindings subscribe to,
//! and what typing, keys, presses, drags, the clipboard and input methods
//! do: each becomes a transaction on the session, after the plugins' hooks
//! had their turn.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use nsh_core::{
    keymap::{Modifiers, When},
    ui::{Os, prelude::*},
};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;

use super::{
    RichEditorChange, RichEditorEvent, RichEditorProps, RichEditorSelection,
    geometry::{DOC, Geometry, Hit},
    keys::{self, Move, Unit},
    node_view::{NodeView, NodeViewEvent},
    session::{EditorSession, NodeViews, ViewRequest},
};
use crate::rich_editor::{
    commands::base_keymap,
    extensions::{HistoryOptions, Keymap, KeymapOptions, history, starter},
    formats::{slice_from_text, slice_to_text},
    model::{Node, SchemaSpec},
    state::{
        Affinity, ClickEvent, Decoration, EditorState, EditorStateConfig, KeyEvent, NodeSelection,
        Plugin, Selection, TextSelection, Transaction, TransactionOrigin,
    },
};

/// The sink is emptied once it holds this many bytes (and no composition
/// is in progress).
const SINK_LIMIT: usize = 256;

/// A rendered block: its id (kept while the block is the same block across
/// changes), its node and where it starts.
#[derive(Clone, Debug)]
pub(crate) struct BlockItem {
    pub(crate) id: u64,
    pub(crate) node: Node,
    /// The position before the node.
    pub(crate) pos: usize,
    /// The id of the box its frame is measured in.
    pub(crate) parent: u64,
}

impl PartialEq for BlockItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.pos == other.pos
            && self.parent == other.parent
            && self.node.ptr_eq(&other.node)
    }
}

/// The blocks' ids: the children of each rendered container.
#[derive(Default)]
pub(crate) struct Tree {
    pub(crate) children: HashMap<u64, Vec<BlockItem>>,
    next: u64,
}

/// The id of the slot a node view with an editable content slot renders
/// its content in (a box measured in the content box).
pub(crate) fn slot_id(id: u64) -> u64 {
    id | (1 << 62)
}

/// A press being dragged.
#[derive(Debug, Clone, Copy)]
struct Drag {
    /// The selection's anchor.
    anchor: usize,
    /// The press in the content less the press in the root.
    shift: Point<Points>,
    /// The scroll offset at the press.
    scroll: Points,
}

/// What the native input that takes typed text holds.
#[derive(Debug, Default)]
struct Sink {
    text: String,
    emptied: Option<String>,
    composing: bool,
    /// The composition whose commit is still to come (its id).
    composition: Option<i64>,
}

/// The view's controller. Cloned into the closures that need it.
#[derive(Clone)]
pub(crate) struct Rv {
    pub(crate) session: EditorSession,
    pub(crate) props: RichEditorProps,
    pub(crate) cx: Scope<RichEditorEvent>,
    /// The state rendered last.
    pub(crate) st: Rc<RefCell<EditorState>>,
    pub(crate) node_views: Rc<RefCell<NodeViews>>,
    pub(crate) tree: Rc<RefCell<Tree>>,
    pub(crate) geo: Rc<RefCell<Geometry>>,
    pub(crate) decorations: Rc<RefCell<Vec<Decoration>>>,
    /// The document changed: the blocks.
    pub(crate) doc_rev: Signal<u64>,
    /// The selection changed.
    pub(crate) sel_rev: Signal<u64>,
    /// The decorations changed.
    pub(crate) deco_rev: Signal<u64>,
    /// A block's frame or a text's lines changed.
    pub(crate) layout_rev: Signal<u64>,
    /// The session has a state the view has not taken yet.
    notified: Signal<u64>,
    pub(crate) focused: Signal<bool>,
    pub(crate) blink: Signal<bool>,
    pub(crate) composition: Signal<Option<String>>,
    pub(crate) sink_value: Signal<String>,
    sink: Rc<RefCell<Sink>>,
    next_composition: Rc<Cell<i64>>,
    drag: Rc<Cell<Option<Drag>>>,
    /// The x Up and Down keep to.
    goal_x: Rc<Cell<Option<Points>>>,
    /// The node a node selection holds, as last told to its view.
    selected: Rc<RefCell<Option<(usize, Node)>>>,
    pub(crate) input: ElementRef,
    pub(crate) scroll: ElementRef,
    pub(crate) content: ElementRef,
    pub(crate) scroll_y: Signal<Points>,
    pub(crate) viewport: Signal<Size<Points>>,
    /// The state's scroll request was not carried out yet.
    scroll_pending: Rc<Cell<bool>>,
    blink_task: Rc<RefCell<Option<Spawned<()>>>>,
    base_keymap: Rc<Keymap>,
}

impl Rv {
    pub(crate) fn new(props: RichEditorProps, cx: Scope<RichEditorEvent>) -> Rv {
        let session = props
            .editor
            .get_untracked()
            .unwrap_or_else(|| own_session(&props));
        let state = session.state();
        let mac = matches!(os(), Os::MacOs | Os::Ios);
        let base_keymap =
            Keymap::new(base_keymap_for(mac), &KeymapOptions { mac }).unwrap_or_default();
        let rv = Rv {
            node_views: Rc::new(RefCell::new(session.node_views())),
            session,
            props,
            cx,
            st: Rc::new(RefCell::new(state.clone())),
            tree: Rc::default(),
            geo: Rc::new(RefCell::new(Geometry {
                font: "16px sans-serif".into(),
                ..Geometry::default()
            })),
            decorations: Rc::default(),
            doc_rev: signal(0),
            sel_rev: signal(0),
            deco_rev: signal(0),
            layout_rev: signal(0),
            notified: signal(0),
            focused: signal(false),
            blink: signal(true),
            composition: signal(None),
            sink_value: signal(String::new()),
            sink: Rc::default(),
            next_composition: Rc::new(Cell::new(1)),
            drag: Rc::default(),
            goal_x: Rc::default(),
            selected: Rc::default(),
            input: element_ref(),
            scroll: element_ref(),
            content: element_ref(),
            scroll_y: signal(px(0.0)),
            viewport: signal(Size::default()),
            scroll_pending: Rc::default(),
            blink_task: Rc::default(),
            base_keymap: Rc::new(base_keymap),
        };
        rv.sync_tree(&state);
        rv.sync_decorations(&state);
        rv.connect();
        rv
    }

    /// Subscribes to the session and binds the keys while focused.
    fn connect(&self) {
        // Sessions change inside JS's calls (`run`), on other threads and
        // in the view's own handlers, where the components' graph may not
        // be entered: subscribers only mark the change and wake a task,
        // which writes the view's signals on the engine thread.
        let wake = Wake::default();
        let changed = wake.clone();
        let id = self.session.subscribe(Arc::new(move |_| changed.wake()));
        let pending: Arc<std::sync::Mutex<Vec<ViewRequest>>> = Arc::default();
        let queue = pending.clone();
        let requested = wake.clone();
        let hook = self.session.attach_view(Arc::new(move |request| {
            queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(request);
            requested.wake();
        }));
        let notified = self.notified;
        spawn(async move {
            loop {
                wake.wait().await;
                notified.update(|n| *n += 1);
            }
        });
        let session = self.session.clone();
        cleanup(move || {
            session.unsubscribe(id);
            session.unsubscribe(hook);
        });
        let rv = self.clone();
        effect(move || {
            rv.notified.get();
            let requests: Vec<ViewRequest> =
                std::mem::take(&mut *pending.lock().unwrap_or_else(|e| e.into_inner()));
            untracked(|| {
                let state = rv.session.state();
                rv.take_state(state);
                for request in requests {
                    match request {
                        ViewRequest::Focus => rv.focus(),
                        ViewRequest::Blur => rv.blur(),
                    }
                }
            });
        });
        let rv = self.clone();
        effect(move || {
            if !rv.focused.get() {
                return;
            }
            for spec in keys::key_table() {
                let rv = rv.clone();
                let event = spec.event.clone();
                on_key_with(
                    &spec.keystroke,
                    KeyOptions {
                        when: When::Always,
                        repeat: spec.repeat,
                    },
                    move |_| rv.key(&event),
                );
            }
        });
        let rv = self.clone();
        effect(move || {
            if rv.props.auto_focus.get() {
                rv.focus();
            }
        });
        // Scrolls the selection into view once its place is known.
        let rv = self.clone();
        effect(move || {
            rv.layout_rev.get();
            rv.sel_rev.get();
            if rv.scroll_pending.get() {
                untracked(|| rv.scroll_into_view());
            }
        });
    }

    /// Renders `state`: the blocks when the document changed, the
    /// decorations, the selection; reports the changes.
    pub(crate) fn take_state(&self, state: EditorState) {
        let old = std::mem::replace(&mut *self.st.borrow_mut(), state.clone());
        let doc_changed = !old.doc().ptr_eq(state.doc());
        if doc_changed {
            self.sync_tree(&state);
            self.doc_rev.update(|r| *r += 1);
        }
        if self.sync_decorations(&state) {
            self.deco_rev.update(|r| *r += 1);
        }
        let selection_changed = doc_changed
            || old.selection().json_id() != state.selection().json_id()
            || old.selection().anchor() != state.selection().anchor()
            || old.selection().head() != state.selection().head()
            || old.selection().affinity() != state.selection().affinity();
        if selection_changed {
            self.sel_rev.update(|r| *r += 1);
            self.sync_node_selection(&state);
        }
        if state.scroll_to_selection() != old.scroll_to_selection() {
            self.scroll_pending.set(true);
        }
        if doc_changed {
            self.cx.emit(RichEditorEvent::Change(RichEditorChange {
                revision: state.revision(),
            }));
        }
        if selection_changed {
            self.cx
                .emit(RichEditorEvent::SelectionChange(RichEditorSelection::of(
                    &state,
                )));
        }
        self.restart_blink();
    }

    // The tree of blocks.

    /// Gives the blocks of `state`'s document their ids: a block keeps its
    /// id while it is the same node, or the node at its index with its
    /// type, so a changed block patches its views rather than making them
    /// again.
    fn sync_tree(&self, state: &EditorState) {
        let mut tree = self.tree.borrow_mut();
        let mut geo = self.geo.borrow_mut();
        let views = self.node_views.borrow();
        let mut alive = HashSet::new();
        let mut texts = HashMap::new();
        let mut atoms = HashMap::new();
        walk(
            &mut tree,
            &views,
            DOC,
            DOC,
            state.doc(),
            0,
            &mut alive,
            &mut texts,
            &mut atoms,
        );
        tree.children
            .retain(|id, _| *id == DOC || alive.contains(id));
        geo.frames
            .retain(|id, _| alive.contains(&(id & !(1 << 62))));
        let mut old_texts = std::mem::take(&mut geo.texts);
        for (id, start) in texts {
            let mut entry = old_texts.remove(&id).unwrap_or_default();
            entry.start = start;
            geo.texts.insert(id, entry);
        }
        geo.atoms = atoms;
    }

    /// The plugins' decorations of `state`: whether they changed.
    fn sync_decorations(&self, state: &EditorState) -> bool {
        let mut all = Vec::new();
        for plugin in state.plugins() {
            if let Some(set) = plugin.decorations(state) {
                all.extend(set.iter().cloned());
            }
        }
        let mut current = self.decorations.borrow_mut();
        if *current == all {
            return false;
        }
        *current = all;
        true
    }

    /// Tells node views a node selection took or left their node.
    fn sync_node_selection(&self, state: &EditorState) {
        let now = state
            .selection()
            .as_node()
            .map(|n| (state.selection().from(), n.node().clone()));
        let mut selected = self.selected.borrow_mut();
        let same = match (&*selected, &now) {
            (Some((a, x)), Some((b, y))) => a == b && x.ptr_eq(y),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some((_, node)) = selected.take()
            && let Some(view) = self.node_view(node.ty().name())
        {
            view.deselect();
        }
        if let Some((_, node)) = &now
            && let Some(view) = self.node_view(node.ty().name())
        {
            view.select();
        }
        *selected = now;
    }

    /// The node view of type `name`: the session's, else a plugin's.
    pub(crate) fn node_view(&self, name: &str) -> Option<Arc<dyn NodeView>> {
        if let Some(view) = self.node_views.borrow().get(name) {
            return Some(view.clone());
        }
        let state = self.st.borrow();
        for plugin in state.plugins() {
            for (n, factory) in plugin.node_views() {
                if n == name
                    && let Some(view) = factory.downcast_ref::<Arc<dyn NodeView>>()
                {
                    return Some(view.clone());
                }
            }
        }
        None
    }

    /// The decorations over `from..to`.
    pub(crate) fn decorations_in(&self, from: usize, to: usize) -> Vec<Decoration> {
        self.decorations
            .borrow()
            .iter()
            .filter(|d| d.from <= to && d.to >= from)
            .cloned()
            .collect()
    }

    // Committing.

    fn dispatch(&self, tr: Transaction) {
        if let Err(e) = self.session.dispatch(tr) {
            log::warn!("RichEditor: a transaction was refused: {e}");
        }
    }

    /// Whether the document can be edited: the prop and every plugin.
    pub(crate) fn editable(&self) -> bool {
        if !self.props.editable.get_untracked() {
            return false;
        }
        let state = self.session.state();
        state
            .plugins()
            .iter()
            .all(|p| p.editable(&state) != Some(false))
    }

    /// Asks each plugin's hook in turn until one handles the event,
    /// dispatching what it does.
    fn some_hook(
        &self,
        state: &EditorState,
        hook: impl Fn(&Plugin, &mut dyn FnMut(Transaction)) -> bool,
    ) -> bool {
        let mut trs = Vec::new();
        let handled = state
            .plugins()
            .iter()
            .any(|p| hook(p, &mut |tr| trs.push(tr)));
        for tr in trs {
            self.dispatch(tr);
        }
        handled
    }

    // Text.

    /// Text typed at the selection: the plugins' `handle_text_input`
    /// first (input rules), then inserted with the stored marks.
    pub(crate) fn text_input(&self, text: &str, composed: bool) {
        if text.is_empty() || !self.editable() {
            return;
        }
        let state = self.session.state();
        let (from, to) = (state.selection().from(), state.selection().to());
        if self.some_hook(&state, |p, d| {
            p.handle_text_input(&state, from, to, text, d)
        }) {
            return;
        }
        let mut tr = state.tr();
        tr.insert_text(text, None, None);
        tr.set_origin(TransactionOrigin::Input);
        tr.scroll_into_view();
        if composed && let Some(id) = self.sink.borrow().composition {
            tr.set_meta("composition", Value::from(id));
        }
        self.dispatch(tr);
    }

    /// The sink's text changed: what was deleted and typed since.
    pub(crate) fn sink_changed(&self, text: &str) {
        let (deleted, inserted, composed) = {
            let mut sink = self.sink.borrow_mut();
            let base = match sink.emptied.take() {
                Some(old) if text.starts_with(old.as_str()) => old,
                Some(_) => String::new(),
                None => std::mem::take(&mut sink.text),
            };
            let prefix = common_prefix(&base, text);
            let deleted = base[prefix..].chars().count();
            let inserted = text[prefix..].to_owned();
            sink.text = text.to_owned();
            (deleted, inserted, sink.composition.is_some())
        };
        self.sink_value.set(text.to_owned());
        for _ in 0..deleted {
            self.key(&KeyEvent::new("Backspace"));
        }
        let mut parts = inserted.split('\n').peekable();
        while let Some(part) = parts.next() {
            self.text_input(part, composed);
            if parts.peek().is_some() {
                self.key(&KeyEvent::new("Enter"));
            }
        }
        {
            let mut sink = self.sink.borrow_mut();
            if !sink.composing {
                sink.composition = None;
            }
        }
        if self.sink.borrow().text.len() > SINK_LIMIT {
            self.empty_sink();
        }
    }

    /// Empties the sink, unless an input method is composing there.
    fn empty_sink(&self) {
        let mut sink = self.sink.borrow_mut();
        if sink.composing || sink.text.is_empty() {
            return;
        }
        sink.emptied = Some(std::mem::take(&mut sink.text));
        drop(sink);
        self.sink_value.set(String::new());
    }

    /// An input method's composition started, changed or ended: its text
    /// shows at the caret, the document waits for the commit (one
    /// transaction, grouped in the history with the composition's).
    pub(crate) fn composition_changed(&self, composing: bool, text: &str) {
        {
            let mut sink = self.sink.borrow_mut();
            if composing && sink.composition.is_none() {
                let id = self.next_composition.get();
                self.next_composition.set(id + 1);
                sink.composition = Some(id);
            }
            sink.composing = composing;
        }
        self.session.set_composing(composing);
        self.composition
            .set((composing && !text.is_empty()).then(|| text.to_owned()));
    }

    /// A key the sink reports itself (soft keyboards: Backspace in an
    /// empty sink).
    pub(crate) fn sink_key(&self, key: &str) {
        if key == "Backspace" && self.sink.borrow().text.is_empty() {
            self.key(&KeyEvent::new("Backspace"));
        }
    }

    // Keys.

    /// A key: the selected node's view, the plugins' `handle_key_down`
    /// (keymaps), the base keymap, the clipboard keys, then the view's
    /// caret movement.
    pub(crate) fn key(&self, event: &KeyEvent) {
        if self.sink.borrow().composing {
            return;
        }
        let state = self.session.state();
        if let Some(node) = state.selection().as_node()
            && let Some(view) = self.node_view(node.node().ty().name())
            && view.handle_event(&NodeViewEvent::Key(event.clone()))
        {
            return;
        }
        let editable = self.editable();
        if editable {
            if self.some_hook(&state, |p, d| p.handle_key_down(&state, event, d)) {
                self.after_key(event);
                return;
            }
            let mut trs = Vec::new();
            if self
                .base_keymap
                .handle(&state, event, &mut |tr| trs.push(tr))
            {
                for mut tr in trs {
                    tr.scroll_into_view();
                    self.dispatch(tr);
                }
                self.after_key(event);
                return;
            }
        }
        if event.mod_key && !event.alt && !event.shift {
            match event.key.as_str() {
                "c" => return self.copy(),
                "x" => {
                    self.copy();
                    if editable {
                        self.delete_selection();
                    }
                    return;
                }
                "a" => return self.select_all(),
                _ => {}
            }
        }
        if let Some(m) = keys::movement(event) {
            self.move_caret(m);
        }
    }

    fn after_key(&self, event: &KeyEvent) {
        if !matches!(event.key.as_str(), "ArrowUp" | "ArrowDown") {
            self.goal_x.set(None);
        }
        self.empty_sink();
    }

    fn delete_selection(&self) {
        let state = self.session.state();
        if state.selection().empty() {
            return;
        }
        let mut tr = state.tr();
        tr.delete_selection();
        tr.scroll_into_view();
        self.dispatch(tr);
    }

    fn select_all(&self) {
        let state = self.session.state();
        let mut tr = state.tr();
        tr.set_selection(crate::rich_editor::state::AllSelection::new(state.doc()).into());
        self.dispatch(tr);
    }

    /// Moves (or extends) the selection.
    fn move_caret(&self, m: Move) {
        let state = self.session.state();
        let doc = state.doc().clone();
        let sel = state.selection().clone();
        let vertical = matches!(m.unit, Unit::Line | Unit::Page);
        if !vertical {
            self.goal_x.set(None);
        }
        // Without Shift a range collapses to its side first.
        if !m.extend && !sel.empty() && m.unit == Unit::Char {
            let pos = if m.dir < 0 { sel.from() } else { sel.to() };
            let next = match doc.resolve(pos) {
                Ok(p) => Selection::near(&p, m.dir),
                Err(_) => return,
            };
            return self.set_selection(next, false);
        }
        let head = sel.head();
        let affinity = sel.affinity();
        let target: Option<(usize, Affinity, Option<Selection>)> = match m.unit {
            Unit::Char => self
                .char_step(&doc, &sel, m.dir)
                .map(|s| (s.head(), Affinity::Downstream, Some(s))),
            Unit::Word => self
                .word_step(&doc, head, m.dir)
                .map(|p| (p, Affinity::Downstream, None)),
            Unit::LineEdge => {
                let geo = self.geo.borrow();
                geo.line_edge(head, affinity, m.dir > 0).map(|p| {
                    let side = if m.dir > 0 {
                        Affinity::Upstream
                    } else {
                        Affinity::Downstream
                    };
                    (p, side, None)
                })
            }
            Unit::Doc => {
                let s = if m.dir < 0 {
                    Selection::at_start(&doc)
                } else {
                    Selection::at_end(&doc)
                };
                Some((s.head(), Affinity::Downstream, Some(s)))
            }
            Unit::Line | Unit::Page => self.vertical(head, affinity, m),
        };
        let Some((pos, side, whole)) = target else {
            return;
        };
        let next = if m.extend {
            let anchor = sel.anchor();
            match (doc.resolve(anchor), doc.resolve(pos)) {
                (Ok(a), Ok(h)) => TextSelection::between(&a, &h, Some(m.dir)),
                _ => return,
            }
        } else if let Some(whole) = whole {
            whole
        } else {
            match doc.resolve(pos) {
                Ok(p) if p.parent().inline_content() => {
                    TextSelection::with_affinity(p.clone(), p, side).into()
                }
                Ok(p) => Selection::near(&p, m.dir),
                Err(_) => return,
            }
        };
        self.set_selection(next, true);
    }

    /// One grapheme toward `dir` from the head, out of the textblock at
    /// its ends (to the next cursor or selectable node).
    fn char_step(&self, doc: &Node, sel: &Selection, dir: i32) -> Option<Selection> {
        if let Some(node) = sel.as_node() {
            let _ = node;
            let pos = if dir > 0 { sel.to() } else { sel.from() };
            return Selection::find_from(&doc.resolve(pos).ok()?, dir, false);
        }
        let head = sel.head();
        let p = doc.resolve(head).ok()?;
        if p.parent().inline_content() {
            let start = p.start(p.depth);
            let text = inline_text(p.parent());
            let off = head - start;
            let next = if dir > 0 {
                next_boundary(&text, off)
            } else {
                prev_boundary(&text, off)
            };
            if let Some(next) = next {
                let at = doc.resolve(start + next).ok()?;
                return Some(TextSelection::new(at.clone(), at).into());
            }
            if p.depth == 0 {
                return None;
            }
            let edge = if dir > 0 {
                p.after(p.depth)
            } else {
                p.before(p.depth)
            };
            return Selection::find_from(&doc.resolve(edge).ok()?, dir, false);
        }
        Selection::find_from(&p, dir, false)
    }

    /// The next word's edge toward `dir`, within the head's textblock
    /// (its edge, then the next block's).
    fn word_step(&self, doc: &Node, head: usize, dir: i32) -> Option<usize> {
        let p = doc.resolve(head).ok()?;
        if !p.parent().inline_content() {
            return None;
        }
        let start = p.start(p.depth);
        let text = inline_text(p.parent());
        let off = head - start;
        let words = word_bounds(&text);
        let found = if dir > 0 {
            words.iter().map(|w| w.1).find(|end| *end > off)
        } else {
            words.iter().rev().map(|w| w.0).find(|s| *s < off)
        };
        match found {
            Some(o) => Some(start + o),
            None => self
                .char_step(doc, &TextSelection::new(p.clone(), p).into(), dir)
                .map(|s| s.head()),
        }
    }

    /// The position a line (or a page) up or down, at the goal x.
    fn vertical(
        &self,
        head: usize,
        affinity: Affinity,
        m: Move,
    ) -> Option<(usize, Affinity, Option<Selection>)> {
        let geo = self.geo.borrow();
        let caret = geo.caret_rect(head, affinity)?;
        let x = self.goal_x.get().unwrap_or(caret.origin.x);
        self.goal_x.set(Some(x));
        let distance = match m.unit {
            Unit::Page => self.viewport.get_untracked().height.max(caret.size.height),
            _ => caret.size.height * 0.5 + px(1.0),
        };
        let y = if m.dir < 0 {
            caret.origin.y - distance + caret.size.height * 0.5 - caret.size.height * 0.5 - px(0.0)
        } else {
            caret.origin.y + caret.size.height + distance - caret.size.height * 0.5
        };
        let hit = geo.hit(Point::new(x, if m.dir < 0 { y - px(1.0) } else { y }))?;
        let (pos, side) = match hit {
            Hit::Text(pos, side) => (pos, side),
            Hit::Node(pos) => {
                drop(geo);
                let doc = self.session.state().doc().clone();
                return Some((
                    pos,
                    Affinity::Downstream,
                    Some(NodeSelection::create(&doc, pos)),
                ));
            }
        };
        if pos == head {
            // Past the first or last line: to the document's edge.
            let doc = self.session.state().doc().clone();
            let s = if m.dir < 0 {
                Selection::at_start(&doc)
            } else {
                Selection::at_end(&doc)
            };
            return Some((s.head(), Affinity::Downstream, None));
        }
        Some((pos, side, None))
    }

    /// Selects `selection` (a move: it scrolls into view).
    fn set_selection(&self, selection: Selection, scroll: bool) {
        let state = self.session.state();
        let mut tr = state.tr();
        tr.set_selection(selection);
        if scroll {
            tr.scroll_into_view();
        }
        self.dispatch(tr);
        self.empty_sink();
    }

    /// Selects `anchor..head` (the ref's `setSelection`).
    pub(crate) fn select_range(&self, anchor: usize, head: usize) {
        let state = self.session.state();
        let doc = state.doc();
        let size = doc.content().size();
        let (Ok(a), Ok(h)) = (doc.resolve(anchor.min(size)), doc.resolve(head.min(size))) else {
            return;
        };
        let selection = if anchor == head
            && let Some(node) = h.node_after()
            && !node.is_inline()
            && NodeSelection::is_selectable(&node)
        {
            NodeSelection::create(doc, head)
        } else {
            TextSelection::between(&a, &h, None)
        };
        self.set_selection(selection, true);
    }

    /// Replaces the document (the ref's `setDocJson`).
    pub(crate) fn set_doc_json(&self, json: &Value) -> bool {
        let state = self.session.state();
        let Ok(doc) = state.schema().node_from_json(json) else {
            return false;
        };
        let mut tr = state.tr();
        let size = state.doc().content().size();
        if tr.replace_with(0, size, doc.content().clone()).is_err() {
            return false;
        }
        tr.set_origin(TransactionOrigin::Api);
        self.session.dispatch(tr).is_ok()
    }

    // The clipboard.

    pub(crate) fn copy(&self) {
        let state = self.session.state();
        if state.selection().empty() {
            return;
        }
        let slice = state.selection().content();
        let text = state
            .some_prop(|p| p.clipboard_text_serializer(&state, &slice))
            .unwrap_or_else(|| slice_to_text(&slice));
        set_clipboard(text);
    }

    /// A paste of `text`: the plugins change and parse it, then
    /// `handle_paste` or a replace of the selection.
    pub(crate) fn paste(&self, text: &str) {
        if !self.editable() || text.is_empty() {
            return;
        }
        let state = self.session.state();
        let mut text = text.to_owned();
        for plugin in state.plugins() {
            text = plugin.transform_pasted_text(&state, text, false);
        }
        let from = state.selection().resolved_from().clone();
        let mut slice = state
            .some_prop(|p| p.clipboard_text_parser(&state, &text, &from, false))
            .unwrap_or_else(|| slice_from_text(state.schema(), &text, &from));
        for plugin in state.plugins() {
            slice = plugin.transform_pasted(&state, slice);
        }
        if self.some_hook(&state, |p, d| p.handle_paste(&state, &slice, d)) {
            return;
        }
        let mut tr = state.tr();
        tr.replace_selection(slice);
        tr.set_origin(TransactionOrigin::Paste);
        tr.set_meta("paste", Value::Bool(true));
        tr.scroll_into_view();
        self.dispatch(tr);
        self.empty_sink();
    }

    // Presses.

    /// A press at `at` in the content box.
    pub(crate) fn press(
        &self,
        at: Point<Points>,
        page: Point<Points>,
        modifiers: Modifiers,
        clicks: u32,
    ) {
        self.focus();
        self.goal_x.set(None);
        let hit = self.geo.borrow().hit(at);
        let Some(hit) = hit else { return };
        let state = self.session.state();
        let doc = state.doc().clone();
        let mod_key = if matches!(os(), Os::MacOs | Os::Ios) {
            modifiers.cmd
        } else {
            modifiers.ctrl
        };
        let click = ClickEvent {
            button: 0,
            mod_key,
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
            meta: modifiers.cmd,
        };
        let pos = match hit {
            Hit::Text(pos, _) | Hit::Node(pos) => pos,
        };
        if let Hit::Node(node_pos) = hit
            && let Some(node) = doc.node_at(node_pos)
            && let Some(view) = self.node_view(node.ty().name())
        {
            let origin = self.node_origin(node_pos);
            let local = Point::new(at.x - origin.x, at.y - origin.y);
            if view.handle_event(&NodeViewEvent::Press {
                at: local,
                click: click.clone(),
            }) {
                return;
            }
        }
        if self.click_hooks(&state, pos, &click, clicks) {
            return;
        }
        let selection = match (hit, clicks) {
            (Hit::Node(p), _) => {
                let Some(node) = doc.node_at(p) else { return };
                if !NodeSelection::is_selectable(&node) {
                    return;
                }
                NodeSelection::create(&doc, p)
            }
            (Hit::Text(p, side), 1) => {
                let Ok(head) = doc.resolve(p) else { return };
                if modifiers.shift {
                    let Ok(anchor) = doc.resolve(state.selection().anchor()) else {
                        return;
                    };
                    self.between(&state, &anchor, &head)
                } else {
                    TextSelection::with_affinity(head.clone(), head, side).into()
                }
            }
            (Hit::Text(p, _), 2) => match word_at(&doc, p) {
                Some((a, b)) => TextSelection::create(&doc, a, Some(b)),
                None => return,
            },
            (Hit::Text(p, _), _) => {
                let Ok(r) = doc.resolve(p) else { return };
                if r.depth == 0 {
                    return;
                }
                TextSelection::create(&doc, r.start(r.depth), Some(r.end(r.depth)))
            }
        };
        let anchor = selection.anchor();
        self.set_selection(selection, false);
        self.drag.set(Some(Drag {
            anchor,
            shift: Point::new(at.x - page.x, at.y - page.y),
            scroll: self.scroll_y.get_untracked(),
        }));
    }

    /// The plugins' click hooks of a `clicks`-fold press at `pos`: each
    /// node around it, innermost first, then the position.
    fn click_hooks(
        &self,
        state: &EditorState,
        pos: usize,
        click: &ClickEvent,
        clicks: u32,
    ) -> bool {
        let Ok(r) = state.doc().resolve(pos) else {
            return false;
        };
        let direct_node = state.doc().node_at(pos).filter(|n| !n.is_text());
        let mut nodes: Vec<(Node, usize, bool)> = Vec::new();
        if let Some(n) = &direct_node
            && !n.is_inline()
        {
            nodes.push((n.clone(), pos, true));
        }
        for depth in (1..=r.depth).rev() {
            nodes.push((r.node(depth).clone(), r.before(depth), false));
        }
        for (node, node_pos, direct) in &nodes {
            let handled = self.some_hook(state, |p, d| match clicks {
                1 => p.handle_click_on(state, pos, node, *node_pos, click, *direct, d),
                2 => p.handle_double_click_on(state, pos, node, *node_pos, click, *direct, d),
                _ => p.handle_triple_click_on(state, pos, node, *node_pos, click, *direct, d),
            });
            if handled {
                return true;
            }
        }
        self.some_hook(state, |p, d| match clicks {
            1 => p.handle_click(state, pos, click, d),
            2 => p.handle_double_click(state, pos, click, d),
            _ => p.handle_triple_click(state, pos, click, d),
        })
    }

    /// The selection a pointer makes from `anchor` to `head`: a plugin's,
    /// else a text selection.
    fn between(
        &self,
        state: &EditorState,
        anchor: &crate::rich_editor::model::ResolvedPos,
        head: &crate::rich_editor::model::ResolvedPos,
    ) -> Selection {
        state
            .some_prop(|p| p.create_selection_between(state, anchor, head))
            .unwrap_or_else(|| TextSelection::between(anchor, head, None))
    }

    /// The pointer dragged to `page` (root): the selection extends there.
    pub(crate) fn drag_to(&self, page: Point<Points>) {
        let Some(drag) = self.drag.get() else { return };
        let scrolled = self.scroll_y.get_untracked() - drag.scroll;
        let at = Point::new(page.x + drag.shift.x, page.y + drag.shift.y + scrolled);
        let hit = self.geo.borrow().hit(at);
        let pos = match hit {
            Some(Hit::Text(p, _) | Hit::Node(p)) => p,
            None => return,
        };
        let state = self.session.state();
        let doc = state.doc();
        let (Ok(a), Ok(h)) = (doc.resolve(drag.anchor), doc.resolve(pos)) else {
            return;
        };
        let selection = self.between(&state, &a, &h);
        let current = state.selection();
        if selection.anchor() == current.anchor() && selection.head() == current.head() {
            return;
        }
        let mut tr = state.tr();
        tr.set_selection(selection);
        self.dispatch(tr);
    }

    pub(crate) fn release(&self) {
        self.drag.set(None);
    }

    /// Block `pos`'s top left in the content box.
    fn node_origin(&self, pos: usize) -> Point<Points> {
        let tree = self.tree.borrow();
        let id = tree
            .children
            .values()
            .flatten()
            .find(|b| b.pos == pos)
            .map(|b| b.id);
        id.and_then(|id| self.geo.borrow().origin(id))
            .unwrap_or_default()
    }

    // Focus and the caret.

    pub(crate) fn focus(&self) {
        self.input.focus();
    }

    pub(crate) fn blur(&self) {
        self.input.blur();
    }

    pub(crate) fn focus_changed(&self, focused: bool) {
        if self.focused.get_untracked() == focused {
            return;
        }
        self.focused.set(focused);
        self.cx.emit(if focused {
            RichEditorEvent::Focus
        } else {
            RichEditorEvent::Blur
        });
        if !focused {
            self.drag.set(None);
        }
        self.restart_blink();
    }

    fn restart_blink(&self) {
        if let Some(task) = self.blink_task.borrow_mut().take() {
            task.cancel();
        }
        self.blink.set(true);
        if !self.focused.get_untracked() {
            return;
        }
        let blink = self.blink;
        let task = set_interval(Duration::from_millis(530), move || {
            blink.update(|b| *b = !*b);
        });
        *self.blink_task.borrow_mut() = Some(task);
    }

    /// The caret's box in the content box (the selection's head).
    pub(crate) fn caret_rect(&self) -> Option<Bounds<Points>> {
        let state = self.st.borrow();
        let sel = state.selection();
        self.geo.borrow().caret_rect(sel.head(), sel.affinity())
    }

    /// Scrolls the head into view, once it is laid out.
    fn scroll_into_view(&self) {
        let state = self.session.state();
        if state
            .plugins()
            .iter()
            .any(|p| p.handle_scroll_to_selection(&state))
        {
            self.scroll_pending.set(false);
            return;
        }
        let Some(caret) = self.caret_rect() else {
            return;
        };
        self.scroll_pending.set(false);
        let top = self.scroll_y.get_untracked();
        let height = self.viewport.get_untracked().height;
        if height <= px(0.0) {
            return;
        }
        let y = if caret.origin.y < top {
            caret.origin.y
        } else if caret.origin.y + caret.size.height > top + height {
            caret.origin.y + caret.size.height - height
        } else {
            return;
        };
        self.scroll
            .scroll_to(Point::new(px(0.0), y.max(px(0.0))), false);
    }
}

/// The walk of [`Rv::sync_tree`] over `node`'s children (content starting
/// at `start`), rendered in box `parent`.
#[allow(clippy::too_many_arguments, reason = "the walk's accumulators")]
fn walk(
    tree: &mut Tree,
    views: &NodeViews,
    id: u64,
    parent: u64,
    node: &Node,
    start: usize,
    alive: &mut HashSet<u64>,
    texts: &mut HashMap<u64, usize>,
    atoms: &mut HashMap<u64, usize>,
) {
    let old = tree.children.remove(&id).unwrap_or_default();
    let mut used = vec![false; old.len()];
    let by_ptr: Vec<usize> = Vec::new();
    let _ = by_ptr;
    let mut items = Vec::with_capacity(node.child_count());
    let mut pos = start;
    for (index, child) in node.content().iter().enumerate() {
        // The same node first, then the node at its index with its type.
        let matched = old
            .iter()
            .enumerate()
            .find(|(i, b)| !used[*i] && b.node.ptr_eq(child))
            .map(|(i, _)| i)
            .or_else(|| {
                old.get(index)
                    .filter(|b| !used[index] && b.node.ty() == child.ty())
                    .map(|_| index)
            });
        let child_id = match matched {
            Some(i) => {
                used[i] = true;
                old[i].id
            }
            None => {
                tree.next += 1;
                tree.next
            }
        };
        alive.insert(child_id);
        items.push(BlockItem {
            id: child_id,
            node: child.clone(),
            pos,
            parent,
        });
        let view = views.get(child.ty().name());
        let atom = match view {
            Some(v) => !v.editable_content_slot(),
            None => child.is_leaf() || child.is_atom() && !child.is_textblock(),
        };
        if atom {
            atoms.insert(child_id, pos);
        } else if child.is_textblock() {
            texts.insert(child_id, pos + 1);
        } else {
            let box_id = if view.is_some() {
                slot_id(child_id)
            } else {
                child_id
            };
            walk(
                tree,
                views,
                child_id,
                box_id,
                child,
                pos + 1,
                alive,
                texts,
                atoms,
            );
        }
        pos += child.node_size();
    }
    tree.children.insert(id, items);
}

/// The session of a view without one: from the `schema`, `content` and
/// `plugins` props.
fn own_session(props: &RichEditorProps) -> EditorSession {
    let schema = props
        .schema
        .get_untracked()
        .and_then(|json| match SchemaSpec::from_json(&json) {
            Ok(schema) => Some(schema),
            Err(e) => {
                log::warn!("RichEditor: the schema cannot be read ({e}); the starter's is used");
                None
            }
        })
        .unwrap_or_else(starter::schema);
    let names: Vec<String> = props
        .plugins
        .get_untracked()
        .map(|list| list.iter().map(|s| s.to_string()).collect())
        .unwrap_or_else(|| vec!["starter".into()]);
    let starter_on = names.iter().any(|n| n == "starter");
    let mut plugins: Vec<Plugin> = Vec::new();
    for name in &names {
        match name.as_str() {
            "starter" => plugins.extend(starter::plugins(&schema)),
            "history" if !starter_on => plugins.push(history(HistoryOptions::default())),
            "baseKeymap" if !starter_on => {
                plugins.push(crate::rich_editor::extensions::keymap(base_keymap()))
            }
            "markdown" => plugins.extend(crate::rich_editor::extensions::markdown::markdown(
                Default::default(),
            )),
            "history" | "baseKeymap" => {}
            other => log::warn!("RichEditor: no built-in plugins named {other:?}"),
        }
    }
    let mut config = EditorStateConfig::new(schema.clone()).plugins(plugins.clone());
    if let Some(json) = props.content.get_untracked() {
        match schema.node_from_json(&json) {
            Ok(doc) => config = config.doc(doc),
            Err(e) => log::warn!("RichEditor: the content cannot be read: {e}"),
        }
    }
    let state = EditorState::create(config).unwrap_or_else(|e| {
        log::warn!("RichEditor: the state cannot be made ({e}); an empty one is used");
        EditorState::create(EditorStateConfig::new(schema))
            .expect("an empty state of a valid schema")
    });
    EditorSession::new(state, NodeViews::new())
}

/// The base keymap of the platform the view runs on.
fn base_keymap_for(mac: bool) -> Vec<(String, crate::rich_editor::commands::BoxCommand)> {
    if mac {
        crate::rich_editor::commands::mac_base_keymap()
    } else {
        crate::rich_editor::commands::pc_base_keymap()
    }
}

/// A textblock's content as text: its texts, and U+FFFC for each inline
/// leaf, so offsets are the content's.
pub(crate) fn inline_text(block: &Node) -> String {
    let mut out = String::new();
    for child in block.content().iter() {
        match child.text() {
            Some(t) => out.push_str(t),
            None => {
                for _ in 0..child.node_size() {
                    out.push('\u{FFFC}');
                }
            }
        }
    }
    out
}

/// The grapheme boundaries of `text` as UTF-16 offsets.
fn boundaries(text: &str) -> Vec<usize> {
    let mut out = vec![0];
    let mut at = 0;
    for g in text.graphemes(true) {
        at += g.encode_utf16().count();
        out.push(at);
    }
    out
}

fn next_boundary(text: &str, off: usize) -> Option<usize> {
    boundaries(text).into_iter().find(|b| *b > off)
}

fn prev_boundary(text: &str, off: usize) -> Option<usize> {
    boundaries(text).into_iter().rev().find(|b| *b < off)
}

/// The words of `text` (UTF-16 ranges).
fn word_bounds(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    for w in text.split_word_bounds() {
        let len = w.encode_utf16().count();
        if w.chars().any(|c| c.is_alphanumeric()) {
            out.push((at, at + len));
        }
        at += len;
    }
    out
}

/// The word at `pos` (document positions), for a double press.
fn word_at(doc: &Node, pos: usize) -> Option<(usize, usize)> {
    let p = doc.resolve(pos).ok()?;
    if !p.parent().inline_content() {
        return None;
    }
    let start = p.start(p.depth);
    let text = inline_text(p.parent());
    let off = pos - start;
    let mut at = 0;
    for w in text.split_word_bounds() {
        let len = w.encode_utf16().count();
        if off >= at && off < at + len
            || (off == at + len && at + len == text.encode_utf16().count())
        {
            return Some((start + at, start + at + len));
        }
        at += len;
    }
    Some((pos, pos))
}

/// The bytes `a` and `b` start with alike, on a char boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map(|((i, _), _)| i)
        .unwrap_or_else(|| a.len().min(b.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes_step_whole() {
        let text = "ae\u{301}👍🏽b";
        let b = boundaries(text);
        assert_eq!(b, [0, 1, 3, 7, 8]);
        assert_eq!(next_boundary(text, 1), Some(3));
        assert_eq!(prev_boundary(text, 7), Some(3));
    }

    #[test]
    fn words_skip_punctuation() {
        assert_eq!(word_bounds("hi, you"), [(0, 2), (4, 7)]);
    }
}

/// A wake-up that any thread or call may signal and one task awaits:
/// several signals before the task runs count as one.
#[derive(Clone, Default)]
struct Wake(Arc<std::sync::Mutex<(bool, Option<std::task::Waker>)>>);

impl Wake {
    fn wake(&self) {
        let waker = {
            let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
            inner.0 = true;
            inner.1.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    async fn wait(&self) {
        std::future::poll_fn(|cx| {
            let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if std::mem::take(&mut inner.0) {
                std::task::Poll::Ready(())
            } else {
                inner.1 = Some(cx.waker().clone());
                std::task::Poll::Pending
            }
        })
        .await;
    }
}

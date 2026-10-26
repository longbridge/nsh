//! [`EditorSession`]: the one live editor a `RichEditor` view and the app's
//! code share. It holds the current [`EditorState`], applies transactions
//! (or hands them to the app's `dispatch_transaction`), runs commands and
//! tells subscribers about each new state. The view is one subscriber; JS's
//! class over the session is another.

use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use nsh_core::ui::{JsHost, JsIn, JsOwned, JsValue, PropError, TsTypes};

use super::node_view::NodeView;
use crate::rich_editor::{
    commands::{BoxCommand, is_mark_active, is_node_active},
    model::Attrs,
    state::{EditorState, StateError, Transaction},
};

/// What a subscriber of a session is known by ([`EditorSession::subscribe`]).
pub type SubscriptionId = u64;

/// A subscriber of a session: called with every new state.
pub type Subscriber = Arc<dyn Fn(&EditorState) + Send + Sync>;

/// The app's own commit of transactions
/// ([`EditorSession::set_dispatch_transaction`]).
pub type DispatchTransaction = Arc<dyn Fn(&EditorSession, Transaction) + Send + Sync>;

/// The node views of an editor: by node type name, what renders nodes of
/// that type ([`NodeView`]).
#[derive(Clone, Default)]
pub struct NodeViews(HashMap<String, Arc<dyn NodeView>>);

impl NodeViews {
    /// No node views: every node renders as the view's default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders nodes of type `name` with `view` (replacing another one).
    pub fn insert(&mut self, name: impl Into<String>, view: Arc<dyn NodeView>) -> &mut Self {
        self.0.insert(name.into(), view);
        self
    }

    /// [`NodeViews::insert`], by value.
    pub fn with(mut self, name: impl Into<String>, view: Arc<dyn NodeView>) -> Self {
        self.insert(name, view);
        self
    }

    /// The view of nodes of type `name`.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn NodeView>> {
        self.0.get(name)
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for NodeViews {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.0.keys()).finish()
    }
}

/// What a session asks of the views showing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ViewRequest {
    /// Take the keyboard focus.
    Focus,
    /// Give the keyboard focus up.
    Blur,
}

/// A view's handler of [`ViewRequest`]s.
pub(crate) type ViewHook = Arc<dyn Fn(ViewRequest) + Send + Sync>;

struct Inner {
    state: EditorState,
    node_views: NodeViews,
    dispatch: Option<DispatchTransaction>,
    subscribers: Vec<(SubscriptionId, Subscriber)>,
    views: Vec<(SubscriptionId, ViewHook)>,
}

struct Shared {
    inner: Mutex<Inner>,
    next_id: AtomicU64,
    composing: AtomicBool,
}

/// The one live editor a `RichEditor` view and the app's code share:
/// cheap to clone (a handle), `Send` and `Sync`. Every change goes through
/// [`EditorSession::dispatch`]; subscribers hear every new state after it.
#[derive(Clone)]
pub struct EditorSession(Arc<Shared>);

impl PartialEq for EditorSession {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for EditorSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorSession")
            .field("revision", &self.state().revision())
            .finish_non_exhaustive()
    }
}

impl EditorSession {
    /// A session over `state`, rendering the node types `node_views` names
    /// with their views.
    pub fn new(state: EditorState, node_views: NodeViews) -> Self {
        Self(Arc::new(Shared {
            inner: Mutex::new(Inner {
                state,
                node_views,
                dispatch: None,
                subscribers: Vec::new(),
                views: Vec::new(),
            }),
            next_id: AtomicU64::new(1),
            composing: AtomicBool::new(false),
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panicking subscriber never runs under the lock: the state is
        // whole whatever poisoned it.
        self.0.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The current state.
    pub fn state(&self) -> EditorState {
        self.lock().state.clone()
    }

    /// The node views.
    pub fn node_views(&self) -> NodeViews {
        self.lock().node_views.clone()
    }

    /// Replaces the node views (views made already stay until their nodes
    /// render again).
    pub fn set_node_views(&self, node_views: NodeViews) {
        self.lock().node_views = node_views;
        let state = self.state();
        self.notify(&state);
    }

    /// Commits `tr`: through the app's `dispatch_transaction` when one is
    /// set (which commits with [`EditorSession::update_state`]), else by
    /// applying it to the current state (plugins' filters and appended
    /// transactions included) and notifying the subscribers. A transaction
    /// the state refuses changes nothing.
    pub fn dispatch(&self, tr: Transaction) -> Result<(), StateError> {
        let custom = self.lock().dispatch.clone();
        if let Some(custom) = custom {
            custom(self, tr);
            return Ok(());
        }
        let state = {
            let mut inner = self.lock();
            let (state, applied) = inner.state.apply_transaction(tr)?;
            if applied.is_empty() {
                // A filter dropped it.
                return Ok(());
            }
            inner.state = state.clone();
            state
        };
        self.notify(&state);
        Ok(())
    }

    /// Lets the app commit transactions itself (`None`: the session
    /// applies them again).
    pub fn set_dispatch_transaction(&self, dispatch: Option<DispatchTransaction>) {
        self.lock().dispatch = dispatch;
    }

    /// Replaces the state (from the app's `dispatch_transaction`, a loaded
    /// document, a reconfiguration) and notifies the subscribers.
    pub fn update_state(&self, state: EditorState) {
        self.lock().state = state.clone();
        self.notify(&state);
    }

    /// Runs `command` on the current state, dispatching what it does:
    /// whether it applied.
    pub fn run(&self, command: &BoxCommand) -> bool {
        let state = self.state();
        let mut trs = Vec::new();
        let applied = command(&state, Some(&mut |tr| trs.push(tr)));
        for tr in trs {
            if let Err(e) = self.dispatch(tr) {
                log::warn!("RichEditor: a command's transaction was refused: {e}");
            }
        }
        applied
    }

    /// Whether `command` applies to the current state (it dispatches
    /// nothing).
    pub fn can(&self, command: &BoxCommand) -> bool {
        command(&self.state(), None)
    }

    /// Whether the mark or node type `name` is active at the selection
    /// (with `attrs`, for nodes): a mark at the cursor or over the
    /// selection, a node around it or selected.
    pub fn is_active(&self, name: &str, attrs: Option<Attrs>) -> bool {
        let state = self.state();
        let schema = state.schema();
        if let Some(mark) = schema.mark_type(name) {
            return is_mark_active(&state, &mark);
        }
        match schema.node_type(name) {
            Some(node) => is_node_active(&state, &node, attrs.as_ref()),
            None => false,
        }
    }

    /// Calls `subscriber` with every new state from now on.
    pub fn subscribe(&self, subscriber: Subscriber) -> SubscriptionId {
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        self.lock().subscribers.push((id, subscriber));
        id
    }

    /// Stops calling subscriber `id` (or a view hook).
    pub fn unsubscribe(&self, id: SubscriptionId) {
        let mut inner = self.lock();
        inner.subscribers.retain(|(i, _)| *i != id);
        inner.views.retain(|(i, _)| *i != id);
    }

    /// Asks the views showing the session to take the keyboard focus.
    pub fn focus(&self) {
        self.request(ViewRequest::Focus);
    }

    /// Asks the views showing the session to give the focus up.
    pub fn blur(&self) {
        self.request(ViewRequest::Blur);
    }

    /// Whether an input method is composing in a view of the session (its
    /// text is not in the document until it commits).
    pub fn composing(&self) -> bool {
        self.0.composing.load(Ordering::Relaxed)
    }

    pub(crate) fn set_composing(&self, composing: bool) {
        self.0.composing.store(composing, Ordering::Relaxed);
    }

    /// Registers a view's handler of requests.
    pub(crate) fn attach_view(&self, hook: ViewHook) -> SubscriptionId {
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        self.lock().views.push((id, hook));
        id
    }

    fn request(&self, request: ViewRequest) {
        let views: Vec<ViewHook> = self.lock().views.iter().map(|(_, h)| h.clone()).collect();
        for view in views {
            view(request);
        }
    }

    fn notify(&self, state: &EditorState) {
        // Called outside the lock: subscribers read the session.
        let subscribers: Vec<Subscriber> = self
            .lock()
            .subscribers
            .iter()
            .map(|(_, s)| s.clone())
            .collect();
        for subscriber in subscribers {
            subscriber(state);
        }
    }
}

/// How an [`EditorSession`] crosses to JS and back, as the JS class over it
/// makes it: set once by that class's module ([`set_session_js`]), it is
/// what the `RichEditor`'s `session` prop decodes with.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct SessionJs {
    /// The session a JS value holds (an instance of the class), if any.
    pub from_js: fn(JsIn<'_>) -> Option<EditorSession>,
    /// The JS object of a session.
    pub to_js: for<'a> fn(&EditorSession, &'a dyn JsHost) -> Result<JsOwned<'a>, PropError>,
}

impl SessionJs {
    /// The bridge of a JS class: `from_js` reads a session out of an
    /// instance, `to_js` makes one.
    pub fn new(
        from_js: fn(JsIn<'_>) -> Option<EditorSession>,
        to_js: for<'a> fn(&EditorSession, &'a dyn JsHost) -> Result<JsOwned<'a>, PropError>,
    ) -> Self {
        Self { from_js, to_js }
    }
}

// Code, not state: one bridge per build, the same for every engine.
static SESSION_JS: OnceLock<SessionJs> = OnceLock::new();

/// Sets how sessions cross to JS (the JS class's module calls it once; a
/// second call is ignored).
pub fn set_session_js(bridge: SessionJs) {
    let _ = SESSION_JS.set(bridge);
}

/// The TypeScript name of the JS class over [`EditorSession`].
pub const SESSION_TS_TYPE: &str = "RichEditorSession";

/// TS: the `RichEditorSession` class's instances.
impl JsValue for EditorSession {
    fn ts_type(_types: &mut TsTypes) -> String {
        SESSION_TS_TYPE.into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        SESSION_JS
            .get()
            .and_then(|bridge| (bridge.from_js)(value))
            .ok_or_else(|| value.expected(&format!("a {SESSION_TS_TYPE}")))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match SESSION_JS.get() {
            Some(bridge) => (bridge.to_js)(self, host),
            None => Ok(JsOwned::new(host, host.null())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::rich_editor::{
        commands::command,
        extensions::starter,
        state::{EditorStateConfig, TextSelection},
    };

    fn session() -> EditorSession {
        let schema = starter::schema();
        let state = EditorState::create(
            EditorStateConfig::new(schema.clone()).plugins(starter::plugins(&schema)),
        )
        .unwrap();
        EditorSession::new(state, NodeViews::new())
    }

    #[test]
    fn dispatch_applies_and_notifies() {
        let s = session();
        let heard = Arc::new(AtomicUsize::new(0));
        let h = heard.clone();
        let id = s.subscribe(Arc::new(move |_| {
            h.fetch_add(1, Ordering::Relaxed);
        }));
        let mut tr = s.state().tr();
        tr.insert_text("hi", None, None);
        s.dispatch(tr).unwrap();
        assert_eq!(s.state().doc().text_content(), "hi");
        assert_eq!(heard.load(Ordering::Relaxed), 1);
        s.unsubscribe(id);
        let mut tr = s.state().tr();
        tr.insert_text("!", None, None);
        s.dispatch(tr).unwrap();
        assert_eq!(heard.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn the_app_can_take_over_dispatching() {
        let s = session();
        let seen = Arc::new(AtomicUsize::new(0));
        let n = seen.clone();
        s.set_dispatch_transaction(Some(Arc::new(move |session, tr| {
            n.fetch_add(1, Ordering::Relaxed);
            let next = session.state().apply(tr).unwrap();
            session.update_state(next);
        })));
        let mut tr = s.state().tr();
        tr.insert_text("x", None, None);
        s.dispatch(tr).unwrap();
        assert_eq!(seen.load(Ordering::Relaxed), 1);
        assert_eq!(s.state().doc().text_content(), "x");
    }

    #[test]
    fn commands_run_and_report_activity() {
        let s = session();
        let mut tr = s.state().tr();
        tr.insert_text("word", None, None);
        let doc = tr.transform().doc().clone();
        tr.set_selection(TextSelection::create(&doc, 1, Some(5)));
        s.dispatch(tr).unwrap();
        let bold = s.state().schema().mark_type("bold").unwrap();
        let toggle = crate::rich_editor::commands::toggle_mark(bold, None, true);
        assert!(s.can(&toggle));
        assert!(!s.is_active("bold", None));
        assert!(s.run(&toggle));
        assert!(s.is_active("bold", None));
        assert!(s.is_active("paragraph", None));
        assert!(!s.run(&command(|_, _| false)));
    }
}

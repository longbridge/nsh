//! `RichEditor`'s TS face in `nsh/base`: `defineSchema` / `starterSchema`
//! (the `RichSchema` class), `createRichEditor` (the `RichEditorSession`
//! class over an [`EditorSession`]), commands as data (`toggleMark`,
//! `setBlockType`, `chain`, ...), plugins as data (`history`, `keymap`,
//! `inputRules`, `markdown`, `starterKit`) and `RichEditorPlugin`, whose
//! hooks are the app's.
//!
//! Everything runs in Rust: a command resolves against the schema once
//! and runs in the kernel, plugins are the kernel's, and states cross to
//! JS as JSON only when the app asks (`editor.state`, `getJSON()`). The
//! input path (keys, typing, input rules) never calls JS; see `plugins.rs`
//! for when the app's hooks run.

mod commands;
mod json;
mod plugins;

use std::sync::Arc;

use nsh_core::{
    JsonValue,
    ui::{Arg, JsHost, JsIn, JsOwned, PropError, Registry, Ret, TsTypes, prelude::*},
};

pub(crate) use self::{
    commands::{RichCommand, RichCommandName, RichFormat},
    plugins::{RichEditorPlugin, RichPluginSpec},
};
use self::{
    commands::{doc_of, fragment_of, resolve},
    json::{
        AttrsJson, Content, NodeJson, SchemaSpecJson, SelectionJson, StateJson, TransactionJson,
    },
    plugins::{
        CallGuard, HookRef, KeyBindings, PluginEntry, RichDecoration, RichInputRule,
        RichInputRuleKind, RichPluginKind, decorations_json, in_js_call, resolve_plugins,
        transaction_from_json, transaction_json,
    },
};
use super::{
    extensions::{history, starter},
    formats::{DocFormat, MarkdownFormat, MarkdownNames, PlainTextFormat},
    model::{Node, Schema, SchemaSpec},
    state::{EditorState, EditorStateConfig, PluginKey, Selection, Transaction, TransactionOrigin},
    view::{EditorSession, NodeViews, SessionJs, SubscriptionId, set_session_js},
};

/// Registers the classes and functions of `nsh/base`'s rich text editor,
/// and how its sessions cross to the `RichEditor` view's `editor` prop.
pub(crate) fn register(registry: &mut Registry) {
    set_session_js(SessionJs::new(session_from_js, session_to_js));
    registry
        .class::<RichSchema>()
        .class::<RichEditorSession>()
        .class::<RichEditorPlugin>()
        .module::<RichEditorApi>();
}

fn session_from_js(value: JsIn<'_>) -> Option<EditorSession> {
    let instance = <Instance<RichEditorSession> as Arg>::from_js(value).ok()?;
    instance.with(|editor| Some(editor.session.clone()))
}

fn session_to_js<'a>(
    session: &EditorSession,
    host: &'a dyn JsHost,
) -> Result<JsOwned<'a>, PropError> {
    let schema = session.state().schema().clone();
    Ret::into_js(
        Instance::new(RichEditorSession::over(session.clone(), schema)),
        host,
    )
}

// --- Schemas ---------------------------------------------------------------

/// A document schema: the node and mark types a rich text editor's
/// documents are made of, and the formats documents read from and write
/// to with it.
#[class]
pub(crate) struct RichSchema {
    schema: Schema,
}

#[class]
impl RichSchema {
    /// The names of its node types, in order (the first block type is the
    /// default one).
    #[getter]
    pub(crate) fn nodes(&self) -> Vec<String> {
        self.schema
            .node_types()
            .iter()
            .map(|t| t.name().to_owned())
            .collect()
    }

    /// The names of its mark types, in order.
    #[getter]
    pub(crate) fn marks(&self) -> Vec<String> {
        self.schema
            .mark_types()
            .iter()
            .map(|t| t.name().to_owned())
            .collect()
    }

    /// The top node type's name.
    #[getter]
    pub(crate) fn top_node(&self) -> String {
        self.schema.top_node_type().name().to_owned()
    }

    /// `content` (a string in `format`, plain text by default, or JSON)
    /// read into a document of the schema; throws when it does not fit.
    pub(crate) fn parse(
        &self,
        content: Content,
        format: Option<RichFormat>,
    ) -> Result<NodeJson, String> {
        doc_of(&self.schema, &content, format).map(|doc| NodeJson(doc.to_json()))
    }

    /// Markdown `markdown` read into a document of the schema (the starter
    /// schema's names).
    pub(crate) fn parse_markdown(&self, markdown: String) -> Result<NodeJson, String> {
        self.parse(Content::Text(markdown), Some(RichFormat::Markdown))
    }

    /// `doc` (a document of the schema) written as `format` (Markdown by
    /// default, `json` as a JSON string, `text` as plain text).
    pub(crate) fn serialize(
        &self,
        doc: NodeJson,
        format: Option<RichFormat>,
    ) -> Result<String, String> {
        let doc = Node::from_json(&self.schema, &doc.0).map_err(|e| e.to_string())?;
        serialize(&doc, format.unwrap_or(RichFormat::Markdown))
    }

    /// Checks `doc` against the schema: the document, normalized (attribute
    /// defaults filled in); throws with what does not fit.
    pub(crate) fn check(&self, doc: NodeJson) -> Result<NodeJson, String> {
        let doc = Node::from_json(&self.schema, &doc.0).map_err(|e| e.to_string())?;
        doc.check().map_err(|e| e.to_string())?;
        Ok(NodeJson(doc.to_json()))
    }
}

fn serialize(doc: &Node, format: RichFormat) -> Result<String, String> {
    match format {
        RichFormat::Json => Ok(doc.to_json().to_string()),
        RichFormat::Markdown => MarkdownFormat::new(MarkdownNames::default())
            .serialize(doc)
            .map_err(|e| e.to_string()),
        RichFormat::Text => PlainTextFormat::default()
            .serialize(doc)
            .map_err(|e| e.to_string()),
    }
}

/// An editor's schema: a `RichSchema`, or a spec read into one. TS:
/// `RichSchema | RichSchemaSpec`.
#[derive(Debug, Clone)]
pub(crate) enum SchemaArg {
    Schema(Instance<RichSchema>),
    Spec(SchemaSpecJson),
}

impl PartialEq for SchemaArg {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Schema(a), Self::Schema(b)) => a.ptr_eq(b),
            (Self::Spec(a), Self::Spec(b)) => a == b,
            _ => false,
        }
    }
}

impl JsValue for SchemaArg {
    fn ts_type(types: &mut TsTypes) -> String {
        format!(
            "{} | {}",
            <Instance<RichSchema> as Arg>::ts_type(types),
            <SchemaSpecJson as JsValue>::ts_type(types)
        )
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if value.host().instance(value.raw()).is_some() {
            return <Instance<RichSchema> as Arg>::from_js(value).map(Self::Schema);
        }
        <SchemaSpecJson as JsValue>::from_js(value).map(Self::Spec)
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Self::Schema(schema) => Ret::into_js(schema.clone(), host),
            Self::Spec(spec) => spec.to_js(host),
        }
    }
}

impl SchemaArg {
    fn schema(&self) -> Result<Schema, String> {
        match self {
            Self::Schema(schema) => Ok(schema.with(|s| s.schema.clone())),
            Self::Spec(spec) => SchemaSpec::from_json(&spec.0).map_err(|e| e.to_string()),
        }
    }
}

// --- Editors ----------------------------------------------------------------

/// What `createRichEditor` makes an editor with.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct RichEditorOptions {
    /// The document schema (`defineSchema(...)`, or a spec); the starter
    /// schema when left out.
    pub(crate) schema: Option<SchemaArg>,
    /// The first document, as JSON (`schema.parse(...)`'s result).
    pub(crate) doc: Option<NodeJson>,
    /// The first content: a string in `format`, or JSON. Ignored when
    /// `doc` is given.
    pub(crate) content: Option<Content>,
    /// The format of `content` when it is a string (plain text by default).
    pub(crate) format: Option<RichFormat>,
    /// The first selection (the start of the document by default).
    pub(crate) selection: Option<SelectionJson>,
    /// The plugins, in order (their order is their priority); the starter
    /// kit (`starterKit()`) when left out, none when empty.
    pub(crate) plugins: Option<Vec<PluginEntry>>,
}

/// What `history` keeps.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct RichHistoryOptions {
    /// How many changes it keeps (100 by default).
    pub(crate) depth: Option<usize>,
    /// Milliseconds within which changes group into one undo step (500 by
    /// default).
    pub(crate) new_group_delay: Option<f64>,
}

/// What `markdown` does; every group is on unless turned off.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct RichMarkdownOptions {
    /// Block shortcuts typed at a textblock's start: headings, quotes,
    /// lists, code fences, rules.
    pub(crate) block_rules: Option<bool>,
    /// Inline marks completed by their closing delimiter.
    pub(crate) inline_rules: Option<bool>,
    /// Pasted plain text that looks like Markdown is read as Markdown.
    pub(crate) paste: Option<bool>,
    /// Copies put Markdown on the clipboard as their plain text.
    pub(crate) copy_as_markdown: Option<bool>,
}

/// One live rich text editor that a `RichEditor` view and the app's code
/// share (`createRichEditor` makes one): it owns the state in Rust, runs
/// commands, takes transactions, and tells subscribers about changes.
#[class]
pub(crate) struct RichEditorSession {
    session: EditorSession,
    schema: Schema,
    /// The app's `dispatchTransaction` and the subscribers' functions.
    hooks: Arc<HookRef>,
    /// Transactions handed to `dispatchTransaction`, by id, until applied.
    pending: Arc<std::sync::Mutex<Vec<(u64, Transaction)>>>,
    subscriptions: Vec<(u64, SubscriptionId)>,
}

impl Drop for RichEditorSession {
    fn drop(&mut self) {
        for (_, id) in self.subscriptions.drain(..) {
            self.session.unsubscribe(id);
        }
    }
}

/// Transactions waiting for the app's `applyTransaction` are kept up to
/// this many (older ones are dropped: the app never applied them).
const MAX_PENDING: usize = 32;

static NEXT_TR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl RichEditorSession {
    fn over(session: EditorSession, schema: Schema) -> Self {
        Self {
            session,
            schema,
            hooks: HookRef::new(),
            pending: Arc::default(),
            subscriptions: Vec::new(),
        }
    }

    fn command(&self, command: &RichCommand) -> Result<super::commands::BoxCommand, String> {
        resolve(command, &self.schema)
    }

    fn handles(this: &Instance<Self>) -> (EditorSession, Schema) {
        this.with(|e| (e.session.clone(), e.schema.clone()))
    }

    /// A transaction from its JSON form: the one waiting under its `id`
    /// when there is one (with its Rust metadata: history), else built from
    /// its steps against the current state.
    fn take_transaction(&self, json: &TransactionJson) -> Result<Transaction, String> {
        if let Some(id) = json.0.get("id").and_then(JsonValue::as_u64)
            && let Ok(mut pending) = self.pending.lock()
            && let Some(i) = pending.iter().position(|(p, _)| *p == id)
        {
            return Ok(pending.remove(i).1);
        }
        transaction_from_json(&self.session.state(), &json.0)
    }
}

#[class]
impl RichEditorSession {
    /// The state as JSON: the document, the selection, the stored marks and
    /// the plugins' states that have a JSON form. Made when read: read it
    /// when needed rather than on every change.
    #[getter]
    pub(crate) fn state(&self) -> StateJson {
        StateJson(self.session.state().to_json())
    }

    /// The document as JSON.
    #[getter]
    pub(crate) fn doc(&self) -> NodeJson {
        NodeJson(self.session.state().doc().to_json())
    }

    /// The selection as JSON.
    #[getter]
    pub(crate) fn selection(&self) -> SelectionJson {
        SelectionJson(self.session.state().selection().to_json())
    }

    /// How many transactions the state has applied: changes with every
    /// change.
    #[getter]
    pub(crate) fn revision(&self) -> f64 {
        self.session.state().revision() as f64
    }

    /// Whether the document is empty: one empty textblock or less.
    #[getter]
    pub(crate) fn is_empty(&self) -> bool {
        let state = self.session.state();
        let doc = state.doc();
        doc.content().size() == 0
            || (doc.child_count() == 1
                && doc.child(0).is_textblock()
                && doc.child(0).content().size() == 0)
    }

    /// How many changes `undo` can undo (with the `history` plugin).
    #[getter]
    pub(crate) fn undo_depth(&self) -> usize {
        history::undo_depth(&self.session.state())
    }

    /// How many undone changes `redo` can redo.
    #[getter]
    pub(crate) fn redo_depth(&self) -> usize {
        history::redo_depth(&self.session.state())
    }

    /// Whether an input method is composing (its text is not in the
    /// document until it commits).
    #[getter]
    pub(crate) fn composing(&self) -> bool {
        self.session.composing()
    }

    /// Runs `command` (`toggleMark("bold")`, `chain([...])`): whether it
    /// applied. Its transaction goes through `dispatchTransaction` when one
    /// is set.
    pub(crate) fn run(this: Instance<Self>, command: RichCommand) -> Result<bool, String> {
        let command = this.with(|e| e.command(&command))?;
        let (session, _) = Self::handles(&this);
        let _guard = CallGuard::enter();
        Ok(session.run(&command))
    }

    /// Whether `command` applies now (it changes nothing): what a toolbar
    /// enables its buttons by.
    pub(crate) fn can(&self, command: RichCommand) -> Result<bool, String> {
        let command = self.command(&command)?;
        Ok(self.session.can(&command))
    }

    /// Whether the mark or node type `name` is active at the selection
    /// (with `attrs`, for nodes: `isActive("heading", { level: 2 })`).
    pub(crate) fn is_active(&self, name: String, attrs: Option<AttrsJson>) -> Result<bool, String> {
        let attrs = commands::attrs_of(&attrs)?;
        Ok(self.session.is_active(&name, attrs))
    }

    /// Dispatches a transaction (steps against the current document, as
    /// `RichTransactionJSON`): through `dispatchTransaction` when one is
    /// set, else applied with the plugins' filters and appended
    /// transactions. Throws when a step does not apply.
    pub(crate) fn dispatch(this: Instance<Self>, tr: TransactionJson) -> Result<(), String> {
        let tr = this.with(|e| e.take_transaction(&tr))?;
        let (session, _) = Self::handles(&this);
        let _guard = CallGuard::enter();
        session.dispatch(tr).map_err(|e| e.to_string())
    }

    /// Applies `tr` to the current state and makes the result the state,
    /// bypassing `dispatchTransaction` (which calls it to commit, as
    /// ProseMirror's `view.updateState(view.state.apply(tr))`). A
    /// transaction `dispatchTransaction` got applies as it was made.
    pub(crate) fn apply_transaction(
        this: Instance<Self>,
        tr: TransactionJson,
    ) -> Result<(), String> {
        let tr = this.with(|e| e.take_transaction(&tr))?;
        let (session, _) = Self::handles(&this);
        let _guard = CallGuard::enter();
        let (state, applied) = session
            .state()
            .apply_transaction(tr)
            .map_err(|e| e.to_string())?;
        if !applied.is_empty() {
            session.update_state(state);
        }
        Ok(())
    }

    /// Takes over committing transactions: `handler` gets each one (the
    /// editor's, the view's and the app's) and commits it with
    /// `applyTransaction`, or drops it; `null` hands committing back.
    /// Transactions from typing reach it in JS's next turn, so the
    /// document changes when it commits them.
    #[setter]
    pub(crate) fn set_dispatch_transaction(&mut self, handler: Option<Callback<TransactionJson>>) {
        let has = handler.is_some();
        self.hooks.update(|h| h.dispatch = handler);
        if !has {
            self.session.set_dispatch_transaction(None);
            return;
        }
        let hooks = self.hooks.clone();
        let pending = self.pending.clone();
        self.session.set_dispatch_transaction(Some(Arc::new(
            move |session: &EditorSession, tr: Transaction| {
                let Some(handler) = hooks.get().and_then(|h| h.dispatch) else {
                    // Another thread: commit as the session would.
                    if let Ok((state, applied)) = session.state().apply_transaction(tr)
                        && !applied.is_empty()
                    {
                        session.update_state(state);
                    }
                    return;
                };
                let id = NEXT_TR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let json = transaction_json(&tr, Some(id));
                if let Ok(mut pending) = pending.lock() {
                    pending.push((id, tr));
                    let excess = pending.len().saturating_sub(MAX_PENDING);
                    pending.drain(..excess);
                }
                if in_js_call() {
                    if let Err(error) = handler.call(json) {
                        log::warn!("RichEditor: dispatchTransaction threw: {error}");
                    }
                } else {
                    handler.post(json);
                }
            },
        )));
    }

    /// Replaces the document with `content` (a string in `format`, plain
    /// text by default, or JSON) as one undoable change.
    pub(crate) fn set_content(
        this: Instance<Self>,
        content: Content,
        format: Option<RichFormat>,
    ) -> Result<(), String> {
        let (session, schema) = Self::handles(&this);
        let doc = doc_of(&schema, &content, format)?;
        let state = session.state();
        let mut tr = state.tr();
        tr.replace_with(0, state.doc().content().size(), doc.content().clone())
            .map_err(|e| e.to_string())?;
        tr.set_selection(Selection::at_start(tr.doc()));
        tr.set_origin(TransactionOrigin::Api);
        let _guard = CallGuard::enter();
        session.dispatch(tr).map_err(|e| e.to_string())
    }

    /// Inserts `content` (a string in `format`, plain text by default, or
    /// JSON) over the selection.
    pub(crate) fn insert_content(
        this: Instance<Self>,
        content: Content,
        format: Option<RichFormat>,
    ) -> Result<bool, String> {
        let (_, schema) = Self::handles(&this);
        // Checked here, so content that does not fit throws.
        fragment_of(&schema, &content, format)?;
        Self::run(
            this,
            RichCommand {
                command: RichCommandName::InsertContent,
                content: Some(content),
                format,
                ..RichCommand::default()
            },
        )
    }

    /// The document as JSON (ProseMirror's shape).
    pub(crate) fn get_json(&self) -> NodeJson {
        self.doc()
    }

    /// The document as Markdown (the starter schema's names).
    pub(crate) fn get_markdown(&self) -> Result<String, String> {
        serialize(self.session.state().doc(), RichFormat::Markdown)
    }

    /// The document as plain text: one line per textblock.
    pub(crate) fn get_text(&self) -> Result<String, String> {
        serialize(self.session.state().doc(), RichFormat::Text)
    }

    /// The text between positions `from` and `to` (the whole document
    /// when left out), blocks separated by `blockSeparator` (a line break
    /// by default).
    pub(crate) fn text_between(
        &self,
        from: Option<usize>,
        to: Option<usize>,
        block_separator: Option<String>,
    ) -> String {
        let state = self.session.state();
        let size = state.doc().content().size();
        let to = to.unwrap_or(size).min(size);
        let from = from.unwrap_or(0).min(to);
        state.doc().text_between(
            from,
            to,
            Some(block_separator.as_deref().unwrap_or("\n")),
            None,
        )
    }

    /// Gives a `RichEditorPlugin`'s decorations (keyed by its `key`):
    /// highlights, placeholders, widgets. They map through every change
    /// natively until set again.
    pub(crate) fn set_decorations(
        this: Instance<Self>,
        key: String,
        decorations: Vec<RichDecoration>,
    ) -> Result<(), String> {
        let (session, _) = Self::handles(&this);
        let meta = decorations_json(&decorations)?;
        let state = session.state();
        if PluginKey::new(&key).get(&state).is_none() {
            return Err(format!("the editor has no plugin with the key `{key}`"));
        }
        let mut tr = state.tr();
        tr.set_plugin_meta(&PluginKey::new(&key), meta);
        tr.set_meta(super::state::ADD_TO_HISTORY, JsonValue::Bool(false));
        tr.set_origin(TransactionOrigin::Plugin(key));
        let _guard = CallGuard::enter();
        session.dispatch(tr).map_err(|e| e.to_string())
    }

    /// Calls `listener` with the new revision after every change, from any
    /// source, in JS's next turn; returns what `unsubscribe` takes.
    pub(crate) fn subscribe(&mut self, #[args(revision)] listener: Callback<f64>) -> f64 {
        let hooks = HookRef::new();
        hooks.update(|h| h.update = Some(listener));
        let held = hooks.clone();
        let id = self.session.subscribe(Arc::new(move |state: &EditorState| {
            if let Some(listener) = held.get().and_then(|h| h.update) {
                listener.post(state.revision() as f64);
            }
        }));
        // The session's subscriber keeps the functions; this keeps the
        // pair.
        let key = id;
        self.subscriptions.push((key, id));
        drop(hooks);
        key as f64
    }

    /// Stops calling a listener `subscribe` added.
    pub(crate) fn unsubscribe(&mut self, id: f64) {
        let id = id as u64;
        if let Some(i) = self.subscriptions.iter().position(|(k, _)| *k == id) {
            let (_, subscription) = self.subscriptions.remove(i);
            self.session.unsubscribe(subscription);
        }
    }

    /// Asks the views showing the editor to take the keyboard focus.
    pub(crate) fn focus(&self) {
        self.session.focus();
    }

    /// Asks the views showing the editor to give the focus up.
    pub(crate) fn blur(&self) {
        self.session.blur();
    }
}

// --- The functions -----------------------------------------------------------

/// The rich text editor's functions.
#[module]
pub(crate) struct RichEditorApi;

#[module(flat)]
impl RichEditorApi {
    /// A schema from its spec (ProseMirror's shape); throws with what is
    /// wrong in it.
    pub(crate) fn define_schema(spec: SchemaSpecJson) -> Result<RichSchema, String> {
        Ok(RichSchema {
            schema: SchemaSpec::from_json(&spec.0).map_err(|e| e.to_string())?,
        })
    }

    /// The starter schema: paragraphs, headings, quotes, code blocks, rules,
    /// images, hard breaks, bullet and ordered lists; bold, italic, code,
    /// links, strikethrough and underline.
    pub(crate) fn starter_schema() -> RichSchema {
        RichSchema {
            schema: starter::schema(),
        }
    }

    /// A rich text editor: its state in Rust, shown by `<RichEditor
    /// editor={...} />`.
    pub(crate) fn create_rich_editor(
        options: Option<RichEditorOptions>,
    ) -> Result<RichEditorSession, String> {
        let options = options.unwrap_or_default();
        let schema = match &options.schema {
            Some(schema) => schema.schema()?,
            None => starter::schema(),
        };
        let doc = match (&options.doc, &options.content) {
            (Some(doc), _) => Some(doc_of(&schema, &Content::Json(doc.0.clone()), None)?),
            (None, Some(content)) => Some(doc_of(&schema, content, options.format)?),
            (None, None) => None,
        };
        let plugins = match &options.plugins {
            Some(entries) => resolve_plugins(entries, &schema)?,
            None => starter::plugins(&schema),
        };
        let mut config = EditorStateConfig::new(schema.clone()).plugins(plugins);
        if let Some(doc) = doc {
            config = config.doc(doc.clone());
            if let Some(selection) = &options.selection {
                let selection =
                    Selection::from_json(&doc, &selection.0).map_err(|e| e.to_string())?;
                config = config.selection(selection);
            }
        }
        let state = EditorState::create(config).map_err(|e| e.to_string())?;
        Ok(RichEditorSession::over(
            EditorSession::new(state, NodeViews::new()),
            schema,
        ))
    }

    /// Adds the mark `name` (with `attrs`) to the selection, or removes it
    /// where present; at a cursor, toggles it for the text typed next.
    pub(crate) fn toggle_mark(name: String, attrs: Option<AttrsJson>) -> RichCommand {
        RichCommand::about(RichCommandName::ToggleMark, name, attrs)
    }

    /// Adds the mark `name` (with `attrs`) to the selection.
    pub(crate) fn set_mark(name: String, attrs: Option<AttrsJson>) -> RichCommand {
        RichCommand::about(RichCommandName::SetMark, name, attrs)
    }

    /// Removes the marks of type `name` from the selection.
    pub(crate) fn unset_mark(name: String) -> RichCommand {
        RichCommand::about(RichCommandName::UnsetMark, name, None)
    }

    /// Makes the selected textblocks nodes of type `name` (with `attrs`):
    /// `setBlockType("heading", { level: 2 })`.
    pub(crate) fn set_block_type(name: String, attrs: Option<AttrsJson>) -> RichCommand {
        RichCommand::about(RichCommandName::SetBlockType, name, attrs)
    }

    /// Wraps the selected blocks in a node of type `name`
    /// (`wrapIn("blockquote")`).
    pub(crate) fn wrap_in(name: String, attrs: Option<AttrsJson>) -> RichCommand {
        RichCommand::about(RichCommandName::WrapIn, name, attrs)
    }

    /// Wraps the selected blocks in a list of type `name`
    /// (`wrapInList("bullet_list")`).
    pub(crate) fn wrap_in_list(name: String, attrs: Option<AttrsJson>) -> RichCommand {
        RichCommand::about(RichCommandName::WrapInList, name, attrs)
    }

    /// Lifts the selected list items (of type `name`) out of their list.
    pub(crate) fn lift_list_item(name: String) -> RichCommand {
        RichCommand::about(RichCommandName::LiftListItem, name, None)
    }

    /// Moves the selected list items (of type `name`) into a list inside
    /// the item before them.
    pub(crate) fn sink_list_item(name: String) -> RichCommand {
        RichCommand::about(RichCommandName::SinkListItem, name, None)
    }

    /// Splits the list item (of type `name`) at the selection.
    pub(crate) fn split_list_item(name: String, attrs: Option<AttrsJson>) -> RichCommand {
        RichCommand::about(RichCommandName::SplitListItem, name, attrs)
    }

    /// Lifts the selected blocks out of their parent.
    pub(crate) fn lift() -> RichCommand {
        RichCommand::plain(RichCommandName::Lift)
    }

    /// Undoes the last change (needs the `history` plugin).
    pub(crate) fn undo() -> RichCommand {
        RichCommand::plain(RichCommandName::Undo)
    }

    /// Redoes the last undone change.
    pub(crate) fn redo() -> RichCommand {
        RichCommand::plain(RichCommandName::Redo)
    }

    /// Selects the whole document.
    pub(crate) fn select_all() -> RichCommand {
        RichCommand::plain(RichCommandName::SelectAll)
    }

    /// Types `text` over the selection (input rules do not run).
    pub(crate) fn insert_text(text: String) -> RichCommand {
        RichCommand {
            command: RichCommandName::InsertText,
            text: Some(text),
            ..RichCommand::default()
        }
    }

    /// Inserts `content` (a string in `format`, plain text by default, or
    /// JSON) over the selection.
    pub(crate) fn insert_content(content: Content, format: Option<RichFormat>) -> RichCommand {
        RichCommand {
            command: RichCommandName::InsertContent,
            content: Some(content),
            format,
            ..RichCommand::default()
        }
    }

    /// Sets attribute `attr` of the node at position `pos` to `value` (an
    /// undoable change: a resized image, a checked task).
    pub(crate) fn set_node_attribute(pos: usize, attr: String, value: JsonValue) -> RichCommand {
        RichCommand {
            command: RichCommandName::SetNodeAttribute,
            pos: Some(pos),
            attr: Some(attr),
            value: Some(value),
            ..RichCommand::default()
        }
    }

    /// Selects text from `anchor` to `head` (a cursor at `anchor` without
    /// one).
    pub(crate) fn set_text_selection(anchor: usize, head: Option<usize>) -> RichCommand {
        RichCommand {
            command: RichCommandName::SetTextSelection,
            pos: Some(anchor),
            head,
            ..RichCommand::default()
        }
    }

    /// Selects the node at position `pos`.
    pub(crate) fn set_node_selection(pos: usize) -> RichCommand {
        RichCommand {
            command: RichCommandName::SetNodeSelection,
            pos: Some(pos),
            ..RichCommand::default()
        }
    }

    /// A command without parameters by name: `command("splitBlock")`,
    /// `command("joinBackward")`, `command("exitCode")`, ...
    pub(crate) fn command(name: RichCommandName) -> RichCommand {
        RichCommand::plain(name)
    }

    /// Runs `commands` one after another as one transaction, on the state
    /// each leaves; applies only when every one does (one undo step).
    pub(crate) fn chain(commands: Vec<RichCommand>) -> RichCommand {
        RichCommand {
            command: RichCommandName::Chain,
            commands: Some(commands),
            ..RichCommand::default()
        }
    }

    /// Runs the first of `commands` that applies (ProseMirror's
    /// `chainCommands`: Enter's list item split, else a block split).
    pub(crate) fn first(commands: Vec<RichCommand>) -> RichCommand {
        RichCommand {
            command: RichCommandName::First,
            commands: Some(commands),
            ..RichCommand::default()
        }
    }

    /// Undo history: `depth` changes (100 by default); changes within
    /// `newGroupDelay` milliseconds (500) undo together.
    pub(crate) fn history(options: Option<RichHistoryOptions>) -> RichPluginSpec {
        let options = options.unwrap_or_default();
        RichPluginSpec {
            plugin: RichPluginKind::History,
            depth: options.depth,
            new_group_delay: options.new_group_delay,
            ..RichPluginSpec::default()
        }
    }

    /// Key bindings: commands by key name (`"Mod-b"`: Cmd-B on Apple
    /// platforms, Ctrl-B elsewhere; `"Shift-Enter"`, `"Alt-ArrowUp"`).
    pub(crate) fn keymap(bindings: KeyBindings) -> RichPluginSpec {
        RichPluginSpec {
            plugin: RichPluginKind::Keymap,
            bindings: Some(bindings),
            ..RichPluginSpec::default()
        }
    }

    /// The base key bindings: Enter, Backspace, Delete, select all and the
    /// platform's text editing keys.
    pub(crate) fn base_keymap() -> RichPluginSpec {
        RichPluginSpec {
            plugin: RichPluginKind::BaseKeymap,
            ..RichPluginSpec::default()
        }
    }

    /// The starter schema's key bindings: marks (`Mod-b`, `Mod-i`, ...),
    /// headings, lists, code blocks, hard breaks.
    pub(crate) fn starter_keymap() -> RichPluginSpec {
        RichPluginSpec {
            plugin: RichPluginKind::StarterKeymap,
            ..RichPluginSpec::default()
        }
    }

    /// Input rules: text typed before the cursor that matches a rule turns
    /// into what the rule makes. Backspace right after one undoes it.
    pub(crate) fn input_rules(rules: Vec<RichInputRule>) -> RichPluginSpec {
        RichPluginSpec {
            plugin: RichPluginKind::InputRules,
            rules: Some(rules),
            ..RichPluginSpec::default()
        }
    }

    /// Markdown as you type (block shortcuts, inline marks), Markdown paste
    /// and Markdown copy; each group on unless `options` turn it off.
    pub(crate) fn markdown(options: Option<RichMarkdownOptions>) -> RichPluginSpec {
        let options = options.unwrap_or_default();
        RichPluginSpec {
            plugin: RichPluginKind::Markdown,
            block_rules: options.block_rules,
            inline_rules: options.inline_rules,
            paste: options.paste,
            copy_as_markdown: options.copy_as_markdown,
            ..RichPluginSpec::default()
        }
    }

    /// The starter kit: the starter schema's input rules and key bindings,
    /// the base key bindings and the history.
    pub(crate) fn starter_kit() -> RichPluginSpec {
        RichPluginSpec {
            plugin: RichPluginKind::StarterKit,
            ..RichPluginSpec::default()
        }
    }

    /// A rule replacing the text that matches `pattern` with
    /// `replacement` (`textRule("->$", "→")`).
    pub(crate) fn text_rule(pattern: String, replacement: String) -> RichInputRule {
        RichInputRule {
            rule: RichInputRuleKind::Text,
            pattern: Some(pattern),
            replacement: Some(replacement),
            ..RichInputRule::default()
        }
    }

    /// A rule giving the text between delimiters the mark `name`
    /// (`markRule("\\*\\*([^*]+)\\*\\*$", "bold")`; the last group is the
    /// text).
    pub(crate) fn mark_rule(
        pattern: String,
        name: String,
        attrs: Option<AttrsJson>,
    ) -> RichInputRule {
        RichInputRule {
            rule: RichInputRuleKind::Mark,
            pattern: Some(pattern),
            name: Some(name),
            attrs,
            ..RichInputRule::default()
        }
    }

    /// A rule wrapping the textblock in a node of type `name` when its
    /// start matches (`wrappingRule("^\\s*>\\s$", "blockquote")`).
    pub(crate) fn wrapping_rule(
        pattern: String,
        name: String,
        attrs: Option<AttrsJson>,
    ) -> RichInputRule {
        RichInputRule {
            rule: RichInputRuleKind::Wrapping,
            pattern: Some(pattern),
            name: Some(name),
            attrs,
            ..RichInputRule::default()
        }
    }

    /// A rule making the textblock a node of type `name` when its start
    /// matches (`textblockRule("^```$", "code_block")`).
    pub(crate) fn textblock_rule(
        pattern: String,
        name: String,
        attrs: Option<AttrsJson>,
    ) -> RichInputRule {
        RichInputRule {
            rule: RichInputRuleKind::Textblock,
            pattern: Some(pattern),
            name: Some(name),
            attrs,
            ..RichInputRule::default()
        }
    }

    /// Typographic rules: smart quotes, `...` to an ellipsis, `--` to an em
    /// dash.
    pub(crate) fn typography_rules() -> Vec<RichInputRule> {
        [
            RichInputRuleKind::SmartQuotes,
            RichInputRuleKind::Ellipsis,
            RichInputRuleKind::EmDash,
        ]
        .into_iter()
        .map(|rule| RichInputRule {
            rule,
            ..RichInputRule::default()
        })
        .collect()
    }

    /// The starter schema's Markdown-like rules: `# ` headings, `> `
    /// quotes, `- ` and `1. ` lists, three backticks for code, `---`
    /// rules, `**bold**`, `*italic*`, `` `code` ``, `~~strike~~`.
    pub(crate) fn markdown_rules() -> Vec<RichInputRule> {
        vec![RichInputRule {
            rule: RichInputRuleKind::Starter,
            ..RichInputRule::default()
        }]
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        commands::{RichCommand, RichCommandName, doc_of, resolve},
        json::Content,
        plugins::{KeyBindings, RichPluginKind, RichPluginSpec, plugins_of},
        *,
    };

    fn editor(markdown: &str) -> EditorSession {
        let schema = starter::schema();
        let doc = doc_of(
            &schema,
            &Content::Text(markdown.into()),
            Some(RichFormat::Markdown),
        )
        .unwrap();
        let state = EditorState::create(
            EditorStateConfig::new(schema.clone())
                .doc(doc)
                .plugins(starter::plugins(&schema)),
        )
        .unwrap();
        EditorSession::new(state, NodeViews::new())
    }

    fn run(session: &EditorSession, command: RichCommand) -> bool {
        let schema = session.state().schema().clone();
        session.run(&resolve(&command, &schema).unwrap())
    }

    #[test]
    fn commands_resolve_and_run() {
        let session = editor("Hello world");
        let select = RichCommand {
            command: RichCommandName::SetTextSelection,
            pos: Some(1),
            head: Some(6),
            ..RichCommand::default()
        };
        assert!(run(&session, select));
        let bold = RichCommand::about(RichCommandName::ToggleMark, "bold".into(), None);
        assert!(run(&session, bold));
        assert!(session.is_active("bold", None));
        assert!(run(&session, RichCommand::plain(RichCommandName::Undo)));
        assert!(!session.is_active("bold", None));
    }

    #[test]
    fn unknown_types_are_errors() {
        let schema = starter::schema();
        let command = RichCommand::about(RichCommandName::ToggleMark, "nope".into(), None);
        assert!(resolve(&command, &schema).is_err());
    }

    #[test]
    fn a_chain_is_one_transaction_and_all_or_nothing() {
        let session = editor("Hello");
        let revision = session.state().revision();
        let chain = RichCommand {
            command: RichCommandName::Chain,
            commands: Some(vec![
                RichCommand::plain(RichCommandName::SelectAll),
                RichCommand {
                    command: RichCommandName::InsertText,
                    text: Some("Replaced".into()),
                    ..RichCommand::default()
                },
            ]),
            ..RichCommand::default()
        };
        assert!(run(&session, chain));
        assert_eq!(session.state().revision(), revision + 1);
        assert_eq!(session.state().doc().text_content(), "Replaced");
        let failing = RichCommand {
            command: RichCommandName::Chain,
            commands: Some(vec![
                RichCommand::plain(RichCommandName::SelectAll),
                RichCommand::about(RichCommandName::SinkListItem, "list_item".into(), None),
            ]),
            ..RichCommand::default()
        };
        assert!(!run(&session, failing));
        assert_eq!(session.state().doc().text_content(), "Replaced");
    }

    #[test]
    fn transactions_round_trip_as_json() {
        let session = editor("ab");
        let state = session.state();
        let mut tr = state.tr();
        tr.insert_text("x", Some(2), Some(2));
        tr.set_meta("custom", json!(1));
        let json = transaction_json(&tr, None);
        assert_eq!(json.0["steps"].as_array().unwrap().len(), 1);
        assert_eq!(json.0["meta"]["custom"], json!(1));
        let back = transaction_from_json(&state, &json.0).unwrap();
        assert_eq!(state.apply(back).unwrap().doc().text_content(), "axb");
    }

    #[test]
    fn plugin_specs_resolve() {
        let schema = starter::schema();
        let keymap = RichPluginSpec {
            plugin: RichPluginKind::Keymap,
            bindings: Some(KeyBindings(vec![(
                "Mod-b".into(),
                RichCommand::about(RichCommandName::ToggleMark, "bold".into(), None),
            )])),
            ..RichPluginSpec::default()
        };
        assert_eq!(plugins_of(&keymap, &schema).unwrap().len(), 1);
        let bad = RichPluginSpec {
            plugin: RichPluginKind::Keymap,
            bindings: Some(KeyBindings(vec![(
                "Mod-b".into(),
                RichCommand::about(RichCommandName::ToggleMark, "missing".into(), None),
            )])),
            ..RichPluginSpec::default()
        };
        assert!(plugins_of(&bad, &schema).is_err());
    }
}

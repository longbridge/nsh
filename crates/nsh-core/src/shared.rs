//! State shared by several engines: [`SharedStore`].
//!
//! One surface is one engine, each with a reactive graph of its own on its
//! own thread, so signals cannot be shared between surfaces (the windows of
//! an app, or the views an embedder shows side by side). A store is what
//! they share instead: string keys to immutable [`SharedValue`]s, changed in
//! atomic [transactions](SharedStore::transaction) that each make one new
//! [version](SharedStore::version). It belongs to no engine: whoever makes
//! the surfaces (an app runner, an embedder) makes one and hands it to each
//! of them, and any thread may write to it (a data source's thread, any of
//! the engines).
//!
//! An engine reads it through a [`Subscriber`]: the keys and key prefixes it
//! [watches](Subscriber::watch), and a wake function the store calls when a
//! transaction changes one of them. A subscriber is woken at most once until
//! it [takes its changes](Subscriber::take_changes), and what it takes is the
//! latest value of each key that changed since, all read at one version:
//!
//! - **Latest values, no queue.** A subscriber that takes its changes less
//!   often than they are made (a busy or stalled engine) skips the versions
//!   in between and gets the values of the latest; what it holds pending is
//!   at most one mark per watched key. Writers never wait for readers.
//! - **Consistent.** The changes taken at once are those of one version: two
//!   keys a transaction changed together are seen together, never one new
//!   and the other old.
//! - **No pacing of its own.** How often values change is the writers' (one
//!   transaction per batch of a data source); how often an engine applies
//!   them is its own (once per turn).
//!
//! Besides state, [events](SharedStore::emit) are one-off messages delivered
//! in order to the subscribers [listening](Subscriber::listen) to their topic
//! (not merged; a subscriber keeps at most [`MAX_EVENTS`] unread, dropping
//! the oldest).
//!
//! The store is a mutex around plain maps: a transaction holds it for the
//! keys it changes and the subscribers they match, a subscriber for the keys
//! it takes. Wake functions run after the lock is released.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    fmt,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard},
    task::{Poll, Waker},
};

use rustc_hash::{FxHashMap, FxHashSet};
use slab::Slab;

use crate::element::{
    Owner, ReadSignal, Signal, SignalEquals, batch, cleanup, create_root, signal_with, spawn,
};

/// The most events a subscriber keeps unread on a topic it listens to: the
/// oldest are dropped past it (with a warning), as events are meant for
/// what happens rarely; data that changes often is state.
pub const MAX_EVENTS: usize = 1024;

/// An immutable value in a [`SharedStore`]: what JSON holds, bytes, and
/// opaque Rust values. Cloning one is cheap (its parts are shared).
#[derive(Clone, Default)]
#[non_exhaustive]
pub enum SharedValue {
    /// No value (`null` in JS).
    #[default]
    Null,
    /// A boolean.
    Bool(bool),
    /// A number.
    Number(f64),
    /// A string.
    String(Arc<str>),
    /// Bytes (an `ArrayBuffer`'s in JS).
    Bytes(Arc<[u8]>),
    /// A list.
    List(Arc<[SharedValue]>),
    /// Named values, in their order.
    Map(Arc<[(Arc<str>, SharedValue)]>),
    /// A Rust value, for Rust code only: JS reads it as `undefined`. It
    /// lets Rust share its own types without converting them.
    Opaque(Arc<dyn Any + Send + Sync>),
}

impl SharedValue {
    /// An opaque Rust value ([`SharedValue::Opaque`]).
    pub fn opaque<T: Any + Send + Sync>(value: T) -> Self {
        SharedValue::Opaque(Arc::new(value))
    }

    /// A map of `entries`, in their order.
    pub fn map<K: Into<Arc<str>>>(entries: impl IntoIterator<Item = (K, SharedValue)>) -> Self {
        SharedValue::Map(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Its boolean, if it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SharedValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Its number, if it is one.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            SharedValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Its string, if it is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            SharedValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Its items, if it is a list.
    pub fn as_list(&self) -> Option<&[SharedValue]> {
        match self {
            SharedValue::List(items) => Some(items),
            _ => None,
        }
    }

    /// The value named `key`, if it is a map that has one.
    pub fn get(&self, key: &str) -> Option<&SharedValue> {
        match self {
            SharedValue::Map(entries) => entries.iter().find(|(k, _)| &**k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Its Rust value, if it is an opaque `T`.
    pub fn downcast<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        match self {
            SharedValue::Opaque(any) => any.clone().downcast::<T>().ok(),
            _ => None,
        }
    }

    /// Whether `self` and `other` are known to be the same value: equal
    /// plain values, or the same shared parts (opaque values and lists or
    /// maps are compared by identity, not deeply).
    pub fn same(&self, other: &SharedValue) -> bool {
        match (self, other) {
            (SharedValue::Null, SharedValue::Null) => true,
            (SharedValue::Bool(a), SharedValue::Bool(b)) => a == b,
            (SharedValue::Number(a), SharedValue::Number(b)) => a.to_bits() == b.to_bits(),
            (SharedValue::String(a), SharedValue::String(b)) => a == b,
            (SharedValue::Bytes(a), SharedValue::Bytes(b)) => Arc::ptr_eq(a, b) || a == b,
            (SharedValue::List(a), SharedValue::List(b)) => Arc::ptr_eq(a, b),
            (SharedValue::Map(a), SharedValue::Map(b)) => Arc::ptr_eq(a, b),
            (SharedValue::Opaque(a), SharedValue::Opaque(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Debug for SharedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharedValue::Null => f.write_str("null"),
            SharedValue::Bool(b) => write!(f, "{b}"),
            SharedValue::Number(n) => write!(f, "{n}"),
            SharedValue::String(s) => write!(f, "{s:?}"),
            SharedValue::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            SharedValue::List(items) => f.debug_list().entries(items.iter()).finish(),
            SharedValue::Map(entries) => f
                .debug_map()
                .entries(entries.iter().map(|(k, v)| (k, v)))
                .finish(),
            SharedValue::Opaque(_) => f.write_str("<opaque>"),
        }
    }
}

impl From<bool> for SharedValue {
    fn from(value: bool) -> Self {
        SharedValue::Bool(value)
    }
}

impl From<f64> for SharedValue {
    fn from(value: f64) -> Self {
        SharedValue::Number(value)
    }
}

impl From<i32> for SharedValue {
    fn from(value: i32) -> Self {
        SharedValue::Number(value.into())
    }
}

impl From<u32> for SharedValue {
    fn from(value: u32) -> Self {
        SharedValue::Number(value.into())
    }
}

impl From<&str> for SharedValue {
    fn from(value: &str) -> Self {
        SharedValue::String(value.into())
    }
}

impl From<String> for SharedValue {
    fn from(value: String) -> Self {
        SharedValue::String(value.into())
    }
}

impl From<Arc<str>> for SharedValue {
    fn from(value: Arc<str>) -> Self {
        SharedValue::String(value)
    }
}

impl From<Vec<SharedValue>> for SharedValue {
    fn from(value: Vec<SharedValue>) -> Self {
        SharedValue::List(value.into())
    }
}

impl<T: Into<SharedValue>> From<Option<T>> for SharedValue {
    fn from(value: Option<T>) -> Self {
        value.map_or(SharedValue::Null, Into::into)
    }
}

impl From<&crate::JsonValue> for SharedValue {
    fn from(value: &crate::JsonValue) -> Self {
        use crate::JsonValue as J;
        match value {
            J::Null => SharedValue::Null,
            J::Bool(b) => SharedValue::Bool(*b),
            J::Number(n) => n.as_f64().map_or(SharedValue::Null, SharedValue::Number),
            J::String(s) => SharedValue::String(s.as_str().into()),
            J::Array(items) => SharedValue::List(items.iter().map(Into::into).collect()),
            J::Object(entries) => SharedValue::Map(
                entries
                    .iter()
                    .map(|(k, v)| (Arc::from(k.as_str()), v.into()))
                    .collect(),
            ),
        }
    }
}

/// The changes a [`Subscriber`] takes: the latest value of each watched key
/// that changed since it last took them (`None`: deleted), all at
/// [`version`](Changes::version).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Changes {
    /// The store's version they were read at.
    pub version: u64,
    /// The keys and their values.
    pub values: Vec<(Arc<str>, Option<SharedValue>)>,
}

/// An event a [`Subscriber`] takes: its topic and value.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SharedEvent {
    /// The topic it was emitted on.
    pub topic: Arc<str>,
    /// What it carries.
    pub value: SharedValue,
}

/// String keys to [`SharedValue`]s that several engines share, changed in
/// versioned transactions (the module docs). Cloning a store gives another
/// handle to the same one.
#[derive(Clone, Default)]
pub struct SharedStore {
    inner: Arc<Mutex<State>>,
}

impl fmt::Debug for SharedStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.lock();
        f.debug_struct("SharedStore")
            .field("version", &state.version)
            .field("keys", &state.entries.len())
            .field("subscribers", &state.subscribers.len())
            .finish()
    }
}

type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct State {
    version: u64,
    entries: FxHashMap<Arc<str>, SharedValue>,
    subscribers: Slab<Sub>,
}

struct Sub {
    keys: FxHashSet<Arc<str>>,
    prefixes: Vec<Arc<str>>,
    topics: FxHashSet<Arc<str>>,
    /// Watched keys changed since the changes were last taken.
    dirty: FxHashSet<Arc<str>>,
    events: VecDeque<SharedEvent>,
    /// Woken and not taken since: no other wake until it takes.
    woken: bool,
    wake: Wake,
}

impl Sub {
    fn watches(&self, key: &str) -> bool {
        self.keys.contains(key) || self.prefixes.iter().any(|p| key.starts_with(&**p))
    }

    /// Marks it for a wake; true when the wake is due now.
    fn arm(&mut self) -> bool {
        !std::mem::replace(&mut self.woken, true)
    }
}

impl SharedStore {
    /// A new, empty store (version 0).
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic while holding it leaves whole entries: what it held is
        // still a valid store.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The version of the last transaction (0: none yet).
    pub fn version(&self) -> u64 {
        self.lock().version
    }

    /// The value of `key` now.
    pub fn get(&self, key: &str) -> Option<SharedValue> {
        self.lock().entries.get(key).cloned()
    }

    /// Runs `f` on a transaction and commits what it did as one version
    /// (none when it changed nothing); returns the store's version after.
    /// Subscribers watching a changed key are woken once the lock is
    /// released, on this thread.
    pub fn transaction(&self, f: impl FnOnce(&mut Transaction)) -> u64 {
        let mut tx = Transaction::default();
        f(&mut tx);
        self.commit(tx)
    }

    /// Sets `key` to `value` (a transaction of one change).
    pub fn set(&self, key: impl Into<Arc<str>>, value: impl Into<SharedValue>) -> u64 {
        self.transaction(|tx| tx.set(key, value))
    }

    /// Deletes `key` (a transaction of one change).
    pub fn delete(&self, key: impl Into<Arc<str>>) -> u64 {
        self.transaction(|tx| tx.delete(key))
    }

    fn commit(&self, tx: Transaction) -> u64 {
        let mut wakes: Vec<Wake> = Vec::new();
        let version = {
            let mut guard = self.lock();
            let state = &mut *guard;
            let mut changed: Vec<Arc<str>> = Vec::with_capacity(tx.changes.len());
            for (key, value) in tx.changes {
                let same = match (&value, state.entries.get(&key)) {
                    (Some(new), Some(old)) => new.same(old),
                    (None, None) => true,
                    _ => false,
                };
                if same {
                    continue;
                }
                match value {
                    Some(value) => {
                        state.entries.insert(key.clone(), value);
                    }
                    None => {
                        state.entries.remove(&key);
                    }
                }
                changed.push(key);
            }
            if changed.is_empty() {
                return state.version;
            }
            state.version += 1;
            for (_, sub) in state.subscribers.iter_mut() {
                let mut hit = false;
                for key in &changed {
                    if sub.watches(key) {
                        sub.dirty.insert(key.clone());
                        hit = true;
                    }
                }
                if hit && sub.arm() {
                    wakes.push(sub.wake.clone());
                }
            }
            state.version
        };
        for wake in wakes {
            wake();
        }
        version
    }

    /// Sends `value` to every subscriber listening to `topic`, in the order
    /// emitted (events are not merged; see [`MAX_EVENTS`]).
    pub fn emit(&self, topic: impl Into<Arc<str>>, value: impl Into<SharedValue>) {
        let topic = topic.into();
        let value = value.into();
        let mut wakes: Vec<Wake> = Vec::new();
        {
            let mut state = self.lock();
            for (_, sub) in state.subscribers.iter_mut() {
                if !sub.topics.contains(&topic) {
                    continue;
                }
                if sub.events.len() >= MAX_EVENTS {
                    sub.events.pop_front();
                    log::warn!(
                        "shared store: a subscriber has {MAX_EVENTS} unread events; dropped the \
                         oldest (topic \"{topic}\")"
                    );
                }
                sub.events.push_back(SharedEvent {
                    topic: topic.clone(),
                    value: value.clone(),
                });
                if sub.arm() {
                    wakes.push(sub.wake.clone());
                }
            }
        }
        for wake in wakes {
            wake();
        }
    }

    /// A subscriber that `wake` tells, from the thread of the transaction or
    /// event, that it has changes to take. It watches nothing yet.
    pub fn subscribe(&self, wake: impl Fn() + Send + Sync + 'static) -> Subscriber {
        let id = self.lock().subscribers.insert(Sub {
            keys: FxHashSet::default(),
            prefixes: Vec::new(),
            topics: FxHashSet::default(),
            dirty: FxHashSet::default(),
            events: VecDeque::new(),
            woken: false,
            wake: Arc::new(wake),
        });
        Subscriber {
            store: self.clone(),
            id,
        }
    }
}

/// The changes of one [`SharedStore::transaction`]: the last one made to a
/// key wins.
#[derive(Default)]
pub struct Transaction {
    changes: Vec<(Arc<str>, Option<SharedValue>)>,
}

impl Transaction {
    fn put(&mut self, key: Arc<str>, value: Option<SharedValue>) {
        match self.changes.iter_mut().find(|(k, _)| *k == key) {
            Some(change) => change.1 = value,
            None => self.changes.push((key, value)),
        }
    }

    /// Sets `key` to `value`.
    pub fn set(&mut self, key: impl Into<Arc<str>>, value: impl Into<SharedValue>) {
        self.put(key.into(), Some(value.into()));
    }

    /// Deletes `key`.
    pub fn delete(&mut self, key: impl Into<Arc<str>>) {
        self.put(key.into(), None);
    }
}

/// One reader of a [`SharedStore`] (an engine): what it watches, and the
/// changes and events pending for it. Dropping it stops them.
pub struct Subscriber {
    store: SharedStore,
    id: usize,
}

impl fmt::Debug for Subscriber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscriber").field("id", &self.id).finish()
    }
}

impl Subscriber {
    fn with<R>(&self, f: impl FnOnce(&mut Sub, &FxHashMap<Arc<str>, SharedValue>) -> R) -> R {
        let mut guard = self.store.lock();
        let state = &mut *guard;
        f(&mut state.subscribers[self.id], &state.entries)
    }

    /// The store it reads.
    pub fn store(&self) -> &SharedStore {
        &self.store
    }

    /// Watches `key`: its changes from now on are this subscriber's.
    pub fn watch(&self, key: impl Into<Arc<str>>) {
        let key = key.into();
        self.with(|sub, _| {
            sub.keys.insert(key);
        });
    }

    /// Stops watching `key` (changes of it already pending are dropped,
    /// unless a watched prefix covers it).
    pub fn unwatch(&self, key: &str) {
        self.with(|sub, _| {
            sub.keys.remove(key);
            if !sub.watches(key) {
                sub.dirty.remove(key);
            }
        });
    }

    /// Watches every key starting with `prefix`.
    pub fn watch_prefix(&self, prefix: impl Into<Arc<str>>) {
        let prefix = prefix.into();
        self.with(|sub, _| {
            if !sub.prefixes.contains(&prefix) {
                sub.prefixes.push(prefix);
            }
        });
    }

    /// Stops watching the keys starting with `prefix`.
    pub fn unwatch_prefix(&self, prefix: &str) {
        self.with(|sub, _| {
            sub.prefixes.retain(|p| &**p != prefix);
            let Sub {
                dirty,
                keys,
                prefixes,
                ..
            } = sub;
            dirty.retain(|k| keys.contains(k) || prefixes.iter().any(|p| k.starts_with(&**p)));
        });
    }

    /// Listens to the events of `topic`.
    pub fn listen(&self, topic: impl Into<Arc<str>>) {
        let topic = topic.into();
        self.with(|sub, _| {
            sub.topics.insert(topic);
        });
    }

    /// Stops listening to `topic` (its unread events are dropped).
    pub fn unlisten(&self, topic: &str) {
        self.with(|sub, _| {
            sub.topics.remove(topic);
            sub.events.retain(|e| &*e.topic != topic);
        });
    }

    /// The current values of every key starting with `prefix`, with the
    /// store's version (what a prefix watcher starts from).
    pub fn snapshot_prefix(&self, prefix: &str) -> Changes {
        let state = self.store.lock();
        let mut values: Vec<(Arc<str>, Option<SharedValue>)> = state
            .entries
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), Some(v.clone())))
            .collect();
        values.sort_by(|a, b| a.0.cmp(&b.0));
        Changes {
            version: state.version,
            values,
        }
    }

    /// Takes the changes pending: the latest value of each watched key that
    /// changed since the last take, all at one version; its next change
    /// wakes it again.
    pub fn take_changes(&self) -> Changes {
        let mut guard = self.store.lock();
        let state = &mut *guard;
        let sub = &mut state.subscribers[self.id];
        sub.woken = false;
        let mut values: Vec<(Arc<str>, Option<SharedValue>)> = sub
            .dirty
            .drain()
            .map(|key| {
                let value = state.entries.get(&key).cloned();
                (key, value)
            })
            .collect();
        values.sort_by(|a, b| a.0.cmp(&b.0));
        Changes {
            version: state.version,
            values,
        }
    }

    /// Takes the unread events, in the order they were emitted.
    pub fn take_events(&self) -> Vec<SharedEvent> {
        self.with(|sub, _| {
            sub.woken = false;
            sub.events.drain(..).collect()
        })
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.store.lock().subscribers.remove(self.id);
    }
}

// --- On an engine's thread ------------------------------------------------

/// The store of the engine whose thread this is (the one its maker gave it,
/// or one of its own when none was given). Call it on the engine's thread,
/// where components and tasks run.
pub fn store() -> SharedStore {
    crate::element::shared_state().0
}

/// The value of `key`, as a signal that follows the store: written when a
/// transaction changes it (at the engine's next turn, with every other key
/// that changed with it), `None` while the key has no value. Watched while
/// the current owner lives.
pub fn signal(key: impl Into<Arc<str>>) -> ReadSignal<Option<SharedValue>> {
    let client = client();
    let key = key.into();
    let signal = client.watch_key(key.clone());
    cleanup(move || client.release_key(&key));
    signal
}

/// The keys starting with `prefix` and their values, as a signal that
/// follows the store (as [`signal`]). Watched while the current owner lives.
pub fn prefix(prefix: impl Into<Arc<str>>) -> ReadSignal<Arc<BTreeMap<Arc<str>, SharedValue>>> {
    let client = client();
    let prefix = prefix.into();
    let signal = client.watch_prefix(prefix.clone());
    cleanup(move || client.release_prefix(&prefix));
    signal
}

/// Runs `f` with each event emitted on `topic` ([`SharedStore::emit`]), at
/// the engine's next turn, in order, while the current owner lives.
pub fn on_event(topic: impl Into<Arc<str>>, f: impl Fn(&SharedValue) + 'static) {
    let client = client();
    let topic = topic.into();
    let id = client.listen(topic.clone(), Rc::new(f));
    cleanup(move || client.unlisten(&topic, id));
}

fn client() -> Rc<Client> {
    let (store, client) = crate::element::shared_state();
    if let Some(client) = client {
        return client;
    }
    let made = Client::new(&store);
    crate::element::set_shared_client(made.clone());
    made
}

/// A key or prefix watched by the engine: its signal, the owner the signal
/// lives under, and how many owners use it.
struct Watched<T: 'static> {
    signal: Signal<T>,
    owner: Owner,
    users: usize,
}

/// A prefix's keys and values.
type PrefixMap = Arc<BTreeMap<Arc<str>, SharedValue>>;

/// An engine's side of its store: its subscriber, the signals of what it
/// watches, and a task the subscriber wakes, which writes them.
pub(crate) struct Client {
    sub: Subscriber,
    keys: RefCell<FxHashMap<Arc<str>, Watched<Option<SharedValue>>>>,
    prefixes: RefCell<FxHashMap<Arc<str>, Watched<PrefixMap>>>,
    listeners: RefCell<FxHashMap<Arc<str>, Vec<(u64, Rc<dyn Fn(&SharedValue)>)>>>,
    next_listener: Cell<u64>,
}

impl Client {
    fn new(store: &SharedStore) -> Rc<Client> {
        // The subscriber wakes the task from writers' threads: `Waker` is
        // `Send`, and waking it queues the task for the engine's next turn.
        let waker: Arc<Mutex<Option<Waker>>> = Arc::default();
        let wake = waker.clone();
        let sub = store.subscribe(move || {
            if let Some(waker) = wake.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                waker.wake_by_ref();
            }
        });
        let client = Rc::new(Client {
            sub,
            keys: RefCell::default(),
            prefixes: RefCell::default(),
            listeners: RefCell::default(),
            next_listener: Cell::new(0),
        });
        let weak = Rc::downgrade(&client);
        // For the engine's life: under an owner of its own, never disposed.
        Owner::detached().run(|| {
            spawn(std::future::poll_fn(move |cx| {
                *waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(cx.waker().clone());
                match weak.upgrade() {
                    // Taken after the waker is in place: a change made
                    // before is taken now, one made after wakes it again.
                    Some(client) => {
                        client.apply();
                        Poll::Pending
                    }
                    None => Poll::Ready(()),
                }
            }))
        });
        client
    }

    fn watch_key(&self, key: Arc<str>) -> ReadSignal<Option<SharedValue>> {
        if let Some(w) = self.keys.borrow_mut().get_mut(&key) {
            w.users += 1;
            return w.signal.read_only();
        }
        // Watched first: a change between the two is taken as a change.
        self.sub.watch(key.clone());
        let value = self.sub.store().get(&key);
        let (owner, signal) = create_root(|| signal_with(value, SignalEquals::Never));
        self.keys.borrow_mut().insert(
            key,
            Watched {
                signal,
                owner,
                users: 1,
            },
        );
        signal.read_only()
    }

    fn release_key(&self, key: &str) {
        let mut keys = self.keys.borrow_mut();
        let Some(w) = keys.get_mut(key) else { return };
        w.users -= 1;
        if w.users == 0 {
            let w = keys.remove(key).expect("watched");
            drop(keys);
            self.sub.unwatch(key);
            w.owner.dispose();
        }
    }

    fn watch_prefix(&self, prefix: Arc<str>) -> ReadSignal<PrefixMap> {
        if let Some(w) = self.prefixes.borrow_mut().get_mut(&prefix) {
            w.users += 1;
            return w.signal.read_only();
        }
        self.sub.watch_prefix(prefix.clone());
        let now = self.sub.snapshot_prefix(&prefix);
        let map: PrefixMap = Arc::new(
            now.values
                .into_iter()
                .filter_map(|(k, v)| Some((k, v?)))
                .collect(),
        );
        let (owner, signal) = create_root(|| signal_with(map, SignalEquals::Never));
        self.prefixes.borrow_mut().insert(
            prefix,
            Watched {
                signal,
                owner,
                users: 1,
            },
        );
        signal.read_only()
    }

    fn release_prefix(&self, prefix: &str) {
        let mut prefixes = self.prefixes.borrow_mut();
        let Some(w) = prefixes.get_mut(prefix) else {
            return;
        };
        w.users -= 1;
        if w.users == 0 {
            let w = prefixes.remove(prefix).expect("watched");
            drop(prefixes);
            self.sub.unwatch_prefix(prefix);
            w.owner.dispose();
        }
    }

    fn listen(&self, topic: Arc<str>, f: Rc<dyn Fn(&SharedValue)>) -> u64 {
        let id = self.next_listener.get();
        self.next_listener.set(id + 1);
        let mut listeners = self.listeners.borrow_mut();
        let list = listeners.entry(topic.clone()).or_default();
        if list.is_empty() {
            self.sub.listen(topic);
        }
        list.push((id, f));
        id
    }

    fn unlisten(&self, topic: &str, id: u64) {
        let mut listeners = self.listeners.borrow_mut();
        let Some(list) = listeners.get_mut(topic) else {
            return;
        };
        list.retain(|(i, _)| *i != id);
        if list.is_empty() {
            listeners.remove(topic);
            self.sub.unlisten(topic);
        }
    }

    /// Writes what changed into the signals, in one batch, and runs the
    /// listeners of the events taken.
    fn apply(&self) {
        let changes = self.sub.take_changes();
        let events = self.sub.take_events();
        if changes.values.is_empty() && events.is_empty() {
            return;
        }
        // What to write, worked out before writing: writing runs what read
        // the signals, which may watch or release keys here.
        let mut keys: Vec<(Signal<Option<SharedValue>>, Option<SharedValue>)> = Vec::new();
        let mut maps: Vec<(Signal<PrefixMap>, PrefixMap)> = Vec::new();
        {
            let watched = self.keys.borrow();
            for (key, value) in &changes.values {
                if let Some(w) = watched.get(key) {
                    keys.push((w.signal, value.clone()));
                }
            }
            for (prefix, w) in self.prefixes.borrow().iter() {
                let mut map: Option<PrefixMap> = None;
                for (key, value) in &changes.values {
                    if !key.starts_with(&**prefix) {
                        continue;
                    }
                    let map = map.get_or_insert_with(|| w.signal.get_untracked());
                    let entries = Arc::make_mut(map);
                    match value {
                        Some(value) => {
                            entries.insert(key.clone(), value.clone());
                        }
                        None => {
                            entries.remove(key);
                        }
                    }
                }
                if let Some(map) = map {
                    maps.push((w.signal, map));
                }
            }
        }
        let listeners: Vec<(Rc<dyn Fn(&SharedValue)>, SharedValue)> = {
            let listeners = self.listeners.borrow();
            events
                .into_iter()
                .flat_map(|e| {
                    listeners
                        .get(&e.topic)
                        .into_iter()
                        .flatten()
                        .map(move |(_, f)| (f.clone(), e.value.clone()))
                })
                .collect()
        };
        batch(|| {
            for (signal, value) in keys {
                signal.set(value);
            }
            for (signal, map) in maps {
                signal.set(map);
            }
            for (f, value) in listeners {
                f(&value);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn counter() -> (Arc<AtomicUsize>, impl Fn() + Send + Sync + 'static) {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        (count, move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
    }

    #[test]
    fn a_transaction_is_one_version() {
        let store = SharedStore::new();
        assert_eq!(store.version(), 0);
        let v = store.transaction(|tx| {
            tx.set("a", 1);
            tx.set("b", "x");
            tx.set("a", 2);
        });
        assert_eq!(v, 1);
        assert_eq!(store.get("a").and_then(|v| v.as_f64()), Some(2.0));
        assert_eq!(
            store.get("b").as_ref().and_then(SharedValue::as_str),
            Some("x")
        );
        // Nothing changed: no new version.
        assert_eq!(store.set("a", 2), 1);
        assert_eq!(store.delete("missing"), 1);
        assert_eq!(store.delete("a"), 2);
        assert!(store.get("a").is_none());
    }

    #[test]
    fn a_subscriber_is_woken_once_until_it_takes() {
        let store = SharedStore::new();
        let (wakes, wake) = counter();
        let sub = store.subscribe(wake);
        sub.watch("price");
        store.set("other", 1);
        assert_eq!(wakes.load(Ordering::SeqCst), 0, "an unwatched key");
        for i in 0..100 {
            store.set("price", i);
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        let changes = sub.take_changes();
        assert_eq!(changes.version, 101);
        assert_eq!(changes.values.len(), 1, "the latest value only");
        assert_eq!(
            changes.values[0].1.as_ref().and_then(|v| v.as_f64()),
            Some(99.0)
        );
        assert!(sub.take_changes().values.is_empty());
        store.set("price", 1);
        assert_eq!(wakes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn changes_taken_together_are_of_one_version() {
        let store = SharedStore::new();
        let sub = store.subscribe(|| {});
        sub.watch_prefix("row/");
        store.transaction(|tx| {
            tx.set("row/1", 1);
            tx.set("row/2", 1);
        });
        store.transaction(|tx| {
            tx.set("row/1", 2);
            tx.delete("row/2");
        });
        let changes = sub.take_changes();
        assert_eq!(changes.version, 2);
        let values: Vec<_> = changes
            .values
            .iter()
            .map(|(k, v)| (&**k, v.as_ref().and_then(|v| v.as_f64())))
            .collect();
        assert_eq!(values, [("row/1", Some(2.0)), ("row/2", None)]);
    }

    #[test]
    fn events_arrive_in_order_and_are_bounded() {
        let store = SharedStore::new();
        let (wakes, wake) = counter();
        let sub = store.subscribe(wake);
        store.emit("opened", 0);
        sub.listen("opened");
        for i in 0..(MAX_EVENTS + 5) {
            store.emit("opened", i as u32);
        }
        store.emit("closed", 1);
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        let events = sub.take_events();
        assert_eq!(events.len(), MAX_EVENTS);
        assert_eq!(events[0].value.as_f64(), Some(5.0));
        assert_eq!(
            events.last().and_then(|e| e.value.as_f64()),
            Some((MAX_EVENTS + 4) as f64)
        );
    }

    #[test]
    fn dropped_subscribers_are_forgotten() {
        let store = SharedStore::new();
        let (wakes, wake) = counter();
        let sub = store.subscribe(wake);
        sub.watch("k");
        drop(sub);
        store.set("k", true);
        assert_eq!(wakes.load(Ordering::SeqCst), 0);
        let again = store.subscribe(|| {});
        again.watch("k");
        store.set("k", false);
        assert_eq!(again.take_changes().values.len(), 1);
    }

    #[test]
    fn opaque_values_stay_rust_values() {
        #[derive(Debug, PartialEq)]
        struct Quote {
            bid: u32,
        }
        let store = SharedStore::new();
        store.set("q", SharedValue::opaque(Quote { bid: 7 }));
        let q = store.get("q").and_then(|v| v.downcast::<Quote>());
        assert_eq!(q.as_deref(), Some(&Quote { bid: 7 }));
    }

    #[test]
    fn writers_on_other_threads_wake_the_reader() {
        let store = SharedStore::new();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx = Mutex::new(tx);
        let sub = store.subscribe(move || {
            let _ = tx.lock().unwrap().send(());
        });
        sub.watch_prefix("n/");
        let writers: Vec<_> = (0..4)
            .map(|w| {
                let store = store.clone();
                std::thread::spawn(move || {
                    for i in 0..1000 {
                        store.set(format!("n/{w}"), i);
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        rx.recv().unwrap();
        let changes = sub.take_changes();
        assert_eq!(changes.version, 4000);
        assert_eq!(changes.values.len(), 4);
        assert!(
            changes
                .values
                .iter()
                .all(|(_, v)| v.as_ref().and_then(|v| v.as_f64()) == Some(999.0))
        );
    }
}

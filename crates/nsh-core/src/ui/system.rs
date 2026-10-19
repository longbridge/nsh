//! What the runtime's system modules (`net`, files, processes) need of the
//! machinery beyond values: the VM's capabilities ([`System`], a parameter
//! the call fills in rather than JS), work that starts in the JS call and
//! finishes as a task ([`Later`]), and errors of a chosen JS class
//! ([`JsError`]).

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    fmt,
    future::Future,
    path::PathBuf,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

use rustc_hash::FxHashMap;

use super::{JsHost, Ret, Thrown, call::JsResult, module::ModuleFuture};
use crate::access::{Access, TrustedPaths};

/// The VM a call comes from, as the host configured it: what the app may
/// do and where its files are. A parameter `sys: System` of a module
/// function or a class's member is not one JS passes: the call fills it
/// in (an `async` one when it is made, as its arguments are converted). A
/// host that configured nothing gives one that grants nothing.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct System {
    /// The capabilities: the app's declaration and the host's grant.
    pub access: Arc<Access>,
    /// The app's own data directory (`${dataDir}`), when the host has one.
    pub data_dir: Option<PathBuf>,
    /// The host's anchors of `${pluginDir}` and `${homeDir}`.
    pub paths: TrustedPaths,
    /// The VM's counts of limited resources.
    pub quotas: Quotas,
    /// The VM's own state of the runtime's modules (`process`'s children).
    pub locals: Locals,
    /// What the host does when the app asks to exit (`process.exit`);
    /// `None`: it installed nothing.
    pub exit: Option<Rc<dyn Fn(i32)>>,
    /// The operating system the app runs on, as the host says.
    pub os: crate::platform::Os,
}

impl System {
    /// The VM's capabilities and directories.
    pub fn new(
        access: Arc<Access>,
        data_dir: Option<PathBuf>,
        paths: TrustedPaths,
        quotas: Quotas,
    ) -> Self {
        Self {
            access,
            data_dir,
            paths,
            quotas,
            locals: Locals::default(),
            exit: None,
            os: crate::platform::Os::default(),
        }
    }

    /// The operating system the app runs on.
    pub fn with_os(mut self, os: crate::platform::Os) -> Self {
        self.os = os;
        self
    }

    /// The VM's state of the runtime modules, kept with it.
    pub fn with_locals(mut self, locals: Locals) -> Self {
        self.locals = locals;
        self
    }

    /// What the host does with an exit request.
    pub fn with_exit(mut self, exit: Option<Rc<dyn Fn(i32)>>) -> Self {
        self.exit = exit;
        self
    }
}

/// One VM's state of a runtime module (a [`Locals`] entry): made on first
/// use, closed when the VM goes.
pub trait Local: Default + 'static {
    /// Ends what the state owns, before the VM's callbacks go (children
    /// killed, connections closed). Runs once.
    fn close(&self) {}
}

type Entry = (TypeId, Rc<dyn Any>, fn(&dyn Any));

/// One VM's states of the runtime modules ([`Local`]), one per type: the
/// host keeps one per VM, hands it to every [`System`] and closes it when
/// the VM goes.
#[derive(Clone, Default)]
pub struct Locals(Rc<RefCell<Vec<Entry>>>);

impl Locals {
    /// The VM's `T`, made the first time.
    pub fn get<T: Local>(&self) -> Rc<T> {
        let id = TypeId::of::<T>();
        if let Some((_, local, _)) = self.0.borrow().iter().find(|(t, ..)| *t == id) {
            return local
                .clone()
                .downcast()
                .expect("the type it is filed under");
        }
        let local = Rc::new(T::default());
        let close: fn(&dyn Any) = |local| {
            if let Some(local) = local.downcast_ref::<T>() {
                local.close();
            }
        };
        self.0.borrow_mut().push((id, local.clone(), close));
        local
    }

    /// Closes every state, in the order they were made, and forgets them
    /// (the host, when its VM goes).
    pub fn close_all(&self) {
        let locals = std::mem::take(&mut *self.0.borrow_mut());
        for (_, local, close) in &locals {
            close(&**local);
        }
    }
}

/// What a call that is not `async` can do beyond computing (a parameter
/// `cx: CallCx`, filled in by the call): start a task of the core, and tie
/// work to what owns the call in JS. Only in the call: it borrows it.
#[derive(Clone, Copy)]
pub struct CallCx<'a> {
    host: &'a dyn JsHost,
}

impl<'a> CallCx<'a> {
    #[doc(hidden)]
    pub fn new(host: &'a dyn JsHost) -> Self {
        Self { host }
    }

    /// Runs `work` as a task of the core, started in order with what JS
    /// sent before this call (nothing waits for it).
    pub fn start(&self, work: impl Future<Output = ()> + 'static) {
        self.host.start_task(Box::new(move || {
            Box::pin(async move {
                work.await;
                Ok(Box::new(()) as Box<dyn super::ToJs>)
            })
        }));
    }

    /// Runs `f` when what owns the call in JS goes: the reactive owner it
    /// runs under, or the module's run under hot reload (otherwise when the
    /// VM goes, as everything else does).
    pub fn on_cleanup(&self, f: impl FnOnce() + 'static) {
        self.host.on_cleanup(Box::new(f));
    }
}

impl fmt::Debug for System {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("System")
            .field("data_dir", &self.data_dir)
            .finish_non_exhaustive()
    }
}

/// One VM's counts of limited resources (open sockets, ...), by kind: a
/// host keeps one per VM and hands it to every [`System`].
#[derive(Clone, Default)]
pub struct Quotas(Rc<RefCell<FxHashMap<&'static str, usize>>>);

impl Quotas {
    /// Takes one `kind` when fewer than `max` are held; dropping the
    /// [`Held`] gives it back.
    pub fn take(&self, kind: &'static str, max: usize) -> Option<Held> {
        let mut counts = self.0.borrow_mut();
        let count = counts.entry(kind).or_default();
        (*count < max).then(|| {
            *count += 1;
            Held {
                quotas: self.clone(),
                kind,
            }
        })
    }

    /// How many `kind` are held.
    pub fn held(&self, kind: &'static str) -> usize {
        self.0.borrow().get(kind).copied().unwrap_or(0)
    }
}

/// One resource of a [`Quotas`] kind, given back when dropped.
pub struct Held {
    quotas: Quotas,
    kind: &'static str,
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(count) = self.quotas.0.borrow_mut().get_mut(self.kind) {
            *count -= 1;
        }
    }
}

impl fmt::Debug for Held {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Held({})", self.kind)
    }
}

/// Work a function or member that is not `async` starts in the JS call
/// and finishes as a task of the core: JS gets a promise of `T`. What the
/// call does before returning it (checking its arguments, taking a quota)
/// throws as a synchronous call's error does; the task's `Err` rejects.
/// TS: `Promise<T>`.
#[must_use = "the work runs only when the call returns it"]
pub struct Later<T>(Pin<Box<dyn Future<Output = Result<T, String>>>>);

impl<T: Ret + 'static> Later<T> {
    /// The task's work: its value, or the message the promise rejects with.
    pub fn new<E: fmt::Display>(work: impl Future<Output = Result<T, E>> + 'static) -> Self {
        Self(Box::pin(
            async move { work.await.map_err(|e| e.to_string()) },
        ))
    }

    pub(crate) fn into_future(self) -> ModuleFuture {
        Box::pin(async move {
            let result: JsResult = match self.0.await {
                Ok(v) => Ok(Box::new(v)),
                Err(e) => Err(e),
            };
            result
        })
    }
}

impl<T> fmt::Debug for Later<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Later")
    }
}

/// An error of a chosen JS class, for a synchronous call's `Err`: what
/// `Result<T, JsError>` throws (any other error type throws an `Error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsError {
    /// A `TypeError`: the call was not allowed or its value is wrong.
    Type(String),
    /// A `RangeError`: a value past a limit.
    Range(String),
    /// An `Error`.
    Error(String),
}

/// What an `Err` of a synchronous call throws: an `Error` with its message
/// for any [`fmt::Display`] error, the chosen class for a [`JsError`].
pub trait IntoThrown {
    /// The exception.
    fn into_thrown(self) -> Thrown;
}

impl<E: fmt::Display> IntoThrown for E {
    fn into_thrown(self) -> Thrown {
        Thrown::Error(self.to_string())
    }
}

impl IntoThrown for JsError {
    fn into_thrown(self) -> Thrown {
        match self {
            JsError::Type(m) => Thrown::Type(m),
            JsError::Range(m) => Thrown::Range(m),
            JsError::Error(m) => Thrown::Error(m),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        JsonValue,
        ui::{JsValue, Registry, declarations, json_host, prelude::*},
    };

    struct Files;

    #[module(flat)]
    impl Files {
        /// Reads `path` later.
        pub(crate) fn read(path: String, sys: System) -> Result<Later<Vec<u8>>, JsError> {
            let _ = (path, sys);
            Ok(Later::new(async { Ok::<_, String>(vec![1]) }))
        }

        /// Writes `bytes`, at once.
        pub(crate) fn write(sys: System, bytes: Vec<u8>) -> u32 {
            let _ = sys;
            bytes.len() as u32
        }

        /// Starts a task, tied to the call's owner.
        pub(crate) fn watch(path: String, cx: CallCx<'_>) {
            cx.on_cleanup(move || drop(path));
            cx.start(async {});
        }

        /// Where the files are.
        #[getter]
        pub(crate) fn root(sys: System) -> Option<String> {
            sys.data_dir.map(|d| d.display().to_string())
        }
    }

    #[class]
    struct Handle;

    #[class]
    impl Handle {
        /// Its next bytes, later.
        pub(crate) fn next(&self, sys: System, size: Option<u32>) -> Later<Option<Vec<u8>>> {
            let _ = (sys, size);
            Later::new(async { Ok::<_, String>(None) })
        }

        /// Itself, for chaining.
        #[overload(r#"on(event: "a", f: () => void): Handle"#)]
        pub(crate) fn on(this: Instance<Self>, event: String, cx: CallCx<'_>) -> Instance<Self> {
            let _ = (event, cx);
            this
        }
    }

    #[test]
    fn bytes_are_uint8_arrays_tasks_promises_and_the_system_no_parameter() {
        let mut registry = Registry::ui_set("test/system");
        registry.module::<Files>().class::<Handle>();
        let d = declarations(&registry, &[]);
        assert!(
            d.contains("function read(path: string): Promise<Uint8Array>;"),
            "{d}"
        );
        assert!(
            d.contains("function write(bytes: Uint8Array): number;"),
            "{d}"
        );
        assert!(
            d.contains("next(size?: number | null): Promise<Uint8Array | null>;"),
            "{d}"
        );
        assert!(d.contains("function watch(path: string): void;"), "{d}");
        assert!(
            d.contains(r#"  on(event: "a", f: () => void): Handle;"#),
            "{d}"
        );
        // A getter is no flat export.
        assert!(!d.contains("function root"), "{d}");
    }

    #[test]
    fn bytes_convert_from_and_to_js() {
        let host = json_host::JsonHost::default();
        let raw = host.value(serde_json::json!([0, 7, 255]));
        let bytes = <Vec<u8>>::from_js(crate::ui::JsIn::new(&host, raw)).unwrap();
        assert_eq!(bytes, [0, 7, 255]);
        let out = bytes.to_js(&host).unwrap();
        assert_eq!(host.read(out), Some(serde_json::json!([0, 7, 255])));
        let bad = host.value(JsonValue::from("x"));
        assert!(<Vec<u8>>::from_js(crate::ui::JsIn::new(&host, bad)).is_err());
    }

    #[test]
    fn quotas_count_what_is_held() {
        let quotas = super::Quotas::default();
        let a = quotas.take("k", 2).unwrap();
        let _b = quotas.take("k", 2).unwrap();
        assert!(quotas.take("k", 2).is_none());
        drop(a);
        assert_eq!(quotas.held("k"), 1);
        assert!(quotas.take("other", 1).is_some());
    }
}

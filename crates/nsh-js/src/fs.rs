//! `fs/promises`: capability-checked files. Paths are checked against
//! `capabilities.fs` on the JS thread; the disk work then runs off it, confined
//! to the granted root by a cap-std `Dir`, so a symlink inside a root cannot
//! lead a later operation outside it.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

use rquickjs::qjs;

use crate::web::js::{self, Ctx, Deferred, R, Raw, V, native};

/// The largest file `readFile` returns.
#[cfg(not(target_arch = "wasm32"))]
const MAX_READ: usize = 64 * 1024 * 1024;
/// The most data one `writeFile` writes.
const MAX_WRITE: usize = 8 * 1024 * 1024;
/// Operations one VM may have outstanding.
const MAX_PENDING: usize = 1024;
/// File data one VM may hold in flight: data handed to `writeFile` until it is
/// written, and data read (contents and directory names) until JS takes it.
const MAX_IN_FLIGHT: usize = 64 * 1024 * 1024;

struct FileInfo {
    size: u64,
    modified: f64,
    accessed: f64,
    created: f64,
    mode: u32,
    file: bool,
    directory: bool,
    symlink: bool,
}
// WebAssembly has no files: its operations only ever fail.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
enum Value {
    Bytes(Vec<u8>),
    Entries(Vec<(String, bool)>, bool),
    Stat(FileInfo),
    Text(String),
    /// A symlink's target, which holds nothing of the budget.
    Link(String),
    Bool(bool),
    Unit,
}
impl Value {
    /// The bytes of the in-flight budget this result holds until JS takes it.
    fn held(&self) -> usize {
        match self {
            Value::Bytes(bytes) => bytes.len(),
            Value::Text(text) => text.len(),
            Value::Entries(entries, _) => entries.iter().map(|(name, _)| name.len()).sum(),
            Value::Stat(_) | Value::Link(_) | Value::Bool(_) | Value::Unit => 0,
        }
    }
}
struct Completion {
    id: u64,
    result: Result<Value, String>,
}

/// One VM's bytes of file data in flight, shared with its workers.
#[derive(Clone, Default)]
struct Budget(Arc<AtomicUsize>);
impl Budget {
    fn reserve(&self, bytes: usize) -> bool {
        self.0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|&total| total <= MAX_IN_FLIGHT)
            })
            .is_ok()
    }
    fn release(&self, bytes: usize) {
        self.0.fetch_sub(bytes, Ordering::AcqRel);
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn exceeded(name: &str) -> String {
        format!(
            "cannot read `{name}`: the runtime already holds {MAX_IN_FLIGHT} bytes of file data in flight"
        )
    }
}

/// One VM's outstanding operations: each one's promise and the bytes it holds
/// from submission (a write's data).
#[non_exhaustive]
pub(crate) struct FileSystem {
    pending: RefCell<HashMap<u64, (Deferred, usize)>>,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
    next: Cell<u64>,
    budget: Budget,
}
impl Default for FileSystem {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            pending: RefCell::new(HashMap::new()),
            tx,
            rx,
            next: Cell::new(0),
            budget: Budget::default(),
        }
    }
}
fn state<'a>(ctx: Ctx) -> &'a FileSystem {
    // SAFETY: the live context owns State; only its files field is borrowed.
    unsafe { &(*crate::state::state(ctx)).files }
}
pub(crate) fn has_pending(ctx: Ctx) -> bool {
    !state(ctx).pending.borrow().is_empty()
}
pub(crate) fn free_all(ctx: Ctx) {
    state(ctx).pending.borrow_mut().clear();
}
/// Settles the promises of finished operations. Returns whether any finished.
pub(crate) fn pump(ctx: Ctx) -> bool {
    let mut completed = false;
    while let Ok(c) = state(ctx).rx.try_recv() {
        completed = true;
        let held = c.result.as_ref().map_or(0, Value::held);
        let entry = state(ctx).pending.borrow_mut().remove(&c.id);
        let Some((d, submitted)) = entry else {
            state(ctx).budget.release(held);
            continue;
        };
        match c.result {
            Ok(v) => match value_js(ctx, v) {
                Ok(v) => d.resolve(ctx, v.raw()),
                Err(_) => {
                    let error = js::take_exception(ctx);
                    d.reject(ctx, error.raw());
                }
            },
            Err(e) => {
                if let Ok(v) = error_value(ctx, &e) {
                    d.reject(ctx, v.raw());
                }
            }
        }
        state(ctx).budget.release(held + submitted);
    }
    completed
}
fn error_value(ctx: Ctx, message: &str) -> R<V> {
    // SAFETY: JS_NewError creates an owned intrinsic Error in this live context.
    let error = V::checked(ctx, unsafe { qjs::JS_NewError(ctx) })?;
    js::define(
        ctx,
        error.raw(),
        "message",
        js::string(ctx, message)?,
        false,
    )?;
    Ok(error)
}
fn info_value(ctx: Ctx, info: FileInfo) -> R<V> {
    let obj = js::new_object(ctx)?;
    for (name, value) in [
        ("isFile", info.file),
        ("isDirectory", info.directory),
        ("isSymbolicLink", info.symlink),
    ] {
        let method = js::closure(ctx, vec![], move |_, _, _| Ok(V::bool(value)))?;
        js::define(ctx, obj.raw(), name, method, true)?;
    }
    js::define(ctx, obj.raw(), "size", V::num(info.size as f64), true)?;
    js::define(ctx, obj.raw(), "mode", V::num(info.mode as f64), true)?;
    let global = js::global(ctx);
    let date = js::get(ctx, global.raw(), "Date")?;
    for (name, millis) in [
        ("mtime", info.modified),
        ("atime", info.accessed),
        ("birthtime", info.created),
    ] {
        js::define(ctx, obj.raw(), &format!("{name}Ms"), V::num(millis), true)?;
        js::define(
            ctx,
            obj.raw(),
            name,
            js::construct(ctx, date.raw(), &[V::num(millis).raw()])?,
            true,
        )?;
    }
    Ok(obj)
}
fn value_js(ctx: Ctx, value: Value) -> R<V> {
    match value {
        Value::Bytes(b) => js::uint8_array(ctx, &b),
        Value::Entries(entries, with_types) => js::new_array(
            ctx,
            entries
                .into_iter()
                .map(|(name, directory)| {
                    if with_types {
                        let obj = js::new_object(ctx)?;
                        js::define(ctx, obj.raw(), "name", js::string(ctx, &name)?, true)?;
                        js::define(
                            ctx,
                            obj.raw(),
                            "isDirectory",
                            js::closure(ctx, vec![], move |_, _, _| Ok(V::bool(directory)))?,
                            true,
                        )?;
                        Ok(obj)
                    } else {
                        js::string(ctx, &name)
                    }
                })
                .collect::<R<Vec<_>>>()?,
        ),
        Value::Stat(info) => info_value(ctx, info),
        Value::Text(t) | Value::Link(t) => js::string(ctx, &t),
        Value::Bool(b) => Ok(V::bool(b)),
        Value::Unit => Ok(V::undefined()),
    }
}
/// Starts one operation off the JS thread. `held` bytes (a write's data) are
/// taken from the VM's in-flight budget until the operation settles.
fn submit(
    ctx: Ctx,
    held: usize,
    job: impl FnOnce(&Budget) -> Result<Value, String> + Send + 'static,
) -> R<V> {
    let files = state(ctx);
    if files.pending.borrow().len() >= MAX_PENDING {
        return Err(js::range_error(
            ctx,
            &format!("fs exceeded the per-runtime outstanding host task limit of {MAX_PENDING}"),
        ));
    }
    if !files.budget.reserve(held) {
        return Err(js::range_error(
            ctx,
            &format!(
                "fs exceeded the per-runtime limit of {MAX_IN_FLIGHT} bytes of file data in flight"
            ),
        ));
    }
    let d = match Deferred::new(ctx) {
        Ok(d) => d,
        Err(e) => {
            files.budget.release(held);
            return Err(e);
        }
    };
    let promise = d.promise.clone_ref();
    let id = files.next.get();
    files.next.set(id.wrapping_add(1));
    files.pending.borrow_mut().insert(id, (d, held));
    let tx = files.tx.clone();
    let budget = files.budget.clone();
    let wake = crate::dom::shared(ctx).system_wake.borrow().clone();
    let work = move || {
        let result = job(&budget);
        let _ = tx.send(Completion { id, result });
        if let Some(wake) = wake {
            wake();
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        drop(handle.spawn_blocking(work));
        return Ok(promise);
    }
    let spawned = std::thread::Builder::new()
        .name("nsh-files".into())
        .spawn(work);
    if let Err(e) = spawned {
        let entry = files.pending.borrow_mut().remove(&id);
        if let Some((d, held)) = entry {
            files.budget.release(held);
            let e = error_value(ctx, &e.to_string())?;
            d.reject(ctx, e.raw());
        }
    }
    Ok(promise)
}
fn is_string(value: Raw) -> bool {
    // SAFETY: inspecting the live value's tag does not execute JS.
    unsafe { qjs::JS_IsString(value) }
}
fn option_keys(ctx: Ctx, value: Raw) -> R<Vec<String>> {
    let mut table = std::ptr::null_mut();
    let mut len = 0;
    // SAFETY: value is a live object; QuickJS returns a table owned until FreePropertyEnum.
    if unsafe {
        qjs::JS_GetOwnPropertyNames(
            ctx,
            &mut table,
            &mut len,
            value,
            (qjs::JS_GPN_STRING_MASK | qjs::JS_GPN_ENUM_ONLY) as i32,
        )
    } < 0
    {
        return Err(js::Throw);
    }
    let result = (|| -> R<Vec<String>> {
        let mut keys = Vec::with_capacity(len as usize);
        for i in 0..len as usize {
            // SAFETY: table contains len entries; AtomToString returns an owned string value.
            let key = V::checked(ctx, unsafe {
                qjs::JS_AtomToString(ctx, (*table.add(i)).atom)
            })?;
            keys.push(js::to_string(ctx, key.raw())?);
        }
        Ok(keys)
    })();
    // SAFETY: table and len are exactly the allocation returned above.
    unsafe { qjs::JS_FreePropertyEnum(ctx, table, len) };
    result
}
fn strict_options(ctx: Ctx, value: Raw, allowed: &[&str]) -> R<()> {
    if js::is_nullish(value) {
        return Ok(());
    }
    if !js::is_object(value) {
        return Err(js::type_error(ctx, "filesystem options must be an object"));
    }
    for key in option_keys(ctx, value)? {
        if !allowed.contains(&key.as_str()) {
            return Err(js::type_error(
                ctx,
                &format!("unknown filesystem option `{key}`"),
            ));
        }
    }
    Ok(())
}
fn bool_option(ctx: Ctx, value: Raw, key: &str) -> R<bool> {
    strict_options(ctx, value, &[key])?;
    if js::is_nullish(value) {
        return Ok(false);
    }
    let v = js::get(ctx, value, key)?;
    if js::is_nullish(v.raw()) {
        return Ok(false);
    }
    // SAFETY: this only checks a live value's tag.
    if !unsafe { qjs::JS_IsBool(v.raw()) } {
        return Err(js::type_error(ctx, &format!("{key} must be a boolean")));
    }
    Ok(js::to_bool(ctx, v.raw()))
}
fn lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => (),
            other => out.push(other.as_os_str()),
        }
    }
    out
}
/// The canonical form of the nearest existing ancestor, with the rest appended.
fn resolved(path: &Path) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        if let Ok(mut real) = current.canonicalize() {
            real.extend(tail.into_iter().rev());
            return Ok(real);
        }
        let name = current
            .file_name()
            .ok_or("invalid filesystem path")?
            .to_owned();
        if !current.pop() {
            return Err("invalid filesystem path".into());
        }
        tail.push(name);
    }
}
fn available(ctx: Ctx) -> bool {
    // SAFETY: short projection of the live renderer state.
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    )) && unsafe { (*crate::state::state(ctx)).dom.os().is_desktop() }
}

/// A checked path: the granted root, opened, and the path within it.
struct Grant {
    #[cfg(not(target_arch = "wasm32"))]
    dir: cap_std::fs::Dir,
    #[cfg(not(target_arch = "wasm32"))]
    relative: PathBuf,
    name: String,
}
/// Checks a path against the `capabilities.fs` roots of a read or a write.
/// `link` operations act on the last component itself (a symlink there is not
/// followed: `lstat`, `readlink`, `exists`, `unlink`, `rename`, `rmdir`), so only
/// its parent is resolved.
fn grant(ctx: Ctx, value: Raw, write: bool, link: bool) -> R<Grant> {
    if !is_string(value) {
        return Err(js::type_error(ctx, "filesystem path must be a string"));
    }
    let path = PathBuf::from(js::to_string(ctx, value)?);
    let shared = crate::dom::shared(ctx);
    let data = shared.system_data_dir.borrow().clone().unwrap_or_default();
    let access = shared.system_access.borrow().clone();
    let paths = shared.system_paths.borrow().clone();
    let roots = if write {
        &access.declared.fs.write
    } else {
        &access.declared.fs.read
    };
    if !roots.is_empty() && !available(ctx) {
        return Err(js::type_error(
            ctx,
            "native filesystem unavailable on this platform",
        ));
    }
    let requested = if path.to_string_lossy().starts_with("${") {
        paths
            .resolve_root(&path.to_string_lossy(), &data)
            .map_err(|e| js::type_error(ctx, &e))?
    } else {
        path
    };
    for raw in roots {
        let Ok(root) = paths.resolve_root(raw, &data).and_then(|p| resolved(&p)) else {
            continue;
        };
        let candidate = if requested.is_absolute() {
            if link {
                match (requested.parent(), requested.file_name()) {
                    (Some(parent), Some(name)) => resolved(parent).map(|p| p.join(name)),
                    _ => resolved(&requested),
                }
            } else {
                resolved(&requested)
            }
        } else {
            Ok(lexical(&root.join(&requested)))
        };
        let mut candidate = candidate.unwrap_or_else(|_| lexical(&requested));
        if !candidate.starts_with(&root) {
            if link && requested.is_absolute() && resolved(&requested).is_ok_and(|p| p == root) {
                candidate = root.clone();
            } else {
                continue;
            }
        }
        let effective = if let Some(host) = &access.granted {
            let grants = if write { &host.fs.write } else { &host.fs.read };
            grants
                .iter()
                .filter_map(|r| paths.resolve_root(r, &data).and_then(|p| resolved(&p)).ok())
                .filter(|r| candidate.starts_with(r))
                .map(|r| {
                    if r.starts_with(&root) {
                        r
                    } else {
                        root.clone()
                    }
                })
                .next()
        } else {
            Some(root.clone())
        };
        let Some(root) = effective else { continue };
        let Ok(relative) = candidate.strip_prefix(&root) else {
            continue;
        };
        let name = if relative.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            relative.display().to_string()
        };
        #[cfg(target_arch = "wasm32")]
        return Ok(Grant { name });
        #[cfg(not(target_arch = "wasm32"))]
        {
            // A granted root belongs to the app: a write may create it.
            if write && !root.exists() && std::fs::create_dir_all(&root).is_err() {
                continue;
            }
            let Ok(dir) = confined::open_root(&root) else {
                continue;
            };
            return Ok(Grant {
                dir,
                relative: if relative.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    relative.to_owned()
                },
                name,
            });
        }
    }
    Err(js::type_error(
        ctx,
        &format!(
            "filesystem {} is not granted by capabilities.fs.{}",
            if write { "write" } else { "read" },
            if write { "write" } else { "read" }
        ),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
impl Grant {
    fn error(&self, verb: &str, error: std::io::Error) -> String {
        format!("cannot {verb} `{}`: {error}", self.name)
    }
    fn read(self, budget: &Budget, text: bool) -> Result<Value, String> {
        use std::io::Read;
        let mut file = confined::open_file(&self.dir, &self.relative, false)
            .map_err(|e| self.error("read", e))?;
        let size = file.metadata().map_err(|e| self.error("read", e))?.len();
        let too_big = |len: u64| {
            format!(
                "`{}` is {len} bytes, over the {MAX_READ}-byte limit for fs.readFile; read it in pieces or keep it out of the script",
                self.name
            )
        };
        if size > MAX_READ as u64 {
            return Err(too_big(size));
        }
        // The file may change while it is read: hold its size, then what was read.
        let mut reserved = size as usize;
        if !budget.reserve(reserved) {
            return Err(Budget::exceeded(&self.name));
        }
        let mut bytes = Vec::with_capacity(reserved);
        let read = file
            .by_ref()
            .take((MAX_READ + 1) as u64)
            .read_to_end(&mut bytes);
        let fits = if bytes.len() > reserved {
            let more = bytes.len() - reserved;
            let fits = budget.reserve(more);
            if fits {
                reserved += more;
            }
            fits
        } else {
            budget.release(reserved - bytes.len());
            reserved = bytes.len();
            true
        };
        let result = if let Err(e) = read {
            Err(self.error("read", e))
        } else if !fits {
            Err(Budget::exceeded(&self.name))
        } else if bytes.len() > MAX_READ {
            Err(too_big(bytes.len() as u64))
        } else if text {
            // Valid UTF-8 keeps its length, and so what it holds of the budget.
            String::from_utf8(bytes)
                .map(Value::Text)
                .map_err(|e| format!("cannot decode `{}` as UTF-8: {e}", self.name))
        } else {
            Ok(Value::Bytes(bytes))
        };
        if result.is_err() {
            budget.release(reserved);
        }
        result
    }
    fn write(self, bytes: Vec<u8>) -> Result<Value, String> {
        use std::io::Write;
        let mut file = confined::open_file(&self.dir, &self.relative, true)
            .map_err(|e| self.error("write", e))?;
        file.set_len(0).map_err(|e| self.error("write", e))?;
        file.write_all(&bytes).map_err(|e| self.error("write", e))?;
        Ok(Value::Unit)
    }
    fn mkdir(self, recursive: bool) -> Result<Value, String> {
        if recursive {
            self.dir.create_dir_all(&self.relative)
        } else {
            self.dir.create_dir(&self.relative)
        }
        .map_err(|e| self.error("create", e))?;
        Ok(Value::Unit)
    }
    /// Whether the entry exists; a symlink exists even when its target does not.
    fn exists(self) -> Result<Value, String> {
        Ok(Value::Bool(
            self.dir.symlink_metadata(&self.relative).is_ok(),
        ))
    }
    fn stat(self, link: bool) -> Result<Value, String> {
        let m = if link {
            self.dir.symlink_metadata(&self.relative)
        } else {
            self.dir.metadata(&self.relative)
        }
        .map_err(|e| self.error("stat", e))?;
        Ok(Value::Stat(confined::info(m)))
    }
    fn readlink(self) -> Result<Value, String> {
        self.dir
            .read_link_contents(&self.relative)
            .map(|p| Value::Link(p.to_string_lossy().into_owned()))
            .map_err(|e| self.error("read link", e))
    }
    fn unlink(self) -> Result<Value, String> {
        self.dir
            .remove_file(&self.relative)
            .map_err(|e| self.error("remove", e))?;
        Ok(Value::Unit)
    }
    fn rmdir(self) -> Result<Value, String> {
        self.dir
            .remove_dir(&self.relative)
            .map_err(|e| self.error("remove", e))?;
        Ok(Value::Unit)
    }
    fn rename(self, to: Self) -> Result<Value, String> {
        self.dir
            .rename(&self.relative, &to.dir, &to.relative)
            .map_err(|e| self.error("rename", e))?;
        Ok(Value::Unit)
    }
    fn entries(self, budget: &Budget, types: bool) -> Result<Value, String> {
        let mut entries = Vec::new();
        let mut name_bytes = 0;
        for entry in self
            .dir
            .read_dir(&self.relative)
            .map_err(|e| self.error("list", e))?
        {
            let entry = entry.map_err(|e| self.error("list", e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            name_bytes += name.len();
            if entries.len() == 10_000 || name_bytes > 1024 * 1024 {
                return Err(format!(
                    "cannot list `{}`: directory exceeded the 10000-entry or 1048576-name-byte fs.readdir limit",
                    self.name
                ));
            }
            let directory = entry.file_type().is_ok_and(|t| t.is_dir());
            entries.push((name, directory));
        }
        if !budget.reserve(name_bytes) {
            return Err(Budget::exceeded(&self.name));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Value::Entries(entries, types))
    }
}
#[cfg(target_arch = "wasm32")]
impl Grant {
    fn unavailable(self) -> Result<Value, String> {
        Err(format!("cannot open `{}`: no native filesystem", self.name))
    }
    fn read(self, _: &Budget, _: bool) -> Result<Value, String> {
        self.unavailable()
    }
    fn write(self, _: Vec<u8>) -> Result<Value, String> {
        self.unavailable()
    }
    fn mkdir(self, _: bool) -> Result<Value, String> {
        self.unavailable()
    }
    fn exists(self) -> Result<Value, String> {
        self.unavailable()
    }
    fn stat(self, _: bool) -> Result<Value, String> {
        self.unavailable()
    }
    fn readlink(self) -> Result<Value, String> {
        self.unavailable()
    }
    fn unlink(self) -> Result<Value, String> {
        self.unavailable()
    }
    fn rmdir(self) -> Result<Value, String> {
        self.unavailable()
    }
    fn rename(self, _: Self) -> Result<Value, String> {
        self.unavailable()
    }
    fn entries(self, _: &Budget, _: bool) -> Result<Value, String> {
        self.unavailable()
    }
}
native! {fn read_file(ctx,_this,args){let text=if js::is_nullish(args.get(1)){false}else{let encoding=if is_string(args.get(1)){js::to_string(ctx,args.get(1))?}else{strict_options(ctx,args.get(1),&["encoding"])?;let v=js::get(ctx,args.get(1),"encoding")?;if !is_string(v.raw()){return Err(js::type_error(ctx,"fs.readFile encoding must be utf8"))}js::to_string(ctx,v.raw())?};if !matches!(encoding.to_ascii_lowercase().as_str(),"utf8"|"utf-8"){return Err(js::type_error(ctx,"fs.readFile only supports UTF-8 text decoding"))}true};let grant=grant(ctx,args.get(0),false,false)?;submit(ctx,0,move|budget|grant.read(budget,text))}}
native! {fn write_file(ctx,_this,args){let value=args.get(1);let bytes=if is_string(value){js::to_string(ctx,value)?.into_bytes()}else{let view=js::view(ctx,value).ok_or_else(||js::type_error(ctx,"fs.writeFile expects string or Uint8Array"))?;if view.kind!=Some(qjs::JSTypedArrayEnum_JS_TYPED_ARRAY_UINT8 as std::ffi::c_int){return Err(js::type_error(ctx,"fs.writeFile expects string or Uint8Array"))}
// SAFETY: copy the buffer before any JS can detach it.
unsafe{js::bytes_of(ctx,value)}.ok_or_else(||js::type_error(ctx,"detached Uint8Array"))?.to_vec()};let grant=grant(ctx,args.get(0),true,false)?;if bytes.len()>MAX_WRITE{return Err(js::range_error(ctx,&format!("`{}` write is {} bytes, over the {MAX_WRITE}-byte fs.writeFile limit",grant.name,bytes.len())))}submit(ctx,bytes.len(),move|_|grant.write(bytes))}}
native! {fn mkdir(ctx,_this,args){let recursive=bool_option(ctx,args.get(1),"recursive")?;let grant=grant(ctx,args.get(0),true,false)?;submit(ctx,0,move|_|grant.mkdir(recursive))}}
native! {fn exists(ctx,_this,args){let grant=grant(ctx,args.get(0),false,true)?;submit(ctx,0,move|_|grant.exists())}}
native! {fn readdir(ctx,_this,args){let types=bool_option(ctx,args.get(1),"withFileTypes")?;let grant=grant(ctx,args.get(0),false,false)?;submit(ctx,0,move|budget|grant.entries(budget,types))}}
native! {fn stat(ctx,_this,args){let grant=grant(ctx,args.get(0),false,false)?;submit(ctx,0,move|_|grant.stat(false))}}
native! {fn lstat(ctx,_this,args){let grant=grant(ctx,args.get(0),false,true)?;submit(ctx,0,move|_|grant.stat(true))}}
native! {fn readlink(ctx,_this,args){let grant=grant(ctx,args.get(0),false,true)?;submit(ctx,0,move|_|grant.readlink())}}
native! {fn unlink(ctx,_this,args){let grant=grant(ctx,args.get(0),true,true)?;submit(ctx,0,move|_|grant.unlink())}}
native! {fn rename(ctx,_this,args){let source=grant(ctx,args.get(0),true,true)?;let target=grant(ctx,args.get(1),true,true)?;submit(ctx,0,move|_|source.rename(target))}}
native! {fn rmdir(ctx,_this,args){let grant=grant(ctx,args.get(0),true,true)?;submit(ctx,0,move|_|grant.rmdir())}}
/// The object `fs/promises` exports, one function per operation.
pub(crate) fn module(ctx: Ctx) -> Raw {
    (|| -> R<V> {
        let obj = js::new_object(ctx)?;
        for (name, fun, len) in [
            ("readFile", read_file as js::Native, 2),
            ("writeFile", write_file, 2),
            ("readdir", readdir, 2),
            ("exists", exists, 1),
            ("unlink", unlink, 1),
            ("rmdir", rmdir, 1),
            ("mkdir", mkdir, 2),
            ("stat", stat, 1),
            ("lstat", lstat, 1),
            ("rename", rename, 2),
            ("readlink", readlink, 1),
        ] {
            let cname = std::ffi::CString::new(name).unwrap();
            js::define(
                ctx,
                obj.raw(),
                name,
                js::function(ctx, &cname, fun, len)?,
                true,
            )?;
        }
        Ok(obj)
    })()
    .map_or(qjs::JS_EXCEPTION, V::take)
}

/// Opening granted roots and the files in them.
#[cfg(not(target_arch = "wasm32"))]
mod confined {
    use std::{io, path::Path};

    use cap_std::fs::{Dir, File, Metadata, OpenOptions};

    /// Opens a canonical root. On Unix every component is opened with
    /// `O_NOFOLLOW` from `/`, so a symlink swapped in after the grant check
    /// cannot redirect the root.
    #[cfg(unix)]
    pub(super) fn open_root(path: &Path) -> io::Result<Dir> {
        use std::{
            ffi::CString,
            os::{
                fd::{AsRawFd, FromRawFd},
                unix::ffi::OsStrExt,
            },
            path::Component,
        };
        let mut current = std::fs::File::open("/")?;
        for part in path.components() {
            match part {
                Component::RootDir => (),
                Component::Normal(name) => {
                    let name = CString::new(name.as_bytes())
                        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                    // SAFETY: current is a live directory and name is NUL-terminated; the
                    // returned descriptor is owned below.
                    let fd = unsafe {
                        libc::openat(
                            current.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        )
                    };
                    if fd < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    // SAFETY: openat returned a new descriptor nothing else owns.
                    current = unsafe { std::fs::File::from_raw_fd(fd) };
                }
                _ => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
            }
        }
        Ok(Dir::from_std_file(current))
    }
    /// Opens a canonical root. cap-std confines every later operation to it;
    /// the root is the path the grant check resolved, opened through the
    /// ambient filesystem once.
    #[cfg(not(unix))]
    pub(super) fn open_root(path: &Path) -> io::Result<Dir> {
        Dir::open_ambient_dir(path, cap_std::ambient_authority())
    }
    /// Opens a regular file to read or to write (created when missing). On Unix
    /// it is opened non-blocking, so a FIFO or device cannot stall the worker;
    /// anything but a regular file is refused before a byte moves.
    pub(super) fn open_file(dir: &Dir, path: &Path, write: bool) -> io::Result<File> {
        let mut options = OpenOptions::new();
        if write {
            options.write(true).create(true);
        } else {
            options.read(true);
        }
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK).mode(0o644);
        }
        let file = dir.open_with(path, &options)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("not a regular file"));
        }
        Ok(file)
    }
    pub(super) fn info(meta: Metadata) -> super::FileInfo {
        let millis = |time: io::Result<cap_std::time::SystemTime>| {
            time.ok()
                .and_then(|t| t.into_std().duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0.0, |t| t.as_secs_f64() * 1000.0)
        };
        #[cfg(unix)]
        let mode = {
            use cap_std::fs::MetadataExt;
            meta.mode()
        };
        // Node's modes on Windows: the type bits, and write bits unless read-only.
        #[cfg(not(unix))]
        let mode = if meta.is_dir() { 0o040000 } else { 0o100000 }
            | if meta.permissions().readonly() {
                0o444
            } else {
                0o666
            };
        super::FileInfo {
            size: meta.len(),
            mode,
            accessed: millis(meta.accessed()),
            created: millis(meta.created()),
            modified: millis(meta.modified()),
            file: meta.is_file(),
            directory: meta.is_dir(),
            symlink: meta.is_symlink(),
        }
    }
}

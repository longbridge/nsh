//! Native, capability-checked UTF-8 files. Disk work runs off the JS thread.
use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, mpsc},
};

use nsh_core::access::Access;
use rquickjs::qjs;

use crate::{
    modules::{Export, Item, Ts},
    web::js::{self, Ctx, Deferred, R, Raw, V, native},
};
const MAX_TEXT: usize = 16 * 1024 * 1024;
const STANDARD_READ: usize = 64 * 1024 * 1024;
const STANDARD_WRITE: usize = 8 * 1024 * 1024;
#[derive(Clone)]
struct Config {
    dir: PathBuf,
    access: Access,
    paths: nsh_core::access::TrustedPaths,
    wake: Arc<dyn Fn() + Send + Sync>,
}
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
enum Value {
    Bytes(Vec<u8>),
    Entries(Vec<(String, FileInfo)>, bool),
    Stat(FileInfo),
    Text(String),
    Bool(bool),
    Unit,
}
struct Completion {
    id: u64,
    result: Result<Value, String>,
}
#[non_exhaustive]
pub(crate) struct FileSystem {
    pending: RefCell<HashMap<u64, (Deferred, bool)>>,
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
    next: std::cell::Cell<u64>,
}
impl Default for FileSystem {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            pending: RefCell::new(HashMap::new()),
            tx,
            rx,
            next: std::cell::Cell::new(0),
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
pub(crate) fn pump(ctx: Ctx) -> bool {
    poll(ctx)
}
pub(crate) fn free_all(ctx: Ctx) {
    state(ctx).pending.borrow_mut().clear();
}
pub(crate) fn poll(ctx: Ctx) -> bool {
    let mut completed = false;
    while let Ok(c) = state(ctx).rx.try_recv() {
        completed = true;
        let d = state(ctx).pending.borrow_mut().remove(&c.id);
        if let Some((d, standard)) = d {
            match c.result {
                Ok(v) => {
                    let v = value_js(ctx, v);
                    match v {
                        Ok(v) => d.resolve(ctx, v.raw()),
                        Err(_) => {
                            let error = js::take_exception(ctx);
                            d.reject(ctx, error.raw());
                        }
                    }
                }
                Err(e) => {
                    let error = if standard {
                        error_value(ctx, &e)
                    } else {
                        js::dom_exception(ctx, &e, "FileSystemError")
                    };
                    if let Ok(v) = error {
                        if !standard {
                            let code = e.split_once(':').map_or("EIO", |(code, _)| code);
                            if let Ok(code) = js::string(ctx, code)
                                && js::define(ctx, v.raw(), "code", code, true).is_err()
                            {
                                let _ = js::take_exception(ctx);
                            }
                        }
                        d.reject(ctx, v.raw());
                    }
                }
            }
        }
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
fn bytes_value(ctx: Ctx, bytes: &[u8]) -> R<V> {
    js::uint8_array(ctx, bytes)
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
        Value::Bytes(b) => bytes_value(ctx, &b),
        Value::Entries(entries, with_types) => js::new_array(
            ctx,
            entries
                .into_iter()
                .map(|(name, info)| {
                    if with_types {
                        let obj = js::new_object(ctx)?;
                        js::define(ctx, obj.raw(), "name", js::string(ctx, &name)?, true)?;
                        let directory = info.directory;
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
        Value::Text(t) => js::string(ctx, &t),
        Value::Bool(b) => Ok(V::bool(b)),
        Value::Unit => Ok(V::undefined()),
    }
}
fn submit(ctx: Ctx, job: impl FnOnce(Config) -> Result<Value, String> + Send + 'static) -> R<V> {
    submit_kind(ctx, false, job)
}
fn standard_submit(
    ctx: Ctx,
    job: impl FnOnce(Config) -> Result<Value, String> + Send + 'static,
) -> R<V> {
    submit_kind(ctx, true, job)
}
fn submit_kind(
    ctx: Ctx,
    standard: bool,
    job: impl FnOnce(Config) -> Result<Value, String> + Send + 'static,
) -> R<V> {
    let d = Deferred::new(ctx)?;
    let promise = d.promise.clone_ref();
    if !available(ctx) {
        let e = js::dom_exception(
            ctx,
            "Secure native filesystem access is unavailable on this platform",
            "NotSupportedError",
        )?;
        d.reject(ctx, e.raw());
        return Ok(promise);
    }
    let shared = crate::dom::shared(ctx);
    let dir = shared.system_data_dir.borrow().clone();
    let wake = shared.system_wake.borrow().clone();
    let config = if standard {
        Some(Config {
            dir: dir.unwrap_or_default(),
            wake: wake.unwrap_or_else(|| Arc::new(|| {})),
            access: shared.system_access.borrow().clone(),
            paths: shared.system_paths.borrow().clone(),
        })
    } else {
        dir.zip(wake).map(|(dir, wake)| Config {
            dir,
            wake,
            access: shared.system_access.borrow().clone(),
            paths: shared.system_paths.borrow().clone(),
        })
    };
    let Some(config) = config else {
        let e = js::dom_exception(
            ctx,
            "FileSystem is unavailable: this host has no native app data directory",
            "NotSupportedError",
        )?;
        d.reject(ctx, e.raw());
        return Ok(promise);
    };
    let limit = if standard { 1024 } else { 64 };
    if state(ctx).pending.borrow().len() >= limit {
        if standard {
            return Err(js::range_error(
                ctx,
                "fs exceeded the per-runtime outstanding host task limit of 1024",
            ));
        }
        let e = js::dom_exception(
            ctx,
            "Too many pending filesystem operations",
            "QuotaExceededError",
        )?;
        d.reject(ctx, e.raw());
        return Ok(promise);
    }
    let id = state(ctx).next.get();
    state(ctx).next.set(id.wrapping_add(1));
    state(ctx).pending.borrow_mut().insert(id, (d, standard));
    let tx = state(ctx).tx.clone();
    let wake = config.wake.clone();
    let work = move || {
        let result = job(config);
        let _ = tx.send(Completion { id, result });
        wake();
    };
    #[cfg(not(target_arch = "wasm32"))]
    if standard && let Ok(handle) = tokio::runtime::Handle::try_current() {
        drop(handle.spawn_blocking(work));
        return Ok(promise);
    }
    let spawned = std::thread::Builder::new()
        .name("nsh-files".into())
        .spawn(work);
    if let Err(e) = spawned {
        let d = state(ctx).pending.borrow_mut().remove(&id);
        if let Some((d, _)) = d {
            let e = js::dom_exception(ctx, &e.to_string(), "FileSystemError")?;
            d.reject(ctx, e.raw());
        }
    }
    Ok(promise)
}
fn path(ctx: Ctx, raw: Raw) -> R<PathBuf> {
    Ok(PathBuf::from(js::to_string(ctx, raw)?))
}
fn authorize(c: &Config, p: &std::path::Path, write: bool) -> Result<PathBuf, String> {
    c.access
        .authorize_file_with_paths(p, &c.dir, &c.paths, write)
        .map_err(|e| format!("EACCES: {e}"))
}
fn io(e: std::io::Error) -> String {
    format!(
        "{}: {e}",
        match e.kind() {
            std::io::ErrorKind::NotFound => "ENOENT",
            std::io::ErrorKind::PermissionDenied => "EACCES",
            std::io::ErrorKind::AlreadyExists => "EEXIST",
            std::io::ErrorKind::NotADirectory => "ENOTDIR",
            std::io::ErrorKind::IsADirectory => "EISDIR",
            std::io::ErrorKind::DirectoryNotEmpty => "ENOTEMPTY",
            _ => "EIO",
        }
    )
}
native! {fn app_data_dir(ctx,_this,_args){ submit(ctx,|c| {let p=authorize(&c,&c.dir,false)?;Ok(Value::Text(p.to_string_lossy().into_owned()))}) }}
native! {fn read_text(ctx,_this,args){let p=path(ctx,args.get(0))?;submit(ctx,move|c|{let p=authorize(&c,&p,false)?;let mut f=secure::open(&p,false,false)?;use std::io::Read;let mut b=Vec::new();f.by_ref().take((MAX_TEXT+1)as u64).read_to_end(&mut b).map_err(io)?;if b.len()>MAX_TEXT{return Err("EFBIG: text exceeds 16 MiB".into())}String::from_utf8(b).map(Value::Text).map_err(|_|"EILSEQ: file is not UTF-8".into())})}}
native! {fn write_text(ctx,_this,args){let p=path(ctx,args.get(0))?;let text=js::to_string(ctx,args.get(1))?;let options=args.get(2);let exclusive=if js::is_nullish(options){false}else{js::to_bool(ctx,js::get(ctx,options,"exclusive")?.raw())};submit(ctx,move|c|{if text.len()>MAX_TEXT{return Err("EFBIG: text exceeds 16 MiB".into())}let p=authorize(&c,&p,true)?;let mut f=secure::open(&p,true,exclusive)?;use std::io::Write;f.write_all(text.as_bytes()).map_err(io)?;f.sync_all().map_err(io)?;Ok(Value::Unit)})}}
native! {fn exists(ctx,_this,args){let p=path(ctx,args.get(0))?;submit(ctx,move|c|{let p=authorize(&c,&p,false)?;match std::fs::symlink_metadata(p){Ok(_)=>Ok(Value::Bool(true)),Err(e)if e.kind()==std::io::ErrorKind::NotFound=>Ok(Value::Bool(false)),Err(e)=>Err(io(e))}})}}
native! {fn mkdir(ctx,_this,args){let p=path(ctx,args.get(0))?;let options=args.get(1);let recursive=if js::is_nullish(options){false}else{js::to_bool(ctx,js::get(ctx,options,"recursive")?.raw())};submit(ctx,move|c|{let p=authorize(&c,&p,true)?;secure::mkdir(&p,recursive)?;Ok(Value::Unit)})}}
native! {fn rename(ctx,_this,args){let a=path(ctx,args.get(0))?;let b=path(ctx,args.get(1))?;submit(ctx,move|c|{let a=authorize(&c,&a,true)?;let b=authorize(&c,&b,true)?;secure::rename(&a,&b)?;Ok(Value::Unit)})}}
native! {fn remove(ctx,_this,args){let p=path(ctx,args.get(0))?;submit(ctx,move|c|{let p=authorize(&c,&p,true)?;secure::remove(&p)?;Ok(Value::Unit)})}}
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
fn lexical(path: &std::path::Path) -> PathBuf {
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
fn resolved(path: &std::path::Path) -> Result<PathBuf, String> {
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
struct StandardGrant {
    #[cfg(unix)]
    dir: cap_std::fs::Dir,
    #[cfg(unix)]
    relative: PathBuf,
    #[cfg(unix)]
    name: String,
}
fn prepare_grant(ctx: Ctx, value: Raw, write: bool, keep: bool) -> R<StandardGrant> {
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
            if keep {
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
            if keep && requested.is_absolute() && resolved(&requested).is_ok_and(|p| p == root) {
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
        #[cfg(not(unix))]
        let _ = root;
        #[cfg(unix)]
        {
            if write && !root.exists() && secure::mkdir(&root, true).is_err() {
                continue;
            }
            let Ok(dir) = secure::dir(&root) else {
                continue;
            };
            let Ok(relative) = candidate.strip_prefix(&root) else {
                continue;
            };
            return Ok(StandardGrant {
                dir,
                relative: if relative.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    relative.to_owned()
                },
                name: if relative.as_os_str().is_empty() {
                    ".".into()
                } else {
                    relative.display().to_string()
                },
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
impl StandardGrant {
    fn describe(&self) -> &str {
        #[cfg(unix)]
        {
            &self.name
        }
        #[cfg(not(unix))]
        {
            "unavailable filesystem"
        }
    }
    #[cfg(unix)]
    fn error(&self, verb: &str, error: std::io::Error) -> String {
        format!("cannot {verb} `{}`: {error}", self.name)
    }
    #[cfg(unix)]
    fn read(self, text: bool) -> Result<Value, String> {
        use std::io::Read;
        let mut file = self
            .dir
            .open(&self.relative)
            .map_err(|e| self.error("read", e))?;
        let size = file.metadata().map_err(|e| self.error("read", e))?.len();
        if size > STANDARD_READ as u64 {
            return Err(format!(
                "`{}` is {size} bytes, over the {STANDARD_READ}-byte limit for fs.readFile; read it in pieces or keep it out of the script",
                self.name
            ));
        }
        let mut bytes = Vec::with_capacity(size as usize);
        file.by_ref()
            .take((STANDARD_READ + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| self.error("read", e))?;
        if bytes.len() > STANDARD_READ {
            return Err(format!(
                "`{}` is {} bytes, over the {STANDARD_READ}-byte limit for fs.readFile; read it in pieces or keep it out of the script",
                self.name,
                bytes.len()
            ));
        }
        if text {
            String::from_utf8(bytes)
                .map(Value::Text)
                .map_err(|e| format!("cannot decode `{}` as UTF-8: {e}", self.name))
        } else {
            Ok(Value::Bytes(bytes))
        }
    }
    #[cfg(unix)]
    fn write(self, bytes: Vec<u8>) -> Result<Value, String> {
        self.dir
            .write(&self.relative, bytes)
            .map_err(|e| self.error("write", e))?;
        Ok(Value::Unit)
    }
    #[cfg(unix)]
    fn mkdir(self, recursive: bool) -> Result<Value, String> {
        if recursive {
            self.dir.create_dir_all(&self.relative)
        } else {
            self.dir.create_dir(&self.relative)
        }
        .map_err(|e| self.error("create", e))?;
        Ok(Value::Unit)
    }
    #[cfg(unix)]
    fn exists(self) -> Result<Value, String> {
        Ok(Value::Bool(
            self.dir.try_exists(&self.relative).unwrap_or(false),
        ))
    }
    #[cfg(unix)]
    fn stat(self, link: bool) -> Result<Value, String> {
        let m = if link {
            self.dir.symlink_metadata(&self.relative)
        } else {
            self.dir.metadata(&self.relative)
        }
        .map_err(|e| self.error("stat", e))?;
        Ok(Value::Stat(secure::info(m)))
    }
    #[cfg(unix)]
    fn readlink(self) -> Result<Value, String> {
        self.dir
            .read_link_contents(&self.relative)
            .map(|p| Value::Text(p.to_string_lossy().into_owned()))
            .map_err(|e| self.error("read link", e))
    }
    #[cfg(unix)]
    fn unlink(self) -> Result<Value, String> {
        self.dir
            .remove_file(&self.relative)
            .map_err(|e| self.error("remove", e))?;
        Ok(Value::Unit)
    }
    #[cfg(unix)]
    fn rmdir(self) -> Result<Value, String> {
        self.dir
            .remove_dir(&self.relative)
            .map_err(|e| self.error("remove", e))?;
        Ok(Value::Unit)
    }
    #[cfg(unix)]
    fn rename(self, to: Self) -> Result<Value, String> {
        self.dir
            .rename(&self.relative, &to.dir, &to.relative)
            .map_err(|e| self.error("rename", e))?;
        Ok(Value::Unit)
    }
    #[cfg(unix)]
    fn entries(self, types: bool) -> Result<Value, String> {
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
            let kind = entry.file_type().ok();
            let info = FileInfo {
                size: 0,
                mode: 0,
                modified: 0.0,
                accessed: 0.0,
                created: 0.0,
                file: false,
                directory: kind.is_some_and(|t| t.is_dir()),
                symlink: false,
            };
            entries.push((name, info));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Value::Entries(entries, types))
    }
}
#[cfg(not(unix))]
impl StandardGrant {
    fn read(self, _: bool) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn write(self, _: Vec<u8>) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn mkdir(self, _: bool) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn exists(self) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn stat(self, _: bool) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn readlink(self) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn unlink(self) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn rmdir(self) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn rename(self, _: Self) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
    fn entries(self, _: bool) -> Result<Value, String> {
        Err("native filesystem unavailable".into())
    }
}
native! {fn read_file(ctx,_this,args){let text=if js::is_nullish(args.get(1)){false}else{let encoding=if is_string(args.get(1)){js::to_string(ctx,args.get(1))?}else{strict_options(ctx,args.get(1),&["encoding"])?;let v=js::get(ctx,args.get(1),"encoding")?;if !is_string(v.raw()){return Err(js::type_error(ctx,"fs.readFile encoding must be utf8"))}js::to_string(ctx,v.raw())?};if !matches!(encoding.to_ascii_lowercase().as_str(),"utf8"|"utf-8"){return Err(js::type_error(ctx,"fs.readFile only supports UTF-8 text decoding"))}true};let grant=prepare_grant(ctx,args.get(0),false,false)?;standard_submit(ctx,move|_|grant.read(text))}}
native! {fn write_file(ctx,_this,args){let value=args.get(1);let bytes=if is_string(value){js::to_string(ctx,value)?.into_bytes()}else{let view=js::view(ctx,value).ok_or_else(||js::type_error(ctx,"fs.writeFile expects string or Uint8Array"))?;if view.kind!=Some(qjs::JSTypedArrayEnum_JS_TYPED_ARRAY_UINT8 as i32){return Err(js::type_error(ctx,"fs.writeFile expects string or Uint8Array"))}
// SAFETY: copy the buffer before any JS can detach it.
unsafe{js::bytes_of(ctx,value)}.ok_or_else(||js::type_error(ctx,"detached Uint8Array"))?.to_vec()};let grant=prepare_grant(ctx,args.get(0),true,false)?;if bytes.len()>STANDARD_WRITE{return Err(js::range_error(ctx,&format!("`{}` write is {} bytes, over the {STANDARD_WRITE}-byte fs.writeFile limit",grant.describe(),bytes.len())))}standard_submit(ctx,move|_|grant.write(bytes))}}
native! {fn standard_mkdir(ctx,_this,args){let recursive=bool_option(ctx,args.get(1),"recursive")?;let grant=prepare_grant(ctx,args.get(0),true,false)?;standard_submit(ctx,move|_|grant.mkdir(recursive))}}
native! {fn standard_exists(ctx,_this,args){let grant=prepare_grant(ctx,args.get(0),false,false)?;standard_submit(ctx,move|_|grant.exists())}}
native! {fn readdir(ctx,_this,args){let types=bool_option(ctx,args.get(1),"withFileTypes")?;let grant=prepare_grant(ctx,args.get(0),false,false)?;standard_submit(ctx,move|_|grant.entries(types))}}
native! {fn stat(ctx,_this,args){let grant=prepare_grant(ctx,args.get(0),false,false)?;standard_submit(ctx,move|_|grant.stat(false))}}
native! {fn lstat(ctx,_this,args){let grant=prepare_grant(ctx,args.get(0),false,true)?;standard_submit(ctx,move|_|grant.stat(true))}}
native! {fn readlink(ctx,_this,args){let grant=prepare_grant(ctx,args.get(0),false,true)?;standard_submit(ctx,move|_|grant.readlink())}}
native! {fn standard_unlink(ctx,_this,args){let grant=prepare_grant(ctx,args.get(0),true,true)?;standard_submit(ctx,move|_|grant.unlink())}}
native! {fn standard_rename(ctx,_this,args){let source=prepare_grant(ctx,args.get(0),true,true)?;let target=prepare_grant(ctx,args.get(1),true,true)?;standard_submit(ctx,move|_|source.rename(target))}}
native! {fn rmdir(ctx,_this,args){let grant=prepare_grant(ctx,args.get(0),true,true)?;standard_submit(ctx,move|_|grant.rmdir())}}
pub(crate) fn make_standard(ctx: Ctx) -> Raw {
    (|| -> R<V> {
        let obj = js::new_object(ctx)?;
        for (name, fun, len) in [
            ("readFile", read_file as js::Native, 2),
            ("writeFile", write_file, 2),
            ("readdir", readdir, 2),
            ("exists", standard_exists, 1),
            ("unlink", standard_unlink, 1),
            ("rmdir", rmdir, 1),
            ("mkdir", standard_mkdir, 2),
            ("stat", stat, 1),
            ("lstat", lstat, 1),
            ("rename", standard_rename, 2),
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
fn make(ctx: Ctx) -> Raw {
    (|| -> R<V> {
        let obj = js::new_object(ctx)?;
        for (name, fun, n) in [
            ("appDataDir", app_data_dir as js::Native, 0),
            ("readText", read_text, 1),
            ("writeText", write_text, 3),
            ("exists", exists, 1),
            ("mkdir", mkdir, 2),
            ("rename", rename, 2),
            ("remove", remove, 1),
        ] {
            let name_c = std::ffi::CString::new(name).unwrap();
            let f = js::function(ctx, &name_c, fun, n)?;
            js::define(ctx, obj.raw(), name, f, true)?;
        }
        Ok(obj)
    })()
    .map_or(qjs::JS_EXCEPTION, V::take)
}
fn available(ctx: Ctx) -> bool {
    // SAFETY: short projection of the live renderer state.
    cfg!(any(target_os = "linux", target_os = "macos"))
        && unsafe { (*crate::state::state(ctx)).dom.os().is_desktop() }
}
pub(crate) fn is_available(ctx: Ctx) -> Raw {
    let shared = crate::dom::shared(ctx);
    V::bool(
        available(ctx)
            && shared.system_data_dir.borrow().is_some()
            && shared.system_wake.borrow().is_some(),
    )
    .take()
}
pub(crate) const EXPORTS: &[Item] = &[
    (
        "FileSystem",
        Export::Value(make),
        Ts::value(
            "Capability-checked native UTF-8 files (16 MiB per file).",
            "FileSystem",
        ),
    ),
    (
        "isAvailable",
        Export::Value(is_available),
        Ts::value(
            "Whether this host supports secure native files and provides an app data directory.",
            "boolean",
        ),
    ),
];
pub(crate) const TYPES: &str = r#"export interface FileSystem {
 /** App-specific native data directory; requires an fs read grant. */
 appDataDir(): Promise<string>;
 /** Rejects missing files with ENOENT and invalid UTF-8 with EILSEQ. */
 readText(path: string): Promise<string>;
 /** Writes and syncs UTF-8. exclusive refuses to overwrite existing files. */
 writeText(path: string, text: string, options?: { exclusive?: boolean }): Promise<void>;
 exists(path: string): Promise<boolean>;
 mkdir(path: string, options?: { recursive?: boolean }): Promise<void>;
 /** Atomically replaces the destination when the filesystem permits it. */
 rename(from: string, to: string): Promise<void>;
 /** Removes one file; directories are rejected. */
 remove(path: string): Promise<void>;
}
"#;

#[cfg(unix)]
mod secure {
    use std::{
        ffi::CString,
        fs::File,
        os::fd::{AsRawFd, FromRawFd},
        path::{Component, Path},
    };

    use super::io;
    fn c(s: &std::ffi::OsStr) -> Result<CString, String> {
        use std::os::unix::ffi::OsStrExt;
        CString::new(s.as_bytes()).map_err(|_| "EINVAL: NUL in path".into())
    }
    fn last_error() -> String {
        io(std::io::Error::last_os_error())
    }
    fn root() -> Result<File, String> {
        File::open("/").map_err(io)
    }
    fn directory(parent: &File, name: &CString) -> Result<File, String> {
        // SAFETY: parent is live and name is NUL-terminated; the returned fd is owned.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            Err(last_error())
        } else {
            // SAFETY: openat returned a new owned descriptor above.
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn parent(path: &Path) -> Result<(File, CString), String> {
        if !path.is_absolute() {
            return Err("EINVAL: expected absolute path".into());
        }
        let name = c(path.file_name().ok_or("EINVAL: root is not a file")?)?;
        let mut dir = root()?;
        for part in path.parent().unwrap().components() {
            match part {
                Component::RootDir => (),
                Component::Normal(s) => dir = directory(&dir, &c(s)?)?,
                _ => return Err("EINVAL: unsafe path component".into()),
            }
        }
        Ok((dir, name))
    }
    pub(super) fn info(meta: cap_std::fs::Metadata) -> super::FileInfo {
        super::FileInfo {
            size: meta.len(),
            mode: {
                use cap_std::fs::MetadataExt;
                meta.mode()
            },
            accessed: meta
                .accessed()
                .ok()
                .and_then(|t| t.into_std().duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0.0, |t| t.as_secs_f64() * 1000.0),
            created: meta
                .created()
                .ok()
                .and_then(|t| t.into_std().duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0.0, |t| t.as_secs_f64() * 1000.0),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.into_std().duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0.0, |t| t.as_secs_f64() * 1000.0),
            file: meta.is_file(),
            directory: meta.is_dir(),
            symlink: meta.is_symlink(),
        }
    }
    pub(super) fn dir(path: &Path) -> Result<cap_std::fs::Dir, String> {
        let mut current = root()?;
        for part in path.components() {
            match part {
                Component::RootDir => (),
                Component::Normal(s) => current = directory(&current, &c(s)?)?,
                _ => return Err("EINVAL: invalid directory component".into()),
            }
        }
        Ok(cap_std::fs::Dir::from_std_file(current))
    }
    pub(super) fn open(p: &Path, write: bool, exclusive: bool) -> Result<File, String> {
        let (dir, name) = parent(p)?;
        let flags = libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if write {
                libc::O_WRONLY | libc::O_CREAT | if exclusive { libc::O_EXCL } else { 0 }
            } else {
                libc::O_RDONLY
            };
        // SAFETY: dir is live and name is NUL-terminated; mode is supplied for O_CREAT.
        let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags, 0o600) };
        if fd < 0 {
            return Err(last_error());
        }
        // SAFETY: openat returned this new owned descriptor and it is nonnegative.
        let f = unsafe { File::from_raw_fd(fd) };
        if !f.metadata().map_err(io)?.is_file() {
            return Err("EINVAL: expected regular file".into());
        }
        if write {
            f.set_len(0).map_err(io)?
        }
        Ok(f)
    }
    pub(super) fn mkdir(p: &Path, recursive: bool) -> Result<(), String> {
        if !recursive {
            let (d, n) = parent(p)?;
            // SAFETY: d is live and n is a NUL-terminated single path component.
            return if unsafe { libc::mkdirat(d.as_raw_fd(), n.as_ptr(), 0o700) } < 0 {
                Err(last_error())
            } else {
                Ok(())
            };
        }
        let mut d = root()?;
        for part in p.components() {
            match part {
                Component::RootDir => (),
                Component::Normal(s) => {
                    let n = c(s)?;
                    // SAFETY: d is live and n is a NUL-terminated single path component.
                    let result = unsafe { libc::mkdirat(d.as_raw_fd(), n.as_ptr(), 0o700) };
                    if result < 0
                        && std::io::Error::last_os_error().kind()
                            != std::io::ErrorKind::AlreadyExists
                    {
                        return Err(last_error());
                    }
                    d = directory(&d, &n)?
                }
                _ => return Err("EINVAL: unsafe path component".into()),
            }
        }
        Ok(())
    }
    pub(super) fn rename(a: &Path, b: &Path) -> Result<(), String> {
        let (ad, an) = parent(a)?;
        let (bd, bn) = parent(b)?;
        // SAFETY: both directory descriptors are live and names are NUL-terminated.
        if unsafe { libc::renameat(ad.as_raw_fd(), an.as_ptr(), bd.as_raw_fd(), bn.as_ptr()) } < 0 {
            Err(last_error())
        } else {
            Ok(())
        }
    }
    pub(super) fn remove(p: &Path) -> Result<(), String> {
        let (d, n) = parent(p)?;
        // SAFETY: d is live and n is NUL-terminated; flags only permit unlinking files.
        if unsafe { libc::unlinkat(d.as_raw_fd(), n.as_ptr(), 0) } < 0 {
            Err(last_error())
        } else {
            Ok(())
        }
    }
}
#[cfg(not(unix))]
mod secure {
    use std::{fs::File, path::Path};
    fn unavailable<T>() -> Result<T, String> {
        Err("ENOTSUP: secure native file operations are unavailable on this platform".into())
    }
    pub(super) fn metadata(_: &Path, _: bool) -> Result<super::FileInfo, String> {
        unavailable()
    }
    pub(super) fn entries(_: &Path) -> Result<Vec<(String, super::FileInfo)>, String> {
        unavailable()
    }
    pub(super) fn readlink(_: &Path) -> Result<String, String> {
        unavailable()
    }
    pub(super) fn remove_dir(_: &Path) -> Result<(), String> {
        unavailable()
    }
    pub(super) fn open(_: &Path, _: bool, _: bool) -> Result<File, String> {
        unavailable()
    }
    pub(super) fn mkdir(_: &Path, _: bool) -> Result<(), String> {
        unavailable()
    }
    pub(super) fn rename(_: &Path, _: &Path) -> Result<(), String> {
        unavailable()
    }
    pub(super) fn remove(_: &Path) -> Result<(), String> {
        unavailable()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn dir() -> PathBuf {
        let p = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .expect("set TMPDIR outside /tmp and the checkout")
            .join(format!(
                "nsh-fs-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn secure_files_and_symlinks() {
        use std::{
            io::{Read, Write},
            os::unix::fs::symlink,
        };
        let p = dir();
        let file = p.join("history.json");
        let mut f = secure::open(&file, true, true).unwrap();
        f.write_all(b"hello").unwrap();
        drop(f);
        assert!(secure::open(&file, true, true).is_err());
        let mut value = String::new();
        secure::open(&file, false, false)
            .unwrap()
            .read_to_string(&mut value)
            .unwrap();
        assert_eq!(value, "hello");
        let link = p.join("link");
        symlink(&file, &link).unwrap();
        assert!(secure::open(&link, false, false).is_err());
        let sub = p.join("sub");
        symlink(&p, &sub).unwrap();
        assert!(secure::open(&sub.join("history.json"), false, false).is_err());
        secure::mkdir(&p.join("nested/deeper"), true).unwrap();
        secure::rename(&file, &p.join("renamed")).unwrap();
        secure::remove(&p.join("renamed")).unwrap();
        std::fs::remove_dir_all(p).unwrap();
    }
}

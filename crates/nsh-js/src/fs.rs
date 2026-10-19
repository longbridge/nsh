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
#[derive(Clone)]
struct Config {
    dir: PathBuf,
    access: Access,
    wake: Arc<dyn Fn() + Send + Sync>,
}
struct FileInfo {
    size: u64,
    modified: f64,
    file: bool,
    directory: bool,
    symlink: bool,
}
enum Value {
    Bytes(Vec<u8>, Option<String>),
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
pub(crate) struct FileSystem {
    pending: RefCell<HashMap<u64, Deferred>>,
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
        if let Some(d) = d {
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
                    if let Ok(v) = js::dom_exception(ctx, &e, "FileSystemError") {
                        let code = e.split_once(':').map_or("EIO", |(code, _)| code);
                        if let Ok(code) = js::string(ctx, code)
                            && js::define(ctx, v.raw(), "code", code, true).is_err()
                        {
                            let _ = js::take_exception(ctx);
                        }
                        d.reject(ctx, v.raw())
                    }
                }
            }
        }
    }
    completed
}
fn buffer_value(ctx: Ctx, bytes: &[u8], encoding: Option<&str>) -> R<V> {
    let global = js::global(ctx);
    let buffer = js::get(ctx, global.raw(), "Buffer")?;
    let from = js::get(ctx, buffer.raw(), "from")?;
    let data = js::array_buffer(ctx, bytes)?;
    let value = js::call(ctx, from.raw(), buffer.raw(), &[data.raw()])?;
    if let Some(encoding) = encoding {
        let encoding = js::string(ctx, encoding)?;
        let to_string = js::get(ctx, value.raw(), "toString")?;
        js::call(ctx, to_string.raw(), value.raw(), &[encoding.raw()])
    } else {
        Ok(value)
    }
}
fn info_value(ctx: Ctx, info: FileInfo) -> R<V> {
    let obj = js::new_object(ctx)?;
    js::define(ctx, obj.raw(), "size", V::num(info.size as f64), true)?;
    js::define(ctx, obj.raw(), "mtimeMs", V::num(info.modified), true)?;
    for (name, value) in [
        ("isFile", info.file),
        ("isDirectory", info.directory),
        ("isSymbolicLink", info.symlink),
    ] {
        let method = js::closure(ctx, vec![], move |_, _, _| Ok(V::bool(value)))?;
        js::define(ctx, obj.raw(), name, method, true)?;
    }
    Ok(obj)
}
fn value_js(ctx: Ctx, value: Value) -> R<V> {
    match value {
        Value::Bytes(b, encoding) => buffer_value(ctx, &b, encoding.as_deref()),
        Value::Entries(entries, with_types) => js::new_array(
            ctx,
            entries
                .into_iter()
                .map(|(name, info)| {
                    if with_types {
                        let obj = info_value(ctx, info)?;
                        js::define(ctx, obj.raw(), "name", js::string(ctx, &name)?, true)?;
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
    let config = shared
        .system_data_dir
        .borrow()
        .clone()
        .zip(shared.system_wake.borrow().clone())
        .map(|(dir, wake)| Config {
            dir,
            access: shared.system_access.borrow().clone(),
            wake,
        });
    let Some(config) = config else {
        let e = js::dom_exception(
            ctx,
            "FileSystem is unavailable: this host has no native app data directory",
            "NotSupportedError",
        )?;
        d.reject(ctx, e.raw());
        return Ok(promise);
    };
    if state(ctx).pending.borrow().len() >= 64 {
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
    state(ctx).pending.borrow_mut().insert(id, d);
    let tx = state(ctx).tx.clone();
    let wake = config.wake.clone();
    let spawned = std::thread::Builder::new()
        .name("nsh-files".into())
        .spawn(move || {
            let result = job(config);
            let _ = tx.send(Completion { id, result });
            wake();
        });
    if let Err(e) = spawned {
        let d = state(ctx).pending.borrow_mut().remove(&id);
        if let Some(d) = d {
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
        .authorize_file(p, &c.dir, write)
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
fn bounded_bytes(ctx: Ctx, value: Raw) -> R<Vec<u8>> {
    // SAFETY: the bytes are inspected and copied before running any JS.
    let bytes = unsafe { js::bytes_of(ctx, value) }
        .ok_or_else(|| js::type_error(ctx, "writeFile expects string, Buffer or Uint8Array"))?;
    if bytes.len() > MAX_TEXT {
        return Err(js::range_error(ctx, "writeFile exceeds 16 MiB"));
    }
    Ok(bytes.to_vec())
}
fn is_string(value: Raw) -> bool {
    // SAFETY: the value is live; this only inspects its tag.
    unsafe { qjs::JS_IsString(value) }
}
fn encoding(ctx: Ctx, options: Raw) -> R<Option<String>> {
    if js::is_nullish(options) {
        return Ok(None);
    }
    if is_string(options) {
        return Ok(Some(js::to_string(ctx, options)?));
    }
    {
        let value = js::get(ctx, options, "encoding")?;
        if js::is_nullish(value.raw()) {
            Ok(None)
        } else {
            js::to_string(ctx, value.raw()).map(Some)
        }
    }
}
native! {fn read_file(ctx,_this,args){let p=path(ctx,args.get(0))?;let encoding=encoding(ctx,args.get(1))?;submit(ctx,move|c|{let p=authorize(&c,&p,false)?;let mut file=secure::open(&p,false,false)?;use std::io::Read;let mut bytes=Vec::new();file.by_ref().take((MAX_TEXT+1)as u64).read_to_end(&mut bytes).map_err(io)?;if bytes.len()>MAX_TEXT{return Err("EFBIG: file exceeds 16 MiB".into())}Ok(Value::Bytes(bytes,encoding))})}}
native! {fn write_file(ctx,_this,args){let p=path(ctx,args.get(0))?;let data=args.get(1);let options=args.get(2);let encoding=encoding(ctx,options)?;let bytes=if is_string(data){let global=js::global(ctx);let buffer=js::get(ctx,global.raw(),"Buffer")?;let from=js::get(ctx,buffer.raw(),"from")?;let enc=js::string(ctx,encoding.as_deref().unwrap_or("utf8"))?;let data=js::call(ctx,from.raw(),buffer.raw(),&[data,enc.raw()])?;bounded_bytes(ctx,data.raw())?}else{bounded_bytes(ctx,data)?};let flag=if js::is_object(options){js::opt_string(ctx,js::get(ctx,options,"flag")?.raw())?}else{None};if !matches!(flag.as_deref(),None|Some("w"|"wx")){return Err(js::type_error(ctx,"writeFile supports flags w and wx"))}let exclusive=flag.as_deref()==Some("wx");submit(ctx,move|c|{if bytes.len()>MAX_TEXT{return Err("EFBIG: file exceeds 16 MiB".into())}let p=authorize(&c,&p,true)?;let mut file=secure::open(&p,true,exclusive)?;use std::io::Write;file.write_all(&bytes).map_err(io)?;file.sync_all().map_err(io)?;Ok(Value::Unit)})}}
native! {fn access(ctx,_this,args){let p=path(ctx,args.get(0))?;let mode=if js::is_nullish(args.get(1)){0}else{js::to_string(ctx,args.get(1))?.parse::<u32>().map_err(|_|js::type_error(ctx,"access mode must be 0, 4, 2, or 6"))?};if mode&!6!=0{return Err(js::type_error(ctx,"access supports F_OK, R_OK and W_OK"))}submit(ctx,move|c|{let p=authorize(&c,&p,mode&2!=0)?;if mode&4!=0{authorize(&c,&p,false)?;}secure::check_access(&p,mode)?;Ok(Value::Unit)})}}
native! {fn readdir(ctx,_this,args){let p=path(ctx,args.get(0))?;let options=args.get(1);let types=if js::is_object(options){js::to_bool(ctx,js::get(ctx,options,"withFileTypes")?.raw())}else{false};submit(ctx,move|c|{let p=authorize(&c,&p,false)?;Ok(Value::Entries(secure::entries(&p)?,types))})}}
native! {fn stat(ctx,_this,args){let p=path(ctx,args.get(0))?;submit(ctx,move|c|{let p=authorize(&c,&p,false)?;Ok(Value::Stat(secure::metadata(&p,false)?))})}}
native! {fn lstat(ctx,_this,args){let p=path(ctx,args.get(0))?;submit(ctx,move|c|{authorize(&c,&p,false)?;let leaf=p.file_name().ok_or("EINVAL: lstat requires a file or directory name")?;let parent=p.parent().ok_or("EINVAL: lstat requires an absolute path")?;let parent=authorize(&c,parent,false)?;let p=parent.join(leaf);Ok(Value::Stat(secure::metadata(&p,true)?))})}}
native! {fn rmdir(ctx,_this,args){let p=path(ctx,args.get(0))?;submit(ctx,move|c|{let p=authorize(&c,&p,true)?;secure::remove_dir(&p)?;Ok(Value::Unit)})}}
pub(crate) fn make_standard(ctx: Ctx) -> Raw {
    (|| -> R<V> {
        let obj = js::new_object(ctx)?;
        for (name, fun, len) in [
            ("readFile", read_file as js::Native, 2),
            ("writeFile", write_file, 3),
            ("mkdir", mkdir, 2),
            ("rename", rename, 2),
            ("unlink", remove, 1),
            ("exists", exists, 1),
            ("access", access, 2),
            ("readdir", readdir, 2),
            ("stat", stat, 1),
            ("lstat", lstat, 1),
            ("rmdir", rmdir, 1),
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
        let constants = js::new_object(ctx)?;
        for (name, value) in [("F_OK", 0), ("R_OK", 4), ("W_OK", 2)] {
            js::define(ctx, constants.raw(), name, V::num(value as f64), true)?;
        }
        js::define(ctx, obj.raw(), "constants", constants, true)?;
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
    pub(super) fn check_access(path: &Path, mode: u32) -> Result<(), String> {
        let (d, name) = parent(path)?;
        // SAFETY: d is live, name is NUL-terminated, and mode contains only R_OK/W_OK.
        if unsafe {
            libc::faccessat(
                d.as_raw_fd(),
                name.as_ptr(),
                mode as i32,
                libc::AT_EACCESS | libc::AT_SYMLINK_NOFOLLOW,
            )
        } < 0
        {
            Err(last_error())
        } else {
            Ok(())
        }
    }
    fn info(meta: cap_std::fs::Metadata) -> super::FileInfo {
        super::FileInfo {
            size: meta.len(),
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
    fn dir(path: &Path) -> Result<cap_std::fs::Dir, String> {
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
    pub(super) fn metadata(path: &Path, link: bool) -> Result<super::FileInfo, String> {
        let (d, name) = parent(path)?;
        let d = cap_std::fs::Dir::from_std_file(d);
        let name = std::ffi::OsStr::new(name.to_str().map_err(|_| "EILSEQ: non-UTF8 filename")?);
        let metadata = if link {
            d.symlink_metadata(name)
        } else {
            d.metadata(name)
        }
        .map_err(io)?;
        Ok(info(metadata))
    }
    pub(super) fn entries(path: &Path) -> Result<Vec<(String, super::FileInfo)>, String> {
        let d = dir(path)?;
        let mut entries = Vec::new();
        let mut bytes = 0;
        for entry in d.entries().map_err(io)? {
            let entry = entry.map_err(io)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "EILSEQ: non-UTF8 filename")?;
            bytes += name.len() + 128;
            if bytes > super::MAX_TEXT {
                return Err("EFBIG: directory listing exceeds 16 MiB".into());
            }
            let metadata = entry.metadata().map_err(io)?;
            entries.push((name, info(metadata)));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }
    pub(super) fn remove_dir(path: &Path) -> Result<(), String> {
        let (d, name) = parent(path)?;
        let d = cap_std::fs::Dir::from_std_file(d);
        let name = std::ffi::OsStr::new(name.to_str().map_err(|_| "EILSEQ: non-UTF8 filename")?);
        d.remove_dir(name).map_err(io)
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
    pub(super) fn check_access(_: &Path, _: u32) -> Result<(), String> {
        unavailable()
    }
    pub(super) fn metadata(_: &Path, _: bool) -> Result<super::FileInfo, String> {
        unavailable()
    }
    pub(super) fn entries(_: &Path) -> Result<Vec<(String, super::FileInfo)>, String> {
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

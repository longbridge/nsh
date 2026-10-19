//! `fs/promises`: capability-checked files. A call checks its path against
//! `capabilities.fs` (a `TypeError` there) and its limits (a `RangeError`);
//! the disk work then runs off the engine thread, confined to the granted
//! root by a cap-std `Dir`, so a symlink inside a root cannot lead a later
//! operation outside it. Results settle as core tasks.

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use crate::ui::{
    ClassDef, Held, Instance, JsError, JsHost, JsIn, JsKind, JsObject, JsOwned, JsValue, Later,
    PropError, Quotas, Ret, System, TsTypes, class, module,
};

/// The largest file `readFile` returns.
#[cfg(not(target_arch = "wasm32"))]
const MAX_READ: usize = 64 * 1024 * 1024;
/// The most data one `writeFile` writes.
const MAX_WRITE: usize = 8 * 1024 * 1024;
/// Operations one VM may have outstanding.
const MAX_PENDING: usize = 1024;
/// File data one VM may hold in flight: data handed to `writeFile` until it
/// is written, and data read (contents and directory names) until JS takes
/// it.
const MAX_IN_FLIGHT: usize = 64 * 1024 * 1024;
/// The [`Quotas`] kind of outstanding operations.
const PENDING: &str = "fs.pending";
/// The [`Quotas`] kind of bytes in flight.
const IN_FLIGHT: &str = "fs.bytes";
/// What a read holds of the budget at a time past the size the file had
/// when it was opened.
#[cfg(not(target_arch = "wasm32"))]
const GROWTH: usize = 1024 * 1024;
/// The most entries and name bytes one `readdir` lists.
#[cfg(not(target_arch = "wasm32"))]
const MAX_ENTRIES: usize = 10_000;
#[cfg(not(target_arch = "wasm32"))]
const MAX_NAME_BYTES: usize = 1024 * 1024;

/// Capability-checked files.
pub(crate) struct Fs;

/// Promise-based files under the roots `capabilities.fs` grants. Failures
/// carry Node's `code` (`ENOENT`, `EEXIST`, ...), `syscall` and `path`; a
/// path no grant covers throws a `TypeError` with `code` `EACCES`.
#[module(flat, default)]
impl Fs {
    /// Reads at most 64 MiB. Omitted encoding returns Uint8Array. A runtime
    /// holds at most 64 MiB of file data in flight. Like every function
    /// here, it fails with an `Error & { code?: string; syscall?: string;
    /// path?: string }` (`error.code === "ENOENT"`).
    #[overload(fn(path: String) -> Vec<u8>)]
    #[overload(fn(path: String, encoding: Encoding) -> String)]
    pub(crate) fn read_file(
        path: String,
        encoding: Option<Encoding>,
        sys: System,
    ) -> Result<Later<Holding<FileData>>, JsError> {
        let grant = grant(&sys, &path, false, false)?;
        let pending = pending(&sys)?;
        let text = encoding.is_some();
        Ok(Later::new(async move {
            let _pending = pending;
            grant.read(&sys.quotas, text).await
        }))
    }

    /// Writes UTF-8 or Uint8Array bytes, replacing an existing regular
    /// file; at most 8 MiB.
    pub(crate) fn write_file(
        path: String,
        data: FileData,
        sys: System,
    ) -> Result<Later<()>, JsError> {
        let grant = grant(&sys, &path, true, false)?;
        let bytes = match data {
            FileData::Text(text) => text.into_bytes(),
            FileData::Bytes(bytes) => bytes,
        };
        if bytes.len() > MAX_WRITE {
            return Err(JsError::Range(format!(
                "`{}` write is {} bytes, over the {MAX_WRITE}-byte fs.writeFile limit",
                grant.name(),
                bytes.len()
            )));
        }
        let pending = pending(&sys)?;
        let held = sys
            .quotas
            .reserve(IN_FLIGHT, bytes.len(), MAX_IN_FLIGHT)
            .ok_or_else(|| {
                JsError::Range(format!(
                    "fs exceeded the per-runtime limit of {MAX_IN_FLIGHT} bytes of file data in flight"
                ))
            })?;
        Ok(Later::new(async move {
            let _held = (pending, held);
            grant.write(bytes).await
        }))
    }

    /// Lists a directory's names, sorted, or its entries with
    /// `{ withFileTypes: true }`; at most 10 000 entries.
    #[overload(fn(path: String, options: WithFileTypes) -> Vec<Instance<Dirent>>)]
    #[overload(fn(path: String, options: Option<ReaddirOptions>) -> Vec<String>)]
    pub(crate) fn readdir(
        path: String,
        options: Option<ReaddirOptions>,
        sys: System,
    ) -> Result<Later<Holding<Entries>>, JsError> {
        let types = options.and_then(|o| o.with_file_types).unwrap_or(false);
        let grant = grant(&sys, &path, false, false)?;
        let pending = pending(&sys)?;
        Ok(Later::new(async move {
            let _pending = pending;
            grant.entries(&sys.quotas, types).await
        }))
    }

    /// Whether the entry exists; a symlink exists even when its target is
    /// missing.
    pub(crate) fn exists(path: String, sys: System) -> Result<Later<bool>, JsError> {
        let grant = grant(&sys, &path, false, true)?;
        start(&sys, grant.exists())
    }

    /// Removes a file or a symlink itself, never its target.
    pub(crate) fn unlink(path: String, sys: System) -> Result<Later<()>, JsError> {
        let grant = grant(&sys, &path, true, true)?;
        start(&sys, grant.unlink())
    }

    /// Removes one empty directory.
    pub(crate) fn rmdir(path: String, sys: System) -> Result<Later<()>, JsError> {
        let grant = grant(&sys, &path, true, true)?;
        start(&sys, grant.rmdir())
    }

    /// Creates a directory (and its missing parents with
    /// `{ recursive: true }`).
    pub(crate) fn mkdir(
        path: String,
        options: Option<MakeDirectoryOptions>,
        sys: System,
    ) -> Result<Later<()>, JsError> {
        let recursive = options.and_then(|o| o.recursive).unwrap_or(false);
        let grant = grant(&sys, &path, true, false)?;
        start(&sys, grant.mkdir(recursive))
    }

    /// The entry's metadata, following a symlink.
    pub(crate) fn stat(path: String, sys: System) -> Result<Later<Stats>, JsError> {
        let grant = grant(&sys, &path, false, false)?;
        start(&sys, grant.stat(false))
    }

    /// The entry's metadata; a symlink's own.
    pub(crate) fn lstat(path: String, sys: System) -> Result<Later<Stats>, JsError> {
        let grant = grant(&sys, &path, false, true)?;
        start(&sys, grant.stat(true))
    }

    /// Moves an entry (a symlink itself, not its target).
    pub(crate) fn rename(from: String, to: String, sys: System) -> Result<Later<()>, JsError> {
        let source = grant(&sys, &from, true, true)?;
        let target = grant(&sys, &to, true, true)?;
        start(&sys, source.rename(target))
    }

    /// A symlink's target, as it is written.
    pub(crate) fn readlink(path: String, sys: System) -> Result<Later<String>, JsError> {
        let grant = grant(&sys, &path, false, true)?;
        start(&sys, grant.readlink())
    }
}

/// One outstanding operation of the VM's at most [`MAX_PENDING`].
fn pending(sys: &System) -> Result<Held, JsError> {
    sys.quotas.take(PENDING, MAX_PENDING).ok_or_else(|| {
        JsError::Range(format!(
            "fs exceeded the per-runtime outstanding host task limit of {MAX_PENDING}"
        ))
    })
}

/// `work` as an outstanding operation.
fn start<T: Ret + 'static>(
    sys: &System,
    work: impl Future<Output = Result<T, JsError>> + 'static,
) -> Result<Later<T>, JsError> {
    let pending = pending(sys)?;
    Ok(Later::new(async move {
        let _pending = pending;
        work.await
    }))
}

/// `readFile`'s encoding: UTF-8 text, as `"utf8"` or `{ encoding: "utf8" }`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Encoding;

impl JsValue for Encoding {
    fn ts_type(_types: &mut TsTypes) -> String {
        r#""utf8" | { encoding: "utf8" }"#.into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        let name = match value.kind() {
            JsKind::String => value.as_string(),
            JsKind::Object => {
                if let Some(key) = value.keys()?.into_iter().find(|k| k != "encoding") {
                    return Err(PropError(format!("unknown field `{key}`")));
                }
                value.field("encoding")?.get().as_string()
            }
            _ => None,
        };
        match name {
            Some(name) if matches!(name.to_ascii_lowercase().as_str(), "utf8" | "utf-8") => {
                Ok(Encoding)
            }
            _ => Err(value.expected(r#""utf8" (only UTF-8 text decoding is supported)"#)),
        }
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        Ok(JsOwned::new(host, host.new_string("utf8")?))
    }
}

/// A file's contents: UTF-8 text or bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileData {
    Text(String),
    Bytes(Vec<u8>),
}

impl JsValue for FileData {
    fn ts_type(_types: &mut TsTypes) -> String {
        "string | Uint8Array".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        if let Some(text) = value.as_string() {
            return Ok(FileData::Text(text));
        }
        value
            .as_bytes()
            .map(FileData::Bytes)
            .ok_or_else(|| value.expected("a string or Uint8Array"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            FileData::Text(text) => text.to_js(host),
            FileData::Bytes(bytes) => bytes.to_js(host),
        }
    }
}

/// `mkdir`'s options.
#[derive(Debug, Clone, JsObject)]
#[prop(exact)]
pub(crate) struct MakeDirectoryOptions {
    /// Creates the missing parents too.
    recursive: Option<bool>,
}

/// `readdir`'s options.
#[derive(Debug, Clone, JsObject)]
#[prop(exact)]
pub(crate) struct ReaddirOptions {
    /// Lists `Dirent`s instead of names.
    with_file_types: Option<bool>,
}

/// `readdir`'s options asking for `Dirent`s: the type of an overload.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WithFileTypes;

impl JsValue for WithFileTypes {
    fn ts_type(_types: &mut TsTypes) -> String {
        "{ withFileTypes: true }".into()
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        match ReaddirOptions::from_js(value)?.with_file_types {
            Some(true) => Ok(WithFileTypes),
            _ => Err(value.expected("{ withFileTypes: true }")),
        }
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        ReaddirOptions {
            with_file_types: Some(true),
        }
        .to_js(host)
    }
}

/// A result holding part of the VM's in-flight budget until JS takes it.
pub(crate) struct Holding<T> {
    value: T,
    _held: Vec<Held>,
}

impl<T: Ret> Ret for Holding<T> {
    fn ts_type(types: &mut TsTypes) -> String {
        T::ts_type(types)
    }

    fn into_js<'a>(self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        // The budget is held until the value is JS's.
        let Holding { value, _held } = self;
        value.into_js(host)
    }

    fn classes(out: &mut Vec<&'static ClassDef>) {
        T::classes(out);
    }
}

/// What `readdir` lists.
// WebAssembly lists nothing.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) enum Entries {
    Names(Vec<String>),
    Typed(Vec<Instance<Dirent>>),
}

impl Ret for Entries {
    fn ts_type(types: &mut TsTypes) -> String {
        format!(
            "{} | {}",
            <Vec<String> as Ret>::ts_type(types),
            <Vec<Instance<Dirent>> as Ret>::ts_type(types)
        )
    }

    fn into_js<'a>(self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        match self {
            Entries::Names(names) => names.into_js(host),
            Entries::Typed(entries) => entries.into_js(host),
        }
    }

    fn classes(out: &mut Vec<&'static ClassDef>) {
        <Vec<Instance<Dirent>> as Ret>::classes(out);
    }
}

/// An entry of a directory `readdir` lists with `{ withFileTypes: true }`.
#[class(interface)]
pub(crate) struct Dirent {
    name: String,
    directory: bool,
}

#[class]
impl Dirent {
    /// The entry's name.
    #[getter]
    pub(crate) fn name(&self) -> String {
        self.name.clone()
    }

    /// Whether it is a directory.
    pub(crate) fn is_directory(&self) -> bool {
        self.directory
    }
}

/// What `stat` and `lstat` tell of an entry.
#[class(interface)]
#[derive(Debug, Clone)]
pub(crate) struct Stats {
    size: f64,
    mode: u32,
    mtime_ms: f64,
    atime_ms: f64,
    birthtime_ms: f64,
    file: bool,
    directory: bool,
    symlink: bool,
}

/// Milliseconds since the Unix epoch as a time.
fn time(millis: f64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(millis.max(0.0) / 1000.0)
}

#[class]
impl Stats {
    /// Its size in bytes.
    #[getter]
    pub(crate) fn size(&self) -> f64 {
        self.size
    }

    /// Its type and permission bits.
    #[getter]
    pub(crate) fn mode(&self) -> u32 {
        self.mode
    }

    /// When it was last modified, in milliseconds since the Unix epoch.
    #[getter]
    pub(crate) fn mtime_ms(&self) -> f64 {
        self.mtime_ms
    }

    /// When it was last accessed, in milliseconds since the Unix epoch.
    #[getter]
    pub(crate) fn atime_ms(&self) -> f64 {
        self.atime_ms
    }

    /// When it was created, in milliseconds since the Unix epoch.
    #[getter]
    pub(crate) fn birthtime_ms(&self) -> f64 {
        self.birthtime_ms
    }

    /// When it was last modified.
    #[getter]
    pub(crate) fn mtime(&self) -> std::time::SystemTime {
        time(self.mtime_ms)
    }

    /// When it was last accessed.
    #[getter]
    pub(crate) fn atime(&self) -> std::time::SystemTime {
        time(self.atime_ms)
    }

    /// When it was created.
    #[getter]
    pub(crate) fn birthtime(&self) -> std::time::SystemTime {
        time(self.birthtime_ms)
    }

    /// Whether it is a regular file.
    pub(crate) fn is_file(&self) -> bool {
        self.file
    }

    /// Whether it is a directory.
    pub(crate) fn is_directory(&self) -> bool {
        self.directory
    }

    /// Whether it is a symlink (only `lstat` tells).
    pub(crate) fn is_symbolic_link(&self) -> bool {
        self.symlink
    }
}

/// `path` without `.` and `..`, resolved by its text.
#[cfg(not(target_arch = "wasm32"))]
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

/// The canonical form of the nearest existing ancestor, with the rest
/// appended.
#[cfg(not(target_arch = "wasm32"))]
fn resolved(path: &Path) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        if let Ok(mut real) = crate::access::canonicalize(&current) {
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

/// A checked path: the granted root, opened, and the path within it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct Grant {
    /// The root, opened; `None`: a read root that does not exist (yet), in
    /// which every entry is missing.
    dir: Option<cap_std::fs::Dir>,
    relative: PathBuf,
    name: String,
    /// The path as the call gave it (an error's `path`).
    path: String,
}

/// WebAssembly has no files: no path is ever granted.
#[cfg(target_arch = "wasm32")]
pub(crate) enum Grant {}

/// Checks `path` against the `capabilities.fs` roots of a read or a write.
/// `link` operations act on the last component itself (a symlink there is
/// not followed: `lstat`, `readlink`, `exists`, `unlink`, `rename`,
/// `rmdir`), so only its parent is resolved.
fn grant(sys: &System, path: &str, write: bool, link: bool) -> Result<Grant, JsError> {
    let access = &sys.access;
    let roots = if write {
        &access.declared.fs.write
    } else {
        &access.declared.fs.read
    };
    let available = cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    )) && sys.os.is_desktop();
    if !roots.is_empty() && !available {
        return Err(
            JsError::Type("native filesystem unavailable on this platform".into())
                .with_code("ENOSYS"),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let given = path;
        let data = sys.data_dir.clone().unwrap_or_default();
        let paths = &sys.paths;
        let path = PathBuf::from(path);
        let requested = if path.to_string_lossy().starts_with("${") {
            paths
                .resolve_root(&path.to_string_lossy(), &data)
                .map_err(JsError::Type)?
        } else {
            path
        };
        // A read root that does not exist yet (`${dataDir}` before the app
        // writes): its entries are missing, unless another root has them.
        let mut missing = None;
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
                if link && requested.is_absolute() && resolved(&requested).is_ok_and(|p| p == root)
                {
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
            // A granted root belongs to the app: a write may create it.
            if write && !root.exists() && std::fs::create_dir_all(&root).is_err() {
                continue;
            }
            let (relative, name) = if relative.as_os_str().is_empty() {
                (PathBuf::from("."), ".".to_owned())
            } else {
                (relative.to_owned(), relative.display().to_string())
            };
            let dir = match confined::open_root(&root) {
                Ok(dir) => dir,
                Err(e) if !write && e.kind() == std::io::ErrorKind::NotFound => {
                    missing.get_or_insert(Grant {
                        dir: None,
                        relative,
                        name,
                        path: given.to_owned(),
                    });
                    continue;
                }
                Err(_) => continue,
            };
            return Ok(Grant {
                dir: Some(dir),
                relative,
                name,
                path: given.to_owned(),
            });
        }
        if let Some(grant) = missing {
            return Ok(grant);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = link;
    let what = if write { "write" } else { "read" };
    Err(JsError::Type(format!(
        "filesystem {what} is not granted by capabilities.fs.{what}"
    ))
    .with_code("EACCES")
    .with("path", path))
}

/// "cannot read `name`: the runtime already holds ... in flight".
#[cfg(not(target_arch = "wasm32"))]
fn exceeded(name: &str) -> JsError {
    JsError::Error(format!(
        "cannot read `{name}`: the runtime already holds {MAX_IN_FLIGHT} bytes of file data in flight"
    ))
}

/// Runs `work` off the engine thread (on the tasks' runtime's blocking
/// threads, else a thread of its own).
#[cfg(not(target_arch = "wasm32"))]
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, JsError> + Send + 'static,
) -> Result<T, JsError> {
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        return runtime
            .spawn_blocking(work)
            .await
            .map_err(|e| JsError::Error(e.to_string()))?;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("nsh-files".into())
        .spawn(move || {
            let _ = tx.send(work());
        })
        .map_err(|e| JsError::Error(e.to_string()))?;
    rx.await.map_err(|e| JsError::Error(e.to_string()))?
}

#[cfg(not(target_arch = "wasm32"))]
impl Grant {
    fn name(&self) -> &str {
        &self.name
    }

    /// The opened root: `NotFound` for a root that does not exist.
    fn dir(&self) -> std::io::Result<&cap_std::fs::Dir> {
        self.dir
            .as_ref()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
    }

    /// I/O error `error` of `syscall` as Node's: the message names the
    /// entry, `code`, `syscall` and `path` tell it apart.
    fn error(&self, verb: &str, syscall: &'static str, error: std::io::Error) -> JsError {
        let code = crate::ui::io_code(&error);
        let thrown = JsError::Error(format!("cannot {verb} `{}`: {error}", self.name));
        match code {
            Some(code) => thrown.with_code(code),
            None => thrown,
        }
        .with("syscall", syscall)
        .with("path", self.path.as_str())
    }

    async fn read(self, quotas: &Quotas, text: bool) -> Result<Holding<FileData>, JsError> {
        let name = self.name.clone();
        let too_big = move |len: usize| {
            JsError::Error(format!(
                "`{name}` is {len} bytes, over the {MAX_READ}-byte limit for fs.readFile; read it in pieces or keep it out of the script"
            ))
            .with_code("ERR_FS_FILE_TOO_LARGE")
        };
        let (grant, file, size) = blocking(move || {
            let file = self
                .dir()
                .and_then(|dir| confined::open_file(dir, &self.relative, false))
                .map_err(|e| self.error("read", "open", e))?;
            let size = file
                .metadata()
                .map_err(|e| self.error("read", "fstat", e))?
                .len();
            Ok((self, file, size))
        })
        .await?;
        let size = usize::try_from(size).unwrap_or(usize::MAX);
        if size > MAX_READ {
            return Err(too_big(size));
        }
        let mut held = vec![
            quotas
                .reserve(IN_FLIGHT, size, MAX_IN_FLIGHT)
                .ok_or_else(|| exceeded(&grant.name))?,
        ];
        let mut reserved = size;
        // The file may grow while it is read (or say 0, as `/proc` does):
        // what is read never goes past what is held by more than the byte
        // that tells growth from the end, and more is held a chunk at a time.
        let grant = std::sync::Arc::new(grant);
        let mut bytes = Vec::with_capacity(size);
        let mut file = file;
        loop {
            let want = reserved - bytes.len() + 1;
            let reader = grant.clone();
            let (back, read, more) = blocking(move || {
                use std::io::Read;
                let mut bytes = bytes;
                let before = bytes.len();
                file.by_ref()
                    .take(want as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|e| reader.error("read", "read", e))?;
                let more = bytes.len() - before == want;
                Ok((file, bytes, more))
            })
            .await?;
            (file, bytes) = (back, read);
            if !more {
                break;
            }
            if bytes.len() > MAX_READ {
                return Err(too_big(bytes.len()));
            }
            let chunk = GROWTH.min(MAX_READ + 1 - reserved);
            held.push(
                quotas
                    .reserve(IN_FLIGHT, chunk, MAX_IN_FLIGHT)
                    .ok_or_else(|| exceeded(&grant.name))?,
            );
            reserved += chunk;
        }
        if bytes.len() > MAX_READ {
            return Err(too_big(bytes.len()));
        }
        let value = if text {
            let decoder = grant.clone();
            // Valid UTF-8 keeps its length, and so what it holds of the
            // budget.
            blocking(move || {
                String::from_utf8(bytes).map(FileData::Text).map_err(|e| {
                    JsError::Error(format!("cannot decode `{}` as UTF-8: {e}", decoder.name))
                        .with_code("EILSEQ")
                        .with("path", decoder.path.as_str())
                })
            })
            .await?
        } else {
            FileData::Bytes(bytes)
        };
        let len = match &value {
            FileData::Text(text) => text.len(),
            FileData::Bytes(bytes) => bytes.len(),
        };
        drop(held);
        // At most what was held a moment ago: it fits.
        let held: Vec<Held> = quotas
            .reserve(IN_FLIGHT, len, MAX_IN_FLIGHT)
            .into_iter()
            .collect();
        Ok(Holding { value, _held: held })
    }

    async fn write(self, bytes: Vec<u8>) -> Result<(), JsError> {
        blocking(move || {
            use std::io::Write;
            let mut file = self
                .dir()
                .and_then(|dir| confined::open_file(dir, &self.relative, true))
                .map_err(|e| self.error("write", "open", e))?;
            file.set_len(0)
                .map_err(|e| self.error("write", "ftruncate", e))?;
            file.write_all(&bytes)
                .map_err(|e| self.error("write", "write", e))
        })
        .await
    }

    async fn mkdir(self, recursive: bool) -> Result<(), JsError> {
        blocking(move || {
            let dir = self.dir();
            if recursive {
                dir.and_then(|dir| dir.create_dir_all(&self.relative))
            } else {
                dir.and_then(|dir| dir.create_dir(&self.relative))
            }
            .map_err(|e| self.error("create", "mkdir", e))
        })
        .await
    }

    /// Whether the entry exists; a symlink exists even when its target does
    /// not.
    async fn exists(self) -> Result<bool, JsError> {
        blocking(move || {
            Ok(self
                .dir()
                .is_ok_and(|dir| dir.symlink_metadata(&self.relative).is_ok()))
        })
        .await
    }

    async fn stat(self, link: bool) -> Result<Stats, JsError> {
        blocking(move || {
            let dir = self.dir();
            if link {
                dir.and_then(|dir| dir.symlink_metadata(&self.relative))
            } else {
                dir.and_then(|dir| dir.metadata(&self.relative))
            }
            .map(confined::stats)
            .map_err(|e| self.error("stat", if link { "lstat" } else { "stat" }, e))
        })
        .await
    }

    async fn readlink(self) -> Result<String, JsError> {
        blocking(move || {
            self.dir()
                .and_then(|dir| dir.read_link_contents(&self.relative))
                .map(|p| p.to_string_lossy().into_owned())
                .map_err(|e| self.error("read link", "readlink", e))
        })
        .await
    }

    async fn unlink(self) -> Result<(), JsError> {
        blocking(move || {
            self.dir()
                .and_then(|dir| dir.remove_file(&self.relative))
                .map_err(|e| self.error("remove", "unlink", e))
        })
        .await
    }

    async fn rmdir(self) -> Result<(), JsError> {
        blocking(move || {
            self.dir()
                .and_then(|dir| dir.remove_dir(&self.relative))
                .map_err(|e| self.error("remove", "rmdir", e))
        })
        .await
    }

    async fn rename(self, to: Self) -> Result<(), JsError> {
        blocking(move || {
            self.dir()
                .and_then(|dir| dir.rename(&self.relative, to.dir()?, &to.relative))
                .map_err(|e| self.error("rename", "rename", e))
        })
        .await
    }

    async fn entries(self, quotas: &Quotas, types: bool) -> Result<Holding<Entries>, JsError> {
        let name = self.name.clone();
        let (entries, name_bytes) = blocking(move || {
            let mut entries = Vec::new();
            let mut name_bytes = 0;
            for entry in self
                .dir()
                .and_then(|dir| dir.read_dir(&self.relative))
                .map_err(|e| self.error("list", "scandir", e))?
            {
                let entry = entry.map_err(|e| self.error("list", "scandir", e))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                name_bytes += name.len();
                if entries.len() == MAX_ENTRIES || name_bytes > MAX_NAME_BYTES {
                    return Err(JsError::Error(format!(
                        "cannot list `{}`: directory exceeded the {MAX_ENTRIES}-entry or {MAX_NAME_BYTES}-name-byte fs.readdir limit",
                        self.name
                    )));
                }
                let directory = entry.file_type().is_ok_and(|t| t.is_dir());
                entries.push((name, directory));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Ok((entries, name_bytes))
        })
        .await?;
        let held = quotas
            .reserve(IN_FLIGHT, name_bytes, MAX_IN_FLIGHT)
            .ok_or_else(|| exceeded(&name))?;
        let value = if types {
            Entries::Typed(
                entries
                    .into_iter()
                    .map(|(name, directory)| Instance::new(Dirent { name, directory }))
                    .collect(),
            )
        } else {
            Entries::Names(entries.into_iter().map(|(name, _)| name).collect())
        };
        Ok(Holding {
            value,
            _held: vec![held],
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Grant {
    fn name(&self) -> &str {
        match *self {}
    }

    async fn read(self, _: &Quotas, _: bool) -> Result<Holding<FileData>, JsError> {
        match self {}
    }

    async fn write(self, _: Vec<u8>) -> Result<(), JsError> {
        match self {}
    }

    async fn mkdir(self, _: bool) -> Result<(), JsError> {
        match self {}
    }

    async fn exists(self) -> Result<bool, JsError> {
        match self {}
    }

    async fn stat(self, _: bool) -> Result<Stats, JsError> {
        match self {}
    }

    async fn readlink(self) -> Result<String, JsError> {
        match self {}
    }

    async fn unlink(self) -> Result<(), JsError> {
        match self {}
    }

    async fn rmdir(self) -> Result<(), JsError> {
        match self {}
    }

    async fn rename(self, _: Self) -> Result<(), JsError> {
        match self {}
    }

    async fn entries(self, _: &Quotas, _: bool) -> Result<Holding<Entries>, JsError> {
        match self {}
    }
}

/// Opening granted roots and the files in them.
#[cfg(not(target_arch = "wasm32"))]
mod confined {
    use std::{io, path::Path};

    use cap_std::fs::{Dir, File, Metadata, OpenOptions};

    /// How a root's ancestors are opened: for search only where the OS has
    /// such a mode (`O_PATH` on Linux and Android, `O_SEARCH` on Apple's),
    /// else for reading.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const SEARCH: libc::c_int = libc::O_PATH;
    #[cfg(target_vendor = "apple")]
    const SEARCH: libc::c_int = libc::O_SEARCH;
    #[cfg(all(
        unix,
        not(any(target_os = "linux", target_os = "android", target_vendor = "apple"))
    ))]
    const SEARCH: libc::c_int = libc::O_RDONLY;

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
        let parts: Vec<_> = path.components().collect();
        for (i, part) in parts.iter().enumerate() {
            // An ancestor is only passed through: opened for search, it
            // needs no read permission (a home directory may be 0711).
            let through = i + 1 < parts.len();
            match part {
                Component::RootDir => (),
                Component::Normal(name) => {
                    let name = CString::new(name.as_bytes())
                        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                    let open = |access: libc::c_int| {
                        // SAFETY: current is a live directory and name is
                        // NUL-terminated; the descriptor returned is owned
                        // below.
                        unsafe {
                            libc::openat(
                                current.as_raw_fd(),
                                name.as_ptr(),
                                access | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                            )
                        }
                    };
                    let mut fd = if through {
                        open(SEARCH)
                    } else {
                        open(libc::O_RDONLY)
                    };
                    // A kernel without the search mode (macOS before 13)
                    // refuses it: read the directory then.
                    if fd < 0 && through && SEARCH != libc::O_RDONLY {
                        fd = open(libc::O_RDONLY);
                    }
                    if fd < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    // SAFETY: openat returned a new descriptor nothing else
                    // owns.
                    current = unsafe { std::fs::File::from_raw_fd(fd) };
                }
                _ => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
            }
        }
        Ok(Dir::from_std_file(current))
    }

    /// Opens a canonical root. cap-std confines every later operation to
    /// it; the root is the path the grant check resolved, opened through
    /// the ambient filesystem once.
    #[cfg(not(unix))]
    pub(super) fn open_root(path: &Path) -> io::Result<Dir> {
        Dir::open_ambient_dir(path, cap_std::ambient_authority())
    }

    /// Opens a regular file to read or to write (created when missing). On
    /// Unix it is opened non-blocking, so a FIFO or device cannot stall the
    /// worker; anything but a regular file is refused before a byte moves.
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
        let meta = file.metadata()?;
        if !meta.is_file() {
            // Node's codes: a directory is EISDIR, a FIFO or device EINVAL.
            let kind = if meta.is_dir() {
                io::ErrorKind::IsADirectory
            } else {
                io::ErrorKind::InvalidInput
            };
            return Err(io::Error::new(kind, "not a regular file"));
        }
        Ok(file)
    }

    pub(super) fn stats(meta: Metadata) -> super::Stats {
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
        // Node's modes on Windows: the type bits, and write bits unless
        // read-only.
        #[cfg(not(unix))]
        let mode = if meta.is_dir() { 0o040000 } else { 0o100000 }
            | if meta.permissions().readonly() {
                0o444
            } else {
                0o666
            };
        super::Stats {
            size: meta.len() as f64,
            mode,
            atime_ms: millis(meta.accessed()),
            birthtime_ms: millis(meta.created()),
            mtime_ms: millis(meta.modified()),
            file: meta.is_file(),
            directory: meta.is_dir(),
            symlink: meta.is_symlink(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::{Registry, declarations};

    #[test]
    fn the_declarations_keep_node_s_overloads_and_shapes() {
        let mut registry = Registry::runtime("fs/promises");
        registry.module::<super::Fs>();
        let d = declarations(&registry, &[]);
        for line in [
            "export declare function readFile(path: string): Promise<Uint8Array>;",
            r#"export declare function readFile(path: string, encoding: "utf8" | { encoding: "utf8" }): Promise<string>;"#,
            "export declare function writeFile(path: string, data: string | Uint8Array): Promise<void>;",
            "export declare function readdir(path: string, options: { withFileTypes: true }): Promise<Dirent[]>;",
            "export declare function readdir(path: string, options?: ReaddirOptions | null): Promise<(string)[]>;",
            "export declare function mkdir(path: string, options?: MakeDirectoryOptions | null): Promise<void>;",
            "export declare function rename(from: string, to: string): Promise<void>;",
            "export declare function exists(path: string): Promise<boolean>;",
            "export declare function lstat(path: string): Promise<Stats>;",
            "export interface Stats {",
            "  readonly mtime: Date;",
            "  readonly mtimeMs: number;",
            "  isSymbolicLink(): boolean;",
            "export interface Dirent {",
            "  readonly name: string;",
            "export default defaultExport;",
        ] {
            assert!(d.contains(line), "{line}\n{d}");
        }
        assert!(!d.contains("declare class"), "{d}");
    }
}

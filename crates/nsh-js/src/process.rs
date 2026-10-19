//! Desktop subprocesses owned by one VM. Pipe readers never enter QuickJS;
//! they send bounded events and wake the engine, which delivers them in a JS turn.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{CStr, CString, c_int},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rquickjs::qjs;

use crate::{
    dom,
    modules::{Export, Item, Ts},
    reactive::{self, arg},
};

type Value = qjs::JSValue;
type Context = qjs::JSContext;
type Wake = Arc<dyn Fn() + Send + Sync>;
const U: Value = qjs::JS_UNDEFINED;
const QUEUE_BYTES: usize = 8 * 1024 * 1024;
const MAX_CHILDREN: usize = 32;

enum Input {
    Write(Vec<u8>),
    Close,
    Kill,
}
enum Event {
    Output(u32, bool, String),
    Exit(u32, Option<i32>, Option<i32>, Option<String>),
}

struct Entry {
    input: mpsc::Sender<Input>,
    worker: Option<JoinHandle<()>>,
    callbacks: [Vec<Value>; 3],
    resolve: Value,
    reject: Value,
    stdout: String,
    stderr: String,
    capture: bool,
    max_output: usize,
    handle: Weak<HandleState>,
    queued_input: Arc<AtomicUsize>,
    stdin_closed: Cell<bool>,
}

// Completed results belong to the reachable JS handle, never the VM registry.
// Methods capture this native object, so extracted methods retain their result too.
#[derive(Default)]
struct HandleState {
    terminal: RefCell<Option<crate::web::js::V>>,
}
struct Handle {
    state: Rc<HandleState>,
}
impl crate::web::js::Trace for Handle {
    fn trace(&self, mark: &mut dyn FnMut(Value)) {
        self.state.terminal.borrow().trace(mark);
    }
}
impl crate::web::Object for Handle {}

/// Per-VM process registry and worker mailbox.
pub(crate) struct Processes {
    entries: RefCell<HashMap<u32, Entry>>,
    next: Cell<u32>,
    tx: mpsc::Sender<Event>,
    rx: RefCell<mpsc::Receiver<Event>>,
    queued: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
}

impl Default for Processes {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            entries: RefCell::default(),
            next: Cell::new(0),
            tx,
            rx: RefCell::new(rx),
            queued: Arc::new(AtomicUsize::new(0)),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Processes {
    /// Terminates owned process trees before freeing the VM's callbacks.
    pub(crate) fn free_all(&mut self, ctx: *mut Context) {
        self.stopped.store(true, Ordering::Release);
        for (_, mut e) in self.entries.get_mut().drain() {
            let _ = e.input.send(Input::Kill);
            if let Some(worker) = e.worker.take() {
                let _ = worker.join();
            }
            for f in e.callbacks.into_iter().flatten() {
                free(ctx, f);
            }
            free(ctx, e.resolve);
            free(ctx, e.reject);
        }
    }
}

/// Stops this VM's subprocess workers while its context is still live.
pub(crate) fn free_all(ctx: *mut Context) {
    // SAFETY: teardown exclusively owns the state and never calls user JS.
    unsafe {
        (*crate::state::state(ctx)).processes.free_all(ctx);
    }
}

fn processes<'a>(ctx: *mut Context) -> &'a Processes {
    // SAFETY: a short projection of the live VM's state.
    unsafe { &(*crate::state::state(ctx)).processes }
}
fn free(ctx: *mut Context, v: Value) {
    // SAFETY: owned value on the engine thread.
    unsafe { qjs::JS_FreeValue(ctx, v) }
}
fn dup(ctx: *mut Context, v: Value) -> Value {
    // SAFETY: live value on the engine thread.
    unsafe { qjs::JS_DupValue(ctx, v) }
}
fn string(ctx: *mut Context, text: &str) -> Value {
    // SAFETY: QuickJS copies the buffer.
    unsafe { qjs::JS_NewStringLen(ctx, text.as_ptr().cast(), text.len() as _) }
}
fn get(ctx: *mut Context, v: Value, key: &CStr) -> Value {
    // SAFETY: live value, property getter exceptions remain pending.
    unsafe { qjs::JS_GetPropertyStr(ctx, v, key.as_ptr()) }
}
fn put(ctx: *mut Context, v: Value, key: &CStr, x: Value) {
    // SAFETY: the object takes the owned property value.
    unsafe {
        qjs::JS_SetPropertyStr(ctx, v, key.as_ptr(), x);
    }
}
fn throw(ctx: *mut Context, message: &str) -> Value {
    let text = CString::new(message).unwrap_or_default();
    // SAFETY: literal format prevents interpreting user text as a format.
    unsafe { qjs::JS_ThrowTypeError(ctx, c"%s".as_ptr(), text.as_ptr()) }
}
fn text(ctx: *mut Context, v: Value) -> Result<String, String> {
    // SAFETY: tag check of a live argument.
    if !unsafe { qjs::JS_IsString(v) } {
        return Err("expected a string".into());
    }
    dom::string_arg(ctx, v).ok_or_else(|| "could not read string".into())
}
fn nullish(v: Value) -> bool {
    // SAFETY: inline tag checks.
    unsafe { qjs::JS_IsUndefined(v) || qjs::JS_IsNull(v) }
}
fn option_text(ctx: *mut Context, v: Value, key: &CStr) -> Result<Option<String>, String> {
    let x = get(ctx, v, key);
    let result = if nullish(x) {
        Ok(None)
    } else {
        text(ctx, x).map(Some)
    };
    free(ctx, x);
    result
}
fn option_number(
    ctx: *mut Context,
    v: Value,
    key: &CStr,
    default: usize,
    max: usize,
) -> Result<usize, String> {
    let x = get(ctx, v, key);
    if nullish(x) {
        free(ctx, x);
        return Ok(default);
    }
    let mut n = 0.0;
    // SAFETY: conversion of a live property value.
    let ok = unsafe { qjs::JS_IsNumber(x) && qjs::JS_ToFloat64(ctx, &mut n, x) == 0 };
    free(ctx, x);
    if !ok || !n.is_finite() || n < 1.0 || n.fract() != 0.0 || n > max as f64 {
        return Err(format!(
            "{} must be an integer from 1 to {max}",
            key.to_string_lossy()
        ));
    }
    Ok(n as usize)
}
fn available(ctx: *mut Context) -> bool {
    // SAFETY: short projection of renderer state.
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    )) && unsafe { (*crate::state::state(ctx)).dom.os().is_desktop() }
}

struct Options {
    command: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    timeout: Option<Duration>,
    max_output: usize,
}

fn options(
    ctx: *mut Context,
    argc: c_int,
    argv: *mut Value,
    capture: bool,
) -> Result<Options, String> {
    if !available(ctx) {
        return Err("nsh/process is available on desktop hosts".into());
    }
    let command = text(ctx, arg(argc, argv, 0))?;
    if command.is_empty() || command.contains('\0') {
        return Err("invalid executable".into());
    }
    let shared = dom::shared(ctx);
    if !shared.system_access.borrow().allows_process(&command) {
        return Err(format!("process capability denied for `{command}`"));
    }
    let argv_value = arg(argc, argv, 1);
    let mut args = Vec::new();
    if !nullish(argv_value) {
        // SAFETY: tag check and length of the live argument array.
        if !unsafe { qjs::JS_IsArray(argv_value) } {
            return Err("arguments must be an array of strings".into());
        }
        let mut len = 0;
        // SAFETY: length of a live array.
        unsafe {
            qjs::JS_GetLength(ctx, argv_value, &mut len);
        }
        if len > 4096 {
            return Err("too many arguments".into());
        }
        for i in 0..len {
            // SAFETY: reading a live array's indexed property.
            let v = unsafe { qjs::JS_GetPropertyInt64(ctx, argv_value, i) };
            let s = text(ctx, v);
            free(ctx, v);
            args.push(s?);
        }
    }
    let opts = arg(argc, argv, 2);
    // SAFETY: inline tag check.
    if !nullish(opts) && !unsafe { qjs::JS_IsObject(opts) } {
        return Err("options must be an object".into());
    }
    let data_dir = shared
        .system_data_dir
        .borrow()
        .clone()
        .ok_or("host did not configure an application data directory")?;
    let cwd = if nullish(opts) {
        data_dir.clone()
    } else {
        option_text(ctx, opts, c"cwd")?
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.clone())
    };
    let cwd = shared
        .system_access
        .borrow()
        .authorize_file(&cwd, &data_dir, false)?;
    if !cwd.is_dir() {
        return Err("cwd must be an existing directory".into());
    }
    // Resolve before changing cwd. Child lookup never consults a workspace or app-provided PATH.
    let command = resolve_executable(&command)?;
    let timeout = if nullish(opts) {
        if capture {
            Some(Duration::from_secs(10))
        } else {
            None
        }
    } else {
        let t = get(ctx, opts, c"timeoutMs");
        let supplied = !nullish(t);
        free(ctx, t);
        if supplied || capture {
            Some(Duration::from_millis(
                option_number(ctx, opts, c"timeoutMs", 10_000, 3_600_000)? as u64,
            ))
        } else {
            None
        }
    };
    let max_output = if nullish(opts) {
        1024 * 1024
    } else {
        option_number(ctx, opts, c"maxOutputBytes", 1024 * 1024, QUEUE_BYTES)?
    };
    if !nullish(opts) {
        if let Some(stdin) = option_text(ctx, opts, c"stdin")?
            && stdin != "pipe"
        {
            return Err("stdin must be 'pipe'".into());
        }
        let env = get(ctx, opts, c"env");
        let has_env = !nullish(env);
        free(ctx, env);
        if has_env {
            return Err(
                "custom environment is not supported; children inherit the host environment".into(),
            );
        }
    }
    Ok(Options {
        command,
        args,
        cwd,
        timeout,
        max_output,
    })
}

fn resolve_executable(command: &str) -> Result<PathBuf, String> {
    let path = Path::new(command);
    if path.is_absolute() {
        return executable(path);
    }
    if command.contains('/') || command.contains('\\') {
        return Err("use a bare executable name or an absolute path".into());
    }
    let host_path = std::env::var_os("PATH").ok_or("host PATH is unset")?;
    for directory in std::env::split_paths(&host_path).filter(|p| p.is_absolute()) {
        let candidate = directory.join(command);
        if let Ok(path) = executable(&candidate) {
            return Ok(path);
        }
        #[cfg(target_os = "windows")]
        if candidate.extension().is_none() {
            if let Ok(path) = executable(&candidate.with_extension("exe")) {
                return Ok(path);
            }
        }
    }
    Err(format!(
        "executable `{command}` was not found in the host PATH"
    ))
}
fn executable(path: &Path) -> Result<PathBuf, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("executable is not a file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("file is not executable".into());
        }
    }
    std::fs::canonicalize(path).map_err(|e| e.to_string())
}

#[derive(Clone)]
struct Mailbox {
    id: u32,
    tx: mpsc::Sender<Event>,
    queued: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    overflow: Arc<AtomicBool>,
    wake: Option<Wake>,
}
impl Mailbox {
    fn output(&self, stderr: bool, chunk: String) -> bool {
        if self.stopped.load(Ordering::Acquire) {
            return false;
        }
        let len = chunk.len();
        if self.queued.fetch_add(len, Ordering::AcqRel) + len > QUEUE_BYTES {
            self.queued.fetch_sub(len, Ordering::AcqRel);
            self.overflow.store(true, Ordering::Release);
            return false;
        }
        if self.tx.send(Event::Output(self.id, stderr, chunk)).is_err() {
            return false;
        }
        if let Some(wake) = &self.wake {
            wake();
        }
        true
    }
    fn exit(&self, code: Option<i32>, signal: Option<i32>, error: Option<String>) {
        let _ = self.tx.send(Event::Exit(self.id, code, signal, error));
        if let Some(wake) = &self.wake {
            wake();
        }
    }
}

#[cfg(unix)]
trait Pipe: std::os::fd::AsRawFd {}
#[cfg(unix)]
impl<T: std::os::fd::AsRawFd> Pipe for T {}
#[cfg(not(unix))]
trait Pipe {}
#[cfg(not(unix))]
impl<T> Pipe for T {}

fn prepare_pipe(pipe: &impl Pipe) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        // SAFETY: the borrowed pipe owns a live fd; preserve its other flags.
        let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    #[cfg(not(unix))]
    let _ = pipe;
    Ok(())
}

fn read_pipe(mut pipe: impl Read + Pipe, mailbox: Mailbox, stderr: bool, stop: Arc<AtomicBool>) {
    if prepare_pipe(&pipe).is_err() {
        mailbox.overflow.store(true, Ordering::Release);
        return;
    }
    let mut buffer = [0; 4096];
    let mut pending = Vec::new();
    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                pending.extend_from_slice(&buffer[..n]);
                loop {
                    match std::str::from_utf8(&pending) {
                        Ok(s) => {
                            if !s.is_empty() && !mailbox.output(stderr, s.to_owned()) {
                                return;
                            }
                            pending.clear();
                            break;
                        }
                        Err(e) => {
                            let valid = e.valid_up_to();
                            if valid != 0 {
                                let s = String::from_utf8_lossy(&pending[..valid]).into_owned();
                                if !mailbox.output(stderr, s) {
                                    return;
                                }
                                pending.drain(..valid);
                            }
                            if let Some(n) = e.error_len() {
                                pending.drain(..n);
                                if !mailbox.output(stderr, "\u{fffd}".into()) {
                                    return;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
    if !pending.is_empty() {
        mailbox.output(stderr, String::from_utf8_lossy(&pending).into_owned());
    }
}

fn write_pipe(
    mut pipe: impl Write + Pipe,
    input: mpsc::Receiver<Input>,
    queued: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
) {
    if prepare_pipe(&pipe).is_err() {
        return;
    }
    while !stop.load(Ordering::Acquire) {
        let command = match input.recv_timeout(Duration::from_millis(5)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let Input::Write(bytes) = command else { break };
        queued.fetch_sub(bytes.len(), Ordering::AcqRel);
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() && !stop.load(Ordering::Acquire) {
            match pipe.write(remaining) {
                Ok(0) => return,
                Ok(n) => remaining = &remaining[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        }
    }
}

fn terminate(child: &mut Child, force: bool) {
    #[cfg(unix)]
    {
        // SAFETY: the child was created in a new process group with its pid as pgid.
        unsafe {
            libc::kill(
                -(child.id() as i32),
                if force { libc::SIGKILL } else { libc::SIGTERM },
            );
        }
    }
    #[cfg(target_os = "windows")]
    {
        // Windows' system utility terminates the complete child tree; no app text is interpreted by a shell.
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let utility = system_root.join("System32").join("taskkill.exe");
        let _ = Command::new(utility)
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = force;
        let _ = child.kill();
    }
}

fn supervise(
    mut child: Child,
    input: mpsc::Receiver<Input>,
    mailbox: Mailbox,
    timeout: Option<Duration>,
    queued_input: Arc<AtomicUsize>,
) {
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdin = child.stdin.take().expect("piped stdin");
    let io_stop = Arc::new(AtomicBool::new(false));
    let (writer_tx, writer_rx) = mpsc::channel();
    let writer_stop = io_stop.clone();
    let writer = thread::spawn(move || write_pipe(stdin, writer_rx, queued_input, writer_stop));
    let out_mail = mailbox.clone();
    let err_mail = mailbox.clone();
    let out_stop = io_stop.clone();
    let err_stop = io_stop.clone();
    let out = thread::spawn(move || read_pipe(stdout, out_mail, false, out_stop));
    let err = thread::spawn(move || read_pipe(stderr, err_mail, true, err_stop));
    let started = Instant::now();
    let mut terminating = None;
    let mut error = None;
    let status = loop {
        let mut kill =
            mailbox.stopped.load(Ordering::Acquire) || mailbox.overflow.load(Ordering::Acquire);
        if mailbox.overflow.load(Ordering::Acquire) {
            error.get_or_insert_with(|| "process output queue exceeded 8 MB".to_owned());
        }
        if timeout.is_some_and(|limit| started.elapsed() >= limit) {
            kill = true;
            error.get_or_insert_with(|| "process timed out".to_owned());
        }
        // A pipe write may block: stdin is written on its own thread below, never this supervisor.
        match input.recv_timeout(Duration::from_millis(10)) {
            Ok(Input::Write(bytes)) => {
                let _ = writer_tx.send(Input::Write(bytes));
            }
            Ok(Input::Close) => {
                let _ = writer_tx.send(Input::Close);
            }
            Ok(Input::Kill) | Err(mpsc::RecvTimeoutError::Disconnected) => kill = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if kill && terminating.is_none() {
            terminate(&mut child, false);
            terminating = Some(Instant::now());
            let _ = writer_tx.send(Input::Close);
        }
        if terminating.is_some_and(|at| at.elapsed() >= Duration::from_millis(250)) {
            terminate(&mut child, true);
        }
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(e) => {
                error = Some(e.to_string());
                terminate(&mut child, true);
                break child.wait().ok();
            }
        }
    };
    // The leader may exit with descendants still holding its pipe handles.
    terminate(&mut child, true);
    let _ = writer_tx.send(Input::Close);
    drop(writer_tx);
    // Drain final buffered output, but an escaped/detached descendant must not
    // keep inherited pipe handles (and therefore VM teardown) alive forever.
    let drain_until = Instant::now() + Duration::from_millis(100);
    while !(writer.is_finished() && out.is_finished() && err.is_finished())
        && Instant::now() < drain_until
    {
        thread::sleep(Duration::from_millis(5));
    }
    io_stop.store(true, Ordering::Release);
    let _ = writer.join();
    let _ = out.join();
    let _ = err.join();
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.as_ref().and_then(|s| s.signal())
    };
    #[cfg(not(unix))]
    let signal = None;
    mailbox.exit(status.and_then(|s| s.code()), signal, error);
}

fn start(
    ctx: *mut Context,
    opts: Options,
    capture: bool,
    resolve: Value,
    reject: Value,
) -> Result<(u32, u32), String> {
    let p = processes(ctx);
    if p.entries.borrow().len() >= MAX_CHILDREN {
        return Err("a VM may run at most 32 children".into());
    }
    let mut cmd = Command::new(&opts.command);
    cmd.args(opts.args)
        .current_dir(opts.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("could not launch process: {e}"))?;
    let pid = child.id();
    let id = p
        .next
        .get()
        .checked_add(1)
        .ok_or("process id space exhausted")?;
    p.next.set(id);
    let (tx, rx) = mpsc::channel();
    let shared = dom::shared(ctx);
    let mailbox = Mailbox {
        id,
        tx: p.tx.clone(),
        queued: p.queued.clone(),
        stopped: p.stopped.clone(),
        overflow: Arc::new(AtomicBool::new(false)),
        wake: shared.system_wake.borrow().clone(),
    };
    let queued_input = Arc::new(AtomicUsize::new(0));
    let worker_input = queued_input.clone();
    let worker = thread::spawn(move || supervise(child, rx, mailbox, opts.timeout, worker_input));
    if capture {
        let _ = tx.send(Input::Close);
    }
    p.entries.borrow_mut().insert(
        id,
        Entry {
            input: tx,
            worker: Some(worker),
            callbacks: Default::default(),
            resolve,
            reject,
            stdout: String::new(),
            stderr: String::new(),
            capture,
            max_output: opts.max_output,
            handle: Weak::new(),
            queued_input,
            stdin_closed: Cell::new(capture),
        },
    );
    // Bind process lifetime to the reactive owner or development module when one exists.
    // VM teardown remains the final owner for processes started from plain callbacks.
    let mut data = [qjs::JS_NewFloat64(id as f64)];
    // SAFETY: immutable VM-local function data; QuickJS duplicates it.
    let cleanup =
        unsafe { qjs::JS_NewCFunctionData(ctx, Some(method), 0, 4, 1, data.as_mut_ptr()) };
    free(ctx, data[0]);
    if crate::reactive::has_owner(ctx) {
        let mut callback = cleanup;
        // SAFETY: one borrowed callback; on_cleanup duplicates it for the owner.
        let result = unsafe { crate::reactive::on_cleanup(ctx, U, 1, &mut callback) };
        free(ctx, result);
        free(ctx, cleanup);
    } else {
        crate::hot::undo_on_rerun(ctx, crate::web::js::V::own(ctx, cleanup));
    }
    Ok((id, pid))
}

unsafe extern "C" fn spawn(
    ctx: *mut Context,
    _this: Value,
    argc: c_int,
    argv: *mut Value,
) -> Value {
    let opts = match options(ctx, argc, argv, false) {
        Ok(o) => o,
        Err(e) => return throw(ctx, &e),
    };
    let (id, pid) = match start(ctx, opts, false, U, U) {
        Ok(v) => v,
        Err(e) => return throw(ctx, &e),
    };
    // SAFETY: a fresh object takes its functions, each closing over an immutable VM-local id.
    unsafe {
        let state = Rc::new(HandleState::default());
        processes(ctx)
            .entries
            .borrow_mut()
            .get_mut(&id)
            .expect("new process")
            .handle = Rc::downgrade(&state);
        let child =
            match crate::web::instance(ctx, U, crate::web::Class::Internal, Handle { state }) {
                Ok(child) => child.take(),
                Err(_) => {
                    if let Some(entry) = processes(ctx).entries.borrow().get(&id) {
                        let _ = entry.input.send(Input::Kill);
                    }
                    return qjs::JS_EXCEPTION;
                }
            };
        put(ctx, child, c"pid", qjs::JS_NewFloat64(pid as f64));
        for (key, magic, length) in [
            (c"on", 0, 2),
            (c"write", 1, 1),
            (c"closeStdin", 2, 0),
            (c"kill", 3, 0),
        ] {
            let mut data = [qjs::JS_NewFloat64(id as f64), child];
            let f =
                qjs::JS_NewCFunctionData(ctx, Some(method), length, magic, 2, data.as_mut_ptr());
            free(ctx, data[0]);
            put(ctx, child, key, f);
        }
        child
    }
}
unsafe extern "C" fn run(ctx: *mut Context, _this: Value, argc: c_int, argv: *mut Value) -> Value {
    let mut callbacks = [U; 2];
    // SAFETY: creates an owned promise and two owned resolving functions.
    let promise = unsafe { qjs::JS_NewPromiseCapability(ctx, callbacks.as_mut_ptr()) };
    let result = options(ctx, argc, argv, true)
        .and_then(|o| start(ctx, o, true, callbacks[0], callbacks[1]));
    if let Err(e) = result {
        call(ctx, callbacks[1], string(ctx, &e));
        free(ctx, callbacks[0]);
        free(ctx, callbacks[1]);
    }
    promise
}

unsafe extern "C" fn method(
    ctx: *mut Context,
    _this: Value,
    argc: c_int,
    argv: *mut Value,
    magic: c_int,
    data: *mut Value,
) -> Value {
    let mut numeric_id = 0.0;
    // SAFETY: function data holds the uint32 id installed at creation.
    unsafe {
        qjs::JS_ToFloat64(ctx, &mut numeric_id, *data);
    }
    let id = numeric_id as u32;
    if magic == 4 {
        if let Some(entry) = processes(ctx).entries.borrow().get(&id) {
            let _ = entry.input.send(Input::Kill);
        }
        return U;
    }
    // SAFETY: public child methods carry the native child object as data[1].
    let handle =
        unsafe { crate::web::peek::<Handle, _>(ctx, *data.add(1), |handle| handle.state.clone()) };
    let Some(handle) = handle else {
        return throw(ctx, "invalid process handle");
    };
    if magic == 0 {
        let name = match text(ctx, arg(argc, argv, 0)) {
            Ok(n) => n,
            Err(e) => return throw(ctx, &e),
        };
        let index = match name.as_str() {
            "stdout" => 0,
            "stderr" => 1,
            "exit" => 2,
            _ => return throw(ctx, "event must be stdout, stderr, or exit"),
        };
        let callback = arg(argc, argv, 1);
        // SAFETY: tag check of live argument.
        if !unsafe { qjs::JS_IsFunction(ctx, callback) } {
            return throw(ctx, "listener must be a function");
        }
        let terminal = handle.terminal.borrow().as_ref().map(|v| dup(ctx, v.raw()));
        if let Some(value) = terminal {
            if index == 2 {
                call(ctx, callback, value);
            } else {
                free(ctx, value);
            }
            return U;
        }
        let mut entries = processes(ctx).entries.borrow_mut();
        let Some(e) = entries.get_mut(&id) else {
            return throw(ctx, "process has exited");
        };
        if e.callbacks[index].len() >= 64 {
            return throw(ctx, "too many process listeners");
        }
        e.callbacks[index].push(dup(ctx, callback));
        return U;
    }
    let bytes = if magic == 1 {
        match text(ctx, arg(argc, argv, 0)) {
            Ok(s) => Some(s.into_bytes()),
            Err(e) => return throw(ctx, &e),
        }
    } else {
        None
    };
    let entries = processes(ctx).entries.borrow();
    let Some(e) = entries.get(&id) else {
        return if magic == 3 {
            U
        } else {
            throw(ctx, "process has exited")
        };
    };
    let input = if let Some(bytes) = bytes {
        if e.stdin_closed.get() {
            return throw(ctx, "process stdin is closed");
        }
        let len = bytes.len();
        if e.queued_input.fetch_add(len, Ordering::AcqRel) + len > 1024 * 1024 {
            e.queued_input.fetch_sub(len, Ordering::AcqRel);
            return throw(ctx, "stdin queue exceeded 1 MB");
        }
        Input::Write(bytes)
    } else if magic == 2 {
        e.stdin_closed.set(true);
        Input::Close
    } else {
        e.stdin_closed.set(true);
        Input::Kill
    };
    if e.input.send(input).is_err() {
        return throw(ctx, "process input is closed");
    }
    U
}

fn call(ctx: *mut Context, f: Value, arg: Value) {
    let mut args = [arg];
    // SAFETY: no registry borrow spans this call; user code may reenter or unregister.
    unsafe {
        let result = qjs::JS_Call(ctx, f, U, 1, args.as_mut_ptr());
        if qjs::JS_IsException(result) {
            let error = qjs::JS_GetException(ctx);
            log::warn!(target:"js", "process callback: {}", dom::string_arg(ctx, error).unwrap_or_default());
            free(ctx, error);
        }
        free(ctx, result);
    }
    free(ctx, arg);
}

/// Delivers worker events on the engine thread. Returns whether work ran.
pub(crate) fn pump(ctx: *mut Context) -> bool {
    let mut ran = false;
    let mut count = 0;
    // Bound one turn so a noisy child cannot starve timers or UI events.
    for _ in 0..256 {
        let event = processes(ctx).rx.borrow_mut().try_recv();
        let Ok(event) = event else {
            break;
        };
        ran = true;
        count += 1;
        match event {
            Event::Output(id, stderr, chunk) => {
                processes(ctx)
                    .queued
                    .fetch_sub(chunk.len(), Ordering::AcqRel);
                let callbacks = {
                    let mut entries = processes(ctx).entries.borrow_mut();
                    let Some(e) = entries.get_mut(&id) else {
                        continue;
                    };
                    if e.capture {
                        if e.stdout.len() + e.stderr.len() + chunk.len() > e.max_output {
                            let _ = e.input.send(Input::Kill);
                            e.stderr = "process capture exceeded maxOutputBytes".into();
                            e.max_output = 0;
                        } else if stderr {
                            e.stderr.push_str(&chunk);
                        } else {
                            e.stdout.push_str(&chunk);
                        }
                    }
                    e.callbacks[usize::from(stderr)]
                        .iter()
                        .map(|&f| dup(ctx, f))
                        .collect::<Vec<_>>()
                };
                for f in callbacks {
                    call(ctx, f, string(ctx, &chunk));
                    free(ctx, f);
                }
            }
            Event::Exit(id, code, signal, mut error) => {
                let (callbacks, resolve, reject, value, worker, capture) = {
                    let mut entries = processes(ctx).entries.borrow_mut();
                    let Some(mut e) = entries.remove(&id) else {
                        continue;
                    };
                    drop(entries);
                    if e.max_output == 0 {
                        error = Some("process capture exceeded maxOutputBytes".into());
                    }
                    // SAFETY: fresh result object owns all of its property values.
                    let value = unsafe { qjs::JS_NewObject(ctx) };
                    // SAFETY: integer values are immediate.
                    put(
                        ctx,
                        value,
                        c"code",
                        code.map(|n| qjs::JS_NewFloat64(n as f64))
                            .unwrap_or(qjs::JS_NULL),
                    );
                    if let Some(signal) = signal {
                        put(ctx, value, c"signal", qjs::JS_NewFloat64(signal as f64));
                    }
                    if let Some(error) = &error {
                        put(ctx, value, c"error", string(ctx, error));
                    }
                    if e.capture {
                        put(ctx, value, c"stdout", string(ctx, &e.stdout));
                        put(ctx, value, c"stderr", string(ctx, &e.stderr));
                    }
                    if let Some(handle) = e.handle.upgrade() {
                        *handle.terminal.borrow_mut() = Some(crate::web::js::V::dup(ctx, value));
                    }
                    let callbacks = std::mem::take(&mut e.callbacks[2]);
                    for index in 0..2 {
                        for f in e.callbacks[index].drain(..) {
                            free(ctx, f);
                        }
                    }
                    let resolve = std::mem::replace(&mut e.resolve, U);
                    let reject = std::mem::replace(&mut e.reject, U);
                    (
                        callbacks,
                        resolve,
                        reject,
                        value,
                        e.worker.take(),
                        e.capture,
                    )
                };
                if let Some(worker) = worker {
                    let _ = worker.join();
                }
                for f in callbacks {
                    call(ctx, f, dup(ctx, value));
                    free(ctx, f);
                }
                if capture {
                    if let Some(error) = error {
                        call(ctx, reject, string(ctx, &error));
                    } else {
                        call(ctx, resolve, dup(ctx, value));
                    }
                }
                free(ctx, resolve);
                free(ctx, reject);
                free(ctx, value);
            }
        }
    }
    if count == 256
        && let Some(wake) = dom::shared(ctx).system_wake.borrow().clone()
    {
        wake();
    }
    ran
}

/// Whether the VM owns a running subprocess.
pub(crate) fn has_pending(ctx: *mut Context) -> bool {
    !processes(ctx).entries.borrow().is_empty()
}

pub(crate) fn module(ctx: *mut Context) -> Value {
    // SAFETY: fresh module object owns both native functions.
    unsafe {
        let v = qjs::JS_NewObject(ctx);
        put(
            ctx,
            v,
            c"spawn",
            reactive::new_function(ctx, c"spawn", spawn, 3),
        );
        put(ctx, v, c"run", reactive::new_function(ctx, c"run", run, 3));
        v
    }
}

/// `nsh/process`'s native exports and colocated generated declarations.
pub(crate) const EXPORTS: &[Item] = &[
    (
        "isAvailable",
        Export::Value(|ctx| {
            if available(ctx) {
                qjs::JS_TRUE
            } else {
                qjs::JS_FALSE
            }
        }),
        Ts::value("Desktop subprocess support.", "boolean"),
    ),
    (
        "Process",
        Export::Value(module),
        Ts::value(
            "Subprocesses owned by this app VM; capabilities are checked before launch.",
            "ProcessModule",
        ),
    ),
];

/// TypeScript types assembled into the generated module declaration.
pub(crate) const TYPES: &str = r#"
export interface ProcessOptions {
  /** Existing directory within a filesystem read grant; defaults to the app data directory. */
  cwd?: string;
  /** Children inherit the host environment, including its PATH. No shell is started. */
  stdin?: "pipe";
  /** Terminates the process tree after this duration; run defaults to 10 seconds. */
  timeoutMs?: number;
  /** run's combined stdout/stderr limit, default 1 MB, maximum 8 MB. */
  maxOutputBytes?: number;
}
export interface ProcessExit { code: number | null; signal?: number; error?: string; }
export interface ProcessResult extends ProcessExit { stdout: string; stderr: string; }
export interface ChildProcess {
  readonly pid: number;
  /** Output is incremental UTF-8; exit follows the final output and fires once per listener. */
  on(event: "stdout" | "stderr", listener: (text: string) => void): void;
  on(event: "exit", listener: (result: ProcessExit) => void): void;
  /** Enqueues UTF-8 stdin; throws if the outstanding queue exceeds 1 MB. */
  write(text: string): void;
  closeStdin(): void;
  /** Terminates the complete child process group, escalating after 250 ms. */
  kill(): void;
}
export interface ProcessModule {
  /** Direct execution; a launch failure throws synchronously. VM shutdown terminates its children. */
  spawn(command: string, args?: readonly string[], options?: ProcessOptions): ChildProcess;
  /** Captures bounded output; launch failures, timeouts and output limits reject. */
  run(command: string, args?: readonly string[], options?: ProcessOptions): Promise<ProcessResult>;
}
"#;

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;

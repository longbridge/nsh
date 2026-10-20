//! Runs a test file: its tests on an engine of their own (the test file's
//! VM, serving `nsh/test`), one after the other, each app they open on a
//! headless engine of its own ([`Headless`]), closed when the test ends.
//!
//! The test file is bundled under an entry made for it, which imports it
//! (its `test` calls declare the tests) and then runs them
//! ([`run_tests`], through the internal module `nsh/test/runner`). What the
//! VM's calls need, the app's sets and how to build an entry, is the
//! [`Session`] of the file being run: one at a time in a process.

use std::{
    collections::HashMap,
    future::Future,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    task::Poll,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApproxTextEngine, MountBatch,
    geometry::px,
    ui::{Registry, Root, prelude::timer},
};
use nsh_engine::{
    AppError, AppErrorKind, Bundle, Edges, Engine, EngineConfig, Environment, MountSink,
    headless::{DirectoryAssets, Headless, MemoryStorage, StubNetwork, UreqNetwork, print_batch},
};

use crate::module::{NetworkOption, OpenOptions};

/// How long a test may take.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Builds an entry ([`Entry`]; an app's may be its directory).
pub type Compile = Arc<dyn Fn(&Path, Entry) -> Result<Built, String> + Send + Sync>;

/// What [`Compile`] built.
#[derive(Clone)]
pub enum Built {
    /// A bundle: its bytecode (without its source map), and its source map.
    Js(Box<Bundle>, Option<String>),
    /// The root of an app of Rust only (`nsh.json` `"root"`): what `open`
    /// of the app starts.
    Rust(Root),
}

/// What an entry [`Compile`] builds is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    /// An app's entry, which a test opens: built as `nsh run` builds it.
    App,
    /// The entry `nsh test` makes for a test file: built against the app's
    /// sets and [`test_sets`](crate::test_sets).
    TestFile,
}

/// Where batches are printed (`nsh test --trace`).
pub type Output = Arc<Mutex<Box<dyn Write + Send>>>;

/// How test files run: the app they test and what is shown of the run.
#[derive(Clone)]
pub struct TestRun {
    /// The UI sets the app's hosts serve (the mobile UI, the app's crates'):
    /// the apps tests open are built against them.
    pub ui_sets: Vec<Registry>,
    /// Builds the apps tests open and the entries made for test files
    /// ([`compiler`](crate::compiler) in this process).
    pub compile: Compile,
    /// Where the entries made for test files go (`<app>/.nsh/test`).
    pub work_dir: PathBuf,
    /// Runs only the tests whose name contains this.
    pub filter: Option<String>,
    /// Prints each batch of the apps the tests open, under the step it came
    /// in, and logs their commits' timings.
    pub trace: Option<Output>,
}

/// How a test ended.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TestResult {
    /// Its name.
    pub name: String,
    /// How long it took.
    pub elapsed: Duration,
    /// Why it failed (`None`: it passed).
    pub failure: Option<Failure>,
}

/// Why a test failed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Failure {
    /// What was thrown.
    pub message: String,
    /// Where (the test file's positions), when an `Error` was thrown.
    pub stack: Option<String>,
    /// The texts on screen of the app it opened last, when it opened one.
    pub screen: Option<Vec<String>>,
}

/// Runs the tests of the test file `file` as `run` says, calling `report`
/// with each one's result as it ends: their results, or why the file could
/// not run (it does not build, it throws as it loads).
pub fn run_file(
    run: &TestRun,
    file: &Path,
    report: &mut dyn FnMut(&TestResult),
) -> Result<Vec<TestResult>, String> {
    // One test file at a time in a process: the VM's calls find its session.
    static RUNNING: Mutex<()> = Mutex::new(());
    let _running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    let file = dunce::canonicalize(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let (bundle, source_map) = compile_test_file(run, &file)?;
    let (events, results) = crossbeam_channel::unbounded();
    let session = Arc::new(Session {
        dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        run: run.clone(),
        built: Mutex::new(HashMap::new()),
        apps: Mutex::new(Vec::new()),
        events: events.clone(),
    });
    *current() = Some(session.clone());
    let ended = drive(
        run,
        bundle,
        &events,
        &results,
        source_map.as_deref(),
        report,
    );
    *current() = None;
    session.close_apps();
    ended
}

/// Starts the test file's VM and collects what it reports until its tests
/// ended.
fn drive(
    run: &TestRun,
    bundle: Bundle,
    events: &Sender<Event>,
    results: &Receiver<Event>,
    source_map: Option<&str>,
    report: &mut dyn FnMut(&TestResult),
) -> Result<Vec<TestResult>, String> {
    let mut ui_sets = run.ui_sets.clone();
    ui_sets.extend(crate::test_sets());
    let config = EngineConfig {
        // The sets' fonts load as the engine starts.
        assets: Some(Arc::new(DirectoryAssets::new(
            run.work_dir.clone(),
            set_files(&ui_sets),
        ))),
        ui_sets,
        ..EngineConfig::new(bundle)
    };
    let sink = VmSink {
        events: events.clone(),
    };
    let _vm = Engine::start(config, || ApproxTextEngine, sink)
        .map_err(|e| format!("the test file's engine did not start: {e}"))?;
    let mut out = Vec::new();
    loop {
        match results.recv() {
            Ok(Event::Result(mut result)) => {
                if let (Some(failure), Some(map)) = (&mut result.failure, source_map) {
                    failure.stack = failure
                        .stack
                        .take()
                        .map(|stack| nsh_engine::symbolicate(map, &stack));
                }
                report(&result);
                out.push(result);
            }
            Ok(Event::Done) => return Ok(out),
            Ok(Event::Error(error)) => match error.kind {
                AppErrorKind::Load | AppErrorKind::Stopped => return Err(error.to_string()),
                _ => log::error!(target: "js", "{error}"),
            },
            Err(_) => return Err("the test file's engine stopped".into()),
        }
    }
}

/// The test file bundled under the entry made for it, and compiled: the
/// VM's app and its source map.
fn compile_test_file(run: &TestRun, file: &Path) -> Result<(Bundle, Option<String>), String> {
    let name = file
        .file_name()
        .map_or("test".into(), |n| n.to_string_lossy().into_owned());
    std::fs::create_dir_all(&run.work_dir)
        .map_err(|e| format!("{}: {e}", run.work_dir.display()))?;
    let entry = run.work_dir.join(format!("{name}.run.js"));
    let path = serde_json::to_string(&file.to_string_lossy().replace('\\', "/"))
        .map_err(|e| e.to_string())?;
    let code = format!(
        "// Made by `nsh test`: runs the tests of {name}.\nimport {path};\nimport {{ run }} from \"{}\";\nrun();\n",
        crate::module::RUNNER
    );
    std::fs::write(&entry, code).map_err(|e| format!("{}: {e}", entry.display()))?;
    let Built::Js(bundle, map) = (run.compile)(&entry, Entry::TestFile)? else {
        return Err(format!("{}: built as a Rust root", entry.display()));
    };
    let map = map.map(|map| readable_sources(&map, &run.work_dir));
    let bundle = match &map {
        Some(map) => bundle.with_source_map(map.clone()),
        None => *bundle,
    };
    Ok((bundle, map))
}

/// The source map `map` (of a bundle made in `dir`) with its sources
/// relative to the current directory, as a terminal shows paths (else
/// absolute): stacks name the test file as its author knows it.
fn readable_sources(map: &str, dir: &Path) -> String {
    let Ok(mut json) = serde_json::from_str::<serde_json::Value>(map) else {
        return map.to_owned();
    };
    let cwd = std::env::current_dir().ok();
    if let Some(sources) = json.get_mut("sources").and_then(|s| s.as_array_mut()) {
        for source in sources {
            let Some(path) = source.as_str() else {
                continue;
            };
            let joined = dir.join(path);
            let path = dunce::canonicalize(&joined).unwrap_or(joined);
            let shown = cwd
                .as_deref()
                .and_then(|cwd| path.strip_prefix(cwd).ok())
                .unwrap_or(&path);
            *source = shown.to_string_lossy().replace('\\', "/").into();
        }
    }
    json.to_string()
}

/// The files `sets` ship, by their path among an app's assets.
fn set_files(sets: &[Registry]) -> Vec<(String, PathBuf)> {
    sets.iter()
        .flat_map(Registry::asset_files)
        .map(|file| (file.path().to_owned(), PathBuf::from(file.source)))
        .collect()
}

/// What the test file's VM reports.
enum Event {
    /// A test ended.
    Result(TestResult),
    /// Every test ended.
    Done,
    /// The VM's error: the test file failed to load, a callback threw.
    Error(AppError),
}

/// The test file's VM renders nothing; its errors are the run's.
struct VmSink {
    events: Sender<Event>,
}

impl MountSink for VmSink {
    fn submit(&self, _batch: MountBatch) {}

    fn report(&self, error: &AppError) {
        let _ = self.events.send(Event::Error(error.clone()));
    }
}

/// An app a test opened: closed (`None`) when the test ends.
pub(crate) type OpenApp = Arc<Mutex<Option<Headless>>>;

/// The test file being run, for its VM's calls.
pub(crate) struct Session {
    /// The test file's directory: `open`'s entries are relative to it.
    dir: PathBuf,
    run: TestRun,
    /// The apps built so far, by entry.
    built: Mutex<HashMap<PathBuf, Built>>,
    /// The apps the running test opened, in order.
    apps: Mutex<Vec<OpenApp>>,
    events: Sender<Event>,
}

fn current() -> MutexGuard<'static, Option<Arc<Session>>> {
    static CURRENT: Mutex<Option<Arc<Session>>> = Mutex::new(None);
    CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

impl Session {
    /// The test file being run.
    pub(crate) fn current() -> Result<Arc<Session>, String> {
        current()
            .clone()
            .ok_or_else(|| "nsh/test runs in `nsh test` only".to_owned())
    }

    /// Starts the app at `entry` as `options` say, once it rendered.
    pub(crate) fn open(
        &self,
        entry: &str,
        options: OpenOptions,
    ) -> Result<(OpenApp, Arc<MemoryStorage>), String> {
        let path = self.dir.join(entry);
        let path = dunce::canonicalize(&path).map_err(|e| format!("open({entry:?}): {e}"))?;
        let app = match self.build(&path)? {
            Built::Js(bundle, Some(map)) => nsh_engine::App::Js(bundle.with_source_map(map)),
            Built::Js(bundle, None) => nsh_engine::App::Js(*bundle),
            Built::Rust(root) => nsh_engine::App::Rust(root),
        };
        let network: Arc<dyn nsh_engine::Network> = match options.network {
            Some(NetworkOption::Live) => Arc::new(UreqNetwork::new()),
            Some(NetworkOption::Stub(responses)) => Arc::new(StubNetwork::new(responses)),
            None => Arc::new(StubNetwork::default()),
        };
        let storage = MemoryStorage::new(options.storage.unwrap_or_default().0);
        let files = set_files(&self.run.ui_sets);
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let safe_area = options.safe_area.unwrap_or_default();
        let default = EngineConfig::new(app);
        let config = EngineConfig {
            viewport: nsh_core::Size::new(
                options
                    .width
                    .map_or(default.viewport.width, |w| px(w as f32)),
                options
                    .height
                    .map_or(default.viewport.height, |h| px(h as f32)),
            ),
            trace: self.run.trace.is_some(),
            network: Some(network),
            assets: Some(Arc::new(DirectoryAssets::new(dir, files))),
            storage: Some(storage.clone()),
            environment: Environment {
                safe_area: Edges {
                    top: px(safe_area.top as f32),
                    right: px(safe_area.right as f32),
                    bottom: px(safe_area.bottom as f32),
                    left: px(safe_area.left as f32),
                },
                ..Default::default()
            },
            ui_sets: self.run.ui_sets.clone(),
            ..default
        };
        let mut app = Headless::start(config)
            .map_err(|e| format!("open({entry:?}): the engine did not start: {e}"))?;
        if let Some(out) = &self.run.trace {
            let out = out.clone();
            app.observe(move |step, batch| {
                let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
                print_batch(&mut **out, step, batch);
            });
        }
        // The entry as a terminal shows paths.
        let shown = std::env::current_dir()
            .ok()
            .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.clone());
        app.step(format!("open({})", shown.display()));
        app.first_render()?;
        let app = Arc::new(Mutex::new(Some(app)));
        self.apps
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(app.clone());
        Ok((app, storage))
    }

    /// The app at `path`, built once.
    fn build(&self, path: &Path) -> Result<Built, String> {
        if let Some(built) = self
            .built
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
        {
            return Ok(built.clone());
        }
        let built = (self.run.compile)(path, Entry::App)?;
        self.built
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_path_buf(), built.clone());
        Ok(built)
    }

    /// Closes the apps the test opened: the texts on screen of the last.
    fn close_apps(&self) -> Option<Vec<String>> {
        let apps = std::mem::take(&mut *self.apps.lock().unwrap_or_else(|e| e.into_inner()));
        let mut screen = None;
        for app in apps {
            if let Some(mut app) = app.lock().unwrap_or_else(|e| e.into_inner()).take() {
                app.catch_up();
                screen = Some(
                    app.screen()
                        .texts()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                );
            }
        }
        screen
    }

    fn wants(&self, name: &str) -> bool {
        self.run.filter.as_deref().is_none_or(|f| name.contains(f))
    }
}

/// Runs the tests the test file declared, in order, reporting each (the
/// internal module's `run`, on the VM's thread).
pub(crate) async fn run_tests() {
    let Ok(session) = Session::current() else {
        return;
    };
    for (name, body) in crate::module::take_tests() {
        if !session.wants(&name) {
            continue;
        }
        let start = Instant::now();
        let outcome = within(TEST_TIMEOUT, body.call_async_traced(())).await;
        // The apps go before the next test starts, whatever this one left
        // running.
        let session_ = session.clone();
        let screen = nsh_core::ui::prelude::blocking(move || session_.close_apps())
            .await
            .ok()
            .flatten();
        let failure = match outcome {
            Some(Ok(())) => None,
            Some(Err(thrown)) => Some(Failure {
                message: thrown.message,
                stack: thrown.stack,
                screen,
            }),
            None => Some(Failure {
                message: format!("the test did not end in {} s", TEST_TIMEOUT.as_secs()),
                stack: None,
                screen,
            }),
        };
        let _ = session.events.send(Event::Result(TestResult {
            name,
            elapsed: start.elapsed(),
            failure,
        }));
    }
    let _ = session.events.send(Event::Done);
}

/// `future`'s output, or `None` once `time` passed.
async fn within<T>(time: Duration, future: impl Future<Output = T>) -> Option<T> {
    let mut future = std::pin::pin!(future);
    let mut delay = std::pin::pin!(timer(time));
    std::future::poll_fn(|cx| {
        if let Poll::Ready(value) = future.as_mut().poll(cx) {
            return Poll::Ready(Some(value));
        }
        if delay.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}

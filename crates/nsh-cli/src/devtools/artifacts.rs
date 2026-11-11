//! What `nsh test --artifacts` and `--keep-open-on-failure` keep of the apps a
//! test file opens on a DevTools host: a timeline of the file's steps and
//! console lines, a screenshot of the first failure, and the host held open
//! for a look.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

use base64::Engine as _;
use nsh_devtools::client::Client;
use serde_json::Value;

use super::{cli::Session, timeline::Timeline};
use crate::ui;

/// How long `--keep-open-on-failure` waits for Enter.
const KEEP_OPEN: Duration = Duration::from_secs(10 * 60);
/// How long the failure's screenshot may take.
const SCREENSHOT: Duration = Duration::from_secs(10);

/// The artifacts of a run: where they go, and the file being run.
pub(crate) struct Artifacts {
    /// `--artifacts`: the directory a file's own directory goes in.
    dir: Option<PathBuf>,
    /// The directory the run's test files are found in: a file's own
    /// directory is named after its path from here.
    root: PathBuf,
    /// `--keep-open-on-failure`.
    keep_open: bool,
    /// The test file being run: one at a time in a process.
    current: Mutex<Option<Arc<FileRun>>>,
}

/// A test file's run: its timeline and the hosts its apps are on.
pub(crate) struct FileRun {
    /// `<artifacts>/<file stem>`.
    dir: Option<PathBuf>,
    /// Records the steps and console lines (with `--artifacts`).
    pub(crate) timeline: Option<Timeline>,
    hosts: Mutex<Vec<LiveHost>>,
    next: AtomicU64,
    /// A failure's screenshot was taken for this file already.
    captured: AtomicBool,
}

/// A host an app of the file runs on.
struct LiveHost {
    id: u64,
    client: Arc<Client>,
    url: String,
    token: String,
}

impl Artifacts {
    /// Keeps what `dir` (`--artifacts`) and `keep_open`
    /// (`--keep-open-on-failure`) ask for, of the test files found in `root`.
    pub(crate) fn new(dir: Option<PathBuf>, root: PathBuf, keep_open: bool) -> Artifacts {
        Artifacts {
            dir,
            root,
            keep_open,
            current: Mutex::new(None),
        }
    }

    /// The test file `file` starts: its apps' hosts record into its
    /// directory (`<artifacts>/<the file's path from the root, sanitized>`),
    /// cleared of what an earlier run left in it.
    pub(crate) fn begin(&self, file: &Path) {
        let dir = self
            .dir
            .as_ref()
            .map(|dir| dir.join(file_dir_name(&self.root, file)));
        if let Some(dir) = &dir {
            if let Err(e) = std::fs::create_dir_all(dir) {
                ui::warn(format!("cannot make {}: {e}", ui::path(dir)));
            }
            for name in ["timeline.jsonl", "failure.png"] {
                let _ = std::fs::remove_file(dir.join(name));
            }
        }
        let run = FileRun {
            timeline: dir.is_some().then(Timeline::start),
            dir,
            hosts: Mutex::new(Vec::new()),
            next: AtomicU64::new(0),
            captured: AtomicBool::new(false),
        };
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(run));
    }

    /// The file being run, for the host an app of it starts.
    pub(crate) fn current(&self) -> Option<Arc<FileRun>> {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The file ended: its timeline is written.
    pub(crate) fn end(&self) {
        let Some(run) = self
            .current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        else {
            return;
        };
        if let (Some(dir), Some(timeline)) = (&run.dir, &run.timeline) {
            let path = dir.join("timeline.jsonl");
            if let Err(e) = timeline.write(&path) {
                ui::warn(format!("cannot write {}: {e}", ui::path(&path)));
            }
        }
    }

    /// A test of the file failed and its apps are still open: keeps the
    /// first failure's screenshot, and holds the hosts open when asked.
    pub(crate) fn failed(&self) {
        let Some(run) = self.current() else {
            return;
        };
        if let Some(dir) = &run.dir {
            run.screenshot(&dir.join("failure.png"));
        }
        if self.keep_open {
            run.keep_open();
        }
    }
}

impl FileRun {
    /// Registers an app's host; the id removes it again ([`FileRun::remove`]).
    pub(crate) fn add(&self, client: Arc<Client>, url: String, token: String) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.hosts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(LiveHost {
                id,
                client,
                url,
                token,
            });
        id
    }

    /// The host `id` went.
    pub(crate) fn remove(&self, id: u64) {
        self.hosts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|h| h.id != id);
    }

    /// Saves the newest open app's window as `path`, for the file's first
    /// failure.
    fn screenshot(&self, path: &Path) {
        if self.captured.swap(true, Ordering::AcqRel) {
            return;
        }
        let client = self
            .hosts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last()
            .map(|h| h.client.clone());
        let Some(client) = client else {
            return;
        };
        let saved = client
            .call_within("Capture.screenshot", Value::Null, SCREENSHOT)
            .map_err(|e| e.message)
            .and_then(|png| {
                let png = png.as_str().ok_or("the host sent no picture")?.to_owned();
                base64::engine::general_purpose::STANDARD
                    .decode(png)
                    .map_err(|e| e.to_string())
            })
            .and_then(|png| std::fs::write(path, png).map_err(|e| e.to_string()));
        if let Err(e) = saved {
            ui::warn(format!("no failure screenshot at {}: {e}", ui::path(path)));
        }
    }

    /// Says where the open hosts are, and waits for Enter on standard input
    /// or [`KEEP_OPEN`].
    fn keep_open(&self) {
        let hosts = self.hosts.lock().unwrap_or_else(|e| e.into_inner());
        if hosts.is_empty() {
            return;
        }
        for host in hosts.iter() {
            // The run's own client holds the input lock for as long as it
            // is connected: let go, so `nsh devtools press` or an MCP
            // client can act on the kept host.
            release_input(&*host.client);
            ui::plain(format!(
                "nsh devtools: kept {} open; token {}",
                host.url, host.token
            ));
        }
        drop(hosts);
        ui::note("press Enter to close it and go on (or wait 10 minutes)");
        let (tx, rx) = mpsc::channel();
        // A thread that outlives the wait (a read cannot be cancelled) ends
        // with the process.
        let _ = std::thread::Builder::new()
            .name("nsh-keep-open".into())
            .spawn(move || {
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                let _ = tx.send(());
            });
        let _ = rx.recv_timeout(KEEP_OPEN);
    }
}

/// Lets go of the input `session` holds (the answer does not matter: a
/// session that holds nothing says so).
fn release_input(session: &dyn Session) {
    let _ = session.call_within("Session.release", Value::Null, SCREENSHOT);
}

/// The name of the directory a test file's artifacts go in: its path from
/// `root`, its separators and anything a file name should not have made `_`
/// (`a/App.test.tsx` is `a__App.test.tsx`), so files of one name in
/// different directories keep apart.
fn file_dir_name(root: &Path, file: &Path) -> String {
    let relative = file.strip_prefix(root).unwrap_or(file);
    let parts: Vec<String> = relative
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let name = parts.join("__");
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "tests".to_owned()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use nsh_devtools::protocol::ErrorBody;

    use super::*;

    struct Recorder(Mutex<Vec<String>>);

    impl Session for Recorder {
        fn call_within(&self, method: &str, _: Value, _: Duration) -> Result<Value, ErrorBody> {
            self.0.lock().unwrap().push(method.into());
            Ok(Value::Null)
        }
    }

    #[test]
    fn a_kept_host_gives_its_input_back() {
        let session = Recorder(Mutex::new(Vec::new()));
        release_input(&session);
        assert_eq!(*session.0.lock().unwrap(), ["Session.release"]);
    }

    #[test]
    fn files_of_one_name_keep_their_own_directories() {
        let root = Path::new("/app");
        assert_eq!(
            file_dir_name(root, Path::new("/app/a/App.test.tsx")),
            "a__App.test.tsx"
        );
        assert_eq!(
            file_dir_name(root, Path::new("/app/b/App.test.tsx")),
            "b__App.test.tsx"
        );
        assert_eq!(
            file_dir_name(root, Path::new("/app/x y.test.ts")),
            "x_y.test.ts"
        );
    }
}

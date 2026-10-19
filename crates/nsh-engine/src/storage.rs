//! The app's storage in a file (`nsh/storage` on the native platforms):
//! its entries as one JSON object, rewritten whole (to a temporary file,
//! then renamed over it) on a thread of its own after changes, so a crash
//! leaves the old file or the new one. One instance per file in the
//! process ([`FileStorage::open`]), which keeps the entries in memory: an
//! app reloaded before its writes reached the disk reads them all the same.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    thread::JoinHandle,
};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::storage::{StorageBackend, StorageChange};

/// An app's storage kept in `storage.json` of a directory: the app's data
/// directory ([`app_data_dir`]), or one the embedding app chooses.
pub struct FileStorage {
    path: PathBuf,
    /// The entries as changed so far, once read.
    entries: Mutex<Option<BTreeMap<String, String>>>,
    writer: Mutex<Option<(Sender<StorageChange>, JoinHandle<()>)>>,
}

impl FileStorage {
    /// The storage in `dir/storage.json`: the one already open in this
    /// process, or a new one (the directory is made on the first write).
    pub fn open(dir: impl Into<PathBuf>) -> Arc<Self> {
        static OPEN: Mutex<Vec<Weak<FileStorage>>> = Mutex::new(Vec::new());
        let path = dir.into().join("storage.json");
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        open.retain(|s| s.strong_count() > 0);
        if let Some(storage) = open
            .iter()
            .filter_map(Weak::upgrade)
            .find(|s| s.path == path)
        {
            return storage;
        }
        let storage = Arc::new(Self {
            path,
            entries: Mutex::new(None),
            writer: Mutex::new(None),
        });
        open.push(Arc::downgrade(&storage));
        storage
    }

    /// The file it keeps the entries in.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The directory an app keeps its data in, by its id (`com.example.app`):
/// Application Support on Apple platforms, `%APPDATA%` on Windows,
/// `$XDG_DATA_HOME` (`~/.local/share`) elsewhere. `None` on Android, whose
/// files directory only the platform knows, and without a home directory.
pub fn app_data_dir(app_id: &str) -> Option<PathBuf> {
    let env = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(target_os = "android") {
        None
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        env("HOME").map(|home| home.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local/share")))
    }?;
    Some(base.join(app_id))
}

fn read(path: &Path) -> Result<BTreeMap<String, String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write(path: &Path, entries: &BTreeMap<String, String>) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("json.tmp");
    let file = std::fs::File::create(&temporary)?;
    let mut out = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut out, entries)?;
    let file = out.into_inner().map_err(|e| e.into_error())?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)
}

fn apply(entries: &mut BTreeMap<String, String>, change: StorageChange) {
    match change {
        StorageChange::Set(key, value) => {
            entries.insert(key, value);
        }
        StorageChange::Remove(key) => {
            entries.remove(&key);
        }
        StorageChange::Clear => entries.clear(),
    }
}

/// Applies the changes as they come, writing the file once per burst.
fn run_writer(path: PathBuf, changes: Receiver<StorageChange>) {
    let mut entries = read(&path).unwrap_or_default();
    while let Ok(change) = changes.recv() {
        apply(&mut entries, change);
        changes
            .try_iter()
            .for_each(|change| apply(&mut entries, change));
        if let Err(e) = write(&path, &entries) {
            log::error!(target: "js", "the app's storage could not be written to {}: {e}", path.display());
        }
    }
}

impl StorageBackend for FileStorage {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entries) = &*entries {
            return Ok(entries.clone());
        }
        let read = read(&self.path)?;
        *entries = Some(read.clone());
        Ok(read)
    }

    fn write(&self, change: StorageChange) {
        if let Some(entries) = &mut *self.entries.lock().unwrap_or_else(|e| e.into_inner()) {
            apply(entries, change.clone());
        }
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let (changes, _) = writer.get_or_insert_with(|| {
            let (tx, rx) = crossbeam_channel::unbounded();
            let path = self.path.clone();
            let thread = std::thread::Builder::new()
                .name("nsh-storage".into())
                .spawn(move || run_writer(path, rx))
                .expect("the storage thread starts");
            (tx, thread)
        });
        // The writer only stops when this is dropped.
        let _ = changes.send(change);
    }
}

impl Drop for FileStorage {
    /// Waits for the last changes to be written.
    fn drop(&mut self) {
        let writer = self
            .writer
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some((changes, thread)) = writer {
            drop(changes);
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_entries_across_instances() {
        let dir = std::env::current_dir()
            .unwrap()
            .join("../../target/agents/storage-test")
            .join(std::process::id().to_string());
        let _ = std::fs::remove_dir_all(&dir);
        let storage = FileStorage::open(&dir);
        assert!(storage.load().unwrap().is_empty());
        storage.write(StorageChange::Set("a".into(), "1".into()));
        storage.write(StorageChange::Set("b".into(), "2".into()));
        // A reload while the first is open shares its entries, written or not.
        let reloaded = FileStorage::open(&dir);
        assert!(Arc::ptr_eq(&storage, &reloaded));
        storage.write(StorageChange::Remove("a".into()));
        assert_eq!(reloaded.load().unwrap().len(), 1);
        drop((storage, reloaded));
        let again = FileStorage::open(&dir);
        assert_eq!(
            again.load().unwrap(),
            BTreeMap::from([("b".to_owned(), "2".to_owned())])
        );
        again.write(StorageChange::Clear);
        drop(again);
        assert!(FileStorage::open(&dir).load().unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

//! An app's key-value storage (`nsh/storage`): strings by string key,
//! kept on the device by the platform's [`StorageBackend`] (a file in the
//! app's data directory, the browser's `localStorage`).
//!
//! The core keeps the entries in memory, loaded from the backend on first
//! use, so reads never touch the disk; every change is handed to the
//! backend, which writes it where it may block (the file backend on a
//! thread of its own). The quota and the `storage` capability are checked
//! here, in Rust.

use std::{collections::BTreeMap, sync::Arc};

/// The most an app may keep, in UTF-8 bytes of its keys and values.
pub const QUOTA: usize = 5 * 1024 * 1024;

/// A change to the entries, for the backend to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageChange {
    /// `key` now holds `value`.
    Set(String, String),
    /// `key` was removed.
    Remove(String),
    /// Every entry was removed.
    Clear,
}

/// Where an app's entries are kept: the platform's.
pub trait StorageBackend: Send + Sync {
    /// The entries kept, read once when the app first uses its storage; an
    /// error leaves the app with none (logged).
    fn load(&self) -> Result<BTreeMap<String, String>, String>;
    /// Keeps `change`. Called on the engine thread: a backend that blocks
    /// writes elsewhere, in order.
    fn write(&self, change: StorageChange);
}

/// Names the backend only, so that platforms' options holding one can
/// derive `Debug`.
impl std::fmt::Debug for dyn StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StorageBackend")
    }
}

/// An app's entries: the engine's, shared with the module functions.
pub(crate) struct Storage {
    backend: Option<Arc<dyn StorageBackend>>,
    allowed: bool,
    entries: Option<BTreeMap<String, String>>,
    bytes: usize,
}

impl Storage {
    /// Entries kept by `backend` (`None`: in memory, for the engine's
    /// life), when `allowed` (the `storage` capability).
    pub(crate) fn new(backend: Option<Arc<dyn StorageBackend>>, allowed: bool) -> Self {
        Self {
            backend,
            allowed,
            entries: None,
            bytes: 0,
        }
    }

    fn entries(&mut self) -> Result<&mut BTreeMap<String, String>, String> {
        if !self.allowed {
            return Err(
                "storage is not granted: add `\"storage\": true` to capabilities in nsh.json"
                    .into(),
            );
        }
        let backend = &self.backend;
        let bytes = &mut self.bytes;
        Ok(self.entries.get_or_insert_with(|| {
            let entries = match backend.as_ref().map(|b| b.load()) {
                Some(Ok(entries)) => entries,
                Some(Err(e)) => {
                    log::error!(target: "js", "the app's storage could not be read: {e}");
                    BTreeMap::new()
                }
                None => BTreeMap::new(),
            };
            *bytes = entries.iter().map(|(k, v)| k.len() + v.len()).sum();
            entries
        }))
    }

    fn write(&self, change: StorageChange) {
        if let Some(backend) = &self.backend {
            backend.write(change);
        }
    }

    /// The value of `key`.
    pub(crate) fn get(&mut self, key: &str) -> Result<Option<String>, String> {
        Ok(self.entries()?.get(key).cloned())
    }

    /// Sets `key` to `value`; fails past [`QUOTA`].
    pub(crate) fn set(&mut self, key: String, value: String) -> Result<(), String> {
        let bytes = self.bytes;
        let entries = self.entries()?;
        let old = entries.get(&key).map_or(0, |v| key.len() + v.len());
        let new = bytes - old + key.len() + value.len();
        if new > QUOTA {
            return Err(format!(
                "storage is full: {new} bytes would exceed the quota of {QUOTA}"
            ));
        }
        if entries.get(&key) == Some(&value) {
            return Ok(());
        }
        entries.insert(key.clone(), value.clone());
        self.bytes = new;
        self.write(StorageChange::Set(key, value));
        Ok(())
    }

    /// Removes `key`.
    pub(crate) fn remove(&mut self, key: &str) -> Result<(), String> {
        let Some(value) = self.entries()?.remove(key) else {
            return Ok(());
        };
        self.bytes -= key.len() + value.len();
        self.write(StorageChange::Remove(key.to_owned()));
        Ok(())
    }

    /// Every key, in order.
    pub(crate) fn keys(&mut self) -> Result<Vec<String>, String> {
        Ok(self.entries()?.keys().cloned().collect())
    }

    /// Removes every entry.
    pub(crate) fn clear(&mut self) -> Result<(), String> {
        let entries = self.entries()?;
        if entries.is_empty() {
            return Ok(());
        }
        entries.clear();
        self.bytes = 0;
        self.write(StorageChange::Clear);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Recorder {
        kept: BTreeMap<String, String>,
        changes: Mutex<Vec<StorageChange>>,
    }

    impl StorageBackend for Recorder {
        fn load(&self) -> Result<BTreeMap<String, String>, String> {
            Ok(self.kept.clone())
        }

        fn write(&self, change: StorageChange) {
            self.changes.lock().unwrap().push(change);
        }
    }

    #[test]
    fn loads_once_and_writes_changes() {
        let backend = Arc::new(Recorder {
            kept: BTreeMap::from([("a".into(), "1".into())]),
            ..Recorder::default()
        });
        let mut storage = Storage::new(Some(backend.clone()), true);
        assert_eq!(storage.get("a").unwrap().as_deref(), Some("1"));
        storage.set("b".into(), "2".into()).unwrap();
        storage.set("b".into(), "2".into()).unwrap();
        storage.remove("a").unwrap();
        storage.remove("missing").unwrap();
        assert_eq!(storage.keys().unwrap(), ["b"]);
        storage.clear().unwrap();
        assert_eq!(
            *backend.changes.lock().unwrap(),
            [
                StorageChange::Set("b".into(), "2".into()),
                StorageChange::Remove("a".into()),
                StorageChange::Clear,
            ]
        );
    }

    #[test]
    fn checks_the_quota_and_the_capability() {
        let mut storage = Storage::new(None, true);
        storage.set("k".into(), "x".repeat(QUOTA - 1)).unwrap();
        assert!(storage.set("j".into(), "y".into()).is_err());
        // Replacing a value counts the new one only.
        storage.set("k".into(), "z".repeat(QUOTA - 1)).unwrap();
        let mut denied = Storage::new(None, false);
        assert!(denied.get("k").unwrap_err().contains("\"storage\": true"));
    }
}

//! `nsh/storage`: the app's key-value storage, strings by string key kept
//! on the device (`crate::storage`).

use crate::ui::{ambient::with_storage, module};

/// The app's key-value storage.
pub(crate) struct Storage;

/// The app's key-value storage: strings by string key, kept on the device
/// across launches (up to 5 MB). Calls run in order, so a read after a
/// write sees it.
#[module]
impl Storage {
    /// The value of `key`, `null` when it has none.
    pub(crate) async fn get_item(key: String) -> Result<Option<String>, String> {
        with_storage(|s| s.get(&key))
    }

    /// Sets `key` to `value`; rejects when the storage would be full.
    pub(crate) async fn set_item(key: String, value: String) -> Result<(), String> {
        with_storage(|s| s.set(key, value))
    }

    /// Removes `key`.
    pub(crate) async fn remove_item(key: String) -> Result<(), String> {
        with_storage(|s| s.remove(&key))
    }

    /// Every key, in order.
    pub(crate) async fn get_all_keys() -> Result<Vec<String>, String> {
        with_storage(|s| s.keys())
    }

    /// Removes every key.
    pub(crate) async fn clear() -> Result<(), String> {
        with_storage(|s| s.clear())
    }
}

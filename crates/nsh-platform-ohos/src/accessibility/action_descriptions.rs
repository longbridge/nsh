//! Immutable action labels outlive ArkUI's shallow copies, which have no retirement ACK.
//! The process budget bounds C-string bytes and entries, not allocator or tree overhead.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString},
    sync::{Mutex, OnceLock},
};

const MAX_BYTES: usize = 4 << 20;
const MAX_ENTRIES: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    InteriorNul,
    BudgetExceeded,
    LockPoisoned,
}

#[derive(Default)]
pub(crate) struct ActionDescriptions {
    values: BTreeMap<&'static str, &'static CStr>,
    bytes: usize,
}

impl ActionDescriptions {
    pub(crate) fn intern(values: &[&str]) -> Result<Vec<&'static CStr>, Error> {
        static LABELS: OnceLock<Mutex<ActionDescriptions>> = OnceLock::new();
        LABELS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| Error::LockPoisoned)?
            .insert(values)
    }

    fn insert(&mut self, values: &[&str]) -> Result<Vec<&'static CStr>, Error> {
        if values.iter().any(|value| value.as_bytes().contains(&0)) {
            return Err(Error::InteriorNul);
        }
        let mut missing = BTreeSet::new();
        let mut bytes = self.bytes;
        for value in values.iter().copied() {
            if self.values.contains_key(value) || !missing.insert(value) {
                continue;
            }
            if self
                .values
                .len()
                .checked_add(missing.len())
                .is_none_or(|count| count > MAX_ENTRIES)
            {
                return Err(Error::BudgetExceeded);
            }
            bytes = bytes
                .checked_add(value.len())
                .and_then(|total| total.checked_add(1))
                .filter(|total| *total <= MAX_BYTES)
                .ok_or(Error::BudgetExceeded)?;
        }
        // Validate and prepare the whole batch before publishing any permanent pointer.
        let additions = missing
            .into_iter()
            .map(|value| CString::new(value).map_err(|_| Error::InteriorNul))
            .collect::<Result<Vec<_>, _>>()?;
        for value in additions {
            let retained: &'static CStr = Box::leak(value.into_boxed_c_str());
            let key = retained
                .to_str()
                .expect("labels originated as UTF-8 strings");
            self.values.insert(key, retained);
        }
        self.bytes = bytes;
        Ok(values.iter().map(|value| self.values[value]).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_labels_share_one_allocation() {
        let mut labels = ActionDescriptions::default();
        let first = labels.insert(&["Increase", "Increase"]).unwrap();
        let later = labels.insert(&["Increase"]).unwrap();
        assert!(std::ptr::eq(first[0], first[1]));
        assert!(std::ptr::eq(first[0], later[0]));
        assert_eq!(labels.values.len(), 1);
        assert_eq!(labels.bytes, "Increase".len() + 1);
    }

    #[test]
    fn old_dynamic_labels_survive_updates_and_store_drop() {
        let first = {
            let mut labels = ActionDescriptions::default();
            let first = labels.insert(&["增加音量"]).unwrap()[0];
            let second = labels.insert(&["降低音量"]).unwrap()[0];
            assert_ne!(first, second);
            assert_eq!(first.to_str().unwrap(), "增加音量");
            first
        };
        assert_eq!(first.to_str().unwrap(), "增加音量");
    }

    #[test]
    fn nul_batch_does_not_publish_earlier_valid_labels() {
        let mut labels = ActionDescriptions::default();
        let old = labels.insert(&["Keep"]).unwrap()[0];
        let bytes = labels.bytes;
        assert_eq!(
            labels.insert(&["New", "invalid\0label"]),
            Err(Error::InteriorNul)
        );
        assert_eq!(labels.values.len(), 1);
        assert_eq!(labels.bytes, bytes);
        assert_eq!(old, c"Keep");
    }

    #[test]
    fn byte_overflow_does_not_publish_a_partial_batch() {
        let mut labels = ActionDescriptions::default();
        labels.insert(&["Keep"]).unwrap();
        let bytes = labels.bytes;
        assert_eq!(
            labels.insert(&["New", &"x".repeat(MAX_BYTES)]),
            Err(Error::BudgetExceeded)
        );
        assert_eq!(labels.values.len(), 1);
        assert_eq!(labels.bytes, bytes);
    }

    #[test]
    fn byte_budget_counts_nul_and_allows_reusing_a_full_store() {
        let mut labels = ActionDescriptions::default();
        let value = "x".repeat(MAX_BYTES - 1);
        let old = labels.insert(&[&value]).unwrap()[0];
        assert_eq!(labels.bytes, MAX_BYTES);
        assert!(std::ptr::eq(old, labels.insert(&[&value]).unwrap()[0]));
        assert_eq!(labels.insert(&[""]), Err(Error::BudgetExceeded));
        assert_eq!(labels.values.len(), 1);
        assert_eq!(labels.bytes, MAX_BYTES);
    }

    #[test]
    fn entry_overflow_does_not_add_a_label() {
        let mut labels = ActionDescriptions::default();
        let values: Vec<_> = (0..MAX_ENTRIES)
            .map(|index| format!("label-{index}"))
            .collect();
        let values: Vec<_> = values.iter().map(String::as_str).collect();
        labels.insert(&values).unwrap();
        let bytes = labels.bytes;
        assert!(labels.insert(&["label-0", "label-0"]).is_ok());
        assert_eq!(labels.insert(&["New"]), Err(Error::BudgetExceeded));
        assert_eq!(labels.values.len(), MAX_ENTRIES);
        assert_eq!(labels.bytes, bytes);
    }

    #[test]
    fn process_store_reuses_labels_across_batches() {
        let a = ActionDescriptions::intern(&["Lower brightness"]).unwrap();
        let b = ActionDescriptions::intern(&["Lower brightness"]).unwrap();
        assert!(std::ptr::eq(a[0], b[0]));
    }
}

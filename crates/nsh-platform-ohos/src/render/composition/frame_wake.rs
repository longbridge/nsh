//! Token routing avoids dereferencing userdata after NativeImage's non-draining unset.
use std::{
    collections::BTreeMap,
    ffi::c_void,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicUsize, Ordering},
    },
};

use ohos_native_image_binding::FrameAvailableListener;

type Wake = dyn Fn() + Send + Sync;
static NEXT: AtomicUsize = AtomicUsize::new(1);
static ROUTES: OnceLock<Mutex<BTreeMap<usize, Weak<Wake>>>> = OnceLock::new();
fn routes() -> &'static Mutex<BTreeMap<usize, Weak<Wake>>> {
    ROUTES.get_or_init(Mutex::default)
}

pub(super) struct FrameWake {
    token: usize,
    _wake: Arc<Wake>,
}
impl FrameWake {
    pub(super) fn new(wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let token = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .ok()?;
        let wake: Arc<Wake> = Arc::new(wake);
        routes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(token, Arc::downgrade(&wake));
        Some(Self { token, _wake: wake })
    }
    pub(super) fn listener(&self) -> FrameAvailableListener {
        FrameAvailableListener {
            context: self.token as *mut c_void,
            onFrameAvailable: Some(available),
        }
    }
}
impl Drop for FrameWake {
    fn drop(&mut self) {
        routes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.token);
    }
}
unsafe extern "C" fn available(context: *mut c_void) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let wake = {
            routes()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&(context as usize))
                .and_then(Weak::upgrade)
        };
        if let Some(wake) = wake {
            wake();
        }
    }));
}
#[cfg(test)]
mod tests {
    use super::*;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    #[test]
    fn late_callbacks_do_not_touch_retired_or_reused_owners() {
        let _serial = TEST_LOCK.lock().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let output = count.clone();
        let first = FrameWake::new(move || {
            output.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
        let old = first.listener();
        // SAFETY: the callback treats context solely as a non-dereferenced integer token.
        unsafe { available(old.context) };
        assert_eq!(count.load(Ordering::Relaxed), 1);
        drop(first);
        let next = FrameWake::new(|| {}).unwrap();
        assert_ne!(old.context, next.listener().context);
        // SAFETY: the callback treats context solely as a non-dereferenced integer token.
        unsafe { available(old.context) };
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn wake_runs_outside_route_lock() {
        let _serial = TEST_LOCK.lock().unwrap();
        let unlocked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = unlocked.clone();
        let wake =
            FrameWake::new(move || observed.store(routes().try_lock().is_ok(), Ordering::Relaxed))
                .unwrap();
        // SAFETY: this live registration supplies its own integer context token.
        unsafe { available(wake.listener().context) };
        assert!(unlocked.load(Ordering::Relaxed));
    }
}

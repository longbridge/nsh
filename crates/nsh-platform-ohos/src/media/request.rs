//! Shared image work with independently cancellable consumers.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct Waiter<T> {
    pub(crate) target: T,
    pub(crate) cancelled: Arc<AtomicBool>,
}
pub(crate) struct Request<T> {
    pub(crate) waiters: Mutex<Vec<Waiter<T>>>,
    stopped: AtomicBool,
}
impl<T: Copy> Request<T> {
    pub(crate) fn new(target: T, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            waiters: Mutex::new(vec![Waiter { target, cancelled }]),
            stopped: AtomicBool::new(false),
        }
    }
    pub(crate) fn join(
        &self,
        target: T,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<bool, &'static str> {
        let mut waiters = self.waiters.lock().expect("image waiters");
        if self.stopped.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if waiters.len() >= 4096 {
            return Err("image request exceeds 4096 consumers");
        }
        waiters.push(Waiter {
            target,
            cancelled: cancelled.clone(),
        });
        Ok(true)
    }
    pub(crate) fn cancelled(&self) -> bool {
        if self.stopped.load(Ordering::Relaxed) {
            return true;
        }
        let waiters = self.waiters.lock().expect("image waiters");
        let cancelled = waiters
            .iter()
            .all(|waiter| waiter.cancelled.load(Ordering::Relaxed));
        if cancelled {
            self.stopped.store(true, Ordering::Relaxed);
        }
        cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelling_one_consumer_keeps_the_shared_download_alive() {
        let first = Arc::new(AtomicBool::new(false));
        let second = Arc::new(AtomicBool::new(false));
        let request = Request::new(1, first.clone());
        assert_eq!(request.join(2, &second), Ok(true));
        first.store(true, Ordering::Relaxed);
        assert!(!request.cancelled());
        let waiters = request.waiters.lock().unwrap();
        assert_eq!(waiters[1].target, 2);
        assert!(!waiters[1].cancelled.load(Ordering::Relaxed));
    }
    #[test]
    fn new_consumers_cannot_revive_a_cancelled_download() {
        let first = Arc::new(AtomicBool::new(true));
        let request = Request::new(1, first);
        assert!(request.cancelled());
        let next = Arc::new(AtomicBool::new(false));
        assert_eq!(request.join(2, &next), Ok(false));
        assert!(!Request::new(2, next).cancelled());
    }
}

//! Bounded blocking IO lanes. Slow fetch bodies cannot starve image or WebView assets.
use std::sync::{Arc, Mutex, OnceLock, mpsc};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct Executor {
    sender: mpsc::SyncSender<Job>,
}
impl Executor {
    fn new(name: &str, workers: usize, capacity: usize) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..workers {
            let receiver = receiver.clone();
            std::thread::Builder::new()
                .name(format!("{name}-{index}"))
                .spawn(move || {
                    loop {
                        let job = receiver.lock().expect("IO queue").recv();
                        let Ok(job) = job else { break };
                        // A malformed request must not permanently remove a worker from the lane.
                        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                            log::error!("OHOS IO job panicked");
                        }
                    }
                })?;
        }
        Ok(Self { sender })
    }
    pub(crate) fn fetch() -> Result<&'static Self, String> {
        static POOL: OnceLock<std::io::Result<Executor>> = OnceLock::new();
        POOL.get_or_init(|| Self::new("nsh-fetch", 6, 256))
            .as_ref()
            .map_err(ToString::to_string)
    }
    pub(crate) fn images() -> Result<&'static Self, String> {
        static POOL: OnceLock<std::io::Result<Executor>> = OnceLock::new();
        POOL.get_or_init(|| Self::new("nsh-image", 4, 256))
            .as_ref()
            .map_err(ToString::to_string)
    }
    pub(crate) fn assets() -> Result<&'static Self, String> {
        static POOL: OnceLock<std::io::Result<Executor>> = OnceLock::new();
        POOL.get_or_init(|| Self::new("nsh-web-asset", 2, 256))
            .as_ref()
            .map_err(ToString::to_string)
    }
    pub(crate) fn submit(&self, job: impl FnOnce() + Send + 'static) -> Result<(), String> {
        // Admission never waits on the UI or engine thread. Dropping a rejected job
        // releases its cancellation/response guards just like a cancelled request.
        self.sender
            .try_send(Box::new(job))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "IO queue is full".into(),
                mpsc::TrySendError::Disconnected(_) => "IO workers stopped".into(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_admission_does_not_wait_for_a_busy_worker() {
        let pool = Executor::new("test-io", 1, 1).unwrap();
        let (started, running) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        pool.submit(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
        })
        .unwrap();
        running.recv().unwrap();
        pool.submit(|| {}).unwrap();
        assert!(pool.submit(|| {}).unwrap_err().contains("full"));
        release.send(()).unwrap();
    }
    #[test]
    fn separate_lanes_are_reused() {
        assert!(std::ptr::eq(
            Executor::fetch().unwrap(),
            Executor::fetch().unwrap()
        ));
        assert!(!std::ptr::eq(
            Executor::fetch().unwrap(),
            Executor::assets().unwrap()
        ));
        assert!(Executor::images().is_ok());
    }
}

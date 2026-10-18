//! A monotonic clock that also works in WebAssembly.
//!
//! `std::time::Instant::now` panics on `wasm32-unknown-unknown`, which has
//! no clock of its own. Off WebAssembly [`Instant`] is the standard one; in
//! WebAssembly it counts from the page's time origin (`performance.now()`,
//! the `nsh.now_ms` import the web runtime provides).

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub use self::wasm::Instant;

/// Milliseconds since the Unix epoch (wall clock: `Date.now()`).
#[cfg(not(target_arch = "wasm32"))]
pub fn unix_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

/// Milliseconds since the Unix epoch (wall clock: `Date.now()`).
#[cfg(target_arch = "wasm32")]
pub fn unix_ms() -> f64 {
    #[link(wasm_import_module = "nsh")]
    unsafe extern "C" {
        fn date_now_ms() -> f64;
    }
    // SAFETY: a pure host function without arguments.
    unsafe { date_now_ms() }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::{
        ops::{Add, AddAssign, Sub, SubAssign},
        time::Duration,
    };

    #[link(wasm_import_module = "nsh")]
    unsafe extern "C" {
        /// Milliseconds since the page's time origin (`performance.now()`).
        fn now_ms() -> f64;
    }

    /// A point in time, from the page's time origin.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant(Duration);

    impl Instant {
        /// Now.
        pub fn now() -> Instant {
            // SAFETY: a pure host function without arguments.
            let ms = unsafe { now_ms() };
            Instant(Duration::from_secs_f64(ms.max(0.0) / 1000.0))
        }

        /// The time from `earlier` to `self` (zero if `earlier` is later).
        pub fn duration_since(&self, earlier: Instant) -> Duration {
            self.0.saturating_sub(earlier.0)
        }

        /// As [`Instant::duration_since`].
        pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
            self.duration_since(earlier)
        }

        /// The time since `self`.
        pub fn elapsed(&self) -> Duration {
            Instant::now().duration_since(*self)
        }

        /// `self + duration`, if representable.
        pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
            self.0.checked_add(duration).map(Instant)
        }

        /// `self - duration`, if representable.
        pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
            self.0.checked_sub(duration).map(Instant)
        }
    }

    impl Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, rhs: Duration) -> Instant {
            Instant(self.0 + rhs)
        }
    }

    impl AddAssign<Duration> for Instant {
        fn add_assign(&mut self, rhs: Duration) {
            self.0 += rhs;
        }
    }

    impl Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, rhs: Duration) -> Instant {
            Instant(self.0.saturating_sub(rhs))
        }
    }

    impl SubAssign<Duration> for Instant {
        fn sub_assign(&mut self, rhs: Duration) {
            self.0 = self.0.saturating_sub(rhs);
        }
    }

    impl Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, rhs: Instant) -> Duration {
            self.duration_since(rhs)
        }
    }
}

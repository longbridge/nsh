//! Stand-ins native modules give headless runs for what only a device
//! does: a microphone that hears the user, a camera, a push service.
//!
//! A module registers its [`Fake`] by name from its `#[native_module]`
//! entry ([`Registry::fake`](crate::element::Registry::fake)). Nothing uses
//! it in an app: only the headless driver (`nsh test`, `nsh_testing`'s
//! `Headless`) does, which installs the fakes of the app's sets before the
//! app starts, so the module answers in place of the platform, and routes a
//! test's calls to them by name (TS: `app.fake(name, method, ...args)`).
//!
//! A fake usually installs itself through the module's own backend hook
//! (`nsh_extra::speech::set_backend`), the one an embedding app gives its
//! own implementation through: the module runs the same code with a test
//! as with a device, and only what the backend answers differs. Every
//! module of `nsh-extra` with a backend hook (the camera, push) can give
//! one the same way.

use crate::JsonValue;

/// A native module's stand-in for headless runs
/// ([`Registry::fake`](crate::element::Registry::fake)).
pub trait Fake: Send + Sync {
    /// Takes the platform's place from now on: the headless driver calls
    /// it when it starts an app whose sets register the fake, before the
    /// app runs, and again for each app it starts. Process-wide state is
    /// reset here (what an earlier app left).
    fn install(&self);

    /// What a test asks of it: `method` with `args`, the test's arguments
    /// as JSON; the answer as JSON (`null` for none), or why it failed,
    /// which fails the test's step.
    fn call(&self, method: &str, args: &[JsonValue]) -> Result<JsonValue, String>;
}

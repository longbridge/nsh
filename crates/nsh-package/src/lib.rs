//! Signed app packages and the trust that verifies them.
//!
//! # Keys: two levels
//!
//! A **root key** stays offline; apps embedding nsh trust its public key
//! (several roots may be trusted, which is how roots rotate). A root
//! certifies **release keys** ([`certify`]): a [`Certificate`] names the
//! release key, the time window it may sign in and the app ids it may sign
//! for (`com.longbridge.*`). Packages are signed by a release key and carry
//! its certificate, so release keys rotate without updating the apps that
//! trust the root. Every key id is derived from its public key
//! ([`PublicKey::id`]).
//!
//! # Revocation and rollback
//!
//! A root also signs **revocation lists** ([`Revocations`]): release keys and
//! package hashes that must not load any more. Each list has a sequence
//! number; a trust store only accepts a list newer than the one it has, so an
//! old list cannot be replayed to un-revoke something. A trust store also
//! remembers the highest build it accepted per app id and refuses older
//! builds (an attacker cannot serve an older, vulnerable signed package).
//! With a state directory ([`TrustStore::open`]) both survive restarts.
//!
//! # Package format (`.nshpkg`)
//!
//! All integers little-endian.
//!
//! ```text
//! offset  size  field
//! 0       4     magic "NSHP"
//! 4       2     format version (1)
//! 6       2     flags (0)
//! 8       4     H: header length
//! 12      H     header (UTF-8 JSON, see [`Header`])
//! 12+H    B     the files' bytes, concatenated in header order
//! 12+H+B  64    Ed25519 signature by the release key over everything before it
//! ```
//!
//! The signature covers every byte before it, so nothing in a package can
//! change without invalidating it; its identity (for revocation) is the
//! SHA-256 of the whole file. A package always carries the bundle's source;
//! bytecode is an extra that only loads when the runtime's
//! `BYTECODE_VERSION` matches (QuickJS bytecode is trusted input: loading
//! crafted bytecode is memory corruption, so it is only ever loaded from a
//! verified package or the app's own signed resources).

mod keys;
mod package;
mod trust;

pub use crate::{
    keys::{PublicKey, SecretKey},
    package::{
        App, Contents, FileKind, Header, VerifiedPackage, build, source_map, verify_signature,
    },
    trust::{Certificate, Revocations, TrustStore, certify, revoke},
};

/// This runtime's version (packages name the lowest they need).
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why a package, certificate or revocation list was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Malformed data.
    #[error("not a valid nsh package: {0}")]
    Format(String),
    /// A key could not be read or made.
    #[error("key: {0}")]
    Key(String),
    /// Signed by a key this store does not trust.
    #[error("signed by key `{0}`, which is not trusted")]
    UntrustedKey(String),
    /// A signature does not verify.
    #[error("the signature does not verify")]
    BadSignature,
    /// The release key's certificate does not cover this package.
    #[error("the release key's certificate does not cover this package: {0}")]
    Certificate(String),
    /// The release key or the package was revoked.
    #[error("revoked: {0}")]
    Revoked(String),
    /// A build older than one already accepted for the app.
    #[error("app `{app}` build {build} is older than build {accepted}, which was accepted before")]
    Rollback {
        /// The app id.
        app: String,
        /// The package's build.
        build: u64,
        /// The highest build accepted before.
        accepted: u64,
    },
    /// The package needs a newer runtime.
    #[error("the package needs runtime {needs}, this is {runtime}")]
    RuntimeTooOld {
        /// What the package needs.
        needs: String,
        /// This runtime.
        runtime: String,
    },
    /// A file does not match its hash.
    #[error("file `{0}` does not match its hash")]
    Integrity(String),
    /// A revocation list older than (or as old as) the one the store has.
    #[error("revocation list {got} is not newer than list {have}")]
    StaleRevocations {
        /// The list offered.
        got: u64,
        /// The list the store has.
        have: u64,
    },
    /// The trust store's state could not be read or written.
    #[error("trust state: {0}")]
    State(String),
}

/// `*` (any), `prefix.*` (ids under `prefix.`) or an exact app id.
pub(crate) fn app_matches(pattern: &str, id: &str) -> bool {
    match pattern.strip_suffix(".*") {
        _ if pattern == "*" => true,
        Some(prefix) => {
            id.len() > prefix.len() + 1
                && id.starts_with(prefix)
                && id.as_bytes()[prefix.len()] == b'.'
        }
        None => pattern == id,
    }
}

/// Whether semver-like `have` (`1.2.3`) is at least `needs`.
pub(crate) fn version_at_least(have: &str, needs: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parse(have) >= parse(needs)
}

/// Unix seconds now.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

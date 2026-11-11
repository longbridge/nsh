//! Code shared by `nsh-platform-macos` and `nsh-platform-ios`: both build on
//! the same Apple frameworks for these. A crate of its own so each backend
//! builds from its published package (a `#[path]` into a sibling crate does
//! not ship with it).
//!
//! [`web_cookies`] is the `WKHTTPCookieStore` of a web view's own website
//! data store, so a normal view shares the app's cookies and an `incognito`
//! view has its in-memory store of its own; both backends answer
//! `Command::WebViewRequest` with it.
#![cfg(any(target_os = "macos", target_os = "ios"))]

pub mod audio;
pub mod notifications;
pub mod speech;
pub mod web_cookies;
pub mod web_evaluation;
pub mod web_scheme;

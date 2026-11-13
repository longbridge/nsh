//! Static HTML of an NSH app's views, with no platform: the CSS of NSH's
//! styles (`css`, which the web backend draws views with: internal, not an
//! API of its own), and a
//! [`Document`] of a mounted tree, built from its mount batches, that
//! serializes as markup a browser lays out at any width (`nsh render`,
//! `docs/dev/native-render.md`).

#[doc(hidden)]
pub mod css;
mod document;

pub use document::{Document, Head, Page};

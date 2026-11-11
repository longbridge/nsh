//! The node tree an app answers `Tree.snapshot` with, and its compact text.
//!
//! The agent that answers (`nsh-testing`) does not depend on this crate: it
//! builds JSON with these field names, which [`SnapshotNode`] reads.

use serde::{Deserialize, Serialize};

/// One node of a snapshot, in tree order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SnapshotNode {
    /// The view's id.
    pub id: u32,
    /// Levels below the snapshot's root.
    pub depth: u32,
    /// Its kind, lower case (`view`, `text`, `input`, `scroll`, `list`, ...).
    pub kind: String,
    /// Its `testID`.
    #[serde(default)]
    pub test_id: Option<String>,
    /// Its accessibility role.
    #[serde(default)]
    pub role: Option<String>,
    /// Its accessibility label.
    #[serde(default)]
    pub label: Option<String>,
    /// The text it shows, cut at 80 characters.
    #[serde(default)]
    pub text: Option<String>,
    /// An input's text.
    #[serde(default)]
    pub value: Option<String>,
    /// An input's placeholder.
    #[serde(default)]
    pub placeholder: Option<String>,
    /// `[x, y, width, height]` in points.
    #[serde(default)]
    pub bounds: Option<[f32; 4]>,
    /// A user sees it.
    #[serde(default)]
    pub visible: bool,
    /// It takes input.
    #[serde(default)]
    pub enabled: bool,
    /// It has the keyboard focus.
    #[serde(default)]
    pub focused: bool,
    /// Selected, where it can tell.
    #[serde(default)]
    pub selected: Option<bool>,
    /// `false`, `true` or `mixed`, where it can tell.
    #[serde(default)]
    pub checked: Option<String>,
    /// A scroll view's `[x, y, content_width, content_height]`.
    #[serde(default)]
    pub scroll: Option<[f32; 4]>,
    /// It is inside a `List` cell.
    #[serde(default)]
    pub recycled: bool,
}

/// The snapshot as compact text, one node per line, two spaces per level:
/// `[id] kind "label or text" id=testID (x,y w×h)` then the flags
/// `disabled`, `focused`, `selected`, `checked=<v>`, `hidden`, `recycled`.
pub fn render_compact(nodes: &[SnapshotNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        for _ in 0..node.depth {
            out.push_str("  ");
        }
        out.push_str(&format!("[{}] {}", node.id, node.kind));
        if let Some(name) = node.label.as_ref().or(node.text.as_ref()) {
            out.push_str(&format!(" {name:?}"));
        }
        if let Some(id) = &node.test_id {
            out.push_str(&format!(" id={id}"));
        }
        if let Some([x, y, w, h]) = node.bounds {
            out.push_str(&format!(
                " ({},{} {}\u{d7}{})",
                x.round(),
                y.round(),
                w.round(),
                h.round()
            ));
        }
        if !node.enabled {
            out.push_str(" disabled");
        }
        if node.focused {
            out.push_str(" focused");
        }
        if node.selected == Some(true) {
            out.push_str(" selected");
        }
        if let Some(checked) = &node.checked {
            out.push_str(&format!(" checked={checked}"));
        }
        if !node.visible {
            out.push_str(" hidden");
        }
        if node.recycled {
            out.push_str(" recycled");
        }
        out.push('\n');
    }
    out
}

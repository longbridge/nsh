//! The snapshot as an XML document, and XPath evaluated on it.
//!
//! WebDriver clients locate elements with XPath (`//button[@name="submit"]`),
//! so the server renders the view tree as XML (one element per node, named by
//! its kind, nested by depth) and maps the selected elements back to view ids.

use std::fmt::Write;

use crate::tree::SnapshotNode;

/// The snapshot as XML: an `<App>` root, one element per node named by its
/// kind and nested by depth. Attributes: `id` (the view id), `name` (the
/// `testID`, as Appium users write it), `testID`, `label`, `text`, `value`,
/// `placeholder`, `role`, `x` `y` `width` `height` (rounded points), `visible`
/// `enabled` `focused`, `selected` and `checked` where known. Absent options
/// are omitted; characters XML 1.0 forbids are dropped.
pub fn render_xml(nodes: &[SnapshotNode]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><App>");
    // Names of the elements still open below `<App>`.
    let mut open: Vec<String> = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let depth = (node.depth as usize).min(open.len());
        while open.len() > depth {
            let name = open.pop().unwrap_or_default();
            let _ = write!(out, "</{name}>");
        }
        let name = element_name(&node.kind);
        let _ = write!(out, "<{name} id=\"{}\"", node.id);
        attr(&mut out, "name", node.test_id.as_deref());
        attr(&mut out, "testID", node.test_id.as_deref());
        attr(&mut out, "label", node.label.as_deref());
        attr(&mut out, "text", node.text.as_deref());
        attr(&mut out, "value", node.value.as_deref());
        attr(&mut out, "placeholder", node.placeholder.as_deref());
        attr(&mut out, "role", node.role.as_deref());
        if let Some([x, y, w, h]) = node.bounds {
            let _ = write!(
                out,
                " x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
                x.round() as i64,
                y.round() as i64,
                w.round() as i64,
                h.round() as i64
            );
        }
        let _ = write!(
            out,
            " visible=\"{}\" enabled=\"{}\" focused=\"{}\"",
            node.visible, node.enabled, node.focused
        );
        if let Some(selected) = node.selected {
            let _ = write!(out, " selected=\"{selected}\"");
        }
        attr(&mut out, "checked", node.checked.as_deref());
        let has_child = nodes
            .get(i + 1)
            .is_some_and(|next| next.depth as usize > depth);
        if has_child {
            out.push('>');
            open.push(name);
        } else {
            out.push_str("/>");
        }
    }
    while let Some(name) = open.pop() {
        let _ = write!(out, "</{name}>");
    }
    out.push_str("</App>");
    out
}

/// An element name XML accepts: a kind with anything else becomes `_`.
fn element_name(kind: &str) -> String {
    let mut name: String = kind
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if !name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        name.insert(0, '_');
    }
    name
}

fn attr(out: &mut String, name: &str, value: Option<&str>) {
    let Some(value) = value else { return };
    let _ = write!(out, " {name}=\"");
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Whitespace other than a space would be normalized to a space when parsed.
            '\t' | '\n' | '\r' => {
                let _ = write!(out, "&#{};", c as u32);
            }
            // XML 1.0's Char production.
            '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}' => {
                out.push(c)
            }
            _ => {}
        }
    }
    out.push('"');
}

/// Why an XPath expression gave no elements.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum XpathError {
    /// The expression or the document does not parse or evaluate.
    Invalid(String),
    /// It selects something other than elements (a number, a string, attributes).
    NotNodes,
}

impl std::fmt::Display for XpathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid XPath: {message}"),
            Self::NotNodes => f.write_str("the XPath expression does not select elements"),
        }
    }
}

impl std::error::Error for XpathError {}

/// The `id` attribute of every element `expr` selects in `xml` (a document
/// from [`render_xml`]), in document order.
#[cfg(feature = "xpath")]
pub fn xpath_ids(xml: &str, expr: &str) -> Result<Vec<u32>, XpathError> {
    use sxd_xpath::Value;

    let package =
        sxd_document::parser::parse(xml).map_err(|e| XpathError::Invalid(format!("{e:?}")))?;
    let document = package.as_document();
    let value = sxd_xpath::evaluate_xpath(&document, expr)
        .map_err(|e| XpathError::Invalid(e.to_string()))?;
    let Value::Nodeset(nodes) = value else {
        return Err(XpathError::NotNodes);
    };
    let mut ids = Vec::new();
    for node in nodes.document_order() {
        let element = node.element().ok_or(XpathError::NotNodes)?;
        // The synthetic `<App>` root has no id; it is not a view.
        if let Some(id) = element.attribute_value("id").and_then(|v| v.parse().ok()) {
            ids.push(id);
        }
    }
    Ok(ids)
}

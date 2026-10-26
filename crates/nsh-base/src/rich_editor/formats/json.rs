//! ProseMirror's JSON shape (`{type, attrs, content, marks, text}`).

use super::{DocFormat, FormatError};
use crate::rich_editor::model::{Node, Schema};

/// Documents as ProseMirror JSON: lossless for every schema.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct JsonFormat {
    /// Whether [`DocFormat::serialize`] indents its output.
    pub pretty: bool,
}

impl DocFormat for JsonFormat {
    type Error = FormatError;

    fn parse(&self, schema: &Schema, input: &str) -> Result<Node, FormatError> {
        let value: serde_json::Value =
            serde_json::from_str(input).map_err(|e| FormatError::Json(e.to_string()))?;
        let doc = Node::from_json(schema, &value).map_err(|e| FormatError::Json(e.to_string()))?;
        // `from_json` builds nodes one by one; the whole tree is only
        // known valid once checked against the schema's content rules.
        doc.check()
            .map_err(|e| FormatError::Schema(e.to_string()))?;
        Ok(doc)
    }

    fn serialize(&self, doc: &Node) -> Result<String, FormatError> {
        let value = doc.to_json();
        let out = if self.pretty {
            serde_json::to_string_pretty(&value)
        } else {
            serde_json::to_string(&value)
        };
        out.map_err(|e| FormatError::Json(e.to_string()))
    }

    fn is_lossless_for(&self, _schema: &Schema) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rich_editor::extensions::starter;

    #[test]
    fn json_round_trips() {
        let schema = starter::schema();
        let input = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Hi "},{"type":"text","marks":[{"type":"bold"}],"text":"there"}]},{"type":"paragraph"}]}"#;
        let f = JsonFormat::default();
        let doc = f.parse(&schema, input).unwrap();
        let out = f.serialize(&doc).unwrap();
        let again = f.parse(&schema, &out).unwrap();
        assert!(doc.eq(&again));
        assert!(f.is_lossless_for(&schema));
    }

    #[test]
    fn invalid_json_is_an_error() {
        let schema = starter::schema();
        let f = JsonFormat::default();
        assert!(matches!(f.parse(&schema, "{"), Err(FormatError::Json(_))));
        assert!(f.parse(&schema, r#"{"type":"nope"}"#).is_err());
    }
}

//! A starter set of optional pieces, none of which the kernel needs: the
//! nodes and marks of `prosemirror-schema-basic` with the lists of
//! `prosemirror-schema-list` and common marks ([`nodes`], [`marks`],
//! [`schema`]), and key bindings, input rules and plugins for them (after
//! `prosemirror-example-setup`). An app takes the pieces it wants and adds
//! its own; the commands take any schema's types.

use std::sync::Arc;

use regex::Regex;
use serde_json::Value;

use super::{
    history::{HistoryOptions, history, redo, undo},
    input_rules::{
        GetAttrs, InputRule, JoinPredicate, ellipsis, em_dash, input_rules as input_rules_plugin,
        mark_input_rule, number, smart_quotes, textblock_type_input_rule, undo_input_rule,
        wrapping_input_rule,
    },
    keymap::keymap as keymap_plugin,
};
use crate::rich_editor::{
    commands::{
        BoxCommand, base_keymap, chain_commands, command, exit_code, join_down, join_up, lift,
        lift_list_item, select_parent_node, set_block_type, sink_list_item, split_list_item,
        toggle_mark, wrap_in, wrap_in_list,
    },
    model::{AttrSpec, Attrs, MarkSpec, NodeSpec, Schema, SchemaSpec},
    state::Plugin,
};

fn attr(default: Option<Value>) -> AttrSpec {
    AttrSpec {
        default,
        ..Default::default()
    }
}

/// The starter's node specs, each `(name, spec)`.
pub mod nodes {
    use super::*;

    /// The document: one block or more.
    pub fn doc() -> (String, NodeSpec) {
        (
            "doc".into(),
            NodeSpec {
                content: Some("block+".into()),
                ..Default::default()
            },
        )
    }

    /// A paragraph of inline content.
    pub fn paragraph() -> (String, NodeSpec) {
        (
            "paragraph".into(),
            NodeSpec {
                content: Some("inline*".into()),
                group: Some("block".into()),
                ..Default::default()
            },
        )
    }

    /// A quoted block of blocks.
    pub fn blockquote() -> (String, NodeSpec) {
        (
            "blockquote".into(),
            NodeSpec {
                content: Some("block+".into()),
                group: Some("block".into()),
                defining: true,
                ..Default::default()
            },
        )
    }

    /// A horizontal rule (a leaf block).
    pub fn horizontal_rule() -> (String, NodeSpec) {
        (
            "horizontal_rule".into(),
            NodeSpec {
                group: Some("block".into()),
                ..Default::default()
            },
        )
    }

    /// A heading with a `level` from 1 to 6 (default 1).
    pub fn heading() -> (String, NodeSpec) {
        (
            "heading".into(),
            NodeSpec {
                content: Some("inline*".into()),
                group: Some("block".into()),
                defining: true,
                attrs: vec![("level".into(), attr(Some(Value::from(1))))],
                ..Default::default()
            },
        )
    }

    /// A block of code: text without marks, with a `language` (default
    /// null).
    pub fn code_block() -> (String, NodeSpec) {
        (
            "code_block".into(),
            NodeSpec {
                content: Some("text*".into()),
                marks: Some(String::new()),
                group: Some("block".into()),
                code: true,
                defining: true,
                attrs: vec![("language".into(), attr(Some(Value::Null)))],
                ..Default::default()
            },
        )
    }

    /// Text.
    pub fn text() -> (String, NodeSpec) {
        (
            "text".into(),
            NodeSpec {
                group: Some("inline".into()),
                ..Default::default()
            },
        )
    }

    /// An inline image with a `src`, an `alt` and a `title`.
    pub fn image() -> (String, NodeSpec) {
        (
            "image".into(),
            NodeSpec {
                inline: true,
                group: Some("inline".into()),
                draggable: true,
                attrs: vec![
                    ("src".into(), attr(None)),
                    ("alt".into(), attr(Some(Value::Null))),
                    ("title".into(), attr(Some(Value::Null))),
                ],
                ..Default::default()
            },
        )
    }

    /// A line break in a textblock.
    pub fn hard_break() -> (String, NodeSpec) {
        (
            "hard_break".into(),
            NodeSpec {
                inline: true,
                group: Some("inline".into()),
                selectable: false,
                leaf_text: Some("\n".into()),
                linebreak_replacement: true,
                ..Default::default()
            },
        )
    }

    /// A list with an `order` (its first number, default 1).
    pub fn ordered_list() -> (String, NodeSpec) {
        (
            "ordered_list".into(),
            NodeSpec {
                content: Some("list_item+".into()),
                group: Some("block".into()),
                attrs: vec![("order".into(), attr(Some(Value::from(1))))],
                ..Default::default()
            },
        )
    }

    /// A bulleted list.
    pub fn bullet_list() -> (String, NodeSpec) {
        (
            "bullet_list".into(),
            NodeSpec {
                content: Some("list_item+".into()),
                group: Some("block".into()),
                ..Default::default()
            },
        )
    }

    /// A list item: a paragraph, then any blocks.
    pub fn list_item() -> (String, NodeSpec) {
        (
            "list_item".into(),
            NodeSpec {
                content: Some("paragraph block*".into()),
                defining: true,
                ..Default::default()
            },
        )
    }
}

/// The starter's mark specs, each `(name, spec)`.
pub mod marks {
    use super::*;

    /// A link with an `href` and a `title`; text typed at its end is not
    /// linked.
    pub fn link() -> (String, MarkSpec) {
        (
            "link".into(),
            MarkSpec {
                attrs: vec![
                    ("href".into(), attr(None)),
                    ("title".into(), attr(Some(Value::Null))),
                ],
                inclusive: false,
                ..Default::default()
            },
        )
    }

    /// Italic (emphasis).
    pub fn italic() -> (String, MarkSpec) {
        ("italic".into(), MarkSpec::default())
    }

    /// Bold (strong emphasis).
    pub fn bold() -> (String, MarkSpec) {
        ("bold".into(), MarkSpec::default())
    }

    /// Inline code. Its spec's `extra` has `code: true`, which input rules
    /// read as ProseMirror's mark spec `code`.
    pub fn code() -> (String, MarkSpec) {
        let mut extra = Attrs::new();
        extra.insert("code".into(), Value::Bool(true));
        (
            "code".into(),
            MarkSpec {
                extra,
                ..Default::default()
            },
        )
    }

    /// Strikethrough.
    pub fn strike() -> (String, MarkSpec) {
        ("strike".into(), MarkSpec::default())
    }

    /// Underline.
    pub fn underline() -> (String, MarkSpec) {
        ("underline".into(), MarkSpec::default())
    }
}

/// The starter schema's spec: every node of [`nodes`] and mark of
/// [`marks`], for an app to extend before making its schema.
pub fn schema_spec() -> SchemaSpec {
    SchemaSpec {
        nodes: vec![
            nodes::doc(),
            nodes::paragraph(),
            nodes::blockquote(),
            nodes::horizontal_rule(),
            nodes::heading(),
            nodes::code_block(),
            nodes::text(),
            nodes::image(),
            nodes::hard_break(),
            nodes::ordered_list(),
            nodes::bullet_list(),
            nodes::list_item(),
        ],
        marks: vec![
            marks::link(),
            marks::italic(),
            marks::bold(),
            marks::code(),
            marks::strike(),
            marks::underline(),
        ],
        top_node: None,
    }
}

/// The starter schema (the paragraph is the default block).
pub fn schema() -> Schema {
    Schema::new(schema_spec()).expect("the starter schema is valid")
}

fn attrs_of(key: &str, value: Value) -> Attrs {
    let mut attrs = Attrs::new();
    attrs.insert(key.into(), value);
    attrs
}

/// Key bindings for the starter's types that `schema` has, after
/// `prosemirror-example-setup`: history (`Mod-z`, `Shift-Mod-z`, `Mod-y`
/// off Apple platforms), marks (`Mod-b`, `Mod-i`, `Mod-u`, `` Mod-` ``),
/// lists (`Shift-Ctrl-8`/`9`, Enter splits items, `Mod-[`/`Mod-]` lift and
/// sink), blocks (`Shift-Ctrl-0` paragraph, `Shift-Ctrl-1`..`6` headings,
/// `Shift-Ctrl-\` code, `Ctrl->` quote, `Mod-_` rule), `Mod-Enter` /
/// `Shift-Enter` line breaks, Backspace undoing an input rule, `Escape`
/// selecting the parent and `Alt-ArrowUp`/`Down` joining.
pub fn keymap(schema: &Schema) -> Vec<(String, BoxCommand)> {
    let mac = cfg!(any(target_os = "macos", target_os = "ios"));
    let mut keys: Vec<(String, BoxCommand)> = vec![
        ("Mod-z".into(), command(undo)),
        ("Shift-Mod-z".into(), command(redo)),
        ("Backspace".into(), command(undo_input_rule)),
        ("Alt-ArrowUp".into(), command(join_up)),
        ("Alt-ArrowDown".into(), command(join_down)),
        ("Mod-BracketLeft".into(), command(lift)),
        ("Escape".into(), command(select_parent_node)),
    ];
    if !mac {
        keys.push(("Mod-y".into(), command(redo)));
    }
    for (name, key) in [
        ("bold", "Mod-b"),
        ("italic", "Mod-i"),
        ("underline", "Mod-u"),
        ("code", "Mod-`"),
    ] {
        if let Some(ty) = schema.mark_type(name) {
            keys.push((key.into(), toggle_mark(ty, None, true)));
        }
    }
    if let Some(ty) = schema.node_type("bullet_list") {
        keys.push(("Shift-Ctrl-8".into(), wrap_in_list(ty, None)));
    }
    if let Some(ty) = schema.node_type("ordered_list") {
        keys.push(("Shift-Ctrl-9".into(), wrap_in_list(ty, None)));
    }
    if let Some(ty) = schema.node_type("blockquote") {
        keys.push(("Ctrl->".into(), wrap_in(ty, None)));
    }
    if let Some(ty) = schema.node_type("hard_break") {
        let insert_break = command(move |state, dispatch| {
            if let Some(dispatch) = dispatch {
                let Ok(node) = ty.create(None, Default::default(), vec![]) else {
                    return false;
                };
                let mut tr = state.tr();
                tr.replace_selection_with(node, false);
                tr.scroll_into_view();
                dispatch(tr);
            }
            true
        });
        let line_break = chain_commands(vec![command(exit_code), insert_break]);
        keys.push(("Mod-Enter".into(), line_break.clone()));
        keys.push(("Shift-Enter".into(), line_break.clone()));
        if mac {
            keys.push(("Ctrl-Enter".into(), line_break));
        }
    }
    if let Some(ty) = schema.node_type("list_item") {
        keys.push(("Enter".into(), split_list_item(ty.clone(), None)));
        keys.push(("Mod-[".into(), lift_list_item(ty.clone())));
        keys.push(("Mod-]".into(), sink_list_item(ty)));
    }
    if let Some(ty) = schema.node_type("paragraph") {
        keys.push(("Shift-Ctrl-0".into(), set_block_type(ty, None)));
    }
    if let Some(ty) = schema.node_type("code_block") {
        keys.push(("Shift-Ctrl-\\".into(), set_block_type(ty, None)));
    }
    if let Some(ty) = schema.node_type("heading") {
        for level in 1..=6 {
            keys.push((
                format!("Shift-Ctrl-{level}"),
                set_block_type(ty.clone(), Some(attrs_of("level", Value::from(level)))),
            ));
        }
    }
    if let Some(ty) = schema.node_type("horizontal_rule") {
        keys.push((
            "Mod-_".into(),
            command(move |state, dispatch| {
                if let Some(dispatch) = dispatch {
                    let Ok(node) = ty.create(None, Default::default(), vec![]) else {
                        return false;
                    };
                    let mut tr = state.tr();
                    tr.replace_selection_with(node, false);
                    tr.scroll_into_view();
                    dispatch(tr);
                }
                true
            }),
        ));
    }
    keys
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a valid expression")
}

/// Markdown-style input rules for the starter's types that `schema` has:
/// `# `..`###### ` headings, `> ` quotes, `- ` / `* ` / `+ ` bullet lists,
/// `1. ` ordered lists, ```` ``` ```` code blocks, `**bold**`, `*italic*`,
/// `` `code` `` and `~~strike~~`, smart quotes, `...` and `--`.
pub fn input_rules(schema: &Schema) -> Plugin {
    input_rules_plugin(input_rule_list(schema))
}

/// The rules of [`input_rules`], for an app to add to.
pub fn input_rule_list(schema: &Schema) -> Vec<InputRule> {
    let mut rules = smart_quotes();
    rules.push(ellipsis());
    rules.push(em_dash());
    if let Some(ty) = schema.node_type("blockquote") {
        rules.push(wrapping_input_rule(regex(r"^\s*>\s$"), ty, None, None));
    }
    if let Some(ty) = schema.node_type("ordered_list") {
        let get_attrs: GetAttrs =
            Arc::new(|found| found.group(1).map(|order| attrs_of("order", number(order))));
        let join: JoinPredicate = Arc::new(|found, node| {
            let order = found
                .group(1)
                .and_then(|order| order.parse::<u64>().ok())
                .unwrap_or(0);
            let start = node
                .attrs()
                .get("order")
                .and_then(Value::as_u64)
                .unwrap_or(1);
            node.child_count() as u64 + start == order
        });
        rules.push(wrapping_input_rule(
            regex(r"^(\d+)\.\s$"),
            ty,
            Some(get_attrs),
            Some(join),
        ));
    }
    if let Some(ty) = schema.node_type("bullet_list") {
        rules.push(wrapping_input_rule(
            regex(r"^\s*([-+*])\s$"),
            ty,
            None,
            None,
        ));
    }
    if let Some(ty) = schema.node_type("code_block") {
        rules.push(textblock_type_input_rule(regex("^```$"), ty, None));
    }
    if let Some(ty) = schema.node_type("heading") {
        let get_attrs: GetAttrs = Arc::new(|found| {
            found
                .group(1)
                .map(|hashes| attrs_of("level", Value::from(hashes.len())))
        });
        rules.push(textblock_type_input_rule(
            regex(r"^(#{1,6})\s$"),
            ty,
            Some(get_attrs),
        ));
    }
    if let Some(ty) = schema.mark_type("bold") {
        rules.push(mark_input_rule(regex(r"\*\*([^*]+)\*\*$"), ty, None));
    }
    if let Some(ty) = schema.mark_type("italic") {
        rules.push(mark_input_rule(
            regex(r"(?:^|[^*])(\*([^*]+)\*)$"),
            ty,
            None,
        ));
    }
    if let Some(ty) = schema.mark_type("code") {
        rules.push(mark_input_rule(regex(r"`([^`]+)`$"), ty, None));
    }
    if let Some(ty) = schema.mark_type("strike") {
        rules.push(mark_input_rule(regex(r"~~([^~]+)~~$"), ty, None));
    }
    rules
}

/// The starter's plugins for `schema`, in the order they run: its input
/// rules, its keymap, the base keymap and the history.
pub fn plugins(schema: &Schema) -> Vec<Plugin> {
    vec![
        input_rules(schema),
        keymap_plugin(keymap(schema)),
        keymap_plugin(base_keymap()),
        history(HistoryOptions::default()),
    ]
}

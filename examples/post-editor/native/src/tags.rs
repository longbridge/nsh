//! The three inline atoms of a short post, in the HTML the server stores
//! (`POST /v2/social/topics`):
//!
//! | Node | HTML |
//! |---|---|
//! | `securityTag` | `<span type="security-tag" counter_id="ST/US/TSLA" trend="up">$Tesla</span>` |
//! | `mention` | `<span class="mention" member_id="1001">@Alice</span>` |
//! | `hashtag` | `<span class="hashtag">#Earnings</span>` |
//!
//! Each is one position of the document: typed over, deleted and moved as a
//! whole. `trend` is absent until the author picks one. The text inside a
//! tag is what the page shows (its trigger and its label); reading HTML back
//! takes the label from it.

use std::sync::Arc;

use nsh_base::rich_editor::{
    extensions::{
        Extension,
        suggestion::{SuggestionOptions, suggestion},
    },
    model::{AttrSpec, AttrType, Attrs, DomOutputSpec, NodeSpec, ParseRule, TextRender},
    state::{ClickEvent, EditorState, NodeSelection, Plugin, PluginKey, PluginSpec, Transaction},
};
use serde_json::Value;

/// An attribute with a default of `null`.
fn optional(validate: AttrType) -> AttrSpec {
    let mut spec = AttrSpec::default();
    spec.default = Some(Value::Null);
    spec.validate = Some(validate);
    spec.rendered = false;
    spec
}

/// A required string attribute.
fn required() -> AttrSpec {
    let mut spec = AttrSpec::default();
    spec.validate = Some(AttrType::String);
    spec.rendered = false;
    spec
}

/// The label: a string, empty until set.
fn label() -> AttrSpec {
    let mut spec = required();
    spec.default = Some(Value::from(""));
    spec
}

/// The label of a tag's text: what follows its trigger.
fn label_of(text: &str, trigger: char) -> String {
    text.trim().trim_start_matches(trigger).to_owned()
}

/// The text a tag shows: its trigger and its label.
fn shown(node: &nsh_base::rich_editor::model::Node, trigger: char) -> String {
    let label = node
        .attrs()
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or_default();
    format!("{trigger}{label}")
}

/// A tag's HTML: `span` with the fixed and attribute-read attributes, its
/// trigger and its label.
fn tag_dom(trigger: &str, attrs: &[(&str, &str)], fixed: &[(&str, &str)]) -> DomOutputSpec {
    let mut el = DomOutputSpec::tag("span");
    for (name, value) in fixed {
        el = el.attr(*name, *value);
    }
    for (name, from) in attrs {
        el = el.attr_of(*name, *from);
    }
    el.child(DomOutputSpec::Text(trigger.into()))
        .child(DomOutputSpec::AttrText("label".into()))
}

/// The attributes `el` (an element) gives a tag rule: `copy` maps DOM
/// attributes to node attributes; the label is the element's text without
/// its trigger. `None` when a required one is missing.
fn read(
    el: &nsh_base::rich_editor::model::DomElement,
    trigger: char,
    copy: &[(&str, &str, bool)],
) -> Option<Attrs> {
    let mut attrs = Attrs::new();
    for (dom, name, required) in copy {
        match el.attr(dom).filter(|v| !v.is_empty()) {
            Some(v) => {
                attrs.insert((*name).into(), Value::from(v));
            }
            None if *required => return None,
            None => {
                attrs.insert((*name).into(), Value::Null);
            }
        }
    }
    attrs.insert(
        "label".into(),
        Value::from(label_of(&el.text_content(), trigger)),
    );
    Some(attrs)
}

fn base() -> NodeSpec {
    let mut spec = NodeSpec::default();
    spec.group = Some("inline".into());
    spec.inline = true;
    spec.atom = true;
    spec.selectable = true;
    spec
}

/// Selects a stock tag a press lands beside. The view puts a press on an
/// inline atom at its nearer side as a caret (`Hit::Text`) and offers no
/// `handle_click_on` for it, so the plugin looks at both neighbors of the
/// position: the tag before it (the press was on its right half) or after
/// it (its left half). The selected tag is what the page opens its menu
/// for; Shift and Mod keep their own meaning.
struct SelectTagOnPress;

impl PluginSpec for SelectTagOnPress {
    fn key(&self) -> PluginKey {
        PluginKey::new("selectSecurityTag")
    }

    fn handle_click(
        &self,
        state: &EditorState,
        pos: usize,
        event: &ClickEvent,
        dispatch: &mut dyn FnMut(Transaction),
    ) -> bool {
        if event.shift || event.mod_key || event.read_only {
            return false;
        }
        let doc = state.doc();
        let Ok(at) = doc.resolve(pos) else {
            return false;
        };
        let is_tag = |node: Option<nsh_base::rich_editor::model::Node>| {
            node.is_some_and(|n| n.ty().name() == "securityTag")
        };
        let from = if is_tag(at.node_after()) {
            pos
        } else if is_tag(at.node_before()) {
            pos - 1
        } else {
            return false;
        };
        let mut tr = state.tr();
        tr.set_selection(NodeSelection::create(doc, from));
        dispatch(tr);
        true
    }
}

/// `securityTag`: a stock, `counterId` its id on the server, `label` the
/// name shown, `trend` `"up"` (bullish), `"down"` (bearish) or `null`. A
/// press on it selects it, for the page's menu of its trend.
pub fn security_tag() -> Extension {
    Extension::node("securityTag", |_| {
        let mut spec = base();
        spec.attrs = vec![
            ("counterId".into(), required()),
            ("label".into(), label()),
            ("trend".into(), optional(AttrType::String)),
        ];
        spec.parse_dom = vec![
            ParseRule::tag("span[type=\"security-tag\"]").with_get_attrs(|input| {
                read(
                    input.element()?,
                    '$',
                    &[("counter_id", "counterId", true), ("trend", "trend", false)],
                )
            }),
        ];
        spec.to_dom = Some(tag_dom(
            "$",
            &[("counter_id", "counterId"), ("trend", "trend")],
            &[("type", "security-tag")],
        ));
        spec.render_text = Some(TextRender::new(|node| shown(node, '$')));
        spec
    })
    .add_plugins(|_| vec![Arc::new(SelectTagOnPress) as Plugin])
}

/// `mention`: a member, `memberId` their id on the server, `label` the name.
pub fn mention() -> Extension {
    Extension::node("mention", |_| {
        let mut spec = base();
        spec.attrs = vec![("memberId".into(), required()), ("label".into(), label())];
        let rule = |selector: &str| {
            ParseRule::tag(selector.to_owned()).with_get_attrs(|input| {
                read(input.element()?, '@', &[("member_id", "memberId", true)])
            })
        };
        spec.parse_dom = vec![rule("span.mention"), rule("a.mention")];
        spec.to_dom = Some(tag_dom(
            "@",
            &[("member_id", "memberId")],
            &[("class", "mention")],
        ));
        spec.render_text = Some(TextRender::new(|node| shown(node, '@')));
        spec
    })
}

/// `hashtag`: a topic, `label` its name.
pub fn hashtag() -> Extension {
    Extension::node("hashtag", |_| {
        let mut spec = base();
        spec.attrs = vec![("label".into(), required())];
        spec.parse_dom = vec![
            ParseRule::tag("span.hashtag").with_get_attrs(|input| read(input.element()?, '#', &[])),
        ];
        spec.to_dom = Some(tag_dom("#", &[], &[("class", "hashtag")]));
        spec.render_text = Some(TextRender::new(|node| shown(node, '#')));
        spec
    })
}

/// The suggestions of the three triggers, `$` (key `security`), `@` (key
/// `mention`) and `#` (key `hashtag`), whose state the page reads with
/// `editor.suggestion(key)`. A trigger works after any character (Chinese
/// text has no spaces to put before it) and the list's keys (arrows, Enter,
/// Tab) are the plugin's: it is an extension above the default priority
/// because the plugins of a `createRichEditor` call come after the keymap,
/// where Enter has split the paragraph already.
pub fn suggestions() -> Extension {
    Extension::new("postSuggestions")
        .with_priority(200)
        .add_plugins(|_| {
            [("security", "$"), ("mention", "@"), ("hashtag", "#")]
                .into_iter()
                .map(|(key, char)| {
                    let mut options = SuggestionOptions::default();
                    options.key = key.into();
                    options.trigger.char = char.into();
                    options.trigger.allowed_prefixes = None;
                    options.capture_keys = ["ArrowUp", "ArrowDown", "Enter", "Tab"]
                        .map(String::from)
                        .to_vec();
                    suggestion(options)
                })
                .collect()
        })
}

//! HTML in, the same HTML out: what the server stores survives the editor.

use nsh_base::rich_editor::{
    extensions::{ExtensionManager, kit::starter_kit},
    formats::{DomParser, ParseOptions, doc_to_html},
    model::{Node, Schema},
};
use post_editor_native::{hashtag, mention, security_tag};
use serde_json::json;

fn schema() -> Schema {
    ExtensionManager::with_core(vec![starter_kit(), security_tag(), mention(), hashtag()])
        .unwrap()
        .schema()
        .clone()
}

fn read(schema: &Schema, html: &str) -> Node {
    DomParser::from_schema(schema).parse(html, &ParseOptions::default())
}

/// Reads `html` and writes it back, expecting the same.
#[track_caller]
fn round_trips(html: &str) -> Node {
    let schema = schema();
    let doc = read(&schema, html);
    doc.check().unwrap();
    assert_eq!(doc_to_html(&doc), html);
    // And once more: the written form reads to the same document.
    assert_eq!(read(&schema, &doc_to_html(&doc)), doc);
    doc
}

/// The inline nodes of the first paragraph.
fn inline_nodes(doc: &Node) -> Vec<Node> {
    let mut out = Vec::new();
    doc.child(0).for_each(&mut |n, _, _| out.push(n.clone()));
    out
}

#[test]
fn a_security_tag_keeps_its_counter_and_trend() {
    let html = "<p>Buy <span type=\"security-tag\" counter_id=\"ST/US/TSLA\" trend=\"up\">$Tesla</span> now</p>";
    let doc = round_trips(html);
    let tag = &inline_nodes(&doc)[1];
    assert_eq!(tag.ty().name(), "securityTag");
    assert_eq!(tag.attrs()["counterId"], json!("ST/US/TSLA"));
    assert_eq!(tag.attrs()["label"], json!("Tesla"));
    assert_eq!(tag.attrs()["trend"], json!("up"));
    // One position: a text selection never lands inside it.
    assert!(tag.is_atom() && tag.is_leaf() && tag.node_size() == 1);
}

#[test]
fn a_security_tag_without_a_trend_writes_none() {
    round_trips("<p><span type=\"security-tag\" counter_id=\"ST/HK/700\">$Tencent</span></p>");
    let doc = round_trips(
        "<p><span type=\"security-tag\" counter_id=\"ST/HK/700\" trend=\"down\">$Tencent</span></p>",
    );
    assert_eq!(inline_nodes(&doc)[0].attrs()["trend"], json!("down"));
}

#[test]
fn a_mention_keeps_its_member() {
    let doc = round_trips("<p>cc <span class=\"mention\" member_id=\"1001\">@Alice</span> ok</p>");
    let node = &inline_nodes(&doc)[1];
    assert_eq!(node.ty().name(), "mention");
    assert_eq!(node.attrs()["memberId"], json!("1001"));
    assert_eq!(node.attrs()["label"], json!("Alice"));
}

#[test]
fn a_hashtag_keeps_its_label() {
    let doc = round_trips("<p><span class=\"hashtag\">#Earnings</span> season</p>");
    let node = &inline_nodes(&doc)[0];
    assert_eq!(node.ty().name(), "hashtag");
    assert_eq!(node.attrs()["label"], json!("Earnings"));
}

#[test]
fn every_tag_among_text_and_paragraphs() {
    round_trips(concat!(
        "<p>看多 <span type=\"security-tag\" counter_id=\"ST/US/NVDA\" trend=\"up\">$英伟达</span> ",
        "<span class=\"mention\" member_id=\"7\">@老王</span> ",
        "<span class=\"hashtag\">#财报</span></p>",
        "<p>第二段 <strong>加粗</strong></p>",
    ));
}

#[test]
fn text_in_labels_is_escaped() {
    round_trips("<p><span class=\"hashtag\">#A&amp;B</span></p>");
    let doc = round_trips("<p><span class=\"mention\" member_id=\"1\">@a&lt;b</span></p>");
    assert_eq!(inline_nodes(&doc)[0].attrs()["label"], json!("a<b"));
}

#[test]
fn a_tag_without_its_id_reads_as_text() {
    let schema = schema();
    let doc = read(&schema, "<p><span type=\"security-tag\">$Tesla</span></p>");
    assert!(
        inline_nodes(&doc).iter().all(|n| n.is_text()),
        "{}",
        doc_to_html(&doc)
    );
}

#[test]
fn the_text_of_a_tag_is_its_trigger_and_label() {
    let schema = schema();
    let doc = read(
        &schema,
        "<p><span type=\"security-tag\" counter_id=\"ST/US/TSLA\">$Tesla</span><span class=\"hashtag\">#Q3</span></p>",
    );
    let tag = &inline_nodes(&doc)[0];
    let render = tag.ty().spec().render_text.clone().unwrap();
    assert_eq!(render.text(tag), "$Tesla");
}

/// The HTML parser collapses white space as a browser does: the space
/// `insertMention` leaves after a tag at the end of a paragraph is written
/// out, and gone once the HTML is read back (a first read settles it, the
/// next ones keep the document).
#[test]
fn a_trailing_space_is_dropped_on_reading() {
    let schema = schema();
    let doc = read(
        &schema,
        "<p>cc <span class=\"mention\" member_id=\"1\">@A</span> </p>",
    );
    assert_eq!(
        doc_to_html(&doc),
        "<p>cc <span class=\"mention\" member_id=\"1\">@A</span></p>"
    );
}

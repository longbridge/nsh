use nsh_devtools::{
    tree::SnapshotNode,
    xml::{XpathError, render_xml, xpath_ids},
};

fn node(id: u32, depth: u32, kind: &str) -> SnapshotNode {
    let mut n = SnapshotNode::default();
    n.id = id;
    n.depth = depth;
    n.kind = kind.into();
    n.visible = true;
    n.enabled = true;
    n
}

fn tree() -> Vec<SnapshotNode> {
    let mut root = node(1, 0, "view");
    root.test_id = Some("root".into());
    let mut button = node(412, 1, "button");
    button.label = Some("Submit".into());
    button.test_id = Some("submit".into());
    button.bounds = Some([1520.4, 880.0, 120.0, 31.6]);
    let mut text = node(414, 2, "text");
    text.text = Some("买入".into());
    vec![root, button, text]
}

#[test]
fn xml_nests_by_depth_and_names_by_kind() {
    assert_eq!(
        render_xml(&tree()),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><App>\
         <view id=\"1\" name=\"root\" testID=\"root\" visible=\"true\" enabled=\"true\" focused=\"false\">\
         <button id=\"412\" name=\"submit\" testID=\"submit\" label=\"Submit\" x=\"1520\" y=\"880\" width=\"120\" height=\"32\" visible=\"true\" enabled=\"true\" focused=\"false\">\
         <text id=\"414\" text=\"买入\" visible=\"true\" enabled=\"true\" focused=\"false\"/>\
         </button></view></App>"
    );
}

#[test]
fn xml_escapes_and_keeps_unicode() {
    let nasty = "a \"b\" & <c> 😀 买入";
    let mut n = node(7, 0, "text");
    n.text = Some(format!("{nasty}\u{0}\u{8}"));
    n.label = Some("l1\nl2".into());
    let xml = render_xml(&[n]);
    let package = sxd_document::parser::parse(&xml).expect("well-formed");
    let doc = package.as_document();
    let app = doc.root().children()[0].element().unwrap();
    let text = app.children()[0].element().unwrap();
    assert_eq!(text.attribute_value("text"), Some(nasty));
    assert_eq!(text.attribute_value("label"), Some("l1\nl2"));
    assert_eq!(text.attribute_value("id"), Some("7"));
}

#[test]
fn xpath_finds_by_attribute_and_text() {
    let mut nodes = tree();
    let mut second = node(415, 2, "text");
    second.text = Some("卖出".into());
    nodes.push(second);
    let xml = render_xml(&nodes);
    assert_eq!(xpath_ids(&xml, "//button[@name=\"submit\"]"), Ok(vec![412]));
    assert_eq!(xpath_ids(&xml, "//*[@text=\"买入\"]"), Ok(vec![414]));
    assert_eq!(xpath_ids(&xml, "(//text)[2]"), Ok(vec![415]));
    assert_eq!(xpath_ids(&xml, "//nothing"), Ok(vec![]));
}

#[test]
fn xpath_errors_are_typed() {
    let xml = render_xml(&tree());
    assert!(matches!(
        xpath_ids(&xml, "//["),
        Err(XpathError::Invalid(_))
    ));
    assert_eq!(xpath_ids(&xml, "count(//view)"), Err(XpathError::NotNodes));
    assert_eq!(xpath_ids(&xml, "//@name"), Err(XpathError::NotNodes));
}

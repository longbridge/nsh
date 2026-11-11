use nsh_devtools::tree::{SnapshotNode, render_compact};

fn node(id: u32, depth: u32, kind: &str) -> SnapshotNode {
    let mut n = SnapshotNode::default();
    n.id = id;
    n.depth = depth;
    n.kind = kind.into();
    n.visible = true;
    n.enabled = true;
    n
}

#[test]
fn compact_lines_are_indented_and_carry_what_matters() {
    let mut root = node(1, 0, "view");
    root.test_id = Some("root".into());
    let mut button = node(412, 1, "button");
    button.label = Some("Submit".into());
    button.test_id = Some("submit".into());
    button.bounds = Some([1520.0, 880.0, 120.0, 32.0]);
    button.enabled = false;
    let mut text = node(414, 2, "text");
    text.text = Some("Submit".into());
    assert_eq!(
        render_compact(&[root, button, text]),
        "[1] view id=root\n  [412] button \"Submit\" id=submit (1520,880 120×32) disabled\n    [414] text \"Submit\"\n"
    );
}

#[test]
fn the_agents_json_reads_back_as_nodes() {
    let json = serde_json::json!([{
        "id": 4, "depth": 1, "kind": "input", "test_id": null, "role": null,
        "label": null, "text": null, "value": "hi", "placeholder": "Name",
        "bounds": [0.0, 1.0, 2.0, 3.0], "visible": true, "enabled": true,
        "focused": true, "selected": null, "checked": null, "scroll": null,
        "recycled": false
    }]);
    let nodes: Vec<SnapshotNode> = serde_json::from_value(json).unwrap();
    assert_eq!(nodes[0].value.as_deref(), Some("hi"));
    assert!(nodes[0].focused);
}

#[test]
fn the_agents_describe_json_reads_back_as_a_node() {
    let json = serde_json::json!({
        "id": 4, "depth": 0, "kind": "view", "test_id": null, "role": "button",
        "label": "Go", "text": null, "value": null, "placeholder": null,
        "bounds": [0.0, 1.0, 2.0, 3.0], "visible": true, "enabled": true,
        "focused": false, "selected": null, "checked": null, "scroll": null,
        "recycled": false
    });
    let node: SnapshotNode = serde_json::from_value(json).unwrap();
    assert_eq!(node.id, 4);
    assert_eq!(node.label.as_deref(), Some("Go"));
}

//! Block structure under Enter, Backspace, Delete and Tab, as ProseMirror's
//! commands (prosemirror-commands, prosemirror-schema-list) and its example
//! setup's keymap shape it.

use super::*;

fn tree(s: &str) -> String {
    format!("Tree {s}")
}

fn sel(at: usize) -> String {
    format!("Sel {at}-{at}")
}

fn text_block(ty: &str, text: &str) -> Value {
    match text.is_empty() {
        true => json!({"type": ty}),
        false => json!({"type": ty, "content": [{"type": "text", "text": text}]}),
    }
}

fn list(ty: &str, items: &[&str]) -> Value {
    let content: Vec<Value> = items
        .iter()
        .map(|t| json!({"type": "list_item", "content": [text_block("paragraph", t)]}))
        .collect();
    json!({"type": ty, "content": content})
}

fn blocks(content: Vec<Value>) -> Value {
    json!({"type": "doc", "content": content})
}

/// Starts the app and focuses the editor (the caret at the document's start).
fn focused(doc: Value) -> Headless {
    let mut app = start(doc);
    app.type_text("id:editor", "").unwrap();
    app
}

fn expect(app: &mut Headless, t: &str, at: usize) {
    see(app, &tree(t));
    see(app, &sel(at));
}

// Paragraphs

#[test]
fn enter_in_the_middle_splits_a_paragraph() {
    let mut app = focused(paragraphs(&["abcd"]));
    keys(&mut app, &["right", "right", "enter"]);
    expect(&mut app, r#"doc(paragraph("ab"), paragraph("cd"))"#, 5);
}

#[test]
fn enter_at_the_start_puts_an_empty_paragraph_above() {
    let mut app = focused(paragraphs(&["abcd"]));
    keys(&mut app, &["enter"]);
    expect(&mut app, r#"doc(paragraph, paragraph("abcd"))"#, 3);
}

#[test]
fn enter_at_the_end_puts_an_empty_paragraph_below() {
    let mut app = focused(paragraphs(&["abcd"]));
    keys(&mut app, &["end", "enter"]);
    expect(&mut app, r#"doc(paragraph("abcd"), paragraph)"#, 7);
}

#[test]
fn enter_on_a_selection_deletes_it_then_splits() {
    let mut app = focused(paragraphs(&["abcd"]));
    keys(&mut app, &["shift+right", "shift+right", "enter"]);
    expect(&mut app, r#"doc(paragraph, paragraph("cd"))"#, 3);
}

#[test]
fn enter_at_the_end_of_a_heading_makes_a_paragraph() {
    let mut app = focused(blocks(vec![text_block("heading", "Title")]));
    keys(&mut app, &["end", "enter"]);
    // ProseMirror: splitBlock gives the new block the default type at the end.
    expect(&mut app, r#"doc(heading("Title"), paragraph)"#, 8);
    see(&mut app, "Types heading,paragraph");
}

#[test]
fn enter_at_the_start_of_a_heading_puts_a_paragraph_above() {
    let mut app = focused(blocks(vec![text_block("heading", "Title")]));
    keys(&mut app, &["enter"]);
    // ProseMirror: splitBlock at a textblock's start gives the empty block
    // above the default type (a paragraph).
    expect(&mut app, r#"doc(paragraph, heading("Title"))"#, 3);
    see(&mut app, "Types paragraph,heading");
}

#[test]
fn shift_enter_inserts_a_hard_break() {
    let mut app = focused(paragraphs(&["abcd"]));
    keys(&mut app, &["right", "right", "shift+enter"]);
    expect(&mut app, r#"doc(paragraph("ab", hard_break, "cd"))"#, 4);
}

// Lists

#[test]
fn enter_in_a_list_item_splits_it() {
    let mut app = focused(blocks(vec![list("bullet_list", &["abcd"])]));
    keys(&mut app, &["right", "right", "enter"]);
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("ab")), list_item(paragraph("cd"))))"#,
        9,
    );
}

#[test]
fn two_enters_at_the_end_of_a_list_end_it() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b"])]));
    keys(&mut app, &["ctrl+end", "enter"]);
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a")), list_item(paragraph("b")), list_item(paragraph)))"#,
        13,
    );
    // ProseMirror: an empty last item is lifted out (liftEmptyBlock).
    keys(&mut app, &["enter"]);
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a")), list_item(paragraph("b"))), paragraph)"#,
        13,
    );
}

#[test]
fn enter_on_an_empty_middle_item_splits_the_list() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "", "c"])]));
    keys(&mut app, &["end", "right"]);
    see(&mut app, &sel(8));
    keys(&mut app, &["enter"]);
    // ProseMirror: liftEmptyBlock lifts the paragraph out of the list, which splits it.
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a"))), paragraph, bullet_list(list_item(paragraph("c"))))"#,
        8,
    );
}

#[test]
fn enter_on_an_empty_nested_item_outdents_it() {
    let nested = json!({"type": "list_item", "content": [
        text_block("paragraph", "a"),
        list("bullet_list", &[""]),
    ]});
    let mut app = focused(blocks(vec![
        json!({"type": "bullet_list", "content": [nested]}),
    ]));
    keys(&mut app, &["ctrl+end", "enter"]);
    // ProseMirror: splitListItem moves it to the outer list, after its parent.
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a")), list_item(paragraph)))"#,
        8,
    );
}

#[test]
fn tab_sinks_an_item_and_shift_tab_lifts_it_back() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b"])]));
    keys(&mut app, &["ctrl+end", "tab"]);
    see(
        &mut app,
        &tree(
            r#"doc(bullet_list(list_item(paragraph("a"), bullet_list(list_item(paragraph("b"))))))"#,
        ),
    );
    keys(&mut app, &["shift+tab"]);
    see(
        &mut app,
        &tree(r#"doc(bullet_list(list_item(paragraph("a")), list_item(paragraph("b"))))"#),
    );
}

#[test]
fn mod_brackets_sink_and_lift_an_item() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b"])]));
    keys(&mut app, &["ctrl+end", "mod+]"]);
    see(
        &mut app,
        &tree(
            r#"doc(bullet_list(list_item(paragraph("a"), bullet_list(list_item(paragraph("b"))))))"#,
        ),
    );
    keys(&mut app, &["mod+["]);
    see(
        &mut app,
        &tree(r#"doc(bullet_list(list_item(paragraph("a")), list_item(paragraph("b"))))"#),
    );
}

#[test]
fn tab_on_the_first_item_does_nothing() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b"])]));
    keys(&mut app, &["tab"]);
    // ProseMirror: sinkListItem needs an item before to nest into.
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a")), list_item(paragraph("b"))))"#,
        3,
    );
}

#[test]
fn shift_tab_at_the_top_level_makes_a_paragraph_and_splits_the_list() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b", "c"])]));
    keys(&mut app, &["end", "right"]);
    see(&mut app, &sel(8));
    keys(&mut app, &["shift+tab"]);
    // ProseMirror: liftOutOfList.
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a"))), paragraph("b"), bullet_list(list_item(paragraph("c"))))"#,
        8,
    );
}

#[test]
fn backspace_at_the_start_of_an_item_joins_it_into_the_previous_one() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b"])]));
    keys(&mut app, &["end", "right"]);
    see(&mut app, &sel(8));
    keys(&mut app, &["backspace"]);
    // ProseMirror: joinBackward joins the two items (joinMaybeClear).
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a"), paragraph("b"))))"#,
        6,
    );
    keys(&mut app, &["backspace"]);
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("ab"))))"#,
        4,
    );
}

#[test]
fn backspace_at_the_start_of_the_first_item_lifts_it_out() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b"])]));
    keys(&mut app, &["backspace"]);
    // ProseMirror: no block before to join, so joinBackward lifts.
    expect(
        &mut app,
        r#"doc(paragraph("a"), bullet_list(list_item(paragraph("b"))))"#,
        1,
    );
}

#[test]
fn delete_at_the_end_of_an_item_joins_the_next_one() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a", "b"])]));
    keys(&mut app, &["end", "delete"]);
    // ProseMirror: joinForward joins the items (joinMaybeClear).
    expect(
        &mut app,
        r#"doc(bullet_list(list_item(paragraph("a"), paragraph("b"))))"#,
        4,
    );
}

#[test]
fn ordered_lists_keep_their_order() {
    let mut start_list = list("ordered_list", &["x"]);
    start_list["attrs"] = json!({"order": 3});
    let mut app = focused(blocks(vec![start_list]));
    keys(&mut app, &["ctrl+end", "enter"]);
    app.type_text("id:editor", "y").unwrap();
    see(
        &mut app,
        &tree(r#"doc(ordered_list(list_item(paragraph("x")), list_item(paragraph("y"))))"#),
    );
    see(&mut app, "3.");
    see(&mut app, "4.");
}

#[test]
fn a_dash_and_space_in_an_item_nests_a_list() {
    let mut app = focused(blocks(vec![list("bullet_list", &["a"])]));
    keys(&mut app, &["end", "enter"]);
    app.type_text("id:editor", "- ").unwrap();
    // The Markdown extension wraps with wrapRangeInList: at the start of an
    // item that has one before it, the item nests under that one (as Tab).
    see(
        &mut app,
        &tree(r#"doc(bullet_list(list_item(paragraph("a"), bullet_list(list_item(paragraph)))))"#),
    );
}

// Blockquotes

#[test]
fn enter_on_an_empty_paragraph_in_a_quote_lifts_it_out() {
    let quote = json!({"type": "blockquote", "content": [
        text_block("paragraph", "a"),
        text_block("paragraph", ""),
    ]});
    let mut app = focused(blocks(vec![quote]));
    keys(&mut app, &["ctrl+end", "enter"]);
    // ProseMirror: liftEmptyBlock.
    expect(&mut app, r#"doc(blockquote(paragraph("a")), paragraph)"#, 6);
}

#[test]
fn backspace_at_the_start_of_a_quote_lifts_it() {
    let quote = json!({"type": "blockquote", "content": [text_block("paragraph", "a")]});
    let mut app = focused(blocks(vec![quote]));
    keys(&mut app, &["backspace"]);
    expect(&mut app, r#"doc(paragraph("a"))"#, 1);
}

// Code blocks

#[test]
fn enter_in_a_code_block_inserts_a_newline() {
    let mut app = focused(blocks(vec![text_block("code_block", "ab")]));
    keys(&mut app, &["end", "enter"]);
    see(&mut app, &doc("ab\n"));
    see(&mut app, "Types code_block");
    see(&mut app, &sel(4));
}

#[test]
fn mod_enter_exits_a_code_block() {
    let mut app = focused(blocks(vec![text_block("code_block", "ab")]));
    keys(&mut app, &["end", "mod+enter"]);
    // ProseMirror: exitCode makes a paragraph after the block.
    expect(&mut app, r#"doc(code_block("ab"), paragraph)"#, 5);
}

#[test]
fn shift_enter_exits_a_code_block() {
    let mut app = focused(blocks(vec![text_block("code_block", "ab")]));
    keys(&mut app, &["end", "shift+enter"]);
    expect(&mut app, r#"doc(code_block("ab"), paragraph)"#, 5);
}

#[test]
fn backspace_in_an_empty_code_block_deletes_it() {
    let mut app = focused(blocks(vec![
        text_block("paragraph", "a"),
        text_block("code_block", ""),
    ]));
    keys(&mut app, &["ctrl+end", "backspace"]);
    // ProseMirror: joinBackward deletes an empty block after a textblock.
    expect(&mut app, r#"doc(paragraph("a"))"#, 2);
}

#[test]
fn backspace_in_an_empty_code_block_at_the_start_makes_a_paragraph() {
    let mut app = focused(blocks(vec![
        text_block("code_block", ""),
        text_block("paragraph", "a"),
    ]));
    keys(&mut app, &["ctrl+home", "backspace"]);
    // Nothing to join backward with: the empty code block becomes a paragraph.
    expect(&mut app, r#"doc(paragraph, paragraph("a"))"#, 1);
}

#[test]
fn code_blocks_take_no_marks() {
    let mut app = focused(blocks(vec![text_block("code_block", "ab")]));
    keys(&mut app, &["mod+a", "mod+b"]);
    see(&mut app, "Bold 0");
    see(&mut app, &tree(r#"doc(code_block("ab"))"#));
}

// Joining

#[test]
fn backspace_at_the_start_of_a_paragraph_joins_the_previous_one() {
    let mut app = focused(paragraphs(&["ab", "cd"]));
    keys(&mut app, &["ctrl+end", "home", "backspace"]);
    expect(&mut app, r#"doc(paragraph("abcd"))"#, 3);
}

#[test]
fn backspace_joins_a_paragraph_into_a_heading() {
    let mut app = focused(blocks(vec![
        text_block("heading", "T"),
        text_block("paragraph", "x"),
    ]));
    keys(&mut app, &["ctrl+end", "home", "backspace"]);
    expect(&mut app, r#"doc(heading("Tx"))"#, 2);
}

#[test]
fn delete_at_the_end_of_a_paragraph_joins_the_next_one() {
    let mut app = focused(paragraphs(&["ab", "cd"]));
    keys(&mut app, &["end", "delete"]);
    expect(&mut app, r#"doc(paragraph("abcd"))"#, 3);
}

#[test]
fn backspace_after_an_atom_deletes_it() {
    let mut app = focused(blocks(vec![
        json!({"type": "stock", "attrs": {"symbol": "AAPL"}}),
        text_block("paragraph", "x"),
    ]));
    keys(&mut app, &["ctrl+end", "home", "backspace"]);
    // ProseMirror: joinBackward deletes an atom before the textblock
    // directly (selectNodeBackward runs only when joinBackward cannot).
    see(&mut app, &tree(r#"doc(paragraph("x"))"#));
    see(&mut app, "Types paragraph");
}

#[test]
fn backspace_at_the_start_of_the_document_does_nothing() {
    let mut app = focused(paragraphs(&["ab", "cd"]));
    keys(&mut app, &["backspace"]);
    expect(&mut app, r#"doc(paragraph("ab"), paragraph("cd"))"#, 1);
}

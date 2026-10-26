//! Tiptap's core commands the starter's key bindings, list keymap and code
//! block bindings run (`clearNodes`, `setNode`, `toggleNode`, `toggleWrap`,
//! `toggleList`, `deleteCurrentNode`, `joinItemForward`, `setHardBreak`,
//! `updateAttributes`, `insertDefaultBlock`), with the helpers they share
//! (`findParentNode`, `isAtStartOfNode`, chains). Ported from
//! `@tiptap/core`'s `commands/` and `helpers/`, for the starter's own use.

use serde_json::Value;

use crate::rich_editor::{
    commands::{
        BoxCommand, Dispatch, command, is_node_active, lift, lift_list_item, set_block_type,
        wrap_in, wrap_in_list,
    },
    model::{Attrs, Fragment, Mark, Node, NodeType, ResolvedPos},
    state::{EditorState, EditorStateConfig, Selection, TextSelection, Transaction},
    transform::{can_join, join_point, lift_target},
};

/// A command function, as [`chain`] takes them.
pub(crate) type Step<'a> = &'a dyn Fn(&EditorState, Dispatch<'_>) -> bool;

/// Runs `steps` one after the other, each on the state the ones before made,
/// into one transaction (Tiptap's `chain().….run()`): whether every step
/// applied. The transaction is dispatched when any step changed something.
pub(crate) fn chain(state: &EditorState, dispatch: Dispatch<'_>, steps: &[Step<'_>]) -> bool {
    let mut tr = state.tr();
    let mut current = state.clone();
    let mut all = true;
    for step in steps {
        let mut made = None;
        if !step(&current, Some(&mut |t| made = Some(t))) {
            all = false;
            continue;
        }
        let Some(made) = made else {
            continue;
        };
        for step in &made.steps {
            if tr.step(step.clone()).is_err() {
                return false;
            }
        }
        if made.selection_set() {
            tr.set_selection(made.selection());
        }
        if made.stored_marks_set() {
            tr.set_stored_marks(made.stored_marks().map(<[Mark]>::to_vec));
        }
        for (key, value) in made.meta_entries() {
            tr.set_meta(key, value.clone());
        }
        if made.scrolled_into_view() {
            tr.scroll_into_view();
        }
        // The next step sees the document and selection so far, without the
        // plugins' appended transactions (they run once, on the whole).
        let config = EditorStateConfig::new(state.schema().clone())
            .doc(tr.doc().clone())
            .selection(tr.selection());
        let Ok(next) = EditorState::create(config) else {
            return false;
        };
        current = match tr.stored_marks() {
            Some(marks) => {
                let mut with_marks = next.tr();
                with_marks.set_stored_marks(Some(marks.to_vec()));
                next.apply(with_marks).unwrap_or(next)
            }
            None => next,
        };
    }
    if let Some(dispatch) = dispatch
        && (tr.doc_changed() || tr.selection_set() || tr.stored_marks_set())
    {
        dispatch(tr);
    }
    all
}

/// The innermost ancestor of the selection's start (from the start's parent
/// out) that `predicate` takes, as Tiptap's `findParentNode`: its node,
/// the position before it (0 for the document), the start of its content
/// and its depth.
pub(crate) fn find_parent_node(
    from: &ResolvedPos,
    predicate: impl Fn(&Node) -> bool,
) -> Option<ParentNode> {
    (0..=from.depth).rev().find_map(|depth| {
        let node = from.node(depth);
        predicate(node).then(|| ParentNode {
            node: node.clone(),
            pos: if depth > 0 { from.before(depth) } else { 0 },
            start: from.start(depth),
            depth,
        })
    })
}

/// An ancestor [`find_parent_node`] found.
pub(crate) struct ParentNode {
    pub(crate) node: Node,
    pub(crate) pos: usize,
    pub(crate) start: usize,
    pub(crate) depth: usize,
}

/// Whether the selection is a cursor at the start of its parent (Tiptap's
/// `isAtStartOfNode`).
pub(crate) fn is_at_start_of_node(state: &EditorState) -> bool {
    let selection = state.selection();
    selection.resolved_from().parent_offset == 0 && selection.from() == selection.to()
}

/// Whether the cursor is at the end of the content of its innermost node
/// of `ty` (Tiptap's `isAtEndOfNode` with a type).
pub(crate) fn is_at_end_of_node(state: &EditorState, ty: &NodeType) -> bool {
    let selection = state.selection();
    let Some(parent) = find_parent_node(selection.resolved_from(), |n| n.ty() == ty) else {
        return false;
    };
    let Ok(inside) = state.doc().resolve(parent.pos + 1) else {
        return false;
    };
    selection.anchor() + 1 == inside.end(inside.depth)
}

/// Whether a list type is one: in the `list` group (Tiptap's `isList`).
pub(crate) fn is_list(ty: &NodeType) -> bool {
    ty.is_in_group("list")
}

/// The marks typing at the selection would get: the stored ones, else the
/// cursor's unless it is at the start of its block (Tiptap's
/// `storedMarks || ($to.parentOffset && $from.marks())`), only those that
/// carry on into a split (`keepOnSplit`).
pub(crate) fn splittable_marks(state: &EditorState) -> Option<Vec<Mark>> {
    let selection = state.selection();
    let marks = match state.stored_marks() {
        Some(marks) => marks.to_vec(),
        None if selection.resolved_to().parent_offset > 0 => selection.resolved_from().marks(),
        None => return None,
    };
    Some(
        marks
            .into_iter()
            .filter(|m| m.ty.spec().keep_on_split)
            .collect(),
    )
}

/// Turns the textblocks of the selection into their parents' default type
/// and lifts every block of it out of its wrappers (Tiptap's
/// `clearNodes`).
pub(crate) fn clear_nodes(state: &EditorState, dispatch: Dispatch<'_>) -> bool {
    let Some(dispatch) = dispatch else {
        return true;
    };
    let mut tr = state.tr();
    for range in state.selection().ranges() {
        let mut nodes = Vec::new();
        state
            .doc()
            .nodes_between(range.from.pos, range.to.pos, &mut |node, pos, _, _| {
                if !node.is_text() {
                    nodes.push((node.clone(), pos));
                }
                true
            });
        for (node, pos) in nodes {
            let from = tr.mapping.map(pos, 1);
            let to = tr.mapping.map(pos + node.node_size(), 1);
            let (Ok(from), Ok(to)) = (tr.doc().resolve(from), tr.doc().resolve(to)) else {
                continue;
            };
            let Some(node_range) = from.block_range(&to, None) else {
                continue;
            };
            let target = lift_target(&node_range);
            if node.is_textblock()
                && let Some(default) = from
                    .parent()
                    .content_match_at(from.index(from.depth))
                    .and_then(|m| m.default_type())
            {
                let _ = tr.set_node_markup(node_range.start(), Some(default), None, None);
            }
            if let Some(target) = target {
                let _ = tr.lift(&node_range, target);
            }
        }
    }
    dispatch(tr);
    true
}

/// Turns the selected textblocks into `ty` with `attrs` over the attributes
/// of the cursor's block, clearing wrappers first when they do not allow it
/// (Tiptap's `setNode`).
pub(crate) fn set_node(ty: NodeType, attrs: Option<Attrs>) -> BoxCommand {
    command(move |state, dispatch| {
        if !ty.is_textblock() {
            return false;
        }
        let attrs = merged_attrs(state, &ty, attrs.as_ref());
        let set = set_block_type(ty.clone(), Some(attrs));
        if set(state, None) {
            return set(state, dispatch);
        }
        chain(state, dispatch, &[&clear_nodes, &*set])
    })
}

// The attributes of the selection's block when anchor and head share it,
// with `attrs` over them, for `ty` (Tiptap copies them on setNode).
fn merged_attrs(state: &EditorState, ty: &NodeType, attrs: Option<&Attrs>) -> Attrs {
    let selection = state.selection();
    let anchor = selection.resolved_anchor();
    let mut merged = Attrs::new();
    if anchor.same_parent(selection.resolved_head()) {
        for (key, value) in anchor.parent().attrs() {
            if ty.spec().attrs.iter().any(|(name, _)| name == key) {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    if let Some(attrs) = attrs {
        merged.extend(attrs.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    merged
}

/// Sets the selected textblocks to `ty` with `attrs`, or back to
/// `toggle_ty` when they are that already (Tiptap's `toggleNode`).
pub(crate) fn toggle_node(ty: NodeType, toggle_ty: NodeType, attrs: Option<Attrs>) -> BoxCommand {
    command(move |state, dispatch| {
        if is_node_active(state, &ty, attrs.as_ref()) {
            set_node(toggle_ty.clone(), None)(state, dispatch)
        } else {
            set_node(ty.clone(), attrs.clone())(state, dispatch)
        }
    })
}

/// Lifts the selection out of a node of `ty` it is in, else wraps it in one
/// (Tiptap's `toggleWrap`).
pub(crate) fn toggle_wrap(ty: NodeType, attrs: Option<Attrs>) -> BoxCommand {
    let wrap = wrap_in(ty.clone(), attrs.clone());
    command(move |state, dispatch| {
        if is_node_active(state, &ty, attrs.as_ref()) {
            lift(state, dispatch)
        } else {
            wrap(state, dispatch)
        }
    })
}

/// Lifts the selection out of a list of `list_ty`, turns the list it is in
/// into one, or wraps it in one, joining the list with lists of its type
/// around it (Tiptap's `toggleList`, without keeping marks).
pub(crate) fn toggle_list(
    list_ty: NodeType,
    item_ty: NodeType,
    attrs: Option<Attrs>,
) -> BoxCommand {
    command(move |state, dispatch| {
        let selection = state.selection();
        let from = selection.resolved_from();
        let Some(range) = from.block_range(selection.resolved_to(), None) else {
            return false;
        };
        let parent_list = find_parent_node(from, |n| is_list(n.ty()));
        let doc = state.doc();
        let all = selection.from() == 0 && selection.to() == doc.content().size();
        let all_list =
            (all && doc.child_count() == 1 && is_list(doc.child(0).ty())).then(|| ParentNode {
                node: doc.child(0).clone(),
                pos: 0,
                start: 1,
                depth: 0,
            });
        let inside = parent_list
            .as_ref()
            .is_some_and(|list| range.depth >= 1 && range.depth <= list.depth + 1);
        let current = if inside { parent_list } else { all_list };
        if let Some(current) = current {
            if current.node.ty() == &list_ty {
                let lift = lift_list_item(item_ty.clone());
                if all {
                    let inner = |state: &EditorState, dispatch: Dispatch<'_>| {
                        let doc = state.doc();
                        let (Ok(start), Ok(end)) = (
                            doc.resolve(1),
                            doc.resolve(doc.child(0).node_size().saturating_sub(1)),
                        ) else {
                            return false;
                        };
                        if let Some(dispatch) = dispatch {
                            let mut tr = state.tr();
                            tr.set_selection(TextSelection::between(&start, &end, None));
                            dispatch(tr);
                        }
                        true
                    };
                    return chain(state, dispatch, &[&inner, &*lift]);
                }
                return lift(state, dispatch);
            }
            let first_item = current.node.first_child().map(|n| n.ty().clone());
            let converted = if list_ty.valid_content(current.node.content()) {
                None
            } else if first_item.as_ref().is_some_and(|ty| ty != &item_ty) {
                let items: Option<Fragment> = current
                    .node
                    .content()
                    .iter()
                    .map(|item| {
                        let ty = if item.ty() == first_item.as_ref().unwrap() {
                            &item_ty
                        } else {
                            item.ty()
                        };
                        ty.create(None, item.content().clone(), item.marks().to_vec())
                    })
                    .collect::<Result<_, _>>()
                    .ok()
                    .filter(|items: &Vec<Node>| list_ty.valid_content(&items.clone().into()))
                    .map(|items| items.into());
                match items {
                    Some(items) => Some(items),
                    None => return false,
                }
            } else {
                return false;
            };
            if let Some(dispatch) = dispatch {
                let mut tr = state.tr();
                match converted {
                    None => {
                        if tr
                            .set_node_markup(current.pos, Some(list_ty.clone()), None, None)
                            .is_err()
                        {
                            return false;
                        }
                    }
                    Some(content) => {
                        let Ok(list) = list_ty.create(attrs.clone(), content, vec![]) else {
                            return false;
                        };
                        let size = current.node.node_size();
                        if tr
                            .replace_with(current.pos, current.pos + size, list.into())
                            .is_err()
                        {
                            return false;
                        }
                        // Items of the same size: the selection stays where it was.
                        let (anchor, head) = (selection.anchor(), selection.head());
                        tr.set_selection(TextSelection::create(tr.doc(), anchor, Some(head)));
                    }
                }
                join_lists(&mut tr, &list_ty);
                tr.scroll_into_view();
                dispatch(tr);
            }
            return true;
        }
        let wrap = wrap_in_list(list_ty.clone(), attrs.clone());
        let joins = {
            let list_ty = list_ty.clone();
            move |state: &EditorState, dispatch: Dispatch<'_>| {
                if let Some(dispatch) = dispatch {
                    let mut tr = state.tr();
                    join_lists(&mut tr, &list_ty);
                    if tr.doc_changed() {
                        dispatch(tr);
                    }
                }
                true
            }
        };
        if wrap(state, None) {
            chain(state, dispatch, &[&*wrap, &joins])
        } else {
            chain(state, dispatch, &[&clear_nodes, &*wrap, &joins])
        }
    })
}

// Joins the list of `list_ty` the selection is in with one of its type
// right before it, then right after it (Tiptap's `joinListBackwards` and
// `joinListForwards`).
fn join_lists(tr: &mut Transaction, list_ty: &NodeType) {
    let selection = tr.selection();
    if let Some(list) = find_parent_node(selection.resolved_from(), |n| n.ty() == list_ty)
        && let Ok(at) = tr.doc().resolve(list.pos.saturating_sub(1))
    {
        let before = if list.depth <= at.depth && list.depth > 0 {
            Some(at.before(list.depth))
        } else if list.depth == at.depth + 1 {
            Some(at.pos)
        } else {
            None
        };
        if let Some(before) = before
            && tr.doc().node_at(before).is_some_and(|n| n.ty() == list_ty)
            && can_join(tr.doc(), list.pos)
        {
            let _ = tr.join(list.pos, 1);
        }
    }
    let selection = tr.selection();
    if let Some(list) = find_parent_node(selection.resolved_from(), |n| n.ty() == list_ty)
        && list.depth > 0
        && let Ok(at) = tr.doc().resolve(list.start)
    {
        let after = at.after(list.depth);
        if tr.doc().node_at(after).is_some_and(|n| n.ty() == list_ty) && can_join(tr.doc(), after) {
            let _ = tr.join(after, 1);
        }
    }
}

/// Deletes the empty node the cursor is in (Tiptap's `deleteCurrentNode`).
pub(crate) fn delete_current_node(state: &EditorState, dispatch: Dispatch<'_>) -> bool {
    let anchor = state.selection().resolved_anchor();
    let current = anchor.parent();
    if current.content().size() > 0 {
        return false;
    }
    for depth in (1..=anchor.depth).rev() {
        if anchor.node(depth).ty() == current.ty() {
            if let Some(dispatch) = dispatch {
                let mut tr = state.tr();
                if tr
                    .delete(anchor.before(depth), anchor.after(depth))
                    .is_err()
                {
                    return false;
                }
                tr.scroll_into_view();
                dispatch(tr);
            }
            return true;
        }
    }
    false
}

/// Joins the node at the selection's start with the one after it, two
/// levels deep (the next list item into this one; Tiptap's
/// `joinItemForward`).
pub(crate) fn join_item_forward(state: &EditorState, dispatch: Dispatch<'_>) -> bool {
    let Some(point) = join_point(state.doc(), state.selection().from(), 1) else {
        return false;
    };
    let Ok(at) = state.doc().resolve(point) else {
        return false;
    };
    let (Some(before), Some(after)) = (at.node_before(), at.node_after()) else {
        return false;
    };
    if before.ty().spec().isolating || after.ty().spec().isolating {
        return false;
    }
    let mut tr = state.tr();
    if tr.join(point, 2).is_err() {
        return false;
    }
    if let Some(dispatch) = dispatch {
        dispatch(tr);
    }
    true
}

/// Leaves a code block, else inserts a node of `ty` (a hard break), keeping
/// the marks typing had when `keep_marks` (Tiptap's `setHardBreak`).
pub(crate) fn set_hard_break(ty: NodeType, keep_marks: bool) -> BoxCommand {
    command(move |state, mut dispatch| {
        let exit = match &mut dispatch {
            Some(dispatch) => crate::rich_editor::commands::exit_code(state, Some(&mut **dispatch)),
            None => crate::rich_editor::commands::exit_code(state, None),
        };
        if exit {
            return true;
        }
        if state
            .selection()
            .resolved_from()
            .parent()
            .ty()
            .spec()
            .isolating
        {
            return false;
        }
        let Ok(node) = ty.create(None, Default::default(), vec![]) else {
            return false;
        };
        if let Some(dispatch) = dispatch {
            let marks = splittable_marks(state);
            let mut tr = state.tr();
            tr.replace_selection_with(node, false);
            if keep_marks && let Some(marks) = marks {
                tr.ensure_marks(marks);
            }
            tr.scroll_into_view();
            dispatch(tr);
        }
        true
    })
}

/// Sets `attr` to `value` on every node of a type in `types` in the
/// selection (Tiptap's `updateAttributes`, as `setTextAlign` runs it):
/// whether the schema has one of the types.
pub(crate) fn update_attribute(
    types: Vec<NodeType>,
    attr: &'static str,
    value: Value,
) -> BoxCommand {
    command(move |state, dispatch| {
        let types: Vec<&NodeType> = types
            .iter()
            .filter(|ty| ty.spec().attrs.iter().any(|(name, _)| name == attr))
            .collect();
        if types.is_empty() {
            return false;
        }
        if let Some(dispatch) = dispatch {
            let mut tr = state.tr();
            for range in state.selection().ranges() {
                let mut found = Vec::new();
                state
                    .doc()
                    .nodes_between(range.from.pos, range.to.pos, &mut |node, pos, _, _| {
                        if types.contains(&node.ty()) {
                            found.push(pos);
                        }
                        true
                    });
                for pos in found {
                    let _ = tr.set_node_attribute(pos, attr, value.clone());
                }
            }
            dispatch(tr);
        }
        true
    })
}

/// Inserts the default block of the place at `pos` and puts the cursor in
/// it (Tiptap's `insertDefaultBlock`).
pub(crate) fn insert_default_block(
    state: &EditorState,
    pos: usize,
    dispatch: Dispatch<'_>,
) -> bool {
    let Ok(at) = state.doc().resolve(pos) else {
        return false;
    };
    let Some(ty) = at
        .parent()
        .content_match_at(at.index(at.depth))
        .and_then(|m| m.default_type())
        .filter(NodeType::is_textblock)
    else {
        return false;
    };
    let Some(node) = ty.create_and_fill(None, Default::default(), vec![]) else {
        return false;
    };
    if let Some(dispatch) = dispatch {
        let mut tr = state.tr();
        if tr.insert(pos, node.into()).is_err() {
            return false;
        }
        let Ok(inside) = tr.doc().resolve(pos + 1) else {
            return false;
        };
        tr.set_selection(Selection::near(&inside, 1));
        tr.scroll_into_view();
        dispatch(tr);
    }
    true
}

//! The virtualized, recycling list: `<List>` (the `list` of `nsh:rt`) and
//! the rows of Rust components that lay out the app's items (`ListView`).
//!
//! Only rows near the viewport exist: the core decides which and asks for
//! cells (`Host::bind_list`, answered here without JS), and reuses them: a
//! recycled cell gets a new index, its item accessor follows. Positions,
//! columns, sticky headers and viewability are native.

use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_int},
    ptr,
};

use nsh_core::{
    Command, EventKind, NodeId, Op, Tag,
    geometry::{Percentage, Points},
};
use rquickjs::qjs;
use rustc_hash::FxHashMap;

use crate::{
    dom,
    reactive::{self, arg},
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

const UNDEFINED: JsValue = qjs::JS_UNDEFINED;

/// The host props a list forwards as they are.
const FORWARDED: &[&str] = &[
    "style",
    "testID",
    "horizontal",
    "showsIndicator",
    "refreshing",
    "onRefresh",
    "onEndReached",
    "onScrollEnd",
    "scrollValue",
    "keyboardDismissMode",
    "numColumns",
    "inverted",
    "maintainVisibleContentPosition",
    "stickyHeaderIndices",
    "contentStyle",
    "contentClass",
    "viewabilityConfig",
    "pagingEnabled",
    "snapToInterval",
    "snapToOffsets",
    "snapToAlignment",
    "decelerationRate",
    "scrollEnabled",
    "bounces",
    "contentInset",
    "contentOffset",
    "keyboardShouldPersistTaps",
    "scrollEventThrottle",
    "disableIntervalMomentum",
    "onScroll",
    "onScrollBeginDrag",
    "onScrollEndDrag",
    "onMomentumScrollBegin",
    "onMomentumScrollEnd",
];

/// The props the list itself uses: every other prop the app passes (the
/// test id, accessibility, classes, touch and layout events, ...) goes to
/// the scroll view as it would on a `<Scroll>`, as React Native's
/// `FlatList` passes the rest to its `ScrollView`.
const OWN: &[&str] = &[
    "each",
    "children",
    "ref",
    "keyExtractor",
    "estimatedItemSize",
    "onEndReachedThreshold",
    "initialScrollIndex",
    "onViewableItemsChanged",
    "header",
    "footer",
    "empty",
    "separator",
];

/// What the app passes a Rust component's rows (`listItemsProps` of the
/// JS glue): the data and the rows; the component lays them out.
pub(crate) const ITEMS_PROPS: &[&str] = &[
    "each",
    "children",
    "keyExtractor",
    "stickyHeaderIndices",
    "initialScrollIndex",
    "onViewableItemsChanged",
    "viewabilityConfig",
    "refreshing",
    "onRefresh",
    "onEndReached",
    "onEndReachedThreshold",
    "header",
    "footer",
    "empty",
];

struct Cell {
    node: JsValue,
    /// The index signal.
    index: JsValue,
    owner: u32,
}

struct ListData {
    props: JsValue,
    node: JsValue,
    /// The owner the list was made under: cells are made as its roots.
    owner: u32,
    /// Cells by slot.
    cells: Vec<Option<Cell>>,
    /// Owners of the header, footer and empty cells.
    extras: Vec<u32>,
    /// The viewable range reported last, and the latest.
    reported: (i64, i64),
    latest: (i64, i64),
    /// A pending `minimumViewTime` timer's id.
    timer: Option<f64>,
}

/// The lists' part of a context's state: each list by its node id.
#[derive(Default)]
pub(crate) struct Lists {
    map: RefCell<FxHashMap<NodeId, Box<ListData>>>,
}

impl Lists {
    /// Frees the values the lists hold (the state is unregistered).
    pub(crate) fn free_all(&mut self, raw: *mut JsContext) {
        for (_, data) in self.map.borrow_mut().drain() {
            free_data(raw, *data);
        }
    }
}

fn free_data(ctx: *mut JsContext, data: ListData) {
    // SAFETY: values the list owned.
    unsafe {
        qjs::JS_FreeValue(ctx, data.props);
        qjs::JS_FreeValue(ctx, data.node);
        for cell in data.cells.into_iter().flatten() {
            qjs::JS_FreeValue(ctx, cell.node);
            qjs::JS_FreeValue(ctx, cell.index);
        }
    }
}

fn lists<'a>(ctx: *mut JsContext) -> &'a Lists {
    // SAFETY: a projection of the state's field, by shared reference.
    unsafe { &(*crate::state::state(ctx)).lists }
}

fn get(ctx: *mut JsContext, obj: JsValue, key: &str) -> JsValue {
    let k = CString::new(key).unwrap_or_default();
    // SAFETY: a property read of a live value.
    unsafe {
        if !qjs::JS_IsObject(obj) {
            return UNDEFINED;
        }
        qjs::JS_GetPropertyStr(ctx, obj, k.as_ptr())
    }
}

fn has(ctx: *mut JsContext, obj: JsValue, key: &str) -> bool {
    // SAFETY: as `get`.
    unsafe {
        if !qjs::JS_IsObject(obj) {
            return false;
        }
        let k = CString::new(key).unwrap_or_default();
        let atom = qjs::JS_NewAtom(ctx, k.as_ptr());
        let r = qjs::JS_HasProperty(ctx, obj, atom);
        qjs::JS_FreeAtom(ctx, atom);
        r == 1
    }
}

fn length(ctx: *mut JsContext, array: JsValue) -> i64 {
    let mut len: i64 = 0;
    // SAFETY: reads a live value's length (0 for non-arrays).
    unsafe {
        if !qjs::JS_IsArray(array) || qjs::JS_GetLength(ctx, array, &mut len) != 0 {
            return 0;
        }
    }
    len
}

fn native(
    ctx: *mut JsContext,
    f: unsafe extern "C" fn(
        *mut JsContext,
        JsValue,
        c_int,
        *mut JsValue,
        c_int,
        *mut JsValue,
    ) -> JsValue,
    length: c_int,
    data: &mut [JsValue],
) -> JsValue {
    // SAFETY: the function keeps dups of `data`.
    unsafe {
        qjs::JS_NewCFunctionData(
            ctx,
            Some(f),
            length,
            0,
            data.len() as c_int,
            data.as_mut_ptr(),
        )
    }
}

/// `list(props)`: a virtualized list of `props.each`, rendering a row with
/// `props.children(item, index)` per cell.
pub(crate) unsafe extern "C" fn list(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    match render(ctx, arg(argc, argv, 0)) {
        Ok(node) => reactive::finish_value(ctx, node),
        Err(e) => e,
    }
}

/// Renders a list for `props` (borrowed); the list's node (owned).
pub(crate) fn render(ctx: *mut JsContext, props: JsValue) -> Result<JsValue, JsValue> {
    let node = dom::create_element(ctx, Tag::List)?;
    let id = dom::node_id(ctx, node).expect("a node");
    // SAFETY: dups the list owns.
    let data = Box::new(ListData {
        props: unsafe { qjs::JS_DupValue(ctx, props) },
        node: unsafe { qjs::JS_DupValue(ctx, node) },
        owner: reactive::current_owner(ctx),
        cells: Vec::new(),
        extras: Vec::new(),
        reported: (-1, -1),
        latest: (-1, -1),
        timer: None,
    });
    lists(ctx).map.borrow_mut().insert(id, data);
    let fail = |e: JsValue| {
        // SAFETY: ours.
        unsafe { qjs::JS_FreeValue(ctx, node) };
        e
    };
    // The items: tells the core how they changed (by identity: the common
    // start and end stay), so measurements follow their items.
    let func = native(ctx, splice_run, 1, &mut [node, props]);
    let r = reactive::render_effect(ctx, func);
    // SAFETY: the effect keeps its own value.
    unsafe { qjs::JS_FreeValue(ctx, r) };
    // The forwarded props, the estimated size, the end threshold and the
    // host props passed on.
    let rest = rest_names(ctx, props).map_err(fail)?;
    // SAFETY: a new array of the previous values.
    let prev = unsafe { qjs::JS_NewArray(ctx) };
    let func = native(ctx, forward_run, 0, &mut [node, props, prev, rest]);
    // SAFETY: the function keeps dups.
    unsafe {
        qjs::JS_FreeValue(ctx, prev);
        qjs::JS_FreeValue(ctx, rest);
    }
    let r = reactive::render_effect(ctx, func);
    unsafe { qjs::JS_FreeValue(ctx, r) };
    // Header and footer cells (always rendered), and the empty one.
    for (role, key) in [("header", "header"), ("footer", "footer")] {
        if has(ctx, props, key) {
            let owner = extra_cell(ctx, id, role, key).map_err(fail)?;
            if let Some(d) = lists(ctx).map.borrow_mut().get_mut(&id) {
                d.extras.push(owner);
            }
        }
    }
    if has(ctx, props, "empty") {
        // SAFETY: a placeholder for the empty cell's state (owner, node).
        let state = unsafe { qjs::JS_NewArray(ctx) };
        let func = native(ctx, empty_run, 0, &mut [node, props, state]);
        unsafe { qjs::JS_FreeValue(ctx, state) };
        let r = reactive::render_effect(ctx, func);
        unsafe { qjs::JS_FreeValue(ctx, r) };
    }
    if has(ctx, props, "onViewableItemsChanged") {
        let mut data = [qjs::JS_NewFloat64(id as f64)];
        let on = native(ctx, on_viewable, 1, &mut data);
        let r = dom::set_handler(ctx, id, EventKind::ViewableItemsChanged, on);
        unsafe { qjs::JS_FreeValue(ctx, on) };
        r.map_err(fail)?;
    }
    let initial = get(ctx, props, "initialScrollIndex");
    // SAFETY: an inline check and a conversion of a live value.
    unsafe {
        if qjs::JS_IsNumber(initial) {
            let mut index = 0f64;
            qjs::JS_ToFloat64(ctx, &mut index, initial);
            dom::push_op(
                ctx,
                Op::Command {
                    id,
                    command: Command::ScrollToIndex {
                        index: index.max(0.0) as u32,
                        animated: false,
                        view_position: Percentage(0.0),
                        view_offset: Points::ZERO,
                    },
                },
            )
            .map_err(fail)?;
        }
        qjs::JS_FreeValue(ctx, initial);
    }
    let ref_ = get(ctx, props, "ref");
    // SAFETY: a live value, called untracked with the node.
    unsafe {
        if qjs::JS_IsFunction(ctx, ref_) {
            let mut args = [node];
            let r = reactive::untracked_with(ctx, || {
                qjs::JS_Call(ctx, ref_, UNDEFINED, 1, args.as_mut_ptr())
            });
            qjs::JS_FreeValue(ctx, ref_);
            if qjs::JS_IsException(r) {
                return Err(fail(r));
            }
            qjs::JS_FreeValue(ctx, r);
        } else {
            qjs::JS_FreeValue(ctx, ref_);
        }
    }
    // Gone with its owner: the cells and the binding.
    let cleanup = native(ctx, dispose, 0, &mut [qjs::JS_NewFloat64(id as f64)]);
    let mut args = [cleanup];
    // SAFETY: registers the cleanup on the current owner.
    unsafe {
        let r = reactive::on_cleanup(ctx, UNDEFINED, 1, args.as_mut_ptr());
        qjs::JS_FreeValue(ctx, r);
        qjs::JS_FreeValue(ctx, cleanup);
    }
    Ok(node)
}

unsafe extern "C" fn dispose(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    let mut id = 0f64;
    // SAFETY: one data value, the list's id.
    unsafe { qjs::JS_ToFloat64(ctx, &mut id, *data) };
    let Some(data) = lists(ctx).map.borrow_mut().remove(&(id as NodeId)) else {
        return UNDEFINED;
    };
    for cell in data.cells.iter().flatten() {
        reactive::dispose_owner(ctx, cell.owner);
    }
    for &owner in &data.extras {
        reactive::dispose_owner(ctx, owner);
    }
    free_data(ctx, *data);
    UNDEFINED
}

fn set(ctx: *mut JsContext, node: JsValue, name: &str, value: JsValue) -> Result<(), JsValue> {
    let r = dom::set_prop_value(ctx, node, name, value);
    // SAFETY: ours.
    unsafe { qjs::JS_FreeValue(ctx, value) };
    r
}

/// A run of the items effect: the items set last in, the current out.
unsafe extern "C" fn splice_run(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values; values got are freed or returned.
    unsafe {
        let (node, props) = (*data, *data.add(1));
        let old = arg(argc, argv, 0);
        let next = get(ctx, props, "each");
        if qjs::JS_IsException(next) {
            return next;
        }
        let (old_len, next_len) = (length(ctx, old), length(ctx, next));
        if old_len > 0 && !qjs::JS_IsStrictEqual(ctx, old, next) {
            let max = old_len.min(next_len);
            let same = |i: i64, j: i64| {
                let a = qjs::JS_GetPropertyInt64(ctx, old, i);
                let b = qjs::JS_GetPropertyInt64(ctx, next, j);
                let s = qjs::JS_IsStrictEqual(ctx, a, b);
                qjs::JS_FreeValue(ctx, a);
                qjs::JS_FreeValue(ctx, b);
                s
            };
            let mut start = 0;
            while start < max && same(start, start) {
                start += 1;
            }
            let mut end = 0;
            while end < max - start && same(old_len - 1 - end, next_len - 1 - end) {
                end += 1;
            }
            let removed = old_len - start - end;
            let inserted = next_len - start - end;
            if removed > 0 || inserted > 0 {
                let mut values = [
                    qjs::JS_NewFloat64(start as f64),
                    qjs::JS_NewFloat64(removed as f64),
                    qjs::JS_NewFloat64(inserted as f64),
                ];
                let splice = qjs::JS_NewArrayFrom(ctx, 3, values.as_mut_ptr());
                if let Err(e) = set(ctx, node, "splice", splice) {
                    qjs::JS_FreeValue(ctx, next);
                    return e;
                }
            }
        }
        if let Err(e) = set(ctx, node, "count", qjs::JS_NewFloat64(next_len as f64)) {
            qjs::JS_FreeValue(ctx, next);
            return e;
        }
        next
    }
}

/// A run of the forwarding effect: sends the props that changed.
unsafe extern "C" fn forward_run(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: four data values; the array keeps dups of the values.
    unsafe {
        let (node, props, prev, rest) = (*data, *data.add(1), *data.add(2), *data.add(3));
        let names = FORWARDED.iter().map(|&n| (n, n)).chain([
            ("estimatedItemSize", "estimatedItemSize"),
            ("endReachedThreshold", "onEndReachedThreshold"),
        ]);
        let mut at = 0;
        for (wire, key) in names {
            if let Err(e) = forward(ctx, node, props, prev, at, wire, key) {
                return e;
            }
            at += 1;
        }
        for i in 0..length(ctx, rest) {
            let name = qjs::JS_GetPropertyInt64(ctx, rest, i);
            let s = qjs::JS_ToCString(ctx, name);
            qjs::JS_FreeValue(ctx, name);
            if s.is_null() {
                return qjs::JS_EXCEPTION;
            }
            let result = match CStr::from_ptr(s).to_str() {
                Ok(key) => forward(ctx, node, props, prev, at, key, key),
                Err(_) => Ok(()),
            };
            qjs::JS_FreeCString(ctx, s);
            if let Err(e) = result {
                return e;
            }
            at += 1;
        }
        UNDEFINED
    }
}

/// Sends prop `wire` of the list (`props[key]`) when it changed from the
/// value kept at `prev[at]`.
fn forward(
    ctx: *mut JsContext,
    node: JsValue,
    props: JsValue,
    prev: JsValue,
    at: u32,
    wire: &str,
    key: &str,
) -> Result<(), JsValue> {
    let mut value = get(ctx, props, key);
    // SAFETY: inline checks and reads of live values; `prev` keeps the
    // value, the old one is freed.
    unsafe {
        if qjs::JS_IsException(value) {
            return Err(value);
        }
        if qjs::JS_IsUndefined(value) || qjs::JS_IsNull(value) {
            value = match wire {
                "estimatedItemSize" => qjs::JS_NewFloat64(50.0),
                "endReachedThreshold" => qjs::JS_NewFloat64(1.0),
                _ => value,
            };
        }
        let old = qjs::JS_GetPropertyUint32(ctx, prev, at);
        let first = qjs::JS_IsUndefined(old)
            && !matches!(wire, "estimatedItemSize" | "endReachedThreshold");
        let result =
            if qjs::JS_IsStrictEqual(ctx, old, value) || first && qjs::JS_IsUndefined(value) {
                Ok(())
            } else {
                dom::set_prop_with_prev(ctx, node, wire, value, old)
            };
        qjs::JS_FreeValue(ctx, old);
        qjs::JS_SetPropertyUint32(ctx, prev, at, value);
        result
    }
}

/// The names of the props in `props` (borrowed) the list passes on to its
/// scroll view (all but its own and the ones it forwards by name), as an
/// array (owned). Read once: compiled JSX gives a list the same props for
/// its whole life, their values change behind getters.
fn rest_names(ctx: *mut JsContext, props: JsValue) -> Result<JsValue, JsValue> {
    // SAFETY: a new array; an enumeration of a live object's own names,
    // freed after use.
    unsafe {
        let out = qjs::JS_NewArray(ctx);
        if !qjs::JS_IsObject(props) {
            return Ok(out);
        }
        let mut tab: *mut qjs::JSPropertyEnum = ptr::null_mut();
        let mut len: u32 = 0;
        if qjs::JS_GetOwnPropertyNames(
            ctx,
            &mut tab,
            &mut len,
            props,
            (qjs::JS_GPN_STRING_MASK | qjs::JS_GPN_ENUM_ONLY) as c_int,
        ) != 0
        {
            qjs::JS_FreeValue(ctx, out);
            return Err(qjs::JS_EXCEPTION);
        }
        let mut n = 0;
        for j in 0..len as usize {
            let name = qjs::JS_AtomToString(ctx, (*tab.add(j)).atom);
            let s = qjs::JS_ToCString(ctx, name);
            let pass = !s.is_null()
                && CStr::from_ptr(s)
                    .to_str()
                    .is_ok_and(|name| !OWN.contains(&name) && !FORWARDED.contains(&name));
            if !s.is_null() {
                qjs::JS_FreeCString(ctx, s);
            }
            if pass {
                qjs::JS_SetPropertyUint32(ctx, out, n, name);
                n += 1;
            } else {
                qjs::JS_FreeValue(ctx, name);
            }
        }
        qjs::JS_FreePropertyEnum(ctx, tab, len);
        Ok(out)
    }
}

/// A list slot's value: a component is called, an element used as is.
unsafe extern "C" fn slot_run(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values (props, key); values got are freed.
    unsafe {
        let (props, key) = (*data, *data.add(1));
        let Some(key) = dom::string_arg(ctx, key) else {
            return qjs::JS_EXCEPTION;
        };
        let value = get(ctx, props, &key);
        slot(ctx, value)
    }
}

/// `value` (owned) as a list slot renders it.
fn slot(ctx: *mut JsContext, value: JsValue) -> JsValue {
    // SAFETY: an inline check; a component is called untracked.
    unsafe {
        if qjs::JS_IsFunction(ctx, value) && dom::node_id(ctx, value).is_none() {
            let mut args = [qjs::JS_NewObject(ctx)];
            let r = reactive::untracked_with(ctx, || {
                qjs::JS_Call(ctx, value, UNDEFINED, 1, args.as_mut_ptr())
            });
            qjs::JS_FreeValue(ctx, args[0]);
            qjs::JS_FreeValue(ctx, value);
            return r;
        }
    }
    value
}

/// A cell of role `role` (`header`, `footer`) showing `props[key]`, made as
/// a root under the list's owner; returns the root.
fn extra_cell(ctx: *mut JsContext, id: NodeId, role: &str, key: &str) -> Result<u32, JsValue> {
    let (props, list, owner) = {
        let map = lists(ctx).map.borrow();
        let d = &map[&id];
        (d.props, d.node, d.owner)
    };
    let (root, r) = reactive::with_root_under(ctx, owner, || -> Result<(), JsValue> {
        let cell = dom::create_element(ctx, Tag::Cell)?;
        let guard = Guard(ctx, cell);
        set(ctx, cell, "slot", new_string(ctx, role))?;
        dom::insert_node(ctx, list, cell, None)?;
        let key = new_string(ctx, key);
        let reader = native(ctx, slot_run, 0, &mut [props, key]);
        // SAFETY: ours.
        unsafe { qjs::JS_FreeValue(ctx, key) };
        let r = dom::insert_value_at(ctx, cell, reader, None);
        unsafe { qjs::JS_FreeValue(ctx, reader) };
        drop(guard);
        r
    });
    r.map(|()| root)
}

/// A JS value freed when dropped.
struct Guard(*mut JsContext, JsValue);

impl Drop for Guard {
    fn drop(&mut self) {
        // SAFETY: ours.
        unsafe { qjs::JS_FreeValue(self.0, self.1) };
    }
}

fn new_string(ctx: *mut JsContext, s: &str) -> JsValue {
    // SAFETY: a new string.
    unsafe { qjs::JS_NewStringLen(ctx, s.as_ptr().cast(), s.len() as _) }
}

/// The empty cell's effect: shown while there are no items. Its state is
/// an array: [owner, cell node].
unsafe extern "C" fn empty_run(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values; values got are freed.
    unsafe {
        let (list, props, state) = (*data, *data.add(1), *data.add(2));
        let each = get(ctx, props, "each");
        let empty = length(ctx, each) == 0;
        qjs::JS_FreeValue(ctx, each);
        let shown = length(ctx, state) > 0;
        if empty && !shown {
            let Some(id) = dom::node_id(ctx, list) else {
                return UNDEFINED;
            };
            let owner = lists(ctx).map.borrow().get(&id).map(|d| d.owner);
            let Some(owner) = owner else {
                return UNDEFINED;
            };
            let mut cell_node = UNDEFINED;
            let (root, r) = reactive::untracked_with(ctx, || {
                reactive::with_root_under(ctx, owner, || -> Result<(), JsValue> {
                    let cell = dom::create_element(ctx, Tag::Cell)?;
                    cell_node = cell;
                    set(ctx, cell, "slot", new_string(ctx, "empty"))?;
                    dom::insert_node(ctx, list, cell, None)?;
                    let key = new_string(ctx, "empty");
                    let reader = native(ctx, slot_run, 0, &mut [props, key]);
                    qjs::JS_FreeValue(ctx, key);
                    let r = dom::insert_value_at(ctx, cell, reader, None);
                    qjs::JS_FreeValue(ctx, reader);
                    r
                })
            });
            qjs::JS_SetPropertyUint32(ctx, state, 0, qjs::JS_NewFloat64(root as f64));
            qjs::JS_SetPropertyUint32(ctx, state, 1, cell_node);
            if let Err(e) = r {
                return e;
            }
        } else if !empty && shown {
            let owner = qjs::JS_GetPropertyUint32(ctx, state, 0);
            let cell = qjs::JS_GetPropertyUint32(ctx, state, 1);
            let mut root = 0f64;
            qjs::JS_ToFloat64(ctx, &mut root, owner);
            reactive::dispose_owner(ctx, root as u32);
            let r = dom::remove_node(ctx, list, cell);
            qjs::JS_FreeValue(ctx, cell);
            qjs::JS_FreeValue(ctx, owner);
            qjs::JS_SetPropertyStr(ctx, state, c"length".as_ptr(), qjs::JS_NewFloat64(0.0));
            if let Err(e) = r {
                return e;
            }
        }
        UNDEFINED
    }
}

/// Answers the core's request for rows: `(slot, item index)` pairs. `None`
/// when `list` is not one of these lists.
pub(crate) fn bind(
    ctx: *mut JsContext,
    list: NodeId,
    binds: &[(u32, u32)],
) -> Option<Result<(), ()>> {
    if !lists(ctx).map.borrow().contains_key(&list) {
        return None;
    }
    let mut failed = false;
    reactive::batch_with(ctx, || {
        for &(slot, index) in binds {
            let existing = lists(ctx)
                .map
                .borrow()
                .get(&list)
                .and_then(|d| d.cells.get(slot as usize))
                .and_then(|c| c.as_ref().map(|c| c.index));
            match existing {
                Some(signal) => {
                    if reactive::peek_number(ctx, signal) != Some(index as f64) {
                        reactive::set_signal(ctx, signal, qjs::JS_NewFloat64(index as f64));
                    }
                }
                None => {
                    if let Err(e) = make_cell(ctx, list, slot, index) {
                        // SAFETY: the pending exception is reported below.
                        let _ = e;
                        failed = true;
                    }
                }
            }
        }
    });
    let r = reactive::finish_value(ctx, UNDEFINED);
    // SAFETY: an inline tag check.
    if unsafe { qjs::JS_IsException(r) } {
        failed = true;
    }
    // After the batch: the cells' new content is in the queue already.
    for &(slot, index) in binds {
        let node = lists(ctx)
            .map
            .borrow()
            .get(&list)
            .and_then(|d| d.cells.get(slot as usize))
            .and_then(|c| c.as_ref().map(|c| c.node));
        if let Some(node) = node
            && set(ctx, node, "cellIndex", qjs::JS_NewFloat64(index as f64)).is_err()
        {
            failed = true;
        }
    }
    Some(if failed { Err(()) } else { Ok(()) })
}

/// A new cell for `slot`, showing item `first`.
fn make_cell(ctx: *mut JsContext, list: NodeId, slot: u32, first: u32) -> Result<(), JsValue> {
    let (props, list_node, owner) = {
        let map = lists(ctx).map.borrow();
        let d = &map[&list];
        (d.props, d.node, d.owner)
    };
    let mut made = None;
    let (root, r) = reactive::with_root_under(ctx, owner, || -> Result<(), JsValue> {
        let index = reactive::new_signal(ctx, qjs::JS_NewFloat64(first as f64));
        let item_fn = native(ctx, item_run, 0, &mut [props, index]);
        let item = reactive::memo(ctx, item_fn, false);
        let cell = dom::create_element(ctx, Tag::Cell)?;
        made = Some((cell, index));
        set(ctx, cell, "slot", qjs::JS_NewFloat64(slot as f64))?;
        dom::insert_node(ctx, list_node, cell, None)?;
        let children = get(ctx, props, "children");
        let mut args = [item, index];
        // SAFETY: the row function, called untracked; values are freed.
        let content = reactive::untracked_with(ctx, || unsafe {
            qjs::JS_Call(ctx, children, UNDEFINED, 2, args.as_mut_ptr())
        });
        unsafe {
            qjs::JS_FreeValue(ctx, children);
            qjs::JS_FreeValue(ctx, item);
            if qjs::JS_IsException(content) {
                return Err(content);
            }
        }
        let r = if has(ctx, props, "separator") {
            // SAFETY: a placeholder for the separator once made.
            let holder = unsafe { qjs::JS_NewArray(ctx) };
            let between = native(ctx, separated_run, 0, &mut [content, props, index, holder]);
            unsafe { qjs::JS_FreeValue(ctx, holder) };
            let r = dom::insert_value_at(ctx, cell, between, None);
            unsafe { qjs::JS_FreeValue(ctx, between) };
            r
        } else {
            dom::insert_value_at(ctx, cell, content, None)
        };
        unsafe { qjs::JS_FreeValue(ctx, content) };
        r
    });
    if let Some((node, index)) = made {
        let mut map = lists(ctx).map.borrow_mut();
        match map.get_mut(&list) {
            Some(d) => {
                let slot = slot as usize;
                if d.cells.len() <= slot {
                    d.cells.resize_with(slot + 1, || None);
                }
                d.cells[slot] = Some(Cell {
                    node,
                    index,
                    owner: root,
                });
            }
            None => {
                drop(map);
                // SAFETY: ours (the list went meanwhile).
                unsafe {
                    qjs::JS_FreeValue(ctx, node);
                    qjs::JS_FreeValue(ctx, index);
                }
            }
        }
    }
    r
}

/// A cell's item: `props.each[index()]`, or the last one past the end (the
/// data shrank before the core unbound the cell).
unsafe extern "C" fn item_run(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values; values got are freed or returned.
    unsafe {
        let (props, index) = (*data, *data.add(1));
        let i = qjs::JS_Call(ctx, index, UNDEFINED, 0, ptr::null_mut());
        let mut n = 0f64;
        qjs::JS_ToFloat64(ctx, &mut n, i);
        qjs::JS_FreeValue(ctx, i);
        let each = get(ctx, props, "each");
        let len = length(ctx, each);
        let r = if n >= 0.0 && (n as i64) < len {
            qjs::JS_GetPropertyInt64(ctx, each, n as i64)
        } else {
            qjs::JS_DupValue(ctx, arg(argc, argv, 0))
        };
        qjs::JS_FreeValue(ctx, each);
        r
    }
}

/// A row with a separator after it (before it in an inverted list), but
/// the last one.
unsafe extern "C" fn separated_run(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: four data values; values got are freed or returned.
    unsafe {
        let (content, props, index, holder) = (*data, *data.add(1), *data.add(2), *data.add(3));
        let i = qjs::JS_Call(ctx, index, UNDEFINED, 0, ptr::null_mut());
        let mut n = 0f64;
        qjs::JS_ToFloat64(ctx, &mut n, i);
        qjs::JS_FreeValue(ctx, i);
        let each = get(ctx, props, "each");
        let len = length(ctx, each);
        qjs::JS_FreeValue(ctx, each);
        let component = get(ctx, props, "separator");
        let shown =
            (n as i64) < len - 1 && !qjs::JS_IsNull(component) && !qjs::JS_IsUndefined(component);
        if !shown {
            qjs::JS_FreeValue(ctx, component);
            return qjs::JS_DupValue(ctx, content);
        }
        let mut separator = qjs::JS_GetPropertyUint32(ctx, holder, 0);
        if qjs::JS_IsUndefined(separator) {
            separator = reactive::untracked_with(ctx, || slot(ctx, component));
            if qjs::JS_IsException(separator) {
                return separator;
            }
            qjs::JS_SetPropertyUint32(ctx, holder, 0, qjs::JS_DupValue(ctx, separator));
        } else {
            qjs::JS_FreeValue(ctx, component);
        }
        let inverted = get(ctx, props, "inverted");
        let inverted_now = reactive::untracked_with(ctx, || qjs::JS_ToBool(ctx, inverted) == 1);
        qjs::JS_FreeValue(ctx, inverted);
        let mut pair = if inverted_now {
            [separator, qjs::JS_DupValue(ctx, content)]
        } else {
            [qjs::JS_DupValue(ctx, content), separator]
        };
        qjs::JS_NewArrayFrom(ctx, 2, pair.as_mut_ptr())
    }
}

// --- Viewability ------------------------------------------------------------------

unsafe extern "C" fn on_viewable(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    let mut id = 0f64;
    // SAFETY: one data value (the list id); the event's fields are read.
    unsafe { qjs::JS_ToFloat64(ctx, &mut id, *data) };
    let id = id as NodeId;
    let event = arg(argc, argv, 0);
    let read = |key: &str| {
        let v = get(ctx, event, key);
        let mut n = -1f64;
        // SAFETY: converting a live value; freed after.
        unsafe {
            qjs::JS_ToFloat64(ctx, &mut n, v);
            qjs::JS_FreeValue(ctx, v);
        }
        n as i64
    };
    let latest = (read("first"), read("last"));
    let props = {
        let mut map = lists(ctx).map.borrow_mut();
        let Some(d) = map.get_mut(&id) else {
            return UNDEFINED;
        };
        d.latest = latest;
        // SAFETY: a dup for the reads below.
        unsafe { qjs::JS_DupValue(ctx, d.props) }
    };
    let config = get(ctx, props, "viewabilityConfig");
    let wait = get(ctx, config, "minimumViewTime");
    let mut ms = 0f64;
    // SAFETY: values got are freed.
    unsafe {
        qjs::JS_ToFloat64(ctx, &mut ms, wait);
        qjs::JS_FreeValue(ctx, wait);
        qjs::JS_FreeValue(ctx, config);
        qjs::JS_FreeValue(ctx, props);
    }
    if ms.is_nan() || ms <= 0.0 {
        return report(ctx, id);
    }
    // Reported once the range stayed `minimumViewTime`.
    let previous = lists(ctx)
        .map
        .borrow_mut()
        .get_mut(&id)
        .and_then(|d| d.timer.take());
    let timers = |name: &CStr, args: &mut [JsValue]| -> JsValue {
        // SAFETY: calls a global timer function; values are freed.
        unsafe {
            let global = qjs::JS_GetGlobalObject(ctx);
            let f = qjs::JS_GetPropertyStr(ctx, global, name.as_ptr());
            let r = qjs::JS_Call(ctx, f, UNDEFINED, args.len() as c_int, args.as_mut_ptr());
            qjs::JS_FreeValue(ctx, f);
            qjs::JS_FreeValue(ctx, global);
            r
        }
    };
    if let Some(timer) = previous {
        let r = timers(c"clearTimeout", &mut [qjs::JS_NewFloat64(timer)]);
        unsafe { qjs::JS_FreeValue(ctx, r) };
    }
    let fire = native(ctx, report_later, 0, &mut [qjs::JS_NewFloat64(id as f64)]);
    let timer = timers(c"setTimeout", &mut [fire, qjs::JS_NewFloat64(ms)]);
    let mut t = 0f64;
    // SAFETY: values made here are freed.
    unsafe {
        qjs::JS_FreeValue(ctx, fire);
        qjs::JS_ToFloat64(ctx, &mut t, timer);
        qjs::JS_FreeValue(ctx, timer);
    }
    if let Some(d) = lists(ctx).map.borrow_mut().get_mut(&id) {
        d.timer = Some(t);
    }
    UNDEFINED
}

unsafe extern "C" fn report_later(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    let mut id = 0f64;
    // SAFETY: one data value (the list id).
    unsafe { qjs::JS_ToFloat64(ctx, &mut id, *data) };
    if let Some(d) = lists(ctx).map.borrow_mut().get_mut(&(id as NodeId)) {
        d.timer = None;
    }
    report(ctx, id as NodeId)
}

/// Calls `onViewableItemsChanged` with the items now viewable and those
/// that changed, unless the range is the one reported last.
fn report(ctx: *mut JsContext, id: NodeId) -> JsValue {
    let (props, (first, last), (prev_first, prev_last)) = {
        let mut map = lists(ctx).map.borrow_mut();
        let Some(d) = map.get_mut(&id) else {
            return UNDEFINED;
        };
        if d.latest == d.reported {
            return UNDEFINED;
        }
        let r = (
            // SAFETY: a dup for the reads below.
            unsafe { qjs::JS_DupValue(ctx, d.props) },
            d.latest,
            d.reported,
        );
        d.reported = d.latest;
        r
    };
    let each = get(ctx, props, "each");
    let key_of = get(ctx, props, "keyExtractor");
    let len = length(ctx, each);
    // SAFETY: tokens are new objects; values got are freed or moved.
    let token = |index: i64, viewable: bool| -> JsValue {
        unsafe {
            let item = qjs::JS_GetPropertyInt64(ctx, each, index);
            let key = if qjs::JS_IsFunction(ctx, key_of) {
                let mut args = [item, qjs::JS_NewFloat64(index as f64)];
                qjs::JS_Call(ctx, key_of, UNDEFINED, 2, args.as_mut_ptr())
            } else {
                let mut k = get(ctx, item, "key");
                if qjs::JS_IsUndefined(k) || qjs::JS_IsNull(k) {
                    qjs::JS_FreeValue(ctx, k);
                    k = get(ctx, item, "id");
                }
                if qjs::JS_IsUndefined(k) || qjs::JS_IsNull(k) {
                    qjs::JS_FreeValue(ctx, k);
                    k = qjs::JS_NewFloat64(index as f64);
                }
                let s = qjs::JS_ToString(ctx, k);
                qjs::JS_FreeValue(ctx, k);
                s
            };
            let o = qjs::JS_NewObject(ctx);
            qjs::JS_SetPropertyStr(ctx, o, c"item".as_ptr(), item);
            qjs::JS_SetPropertyStr(ctx, o, c"key".as_ptr(), key);
            qjs::JS_SetPropertyStr(ctx, o, c"index".as_ptr(), qjs::JS_NewFloat64(index as f64));
            qjs::JS_SetPropertyStr(
                ctx,
                o,
                c"isViewable".as_ptr(),
                if viewable {
                    qjs::JS_TRUE
                } else {
                    qjs::JS_FALSE
                },
            );
            o
        }
    };
    let mut viewable = Vec::new();
    let mut changed = Vec::new();
    if first >= 0 {
        for i in first..=last.min(len - 1) {
            viewable.push(token(i, true));
            if i < prev_first || i > prev_last {
                changed.push(token(i, true));
            }
        }
    }
    if prev_first >= 0 {
        for i in prev_first..=prev_last.min(len - 1) {
            if i < first || i > last {
                changed.push(token(i, false));
            }
        }
    }
    // SAFETY: the info object takes the arrays; the handler is called and
    // values are freed.
    unsafe {
        let info = qjs::JS_NewObject(ctx);
        let mut v = viewable;
        let mut c = changed;
        qjs::JS_SetPropertyStr(
            ctx,
            info,
            c"viewableItems".as_ptr(),
            qjs::JS_NewArrayFrom(ctx, v.len() as c_int, v.as_mut_ptr()),
        );
        qjs::JS_SetPropertyStr(
            ctx,
            info,
            c"changed".as_ptr(),
            qjs::JS_NewArrayFrom(ctx, c.len() as c_int, c.as_mut_ptr()),
        );
        let handler = get(ctx, props, "onViewableItemsChanged");
        let mut args = [info];
        let r = if qjs::JS_IsFunction(ctx, handler) {
            qjs::JS_Call(ctx, handler, UNDEFINED, 1, args.as_mut_ptr())
        } else {
            UNDEFINED
        };
        for v in [handler, info, each, key_of, props] {
            qjs::JS_FreeValue(ctx, v);
        }
        r
    }
}

/// `nsh/list`: `List` (its tags compile to `nsh:rt`'s `list`, the same
/// function).
pub(crate) const EXPORTS: &[crate::modules::Item] = &[(
    "List",
    crate::modules::Export::Function(list, 1),
    crate::modules::Ts::function(
        "A virtualized, recycling list on a native scroll view. Only rows near the\n\
         viewport exist; the native side decides which, and cells are reused.\n\
         Positions, columns, sticky headers and viewability are computed natively.\n\n\
         Rows are recycled: keep per-item state in your data (keyed by id), not in\n\
         signals created inside the row.",
        &["<T>(props: ListProps<T>): YNode"],
    ),
)];

/// `nsh/list`'s types.
pub(crate) const TYPES: &str = r#"/** A component (`Header`) or an element (`<Header />`). */
export type ListSlot = (() => JSX.Element) | JSX.Element;

/** An item that became viewable or stopped being viewable. */
export interface ViewToken<T> {
  item: T;
  key: string;
  index: number;
  isViewable: boolean;
}

export interface ViewabilityConfig {
  /** Percent of the viewport an item must cover to count as viewable. */
  viewAreaCoveragePercentThreshold?: number;
  /** Percent of an item that must be visible. Without either threshold, any visible pixel counts. */
  itemVisiblePercentThreshold?: number;
  /** Milliseconds items must stay viewable before they are reported. */
  minimumViewTime?: number;
}

/** A list's props: its own, and those of the scroll view it is (the test
 * id, accessibility, classes, touch and layout events, ...), which it
 * passes on as they are. */
export interface ListProps<T> extends Omit<ViewProps, "children">, ScrollingHostProps, ListHostProps {
  each: readonly T[];
  /** Above the items, scrolling with them. */
  header?: ListSlot;
  /** Below the items, scrolling with them. */
  footer?: ListSlot;
  /** Shown instead of the items while `each` is empty. */
  empty?: ListSlot;
  /** Rendered between items (one column). */
  separator?: ListSlot;
  /** Scrolls to this item before the first frame. */
  initialScrollIndex?: number;
  /** Key of an item in `onViewableItemsChanged` tokens (default: `item.key`, `item.id`, or the index). */
  keyExtractor?: (item: T, index: number) => string;
  onViewableItemsChanged?: (info: { viewableItems: ViewToken<T>[]; changed: ViewToken<T>[] }) => void;
  style?: StyleProp<ViewStyle>;
  /** Padding and gaps of the content as classes (`p-*`, `gap-*`);
   * `contentStyle` wins over them. */
  contentClass?: string;
  /** The id UI tests find the list's scroll view by (`nsh test`'s `id:`, XCUITest, UI Automator). */
  testID?: string;
  /** How close to the end (in viewports) `onEndReached` fires; default 1. */
  onEndReachedThreshold?: number;
  ref?: YNode | ((node: YNode) => void);
  /** Renders one row. Called once per recycled cell; `item` and `index`
   * change when the cell is reused for another item. */
  children: (item: () => T, index: () => number) => JSX.Element;
}

/** What an app passes to a Rust component that lays out a list of rows
 * (an `Items` prop): the data and the rows; the component decides the
 * layout. */
export type ListItemsProps<T> = Pick<ListProps<T>, "each" | "children" | "keyExtractor" | "stickyHeaderIndices" | "initialScrollIndex" | "onViewableItemsChanged" | "viewabilityConfig" | "refreshing" | "onRefresh" | "onEndReached" | "onEndReachedThreshold" | "header" | "footer" | "empty">;
"#;

#[cfg(test)]
mod tests {
    use crate::{Host, HostOptions};

    /// A list's slots are its own: it makes each once and never passes
    /// them to its scroll view (as compiled JSX passes them: getters that
    /// make the slot's elements each time they are read).
    #[test]
    fn a_lists_slots_stay_with_the_list() {
        let host = Host::with_options(HostOptions::default()).unwrap();
        host.eval_source(
            "import { mount } from \"nsh\";\n\
             import { text } from \"nsh:rt\";\n\
             import { List } from \"nsh/list\";\n\
             mount(() => List({\n\
               each: [1, 2],\n\
               children: (n) => text(`Row ${n()}`),\n\
               get header() { return text(\"Head\"); },\n\
               get footer() { return text(\"Foot\"); },\n\
               get empty() { return text(\"None\"); },\n\
               get separator() { return () => text(\"-\"); },\n\
             }));",
        )
        .unwrap();
        for name in ["header", "footer", "empty", "separator"] {
            assert!(
                !host.warned(&format!("unsupported prop `{name}`")),
                "`{name}` went to the scroll view"
            );
        }
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let made = |wanted: &str| {
            ops.iter()
                .filter(
                    |op| matches!(op, nsh_core::Op::CreateText { text, .. } if &**text == wanted),
                )
                .count()
        };
        assert_eq!((made("Head"), made("Foot")), (1, 1));
    }
}

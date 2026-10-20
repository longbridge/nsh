//! Touches and the gesture responder system, and `PanResponder`
//! (`nsh/gesture`), in Rust as it runs on every touch move.
//!
//! Native views whose node has a touch or responder handler listen to
//! touches (`onTouch`); their touches (and those on views inside them)
//! arrive with the path of listening views from the target outwards.
//! Handlers then run as in React Native: touch handlers capture inwards and
//! bubble outwards, and the responder is negotiated with the
//! `…ShouldSetResponder` handlers, which also get the gesture since the
//! first finger went down (`gestureState`, as `PanResponder`'s). A view with
//! `panActivation` is not asked: it becomes the responder when the platform
//! activates its pan (`TouchEvent::activated`).

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, c_int},
};

use nsh_core::{Command, NodeId, Op, TouchEvent, TouchPhase, TouchPoint, decl, ui::PanGesture};
use rquickjs::qjs;
use rustc_hash::FxHashMap;

use crate::{
    dom,
    modules::{Export, Item, Ts},
    reactive::{self, arg},
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

const UNDEFINED: JsValue = qjs::JS_UNDEFINED;

/// The touch and responder props: name, handler type and documentation
/// (the declarations of `TouchProps`).
pub(crate) const TOUCH_PROPS: [(&str, &str, &str); 20] = [
    (
        "onTouchStart",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onTouchMove",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onTouchEnd",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onTouchCancel",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onTouchStartCapture",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onTouchMoveCapture",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onTouchEndCapture",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onTouchCancelCapture",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onStartShouldSetResponder",
        decl!("(e: GestureResponderEvent, gestureState: ResponderGestureState) => boolean"),
        decl!("Become the responder when a finger goes down (asked from the target outwards)."),
    ),
    (
        "onMoveShouldSetResponder",
        decl!("(e: GestureResponderEvent, gestureState: ResponderGestureState) => boolean"),
        decl!(
            "Become the responder when a finger moves (`gestureState`: the gesture since the first finger went down, e.g. `Math.abs(g.dx) > 10`)."
        ),
    ),
    (
        "onStartShouldSetResponderCapture",
        decl!("(e: GestureResponderEvent, gestureState: ResponderGestureState) => boolean"),
        decl!("As above, asked from the outside in, before the bubbling ones."),
    ),
    (
        "onMoveShouldSetResponderCapture",
        decl!("(e: GestureResponderEvent, gestureState: ResponderGestureState) => boolean"),
        "",
    ),
    (
        "onResponderGrant",
        decl!("(e: GestureResponderEvent) => boolean | void"),
        decl!(
            "This view is the responder now. Return true to keep native scroll views from taking the touches."
        ),
    ),
    (
        "onResponderReject",
        decl!("(e: GestureResponderEvent) => void"),
        decl!("Another view is the responder and would not let go."),
    ),
    (
        "onResponderStart",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onResponderMove",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onResponderEnd",
        decl!("(e: GestureResponderEvent) => void"),
        "",
    ),
    (
        "onResponderRelease",
        decl!("(e: GestureResponderEvent) => void"),
        decl!("The last finger lifted."),
    ),
    (
        "onResponderTerminate",
        decl!("(e: GestureResponderEvent) => void"),
        decl!("Another view (or a native gesture) took the touches."),
    ),
    (
        "onResponderTerminationRequest",
        decl!("(e: GestureResponderEvent) => boolean"),
        decl!("Another view wants the touches: return false to keep them (default true)."),
    ),
];

/// The index of touch prop `name`.
pub(crate) fn touch_prop(name: &str) -> Option<u8> {
    TOUCH_PROPS
        .iter()
        .position(|p| p.0 == name)
        .map(|i| i as u8)
}

fn index(name: &str) -> u8 {
    touch_prop(name).expect("a touch prop")
}

/// The touches' part of a context's state.
#[derive(Default)]
pub(crate) struct Touches {
    /// Listening views around each finger, innermost first.
    paths: RefCell<FxHashMap<u32, Vec<NodeId>>>,
    responder: Cell<Option<NodeId>>,
    /// The responder keeps native scroll views from taking the touches.
    blocks: Cell<bool>,
    /// `PanResponder` gesture state ids.
    pan_states: Cell<u32>,
    /// The gesture since the first finger went down, for the negotiation.
    gesture: Cell<PanGesture>,
}

fn touches<'a>(ctx: *mut JsContext) -> &'a Touches {
    // SAFETY: a projection of the state's field, by shared reference.
    unsafe { &(*crate::state::state(ctx)).touches }
}

fn number(v: f64) -> JsValue {
    qjs::JS_NewFloat64(v)
}

fn set(ctx: *mut JsContext, obj: JsValue, key: &CStr, v: JsValue) {
    // SAFETY: the object takes `v`.
    unsafe { qjs::JS_SetPropertyStr(ctx, obj, key.as_ptr(), v) };
}

fn get(ctx: *mut JsContext, obj: JsValue, key: &CStr) -> JsValue {
    // SAFETY: a property read of a live value.
    unsafe {
        if !qjs::JS_IsObject(obj) {
            return UNDEFINED;
        }
        qjs::JS_GetPropertyStr(ctx, obj, key.as_ptr())
    }
}

fn get_number(ctx: *mut JsContext, obj: JsValue, key: &CStr) -> f64 {
    let v = get(ctx, obj, key);
    let mut n = 0f64;
    // SAFETY: converting and freeing a value got.
    unsafe {
        qjs::JS_ToFloat64(ctx, &mut n, v);
        qjs::JS_FreeValue(ctx, v);
    }
    n
}

fn free(ctx: *mut JsContext, v: JsValue) {
    // SAFETY: an owned value.
    unsafe { qjs::JS_FreeValue(ctx, v) };
}

/// A touch point as JS sees it.
fn point(ctx: *mut JsContext, p: &TouchPoint, timestamp: f64) -> JsValue {
    // SAFETY: a new object.
    let o = unsafe { qjs::JS_NewObject(ctx) };
    set(ctx, o, c"identifier", number(p.id as f64));
    set(ctx, o, c"target", number(p.target as f64));
    set(ctx, o, c"pageX", number(p.page.x.0 as f64));
    set(ctx, o, c"pageY", number(p.page.y.0 as f64));
    set(ctx, o, c"locationX", number(p.location.x.0 as f64));
    set(ctx, o, c"locationY", number(p.location.y.0 as f64));
    set(ctx, o, c"timestamp", number(timestamp));
    o
}

fn array(ctx: *mut JsContext, mut values: Vec<JsValue>) -> JsValue {
    // SAFETY: the array takes the values.
    unsafe { qjs::JS_NewArrayFrom(ctx, values.len() as c_int, values.as_mut_ptr()) }
}

/// `e.stopPropagation()`.
unsafe extern "C" fn stop_propagation(
    ctx: *mut JsContext,
    this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
) -> JsValue {
    set(ctx, this, c"stopped", qjs::JS_TRUE);
    UNDEFINED
}

/// The event handlers get: about `about`, with `touches` down and
/// `changed`.
fn event(
    ctx: *mut JsContext,
    about: &TouchPoint,
    touches: &[&TouchPoint],
    changed: &[&TouchPoint],
    timestamp: f64,
) -> JsValue {
    // SAFETY: a new object taking its fields.
    let e = unsafe { qjs::JS_NewObject(ctx) };
    set(ctx, e, c"identifier", number(about.id as f64));
    set(ctx, e, c"locationX", number(about.location.x.0 as f64));
    set(ctx, e, c"locationY", number(about.location.y.0 as f64));
    set(ctx, e, c"pageX", number(about.page.x.0 as f64));
    set(ctx, e, c"pageY", number(about.page.y.0 as f64));
    set(ctx, e, c"target", number(about.target as f64));
    set(ctx, e, c"timestamp", number(timestamp));
    let list =
        |ps: &[&TouchPoint]| array(ctx, ps.iter().map(|p| point(ctx, p, timestamp)).collect());
    set(ctx, e, c"touches", list(touches));
    set(ctx, e, c"changedTouches", list(changed));
    set(ctx, e, c"stopped", qjs::JS_FALSE);
    set(
        ctx,
        e,
        c"stopPropagation",
        reactive::new_function(ctx, c"stopPropagation", stop_propagation, 0),
    );
    // SAFETY: a dup the object holds (a cycle the GC collects).
    set(ctx, e, c"nativeEvent", unsafe { qjs::JS_DupValue(ctx, e) });
    e
}

fn stopped(ctx: *mut JsContext, e: JsValue) -> bool {
    let v = get(ctx, e, c"stopped");
    // SAFETY: an inline check of a value got, then freed.
    let s = unsafe { qjs::JS_ToBool(ctx, v) == 1 };
    free(ctx, v);
    s
}

/// Calls node `id`'s handler `prop` with `e`; its result (owned).
fn call(ctx: *mut JsContext, id: NodeId, prop: u8, e: JsValue) -> Result<JsValue, JsValue> {
    call_with(ctx, id, prop, &mut [e])
}

/// Calls node `id`'s handler `prop` with `args` (borrowed); its result
/// (owned).
fn call_with(
    ctx: *mut JsContext,
    id: NodeId,
    prop: u8,
    args: &mut [JsValue],
) -> Result<JsValue, JsValue> {
    let Some(f) = dom::touch_handler(ctx, id, prop) else {
        return Ok(UNDEFINED);
    };
    // SAFETY: an owned function, called then freed.
    let r = unsafe { qjs::JS_Call(ctx, f, UNDEFINED, args.len() as c_int, args.as_mut_ptr()) };
    free(ctx, f);
    // SAFETY: an inline tag check.
    if unsafe { qjs::JS_IsException(r) } {
        Err(r)
    } else {
        Ok(r)
    }
}

/// Calls `prop` with `args` and tells whether it returned `true`.
fn asks(ctx: *mut JsContext, id: NodeId, prop: u8, args: &mut [JsValue]) -> Result<bool, JsValue> {
    let r = call_with(ctx, id, prop, args)?;
    // SAFETY: an inline check of an owned value.
    let yes = unsafe { qjs::JS_IsBool(r) && qjs::JS_VALUE_GET_BOOL(r) };
    free(ctx, r);
    Ok(yes)
}

fn run(ctx: *mut JsContext, id: NodeId, prop: u8, e: JsValue) -> Result<(), JsValue> {
    call(ctx, id, prop, e).map(|r| free(ctx, r))
}

/// `name` on `path` (innermost first): capture outside in, then bubble.
fn two_phase(ctx: *mut JsContext, path: &[NodeId], name: &str, e: JsValue) -> Result<(), JsValue> {
    let capture = index(&format!("{name}Capture"));
    let bubble = index(name);
    for &id in path.iter().rev() {
        if stopped(ctx, e) {
            return Ok(());
        }
        run(ctx, id, capture, e)?;
    }
    for &id in path {
        if stopped(ctx, e) {
            return Ok(());
        }
        run(ctx, id, bubble, e)?;
    }
    Ok(())
}

fn command(ctx: *mut JsContext, id: NodeId, command: Command) -> Result<(), JsValue> {
    dom::push_op(ctx, Op::Command { id, command })
}

fn grant(ctx: *mut JsContext, id: NodeId, e: JsValue) -> Result<(), JsValue> {
    let t = touches(ctx);
    t.responder.set(Some(id));
    let block = asks(ctx, id, index("onResponderGrant"), &mut [e])?;
    if block {
        command(ctx, id, Command::SetJsResponder { block: true })?;
    } else if t.blocks.get() {
        command(ctx, 1, Command::ClearJsResponder)?;
    }
    t.blocks.set(block);
    Ok(())
}

fn release(ctx: *mut JsContext) -> Result<(), JsValue> {
    let t = touches(ctx);
    t.responder.set(None);
    if t.blocks.replace(false) {
        command(ctx, 1, Command::ClearJsResponder)?;
    }
    Ok(())
}

/// The negotiation's `gestureState` (owned): the gesture since the first
/// finger went down.
fn gesture_state(ctx: *mut JsContext) -> JsValue {
    let g = touches(ctx).gesture.get();
    // SAFETY: a new object taking its fields.
    let o = unsafe { qjs::JS_NewObject(ctx) };
    for (k, v) in [
        (c"moveX", g.move_x),
        (c"moveY", g.move_y),
        (c"x0", g.x0),
        (c"y0", g.y0),
        (c"dx", g.dx),
        (c"dy", g.dy),
        (c"vx", g.vx),
        (c"vy", g.vy),
    ] {
        set(ctx, o, k, number(v as f64));
    }
    set(
        ctx,
        o,
        c"numberActiveTouches",
        number(g.active_touches as f64),
    );
    o
}

/// Asks `path` whether a view wants to become the responder (`kind`:
/// `onStartShouldSetResponder` or `onMoveShouldSetResponder`), from the
/// views around both the target and the current responder, the responder
/// itself and views waiting for their pan excluded.
fn negotiate(ctx: *mut JsContext, kind: &str, path: &[NodeId], e: JsValue) -> Result<(), JsValue> {
    let responder = touches(ctx).responder.get();
    let mut from = 0;
    if let Some(r) = responder {
        while from < path.len() && !dom::is_ancestor_of(ctx, path[from], r) {
            from += 1;
        }
        if path.get(from) == Some(&r) {
            from += 1;
        }
    }
    let capture = index(&format!("{kind}Capture"));
    let bubble = index(kind);
    let asked = &path[from.min(path.len())..];
    // Made for the first handler asked.
    let mut gs = UNDEFINED;
    let mut ask = |id: NodeId, prop: u8| -> Result<bool, JsValue> {
        if dom::pan_gated(ctx, id) || !dom::touch_handler_exists(ctx, id, prop) {
            return Ok(false);
        }
        // SAFETY: an inline tag check.
        if unsafe { qjs::JS_IsUndefined(gs) } {
            gs = gesture_state(ctx);
        }
        asks(ctx, id, prop, &mut [e, gs])
    };
    let mut wanting = None;
    let mut result = Ok(());
    for &id in asked.iter().rev() {
        match ask(id, capture) {
            Ok(false) => {}
            Ok(true) => {
                wanting = Some(id);
                break;
            }
            Err(x) => {
                result = Err(x);
                break;
            }
        }
    }
    if wanting.is_none() && result.is_ok() {
        for &id in asked {
            match ask(id, bubble) {
                Ok(false) => {}
                Ok(true) => {
                    wanting = Some(id);
                    break;
                }
                Err(x) => {
                    result = Err(x);
                    break;
                }
            }
        }
    }
    free(ctx, gs);
    result?;
    match wanting {
        Some(wanting) if Some(wanting) != responder => take_over(ctx, wanting, e),
        _ => Ok(()),
    }
}

/// View `wanting` wants the touches: it becomes the responder unless the
/// current one refuses to let go.
fn take_over(ctx: *mut JsContext, wanting: NodeId, e: JsValue) -> Result<(), JsValue> {
    if let Some(r) = touches(ctx).responder.get() {
        let request = index("onResponderTerminationRequest");
        if dom::touch_handler_exists(ctx, r, request) {
            let r_ok = call(ctx, r, request, e)?;
            // SAFETY: an inline check of an owned value.
            let refused = unsafe { qjs::JS_IsBool(r_ok) && !qjs::JS_VALUE_GET_BOOL(r_ok) };
            free(ctx, r_ok);
            if refused {
                return run(ctx, wanting, index("onResponderReject"), e);
            }
        }
        run(ctx, r, index("onResponderTerminate"), e)?;
    }
    grant(ctx, wanting, e)
}

/// Delivers a touch event to the JS handlers along its path.
pub(crate) fn dispatch(ctx: *mut JsContext, touch: &TouchEvent) -> Result<(), JsValue> {
    let t = touches(ctx);
    let phase = touch.phase;
    for p in &touch.points {
        if phase == TouchPhase::Start && p.changed {
            let path: Vec<NodeId> = p
                .path
                .iter()
                .copied()
                .filter(|&id| dom::listens_to_touches(ctx, id))
                .collect();
            t.paths.borrow_mut().insert(p.id, path);
        }
    }
    let lifted = matches!(phase, TouchPhase::End | TouchPhase::Cancel);
    let mut gesture = t.gesture.get();
    match phase {
        // The gesture starts with the first finger.
        TouchPhase::Start if touch.points.len() == 1 => gesture = PanGesture::begin(touch),
        TouchPhase::Start | TouchPhase::Move => gesture.update(touch),
        TouchPhase::End | TouchPhase::Cancel => {}
    }
    t.gesture.set(gesture);
    let down: Vec<&TouchPoint> = touch
        .points
        .iter()
        .filter(|p| !lifted || !p.changed)
        .collect();
    let changed: Vec<&TouchPoint> = touch.points.iter().filter(|p| p.changed).collect();
    let Some(about) = changed.first().copied() else {
        return Ok(());
    };
    let path = t.paths.borrow().get(&about.id).cloned().unwrap_or_default();
    let e = event(ctx, about, &down, &changed, touch.timestamp);
    let result = (|| -> Result<(), JsValue> {
        let raw = match phase {
            TouchPhase::Start => "onTouchStart",
            TouchPhase::Move => "onTouchMove",
            TouchPhase::End => "onTouchEnd",
            TouchPhase::Cancel => "onTouchCancel",
        };
        two_phase(ctx, &path, raw, e)?;
        set(ctx, e, c"stopped", qjs::JS_FALSE);
        // A pan the platform activated: its view takes the touches.
        if let Some(id) = touch.activated
            && dom::pan_gated(ctx, id)
            && dom::listens_to_touches(ctx, id)
            && touches(ctx).responder.get() != Some(id)
        {
            take_over(ctx, id, e)?;
        }
        match phase {
            TouchPhase::Start => negotiate(ctx, "onStartShouldSetResponder", &path, e)?,
            TouchPhase::Move => negotiate(ctx, "onMoveShouldSetResponder", &path, e)?,
            _ => {}
        }
        if let Some(r) = touches(ctx).responder.get() {
            if phase == TouchPhase::Cancel {
                run(ctx, r, index("onResponderTerminate"), e)?;
                release(ctx)?;
            } else {
                let handler = match phase {
                    TouchPhase::Start => "onResponderStart",
                    TouchPhase::Move => "onResponderMove",
                    _ => "onResponderEnd",
                };
                run(ctx, r, index(handler), e)?;
                if phase == TouchPhase::End && down.is_empty() {
                    release(ctx)?;
                    run(ctx, r, index("onResponderRelease"), e)?;
                }
            }
        }
        Ok(())
    })();
    free(ctx, e);
    if lifted {
        let mut paths = touches(ctx).paths.borrow_mut();
        for p in &changed {
            paths.remove(&p.id);
        }
    }
    result
}

// --- PanResponder -----------------------------------------------------------------

/// The pan handlers, by magic.
const PAN_HANDLERS: [&CStr; 12] = [
    c"onStartShouldSetResponderCapture",
    c"onStartShouldSetResponder",
    c"onMoveShouldSetResponderCapture",
    c"onMoveShouldSetResponder",
    c"onResponderGrant",
    c"onResponderReject",
    c"onResponderStart",
    c"onResponderMove",
    c"onResponderEnd",
    c"onResponderRelease",
    c"onResponderTerminate",
    c"onResponderTerminationRequest",
];

/// `PanResponder.create(config)`: `{ panHandlers }` tracking a pan's
/// gesture state (`dx`, `vx`, ...) across the responder's events.
unsafe extern "C" fn pan_create(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let config = arg(argc, argv, 0);
    let t = touches(ctx);
    let id = t.pan_states.get() + 1;
    t.pan_states.set(id);
    // SAFETY: new objects; the handlers keep dups of `data`.
    unsafe {
        let gs = qjs::JS_NewObject(ctx);
        set(ctx, gs, c"stateID", number(id as f64));
        for key in [
            c"moveX",
            c"moveY",
            c"x0",
            c"y0",
            c"dx",
            c"dy",
            c"vx",
            c"vy",
            c"numberActiveTouches",
        ] {
            set(ctx, gs, key, number(0.0));
        }
        // The centroid, time and touch count of the last update.
        let last = qjs::JS_NewObject(ctx);
        let handlers = qjs::JS_NewObject(ctx);
        for (magic, name) in PAN_HANDLERS.iter().enumerate() {
            let mut data = [config, gs, last];
            let f = qjs::JS_NewCFunctionData(
                ctx,
                Some(pan_handler),
                1,
                magic as c_int,
                3,
                data.as_mut_ptr(),
            );
            set(ctx, handlers, name, f);
        }
        free(ctx, gs);
        free(ctx, last);
        let r = qjs::JS_NewObject(ctx);
        set(ctx, r, c"panHandlers", handlers);
        r
    }
}

/// The centroid of `e.touches`.
fn centroid(ctx: *mut JsContext, e: JsValue) -> (f64, f64, usize) {
    let list = get(ctx, e, c"touches");
    let mut len: i64 = 0;
    let (mut x, mut y) = (0.0, 0.0);
    // SAFETY: reads of live values, freed after.
    unsafe {
        if qjs::JS_IsArray(list) && qjs::JS_GetLength(ctx, list, &mut len) == 0 {
            for i in 0..len.max(0) {
                let p = qjs::JS_GetPropertyInt64(ctx, list, i);
                x += get_number(ctx, p, c"pageX");
                y += get_number(ctx, p, c"pageY");
                qjs::JS_FreeValue(ctx, p);
            }
        }
        qjs::JS_FreeValue(ctx, list);
    }
    let n = len.max(1) as f64;
    (x / n, y / n, len.max(0) as usize)
}

fn remember(ctx: *mut JsContext, last: JsValue, x: f64, y: f64, time: f64, count: usize) {
    set(ctx, last, c"x", number(x));
    set(ctx, last, c"y", number(y));
    set(ctx, last, c"t", number(time));
    set(ctx, last, c"n", number(count as f64));
}

/// A grant or a first finger: the gesture starts here.
fn reset(ctx: *mut JsContext, gs: JsValue, last: JsValue, e: JsValue) {
    let (x, y, n) = centroid(ctx, e);
    for (k, v) in [
        (c"moveX", x),
        (c"moveY", y),
        (c"x0", x),
        (c"y0", y),
        (c"dx", 0.0),
        (c"dy", 0.0),
        (c"vx", 0.0),
        (c"vy", 0.0),
    ] {
        set(ctx, gs, k, number(v));
    }
    set(ctx, gs, c"numberActiveTouches", number(n as f64));
    remember(ctx, last, x, y, get_number(ctx, e, c"timestamp"), n);
}

/// Moves the gesture to `e`'s touches (a finger going down or up makes
/// the centroid jump, which is no movement).
fn update(ctx: *mut JsContext, gs: JsValue, last: JsValue, e: JsValue) {
    let time = get_number(ctx, e, c"timestamp");
    let (lx, ly, lt, ln) = (
        get_number(ctx, last, c"x"),
        get_number(ctx, last, c"y"),
        get_number(ctx, last, c"t"),
        get_number(ctx, last, c"n") as usize,
    );
    let (x, y, n) = centroid(ctx, e);
    if time == lt && n == ln {
        return;
    }
    set(ctx, gs, c"numberActiveTouches", number(n as f64));
    if n == ln {
        let dt = (time - lt).max(1.0);
        set(ctx, gs, c"dx", number(get_number(ctx, gs, c"dx") + x - lx));
        set(ctx, gs, c"dy", number(get_number(ctx, gs, c"dy") + y - ly));
        // Smoothed over a few samples.
        set(
            ctx,
            gs,
            c"vx",
            number(get_number(ctx, gs, c"vx") * 0.3 + (x - lx) / dt * 0.7),
        );
        set(
            ctx,
            gs,
            c"vy",
            number(get_number(ctx, gs, c"vy") * 0.3 + (y - ly) / dt * 0.7),
        );
    }
    set(ctx, gs, c"moveX", number(x));
    set(ctx, gs, c"moveY", number(y));
    remember(ctx, last, x, y, time, n);
}

/// Calls the config's `name(e, gs)`; `None` when it has none.
fn callback(
    ctx: *mut JsContext,
    config: JsValue,
    name: &CStr,
    e: JsValue,
    gs: JsValue,
) -> Option<JsValue> {
    let f = get(ctx, config, name);
    // SAFETY: an inline check; the call's arguments are borrowed.
    unsafe {
        if !qjs::JS_IsFunction(ctx, f) {
            qjs::JS_FreeValue(ctx, f);
            return None;
        }
        let mut args = [e, gs];
        let r = qjs::JS_Call(ctx, f, UNDEFINED, 2, args.as_mut_ptr());
        qjs::JS_FreeValue(ctx, f);
        Some(r)
    }
}

/// The config's answer, or `default` without a callback.
fn answer(r: Option<JsValue>, default: bool) -> JsValue {
    match r {
        Some(v) => v,
        None => {
            if default {
                qjs::JS_TRUE
            } else {
                qjs::JS_FALSE
            }
        }
    }
}

fn next_state(ctx: *mut JsContext, gs: JsValue) {
    let t = touches(ctx);
    let id = t.pan_states.get() + 1;
    t.pan_states.set(id);
    set(ctx, gs, c"stateID", number(id as f64));
}

unsafe extern "C" fn pan_handler(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values (`pan_create`).
    let (config, gs, last) = unsafe { (*data, *data.add(1), *data.add(2)) };
    let e = arg(argc, argv, 0);
    let ignore = |r: Option<JsValue>| match r {
        Some(v) => v,
        None => UNDEFINED,
    };
    match magic {
        // onStartShouldSetResponderCapture
        0 => {
            let (_, _, n) = centroid(ctx, e);
            if n == 1 {
                reset(ctx, gs, last, e);
            }
            set(ctx, gs, c"numberActiveTouches", number(n as f64));
            let r = callback(ctx, config, c"onStartShouldSetPanResponderCapture", e, gs);
            answer(r, false)
        }
        1 => answer(
            callback(ctx, config, c"onStartShouldSetPanResponder", e, gs),
            false,
        ),
        2 => {
            update(ctx, gs, last, e);
            answer(
                callback(ctx, config, c"onMoveShouldSetPanResponderCapture", e, gs),
                false,
            )
        }
        3 => {
            update(ctx, gs, last, e);
            answer(
                callback(ctx, config, c"onMoveShouldSetPanResponder", e, gs),
                false,
            )
        }
        // onResponderGrant: the gesture starts at the grant.
        4 => {
            reset(ctx, gs, last, e);
            if let Some(r) = callback(ctx, config, c"onPanResponderGrant", e, gs) {
                // SAFETY: an inline tag check.
                if unsafe { qjs::JS_IsException(r) } {
                    return r;
                }
                free(ctx, r);
            }
            answer(
                callback(ctx, config, c"onShouldBlockNativeResponder", e, gs),
                true,
            )
        }
        5 => ignore(callback(ctx, config, c"onPanResponderReject", e, gs)),
        6..=8 => {
            update(ctx, gs, last, e);
            let name = match magic {
                6 => c"onPanResponderStart",
                7 => c"onPanResponderMove",
                _ => c"onPanResponderEnd",
            };
            ignore(callback(ctx, config, name, e, gs))
        }
        9 | 10 => {
            let name = if magic == 9 {
                c"onPanResponderRelease"
            } else {
                c"onPanResponderTerminate"
            };
            let r = ignore(callback(ctx, config, name, e, gs));
            next_state(ctx, gs);
            r
        }
        _ => answer(
            callback(ctx, config, c"onPanResponderTerminationRequest", e, gs),
            true,
        ),
    }
}

fn pan_responder(ctx: *mut JsContext) -> JsValue {
    // SAFETY: a new object taking the function.
    let o = unsafe { qjs::JS_NewObject(ctx) };
    set(
        ctx,
        o,
        c"create",
        reactive::new_function(ctx, c"create", pan_create, 1),
    );
    o
}

/// `nsh/gesture`.
pub(crate) const GESTURE_EXPORTS: &[Item] = &[(
    "PanResponder",
    Export::Value(pan_responder),
    Ts::value(
        "Pan gestures on top of the responder system (React Native's\n`PanResponder`).",
        "{ create(config: PanResponderCallbacks): PanResponderInstance }",
    ),
)];

/// The touch events' types (part of every element's props: `types.d.ts`).
pub(crate) const TOUCH_TYPES: &str = r#"/** A finger of a touch event (points). */
export interface TouchPoint {
  /** Identifies the finger while it is down. */
  identifier: number;
  /** Position in the view the finger went down on (`target`). */
  locationX: number;
  locationY: number;
  /** Position in the surface. */
  pageX: number;
  pageY: number;
  /** Id of the view the finger went down on. */
  target: number;
  /** Milliseconds. */
  timestamp: number;
}

/** A touch or responder event: the finger it is about, the fingers down
 * and those that changed (React Native's `nativeEvent`, which it also is). */
export interface GestureResponderEvent extends TouchPoint {
  touches: TouchPoint[];
  changedTouches: TouchPoint[];
  nativeEvent: GestureResponderEvent;
  /** Touch handlers (`onTouchStart`…): views further out do not see it. */
  stopPropagation(): void;
}

/** A gesture followed through touch events: the fingers' centroid, how far
 * it moved and how fast (React Native's `PanResponderGestureState`). The
 * `…ShouldSetResponder` handlers get it since the first finger went down. */
export interface ResponderGestureState {
  /** Latest position of the touches' centroid (surface points). */
  moveX: number;
  moveY: number;
  /** Where the centroid was when the gesture began. */
  x0: number;
  y0: number;
  /** Distance moved since then. */
  dx: number;
  dy: number;
  /** Velocity (points per millisecond). */
  vx: number;
  vy: number;
  numberActiveTouches: number;
}

/** When a view's pan activates (`panActivation`; react-native-gesture-handler's
 * `activeOffsetX` / `failOffsetY`), decided natively on the main thread as the
 * finger moves: past an active offset the view becomes the responder and the
 * scroll views around it can no longer take the touches; past a fail offset
 * first it never does, and they keep them. Distances in points along one axis,
 * either direction; without active offsets it activates after 10 points. */
export interface PanActivation {
  activeOffsetX?: number;
  activeOffsetY?: number;
  failOffsetX?: number;
  failOffsetY?: number;
}
"#;

/// `nsh/gesture`'s types.
pub(crate) const GESTURE_TYPES: &str = r#"/** The state of a pan gesture (React Native's `PanResponderGestureState`):
 * since the grant once the view is the responder. */
export interface PanResponderGestureState extends ResponderGestureState {
  stateID: number;
}

/** A `PanResponder.create` callback. */
export type PanHandler<R = void> = (e: GestureResponderEvent, gestureState: PanResponderGestureState) => R;

/** `PanResponder.create` callbacks. */
export interface PanResponderCallbacks {
  onStartShouldSetPanResponder?: PanHandler<boolean>;
  onStartShouldSetPanResponderCapture?: PanHandler<boolean>;
  onMoveShouldSetPanResponder?: PanHandler<boolean>;
  onMoveShouldSetPanResponderCapture?: PanHandler<boolean>;
  onPanResponderGrant?: PanHandler;
  onPanResponderReject?: PanHandler;
  onPanResponderStart?: PanHandler;
  onPanResponderMove?: PanHandler;
  onPanResponderEnd?: PanHandler;
  onPanResponderRelease?: PanHandler;
  onPanResponderTerminate?: PanHandler;
  onPanResponderTerminationRequest?: PanHandler<boolean>;
  /** Keep native scroll views from taking the touches (default true). */
  onShouldBlockNativeResponder?: PanHandler<boolean>;
}

/** What `PanResponder.create` returns: spread `panHandlers` on a view. */
export interface PanResponderInstance {
  panHandlers: TouchProps;
}
"#;

#[cfg(test)]
mod tests {
    use nsh_core::{
        EventKind, EventPayload, Op, TouchEvent, TouchPhase, TouchPoint,
        geometry::{point, px},
    };

    use crate::Host;

    fn touch(phase: TouchPhase, x: f32, y: f32, t: f64) -> EventPayload {
        EventPayload::Touch(Box::new(TouchEvent {
            phase,
            timestamp: t,
            activated: None,
            points: vec![TouchPoint {
                id: 1,
                target: 4,
                path: vec![4, 2],
                page: point(px(x), px(y)),
                location: point(px(x), px(y)),
                changed: true,
            }],
        }))
    }

    #[test]
    fn touches_capture_bubble_and_a_pan_responder_tracks_the_gesture() {
        let host = Host::new().unwrap();
        host.eval_source(
            r#"
            import { element, append, spread } from "nsh:rt";
            import { PanResponder } from "nsh/gesture";
            const log = [];
            globalThis.log = log;
            const outer = element("view");
            const inner = element("view");
            append(outer, inner);
            spread(outer, {
                onTouchStartCapture: () => log.push("outer capture"),
                onTouchStart: () => log.push("outer bubble"),
            });
            const pan = PanResponder.create({
                onStartShouldSetPanResponder: () => true,
                onPanResponderGrant: () => log.push("grant"),
                onPanResponderMove: (e, g) => log.push(`move ${g.dx},${g.dy}`),
                onPanResponderRelease: (e, g) => log.push(`release ${g.dx}`),
            });
            spread(inner, { ...pan.panHandlers, onTouchStart: () => log.push("inner bubble") });
            globalThis.keep = outer;
            "#,
        )
        .unwrap();
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let listens: Vec<_> = ops
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    Op::Listen {
                        event: EventKind::Touch,
                        enabled: true,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(listens.len(), 2, "both views listen: {ops:?}");
        host.dispatch_event(
            4,
            EventKind::Touch,
            &touch(TouchPhase::Start, 10.0, 10.0, 0.0),
        );
        host.dispatch_event(
            4,
            EventKind::Touch,
            &touch(TouchPhase::Move, 30.0, 15.0, 16.0),
        );
        host.dispatch_event(
            4,
            EventKind::Touch,
            &touch(TouchPhase::End, 30.0, 15.0, 32.0),
        );
        host.eval_source(r#"import { text } from "nsh:rt"; text(globalThis.log.join("|"));"#)
            .unwrap();
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let log = ops
            .iter()
            .find_map(|o| match o {
                Op::CreateText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            log,
            "outer capture|inner bubble|outer bubble|grant|move 20,5|release 20"
        );
        // The grant blocks native scroll views (the default), the release
        // lets go.
        assert!(ops.iter().any(|o| matches!(
            o,
            Op::Command {
                id: 4,
                command: nsh_core::Command::SetJsResponder { block: true }
            }
        )));
        assert!(ops.iter().any(|o| matches!(
            o,
            Op::Command {
                id: 1,
                command: nsh_core::Command::ClearJsResponder
            }
        )));
    }
}

//! Navigation: `createNavigator`, `Stack`, `Screen` and `Modal` (the
//! mobile UI exports them: `Registry::native_values`).
//!
//! A stack renders each route of its navigator with the app's function into
//! a native screen (UINavigationController on iOS, a stack with a toolbar
//! on Android); screens stay mounted while their route is in the stack. A
//! screen the user pops natively removes its route. Screens and modals
//! provide their insets to what they render (`useSafeAreaInsets`).

use std::{
    ffi::{CStr, CString, c_int},
    ptr,
};

use nsh_core::{EventKind, Tag};
use rquickjs::qjs;

use crate::{
    dom,
    modules::{Export, Item, Ts},
    reactive::{self, arg},
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

const UNDEFINED: JsValue = qjs::JS_UNDEFINED;

type DataFn = unsafe extern "C" fn(
    *mut JsContext,
    JsValue,
    c_int,
    *mut JsValue,
    c_int,
    *mut JsValue,
) -> JsValue;

fn native(ctx: *mut JsContext, f: DataFn, length: c_int, data: &mut [JsValue]) -> JsValue {
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

fn get(ctx: *mut JsContext, obj: JsValue, key: &CStr) -> JsValue {
    // SAFETY: a property read of a live value.
    unsafe {
        if !qjs::JS_IsObject(obj) {
            return UNDEFINED;
        }
        qjs::JS_GetPropertyStr(ctx, obj, key.as_ptr())
    }
}

fn free(ctx: *mut JsContext, v: JsValue) {
    // SAFETY: an owned value.
    unsafe { qjs::JS_FreeValue(ctx, v) };
}

/// The mobile UI's navigation (its `Registry::native_values`).
pub(crate) const EXPORTS: &[Item] = &[
    (
        "createNavigator",
        Export::Function(create_navigator, 1),
        Ts::function(
            "A stack of routes for `<Stack>`, starting with `initial`.",
            &["<R>(initial: R | R[]): Navigator<R>"],
        ),
    ),
    (
        "Stack",
        Export::Function(stack, 1),
        Ts::function(
            "A native navigation stack: UINavigationController on iOS (swipe back,\nnative header), a stack with a toolbar and predictive back on Android.\nScreens stay mounted (with their state) while they are in the stack.",
            &["<R>(props: StackProps<R>): YNode"],
        ),
    ),
    (
        "Screen",
        Export::Function(screen, 1),
        Ts::function(
            "One screen of a `<Stack>`, with its header options.",
            &["(props: ScreenProps): YNode"],
        ),
    ),
    (
        "Modal",
        Export::Function(modal, 1),
        Ts::function(
            "Content presented over everything else, in its own native window layer.",
            &["(props: ModalProps): JSX.Element"],
        ),
    ),
];

/// The types of the navigation's exports.
pub(crate) const TYPES: &str = r#"/** A stack of routes for `<Stack>`: plain values, compared by identity. */
export interface Navigator<R> {
  /** The routes, bottom to top (reactive). */
  routes: Accessor<readonly R[]>;
  /** Pushes a screen. */
  push(route: R): void;
  /** Pops `count` screens (at least one stays). */
  pop(count?: number): void;
  /** Pops back to the first screen. */
  popToTop(): void;
  /** Replaces the top screen. */
  replace(route: R): void;
  /** Replaces all screens. */
  reset(routes: R[]): void;
  /** Removes a route wherever it is (the user popped its screen). */
  remove(route: R): void;
}

export interface StackProps<R> {
  navigator: Navigator<R>;
  style?: StyleProp<ViewStyle>;
  /** Renders a route; return a `<Screen>` for header options. */
  children: (route: R, index: Accessor<number>) => JSX.Element;
}

export interface ScreenProps extends ScreenHostProps {
  headerLeft?: JSX.Element;
  headerRight?: JSX.Element;
  /** Replaces the title. */
  headerTitle?: JSX.Element;
  style?: StyleProp<ViewStyle>;
  children?: JSX.Element;
  ios?: IOSScreenHostProps;
  desktop?: DesktopScreenHostProps;
}

export interface ModalProps extends ModalHostProps {
  /** Shown while true (default). */
  visible?: boolean;
  style?: StyleProp<ViewStyle>;
  children?: JSX.Element;
  ios?: IOSModalHostProps;
  android?: AndroidModalHostProps;
  desktop?: DesktopModalHostProps;
  web?: WebModalHostProps;
}
"#;

// --- The navigator --------------------------------------------------------------

/// The routes (a JS array) as values (owned).
fn routes_of(ctx: *mut JsContext, routes: JsValue) -> Vec<JsValue> {
    let mut len: i64 = 0;
    let mut out = Vec::new();
    // SAFETY: reads of a live array.
    unsafe {
        if !qjs::JS_IsArray(routes) || qjs::JS_GetLength(ctx, routes, &mut len) != 0 {
            return out;
        }
        for i in 0..len.max(0) {
            out.push(qjs::JS_GetPropertyInt64(ctx, routes, i));
        }
    }
    out
}

/// `createNavigator(initial)`: the route stack of a `<Stack>`.
unsafe extern "C" fn create_navigator(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let initial = arg(argc, argv, 0);
    // SAFETY: a new array of the initial route(s).
    let routes = unsafe {
        if qjs::JS_IsArray(initial) {
            let mut values = routes_of(ctx, initial);
            qjs::JS_NewArrayFrom(ctx, values.len() as c_int, values.as_mut_ptr())
        } else {
            let mut values = [qjs::JS_DupValue(ctx, initial)];
            qjs::JS_NewArrayFrom(ctx, 1, values.as_mut_ptr())
        }
    };
    let signal = reactive::new_signal(ctx, routes);
    free(ctx, routes);
    // SAFETY: a new object taking the functions.
    let nav = unsafe { qjs::JS_NewObject(ctx) };
    let methods: [(&CStr, c_int, c_int); 6] = [
        (c"push", 1, 0),
        (c"pop", 1, 1),
        (c"popToTop", 0, 2),
        (c"replace", 1, 3),
        (c"reset", 1, 4),
        (c"remove", 1, 5),
    ];
    for (name, length, magic) in methods {
        let mut data = [signal];
        // SAFETY: the function keeps a dup of the signal; the object takes it.
        unsafe {
            let f =
                qjs::JS_NewCFunctionData(ctx, Some(navigate), length, magic, 1, data.as_mut_ptr());
            qjs::JS_SetPropertyStr(ctx, nav, name.as_ptr(), f);
        }
    }
    // `routes`: a read-only accessor.
    let read = native(ctx, read_routes, 0, &mut [signal]);
    let routes = reactive::memo(ctx, read, false);
    // SAFETY: the object takes the accessor; the signal is held by the
    // functions.
    unsafe {
        qjs::JS_SetPropertyStr(ctx, nav, c"routes".as_ptr(), routes);
        qjs::JS_FreeValue(ctx, signal);
    }
    nav
}

unsafe extern "C" fn read_routes(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value: the routes signal.
    unsafe { qjs::JS_Call(ctx, *data, UNDEFINED, 0, ptr::null_mut()) }
}

/// The navigator's functions, by magic: push, pop, popToTop, replace,
/// reset, remove.
unsafe extern "C" fn navigate(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value (the signal); values got are freed or moved.
    let signal = unsafe { *data };
    let current = reactive::peek(ctx, signal);
    let mut routes = routes_of(ctx, current);
    let a0 = arg(argc, argv, 0);
    let dup = |v: JsValue| unsafe { qjs::JS_DupValue(ctx, v) };
    match magic {
        0 => routes.push(dup(a0)),
        1 => {
            let mut count = 1f64;
            // SAFETY: converting a live value.
            if !unsafe { qjs::JS_IsUndefined(a0) } {
                unsafe { qjs::JS_ToFloat64(ctx, &mut count, a0) };
            }
            let keep = routes.len().saturating_sub(count.max(0.0) as usize).max(1);
            for v in routes.drain(keep.min(routes.len())..) {
                free(ctx, v);
            }
        }
        2 => {
            for v in routes.drain(1.min(routes.len())..) {
                free(ctx, v);
            }
        }
        3 => {
            if let Some(v) = routes.pop() {
                free(ctx, v);
            }
            routes.push(dup(a0));
        }
        4 => {
            for v in routes.drain(..) {
                free(ctx, v);
            }
            routes = routes_of(ctx, a0);
        }
        _ => {
            let (gone, kept): (Vec<_>, Vec<_>) = routes
                .into_iter()
                // SAFETY: identity comparisons of live values.
                .partition(|&v| unsafe { qjs::JS_IsStrictEqual(ctx, v, a0) });
            for v in gone {
                free(ctx, v);
            }
            routes = kept;
        }
    }
    // SAFETY: a new array taking the routes.
    let next = unsafe { qjs::JS_NewArrayFrom(ctx, routes.len() as c_int, routes.as_mut_ptr()) };
    reactive::set_signal(ctx, signal, next);
    reactive::finish_value(ctx, UNDEFINED)
}

// --- Stack ------------------------------------------------------------------------

/// `<Stack navigator>{(route, index) => <Screen …>}</Stack>`.
unsafe extern "C" fn stack(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let props = arg(argc, argv, 0);
    let node = match dom::create_element(ctx, Tag::Stack) {
        Ok(node) => node,
        Err(e) => return e,
    };
    let result = (|| -> Result<(), JsValue> {
        dom::spread_except(ctx, node, props, &["navigator", "children"])?;
        let navigator = get(ctx, props, c"navigator");
        let routes = get(ctx, navigator, c"routes");
        free(ctx, navigator);
        let render = native(ctx, stack_row, 2, &mut [props]);
        // SAFETY: `each(routes, render)`: rows by route identity.
        let mut args = [routes, render];
        let rows = unsafe { crate::dom::each(ctx, UNDEFINED, 2, args.as_mut_ptr()) };
        free(ctx, routes);
        free(ctx, render);
        // SAFETY: an inline tag check.
        if unsafe { qjs::JS_IsException(rows) } {
            return Err(rows);
        }
        let r = dom::insert_value_at(ctx, node, rows, None);
        free(ctx, rows);
        r
    })();
    match result {
        Ok(()) => reactive::finish_value(ctx, node),
        Err(e) => {
            free(ctx, node);
            e
        }
    }
}

/// One route of a stack: its screen (the app's `<Screen>`, or one around
/// what it rendered); a native pop removes the route.
unsafe extern "C" fn stack_row(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value (the stack's props); `item` is an accessor.
    let props = unsafe { *data };
    let (item, index) = (arg(argc, argv, 0), arg(argc, argv, 1));
    let route = reactive::untracked_with(ctx, || unsafe {
        qjs::JS_Call(ctx, item, UNDEFINED, 0, ptr::null_mut())
    });
    let children = get(ctx, props, c"children");
    let mut args = [route, index];
    // SAFETY: the app's function, called untracked.
    let rendered = reactive::untracked_with(ctx, || unsafe {
        qjs::JS_Call(ctx, children, UNDEFINED, 2, args.as_mut_ptr())
    });
    free(ctx, children);
    // SAFETY: an inline tag check.
    if unsafe { qjs::JS_IsException(rendered) } {
        free(ctx, route);
        return rendered;
    }
    let navigator = get(ctx, props, c"navigator");
    let remove = get(ctx, navigator, c"remove");
    free(ctx, navigator);
    let popped = native(ctx, pop_route, 0, &mut [remove, route]);
    free(ctx, remove);
    free(ctx, route);
    // An accessor (e.g. a component made hot) may render another screen
    // later.
    let result =
        if unsafe { qjs::JS_IsFunction(ctx, rendered) } && dom::node_id(ctx, rendered).is_none() {
            let wrap = native(ctx, screen_of_accessor, 0, &mut [rendered, popped]);
            reactive::memo(ctx, wrap, false)
        } else {
            screen_for(ctx, rendered, popped)
        };
    free(ctx, rendered);
    free(ctx, popped);
    result
}

unsafe extern "C" fn pop_route(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values: the navigator's `remove` and the route.
    unsafe {
        let mut args = [*data.add(1)];
        qjs::JS_Call(ctx, *data, UNDEFINED, 1, args.as_mut_ptr())
    }
}

unsafe extern "C" fn screen_of_accessor(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values: the accessor and the pop callback.
    let (accessor, popped) = unsafe { (*data, *data.add(1)) };
    let rendered = unsafe { qjs::JS_Call(ctx, accessor, UNDEFINED, 0, ptr::null_mut()) };
    if unsafe { qjs::JS_IsException(rendered) } {
        return rendered;
    }
    let r = reactive::untracked_with(ctx, || screen_for(ctx, rendered, popped));
    free(ctx, rendered);
    r
}

/// The screen for what a route rendered (borrowed): itself when it is one,
/// else a screen around it; told about native pops (`popped`).
fn screen_for(ctx: *mut JsContext, rendered: JsValue, popped: JsValue) -> JsValue {
    let node = if dom::node_tag(ctx, rendered) == Some(Tag::Screen) {
        // SAFETY: a dup returned.
        unsafe { qjs::JS_DupValue(ctx, rendered) }
    } else {
        // SAFETY: new props for a screen around the content.
        let props = unsafe { qjs::JS_NewObject(ctx) };
        unsafe {
            qjs::JS_SetPropertyStr(
                ctx,
                props,
                c"children".as_ptr(),
                qjs::JS_DupValue(ctx, rendered),
            );
        }
        let mut args = [props];
        let node = unsafe { screen(ctx, UNDEFINED, 1, args.as_mut_ptr()) };
        free(ctx, props);
        node
    };
    // SAFETY: an inline tag check.
    if !unsafe { qjs::JS_IsException(node) } {
        dom::set_popped(ctx, node, popped);
    }
    node
}

// --- Screen -------------------------------------------------------------------------

/// The insets a screen or modal reports, with `initial` until it does:
/// `[insets accessor, onInsets handler]` (owned).
fn native_insets(ctx: *mut JsContext, top_zero: bool) -> (JsValue, JsValue) {
    let reported = reactive::new_signal(ctx, qjs::JS_NULL);
    let read = native(
        ctx,
        read_insets,
        0,
        &mut [
            reported,
            if top_zero {
                qjs::JS_TRUE
            } else {
                qjs::JS_FALSE
            },
        ],
    );
    let insets = reactive::memo(ctx, read, false);
    let on = native(ctx, on_insets, 1, &mut [reported]);
    free(ctx, reported);
    (insets, on)
}

unsafe extern "C" fn read_insets(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values: the reported insets signal and whether the
    // top is the header's (0 until reported).
    unsafe {
        let (reported, top_zero) = (*data, qjs::JS_ToBool(ctx, *data.add(1)) == 1);
        let v = qjs::JS_Call(ctx, reported, UNDEFINED, 0, ptr::null_mut());
        if qjs::JS_IsException(v) || !qjs::JS_IsNull(v) {
            return v;
        }
        let safe_area = crate::env::reexport(ctx, "safeArea");
        let area = qjs::JS_Call(ctx, safe_area, UNDEFINED, 0, ptr::null_mut());
        qjs::JS_FreeValue(ctx, safe_area);
        if !top_zero || qjs::JS_IsException(area) {
            return area;
        }
        let copy = qjs::JS_NewObject(ctx);
        for key in [c"right", c"bottom", c"left"] {
            qjs::JS_SetPropertyStr(ctx, copy, key.as_ptr(), get(ctx, area, key));
        }
        qjs::JS_SetPropertyStr(ctx, copy, c"top".as_ptr(), qjs::JS_NewFloat64(0.0));
        qjs::JS_FreeValue(ctx, area);
        copy
    }
}

unsafe extern "C" fn on_insets(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value (the signal); a copy of the event's insets.
    unsafe {
        let e = arg(argc, argv, 0);
        let copy = qjs::JS_NewObject(ctx);
        for key in [c"top", c"right", c"bottom", c"left"] {
            qjs::JS_SetPropertyStr(ctx, copy, key.as_ptr(), get(ctx, e, key));
        }
        reactive::set_signal(ctx, *data, copy);
    }
    reactive::finish_value(ctx, UNDEFINED)
}

/// What a provider of the insets context renders: `children` (a reader
/// of the app's props) with `insets`.
fn with_insets(ctx: *mut JsContext, insets: JsValue, children: JsValue) -> JsValue {
    let context = crate::env::insets_context(ctx);
    let provider = get(ctx, context, c"Provider");
    // SAFETY: provider props: the value, and the children as a getter.
    unsafe {
        let props = qjs::JS_NewObject(ctx);
        qjs::JS_SetPropertyStr(ctx, props, c"value".as_ptr(), qjs::JS_DupValue(ctx, insets));
        let atom = qjs::JS_NewAtom(ctx, c"children".as_ptr());
        qjs::JS_DefineProperty(
            ctx,
            props,
            atom,
            UNDEFINED,
            children,
            UNDEFINED,
            (qjs::JS_PROP_HAS_GET | qjs::JS_PROP_HAS_ENUMERABLE | qjs::JS_PROP_ENUMERABLE) as c_int,
        );
        qjs::JS_FreeAtom(ctx, atom);
        let mut args = [props];
        let r = qjs::JS_Call(ctx, provider, UNDEFINED, 1, args.as_mut_ptr());
        qjs::JS_FreeValue(ctx, props);
        qjs::JS_FreeValue(ctx, provider);
        r
    }
}

/// A getter of `props[key]`.
fn reader(ctx: *mut JsContext, props: JsValue, key: &str) -> JsValue {
    // SAFETY: a new string the function keeps a dup of.
    let k = unsafe { qjs::JS_NewStringLen(ctx, key.as_ptr().cast(), key.len() as _) };
    let f = native(ctx, read_prop, 0, &mut [props, k]);
    free(ctx, k);
    f
}

unsafe extern "C" fn read_prop(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values (props, key).
    unsafe {
        let Some(key) = dom::string_arg(ctx, *data.add(1)) else {
            return qjs::JS_EXCEPTION;
        };
        let k = CString::new(key).unwrap_or_default();
        get(ctx, *data, &k)
    }
}

/// `<Screen title … headerLeft headerRight headerTitle>`: one screen of a
/// stack, with its header options.
unsafe extern "C" fn screen(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let props = arg(argc, argv, 0);
    let node = match dom::create_element(ctx, Tag::Screen) {
        Ok(node) => node,
        Err(e) => return e,
    };
    let result = (|| -> Result<(), JsValue> {
        dom::spread_except(
            ctx,
            node,
            props,
            &[
                "children",
                "headerLeft",
                "headerRight",
                "headerTitle",
                "onDismiss",
                "onInsets",
            ],
        )?;
        let (insets, on_insets) = native_insets(ctx, true);
        let id = dom::node_id(ctx, node).expect("a node");
        let r = dom::set_handler(ctx, id, EventKind::Insets, on_insets);
        free(ctx, on_insets);
        r?;
        let dismiss = native(ctx, on_dismiss, 0, &mut [node, props]);
        let r = dom::set_handler(ctx, id, EventKind::Dismiss, dismiss);
        free(ctx, dismiss);
        r?;
        // The header's slots, then the content with the screen's insets.
        let mut parts = Vec::new();
        for (slot, key) in [
            ("left", "headerLeft"),
            ("right", "headerRight"),
            ("title", "headerTitle"),
        ] {
            let content = reader(ctx, props, key);
            // SAFETY: a new string the function keeps a dup of.
            let name = unsafe { qjs::JS_NewStringLen(ctx, slot.as_ptr().cast(), slot.len() as _) };
            let item = native(ctx, header_item, 0, &mut [content, name]);
            free(ctx, content);
            free(ctx, name);
            parts.push(reactive::memo(ctx, item, false));
        }
        let children = reader(ctx, props, "children");
        let content = with_insets(ctx, insets, children);
        free(ctx, children);
        free(ctx, insets);
        // SAFETY: an inline tag check.
        if unsafe { qjs::JS_IsException(content) } {
            for p in parts {
                free(ctx, p);
            }
            return Err(content);
        }
        parts.push(content);
        let all = unsafe { qjs::JS_NewArrayFrom(ctx, parts.len() as c_int, parts.as_mut_ptr()) };
        let r = dom::insert_value_at(ctx, node, all, None);
        free(ctx, all);
        r
    })();
    match result {
        Ok(()) => reactive::finish_value(ctx, node),
        Err(e) => {
            free(ctx, node);
            e
        }
    }
}

/// A header slot: an item holding the app's content, while it has some.
unsafe extern "C" fn header_item(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values: the content's reader and the slot's name.
    unsafe {
        let (content, name) = (*data, *data.add(1));
        let c = qjs::JS_Call(ctx, content, UNDEFINED, 0, ptr::null_mut());
        if qjs::JS_IsException(c) {
            return c;
        }
        if qjs::JS_IsNull(c)
            || qjs::JS_IsUndefined(c)
            || (qjs::JS_IsBool(c) && !qjs::JS_VALUE_GET_BOOL(c))
        {
            qjs::JS_FreeValue(ctx, c);
            return qjs::JS_NULL;
        }
        reactive::untracked_with(ctx, || {
            let item = match dom::create_element(ctx, Tag::HeaderItem) {
                Ok(item) => item,
                Err(e) => {
                    qjs::JS_FreeValue(ctx, c);
                    return e;
                }
            };
            let r = dom::set_prop_value(ctx, item, "headerSlot", name)
                .and_then(|()| dom::insert_value_at(ctx, item, c, None));
            qjs::JS_FreeValue(ctx, c);
            match r {
                Ok(()) => item,
                Err(e) => {
                    qjs::JS_FreeValue(ctx, item);
                    e
                }
            }
        })
    }
}

/// A screen's dismissal: a native pop tells the stack, then the app.
unsafe extern "C" fn on_dismiss(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values: the node and the props; values got are
    // freed.
    unsafe {
        let (node, props) = (*data, *data.add(1));
        let popped = dom::popped(ctx, node);
        if qjs::JS_IsFunction(ctx, popped) {
            let r = qjs::JS_Call(ctx, popped, UNDEFINED, 0, ptr::null_mut());
            qjs::JS_FreeValue(ctx, popped);
            if qjs::JS_IsException(r) {
                return r;
            }
            qjs::JS_FreeValue(ctx, r);
        } else {
            qjs::JS_FreeValue(ctx, popped);
        }
        let own = get(ctx, props, c"onDismiss");
        let r = if qjs::JS_IsFunction(ctx, own) {
            qjs::JS_Call(ctx, own, UNDEFINED, 0, ptr::null_mut())
        } else {
            UNDEFINED
        };
        qjs::JS_FreeValue(ctx, own);
        r
    }
}

// --- Modal --------------------------------------------------------------------------

/// `<Modal visible onRequestClose …>`: content over everything else, in
/// its own native window layer, while `visible` is not false.
unsafe extern "C" fn modal(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let props = arg(argc, argv, 0);
    let run = native(ctx, modal_run, 0, &mut [props]);
    let shown = reactive::memo(ctx, run, false);
    reactive::finish_value(ctx, shown)
}

unsafe extern "C" fn modal_run(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value (the props).
    let props = unsafe { *data };
    let visible = get(ctx, props, c"visible");
    // SAFETY: an inline check of a live value.
    let hidden = unsafe { qjs::JS_IsBool(visible) && !qjs::JS_VALUE_GET_BOOL(visible) };
    free(ctx, visible);
    if hidden {
        return qjs::JS_NULL;
    }
    reactive::untracked_with(ctx, || {
        let node = match dom::create_element(ctx, Tag::Modal) {
            Ok(node) => node,
            Err(e) => return e,
        };
        let result = (|| -> Result<(), JsValue> {
            dom::spread_except(ctx, node, props, &["children", "visible", "onInsets"])?;
            let (insets, on_insets) = native_insets(ctx, false);
            let id = dom::node_id(ctx, node).expect("a node");
            let r = dom::set_handler(ctx, id, EventKind::Insets, on_insets);
            free(ctx, on_insets);
            r?;
            let children = reader(ctx, props, "children");
            let content = with_insets(ctx, insets, children);
            free(ctx, children);
            free(ctx, insets);
            // SAFETY: an inline tag check.
            if unsafe { qjs::JS_IsException(content) } {
                return Err(content);
            }
            let r = dom::insert_value_at(ctx, node, content, None);
            free(ctx, content);
            r
        })();
        match result {
            Ok(()) => node,
            Err(e) => {
                free(ctx, node);
                e
            }
        }
    })
}

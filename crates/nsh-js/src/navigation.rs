//! Navigation and dialogs: `createNavStack`, `NavStack`, `NavPage` and
//! `Dialog` (`nsh/base` exports them: `Registry::native_values`).
//!
//! A stack renders each route of its state with the app's function into a
//! native page (UINavigationController on iOS, a stack with a toolbar on
//! Android); pages stay mounted while their route is in the stack. A page
//! the user pops natively removes its route. Pages and dialogs provide
//! their insets to what they render (`safeAreaInsets`).
//!
//! NSH draws no look of its own: the desktops and the web show a page's
//! `header` slot at its top (the core places it) and nothing else; a
//! dialog renders the app's popup and, when styled, its backdrop, inside a
//! native layer that has no look either.
//!
//! The handlers of a page's and a dialog's native events are the
//! runtime's: they keep the stack in step and call the app's props
//! (`onDismiss`, `onBackPress`, `onOpen`, `onClose`, `onCancel`,
//! `onOpenChange`), so the app's names are not the core's events'.

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

/// `nsh/base`'s navigation and dialogs (its `Registry::native_values`).
pub(crate) const EXPORTS: &[Item] = &[
    (
        "createNavStack",
        Export::Function(create_nav_stack, 1),
        Ts::function(
            "The state of a `<NavStack>`: its routes, starting with `routes`.",
            &["<R>(routes: R[]): NavStackState<R>"],
        ),
    ),
    (
        "NavStack",
        Export::Function(stack, 1),
        Ts::function(
            "A native navigation stack: UINavigationController on iOS (swipe back,\nnative header), a stack with a toolbar and predictive back on Android.\nPages stay mounted (with their state) while they are in the stack.",
            &["<R>(props: NavStackProps<R>): YNode"],
        ),
    ),
    (
        "NavPage",
        Export::Function(screen, 1),
        Ts::function(
            "One page of a `<NavStack>`: its `header` (desktops, the web) and the system\nheader's options (iOS, Android).",
            &["(props: NavPageProps): YNode"],
        ),
    ),
    (
        "Dialog",
        Export::Function(modal, 1),
        Ts::function(
            "A modal surface over everything else, in its own native window layer\n(`presentation` picks its form on iOS). It owns no look: `class` and `style`\nstyle the popup, `backdropClass` and `backdropStyle` a backdrop behind it.",
            &["(props: DialogProps): JSX.Element"],
        ),
    ),
];

/// The types of the navigation's exports.
pub(crate) const TYPES: &str = r#"/** The routes of a `<NavStack>`: the app's values; each push is a page of its own, the same route pushed twice included. */
export interface NavStackState<R> {
  /** The routes, bottom to top (reactive). */
  routes: Accessor<readonly R[]>;
  /** Pushes a page. */
  push(route: R): void;
  /** Pops `count` pages (default 1; at least one stays). */
  pop(count?: number): void;
  /** Pops back to the first page. */
  popToRoot(): void;
  /** Replaces the top page. */
  replace(route: R): void;
  /** Replaces the pages with `routes`; a route that stays (the same value)
   * keeps its page and its state. */
  setRoutes(routes: R[]): void;
  /** Removes the topmost page of `route`, wherever it is. */
  remove(route: R): void;
}

export interface NavStackProps<R> {
  state: NavStackState<R>;
  /** Finds the stack in tests and `nsh test --trace`. */
  testID?: string;
  style?: StyleProp<ViewStyle>;
  class?: string;
  /** Renders a route; return a `<NavPage>` for header options. */
  children: (route: R, index: Accessor<number>) => JSX.Element;
}

/** The color of the status bar's content. */
export type StatusBarForeground = "auto" | "light" | "dark";

/** What the status bar shows while a page or dialog is up (`statusBar`; iOS
 * and Android). The fields left out keep what is below: the pages under it
 * and the app's default (`nsh.json` `statusBar`). */
export interface StatusBar {
  /** The content's color: "light" for dark backgrounds, "dark" for light ones, "auto" the platform's for the appearance. */
  foreground?: StatusBarForeground;
  /** Hidden. */
  hidden?: boolean;
  /** Animate the change when the page or dialog comes and goes. */
  animated?: boolean;
}

/** The colors of a page's system header (iOS and Android). */
export interface HeaderStyle {
  backgroundColor?: ColorValue;
  /** The title's. */
  color?: ColorValue;
  /** The back button's and the bar buttons'. */
  tintColor?: ColorValue;
}

export interface NavPageProps extends NavPageHostProps {
  /** Finds the page in tests and `nsh test --trace`. */
  testID?: string;
  /** The app's header on the desktops and the web, which have no system
   * header: placed at the top of the page, as wide as the page and as tall
   * as it lays out, with the content below it. NSH draws nothing else
   * there (no title, no back button); iOS and Android show their system
   * header instead. */
  header?: JSX.Element;
  /** iOS and Android: the system header's leading item. */
  headerLeft?: JSX.Element;
  /** iOS and Android: the system header's trailing item. */
  headerRight?: JSX.Element;
  /** iOS and Android: replaces the system header's title. */
  headerTitle?: JSX.Element;
  /** iOS and Android: the system header's colors. */
  headerStyle?: HeaderStyle;
  /** iOS and Android: the system header's colors as classes (`bg-*`,
   * `text-*` for the title, `tint-*` for the buttons); `headerStyle` wins
   * over them. */
  headerClass?: string;
  /** The user popped the page (back button, swipe); its route is gone. */
  onDismiss?: () => void;
  /** The user pressed back on the page (the header's back button, Android
   * back, Escape / Alt+Left / the mouse's back button on the desktops, the
   * browser's back), whether or not `popOnBack` let it pop. */
  onBackPress?: () => void;
  style?: StyleProp<ViewStyle>;
  class?: string;
  children?: JSX.Element;
}

/** What closed a dialog: Escape or the back button (`"cancel"`), a press
 * on its backdrop part, a sheet swiped down. */
export type DialogChangeReason = "cancel" | "backdropPress" | "swipe";

export interface DialogProps extends DialogHostProps {
  /** Finds the dialog in tests and `nsh test --trace`. */
  testID?: string;
  /** Shown while true (default). */
  open?: boolean;
  /** How it comes in and goes (default `"slide"`). */
  animation?: "slide" | "fade" | "none";
  /** It finished appearing. */
  onOpen?: () => void;
  /** It went away: the user closed it, or the app removed it. */
  onClose?: () => void;
  /** The user tried to close it (Escape, back, a backdrop press, a swipe
   * down), whether or not a `closeOn*` flag let it close. */
  onCancel?: () => void;
  /** The user closed it (the `closeOn*` flag of what they did let it
   * close): it closes whatever `open` says, so keep `open` in sync (set it
   * to false here); while `open` stays true it stays closed, and it opens
   * again when `open` goes from false to true. */
  onOpenChange?: (open: boolean, reason: DialogChangeReason) => void;
  /** The popup's style: the view holding the children. The dialog itself
   * has no look: on the desktops and the web it is a transparent layer over
   * the window (`presentation` is ignored there). */
  style?: StyleProp<ViewStyle>;
  /** The popup's classes. */
  class?: string;
  /** The backdrop's style: with this or `backdropClass`, a view filling the
   * dialog behind the popup (none otherwise), whose presses are close
   * attempts (`"backdropPress"`, closing when `closeOnBackdropPress`). */
  backdropStyle?: StyleProp<ViewStyle>;
  /** The backdrop's classes (see `backdropStyle`). */
  backdropClass?: string;
  /** Finds the backdrop in tests and `nsh test --trace`. */
  backdropTestID?: string;
  children?: JSX.Element;
  ios?: IOSDialogHostProps;
  android?: AndroidDialogHostProps;
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

/// `createNavStack(routes)`: the route stack of a `<NavStack>`.
unsafe extern "C" fn create_nav_stack(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let initial = arg(argc, argv, 0);
    // SAFETY: an inline tag check.
    if !unsafe { qjs::JS_IsArray(initial) } {
        // SAFETY: throwing in the calling context.
        return unsafe {
            qjs::JS_ThrowTypeError(ctx, c"createNavStack: an array of routes".as_ptr())
        };
    }
    let mut entries: Vec<JsValue> = routes_of(ctx, initial)
        .into_iter()
        .map(|route| entry(ctx, route))
        .collect();
    // SAFETY: a new array taking the entries.
    let entries =
        unsafe { qjs::JS_NewArrayFrom(ctx, entries.len() as c_int, entries.as_mut_ptr()) };
    let signal = reactive::new_signal(ctx, entries);
    free(ctx, entries);
    // SAFETY: a new object taking the functions.
    let nav = unsafe { qjs::JS_NewObject(ctx) };
    let methods: [(&CStr, c_int, c_int); 6] = [
        (c"push", 1, 0),
        (c"pop", 1, 1),
        (c"popToRoot", 0, 2),
        (c"replace", 1, 3),
        (c"setRoutes", 1, 4),
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
    // SAFETY: the object takes the accessor and the signal, the latter
    // hidden (not enumerable, not writable): `<NavStack>` renders its entries.
    unsafe {
        qjs::JS_SetPropertyStr(ctx, nav, c"routes".as_ptr(), routes);
        qjs::JS_DefinePropertyValueStr(ctx, nav, ENTRIES.as_ptr(), signal, 0);
    }
    nav
}

/// The hidden property of a navigator holding its entries' signal.
const ENTRIES: &CStr = c"__nshEntries";

/// One screen of a navigator: an object of its own around the route, so a
/// route pushed twice is two screens, and the user popping one removes
/// that one (takes `route`).
fn entry(ctx: *mut JsContext, route: JsValue) -> JsValue {
    // SAFETY: a new object taking the route.
    unsafe {
        let entry = qjs::JS_NewObject(ctx);
        qjs::JS_SetPropertyStr(ctx, entry, c"route".as_ptr(), route);
        entry
    }
}

/// The route of an entry (owned).
fn route_of(ctx: *mut JsContext, entry: JsValue) -> JsValue {
    get(ctx, entry, c"route")
}

unsafe extern "C" fn read_routes(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value: the entries' signal.
    let entries = unsafe { qjs::JS_Call(ctx, *data, UNDEFINED, 0, ptr::null_mut()) };
    let mut routes: Vec<JsValue> = routes_of(ctx, entries)
        .into_iter()
        .map(|e| {
            let route = route_of(ctx, e);
            free(ctx, e);
            route
        })
        .collect();
    free(ctx, entries);
    // SAFETY: a new array taking the routes.
    unsafe { qjs::JS_NewArrayFrom(ctx, routes.len() as c_int, routes.as_mut_ptr()) }
}

/// The stack state's functions, by magic: push, pop, popToRoot, replace,
/// setRoutes, remove; 6: remove one entry (the user popped its page).
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
        0 => routes.push(entry(ctx, dup(a0))),
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
            routes.push(entry(ctx, dup(a0)));
        }
        4 => {
            // A route that stays keeps its page (its entry), in order: the
            // first unused entry of the same route.
            let mut old: Vec<Option<JsValue>> = routes.drain(..).map(Some).collect();
            routes = routes_of(ctx, a0)
                .into_iter()
                .map(|route| {
                    let kept = old.iter_mut().find(|slot| {
                        slot.is_some_and(|e| {
                            let r = route_of(ctx, e);
                            // SAFETY: an identity comparison of live values.
                            let same = unsafe { qjs::JS_IsStrictEqual(ctx, r, route) };
                            free(ctx, r);
                            same
                        })
                    });
                    match kept.and_then(Option::take) {
                        Some(e) => {
                            free(ctx, route);
                            e
                        }
                        None => entry(ctx, route),
                    }
                })
                .collect();
            for e in old.into_iter().flatten() {
                free(ctx, e);
            }
        }
        5 => {
            // The topmost screen showing the route.
            let at = routes.iter().rposition(|&e| {
                let route = route_of(ctx, e);
                // SAFETY: an identity comparison of live values.
                let same = unsafe { qjs::JS_IsStrictEqual(ctx, route, a0) };
                free(ctx, route);
                same
            });
            if let Some(at) = at {
                free(ctx, routes.remove(at));
            }
        }
        _ => {
            // SAFETY: identity comparisons of live values.
            if let Some(at) = routes
                .iter()
                .position(|&e| unsafe { qjs::JS_IsStrictEqual(ctx, e, a0) })
            {
                free(ctx, routes.remove(at));
            }
        }
    }
    // SAFETY: a new array taking the routes.
    let next = unsafe { qjs::JS_NewArrayFrom(ctx, routes.len() as c_int, routes.as_mut_ptr()) };
    reactive::set_signal(ctx, signal, next);
    reactive::finish_value(ctx, UNDEFINED)
}

// --- Stack ------------------------------------------------------------------------

/// `<NavStack state>{(route, index) => <NavPage …>}</NavStack>`.
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
        dom::spread_except(ctx, node, props, &["state", "children"])?;
        let navigator = get(ctx, props, c"state");
        let entries = get(ctx, navigator, ENTRIES);
        free(ctx, navigator);
        let render = native(ctx, stack_row, 2, &mut [props]);
        // SAFETY: `each(entries, render)`: a row per pushed screen.
        let mut args = [entries, render];
        let rows = unsafe { crate::dom::each(ctx, UNDEFINED, 2, args.as_mut_ptr()) };
        free(ctx, entries);
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

/// One route of a stack: its screen (the app's `<NavPage>`, or one around
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
    let entry = reactive::untracked_with(ctx, || unsafe {
        qjs::JS_Call(ctx, item, UNDEFINED, 0, ptr::null_mut())
    });
    let route = route_of(ctx, entry);
    let children = get(ctx, props, c"children");
    let mut args = [route, index];
    // SAFETY: the app's function, called untracked.
    let rendered = reactive::untracked_with(ctx, || unsafe {
        qjs::JS_Call(ctx, children, UNDEFINED, 2, args.as_mut_ptr())
    });
    free(ctx, children);
    // SAFETY: an inline tag check.
    free(ctx, route);
    if unsafe { qjs::JS_IsException(rendered) } {
        free(ctx, entry);
        return rendered;
    }
    let navigator = get(ctx, props, c"state");
    let entries = get(ctx, navigator, ENTRIES);
    free(ctx, navigator);
    let popped = native(ctx, pop_route, 0, &mut [entries, entry]);
    free(ctx, entries);
    free(ctx, entry);
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
    // SAFETY: two data values: the navigator's entries and the entry.
    let mut args = [unsafe { *data.add(1) }];
    // SAFETY: `navigate` removing that entry (magic 6) from the signal.
    unsafe { navigate(ctx, UNDEFINED, 1, args.as_mut_ptr(), 6, data) }
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

/// `<NavPage title … header headerLeft headerRight headerTitle>`: one page
/// of a stack, with its header options.
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
                "header",
                "headerLeft",
                "headerRight",
                "headerTitle",
                "onDismiss",
                "onBackPress",
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
        // Only when the app listens: the event mask tells the platform
        // whether a back press goes to JS at all.
        let listens = get(ctx, props, c"onBackPress");
        // SAFETY: an inline check of a live value.
        let listens_fn = unsafe { !qjs::JS_IsUndefined(listens) && !qjs::JS_IsNull(listens) };
        free(ctx, listens);
        if listens_fn {
            forward(ctx, id, EventKind::BackRequest, props, "onBackPress")?;
        }
        // The header's slots, then the content with the screen's insets.
        let mut parts = Vec::new();
        for (slot, key) in [
            ("left", "headerLeft"),
            ("right", "headerRight"),
            ("title", "headerTitle"),
            ("header", "header"),
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

/// A page's dismissal: a native pop tells the stack, then the app
/// (`onDismiss`).
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

// --- Handlers of the app's props -------------------------------------------------

/// Sets the handler of `event` on node `id` to one calling `props[key]`
/// (read when the event comes) with the event's arguments.
fn forward(
    ctx: *mut JsContext,
    id: nsh_core::NodeId,
    event: EventKind,
    props: JsValue,
    key: &str,
) -> Result<(), JsValue> {
    // SAFETY: a new string the function keeps a dup of.
    let k = unsafe { qjs::JS_NewStringLen(ctx, key.as_ptr().cast(), key.len() as _) };
    let f = native(ctx, call_prop, 1, &mut [props, k]);
    free(ctx, k);
    let r = dom::set_handler(ctx, id, event, f);
    free(ctx, f);
    r
}

unsafe extern "C" fn call_prop(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: two data values (props, key); the arguments are borrowed.
    unsafe {
        let Some(key) = dom::string_arg(ctx, *data.add(1)) else {
            return qjs::JS_EXCEPTION;
        };
        let k = CString::new(key).unwrap_or_default();
        let f = get(ctx, *data, &k);
        let r = if qjs::JS_IsFunction(ctx, f) {
            qjs::JS_Call(ctx, f, UNDEFINED, argc, argv)
        } else {
            UNDEFINED
        };
        free(ctx, f);
        r
    }
}

// --- Dialog -------------------------------------------------------------------------

/// `<Dialog open onOpenChange …>`: content over everything else, in its
/// own native window layer, while it is open: `open` is not false and the
/// user did not close it ([`close_attempt`]).
///
/// What shows is the app's: the popup (a view with the dialog's `class`
/// and `style`, holding the children) and, only when the app styles one
/// (`backdropClass` / `backdropStyle`), a backdrop behind it filling the
/// layer, whose presses are close attempts (`"backdropPress"`).
unsafe extern "C" fn modal(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let props = arg(argc, argv, 0);
    // Closed by the user while `open` stayed true: `closed` (a plain
    // object's flag, which the run resets without writing a signal it
    // reads) and `tick`, which runs it again.
    // SAFETY: a new object.
    let state = unsafe { qjs::JS_NewObject(ctx) };
    let tick = reactive::new_signal(ctx, qjs::JS_NewFloat64(0.0));
    let run = native(ctx, modal_run, 0, &mut [props, state, tick]);
    free(ctx, state);
    free(ctx, tick);
    let shown = reactive::memo(ctx, run, false);
    reactive::finish_value(ctx, shown)
}

/// The `closed` flag of a dialog's state object.
fn closed_by_user(ctx: *mut JsContext, state: JsValue) -> bool {
    let v = get(ctx, state, c"closed");
    // SAFETY: an inline check of a live value.
    let closed = unsafe { qjs::JS_IsBool(v) && qjs::JS_VALUE_GET_BOOL(v) };
    free(ctx, v);
    closed
}

fn set_closed_by_user(ctx: *mut JsContext, state: JsValue, closed: bool) {
    // SAFETY: a property write of a live object.
    unsafe {
        qjs::JS_SetPropertyStr(
            ctx,
            state,
            c"closed".as_ptr(),
            if closed { qjs::JS_TRUE } else { qjs::JS_FALSE },
        );
    }
}

unsafe extern "C" fn modal_run(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values (props, state, tick).
    let (props, state, tick) = unsafe { (*data, *data.add(1), *data.add(2)) };
    // Tracked: a close by the user runs this again.
    let t = unsafe { qjs::JS_Call(ctx, tick, UNDEFINED, 0, ptr::null_mut()) };
    free(ctx, t);
    let open = get(ctx, props, c"open");
    // SAFETY: an inline check of a live value.
    let closed = unsafe { qjs::JS_IsBool(open) && !qjs::JS_VALUE_GET_BOOL(open) };
    free(ctx, open);
    if closed {
        // The app followed (or closed it itself): the next `open` shows it.
        set_closed_by_user(ctx, state, false);
        return qjs::JS_NULL;
    }
    if closed_by_user(ctx, state) {
        return qjs::JS_NULL;
    }
    reactive::untracked_with(ctx, || {
        let node = match dom::create_element(ctx, Tag::Modal) {
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
                    "open",
                    "class",
                    "style",
                    "backdropClass",
                    "backdropStyle",
                    "backdropTestID",
                    "onInsets",
                    "onOpen",
                    "onClose",
                    "onCancel",
                    "onOpenChange",
                ],
            )?;
            let (insets, on_insets) = native_insets(ctx, false);
            let id = dom::node_id(ctx, node).expect("a node");
            let r = dom::set_handler(ctx, id, EventKind::Insets, on_insets);
            free(ctx, on_insets);
            r?;
            forward(ctx, id, EventKind::Show, props, "onOpen")?;
            forward(ctx, id, EventKind::Dismiss, props, "onClose")?;
            let cancel = native(ctx, on_cancel, 1, &mut [props, state, tick]);
            let r = dom::set_handler(ctx, id, EventKind::RequestClose, cancel);
            free(ctx, cancel);
            r?;
            // The backdrop part, while the app styles one.
            let has = native(ctx, has_backdrop, 0, &mut [props]);
            let make = native(ctx, backdrop_part, 0, &mut [props, state, tick]);
            let backdrop = dom::cond_value(ctx, has, make);
            free(ctx, has);
            free(ctx, make);
            // SAFETY: an inline tag check.
            if unsafe { qjs::JS_IsException(backdrop) } {
                free(ctx, insets);
                return Err(backdrop);
            }
            let popup = popup_part(ctx, props, insets);
            free(ctx, insets);
            let popup = match popup {
                Ok(popup) => popup,
                Err(e) => {
                    free(ctx, backdrop);
                    return Err(e);
                }
            };
            let mut parts = [backdrop, popup];
            // SAFETY: a new array taking the parts.
            let all = unsafe { qjs::JS_NewArrayFrom(ctx, 2, parts.as_mut_ptr()) };
            let r = dom::insert_value_at(ctx, node, all, None);
            free(ctx, all);
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

/// The popup: a view with the dialog's `class` and `style` holding its
/// children (with the dialog's `insets`, borrowed); owned.
fn popup_part(ctx: *mut JsContext, props: JsValue, insets: JsValue) -> Result<JsValue, JsValue> {
    let popup = dom::create_element(ctx, Tag::View)?;
    let result = (|| -> Result<(), JsValue> {
        for (prop, key) in [("class", "class"), ("style", "style")] {
            let read = reader(ctx, props, key);
            let r = dom::bind_prop_with(ctx, popup, prop, read);
            free(ctx, read);
            r?;
        }
        let children = reader(ctx, props, "children");
        let content = with_insets(ctx, insets, children);
        free(ctx, children);
        // SAFETY: an inline tag check.
        if unsafe { qjs::JS_IsException(content) } {
            return Err(content);
        }
        let r = dom::insert_value_at(ctx, popup, content, None);
        free(ctx, content);
        r
    })();
    match result {
        Ok(()) => Ok(popup),
        Err(e) => {
            free(ctx, popup);
            Err(e)
        }
    }
}

/// Whether the app styles a backdrop (`backdropClass` or `backdropStyle`).
unsafe extern "C" fn has_backdrop(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value (the props).
    let props = unsafe { *data };
    let mut has = false;
    for key in [c"backdropClass", c"backdropStyle"] {
        let v = get(ctx, props, key);
        // SAFETY: inline checks of a live value.
        has |= unsafe { !qjs::JS_IsUndefined(v) && !qjs::JS_IsNull(v) };
        free(ctx, v);
    }
    if has { qjs::JS_TRUE } else { qjs::JS_FALSE }
}

/// The backdrop: a view filling the dialog behind the popup, with the
/// app's `backdropClass` and `backdropStyle` and no look of its own; a
/// press on it is a close attempt (`"backdropPress"`).
unsafe extern "C" fn backdrop_part(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values (props, state, tick).
    let props = unsafe { *data };
    let view = match dom::create_element(ctx, Tag::View) {
        Ok(view) => view,
        Err(e) => return e,
    };
    let result = (|| -> Result<(), JsValue> {
        let class = native(ctx, backdrop_class, 0, &mut [props]);
        let r = dom::bind_prop_with(ctx, view, "class", class);
        free(ctx, class);
        r?;
        let style = reader(ctx, props, "backdropStyle");
        let r = dom::bind_prop_with(ctx, view, "style", style);
        free(ctx, style);
        r?;
        let test_id = reader(ctx, props, "backdropTestID");
        let r = dom::bind_prop_with(ctx, view, "testID", test_id);
        free(ctx, test_id);
        r?;
        // SAFETY: the three data values, which the function keeps dups of.
        let press = native(ctx, backdrop_press, 0, unsafe {
            std::slice::from_raw_parts_mut(data, 3)
        });
        let r = dom::set_prop_value(ctx, view, "onPress", press);
        free(ctx, press);
        r
    })();
    match result {
        Ok(()) => view,
        Err(e) => {
            free(ctx, view);
            e
        }
    }
}

/// The backdrop's classes: filling the dialog, then the app's.
unsafe extern "C" fn backdrop_class(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value (the props).
    let v = get(ctx, unsafe { *data }, c"backdropClass");
    // SAFETY: an inline tag check.
    let own = if unsafe { qjs::JS_IsString(v) } {
        dom::string_arg(ctx, v).unwrap_or_default()
    } else {
        String::new()
    };
    free(ctx, v);
    let class = format!("absolute inset-0 {own}");
    // SAFETY: a new string.
    unsafe { qjs::JS_NewStringLen(ctx, class.as_ptr().cast(), class.len() as _) }
}

unsafe extern "C" fn backdrop_press(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values (props, state, tick).
    let (props, state, tick) = unsafe { (*data, *data.add(1), *data.add(2)) };
    close_attempt(ctx, props, state, tick, "backdropPress")
}

/// The platform reports that the user tried to close a dialog natively
/// (payload: `reason`).
unsafe extern "C" fn on_cancel(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: three data values (props, state, tick); the event is
    // borrowed.
    let (props, state, tick) = unsafe { (*data, *data.add(1), *data.add(2)) };
    let e = arg(argc, argv, 0);
    let value = get(ctx, e, c"reason");
    // SAFETY: an inline tag check.
    let reason = if unsafe { qjs::JS_IsString(value) } {
        dom::string_arg(ctx, value).unwrap_or_default()
    } else {
        String::new()
    };
    free(ctx, value);
    let reason = match reason.as_str() {
        "backdropPress" => "backdropPress",
        "swipe" => "swipe",
        _ => "cancel",
    };
    close_attempt(ctx, props, state, tick, reason)
}

/// The user tried to close a dialog: `onCancel`, then, when the
/// `closeOn*` flag of what they did lets it close, `onOpenChange(false,
/// reason)` and the dialog closes (a platform that closed it natively
/// already reports `onClose`; otherwise removing it does).
fn close_attempt(
    ctx: *mut JsContext,
    props: JsValue,
    state: JsValue,
    tick: JsValue,
    reason: &str,
) -> JsValue {
    let flag: &CStr = match reason {
        "backdropPress" => c"closeOnBackdropPress",
        "swipe" => c"closeOnSwipe",
        _ => c"closeOnEscape",
    };
    let call = |key: &CStr, args: &mut [JsValue]| -> JsValue {
        let f = get(ctx, props, key);
        // SAFETY: a call of the app's function with borrowed arguments.
        let r = unsafe {
            if qjs::JS_IsFunction(ctx, f) {
                qjs::JS_Call(ctx, f, UNDEFINED, args.len() as c_int, args.as_mut_ptr())
            } else {
                UNDEFINED
            }
        };
        free(ctx, f);
        r
    };
    let r = call(c"onCancel", &mut []);
    // SAFETY: an inline tag check.
    if unsafe { qjs::JS_IsException(r) } {
        return r;
    }
    free(ctx, r);
    let closes = get(ctx, props, flag);
    // SAFETY: an inline check of a live value.
    let kept = unsafe { qjs::JS_IsBool(closes) && !qjs::JS_VALUE_GET_BOOL(closes) };
    free(ctx, closes);
    if kept || closed_by_user(ctx, state) {
        return UNDEFINED;
    }
    reactive::batch_with(ctx, || {
        // SAFETY: a new string, freed after the call.
        let reason =
            unsafe { qjs::JS_NewStringLen(ctx, reason.as_ptr().cast(), reason.len() as _) };
        let r = call(c"onOpenChange", &mut [qjs::JS_FALSE, reason]);
        free(ctx, reason);
        // SAFETY: an inline tag check.
        if unsafe { qjs::JS_IsException(r) } {
            return r;
        }
        free(ctx, r);
        // Closed whether or not the app keeps `open` in sync.
        set_closed_by_user(ctx, state, true);
        let n = reactive::peek_number(ctx, tick).unwrap_or(0.0);
        reactive::set_signal(ctx, tick, qjs::JS_NewFloat64(n + 1.0));
        reactive::finish_value(ctx, UNDEFINED)
    })
}

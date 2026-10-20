//! `nsh/desktop`'s hand-written exports (with the registry's `Window`):
//! `isAvailable`, `titleBar()` (the environment's accessor) and `Menu`,
//! whose items call the app's functions: the menus are decoded natively
//! into [`MenuItem`]s with an id per item, the functions kept by id, and
//! the platform's [`EventKind::Menu`] calls the chosen item's.

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, c_int},
    ptr,
    sync::Arc,
};

use nsh_core::{
    Command, EventKind, Op, ROOT_ID,
    desktop::{DesktopCommand, MenuAction, MenuItem, MenuRole, Shortcut},
    geometry::{Points, point},
    platform::Os,
};
use rquickjs::qjs;

use crate::{
    dom,
    modules::{Export, Item, Ts},
    reactive::{self, arg},
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

const UNDEFINED: JsValue = qjs::JS_UNDEFINED;

/// Menu items' functions: (id, owned function).
type Functions = Vec<(u32, JsValue)>;

/// The menus' part of a context's state.
pub(crate) struct Menus {
    /// The application menu's functions (id, owned function).
    app: RefCell<Vec<(u32, JsValue)>>,
    /// The last context menu's functions.
    popup: RefCell<Vec<(u32, JsValue)>>,
    /// Resolves the promise of the context menu shown (owned), once the
    /// platform reports it closed.
    closed: Cell<JsValue>,
    /// The root's `onMenu` handler is set.
    listening: Cell<bool>,
    next: Cell<u32>,
}

impl Default for Menus {
    fn default() -> Self {
        Menus {
            app: RefCell::default(),
            popup: RefCell::default(),
            closed: Cell::new(UNDEFINED),
            listening: Cell::new(false),
            next: Cell::new(0),
        }
    }
}

/// Resolves the promise of the context menu shown, if there is one.
fn resolve_closed(ctx: *mut JsContext) {
    let resolve = menus(ctx).closed.replace(UNDEFINED);
    if !nullish(resolve) {
        // SAFETY: an owned resolving function, called then freed.
        unsafe {
            let r = qjs::JS_Call(ctx, resolve, UNDEFINED, 0, ptr::null_mut());
            qjs::JS_FreeValue(ctx, r);
            qjs::JS_FreeValue(ctx, resolve);
        }
    }
}

impl Menus {
    /// Frees the values held (the state is unregistered).
    pub(crate) fn free_all(&mut self, raw: *mut JsContext) {
        // SAFETY: owned; no JS runs from here on.
        unsafe { qjs::JS_FreeValue(raw, self.closed.replace(UNDEFINED)) };
        for (_, f) in self
            .app
            .get_mut()
            .drain(..)
            .chain(self.popup.get_mut().drain(..))
        {
            // SAFETY: owned values; no JS runs from here on.
            unsafe { qjs::JS_FreeValue(raw, f) };
        }
    }
}

fn menus<'a>(ctx: *mut JsContext) -> &'a Menus {
    // SAFETY: a projection of the state's field, by shared reference.
    unsafe { &(*crate::state::state(ctx)).menus }
}

fn os(ctx: *mut JsContext) -> Os {
    // SAFETY: the state's renderer, by shared reference.
    unsafe { &(*crate::state::state(ctx)).dom }.os()
}

fn throw(ctx: *mut JsContext, message: &str) -> JsValue {
    let message = std::ffi::CString::new(message).unwrap_or_default();
    // SAFETY: throws on a live context; `%s` keeps the message literal.
    unsafe { qjs::JS_ThrowTypeError(ctx, c"%s".as_ptr(), message.as_ptr()) }
}

/// `undefined` or `null`.
fn nullish(v: JsValue) -> bool {
    // SAFETY: inline tag checks.
    unsafe { qjs::JS_IsUndefined(v) || qjs::JS_IsNull(v) }
}

fn is_object(v: JsValue) -> bool {
    // SAFETY: an inline tag check.
    unsafe { qjs::JS_IsObject(v) }
}

/// Property `key` of `object` (owned).
fn get(ctx: *mut JsContext, object: JsValue, key: &CStr) -> JsValue {
    // SAFETY: a read of a live object.
    unsafe { qjs::JS_GetPropertyStr(ctx, object, key.as_ptr()) }
}

fn free(ctx: *mut JsContext, v: JsValue) {
    // SAFETY: an owned value.
    unsafe { qjs::JS_FreeValue(ctx, v) };
}

/// String property `key`, if it is set.
fn string(ctx: *mut JsContext, object: JsValue, key: &CStr) -> Option<String> {
    let v = get(ctx, object, key);
    let s = if nullish(v) {
        None
    } else {
        dom::string_arg(ctx, v)
    };
    free(ctx, v);
    s
}

/// The role named `name`, as `MenuRole`'s conversion reads it.
fn menu_role(ctx: *mut JsContext, name: &str) -> Option<MenuRole> {
    let host = crate::classes::host_outside_calls(ctx);
    let value = nsh_core::ui::JsHost::new_string(&host, name).ok()?;
    let value = nsh_core::ui::JsOwned::new(&host, value);
    let role = <MenuRole as nsh_core::ui::JsValue>::from_js(value.get()).ok();
    host.clear_pending();
    role
}

/// Boolean property `key`, if it is set.
fn flag(ctx: *mut JsContext, object: JsValue, key: &CStr) -> Option<bool> {
    let v = get(ctx, object, key);
    let b = if nullish(v) {
        None
    } else {
        // SAFETY: `ToBoolean` of a live value.
        Some(unsafe { qjs::JS_ToBool(ctx, v) } == 1)
    };
    free(ctx, v);
    b
}

/// Decodes menus, keeping their functions.
struct Decoder {
    ctx: *mut JsContext,
    mac: bool,
    /// The items' functions (id, owned function).
    functions: Vec<(u32, JsValue)>,
}

impl Decoder {
    /// The items of array `list`.
    fn items(&mut self, list: JsValue) -> Result<Vec<MenuItem>, String> {
        let ctx = self.ctx;
        // SAFETY: an inline tag check.
        if !unsafe { qjs::JS_IsArray(list) } {
            return Err("a menu is an array of items".into());
        }
        let mut len: i64 = 0;
        // SAFETY: the length of a live array.
        unsafe { qjs::JS_GetLength(ctx, list, &mut len) };
        let mut out = Vec::with_capacity(len.max(0) as usize);
        for i in 0..len.max(0) {
            // SAFETY: an element of a live array (owned).
            let v = unsafe { qjs::JS_GetPropertyInt64(ctx, list, i) };
            let item = self.item(v);
            free(ctx, v);
            if let Some(item) = item? {
                out.push(item);
            }
        }
        Ok(out)
    }

    /// One item; `None` for items that are nothing (`null`, `false`: a
    /// condition left them out).
    fn item(&mut self, v: JsValue) -> Result<Option<MenuItem>, String> {
        let ctx = self.ctx;
        if !is_object(v) {
            // SAFETY: `ToBoolean` of a live value.
            return if unsafe { qjs::JS_ToBool(ctx, v) } == 0 {
                Ok(None)
            } else {
                Err("a menu item is an object".into())
            };
        }
        if string(ctx, v, c"type").as_deref() == Some("separator") {
            return Ok(Some(MenuItem::Separator));
        }
        let label = string(ctx, v, c"label");
        let enabled = flag(ctx, v, c"enabled").unwrap_or(true);
        let submenu = get(ctx, v, c"submenu");
        if !nullish(submenu) {
            let items = self.items(submenu);
            free(ctx, submenu);
            return Ok(Some(MenuItem::Submenu {
                label: label.ok_or("a submenu needs a label")?,
                enabled,
                items: items?,
            }));
        }
        let role = match string(ctx, v, c"role") {
            Some(name) => {
                Some(menu_role(ctx, &name).ok_or_else(|| format!("unknown menu role `{name}`"))?)
            }
            None => None,
        };
        if label.is_none() && role.is_none() {
            return Err("a menu item needs a label or a role".into());
        }
        let shortcut = match string(ctx, v, c"shortcut") {
            Some(text) => Some(Shortcut::parse(&text, self.mac).map_err(|e| e.to_string())?),
            None => role.and_then(|r| r.shortcut(self.mac)),
        };
        let on_select = get(ctx, v, c"onSelect");
        // SAFETY: an inline check.
        let id = if unsafe { qjs::JS_IsFunction(ctx, on_select) } {
            let m = menus(ctx);
            let id = m.next.get() + 1;
            m.next.set(id);
            self.functions.push((id, on_select));
            id
        } else {
            free(ctx, on_select);
            0
        };
        Ok(Some(MenuItem::Action(MenuAction {
            id,
            label: label.unwrap_or_default(),
            shortcut,
            enabled,
            checked: flag(ctx, v, c"checked"),
            role,
        })))
    }

    fn free(self) {
        for (_, f) in self.functions {
            free(self.ctx, f);
        }
    }
}

/// Decodes `list` into items and their functions; throws on a bad menu.
fn decode(ctx: *mut JsContext, list: JsValue) -> Result<(Arc<[MenuItem]>, Functions), JsValue> {
    let mut decoder = Decoder {
        ctx,
        mac: os(ctx) == Os::MacOs,
        functions: Vec::new(),
    };
    match decoder.items(list) {
        Ok(items) => Ok((items.into(), std::mem::take(&mut decoder.functions))),
        Err(message) => {
            decoder.free();
            Err(throw(ctx, &format!("nsh/desktop: {message}")))
        }
    }
}

/// Keeps `functions` in `slot`, freeing the ones they replace.
fn keep(ctx: *mut JsContext, slot: &RefCell<Vec<(u32, JsValue)>>, functions: Vec<(u32, JsValue)>) {
    let old = slot.replace(functions);
    for (_, f) in old {
        free(ctx, f);
    }
}

fn send(ctx: *mut JsContext, command: DesktopCommand) -> Result<(), JsValue> {
    dom::push_op(
        ctx,
        Op::Command {
            id: ROOT_ID,
            command: Command::Desktop(Box::new(command)),
        },
    )
}

/// Sets the root's `onMenu` handler, once.
fn listen(ctx: *mut JsContext) -> Result<(), JsValue> {
    let m = menus(ctx);
    if m.listening.get() {
        return Ok(());
    }
    let on = reactive::new_function(ctx, c"onMenu", on_menu, 1);
    let r = dom::set_handler(ctx, ROOT_ID, EventKind::Menu, on);
    free(ctx, on);
    r?;
    m.listening.set(true);
    Ok(())
}

/// `Menu.setApplicationMenu(items | null)`.
unsafe extern "C" fn set_application_menu(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    if !os(ctx).is_desktop() {
        return UNDEFINED;
    }
    let list = arg(argc, argv, 0);
    let result = if nullish(list) {
        keep(ctx, &menus(ctx).app, Vec::new());
        send(ctx, DesktopCommand::SetMenu(None))
    } else {
        match decode(ctx, list) {
            Ok((items, functions)) => {
                keep(ctx, &menus(ctx).app, functions);
                listen(ctx).and_then(|()| send(ctx, DesktopCommand::SetMenu(Some(items))))
            }
            Err(e) => Err(e),
        }
    };
    result.err().unwrap_or(UNDEFINED)
}

/// `Menu.popup(items, at?)`.
unsafe extern "C" fn popup(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let mut funcs = [UNDEFINED; 2];
    // SAFETY: a new promise and its resolving functions (owned).
    let promise = unsafe { qjs::JS_NewPromiseCapability(ctx, funcs.as_mut_ptr()) };
    free(ctx, funcs[1]);
    if !os(ctx).is_desktop() {
        // Nothing shows: settled at once.
        // SAFETY: an owned resolving function, called then freed.
        unsafe {
            let r = qjs::JS_Call(ctx, funcs[0], UNDEFINED, 0, ptr::null_mut());
            qjs::JS_FreeValue(ctx, r);
        }
        free(ctx, funcs[0]);
        return promise;
    }
    let (items, functions) = match decode(ctx, arg(argc, argv, 0)) {
        Ok(decoded) => decoded,
        Err(e) => {
            free(ctx, funcs[0]);
            free(ctx, promise);
            return e;
        }
    };
    keep(ctx, &menus(ctx).popup, functions);
    // One context menu at a time: one shown before is closed by now.
    resolve_closed(ctx);
    menus(ctx).closed.set(funcs[0]);
    let at = arg(argc, argv, 1);
    let at = if is_object(at) {
        let number = |key: &CStr| {
            let v = get(ctx, at, key);
            let mut n = 0f64;
            // SAFETY: `ToNumber` of a live value.
            unsafe { qjs::JS_ToFloat64(ctx, &mut n, v) };
            free(ctx, v);
            n as f32
        };
        Some(point(Points(number(c"x")), Points(number(c"y"))))
    } else {
        None
    };
    match listen(ctx).and_then(|()| send(ctx, DesktopCommand::PopupMenu { items, at })) {
        Ok(()) => promise,
        Err(e) => {
            free(ctx, promise);
            e
        }
    }
}

/// The root's `onMenu`: calls the chosen item's function.
unsafe extern "C" fn on_menu(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let e = arg(argc, argv, 0);
    let v = get(ctx, e, c"id");
    let mut id = 0f64;
    // SAFETY: `ToNumber` of a live value.
    unsafe { qjs::JS_ToFloat64(ctx, &mut id, v) };
    free(ctx, v);
    let id = id as u32;
    // A context menu closed (after its chosen item was reported).
    if flag(ctx, e, c"closed") == Some(true) {
        resolve_closed(ctx);
        return UNDEFINED;
    }
    let m = menus(ctx);
    let f = [&m.app, &m.popup].into_iter().find_map(|slot| {
        slot.borrow()
            .iter()
            .find(|(i, _)| *i == id)
            // SAFETY: a dup for the call.
            .map(|&(_, f)| unsafe { qjs::JS_DupValue(ctx, f) })
    });
    let Some(f) = f else { return UNDEFINED };
    // SAFETY: a call of an owned function, freed after it.
    unsafe {
        let r = qjs::JS_Call(ctx, f, UNDEFINED, 0, ptr::null_mut());
        qjs::JS_FreeValue(ctx, f);
        if qjs::JS_IsException(r) {
            return r;
        }
        qjs::JS_FreeValue(ctx, r);
    }
    UNDEFINED
}

/// `Menu`: an object of its functions.
fn menu(ctx: *mut JsContext) -> JsValue {
    // SAFETY: a new object taking the functions.
    unsafe {
        let o = qjs::JS_NewObject(ctx);
        let set = reactive::new_function(ctx, c"setApplicationMenu", set_application_menu, 1);
        qjs::JS_SetPropertyStr(ctx, o, c"setApplicationMenu".as_ptr(), set);
        let pop = reactive::new_function(ctx, c"popup", popup, 2);
        qjs::JS_SetPropertyStr(ctx, o, c"popup".as_ptr(), pop);
        o
    }
}

/// The hand-written exports of `nsh/desktop` (with the registry's `Window`).
pub(crate) const EXPORTS: &[Item] = &[
    (
        "isAvailable",
        Export::Value(|ctx| {
            if os(ctx).is_desktop() {
                qjs::JS_TRUE
            } else {
                qjs::JS_FALSE
            }
        }),
        Ts::value("True on desktops (macOS, Windows, Linux).", "boolean"),
    ),
    (
        "titleBar",
        Export::Value(crate::env::title_bar),
        Ts::value(
            "The title bar's area while the app hides it (`Window.setTitleBar(\"hidden\")`),\nin points (reactive): its `height`, and the widths the system's window\ncontrols take at its `left` and `right` ends (macOS's traffic lights,\nWindows' caption buttons, GTK's window buttons). All 0 while the system\nshows its title bar, and off desktops.",
            "Accessor<TitleBarArea>",
        ),
    ),
    (
        "Menu",
        Export::Value(menu),
        Ts::value(
            "The application menu and context menus (desktops; nothing happens\nelsewhere).",
            "MenuModule",
        ),
    ),
];

/// `nsh/desktop`'s types (with the registry's).
pub(crate) const TYPES: &str = r#"/** The hidden title bar's area, in points. */
export interface TitleBarArea {
  /** Its height. */
  height: number;
  /** The width the window controls take at its left end (macOS). */
  left: number;
  /** The width the window controls take at its right end (Windows, GTK). */
  right: number;
}

/** A standard menu item: the platform carries it out, and labels it when the
 * item has no label (macOS adds the app's name to `about`, `hide` and
 * `quit`). The editing roles act on the focused text field and keep its own
 * shortcuts. `about`, `hide`, `hideOthers` and `showAll` are macOS's: left
 * out elsewhere. */
export type MenuRole =
  | "about"
  | "hide"
  | "hideOthers"
  | "showAll"
  | "quit"
  | "undo"
  | "redo"
  | "cut"
  | "copy"
  | "paste"
  | "selectAll"
  | "minimize"
  | "zoom"
  | "toggleFullScreen"
  | "close";

/** An item of a menu: a separator, a submenu, or an item that calls
 * `onSelect` (or carries out its `role`). `null`, `undefined` and `false`
 * items are left out (`cond && { … }`). */
export type MenuItem =
  | { type: "separator" }
  | {
      /** What it shows (a role's own label without it). */
      label?: string;
      /** A standard item. */
      role?: MenuRole;
      /** Its shortcut: modifiers and a key joined by `+`, in any case
       * (lowercase is the recommended style; a capital letter is the same
       * key, not Shift):
       * `"mod+n"`, `"mod+shift+z"`, `"alt+f4"`, `"f11"`. `mod` (`cmdorctrl`)
       * is Command on macOS and Control elsewhere; `cmd` (`command`,
       * `meta`, `super`, `win`), `ctrl` (`control`), `alt` (`option`,
       * `opt`), `shift`. Keys: the character a key types on the US layout
       * without Shift (`n`, `1`, `,`, `/`; `"shift+/"` for `?`), `plus`
       * (`=`), `minus` (`-`), `space`, `enter`, `escape`, `tab`,
       * `backspace`, `delete`, `insert`, `up`, `down`, `left`, `right`,
       * `home`, `end`, `pageup`, `pagedown`, `f1`–`f24`: the keystrokes of
       * `nsh/keymap`. A role has its platform's by default. */
      shortcut?: string;
      /** It can be chosen (default true). */
      enabled?: boolean;
      /** A check mark: set (true or false) for a checkable item. */
      checked?: boolean;
      /** Called when it is chosen (by a click or its shortcut). */
      onSelect?: () => void;
      /** Its items: makes it a submenu (`label` is required). */
      submenu?: readonly (MenuItem | null | undefined | false)[];
    };

/** The application menu and context menus. */
export interface MenuModule {
  /** Sets the application menu, a submenu per top-level item (`File`,
   * `Edit`, …): macOS's menu bar, whose first menu is the app menu (titled
   * with the app's name), or the window's menu bar on Windows and Linux,
   * which is not shown while the title bar is hidden (its shortcuts still
   * work: show its menus with `Menu.popup` from a title bar of the app's
   * own). Call it again to change it (in an `effect`, it follows the
   * signals it reads); `null` restores the platform's default menu. */
  setApplicationMenu(items: readonly (MenuItem | null | undefined | false)[] | null): void;
  /** Shows a context menu with its top left corner at `at` (the window's
   * coordinates, points), else at the pointer. The promise resolves once
   * the menu closes, chosen or dismissed: a button that opened it can look
   * pressed until then. */
  popup(items: readonly (MenuItem | null | undefined | false)[], at?: { x: number; y: number }): Promise<void>;
}
"#;

#[cfg(test)]
mod tests {
    use nsh_core::{desktop::MenuRole, ui::JsValue as _};

    /// `MenuRole`'s declaration (written here, with the docs of each role)
    /// names the roles the Rust type reads.
    #[test]
    fn menu_roles_are_declared() {
        let declared = super::TYPES
            .split("export type MenuRole =")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .expect("MenuRole's declaration");
        let mut names: Vec<String> = declared
            .split('|')
            .map(|n| n.trim().trim_matches('"').to_owned())
            .filter(|n| !n.is_empty())
            .collect();
        let host = crate::Host::new().unwrap();
        let mut roles: Vec<String> = host.with_raw(|ctx| {
            let js = crate::classes::host_outside_calls(ctx);
            MenuRole::ALL
                .iter()
                .map(|r| r.to_js(&js).unwrap().get().as_string().unwrap())
                .collect()
        });
        names.sort();
        roles.sort();
        assert_eq!(names, roles);
    }
}

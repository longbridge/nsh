//! The surface's environment: one signal per part
//! (window, safe area, keyboard, color scheme, font scale, screen reader,
//! reduced motion, layout direction, locale) written when the platform reports a
//! change, read through read-only accessors; `nsh/device` (`Platform`,
//! `PixelRatio`, `I18nManager`) and `nsh/accessibility`; the insets
//! context screens and modals provide (`safeAreaInsets`).

use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, c_int},
    ptr,
};

use nsh_core::platform::{Appearance, ColorScheme, Environment, Idiom, Os};
use rquickjs::qjs;

use crate::{
    modules::{Export, Item, Ts},
    reactive::{self, Native, arg},
};

type JsValue = qjs::JSValue;
type JsContext = qjs::JSContext;

const UNDEFINED: JsValue = qjs::JS_UNDEFINED;

/// The parts of the environment, each a signal.
#[derive(Clone, Copy)]
enum Part {
    Window,
    SafeArea,
    Keyboard,
    ColorScheme,
    FontScale,
    ScreenReader,
    ReduceMotion,
    Rtl,
    TitleBar,
    Locale,
}

const PARTS: usize = 10;

/// The environment's part of a context's state.
pub(crate) struct Env {
    /// What the platform reported last (`None`: nothing yet).
    current: RefCell<Option<Environment>>,
    /// One signal per part (owned).
    signals: [Cell<JsValue>; PARTS],
    /// Read-only accessors of the signals (owned memos).
    accessors: [Cell<JsValue>; PARTS],
    /// `I18nManager.allowRTL` / `forceRTL`.
    allow_rtl: Cell<bool>,
    force_rtl: Cell<bool>,
    /// The app's appearance (`nsh.json`, `setColorScheme`).
    appearance: Cell<Appearance>,
    /// The insets context (owned): screens and modals provide theirs.
    insets: Cell<JsValue>,
    /// `AccessibilityInfo.addEventListener` listeners: (id, screen reader
    /// (else reduced motion), function owned).
    listeners: RefCell<Vec<(u32, bool, JsValue)>>,
    next_listener: Cell<u32>,
}

impl Default for Env {
    fn default() -> Self {
        Env {
            current: RefCell::new(None),
            signals: std::array::from_fn(|_| Cell::new(UNDEFINED)),
            accessors: std::array::from_fn(|_| Cell::new(UNDEFINED)),
            allow_rtl: Cell::new(true),
            force_rtl: Cell::new(false),
            appearance: Cell::new(Appearance::System),
            insets: Cell::new(UNDEFINED),
            listeners: RefCell::new(Vec::new()),
            next_listener: Cell::new(1),
        }
    }
}

fn env<'a>(ctx: *mut JsContext) -> &'a Env {
    // SAFETY: a projection of the state's field, reached by shared
    // reference only.
    unsafe { &(*crate::state::state(ctx)).env }
}

impl Env {
    /// Frees the values the environment holds (the state is unregistered).
    pub(crate) fn free_all(&mut self, raw: *mut JsContext) {
        // SAFETY: the environment's own values; no JS runs from here on.
        unsafe {
            for c in self.signals.iter().chain(&self.accessors) {
                qjs::JS_FreeValue(raw, c.replace(UNDEFINED));
            }
            qjs::JS_FreeValue(raw, self.insets.replace(UNDEFINED));
            for (_, _, f) in self.listeners.borrow_mut().drain(..) {
                qjs::JS_FreeValue(raw, f);
            }
        }
    }
}

/// Makes the signals and accessors (after the graph and the renderer).
pub(crate) fn init(ctx: *mut JsContext) {
    let e = env(ctx);
    for i in 0..PARTS {
        let signal = reactive::new_signal(ctx, UNDEFINED);
        let mut data = [signal];
        // SAFETY: the function keeps a dup of the signal.
        let func = unsafe { qjs::JS_NewCFunctionData(ctx, Some(read), 0, 0, 1, data.as_mut_ptr()) };
        e.signals[i].set(signal);
        e.accessors[i].set(reactive::memo(ctx, func, false));
    }
    // The insets of the nearest screen or modal: the surface's safe area
    // outside of them.
    let safe_area = e.accessors[Part::SafeArea as usize].get();
    let mut args = [safe_area];
    // SAFETY: a context whose default is the accessor (borrowed).
    let insets = unsafe { crate::dom::context(ctx, UNDEFINED, 1, args.as_mut_ptr()) };
    e.insets.set(insets);
}

/// An accessor's run: reads its signal (tracked).
unsafe extern "C" fn read(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    // SAFETY: one data value: a signal object.
    unsafe { qjs::JS_Call(ctx, *data, UNDEFINED, 0, ptr::null_mut()) }
}

fn object(ctx: *mut JsContext, fields: &[(&CStr, JsValue)]) -> JsValue {
    // SAFETY: a new object taking the field values.
    unsafe {
        let o = qjs::JS_NewObject(ctx);
        for (k, v) in fields {
            qjs::JS_SetPropertyStr(ctx, o, k.as_ptr(), *v);
        }
        o
    }
}

/// A number as JS shows it: the f32's shortest decimal (`51.809525`, not
/// the f64 widening `51.80952453613281`).
fn number(v: f32) -> JsValue {
    let shortest = v.to_string().parse::<f64>().unwrap_or(v as f64);
    qjs::JS_NewFloat64(shortest)
}

fn boolean(v: bool) -> JsValue {
    if v { qjs::JS_TRUE } else { qjs::JS_FALSE }
}

fn string(ctx: *mut JsContext, s: &str) -> JsValue {
    // SAFETY: a new string.
    unsafe { qjs::JS_NewStringLen(ctx, s.as_ptr().cast(), s.len() as _) }
}

/// The layout direction: the user's language, unless the app turned it
/// off or forced it.
fn rtl(e: &Env, env: &Environment) -> bool {
    (e.allow_rtl.get() && env.rtl) || e.force_rtl.get()
}

/// The color scheme shown: the app's, else the system's.
fn scheme(e: &Env, env: &Environment) -> &'static str {
    match e.appearance.get().resolve(env.color_scheme) {
        ColorScheme::Light => "light",
        ColorScheme::Dark => "dark",
    }
}

/// Shows the surface in `appearance` (`nsh.json` `"appearance"`,
/// `setColorScheme`): `colorScheme()` follows it at once, the core
/// (`dark:` classes, the components' theme) and the platform's controls
/// with the root's `appearance` prop.
pub(crate) fn set_appearance(ctx: *mut JsContext, appearance: Appearance) -> JsValue {
    let e = env(ctx);
    e.appearance.set(appearance);
    let op = nsh_core::Op::SetProp {
        id: nsh_core::ROOT_ID,
        prop: nsh_core::Prop::Appearance(appearance),
    };
    if let Err(err) = crate::dom::push_op(ctx, op) {
        return err;
    }
    let value = string(ctx, scheme(e, &current(ctx)));
    reactive::set_signal(ctx, e.signals[Part::ColorScheme as usize].get(), value);
    reactive::finish_value(ctx, UNDEFINED)
}

/// `"light"` / `"dark"` / `"system"`.
pub(crate) fn parse_appearance(name: &str) -> Option<Appearance> {
    match name {
        "system" => Some(Appearance::System),
        "light" => Some(Appearance::Light),
        "dark" => Some(Appearance::Dark),
        _ => None,
    }
}

/// `setColorScheme(scheme)`.
unsafe extern "C" fn set_color_scheme(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let name = crate::dom::string_arg(ctx, arg(argc, argv, 0)).unwrap_or_default();
    match parse_appearance(&name) {
        Some(appearance) => set_appearance(ctx, appearance),
        // SAFETY: throws on a live context.
        None => unsafe {
            qjs::JS_ThrowTypeError(
                ctx,
                c"setColorScheme: expected \"light\", \"dark\" or \"system\"".as_ptr(),
            )
        },
    }
}

/// Takes in what the platform reports: writes the parts that changed.
pub(crate) fn set_environment(ctx: *mut JsContext, next: &Environment) -> Result<(), ()> {
    let e = env(ctx);
    let previous = e.current.replace(Some(*next));
    let changed = |f: &dyn Fn(&Environment) -> bool| match &previous {
        Some(p) => !f(p),
        None => true,
    };
    let insets = |i: &nsh_core::Edges<nsh_core::Points>| {
        [
            (c"top", number(i.top.0)),
            (c"right", number(i.right.0)),
            (c"bottom", number(i.bottom.0)),
            (c"left", number(i.left.0)),
        ]
    };
    let mut writes: Vec<(Part, JsValue)> = Vec::new();
    if changed(&|p| p.window == next.window) {
        let (w, h) = (next.window.width.0, next.window.height.0);
        writes.push((
            Part::Window,
            object(ctx, &[(c"width", number(w)), (c"height", number(h))]),
        ));
    }
    if changed(&|p| p.safe_area == next.safe_area) {
        writes.push((Part::SafeArea, object(ctx, &insets(&next.safe_area))));
    }
    if changed(&|p| p.title_bar == next.title_bar) {
        let t = next.title_bar;
        writes.push((
            Part::TitleBar,
            object(
                ctx,
                &[
                    (c"height", number(t.top.0)),
                    (c"left", number(t.left.0)),
                    (c"right", number(t.right.0)),
                ],
            ),
        ));
    }
    if changed(&|p| {
        p.keyboard_height == next.keyboard_height && p.keyboard_duration == next.keyboard_duration
    }) {
        writes.push((
            Part::Keyboard,
            object(
                ctx,
                &[
                    (c"height", number(next.keyboard_height.0)),
                    (c"duration", number(next.keyboard_duration)),
                ],
            ),
        ));
    }
    if changed(&|p| p.color_scheme == next.color_scheme) {
        writes.push((Part::ColorScheme, string(ctx, scheme(e, next))));
    }
    if changed(&|p| p.font_scale == next.font_scale) {
        writes.push((Part::FontScale, number(next.font_scale)));
    }
    if changed(&|p| p.locale == next.locale) {
        writes.push((Part::Locale, string(ctx, next.locale.as_str())));
    }
    let screen_reader = changed(&|p| p.screen_reader == next.screen_reader);
    if screen_reader {
        writes.push((Part::ScreenReader, boolean(next.screen_reader)));
    }
    let reduce_motion = changed(&|p| p.reduce_motion == next.reduce_motion);
    if reduce_motion {
        writes.push((Part::ReduceMotion, boolean(next.reduce_motion)));
    }
    if changed(&|p| rtl(e, p) == rtl(e, next)) {
        writes.push((Part::Rtl, boolean(rtl(e, next))));
    }
    // SAFETY: the state's renderer, by shared reference.
    unsafe { &(*crate::state::state(ctx)).dom }.set_direction(rtl(e, next));
    reactive::batch_with(ctx, || {
        for (part, value) in writes {
            reactive::set_signal(ctx, e.signals[part as usize].get(), value);
        }
    });
    let mut failed = reactive::finish_value(ctx, UNDEFINED);
    // Settings listeners hear about changes after the first report.
    if previous.is_some() {
        for (screen, now) in [
            (true, screen_reader.then_some(next.screen_reader)),
            (false, reduce_motion.then_some(next.reduce_motion)),
        ] {
            let Some(now) = now else { continue };
            let listeners: Vec<JsValue> = e
                .listeners
                .borrow()
                .iter()
                .filter(|(_, s, _)| *s == screen)
                // SAFETY: dups for the calls below.
                .map(|&(_, _, f)| unsafe { qjs::JS_DupValue(ctx, f) })
                .collect();
            for f in listeners {
                let mut args = [boolean(now)];
                // SAFETY: an owned function, freed after the call.
                let r = unsafe { qjs::JS_Call(ctx, f, UNDEFINED, 1, args.as_mut_ptr()) };
                unsafe {
                    qjs::JS_FreeValue(ctx, f);
                    if qjs::JS_IsException(r) {
                        failed = r;
                    } else {
                        qjs::JS_FreeValue(ctx, r);
                    }
                }
            }
        }
    }
    // SAFETY: an inline tag check.
    if unsafe { qjs::JS_IsException(failed) } {
        Err(())
    } else {
        Ok(())
    }
}

fn accessor(ctx: *mut JsContext, part: Part) -> JsValue {
    // SAFETY: a dup of an accessor the environment owns.
    unsafe { qjs::JS_DupValue(ctx, env(ctx).accessors[part as usize].get()) }
}

/// `titleBar()` (`nsh/desktop`): the hidden title bar's area (owned).
pub(crate) fn title_bar(ctx: *mut JsContext) -> JsValue {
    accessor(ctx, Part::TitleBar)
}

pub(crate) fn current(ctx: *mut JsContext) -> Environment {
    (*env(ctx).current.borrow()).unwrap_or_default()
}

/// The user's locale (`Intl`'s default), read where it is used: a change
/// applies to what is formatted next.
pub(crate) fn locale(ctx: *mut JsContext) -> nsh_core::platform::Locale {
    current(ctx).locale
}

/// `safeAreaInsets()`: the insets of the nearest screen or modal (an
/// accessor).
unsafe extern "C" fn safe_area_insets(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
) -> JsValue {
    let insets = env(ctx).insets.get();
    // SAFETY: calling the context reads its nearest value.
    unsafe { qjs::JS_Call(ctx, insets, UNDEFINED, 0, ptr::null_mut()) }
}

/// The insets context (borrowed), which screens and modals provide.
pub(crate) fn insets_context(ctx: *mut JsContext) -> JsValue {
    env(ctx).insets.get()
}

/// The values registries re-export as they are (`Registry::reexport`):
/// the environment's accessors and hooks the UI sets export.
pub(crate) const REEXPORTS: &[Item] = &[
    (
        "safeArea",
        Export::Value(|ctx| accessor(ctx, Part::SafeArea)),
        Ts::value(
            "Insets of the surface not covered by system UI (notch, home indicator,\ntitle bar), in points (reactive).",
            "Accessor<Insets>",
        ),
    ),
    (
        "keyboardFrame",
        Export::Value(|ctx| accessor(ctx, Part::Keyboard)),
        Ts::value(
            "Height of the software keyboard over the surface (0 when hidden) and the\nduration of its animation in seconds (reactive).",
            "Accessor<{ height: number; duration: number }>",
        ),
    ),
    (
        "safeAreaInsets",
        Export::Function(safe_area_insets, 0),
        Ts::function(
            "Insets not covered by system UI or headers where this component renders:\nthe surface's safe area, a dialog's, or a page's (below its header).",
            &["(): Accessor<Insets>"],
        ),
    ),
    (
        "nextTick",
        Export::Function(crate::standard::process::next_tick, 1),
        Ts::function(
            "Calls `callback` with `args` as a JS job, after the current one and before timers.",
            &["<A extends unknown[]>(callback: (...args: A) => void, ...args: A): void"],
        ),
    ),
];

/// Value `name` a registry re-exports or names as native
/// (`Registry::native_values`): [`REEXPORTS`] and navigation's.
pub(crate) fn reexport(ctx: *mut JsContext, name: &str) -> JsValue {
    let item = REEXPORTS
        .iter()
        .chain(crate::navigation::EXPORTS)
        .find(|(n, ..)| *n == name);
    match item {
        Some((n, Export::Function(f, length), _)) => {
            let cname = std::ffi::CString::new(*n).unwrap_or_default();
            reactive::new_function(ctx, &cname, *f, *length)
        }
        Some((_, Export::Value(make), _)) => make(ctx),
        None => {
            let message =
                std::ffi::CString::new(format!("nsh: the runtime has no native `{name}`"))
                    .unwrap_or_default();
            // SAFETY: throws on a live context.
            unsafe { qjs::JS_ThrowReferenceError(ctx, c"%s".as_ptr(), message.as_ptr()) }
        }
    }
}

// --- nsh/device ---------------------------------------------------------------------

fn os_version(ctx: *mut JsContext, env: &Environment) -> JsValue {
    let (major, minor) = env.os_version;
    // React Native: a string on iOS ("17.5"), the API level on Android.
    if env.os == Os::Android {
        qjs::JS_NewFloat64(major as f64)
    } else {
        string(ctx, &format!("{major}.{minor}"))
    }
}

fn idiom(env: &Environment) -> &'static str {
    match env.idiom {
        Idiom::Phone => "phone",
        Idiom::Pad => "pad",
        Idiom::Mac => "mac",
        Idiom::Tv => "tv",
        Idiom::Desktop => "desktop",
    }
}

/// `Platform`: the OS and form factor, constant while the app runs.
fn platform(ctx: *mut JsContext) -> JsValue {
    let e = current(ctx);
    let constants = object(
        ctx,
        &[
            (c"osVersion", os_version(ctx, &e)),
            (c"interfaceIdiom", string(ctx, idiom(&e))),
        ],
    );
    let select = reactive::new_function(ctx, c"select", platform_select, 1);
    object(
        ctx,
        &[
            (c"OS", string(ctx, e.os.name())),
            (c"isDesktop", boolean(e.desktop())),
            (c"Version", os_version(ctx, &e)),
            (c"isPad", boolean(e.idiom == Idiom::Pad)),
            (c"isTV", boolean(e.idiom == Idiom::Tv)),
            (c"isMacCatalyst", boolean(e.idiom == Idiom::Mac)),
            (c"constants", constants),
            (c"select", select),
        ],
    )
}

/// `Platform.select(spec)`: this OS's value, else `mobile` / `desktop`,
/// else `native` (not on the web, as in React Native), else `default`.
unsafe extern "C" fn platform_select(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let spec = arg(argc, argv, 0);
    // SAFETY: an inline check, then property reads of a live object.
    if !unsafe { qjs::JS_IsObject(spec) } {
        return UNDEFINED;
    }
    let env = current(ctx);
    let kind = if env.desktop() { "desktop" } else { "mobile" };
    let keys = [env.os.name(), kind, "native", "default"];
    for key in keys
        .into_iter()
        .filter(|k| *k != "native" || env.os != Os::Web)
    {
        let key = std::ffi::CString::new(key).unwrap_or_default();
        let v = unsafe { qjs::JS_GetPropertyStr(ctx, spec, key.as_ptr()) };
        if !unsafe { qjs::JS_IsUndefined(v) } {
            return v;
        }
    }
    UNDEFINED
}

/// `PixelRatio`.
fn pixel_ratio(ctx: *mut JsContext) -> JsValue {
    let natives: [(&CStr, Native, c_int); 4] = [
        (c"get", pixel_ratio_get, 0),
        (c"getFontScale", pixel_ratio_font_scale, 0),
        (c"getPixelSizeForLayoutSize", pixel_size_for_layout_size, 1),
        (c"roundToNearestPixel", round_to_nearest_pixel, 1),
    ];
    let fields: Vec<(&CStr, JsValue)> = natives
        .iter()
        .map(|&(name, f, length)| (name, reactive::new_function(ctx, name, f, length)))
        .collect();
    object(ctx, &fields)
}

unsafe extern "C" fn pixel_ratio_get(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
) -> JsValue {
    number(current(ctx).pixel_ratio)
}

unsafe extern "C" fn pixel_ratio_font_scale(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
) -> JsValue {
    number(current(ctx).font_scale)
}

fn number_arg(ctx: *mut JsContext, argc: c_int, argv: *mut JsValue) -> f64 {
    let mut v = 0f64;
    // SAFETY: converting a live value.
    unsafe { qjs::JS_ToFloat64(ctx, &mut v, arg(argc, argv, 0)) };
    v
}

unsafe extern "C" fn pixel_size_for_layout_size(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let ratio = current(ctx).pixel_ratio as f64;
    qjs::JS_NewFloat64((number_arg(ctx, argc, argv) * ratio).round())
}

unsafe extern "C" fn round_to_nearest_pixel(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let ratio = (current(ctx).pixel_ratio as f64).max(f64::MIN_POSITIVE);
    qjs::JS_NewFloat64((number_arg(ctx, argc, argv) * ratio).round() / ratio)
}

/// `I18nManager`: the layout direction.
fn i18n_manager(ctx: *mut JsContext) -> JsValue {
    // SAFETY: a new object with a getter and functions it takes.
    unsafe {
        let o = object(
            ctx,
            &[
                (c"doLeftAndRightSwapInRTL", qjs::JS_TRUE),
                (
                    c"allowRTL",
                    reactive::new_function(ctx, c"allowRTL", allow_rtl, 1),
                ),
                (
                    c"forceRTL",
                    reactive::new_function(ctx, c"forceRTL", force_rtl, 1),
                ),
                (
                    c"getConstants",
                    reactive::new_function(ctx, c"getConstants", i18n_constants, 0),
                ),
            ],
        );
        let getter = reactive::new_function(ctx, c"isRTL", is_rtl, 0);
        let atom = qjs::JS_NewAtom(ctx, c"isRTL".as_ptr());
        qjs::JS_DefineProperty(
            ctx,
            o,
            atom,
            UNDEFINED,
            getter,
            UNDEFINED,
            (qjs::JS_PROP_HAS_GET | qjs::JS_PROP_HAS_ENUMERABLE | qjs::JS_PROP_ENUMERABLE) as c_int,
        );
        qjs::JS_FreeAtom(ctx, atom);
        qjs::JS_FreeValue(ctx, getter);
        o
    }
}

/// The layout direction (tracked).
unsafe extern "C" fn is_rtl(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
) -> JsValue {
    let rtl = env(ctx).accessors[Part::Rtl as usize].get();
    // SAFETY: calling an accessor the environment owns.
    unsafe { qjs::JS_Call(ctx, rtl, UNDEFINED, 0, ptr::null_mut()) }
}

unsafe extern "C" fn i18n_constants(
    ctx: *mut JsContext,
    this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    // SAFETY: as `is_rtl`.
    let rtl = unsafe { is_rtl(ctx, this, argc, argv) };
    object(
        ctx,
        &[(c"isRTL", rtl), (c"doLeftAndRightSwapInRTL", qjs::JS_TRUE)],
    )
}

fn set_rtl_flag(ctx: *mut JsContext, argc: c_int, argv: *mut JsValue, force: bool) -> JsValue {
    // SAFETY: converting a live value.
    let on = unsafe { qjs::JS_ToBool(ctx, arg(argc, argv, 0)) } == 1;
    let e = env(ctx);
    if force {
        e.force_rtl.set(on);
    } else {
        e.allow_rtl.set(on);
    }
    let now = current(ctx);
    let value = rtl(e, &now);
    // SAFETY: the state's renderer, by shared reference.
    unsafe { &(*crate::state::state(ctx)).dom }.set_direction(value);
    reactive::set_signal(ctx, e.signals[Part::Rtl as usize].get(), boolean(value));
    reactive::finish_value(ctx, UNDEFINED)
}

unsafe extern "C" fn allow_rtl(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    set_rtl_flag(ctx, argc, argv, false)
}

unsafe extern "C" fn force_rtl(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    set_rtl_flag(ctx, argc, argv, true)
}

/// The exports of `nsh/device`.
pub(crate) const DEVICE_EXPORTS: &[Item] = &[
    (
        "windowSize",
        Export::Value(|ctx| accessor(ctx, Part::Window)),
        Ts::value(
            "Size of the surface in points (reactive).",
            "Accessor<{ width: number; height: number }>",
        ),
    ),
    (
        "colorScheme",
        Export::Value(|ctx| accessor(ctx, Part::ColorScheme)),
        Ts::value(
            "The appearance shown (reactive): the system's, unless the app set its\nown (`nsh.json` `\"appearance\"`, `setColorScheme`).",
            "Accessor<\"light\" | \"dark\">",
        ),
    ),
    (
        "setColorScheme",
        Export::Function(set_color_scheme, 1),
        Ts::function(
            "Shows the app in `scheme` whatever the system's appearance, at once:\n`colorScheme()`, `dark:` classes, the components' theme and the\nplatform's controls (alerts, the status bar's default style) follow it;\n`\"system\"` follows the system again. `nsh.json` `\"appearance\"` sets it\nfrom the start.",
            &["(scheme: \"light\" | \"dark\" | \"system\"): void"],
        ),
    ),
    (
        "fontScale",
        Export::Value(|ctx| accessor(ctx, Part::FontScale)),
        Ts::value(
            "The user's preferred text size relative to the default (reactive):\nthe system's, unless the embedding app set its own.",
            "Accessor<number>",
        ),
    ),
    (
        "locale",
        Export::Value(|ctx| accessor(ctx, Part::Locale)),
        Ts::value(
            "The user's locale as a BCP 47 tag (`\"en-US\"`, `\"zh-Hans-CN\"`), which\n`Intl` formats for by default (reactive): the system's, unless the\nembedding app set its own.",
            "Accessor<string>",
        ),
    ),
    (
        "Platform",
        Export::Value(platform),
        Ts::value(
            "The platform the app runs on (React Native's `Platform`).",
            "PlatformStatic",
        ),
    ),
    (
        "PixelRatio",
        Export::Value(pixel_ratio),
        Ts::value(
            "Device pixels per point (React Native's `PixelRatio`). Not reactive:\nread in components that also read `windowSize()` to follow a window\nmoving to another screen.",
            "PixelRatioStatic",
        ),
    ),
    (
        "I18nManager",
        Export::Value(i18n_manager),
        Ts::value(
            "Right-to-left layout (React Native's `I18nManager`). Right to left,\nrows run from the right and `left` / `right` style properties mean start\nand end. Changes apply at once (no restart).",
            "I18nManagerStatic",
        ),
    ),
];

/// `nsh/device`'s types.
pub(crate) const DEVICE_TYPES: &str = r#"export interface PlatformSelectSpec<T> {
  ios?: T;
  android?: T;
  macos?: T;
  windows?: T;
  linux?: T;
  web?: T;
  /** iOS, Android, and phone and tablet browsers. */
  mobile?: T;
  /** macOS, Windows, Linux, and desktop browsers. */
  desktop?: T;
  /** Every platform but the web. */
  native?: T;
  default?: T;
}

export interface PlatformStatic {
  readonly OS: OS;
  /** Desktop behavior applies: macOS, Windows, Linux, and browsers with a
   * mouse and a wide viewport (`interfaceIdiom` `"desktop"`). */
  readonly isDesktop: boolean;
  /** iOS and desktops: the version string (`"17.5"`); Android: the API
   * level; the web: `"0.0"`. */
  readonly Version: string | number;
  readonly isPad: boolean;
  readonly isTV: boolean;
  /** Mac Catalyst in the Mac idiom. */
  readonly isMacCatalyst: boolean;
  readonly constants: {
    readonly osVersion: string | number;
    readonly interfaceIdiom: "phone" | "pad" | "mac" | "tv" | "desktop";
  };
  /** The value for this platform: its OS (`ios`, `macos`, `web`, ...),
   * else `mobile` / `desktop`, else `native` (not on the web), else
   * `default`. */
  select<T>(spec: PlatformSelectSpec<T>): T | undefined;
}

export interface PixelRatioStatic {
  get(): number;
  getFontScale(): number;
  getPixelSizeForLayoutSize(size: number): number;
  roundToNearestPixel(size: number): number;
}

export interface I18nManagerStatic {
  /** Whether the app lays out right to left (reactive). */
  isRTL: boolean;
  /** `left` and `right` swap right to left (always, as React Native's default). */
  doLeftAndRightSwapInRTL: true;
  /** Follow right-to-left languages (default true). */
  allowRTL(allow: boolean): void;
  /** Lay out right to left whatever the language. */
  forceRTL(force: boolean): void;
  getConstants(): { isRTL: boolean; doLeftAndRightSwapInRTL: boolean };
}
"#;

// --- nsh/accessibility --------------------------------------------------------------

/// `AccessibilityInfo`: the runtime module's commands, and the settings.
fn accessibility_info(ctx: *mut JsContext) -> JsValue {
    let info = crate::components::runtime_module(ctx, "nsh/accessibility", "AccessibilityInfo")
        // SAFETY: a new object.
        .unwrap_or_else(|| unsafe { qjs::JS_NewObject(ctx) });
    let natives: [(&CStr, Native, c_int); 3] = [
        (c"isScreenReaderEnabled", is_screen_reader_enabled, 0),
        (c"isReduceMotionEnabled", is_reduce_motion_enabled, 0),
        (c"addEventListener", add_event_listener, 2),
    ];
    for (name, f, length) in natives {
        let func = reactive::new_function(ctx, name, f, length);
        // SAFETY: the object takes the function.
        unsafe { qjs::JS_SetPropertyStr(ctx, info, name.as_ptr(), func) };
    }
    info
}

/// A promise resolved with `value` (owned).
fn resolved(ctx: *mut JsContext, value: JsValue) -> JsValue {
    let mut funcs = [UNDEFINED; 2];
    // SAFETY: a new promise; its functions and `value` are freed after the
    // call.
    unsafe {
        let promise = qjs::JS_NewPromiseCapability(ctx, funcs.as_mut_ptr());
        let mut args = [value];
        let r = qjs::JS_Call(ctx, funcs[0], UNDEFINED, 1, args.as_mut_ptr());
        qjs::JS_FreeValue(ctx, r);
        qjs::JS_FreeValue(ctx, value);
        qjs::JS_FreeValue(ctx, funcs[0]);
        qjs::JS_FreeValue(ctx, funcs[1]);
        promise
    }
}

unsafe extern "C" fn is_screen_reader_enabled(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
) -> JsValue {
    resolved(ctx, boolean(current(ctx).screen_reader))
}

unsafe extern "C" fn is_reduce_motion_enabled(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
) -> JsValue {
    resolved(ctx, boolean(current(ctx).reduce_motion))
}

/// `addEventListener("screenReaderChanged" | "reduceMotionChanged", f)`:
/// `{ remove() }`.
unsafe extern "C" fn add_event_listener(
    ctx: *mut JsContext,
    _this: JsValue,
    argc: c_int,
    argv: *mut JsValue,
) -> JsValue {
    let screen = match crate::dom::string_arg(ctx, arg(argc, argv, 0)).as_deref() {
        Some("screenReaderChanged") => true,
        Some("reduceMotionChanged") => false,
        _ => {
            // SAFETY: throws on a live context.
            return unsafe {
                qjs::JS_ThrowTypeError(
                    ctx,
                    c"addEventListener: \"screenReaderChanged\" or \"reduceMotionChanged\""
                        .as_ptr(),
                )
            };
        }
    };
    let f = arg(argc, argv, 1);
    // SAFETY: an inline check, then a dup the environment owns.
    if !unsafe { qjs::JS_IsFunction(ctx, f) } {
        return unsafe { qjs::JS_ThrowTypeError(ctx, c"addEventListener: a function".as_ptr()) };
    }
    let e = env(ctx);
    let id = e.next_listener.get();
    e.next_listener.set(id + 1);
    e.listeners
        .borrow_mut()
        .push((id, screen, unsafe { qjs::JS_DupValue(ctx, f) }));
    let mut data = [qjs::JS_NewFloat64(id as f64)];
    // SAFETY: the function keeps a dup of `data`.
    let remove =
        unsafe { qjs::JS_NewCFunctionData(ctx, Some(remove_listener), 0, 0, 1, data.as_mut_ptr()) };
    object(ctx, &[(c"remove", remove)])
}

unsafe extern "C" fn remove_listener(
    ctx: *mut JsContext,
    _this: JsValue,
    _argc: c_int,
    _argv: *mut JsValue,
    _magic: c_int,
    data: *mut JsValue,
) -> JsValue {
    let mut id = 0f64;
    // SAFETY: one data value, a number.
    unsafe { qjs::JS_ToFloat64(ctx, &mut id, *data) };
    let removed: Vec<JsValue> = {
        let mut listeners = env(ctx).listeners.borrow_mut();
        let (gone, kept): (Vec<_>, Vec<_>) =
            listeners.drain(..).partition(|(i, ..)| *i == id as u32);
        *listeners = kept;
        gone.into_iter().map(|(.., f)| f).collect()
    };
    for f in removed {
        // SAFETY: values the environment owned.
        unsafe { qjs::JS_FreeValue(ctx, f) };
    }
    UNDEFINED
}

/// The exports of `nsh/accessibility` (with the runtime registry's).
pub(crate) const ACCESSIBILITY_EXPORTS: &[Item] = &[
    (
        "reduceMotion",
        Export::Value(|ctx| accessor(ctx, Part::ReduceMotion)),
        Ts::value(
            "Whether the user asked for less motion (reactive; also `motion-reduce:`).",
            "Accessor<boolean>",
        ),
    ),
    (
        "screenReaderEnabled",
        Export::Value(|ctx| accessor(ctx, Part::ScreenReader)),
        Ts::value(
            "Whether a screen reader (VoiceOver, TalkBack) runs (reactive).",
            "Accessor<boolean>",
        ),
    ),
    (
        "AccessibilityInfo",
        Export::Value(accessibility_info),
        Ts::value(
            "Screen reader and motion settings. `screenReaderEnabled()` and\n`reduceMotion()` are the reactive forms.",
            "AccessibilityInfoModule & AccessibilitySettings",
        ),
    ),
];

/// `nsh/accessibility`'s types (with the runtime registry's module).
pub(crate) const ACCESSIBILITY_TYPES: &str = r#"export type AccessibilityChangeEvent = "screenReaderChanged" | "reduceMotionChanged";

/** The settings, at once, and their changes. */
export interface AccessibilitySettings {
  isScreenReaderEnabled(): Promise<boolean>;
  isReduceMotionEnabled(): Promise<boolean>;
  addEventListener(event: AccessibilityChangeEvent, listener: (enabled: boolean) => void): { remove(): void };
}
"#;

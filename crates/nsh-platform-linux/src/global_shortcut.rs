//! Global shortcuts on Linux (`DesktopCommand::RegisterGlobalShortcut`,
//! `nsh/desktop`'s `GlobalShortcut`), through the desktop portal's
//! `org.freedesktop.portal.GlobalShortcuts`: the only way on Wayland, where
//! no app may grab keys, and served on X11 desktops too (Plasma, GNOME 48+,
//! Hyprland's portal).
//!
//! - A portal session binds a set of shortcuts once: each change of the set
//!   closes the session and binds the whole set in a new one, on the next
//!   idle, so registering several at once asks once.
//! - A shortcut's id on the portal is its keystroke (`ctrl+shift+k`): what a
//!   desktop that lets the user bind it shows, and what Hyprland's
//!   `bind = …, global, <app id>:<id>` names. Its keystroke is only the
//!   preferred trigger: the desktop may ask the user, or bind another.
//! - Without a portal, or when it refuses, a registration answers
//!   `registered: false`.

use std::{
    cell::{Cell, RefCell},
    rc::Weak,
};

use gtk4::{gio, glib, prelude::*};
use nsh_core::{EventKind, EventPayload, PayloadValue, ROOT_ID, desktop::Shortcut};

use crate::surface::Surface;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SESSION: &str = "org.freedesktop.portal.Session";

/// A shortcut the app registered.
struct Global {
    id: u32,
    shortcut: Shortcut,
    surface: Weak<Surface>,
    /// The portal's answer was reported.
    answered: bool,
}

#[derive(Default)]
struct State {
    globals: Vec<Global>,
    /// The portal session bound to the set, once created.
    session: Option<String>,
    /// The `Activated` subscription.
    activated: Option<gio::SignalSubscription>,
    /// A bind waits for the next idle.
    scheduled: bool,
    /// Bumped at each bind: answers of an earlier one are dropped.
    generation: u64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
    static TOKEN: Cell<u32> = const { Cell::new(0) };
}

/// Registers `shortcut` as `id` for `surface`'s app.
pub(crate) fn register(surface: &std::rc::Rc<Surface>, id: u32, shortcut: Shortcut) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.globals.retain(|g| g.id != id);
        s.globals.push(Global {
            id,
            shortcut,
            surface: std::rc::Rc::downgrade(surface),
            answered: false,
        });
    });
    schedule();
}

/// Gives back the shortcut registered as `id`.
pub(crate) fn unregister(id: u32) {
    let had = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let before = s.globals.len();
        s.globals.retain(|g| g.id != id);
        before != s.globals.len()
    });
    if had {
        schedule();
    }
}

/// The shortcut's id on the portal: its keystroke, lowercase
/// (`ctrl+shift+k`).
pub(crate) fn portal_id(shortcut: &Shortcut) -> String {
    shortcut.label().to_lowercase().replace("win+", "super+")
}

/// The shortcut as the XDG shortcuts spec writes a trigger
/// (`CTRL+SHIFT+k`).
pub(crate) fn trigger(shortcut: &Shortcut) -> Option<String> {
    let key = crate::chrome::accelerator(&Shortcut::key(shortcut.key))?;
    let mut out = String::new();
    for (on, name) in [
        (shortcut.ctrl, "CTRL+"),
        (shortcut.alt, "ALT+"),
        (shortcut.shift, "SHIFT+"),
        (shortcut.meta, "LOGO+"),
    ] {
        if on {
            out.push_str(name);
        }
    }
    out.push_str(&key);
    Some(out)
}

fn schedule() {
    if STATE.with(|s| std::mem::replace(&mut s.borrow_mut().scheduled, true)) {
        return;
    }
    glib::idle_add_local_once(bind);
}

/// Answers every global not answered yet: registered when `registered`
/// holds for its portal id; those refused are dropped.
fn answer(registered: impl Fn(&str) -> bool) {
    let answers: Vec<(Weak<Surface>, u32, bool)> = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let answers: Vec<(Weak<Surface>, u32, bool)> = s
            .globals
            .iter_mut()
            .filter(|g| !g.answered)
            .map(|g| {
                g.answered = true;
                (g.surface.clone(), g.id, registered(&portal_id(&g.shortcut)))
            })
            .collect();
        // Refused shortcuts are no longer registered.
        let mut refused = Vec::new();
        for (_, id, ok) in &answers {
            if !ok {
                refused.push(*id);
            }
        }
        s.globals.retain(|g| !refused.contains(&g.id));
        answers
    });
    for (surface, id, registered) in answers {
        if let Some(surface) = surface.upgrade() {
            surface.event(
                ROOT_ID,
                EventKind::Menu,
                EventPayload::Map(vec![
                    ("id", PayloadValue::Number(f64::from(id))),
                    ("registered", PayloadValue::Bool(registered)),
                ]),
            );
        }
    }
}

/// Binds the set in a new session, closing the last one.
fn bind() {
    let (old, generation, empty) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.scheduled = false;
        s.generation += 1;
        (s.session.take(), s.generation, s.globals.is_empty())
    });
    let bus = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(bus) => bus,
        Err(_) => return answer(|_| false),
    };
    if let Some(old) = old {
        bus.call(
            Some(PORTAL),
            &old,
            SESSION,
            "Close",
            None,
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
    if empty {
        return;
    }
    subscribe(&bus);
    let session_token = token();
    let options = glib::VariantDict::new(None);
    options.insert("session_handle_token", session_token.as_str());
    request(
        &bus,
        "CreateSession",
        |options| (options,).to_variant(),
        options,
        move |bus, results| {
            if !current(generation) {
                return;
            }
            let Some(session) = results.and_then(|r| {
                r.lookup_value("session_handle", None)
                    .and_then(|v| v.str().map(str::to_owned))
            }) else {
                log::warn!("nsh: the desktop portal has no global shortcuts session");
                return answer(|_| false);
            };
            STATE.with(|s| s.borrow_mut().session = Some(session.clone()));
            bind_shortcuts(bus, &session, generation);
        },
    );
}

fn current(generation: u64) -> bool {
    STATE.with(|s| s.borrow().generation == generation)
}

fn bind_shortcuts(bus: &gio::DBusConnection, session: &str, generation: u64) {
    let shortcuts: Vec<(String, glib::Variant)> = STATE.with(|s| {
        s.borrow()
            .globals
            .iter()
            .map(|g| {
                let id = portal_id(&g.shortcut);
                let props = glib::VariantDict::new(None);
                props.insert("description", g.shortcut.label());
                if let Some(trigger) = trigger(&g.shortcut) {
                    props.insert("preferred_trigger", trigger);
                }
                (id, props.end())
            })
            .collect()
    });
    let Ok(session) = glib::variant::ObjectPath::try_from(session.to_owned()) else {
        return answer(|_| false);
    };
    request(
        bus,
        "BindShortcuts",
        move |options| {
            (
                session.clone(),
                glib::Variant::array_from_iter_with_type(
                    glib::VariantTy::new("(sa{sv})").expect("a valid type"),
                    shortcuts.iter().map(|(id, props)| {
                        glib::Variant::tuple_from_iter([id.to_variant(), props.clone()])
                    }),
                ),
                "",
                options,
            )
                .to_variant()
        },
        glib::VariantDict::new(None),
        move |_, results| {
            if !current(generation) {
                return;
            }
            let bound: Vec<String> = results
                .and_then(|r| r.lookup_value("shortcuts", None))
                .map(|list| {
                    list.iter()
                        .filter_map(|s| s.child_value(0).str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            answer(|id| bound.iter().any(|b| b == id));
        },
    );
}

/// Subscribes to the portal's `Activated`, once.
fn subscribe(bus: &gio::DBusConnection) {
    if STATE.with(|s| s.borrow().activated.is_some()) {
        return;
    }
    let subscription = bus.subscribe_to_signal(
        Some(PORTAL),
        Some(INTERFACE),
        Some("Activated"),
        Some(PORTAL_PATH),
        None,
        gio::DBusSignalFlags::NONE,
        |signal| {
            let session = signal.parameters.child_value(0);
            let id = signal.parameters.child_value(1);
            let (Some(session), Some(id)) = (session.str(), id.str()) else {
                return;
            };
            pressed(session, id);
        },
    );
    STATE.with(|s| s.borrow_mut().activated = Some(subscription));
}

/// The shortcut called `id` of `session` was pressed.
fn pressed(session: &str, id: &str) {
    let found = STATE.with(|s| {
        let s = s.borrow();
        if s.session.as_deref() != Some(session) {
            return None;
        }
        s.globals
            .iter()
            .find(|g| portal_id(&g.shortcut) == id)
            .map(|g| (g.surface.clone(), g.id))
    });
    if let Some((surface, id)) = found
        && let Some(surface) = surface.upgrade()
    {
        surface.event(
            ROOT_ID,
            EventKind::Menu,
            EventPayload::Map(vec![("id", PayloadValue::Number(f64::from(id)))]),
        );
    }
}

/// A new handle token.
fn token() -> String {
    let n = TOKEN.with(|t| {
        t.set(t.get() + 1);
        t.get()
    });
    format!("nsh{}_{n}", std::process::id())
}

/// Calls portal method `method` with `args(options)` (options with a
/// `handle_token` added) and hands its `Request`'s results to `done`
/// (`None`: refused, cancelled or no portal).
fn request(
    bus: &gio::DBusConnection,
    method: &'static str,
    args: impl FnOnce(glib::Variant) -> glib::Variant,
    options: glib::VariantDict,
    done: impl FnOnce(&gio::DBusConnection, Option<glib::VariantDict>) + 'static,
) {
    let handle_token = token();
    options.insert("handle_token", handle_token.as_str());
    // The request's path, known before the call so that no response is
    // missed: the sender's unique name without its `:`, dots as `_`.
    let sender = bus
        .unique_name()
        .map(|n| n.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default();
    let path = format!("{PORTAL_PATH}/request/{sender}/{handle_token}");
    let done = std::rc::Rc::new(RefCell::new(Some(done)));
    let subscription: std::rc::Rc<RefCell<Option<gio::SignalSubscription>>> =
        std::rc::Rc::default();
    let finish = {
        let done = done.clone();
        let subscription = subscription.clone();
        let bus = bus.clone();
        move |results: Option<glib::VariantDict>| {
            subscription.borrow_mut().take();
            if let Some(done) = done.borrow_mut().take() {
                done(&bus, results);
            }
        }
    };
    let finish = std::rc::Rc::new(RefCell::new(Some(finish)));
    let on_response = finish.clone();
    *subscription.borrow_mut() = Some(bus.subscribe_to_signal(
        Some(PORTAL),
        Some(REQUEST),
        Some("Response"),
        Some(&path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let code = signal.parameters.child_value(0).get::<u32>().unwrap_or(2);
            let results = glib::VariantDict::new(Some(&signal.parameters.child_value(1)));
            if let Some(finish) = on_response.borrow_mut().take() {
                finish((code == 0).then_some(results));
            }
        },
    ));
    let on_error = finish;
    bus.call(
        Some(PORTAL),
        PORTAL_PATH,
        INTERFACE,
        method,
        Some(&args(options.end())),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        move |result| {
            if let Err(e) = result {
                log::warn!("nsh: the desktop portal refused {method}: {e}");
                if let Some(finish) = on_error.borrow_mut().take() {
                    finish(None);
                }
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use nsh_core::desktop::{Key, Shortcut};

    use super::*;

    #[test]
    fn shortcuts_name_and_trigger_for_the_portal() {
        let s = Shortcut::parse("ctrl+shift+k", false).unwrap();
        assert_eq!(portal_id(&s), "ctrl+shift+k");
        assert_eq!(trigger(&s).as_deref(), Some("CTRL+SHIFT+k"));
        let s = Shortcut {
            meta: true,
            alt: true,
            ..Shortcut::key(Key::F(5))
        };
        assert_eq!(portal_id(&s), "alt+super+f5");
        assert_eq!(trigger(&s).as_deref(), Some("ALT+LOGO+F5"));
        let s = Shortcut::parse("mod+,", false).unwrap();
        assert_eq!(trigger(&s).as_deref(), Some("CTRL+comma"));
    }
}

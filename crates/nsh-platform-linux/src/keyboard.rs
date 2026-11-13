//! The keys and text of a focused view: the `on_key_down` / `on_key_up`
//! events a view and its ancestors hear, the capture rules mounted on
//! them, text generation through an input method (`on_text_input`; its
//! preedit is `on_composition_change`) and where the input method shows its
//! candidates (`imeCursorArea`).
//!
//! The focus and the keys themselves are [`crate::key_focus`]'s: its one
//! key controller per window decides, in GTK's capture phase, whether the
//! key is the app's ([`crate::keymap::claims`]), captured by the focused
//! view (`capture_keys`), an active composition's input method, or a
//! default (Tab's traversal, Return and Space pressing, a context menu,
//! scrolling), and, last, ordinary text through the view's input method.
//! This module only does what that decision needs: it builds the
//! [`KeyEvent`], matches the focus's capture rules ([`captures`]), reports
//! the event to the engine as the plain `KeyDown` / `KeyUp` events
//! ([`notify`], [`released`]), and runs the input method ([`text`]). No
//! engine answer is waited for: what a listener does is reported
//! afterwards, in the app's frame.
//!
//! Events follow the focus, in both phases: a key up goes to the view
//! focused when the source reports it, even when its press went to another
//! view (a focus move, a popup grab), as a browser reports it on the
//! focused element. No platform retains an original down target, and a
//! blur emits no synthetic release. Keyboard ancestry follows
//! `ViewProps::key_parent` for a portaled view whose native `Insert` parent
//! differs from its logical parent; focus, hit testing and layout keep the
//! native parent. `KeyEvent::code` is the DOM `KeyboardEvent.code` of the
//! hardware keycode GDK reports, so it names the key's physical place on
//! any layout (`"KeyA"` for the key an AZERTY or Cyrillic layout may make
//! type `q` or `ф`); `""` when the keycode names no key this backend
//! knows, never a value derived from the character. The logical key is
//! GDK's actual keyval, so Shift's case and a non-US character are
//! reported as the source gives them.
//!
//! A captured key still notifies the focused view and its ancestors (the
//! engine bubbles it), with `KeyEvent::default_prevented` set as the
//! mounted policy's decision; it runs no default (no traversal, activation
//! or context menu) and commits no text. A key that only *listens* is not
//! captured: the app hears it and the default still runs.
//!
//! A native input, web view, native component or video owns its keys and
//! text: the input method is never attached to it and its keys are never
//! reported as view input. A view that hears none of the raw keys, the
//! text events, and captures none takes [`crate::key_focus`]'s original
//! path with no raw [`KeyEvent`] allocation. Modifier transitions remain
//! tracked across focus moves, including from native inputs.

use std::{cell::RefCell, rc::Rc};

use gtk4::{self as gtk, gdk, glib, prelude::*};
use nsh_core::{
    Bounds, EventKind, EventPayload, KeyEvent, Points, TextInputEvent, ViewKind,
    element::SharedString,
    keymap::{Key as ShortcutKey, Keystroke},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{surface::Surface, view::NshView};

/// A surface's keyboard state.
#[derive(Default)]
pub(crate) struct Keyboard {
    /// The view GTK last gave the focus to (none: a native field, a hosted
    /// widget or nothing).
    focused: RefCell<Option<glib::WeakRef<NshView>>>,
    /// The hardware keycodes held down, and the window each was pressed
    /// in, for auto-repeat detection (GDK does not mark repeats).
    pressed: RefCell<FxHashMap<u32, glib::WeakRef<gtk::Window>>>,
    /// The input method attached to the focused view, while it hears text.
    ime: RefCell<Option<Ime>>,
    /// The preedit the input method last reported (to send each change
    /// once). It runs even when only committed text is listened to.
    preedit: RefCell<String>,
    /// The physical modifier keys held (by keycode) and the modifier each
    /// stands for: a key event's modifier mask comes from these, not from
    /// its pre-key state, so releasing one Shift keeps the other's.
    modifiers: RefCell<FxHashMap<u32, gdk::ModifierType>>,
    /// The surface, for the input method's signals (which outlive the call
    /// that attaches it).
    weak: RefCell<Option<std::rc::Weak<Surface>>>,
    /// The text the input method committed (`gtk_test`): the XTest test
    /// has no engine control to observe the event.
    #[cfg(all(test, feature = "dev"))]
    committed: RefCell<Vec<String>>,
}

/// An input method context of a focused view.
struct Ime {
    context: gtk::IMMulticontext,
    /// The view it is attached to (`client_widget`).
    widget: gtk::Widget,
    /// The keycodes whose press this context filtered: their release is the
    /// context's too. Dropped with the context.
    downs: FxHashSet<u32>,
}

/// Remembers the surface, for the input method's signals. The key
/// controller itself is [`crate::key_focus`]'s.
pub(crate) fn install(surface: &Rc<Surface>) {
    *surface.keyboard.weak.borrow_mut() = Some(Rc::downgrade(surface));
}

/// The app stopped: forgets the focus, the keys held and the input method.
pub(crate) fn reset(surface: &Surface) {
    end_composition(surface);
    detach_ime(surface);
    surface.keyboard.pressed.borrow_mut().clear();
    surface.keyboard.modifiers.borrow_mut().clear();
    surface.keyboard.focused.borrow_mut().take();
    surface.keyboard.preedit.borrow_mut().clear();
}

/// Whether `kind` is heard by `view`.
fn hears(view: &NshView, kind: EventKind) -> bool {
    view.state().events.contains(kind)
}

/// The view is going away: an input method attached to it ends its
/// composition and is detached before the widget is destroyed, and a focus
/// tracked on it is forgotten (GTK reports the next focus separately).
pub(crate) fn removed(surface: &Surface, view: &NshView) {
    if attached_to(surface, view) {
        end_composition(surface);
        detach_ime(surface);
    }
    if owner(surface, view) {
        surface.keyboard.focused.borrow_mut().take();
    }
}

/// Whether `view` is the view the input method is for (the focus the
/// keyboard follows): a view of another, inactive window changing does not
/// repoint it.
fn owner(surface: &Surface, view: &NshView) -> bool {
    surface
        .keyboard
        .focused
        .borrow()
        .as_ref()
        .and_then(|weak| weak.upgrade())
        .is_some_and(|focused| focused == *view)
}

/// Whether the input method attached to `view` is composing (its preedit
/// is not empty): the keys then go to it before the platform's defaults.
pub(crate) fn composing(surface: &Surface, view: &NshView) -> bool {
    attached_to(surface, view) && !surface.keyboard.preedit.borrow().is_empty()
}

/// Whether the surface's input method context is attached to `view`.
fn attached_to(surface: &Surface, view: &NshView) -> bool {
    surface
        .keyboard
        .ime
        .borrow()
        .as_ref()
        .is_some_and(|ime| ime.widget == view.clone().upcast::<gtk::Widget>())
}

/// `MountItem::ViewProps`: where the input method puts its candidate
/// window may have moved, or the view may have stopped hearing text.
pub(crate) fn props_changed(surface: &Surface, view: &NshView) {
    events_changed(surface, view);
}

/// `MountItem::Events`: the focused view started (or stopped) hearing text.
pub(crate) fn events_changed(surface: &Surface, view: &NshView) {
    if !owner(surface, view) {
        return;
    }
    if wants_text(view) && crate::key_focus::is_stop(view) {
        attach_ime(surface, view);
    } else {
        end_composition(surface);
        detach_ime(surface);
    }
}

/// The focus moved (`key_focus::moved`): the input method follows it. The
/// keys held are not released by a focus move (a window-level controller
/// still sees their releases): they stay held.
pub(crate) fn focus_changed(surface: &Surface, window: &gtk::Window, next: Option<&NshView>) {
    // Each window retains its local focus. Setting it while the window is
    // inactive must not end the active window's composition.
    if next.is_some() && !window.is_active() {
        return;
    }
    let tracked = surface
        .keyboard
        .focused
        .borrow()
        .as_ref()
        .and_then(|weak| weak.upgrade());
    match next {
        Some(view) if tracked.as_ref() != Some(view) => {
            // The keys held are not released by a focus move: their
            // repeats and the focused release keep working (a window-level
            // controller still sees the release).
            end_composition(surface);
            detach_ime(surface);
            attach_ime(surface, view);
            *surface.keyboard.focused.borrow_mut() = Some(view.downgrade());
        }
        Some(_) => {}
        None => {
            // This window lost its focus: forget it only when the view
            // tracked was in it (another window's view keeps the input
            // method). Keys held stay too: the window may still have the
            // OS focus and send their releases.
            let in_window = tracked.is_some_and(|view| {
                view.root()
                    .and_then(|root| root.downcast::<gtk::Window>().ok())
                    .as_ref()
                    == Some(window)
            });
            if in_window {
                end_composition(surface);
                detach_ime(surface);
                surface.keyboard.focused.borrow_mut().take();
            }
        }
    }
}

/// `window` became active: its focus widget is the keyboard's focus again.
/// GTK keeps each window's focus widget while it is inactive, and no
/// `focus-widget` change tells of the switch back, so the input method is
/// pointed at that view (and its composition ended for the one it left).
pub(crate) fn window_activated(surface: &Surface, window: &gtk::Window) {
    let Some(focus) = GtkWindowExt::focus(window) else {
        return;
    };
    let Some(view) = focus.downcast::<NshView>().ok() else {
        return;
    };
    if !crate::window::holds(surface, &view) {
        return;
    }
    focus_changed(surface, window, Some(&view));
}

/// Whether `view` hears what an input method context is needed for.
fn wants_text(view: &NshView) -> bool {
    hears(view, EventKind::TextInput) || hears(view, EventKind::CompositionChange)
}

/// `window` lost the OS focus (`is-active` off): its releases may never
/// come, so the keys held there are no press of a key any more (another
/// window's stay). Its input method leaves the view it had and ends a
/// pending composition. No key is released: a blur is no key release.
pub(crate) fn window_deactivated(surface: &Surface, window: &gtk::Window) {
    focus_changed(surface, window, None);
    surface
        .keyboard
        .pressed
        .borrow_mut()
        .retain(|_, held| held.upgrade().as_ref() != Some(window));
    // Their releases may never come: a held modifier is no more held.
    surface.keyboard.modifiers.borrow_mut().clear();
}

/// Whether a key went down before in `window` (an auto-repeat, which GDK
/// does not mark).
pub(crate) fn held(surface: &Surface, window: &gtk::Window, keycode: u32) -> bool {
    let mut pressed = surface.keyboard.pressed.borrow_mut();
    if pressed
        .get(&keycode)
        .is_some_and(|held| held.upgrade().as_ref() == Some(window))
    {
        return true;
    }
    pressed.insert(keycode, window.downgrade());
    false
}

/// The keyboard ancestor of a view: its logical parent when `key_parent`
/// overrides the native one (a portaled `Overlay` is inserted at a native
/// host, not under its logical parent), else its mounted native parent.
/// A logical parent outside the view's window is not an ancestor, so the
/// window boundary stays the one the app sees.
fn keyboard_parent(surface: &Surface, view: &NshView) -> Option<NshView> {
    let Some(id) = ({ view.state().props.key_parent }) else {
        return view.parent_view();
    };
    let parent = surface.mounter.try_borrow().ok()?.views.get(&id).cloned()?;
    (parent.id() != view.id() && parent.root() == view.root()).then_some(parent)
}

/// Whether `view` hears a raw key or text event (text reaches the focused
/// view only, so an ancestor's text listener never counts) or captures
/// keys, or a logical ancestor hears a raw key or captures keys: only then
/// does a key need a [`KeyEvent`], its capture rules and the per-key
/// state. A view that hears none takes [`crate::key_focus`]'s original
/// path, with nothing built.
pub(crate) fn listens(surface: &Surface, view: &NshView) -> bool {
    {
        let state = view.state();
        if state.events.contains(EventKind::TextInput)
            || state.events.contains(EventKind::CompositionChange)
        {
            return true;
        }
    }
    let mut current = Some(view.clone());
    while let Some(view) = current {
        {
            let state = view.state();
            let raw = state.events.contains(EventKind::KeyDown)
                || state.events.contains(EventKind::KeyUp);
            if raw || !state.props.key_capture.is_none() {
                return true;
            }
        }
        current = keyboard_parent(surface, &view);
    }
    false
}

/// Whether `view` hosts native content that owns its own keys and text: a
/// native input, a web view, a native component or a video player. A
/// capture rule around it never intercepts its editing, and the platform
/// does not report its keys as view input.
pub(crate) fn native_owns(view: &NshView) -> bool {
    matches!(
        view.state().kind,
        ViewKind::Input | ViewKind::WebView | ViewKind::Native | ViewKind::Video
    )
}

/// Whether the focused view or one of its enabled ancestors captures `key`
/// (`capture_keys`, `ViewProps::key_capture`): a captured key notifies and
/// runs no default (Tab's traversal, Return and Space pressing, a context
/// menu, scrolling).
///
/// The rule is matched against the canonical shortcut `Keystroke` of the
/// event (`event`'s modifiers, and the keyval that `keycode` types without
/// Shift on the event's layout group), the vocabulary `nsh/keymap` already
/// parses. The raw logical event key is never parsed as a shortcut.
pub(crate) fn captures(
    surface: &Surface,
    view: &NshView,
    event: &KeyEvent,
    keycode: u32,
    state: gdk::ModifierType,
    group: i32,
) -> bool {
    let stroke = shortcut_stroke(view, event, keycode, state, group);
    let mut current = Some(view.clone());
    while let Some(view) = current {
        let state = view.state();
        if !state.props.accessibility.state.disabled
            && state.props.key_capture.captures_key(stroke.as_ref())
        {
            return true;
        }
        drop(state);
        current = keyboard_parent(surface, &view);
    }
    false
}

/// Reports a key transition to the focused view (`KeyDown` / `KeyUp`): the
/// engine bubbles it to the view's ancestors, whatever they (or a
/// capture) do with it.
pub(crate) fn notify(surface: &Surface, view: &NshView, kind: EventKind, event: KeyEvent) {
    surface.event(view.id(), kind, EventPayload::Key(event));
}

/// The keyboard layout group of a GDK key event (`0` when there is no
/// event or it is not a key event): `gdk::KeyEvent::layout`.
pub(crate) fn layout(event: Option<&gdk::Event>) -> i32 {
    event
        .and_then(|event| event.downcast_ref::<gdk::KeyEvent>())
        .map_or(0, |event| event.layout() as i32)
}

/// Keeps GDK's private modifier bits (including X11 NumLock), which the
/// gtk-rs flags conversion discards before invoking key callbacks.
pub(crate) fn modifier_state(
    event: Option<&gdk::Event>,
    fallback: gdk::ModifierType,
) -> gdk::ModifierType {
    use glib::translate::ToGlibPtr;

    event.map_or(fallback, |event| {
        // SAFETY: the borrowed event owns this valid GdkEvent pointer for
        // the duration of the call; the getter returns a value, not storage.
        let bits = unsafe { gdk::ffi::gdk_event_get_modifier_state(event.to_glib_none().0) };
        gdk::ModifierType::from_bits_retain(bits)
    })
}

/// The modifier a modifier key stands for.
fn modifier_mask(keyval: gdk::Key) -> Option<gdk::ModifierType> {
    use gdk::ModifierType as M;
    Some(match keyval {
        gdk::Key::Control_L | gdk::Key::Control_R => M::CONTROL_MASK,
        gdk::Key::Shift_L | gdk::Key::Shift_R => M::SHIFT_MASK,
        gdk::Key::Alt_L | gdk::Key::Alt_R | gdk::Key::Meta_L | gdk::Key::Meta_R => M::ALT_MASK,
        gdk::Key::Super_L | gdk::Key::Super_R => M::SUPER_MASK,
        _ => return None,
    })
}

/// Records a modifier key's press or release. Its mask then follows the
/// physical keys held (either Shift, Control, Alt, Super), not the event's
/// pre-key state, which GDK also cannot give after the transition.
pub(crate) fn track_modifiers(surface: &Surface, keyval: gdk::Key, keycode: u32, down: bool) {
    let Some(mask) = modifier_mask(keyval) else {
        return;
    };
    let mut held = surface.keyboard.modifiers.borrow_mut();
    if down {
        held.insert(keycode, mask);
    } else {
        held.remove(&keycode);
    }
}

/// A key event's modifiers: GDK's state (the one before the key) with a
/// modifier key's own mask taken from the physical keys held, so one Shift
/// releasing does not clear the other's. Every other mask is GDK's, which
/// this key does not change.
fn effective_modifiers(
    surface: &Surface,
    state: gdk::ModifierType,
    keyval: gdk::Key,
) -> gdk::ModifierType {
    let Some(mask) = modifier_mask(keyval) else {
        return state;
    };
    let mut state = state;
    let held = surface
        .keyboard
        .modifiers
        .borrow()
        .values()
        .any(|held| held.contains(mask));
    state.set(mask, held);
    state
}

/// A real key came up: clear its hold in `window` (whatever view heard its
/// press), report `KeyUp` to the view focused now and offer the release to
/// the input method that took the press (never to another view's context:
/// the focus may have moved since). A held modifier is tracked even when
/// nothing has the focus or the key went to a native widget: the next raw
/// event must carry it.
pub(crate) fn released(
    surface: &Surface,
    window: Option<&gtk::Window>,
    view: Option<&NshView>,
    keyval: gdk::Key,
    keycode: u32,
    state: gdk::ModifierType,
    event: Option<&gdk::Event>,
) {
    track_modifiers(surface, keyval, keycode, false);
    // Only the window the key was pressed in loses its hold (another
    // window's press of the same code is its own); a release reported
    // without a window ends any hold.
    let mut pressed = surface.keyboard.pressed.borrow_mut();
    if let Some(window) = window {
        if pressed
            .get(&keycode)
            .is_some_and(|held| held.upgrade().as_ref() == Some(window))
        {
            pressed.remove(&keycode);
        }
    } else {
        pressed.remove(&keycode);
    }
    drop(pressed);
    let Some(view) = view else {
        return;
    };
    if !listens(surface, view) || native_owns(view) {
        return;
    }
    // The view focused now hears the release, even when the press went to
    // another view; its capture decision is the mounted policy now.
    let mut key = key_event(surface, keyval, keycode, state, KeyPhase::Up);
    let captured = captures(surface, view, &key, keycode, state, layout(event));
    key.default_prevented = captured;
    notify(surface, view, EventKind::KeyUp, key);
    // GDK's input methods see the release too (`filter_keypress` handles
    // both), but only the context that filtered its press: the current one
    // may be another view's (the focus moved) or may only have seen the
    // press, not its release. A release the mounted capture rule now takes
    // commits no text, as its press would: the keycode is still consumed
    // from the per-key state, but a compose sequence it would end on GTK's
    // simple input method (releasing Control or Shift) never reaches it.
    let saw_press = surface
        .keyboard
        .ime
        .borrow_mut()
        .as_mut()
        .is_some_and(|ime| ime.downs.remove(&keycode));
    if saw_press
        && !captured
        && let Some(event) = event
        && let Some(ime) = surface.keyboard.ime.borrow().as_ref()
    {
        ime.context.filter_keypress(event);
    }
}

/// The transition being reported; releases cannot be auto-repeats.
pub(crate) enum KeyPhase {
    Down { repeat: bool },
    Up,
}

/// The key event of a key press or release. `key` is the actual logical
/// key GDK reports (`"A"` with Shift, `"é"` on a French layout), named by
/// the DOM's spelling for common named keys. `code` is the DOM
/// `KeyboardEvent.code` of the hardware `keycode`: the key's physical
/// place, never the character the layout types there, and `""` when the
/// keycode names no key (GDK reports none, or a device's own key).
pub(crate) fn key_event(
    surface: &Surface,
    keyval: gdk::Key,
    keycode: u32,
    state: gdk::ModifierType,
    phase: KeyPhase,
) -> KeyEvent {
    let repeat = match phase {
        KeyPhase::Down { repeat } => repeat,
        KeyPhase::Up => false,
    };
    let modifiers = crate::input::modifiers(effective_modifiers(surface, state, keyval));
    let mut event = KeyEvent::default();
    event.key = logical_key(keyval);
    event.code = SharedString::new_static(dom_code(keycode));
    event.modifiers = modifiers;
    event.repeat = repeat;
    event
}

/// Offers the key to the focused view's input method context; true when it
/// took it (its commit or preedit arrives through the signals). Also
/// attaches the context when the view started hearing text after it got
/// the focus.
pub(crate) fn text(
    surface: &Surface,
    view: &NshView,
    keycode: u32,
    event: Option<&gdk::Event>,
) -> bool {
    if !wants_text(view) {
        return false;
    }
    if !attached_to(surface, view) {
        // The key went to `view`: a context still bound to the view it
        // replaced (the focus moved between windows without a
        // `focus-widget` change) would take its text and put its candidates
        // there. End what it composed, then bind it here.
        end_composition(surface);
        *surface.keyboard.focused.borrow_mut() = Some(view.downgrade());
        attach_ime(surface, view);
    }
    let Some(event) = event else {
        return false;
    };
    // The context filters this press (its commits and preedits arrive
    // through the signals); its release belongs to it too.
    let handled = match surface.keyboard.ime.borrow().as_ref() {
        Some(ime) => ime.context.filter_keypress(event),
        None => return false,
    };
    if let Some(ime) = surface.keyboard.ime.borrow_mut().as_mut() {
        ime.downs.insert(keycode);
    }
    handled
}

/// Attaches an input method context to `view` when it hears text (or
/// replaces the one it has).
fn attach_ime(surface: &Surface, view: &NshView) {
    if !wants_text(view) || !crate::key_focus::is_stop(view) || native_owns(view) {
        detach_ime(surface);
        return;
    }
    let same = surface
        .keyboard
        .ime
        .borrow()
        .as_ref()
        .is_some_and(|ime| ime.widget == view.clone().upcast::<gtk::Widget>());
    if same {
        update_ime_area(surface, view);
        return;
    }
    detach_ime(surface);
    let context = gtk::IMMulticontext::new();
    context.set_client_widget(Some(view));
    // The signals outlive this call: they find the surface and the view
    // through weak references.
    let weak_surface = match surface.keyboard.weak.borrow().clone() {
        Some(weak) => weak,
        None => return,
    };
    let weak = view.downgrade();
    let signals = weak_surface.clone();
    context.connect_commit(move |context, text| {
        let (Some(surface), Some(view)) = (signals.upgrade(), weak.upgrade()) else {
            return;
        };
        if !crate::key_focus::is_stop(&view)
            || !surface
                .keyboard
                .ime
                .borrow()
                .as_ref()
                .is_some_and(|ime| ime.context == *context)
        {
            return;
        }
        let had_preedit = !surface.keyboard.preedit.borrow().is_empty();
        surface.keyboard.preedit.borrow_mut().clear();
        #[cfg(all(test, feature = "dev"))]
        surface
            .keyboard
            .committed
            .borrow_mut()
            .push(text.to_owned());
        if hears(&view, EventKind::TextInput) {
            surface.event(
                view.id(),
                EventKind::TextInput,
                EventPayload::TextInput(TextInputEvent::new(text.to_owned())),
            );
        }
        if had_preedit && hears(&view, EventKind::CompositionChange) {
            surface.event(
                view.id(),
                EventKind::CompositionChange,
                composition(false, ""),
            );
        }
    });
    let weak = view.downgrade();
    let signals = weak_surface;
    context.connect_preedit_changed(move |context| {
        let (Some(surface), Some(view)) = (signals.upgrade(), weak.upgrade()) else {
            return;
        };
        if !crate::key_focus::is_stop(&view)
            || !surface
                .keyboard
                .ime
                .borrow()
                .as_ref()
                .is_some_and(|ime| ime.context == *context)
        {
            return;
        }
        let (text, _, _) = context.preedit_string();
        let text = text.to_string();
        if *surface.keyboard.preedit.borrow() == text {
            return;
        }
        *surface.keyboard.preedit.borrow_mut() = text;
        // Preedit is native input state even when the application only
        // subscribes to committed text. Only the notification is optional.
        if hears(&view, EventKind::CompositionChange) {
            let text = surface.keyboard.preedit.borrow();
            surface.event(
                view.id(),
                EventKind::CompositionChange,
                composition(!text.is_empty(), &text),
            );
        }
    });
    *surface.keyboard.ime.borrow_mut() = Some(Ime {
        context: context.clone(),
        widget: view.clone().upcast(),
        downs: FxHashSet::default(),
    });
    context.focus_in();
    update_ime_area_with(view, &context);
}

/// Removes the focused view's input method context.
fn detach_ime(surface: &Surface) {
    let context = surface
        .keyboard
        .ime
        .borrow()
        .as_ref()
        .map(|ime| ime.context.clone());
    if let Some(context) = context {
        // An eligible view may receive a final commit from focus_out.
        // Keep that context current until its synchronous signals finish.
        context.focus_out();
        context.set_client_widget(None::<&gtk::Widget>);
        surface.keyboard.ime.borrow_mut().take();
    }
    surface.keyboard.preedit.borrow_mut().clear();
}

/// Tells the input method where the focused view's candidate window goes
/// (`imeCursorArea`).
fn update_ime_area(surface: &Surface, view: &NshView) {
    if let Some(ime) = surface.keyboard.ime.borrow().as_ref() {
        update_ime_area_with(view, &ime.context);
    }
}

fn update_ime_area_with(view: &NshView, context: &gtk::IMMulticontext) {
    let rectangle = ime_rectangle(view.state().props.ime_cursor_area);
    context.set_cursor_location(&rectangle);
}

/// Where the candidate window goes: `ime_cursor_area` as it is, in the
/// view's own coordinates, or the default at its origin: a point-sized
/// rectangle there, never a stale one from before the area was cleared.
///
/// GTK's `IMContext::set_cursor_location` is relative to the client widget,
/// which is the view, so a nested or transformed view's rectangle is not
/// translated to root coordinates here: the input method computes the
/// window position from the client widget's own place.
pub(crate) fn ime_rectangle(area: Option<Bounds<Points>>) -> gdk::Rectangle {
    let Some(area) = area else {
        return gdk::Rectangle::new(0, 0, 1, 1);
    };
    gdk::Rectangle::new(
        area.origin.x.0.round() as i32,
        area.origin.y.0.round() as i32,
        area.size.width.0.round().max(1.0) as i32,
        area.size.height.0.round().max(1.0) as i32,
    )
}

/// The preedit the attached input method currently reports (`gtk_test`);
/// `None` when it composes nothing.
#[cfg(all(test, feature = "dev"))]
pub(crate) fn preedit(surface: &Surface) -> Option<String> {
    let preedit = surface.keyboard.preedit.borrow();
    (!preedit.is_empty()).then(|| (*preedit).clone())
}

/// The text the input method committed (`gtk_test`).
#[cfg(all(test, feature = "dev"))]
pub(crate) fn committed(surface: &Surface) -> Vec<String> {
    surface.keyboard.committed.borrow().clone()
}

/// The view the attached input method serves (`gtk_test`).
#[cfg(test)]
pub(crate) fn ime_view(surface: &Surface) -> Option<nsh_core::NodeId> {
    surface
        .keyboard
        .ime
        .borrow()
        .as_ref()
        .and_then(|ime| ime.widget.clone().downcast::<NshView>().ok())
        .map(|view| view.id())
}

/// A composition left pending when the focus goes: the view that was
/// composing hears it end (its `on_composition_change`), and the preedit is
/// cleared.
fn end_composition(surface: &Surface) {
    if surface.keyboard.preedit.borrow().is_empty() {
        return;
    }
    surface.keyboard.preedit.borrow_mut().clear();
    let view = surface
        .keyboard
        .focused
        .borrow()
        .as_ref()
        .and_then(|weak| weak.upgrade());
    if let Some(view) = view
        && hears(&view, EventKind::CompositionChange)
    {
        surface.event(
            view.id(),
            EventKind::CompositionChange,
            composition(false, ""),
        );
    }
}

/// A composition payload (`EventKind::CompositionChange`).
fn composition(composing: bool, text: &str) -> EventPayload {
    EventPayload::Map(vec![
        ("composing", nsh_core::PayloadValue::Bool(composing)),
        ("text", nsh_core::PayloadValue::Str(text.to_owned())),
    ])
}

/// The DOM `KeyboardEvent.code` of GDK's hardware keycode: where the key
/// is, by the XKB keycode (the X11 keycode; the evdev code plus 8 on
/// Wayland), never what the layout types on it. The names are the UI
/// Events code values, as Chromium maps native keycodes. `""` for a key
/// this backend has no code for; no code is ever derived from the keyval.
fn dom_code(keycode: u32) -> &'static str {
    match keycode {
        0x0009 => "Escape",
        0x000A => "Digit1",
        0x000B => "Digit2",
        0x000C => "Digit3",
        0x000D => "Digit4",
        0x000E => "Digit5",
        0x000F => "Digit6",
        0x0010 => "Digit7",
        0x0011 => "Digit8",
        0x0012 => "Digit9",
        0x0013 => "Digit0",
        0x0014 => "Minus",
        0x0015 => "Equal",
        0x0016 => "Backspace",
        0x0017 => "Tab",
        0x0018 => "KeyQ",
        0x0019 => "KeyW",
        0x001A => "KeyE",
        0x001B => "KeyR",
        0x001C => "KeyT",
        0x001D => "KeyY",
        0x001E => "KeyU",
        0x001F => "KeyI",
        0x0020 => "KeyO",
        0x0021 => "KeyP",
        0x0022 => "BracketLeft",
        0x0023 => "BracketRight",
        0x0024 => "Enter",
        0x0025 => "ControlLeft",
        0x0026 => "KeyA",
        0x0027 => "KeyS",
        0x0028 => "KeyD",
        0x0029 => "KeyF",
        0x002A => "KeyG",
        0x002B => "KeyH",
        0x002C => "KeyJ",
        0x002D => "KeyK",
        0x002E => "KeyL",
        0x002F => "Semicolon",
        0x0030 => "Quote",
        0x0031 => "Backquote",
        0x0032 => "ShiftLeft",
        0x0033 => "Backslash",
        0x0034 => "KeyZ",
        0x0035 => "KeyX",
        0x0036 => "KeyC",
        0x0037 => "KeyV",
        0x0038 => "KeyB",
        0x0039 => "KeyN",
        0x003A => "KeyM",
        0x003B => "Comma",
        0x003C => "Period",
        0x003D => "Slash",
        0x003E => "ShiftRight",
        0x003F => "NumpadMultiply",
        0x0040 => "AltLeft",
        0x0041 => "Space",
        0x0042 => "CapsLock",
        0x0043 => "F1",
        0x0044 => "F2",
        0x0045 => "F3",
        0x0046 => "F4",
        0x0047 => "F5",
        0x0048 => "F6",
        0x0049 => "F7",
        0x004A => "F8",
        0x004B => "F9",
        0x004C => "F10",
        0x004D => "NumLock",
        0x004E => "ScrollLock",
        0x004F => "Numpad7",
        0x0050 => "Numpad8",
        0x0051 => "Numpad9",
        0x0052 => "NumpadSubtract",
        0x0053 => "Numpad4",
        0x0054 => "Numpad5",
        0x0055 => "Numpad6",
        0x0056 => "NumpadAdd",
        0x0057 => "Numpad1",
        0x0058 => "Numpad2",
        0x0059 => "Numpad3",
        0x005A => "Numpad0",
        0x005B => "NumpadDecimal",
        0x005D => "Lang5",
        0x005E => "IntlBackslash",
        0x005F => "F11",
        0x0060 => "F12",
        0x0061 => "IntlRo",
        0x0062 => "Lang3",
        0x0063 => "Lang4",
        0x0064 => "Convert",
        0x0065 => "KanaMode",
        0x0066 => "NonConvert",
        0x0068 => "NumpadEnter",
        0x0069 => "ControlRight",
        0x006A => "NumpadDivide",
        0x006B => "PrintScreen",
        0x006C => "AltRight",
        0x006E => "Home",
        0x006F => "ArrowUp",
        0x0070 => "PageUp",
        0x0071 => "ArrowLeft",
        0x0072 => "ArrowRight",
        0x0073 => "End",
        0x0074 => "ArrowDown",
        0x0075 => "PageDown",
        0x0076 => "Insert",
        0x0077 => "Delete",
        0x0079 => "AudioVolumeMute",
        0x007A => "AudioVolumeDown",
        0x007B => "AudioVolumeUp",
        0x007C => "Power",
        0x007D => "NumpadEqual",
        0x007F => "Pause",
        0x0080 => "ShowAllWindows",
        0x0081 => "NumpadComma",
        0x0082 => "Lang1",
        0x0083 => "Lang2",
        0x0084 => "IntlYen",
        0x0085 => "MetaLeft",
        0x0086 => "MetaRight",
        0x0087 => "ContextMenu",
        0x0088 => "BrowserStop",
        0x0089 => "Again",
        0x008B => "Undo",
        0x008C => "Select",
        0x008D => "Copy",
        0x008E => "Open",
        0x008F => "Paste",
        0x0090 => "Find",
        0x0091 => "Cut",
        0x0092 => "Help",
        0x0094 => "LaunchApp2",
        0x0096 => "Sleep",
        0x0097 => "WakeUp",
        0x0098 => "LaunchApp1",
        0x00A3 => "LaunchMail",
        0x00A4 => "BrowserFavorites",
        0x00A6 => "BrowserBack",
        0x00A7 => "BrowserForward",
        0x00A9 => "Eject",
        0x00AB => "MediaTrackNext",
        0x00AC => "MediaPlayPause",
        0x00AD => "MediaTrackPrevious",
        0x00AE => "MediaStop",
        0x00AF => "MediaRecord",
        0x00B0 => "MediaRewind",
        0x00B3 => "MediaSelect",
        0x00B4 => "BrowserHome",
        0x00B5 => "BrowserRefresh",
        0x00BB => "NumpadParenLeft",
        0x00BC => "NumpadParenRight",
        0x00BF => "F13",
        0x00C0 => "F14",
        0x00C1 => "F15",
        0x00C2 => "F16",
        0x00C3 => "F17",
        0x00C4 => "F18",
        0x00C5 => "F19",
        0x00C6 => "F20",
        0x00C7 => "F21",
        0x00C8 => "F22",
        0x00C9 => "F23",
        0x00CA => "F24",
        0x00D1 => "MediaPause",
        0x00D7 => "MediaPlay",
        0x00D8 => "MediaFastForward",
        0x00E1 => "BrowserSearch",
        0x00E8 => "BrightnessDown",
        0x00E9 => "BrightnessUp",
        0x00EB => "DisplayToggleIntExt",
        0x00EF => "MailSend",
        0x00F0 => "MailReply",
        0x00F1 => "MailForward",
        0x017C => "ZoomToggle",
        0x01D8 => "Fn",
        0x024B => "LaunchControlPanel",
        0x024C => "SelectTask",
        0x024D => "LaunchScreenSaver",
        0x024F => "LaunchAssistant",
        0x0250 => "KeyboardLayoutSelect",
        _ => "",
    }
}

/// Whether a key is held down (`gtk_test`).
#[cfg(test)]
pub(crate) fn held_key(surface: &Surface, keycode: u32) -> bool {
    surface.keyboard.pressed.borrow().contains_key(&keycode)
}

/// The canonical shortcut `Keystroke` of a key event: its modifiers, and
/// the keyval `keycode` types with Shift and CapsLock removed on the
/// event's layout group, as `nsh/keymap` names keys. `None` when the
/// layout names no shortcut key there (a dead key, a non-ASCII character).
pub(crate) fn shortcut_stroke(
    view: &NshView,
    event: &KeyEvent,
    keycode: u32,
    state: gdk::ModifierType,
    group: i32,
) -> Option<Keystroke> {
    let display = view.display();
    // Remove character case and Shift, not NumLock or layout-level
    // selectors: an unshifted numeric keypad key is still a digit with
    // NumLock on.
    let unshifted = state.difference(gdk::ModifierType::SHIFT_MASK | gdk::ModifierType::LOCK_MASK);
    let base = shortcut_base(&display, keycode, unshifted, group)?;
    let key = shortcut_key(base)?;
    Some(Keystroke {
        key,
        modifiers: event.modifiers,
    })
}

/// The keyval a shortcut names for `keycode`: the event layout group's
/// key, or, under a non-Latin group, the first other group's key that the
/// shortcut vocabulary names, as GTK's shortcut triggers match a layout's
/// Latin group. Every keyval is the display's own; nothing is invented.
fn shortcut_base(
    display: &gdk::Display,
    keycode: u32,
    state: gdk::ModifierType,
    group: i32,
) -> Option<gdk::Key> {
    let translated = |group| {
        display
            .translate_key(keycode, state, group)
            .map(|(key, ..)| key)
    };
    let base = translated(group)?;
    if shortcut_key(base).is_some_and(|key| !matches!(key, ShortcutKey::Char(_)))
        || base.to_unicode().is_some_and(|c| c.is_ascii_graphic())
    {
        return Some(base);
    }
    // Ask GDK which groups exist instead of translating guessed indices:
    // X11 rejects group numbers outside its keymap.
    let mut previous = group;
    for (mapping, _) in display.map_keycode(keycode).into_iter().flatten() {
        let other = mapping.group();
        if other == group || other == previous {
            continue;
        }
        previous = other;
        if let Some(key) = translated(other)
            && key.to_unicode().is_some_and(|c| c.is_ascii_graphic())
        {
            return Some(key);
        }
    }
    Some(base)
}

/// The `nsh/keymap` key of a GDK keyval, or `None` when the keymap's
/// vocabulary does not name it. Letters are the keymap's canonical lower
/// case.
fn shortcut_key(keyval: gdk::Key) -> Option<ShortcutKey> {
    use gdk::Key as K;
    Some(match keyval {
        K::Return | K::KP_Enter | K::ISO_Enter => ShortcutKey::Enter,
        K::Escape => ShortcutKey::Escape,
        K::Tab | K::KP_Tab | K::ISO_Left_Tab => ShortcutKey::Tab,
        K::BackSpace => ShortcutKey::Backspace,
        K::Delete | K::KP_Delete => ShortcutKey::Delete,
        K::Insert | K::KP_Insert => ShortcutKey::Insert,
        K::Up | K::KP_Up => ShortcutKey::Up,
        K::Down | K::KP_Down => ShortcutKey::Down,
        K::Left | K::KP_Left => ShortcutKey::Left,
        K::Right | K::KP_Right => ShortcutKey::Right,
        K::Home | K::KP_Home => ShortcutKey::Home,
        K::End | K::KP_End => ShortcutKey::End,
        K::Page_Up | K::KP_Page_Up => ShortcutKey::PageUp,
        K::Page_Down | K::KP_Page_Down => ShortcutKey::PageDown,
        K::space | K::KP_Space => ShortcutKey::Char(' '),
        K::F1 => ShortcutKey::F(1),
        K::F2 => ShortcutKey::F(2),
        K::F3 => ShortcutKey::F(3),
        K::F4 => ShortcutKey::F(4),
        K::F5 => ShortcutKey::F(5),
        K::F6 => ShortcutKey::F(6),
        K::F7 => ShortcutKey::F(7),
        K::F8 => ShortcutKey::F(8),
        K::F9 => ShortcutKey::F(9),
        K::F10 => ShortcutKey::F(10),
        K::F11 => ShortcutKey::F(11),
        K::F12 => ShortcutKey::F(12),
        K::F13 => ShortcutKey::F(13),
        K::F14 => ShortcutKey::F(14),
        K::F15 => ShortcutKey::F(15),
        K::F16 => ShortcutKey::F(16),
        K::F17 => ShortcutKey::F(17),
        K::F18 => ShortcutKey::F(18),
        K::F19 => ShortcutKey::F(19),
        K::F20 => ShortcutKey::F(20),
        K::F21 => ShortcutKey::F(21),
        K::F22 => ShortcutKey::F(22),
        K::F23 => ShortcutKey::F(23),
        K::F24 => ShortcutKey::F(24),
        _ => {
            let c = keyval.to_unicode().filter(char::is_ascii_graphic)?;
            ShortcutKey::Char(c.to_ascii_lowercase())
        }
    })
}

/// The logical key of a GDK keyval: its actual typed character (Shift's
/// case and the layout's character, not a fabricated US one), a DOM name
/// for the common named keys, and GDK's name for the rest. A keyval with
/// no name is `Unidentified`.
fn logical_key(keyval: gdk::Key) -> SharedString {
    use gdk::Key as K;
    let named = match keyval {
        K::VoidSymbol => "Unidentified",
        K::Return | K::KP_Enter | K::ISO_Enter => "Enter",
        K::Escape => "Escape",
        K::Tab | K::KP_Tab | K::ISO_Left_Tab => "Tab",
        K::BackSpace => "Backspace",
        K::space | K::KP_Space => " ",
        K::Delete | K::KP_Delete => "Delete",
        K::Insert | K::KP_Insert => "Insert",
        K::Home | K::KP_Home => "Home",
        K::End | K::KP_End => "End",
        K::Page_Up | K::KP_Page_Up => "PageUp",
        K::Page_Down | K::KP_Page_Down => "PageDown",
        K::Left | K::KP_Left => "ArrowLeft",
        K::Right | K::KP_Right => "ArrowRight",
        K::Up | K::KP_Up => "ArrowUp",
        K::Down | K::KP_Down => "ArrowDown",
        K::Shift_L | K::Shift_R => "Shift",
        K::Control_L | K::Control_R => "Control",
        K::Alt_L | K::Alt_R | K::Meta_L | K::Meta_R => "Alt",
        K::Super_L | K::Super_R => "Meta",
        K::Caps_Lock => "CapsLock",
        K::Num_Lock => "NumLock",
        K::Scroll_Lock => "ScrollLock",
        K::Print => "PrintScreen",
        K::Menu => "ContextMenu",
        K::ISO_Level3_Shift => "AltGraph",
        K::Multi_key => "Compose",
        K::F1 => "F1",
        K::F2 => "F2",
        K::F3 => "F3",
        K::F4 => "F4",
        K::F5 => "F5",
        K::F6 => "F6",
        K::F7 => "F7",
        K::F8 => "F8",
        K::F9 => "F9",
        K::F10 => "F10",
        K::F11 => "F11",
        K::F12 => "F12",
        K::F13 => "F13",
        K::F14 => "F14",
        K::F15 => "F15",
        K::F16 => "F16",
        K::F17 => "F17",
        K::F18 => "F18",
        K::F19 => "F19",
        K::F20 => "F20",
        K::F21 => "F21",
        K::F22 => "F22",
        K::F23 => "F23",
        K::F24 => "F24",
        _ => {
            if let Some(c) = keyval.to_unicode().filter(|c| !c.is_control()) {
                return SharedString::from(c.to_string());
            }
            return match keyval.name() {
                Some(name) if name.as_str().starts_with("dead_") => SharedString::from("Dead"),
                Some(name) => SharedString::from(name.to_string()),
                None => SharedString::from("Unidentified"),
            };
        }
    };
    SharedString::from(named)
}

#[cfg(test)]
mod tests {
    use nsh_core::{MountItem, keymap::Modifiers};

    use super::*;
    use crate::gtk_test::{apply, run, surface, view};

    /// The XTest pair: Shift_L and Shift_R down, Shift_L up (the right one
    /// still holds Shift, so the key up says so), the key typed meanwhile
    /// still has Shift, then Shift_R up (Shift is gone).
    #[test]
    fn both_modifier_keys_own_the_mask_they_share() {
        use gdk::ModifierType as M;

        run(|| {
            let surface = surface();
            apply(
                &surface,
                &[MountItem::Create {
                    id: 2,
                    kind: ViewKind::View,
                }],
            );
            let event = |keyval, keycode, state, repeat, down| {
                let phase = if down {
                    super::KeyPhase::Down { repeat }
                } else {
                    super::KeyPhase::Up
                };
                super::key_event(&surface, keyval, keycode, state, phase)
            };
            super::track_modifiers(&surface, gdk::Key::Shift_L, 50, true);
            assert!(
                event(gdk::Key::Shift_L, 50, M::empty(), false, true)
                    .modifiers
                    .shift
            );
            super::track_modifiers(&surface, gdk::Key::Shift_R, 62, true);
            assert!(
                event(gdk::Key::Shift_R, 62, M::SHIFT_MASK, false, true)
                    .modifiers
                    .shift
            );
            // The left one goes: the right one still holds Shift, and its
            // key up says so.
            super::track_modifiers(&surface, gdk::Key::Shift_L, 50, false);
            assert!(
                event(gdk::Key::Shift_L, 50, M::SHIFT_MASK, false, false)
                    .modifiers
                    .shift
            );
            // The key typed meanwhile still has Shift (`B`).
            let typed = event(gdk::Key::B, 56, M::SHIFT_MASK, false, true);
            assert!(typed.modifiers.shift);
            assert_eq!(typed.key, "B", "the actual shifted character");
            assert_eq!(
                typed.code, "KeyB",
                "the physical B key, whatever case it types"
            );
            // The right one goes too: Shift is gone.
            super::track_modifiers(&surface, gdk::Key::Shift_R, 62, false);
            assert!(
                !event(gdk::Key::Shift_R, 62, M::SHIFT_MASK, false, false)
                    .modifiers
                    .shift
            );
            // Control has its own keys and mask.
            super::track_modifiers(&surface, gdk::Key::Control_L, 37, true);
            assert!(
                event(gdk::Key::Control_L, 37, M::empty(), false, true)
                    .modifiers
                    .ctrl
            );
            super::track_modifiers(&surface, gdk::Key::Control_L, 37, false);
            assert!(
                !event(gdk::Key::Control_L, 37, M::CONTROL_MASK, false, false)
                    .modifiers
                    .ctrl
            );
            // A non-modifier key keeps the masks GDK gave it.
            let held = event(
                gdk::Key::a,
                38,
                M::SHIFT_MASK | M::CONTROL_MASK,
                false,
                true,
            );
            assert!(held.modifiers.shift && held.modifiers.ctrl);
        });
    }

    /// GTK's simple input method commits the Unicode character a
    /// Ctrl-Shift-U hex sequence names when Control or Shift is released.
    /// A release the mounted capture rule takes must consume its per-key
    /// state but never reach the input method, while an uncaptured release
    /// on the same composition still commits. Requires
    /// `GTK_IM_MODULE=gtk-im-context-simple` on an X11 display.
    #[cfg(feature = "dev")]
    mod unicode_compose {
        use nsh_core::{EventKind, EventMask, KeyCapture, ROOT_ID, ViewKind, ViewProps};

        use super::*;
        use crate::gtk_test::{frame, show, wait_until};

        /// A focusable view that hears text at `(0, y)`.
        fn text_view(surface: &Rc<Surface>, id: nsh_core::NodeId, y: f32) {
            apply(
                surface,
                &[
                    MountItem::Create {
                        id,
                        kind: ViewKind::View,
                    },
                    MountItem::Insert {
                        parent: ROOT_ID,
                        child: id,
                        before: None,
                    },
                    MountItem::Frame {
                        id,
                        frame: frame(0.0, y, 100.0, 40.0),
                    },
                ],
            );
            props(surface, id, |props| props.focusable = Some(true));
            let mut events = EventMask::default();
            events.set(EventKind::TextInput, true);
            apply(surface, &[MountItem::Events { id, events }]);
        }

        /// `props` of `id`, changed as `edit` says.
        fn props(surface: &Rc<Surface>, id: nsh_core::NodeId, edit: impl FnOnce(&mut ViewProps)) {
            let mut props = view(surface, id).state().props.clone();
            edit(&mut props);
            apply(surface, &[MountItem::ViewProps { id, props }]);
        }

        /// The display's keycode for `key`.
        fn keycode(key: gdk::Key) -> u32 {
            gdk::Display::default()
                .and_then(|display| display.map_keyval(key))
                .and_then(|keys| keys.first().map(|key| key.keycode()))
                .expect("a keycode")
        }

        #[test]
        fn a_captured_release_does_not_commit_a_unicode_composition() {
            run(|| {
                if gdk::Display::default()
                    .is_none_or(|display| display.type_().name() != "GdkX11Display")
                {
                    return;
                }
                let surface = surface();
                text_view(&surface, 2, 0.0);
                let first = view(&surface, 2);
                let window = show(&surface);
                assert!(wait_until(|| first.is_mapped()));
                let (shift, control) = (keycode(gdk::Key::Shift_L), keycode(gdk::Key::Control_L));
                crate::xtest::Xtest::with(|x| {
                    x.focus(&window);
                    assert!(wait_until(|| window.is_active()));
                    crate::key_focus::focus(&first);
                    assert!(wait_until(|| first.is_focus()));
                    assert!(wait_until(|| ime_view(&surface) == Some(2)));
                    // Ctrl-Shift-U then 1 2 3 composes U+0123.
                    assert!(x.key(crate::xtest::CONTROL_L, true));
                    assert!(x.key(crate::xtest::SHIFT_L, true));
                    assert!(x.key(crate::xtest::char_keysym('u'), true));
                    assert!(x.key(crate::xtest::char_keysym('u'), false));
                    for c in ['1', '2', '3'] {
                        assert!(x.key(crate::xtest::char_keysym(c), true));
                        assert!(x.key(crate::xtest::char_keysym(c), false));
                    }
                    assert!(
                        wait_until(|| preedit(&surface).as_deref() == Some("u123")),
                        "GTK's simple input method composes the hex sequence"
                    );
                    assert!(
                        held_key(&surface, shift) && held_key(&surface, control),
                        "the modifiers are held"
                    );
                    // Capture starts between the press and release.
                    props(&surface, 2, |props| props.key_capture = KeyCapture::All);
                    assert!(x.key(crate::xtest::SHIFT_L, false));
                    assert!(
                        wait_until(|| !held_key(&surface, shift)),
                        "the captured release is processed"
                    );
                    assert_eq!(
                        committed(&surface),
                        Vec::<String>::new(),
                        "a captured release commits no text"
                    );
                    assert_eq!(
                        preedit(&surface).as_deref(),
                        Some("u123"),
                        "the input method never saw the captured release"
                    );
                    // The same release without the capture still commits:
                    // the composition was live, not cancelled by the first
                    // release.
                    props(&surface, 2, |props| props.key_capture = KeyCapture::None);
                    assert!(x.key(crate::xtest::CONTROL_L, false));
                    assert!(
                        wait_until(|| !held_key(&surface, control)),
                        "the uncaptured release is processed"
                    );
                    assert!(
                        wait_until(|| committed(&surface).last().map(String::as_str) == Some("ģ")),
                        "an uncaptured release commits the composed character"
                    );
                    assert_eq!(preedit(&surface), None);
                })
                .expect("real XTest is required");
                window.destroy();
            });
        }
    }

    #[test]
    fn numlock_preserves_numeric_keypad_keys_and_capture() {
        run(|| {
            let surface = surface();
            apply(
                &surface,
                &[MountItem::Create {
                    id: 2,
                    kind: ViewKind::View,
                }],
            );
            let view = view(&surface, 2);
            let display = view.display();
            // X11's NumLock uses Mod2, a private GDK modifier bit which
            // must survive removing Shift and CapsLock.
            if display.type_().name() != "GdkX11Display" {
                return;
            }
            let keycode = display.map_keyval(gdk::Key::KP_1).unwrap()[0].keycode();
            let numlock = gdk::ModifierType::from_bits_retain(1 << 4);
            let numeric = key_event(
                &surface,
                gdk::Key::KP_1,
                keycode,
                numlock,
                KeyPhase::Down { repeat: false },
            );
            assert_eq!(numeric.key, "1");
            assert_eq!(numeric.code, "Numpad1", "the keypad key's physical place");
            let capture = nsh_core::KeyCapture::keys([Keystroke::parse_with("1", false).unwrap()]);
            let stroke =
                shortcut_stroke(&view, &numeric, keycode, numlock, 0).expect("a shortcut key");
            assert!(capture.captures_key(Some(&stroke)));
            let navigation = key_event(
                &surface,
                gdk::Key::KP_End,
                keycode,
                gdk::ModifierType::empty(),
                KeyPhase::Down { repeat: false },
            );
            assert_eq!(navigation.key, "End");
            assert_eq!(
                navigation.code, "Numpad1",
                "the code is the keypad key, not the navigation key it acts as"
            );
            let stroke =
                shortcut_stroke(&view, &navigation, keycode, gdk::ModifierType::empty(), 0)
                    .expect("End is a shortcut key");
            assert!(!capture.captures_key(Some(&stroke)));
        });
    }

    /// A key the layout cannot name keeps its source name and reports no
    /// fabricated code; a dead key is the DOM's `Dead`.
    #[test]
    fn an_unmapped_key_keeps_what_the_source_reports() {
        crate::gtk_test::run(|| {
            assert_eq!(
                super::logical_key(gtk4::gdk::Key::VoidSymbol),
                "Unidentified"
            );
            assert_eq!(super::logical_key(gtk4::gdk::Key::dead_acute), "Dead");
            let mut event = KeyEvent::default();
            event.key = super::logical_key(gtk4::gdk::Key::dead_acute);
            assert_eq!(event.key, "Dead");
            assert_eq!(event.code, "");
            assert!(!event.default_prevented);
            assert_eq!(Modifiers::default(), event.modifiers);
        });
    }

    /// The code is the hardware key's place, never what the layout types
    /// there: the same keycode reports the same DOM code under another
    /// layout's keyval, the keypad's keys keep their keypad codes when they
    /// act as navigation keys, modifier sides differ, and a keycode this
    /// backend has no code for is `""`.
    #[test]
    fn the_code_is_the_physical_keys_place_on_any_layout() {
        run(|| {
            let surface = surface();
            let event = |keyval, keycode| {
                key_event(
                    &surface,
                    keyval,
                    keycode,
                    gdk::ModifierType::empty(),
                    KeyPhase::Down { repeat: false },
                )
            };
            // XKB keycode 38 is the A key: `q` on AZERTY, `ф` on Cyrillic.
            let french = event(gdk::Key::q, 38);
            assert_eq!(french.key, "q", "the layout's character is kept");
            assert_eq!(french.code, "KeyA", "the physical key is reported");
            let cyrillic = event(gdk::Key::Cyrillic_ef, 38);
            assert_eq!(cyrillic.key, "ф");
            assert_eq!(cyrillic.code, "KeyA");
            // Punctuation is the key's place too.
            assert_eq!(event(gdk::Key::semicolon, 47).code, "Semicolon");
            assert_eq!(event(gdk::Key::slash, 61).code, "Slash");
            // The keypad's enter and decimal point are not the main ones.
            assert_eq!(event(gdk::Key::Return, 36).code, "Enter");
            assert_eq!(event(gdk::Key::KP_Enter, 104).code, "NumpadEnter");
            assert_eq!(event(gdk::Key::KP_Decimal, 91).code, "NumpadDecimal");
            // Both sides of each modifier have their own code.
            for (keyval, keycode, code) in [
                (gdk::Key::Shift_L, 50, "ShiftLeft"),
                (gdk::Key::Shift_R, 62, "ShiftRight"),
                (gdk::Key::Control_L, 37, "ControlLeft"),
                (gdk::Key::Control_R, 105, "ControlRight"),
                (gdk::Key::Alt_L, 64, "AltLeft"),
                (gdk::Key::Alt_R, 108, "AltRight"),
                (gdk::Key::Meta_L, 133, "MetaLeft"),
                (gdk::Key::Meta_R, 134, "MetaRight"),
            ] {
                assert_eq!(event(keyval, keycode).code, code);
            }
            // A synthetic event (no hardware keycode) and a keycode with
            // no DOM code report the empty string, never a guessed one.
            assert_eq!(event(gdk::Key::VoidSymbol, 0).code, "");
            assert_eq!(event(gdk::Key::VoidSymbol, 0xFFFF).code, "");
        });
    }
}

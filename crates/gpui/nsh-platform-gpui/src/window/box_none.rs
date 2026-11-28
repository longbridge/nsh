//! Windows whose `pointerEvents` is `"box-none"`: each takes the pointer
//! only over the views in its root that take it
//! ([`crate::input::takes_pointer`]) and lets it through to what is below
//! elsewhere. macOS and Windows decide it for where the pointer is
//! ([`PointerArea::takes`]), following it while such a window shows; Linux
//! gives the window an input region of those views' rectangles
//! ([`PointerArea::rects`]).

use std::rc::Rc;

use gpui::Window;
#[cfg(target_os = "linux")]
pub(crate) use linux::*;
#[cfg(target_os = "macos")]
pub(crate) use mac::*;
use nsh_core::{Bounds, Points};
#[cfg(windows)]
pub(crate) use win::*;

/// Where a `"box-none"` window takes the pointer, in its coordinates
/// (points from its top left corner); `None` once it is gone.
pub(crate) trait PointerArea {
    /// Whether it takes a press at a point.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    fn takes(&self, x: f32, y: f32) -> Option<bool>;
    /// The rectangles it takes presses in.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn rects(&self) -> Option<Vec<Bounds<Points>>>;
}

/// A `"box-none"` window's area.
pub(crate) type Area = Rc<dyn PointerArea>;

#[cfg(target_os = "macos")]
mod mac {
    use std::{cell::RefCell, ptr::NonNull};

    use block2::RcBlock;
    use objc2::{
        rc::Retained,
        runtime::{AnyObject, ProtocolObject},
    };
    use objc2_app_kit::{
        NSEvent, NSEventMask, NSWindow, NSWindowDidChangeOcclusionStateNotification,
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};

    use super::*;

    struct Entry {
        window: Retained<NSWindow>,
        area: Area,
    }

    thread_local! {
        static WINDOWS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
        /// The monitors following the pointer while one of the windows
        /// shows.
        static MONITORS: RefCell<Vec<Retained<AnyObject>>> = const { RefCell::new(Vec::new()) };
        /// Hears the windows show, hide and minimize while there are any.
        static OBSERVER: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
            const { RefCell::new(None) };
    }

    fn key(w: &NSWindow) -> usize {
        std::ptr::from_ref(w) as usize
    }

    /// The window takes the pointer only over the views `area` finds, or
    /// (`None`) no longer decides it so (the click-through flag stays as
    /// `set_click_through` left it).
    ///
    /// AppKit picks the window a press goes to before the window sees it,
    /// so the window follows the pointer and lets it through or not for
    /// where it is (`ignoresMouseEvents`).
    pub(crate) fn set(window: &Window, area: Option<Area>) {
        let Some(w) = super::super::native::ns_window(window) else {
            return;
        };
        forget_key(key(&w));
        let Some(area) = area else { return };
        WINDOWS.with_borrow_mut(|e| {
            e.push(Entry {
                window: w.clone(),
                area,
            })
        });
        observe_visibility();
        follow_while_shown();
        follow(Some(key(&w)));
    }

    /// The window drew a frame: it follows its new layout under the
    /// pointer where it rests.
    pub(crate) fn drawn(window: &Window) {
        if WINDOWS.with_borrow(Vec::is_empty) {
            return;
        }
        if let Some(w) = super::super::native::ns_window(window) {
            follow(Some(key(&w)));
        }
    }

    /// The window was ordered in: it takes the pointer for where it is now,
    /// before the pointer moves.
    pub(crate) fn shown(window: &Window) {
        if let Some(w) = super::super::native::ns_window(window) {
            shown_ns(&w);
        }
    }

    /// [`shown`], for AppKit's window (an order-in left to a later turn).
    pub(crate) fn shown_ns(w: &NSWindow) {
        if WINDOWS.with_borrow(Vec::is_empty) {
            return;
        }
        follow_while_shown();
        follow(Some(key(w)));
    }

    pub(crate) fn removing(window: &Window) {
        if let Some(w) = super::super::native::ns_window(window) {
            forget_key(key(&w));
        }
    }

    fn forget_key(k: usize) {
        let empty = WINDOWS.with_borrow_mut(|e| {
            e.retain(|e| key(&e.window) != k);
            e.is_empty()
        });
        follow_while_shown();
        if empty && let Some(observer) = OBSERVER.take() {
            // SAFETY: an observer the center returned.
            unsafe { NSNotificationCenter::defaultCenter().removeObserver(observer.as_ref()) };
        }
    }

    fn shows(w: &NSWindow) -> bool {
        w.isVisible() && !w.isMiniaturized()
    }

    /// Hears every window's visibility change (shown, hidden, minimized,
    /// the app hidden): the pointer is followed only while one of these
    /// windows shows, and one that shows follows it from there.
    fn observe_visibility() {
        if OBSERVER.with_borrow(Option::is_some) {
            return;
        }
        let block = RcBlock::new(|_: NonNull<NSNotification>| {
            follow_while_shown();
            follow(None);
        });
        // SAFETY: an immutable AppKit constant, posted on the main thread
        // where the block runs; the observer is removed with the last
        // window.
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSWindowDidChangeOcclusionStateNotification),
                None,
                None,
                &block,
            )
        };
        OBSERVER.set(Some(observer));
    }

    /// Watches the pointer move over every app (the global monitor) and
    /// this one (the local monitor), and the buttons coming up, while one
    /// of the windows shows; never from a monitor's own handler.
    fn follow_while_shown() {
        let any = WINDOWS.with_borrow(|e| e.iter().any(|e| shows(&e.window)));
        if !any {
            for monitor in MONITORS.with_borrow_mut(std::mem::take) {
                // SAFETY: a monitor `NSEvent` returned.
                unsafe { NSEvent::removeMonitor(&monitor) };
            }
            return;
        }
        if MONITORS.with_borrow(|m| !m.is_empty()) {
            return;
        }
        let mask = NSEventMask::MouseMoved
            | NSEventMask::LeftMouseUp
            | NSEventMask::RightMouseUp
            | NSEventMask::OtherMouseUp;
        let local = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            follow(None);
            event.as_ptr()
        });
        let global = RcBlock::new(|_: NonNull<NSEvent>| follow(None));
        let mut monitors = Vec::new();
        // SAFETY: the block returns the event it got; the monitor is
        // removed when no window shows.
        monitors
            .extend(unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) });
        monitors.extend(NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            mask, &global,
        ));
        MONITORS.set(monitors);
    }

    /// Every window that shows (only `only`, given one) lets the pointer
    /// through unless it is over one of its views. Not while a button is
    /// held: the press stays with the window it went to.
    fn follow(only: Option<usize>) {
        if NSEvent::pressedMouseButtons() != 0 {
            return;
        }
        let at = NSEvent::mouseLocation();
        // Taken out first: the area reads the views, and AppKit may call
        // back while a window changes.
        let entries: Vec<(Retained<NSWindow>, Area)> = WINDOWS.with_borrow(|e| {
            e.iter()
                .filter(|e| only.is_none_or(|o| o == key(&e.window)))
                .map(|e| (e.window.clone(), e.area.clone()))
                .collect()
        });
        for (w, area) in entries {
            if !shows(&w) {
                continue;
            }
            // GPUI's window coordinates: from the content's top left.
            let p = w.convertPointFromScreen(at);
            let height = w
                .contentView()
                .map_or(w.frame().size.height, |v| v.frame().size.height);
            match area.takes(p.x as f32, (height - p.y) as f32) {
                Some(takes) => {
                    if w.ignoresMouseEvents() == takes {
                        w.setIgnoresMouseEvents(!takes);
                    }
                }
                // Gone: forgotten on the next turn, not from a monitor's
                // own handler.
                None => {
                    let k = key(&w);
                    crate::rt::outside_app(move || forget_key(k));
                }
            }
        }
    }
}

#[cfg(windows)]
mod win {
    use std::cell::{Cell, RefCell};

    use windows_sys::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ScreenToClient,
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
            },
            WindowsAndMessaging::{
                GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, IsIconic, IsWindowVisible, KillTimer,
                LWA_ALPHA, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, WS_EX_LAYERED,
                WS_EX_TRANSPARENT,
            },
        },
    };

    use super::*;
    use crate::window::native::{hwnd, set_style};

    /// `WS_EX_TRANSPARENT` on (layered) `h`: presses go to the windows
    /// below, of any process. Read as each press comes: no frame change.
    fn set_passing(h: HWND, passing: bool) {
        // SAFETY: plain reads and writes of a window of this thread's
        // styles.
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
            let next = if passing {
                ex | WS_EX_TRANSPARENT
            } else {
                ex & !WS_EX_TRANSPARENT
            };
            if next != ex {
                SetWindowLongPtrW(h, GWL_EXSTYLE, next as isize);
            }
        }
    }

    /// How often the cursor is looked at while a `"box-none"` window shows
    /// (a frame at 60 Hz).
    const FOLLOW_MS: u32 = 16;

    struct Entry {
        hwnd: isize,
        area: Area,
        /// Its scale when it last drew (physical pixels per point).
        scale: Cell<f32>,
        /// Where the cursor was in it (pixels) and the [`LAYOUT`] when it
        /// last decided: the same again, the decision stands.
        decided: Cell<Option<(i32, i32, u64)>>,
    }

    thread_local! {
        static WINDOWS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
        /// The timer following the cursor while one of them shows.
        static TIMER: Cell<usize> = const { Cell::new(0) };
        /// A count that moves as the windows draw.
        static LAYOUT: Cell<u64> = const { Cell::new(0) };
    }

    /// The window takes the pointer only over the views `area` finds, or
    /// (`None`) no longer decides it so (its styles stay as
    /// `set_click_through` left them). It is layered while it does, and
    /// passes presses to the windows below, of any process, with
    /// `WS_EX_TRANSPARENT` where the cursor is over none of its views.
    pub(crate) fn set(window: &Window, area: Option<Area>) {
        let Some(h) = hwnd(window) else { return };
        forget(h);
        let Some(area) = area else { return };
        set_style(h, GWL_EXSTYLE, WS_EX_LAYERED, true);
        // SAFETY: a plain call on a live layered window: fully opaque.
        unsafe { SetLayeredWindowAttributes(h, 0, 255, LWA_ALPHA) };
        WINDOWS.with_borrow_mut(|e| {
            e.push(Entry {
                hwnd: h as isize,
                area,
                scale: Cell::new(window.scale_factor()),
                decided: Cell::new(None),
            })
        });
        follow_while_shown();
        follow(Some(h as isize));
    }

    /// The window drew a frame (it also draws as it shows or comes back
    /// from the minimized state): it follows its new layout under the
    /// cursor where it rests, and the cursor is followed while it shows.
    pub(crate) fn drawn(window: &Window) {
        if WINDOWS.with_borrow(Vec::is_empty) {
            return;
        }
        let Some(h) = hwnd(window) else { return };
        let key = h as isize;
        let known = WINDOWS.with_borrow(|e| {
            e.iter()
                .find(|e| e.hwnd == key)
                .map(|e| e.scale.set(window.scale_factor()))
                .is_some()
        });
        if known {
            LAYOUT.set(LAYOUT.get().wrapping_add(1));
            follow_while_shown();
            follow(Some(key));
        }
    }

    /// The window was shown: it takes the pointer for where the cursor is
    /// now, and the cursor is followed.
    pub(crate) fn shown(window: &Window) {
        drawn(window);
    }

    pub(crate) fn removing(window: &Window) {
        if let Some(h) = hwnd(window) {
            forget(h);
        }
    }

    fn forget(h: HWND) {
        let key = h as isize;
        WINDOWS.with_borrow_mut(|e| e.retain(|e| e.hwnd != key));
        follow_while_shown();
    }

    fn shows(h: HWND) -> bool {
        // SAFETY: plain queries; false for a window that is gone.
        unsafe { IsWindowVisible(h) != 0 && IsIconic(h) == 0 }
    }

    /// Starts the timer while one of the windows shows, stops it otherwise.
    fn follow_while_shown() {
        let any = WINDOWS.with_borrow(|e| e.iter().any(|e| shows(e.hwnd as HWND)));
        let timer = TIMER.get();
        if any && timer == 0 {
            // SAFETY: a thread timer (no window) with a callback of the
            // right signature; this thread's message loop dispatches it.
            TIMER.set(unsafe { SetTimer(std::ptr::null_mut(), 0, FOLLOW_MS, Some(tick)) });
        } else if !any && timer != 0 {
            // SAFETY: a thread timer this thread made.
            unsafe { KillTimer(std::ptr::null_mut(), timer) };
            TIMER.set(0);
        }
    }

    unsafe extern "system" fn tick(_: HWND, _: u32, _: usize, _: u32) {
        follow(None);
        // Hidden or minimized since: no more ticks.
        follow_while_shown();
    }

    fn button_held() -> bool {
        [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
            .into_iter()
            // SAFETY: a plain query of the keys' state.
            .any(|vk| unsafe { GetAsyncKeyState(vk as i32) } < 0)
    }

    /// Every window that shows (only `only`, given one) passes presses on
    /// unless the cursor is over one of its views. Not while a button is
    /// held: the press stays with the window it went to.
    fn follow(only: Option<isize>) {
        if button_held() {
            return;
        }
        let mut cursor = POINT { x: 0, y: 0 };
        // SAFETY: an out value.
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return;
        }
        let layout = LAYOUT.get();
        let entries: Vec<(isize, Area, f32)> = WINDOWS.with_borrow(|e| {
            e.iter()
                .filter(|e| only.is_none_or(|o| o == e.hwnd))
                .map(|e| (e.hwnd, e.area.clone(), e.scale.get()))
                .collect()
        });
        for (key, area, scale) in entries {
            let h = key as HWND;
            if !shows(h) {
                continue;
            }
            let mut p = cursor;
            // SAFETY: a window of this thread; an in-out point.
            if unsafe { ScreenToClient(h, &mut p) } == 0 {
                continue;
            }
            // The cursor where it was in the window, the window laid out as
            // it was: the decision stands (the timer finds this most of the
            // time).
            let now = Some((p.x, p.y, layout));
            if WINDOWS.with_borrow(|e| {
                e.iter()
                    .find(|e| e.hwnd == key)
                    .is_some_and(|e| e.decided.replace(now) == now)
            }) {
                continue;
            }
            match area.takes(p.x as f32 / scale, p.y as f32 / scale) {
                Some(takes) => set_passing(h, !takes),
                None => {
                    // Gone: forgotten on the next turn, not from the
                    // timer's own callback.
                    crate::rt::outside_app(move || forget(key as HWND));
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::cell::RefCell;

    use gpui::{Pixels, px};

    use super::*;

    struct Entry {
        id: gpui::WindowId,
        area: Area,
        /// The region last given to the window.
        region: Option<Vec<gpui::Bounds<Pixels>>>,
    }

    thread_local! {
        static WINDOWS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    }

    /// The window takes the pointer only over the views `area` finds (its
    /// input region), or (`None`) no longer decides it so (the region stays
    /// as the caller set it).
    pub(crate) fn set(window: &Window, area: Option<Area>) {
        let id = window.window_handle().window_id();
        WINDOWS.with_borrow_mut(|e| e.retain(|e| e.id != id));
        let Some(area) = area else { return };
        WINDOWS.with_borrow_mut(|e| {
            e.push(Entry {
                id,
                area,
                region: None,
            })
        });
        drawn(window);
    }

    /// The window drew a frame: its input region follows its new layout.
    pub(crate) fn drawn(window: &Window) {
        if WINDOWS.with_borrow(Vec::is_empty) {
            return;
        }
        let id = window.window_handle().window_id();
        let Some(area) =
            WINDOWS.with_borrow(|e| e.iter().find(|e| e.id == id).map(|e| e.area.clone()))
        else {
            return;
        };
        let Some(rects) = area.rects() else {
            WINDOWS.with_borrow_mut(|e| e.retain(|e| e.id != id));
            return;
        };
        let region: Vec<gpui::Bounds<Pixels>> = rects
            .iter()
            .map(|r| {
                // Out to whole pixels: GPUI cuts each field down.
                let (x, y) = (r.origin.x.0.floor(), r.origin.y.0.floor());
                let right = (r.origin.x.0 + r.size.width.0).ceil();
                let bottom = (r.origin.y.0 + r.size.height.0).ceil();
                gpui::Bounds::new(
                    gpui::point(px(x), px(y)),
                    gpui::size(px(right - x), px(bottom - y)),
                )
            })
            .collect();
        let changed = WINDOWS.with_borrow_mut(|e| {
            e.iter_mut().find(|e| e.id == id).is_some_and(|e| {
                let changed = e.region.as_ref() != Some(&region);
                if changed {
                    e.region = Some(region.clone());
                }
                changed
            })
        });
        if changed {
            set_input_region(window, Some(&region));
        }
    }

    /// The window was shown: its region is given again (a surface shown
    /// anew may have dropped it).
    pub(crate) fn shown(window: &Window) {
        let id = window.window_handle().window_id();
        let known = WINDOWS.with_borrow_mut(|e| {
            e.iter_mut()
                .find(|e| e.id == id)
                .map(|e| e.region = None)
                .is_some()
        });
        if known {
            drawn(window);
        }
    }

    pub(crate) fn removing(window: &Window) {
        let id = window.window_handle().window_id();
        WINDOWS.with_borrow_mut(|e| e.retain(|e| e.id != id));
    }

    /// The window takes the pointer only in `region` (window coordinates;
    /// empty: nowhere), or (`None`) everywhere: Wayland's input region
    /// through GPUI, X11's input shape here (GPUI sets none there).
    pub(crate) fn set_input_region(window: &Window, region: Option<&[gpui::Bounds<Pixels>]>) {
        window.set_input_region(region);
        x11::set_input_shape(window, region);
    }

    mod x11 {
        use std::cell::OnceCell;

        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use x11rb::{
            connection::{Connection, RequestConnection},
            protocol::{
                shape::{self, ConnectionExt as _, SK, SO},
                xproto::{ClipOrdering, Rectangle},
            },
            rust_connection::RustConnection,
        };

        use super::*;

        thread_local! {
            /// A connection to the X server with the shape extension, if
            /// there is one.
            static CONNECTION: OnceCell<Option<RustConnection>> = const { OnceCell::new() };
        }

        /// The X11 window under `window`, if it is one.
        fn x11_window(window: &Window) -> Option<u32> {
            match HasWindowHandle::window_handle(window).ok()?.as_raw() {
                RawWindowHandle::Xlib(h) => Some(h.window as u32),
                RawWindowHandle::Xcb(h) => Some(h.window.get()),
                _ => None,
            }
        }

        fn connect() -> Option<RustConnection> {
            let (conn, _) = x11rb::connect(None).ok()?;
            conn.extension_information(shape::X11_EXTENSION_NAME)
                .ok()
                .flatten()?;
            Some(conn)
        }

        /// Sets the window's input shape (X11's input region) to `region`,
        /// in device pixels, or back to the whole window.
        pub(super) fn set_input_shape(window: &Window, region: Option<&[gpui::Bounds<Pixels>]>) {
            let Some(xid) = x11_window(window) else {
                return;
            };
            let scale = window.scale_factor();
            CONNECTION.with(|c| {
                let Some(conn) = c.get_or_init(connect) else {
                    return;
                };
                let sent = match region {
                    None => conn
                        .shape_mask(SO::SET, SK::INPUT, xid, 0, 0, x11rb::NONE)
                        .map(drop),
                    Some(rects) => {
                        let rects: Vec<Rectangle> = rects
                            .iter()
                            .map(|r| {
                                let x = (f32::from(r.origin.x) * scale).floor();
                                let y = (f32::from(r.origin.y) * scale).floor();
                                let right = (f32::from(r.origin.x + r.size.width) * scale).ceil();
                                let bottom = (f32::from(r.origin.y + r.size.height) * scale).ceil();
                                Rectangle {
                                    x: x.clamp(i16::MIN as f32, i16::MAX as f32) as i16,
                                    y: y.clamp(i16::MIN as f32, i16::MAX as f32) as i16,
                                    width: (right - x).clamp(0.0, u16::MAX as f32) as u16,
                                    height: (bottom - y).clamp(0.0, u16::MAX as f32) as u16,
                                }
                            })
                            .collect();
                        conn.shape_rectangles(
                            SO::SET,
                            SK::INPUT,
                            ClipOrdering::UNSORTED,
                            xid,
                            0,
                            0,
                            &rects,
                        )
                        .map(drop)
                    }
                };
                if let Err(err) = sent
                    .map_err(|e| e.to_string())
                    .and_then(|()| conn.flush().map_err(|e| e.to_string()))
                {
                    log::debug!("window: cannot set the input shape: {err}");
                }
            });
        }
    }
}

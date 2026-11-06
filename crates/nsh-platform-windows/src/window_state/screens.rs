//! Monitor-local pixel/DIP transforms in one desktop coordinate space.
//! Lay out touching monitors from the primary display, in DIPs. Scaling an
//! absolute screen origin by a window's DPI loses the monitor's offset.

use nsh_core::{
    desktop::{Display, DisplayId},
    geometry::{Bounds, Point, Points, point, size},
};
use windows::{
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{
            DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC,
            HMONITOR, MONITORINFO, MONITORINFOEXW,
        },
        UI::{
            Shell::GetScaleFactorForMonitor,
            WindowsAndMessaging::{EDD_GET_DEVICE_INTERFACE_NAME, MONITORINFOF_PRIMARY},
        },
    },
    core::{BOOL, PCWSTR},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Screen {
    pub(super) monitor: HMONITOR,
    pub(super) native: RECT,
    pub(super) work: RECT,
    pub(super) scale: f32,
    pub(super) display: Display,
}

impl Screen {
    pub(super) fn bounds(self, rect: RECT) -> Bounds<Points> {
        Bounds::new(
            point(
                self.display.bounds.origin.x
                    + Points((rect.left - self.native.left) as f32 / self.scale),
                self.display.bounds.origin.y
                    + Points((rect.top - self.native.top) as f32 / self.scale),
            ),
            size(
                Points((rect.right - rect.left) as f32 / self.scale),
                Points((rect.bottom - rect.top) as f32 / self.scale),
            ),
        )
    }

    pub(super) fn native_bounds(self, bounds: Bounds<Points>) -> RECT {
        let left = self.native.left
            + ((bounds.origin.x - self.display.bounds.origin.x).0 * self.scale).round() as i32;
        let top = self.native.top
            + ((bounds.origin.y - self.display.bounds.origin.y).0 * self.scale).round() as i32;
        RECT {
            left,
            top,
            right: left.saturating_add((bounds.size.width.0 * self.scale).round() as i32),
            bottom: top.saturating_add((bounds.size.height.0 * self.scale).round() as i32),
        }
    }
}

#[derive(Default)]
pub(super) struct Screens(pub Vec<Screen>);

impl Screens {
    pub(super) fn read() -> Self {
        unsafe extern "system" fn collect(
            monitor: HMONITOR,
            _: HDC,
            _: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: the extended structure advertises its full size; EnumDisplayMonitors
            // calls synchronously while the caller's Vec is alive.
            unsafe {
                if !GetMonitorInfoW(monitor, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
                    return true.into();
                }
                let native = info.monitorInfo.rcMonitor;
                if native.right <= native.left || native.bottom <= native.top {
                    return true.into();
                }
                let scale = GetScaleFactorForMonitor(monitor)
                    .map_or(1.0, |s| s.0 as f32 / 100.0)
                    .max(1.0);
                let mut device = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                let stable = if EnumDisplayDevicesW(
                    PCWSTR(info.szDevice.as_ptr()),
                    0,
                    &mut device,
                    EDD_GET_DEVICE_INTERFACE_NAME,
                )
                .as_bool()
                    && device.DeviceID[0] != 0
                {
                    &device.DeviceID[..]
                } else {
                    &info.szDevice[..]
                };
                let end = stable.iter().position(|c| *c == 0).unwrap_or(stable.len());
                let name = String::from_utf16_lossy(&stable[..end]);
                let display = Display::new(
                    DisplayId::from_name(name.as_bytes()),
                    Bounds::default(),
                    Bounds::default(),
                    info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                );
                let screens = &mut *(data.0 as *mut Vec<Screen>);
                screens.push(Screen {
                    monitor,
                    native,
                    work: info.monitorInfo.rcWork,
                    scale,
                    display,
                });
                true.into()
            }
        }
        let mut screens = Vec::new();
        // SAFETY: the callback borrows this Vec only during enumeration.
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(collect),
                LPARAM(&mut screens as *mut _ as isize),
            );
        }
        Self::layout(screens)
    }

    fn layout(mut screens: Vec<Screen>) -> Self {
        screens.sort_by_key(|s| {
            (
                !s.display.primary,
                s.native.left,
                s.native.top,
                s.display.id.0,
            )
        });
        let Some(primary) = screens.first().copied() else {
            return Self(screens);
        };
        for screen in &mut screens {
            screen.display.bounds = Bounds::new(
                point(
                    Points(screen.native.left as f32 / primary.scale),
                    Points(screen.native.top as f32 / primary.scale),
                ),
                size(
                    Points((screen.native.right - screen.native.left) as f32 / screen.scale),
                    Points((screen.native.bottom - screen.native.top) as f32 / screen.scale),
                ),
            );
        }
        let mut placed = vec![false; screens.len()];
        placed[0] = true;
        loop {
            let mut changed = false;
            for child in 1..screens.len() {
                if placed[child] {
                    continue;
                }
                for parent in 0..screens.len() {
                    if !placed[parent] {
                        continue;
                    }
                    if let Some(origin) = adjacent(screens[parent], screens[child]) {
                        screens[child].display.bounds.origin = origin;
                        placed[child] = true;
                        changed = true;
                        break;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for screen in &mut screens {
            screen.display.visible_bounds = screen.bounds(screen.work);
        }
        Self(screens)
    }

    pub(super) fn displays(&self) -> Vec<Display> {
        self.0.iter().map(|s| s.display).collect()
    }

    pub(super) fn native_screen(&self, rect: RECT) -> Option<Screen> {
        self.best(
            Bounds::new(
                point(Points(rect.left as f32), Points(rect.top as f32)),
                size(
                    Points((rect.right - rect.left) as f32),
                    Points((rect.bottom - rect.top) as f32),
                ),
            ),
            false,
        )
    }

    pub(super) fn logical_screen(&self, rect: Bounds<Points>) -> Option<Screen> {
        self.best(rect, true)
    }

    fn best(&self, bounds: Bounds<Points>, logical: bool) -> Option<Screen> {
        self.0.iter().copied().max_by(|a, b| {
            let score = |s: Screen| {
                let area = if logical {
                    s.display.bounds
                } else {
                    Bounds::new(
                        point(Points(s.native.left as f32), Points(s.native.top as f32)),
                        size(
                            Points((s.native.right - s.native.left) as f32),
                            Points((s.native.bottom - s.native.top) as f32),
                        ),
                    )
                };
                let overlap = bounds.intersect(&area);
                let visible = overlap.size.width.0.max(0.0) * overlap.size.height.0.max(0.0);
                let dx = (area.origin.x.0 - bounds.origin.x.0)
                    .max(0.0)
                    .max(bounds.origin.x.0 - (area.origin.x + area.size.width).0);
                let dy = (area.origin.y.0 - bounds.origin.y.0)
                    .max(0.0)
                    .max(bounds.origin.y.0 - (area.origin.y + area.size.height).0);
                (visible, -(dx * dx + dy * dy), s.display.primary)
            };
            let (aa, ad, ap) = score(*a);
            let (ba, bd, bp) = score(*b);
            aa.total_cmp(&ba).then(ad.total_cmp(&bd)).then(ap.cmp(&bp))
        })
    }
}

fn adjacent(parent: Screen, child: Screen) -> Option<Point<Points>> {
    let p = parent.native;
    let c = child.native;
    let origin = parent.display.bounds.origin;
    let extent = parent.display.bounds.size;
    let y = origin.y + Points((c.top - p.top) as f32 / parent.scale);
    let x = origin.x + Points((c.left - p.left) as f32 / parent.scale);
    if p.top.max(c.top) <= p.bottom.min(c.bottom) {
        if c.left == p.right {
            return Some(point(origin.x + extent.width, y));
        }
        if c.right == p.left {
            return Some(point(origin.x - child.display.bounds.size.width, y));
        }
    }
    if p.left.max(c.left) <= p.right.min(c.right) {
        if c.top == p.bottom {
            return Some(point(x, origin.y + extent.height));
        }
        if c.bottom == p.top {
            return Some(point(x, origin.y - child.display.bounds.size.height));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use nsh_core::desktop::WindowBounds;

    use super::*;

    fn screen(id: u64, native: RECT, scale: f32, primary: bool) -> Screen {
        Screen {
            monitor: HMONITOR(id as usize as *mut _),
            native,
            work: native,
            scale,
            display: Display::new(DisplayId(id), Bounds::default(), Bounds::default(), primary),
        }
    }

    #[test]
    fn mixed_dpi_restore_uses_the_saved_display_not_the_receiving_windows_dpi() {
        let screens = Screens::layout(vec![
            screen(
                1,
                RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                1.0,
                true,
            ),
            screen(
                2,
                RECT {
                    left: 1920,
                    top: 0,
                    right: 5760,
                    bottom: 2160,
                },
                2.0,
                false,
            ),
        ]);
        let native = RECT {
            left: 2200,
            top: 120,
            right: 3200,
            bottom: 920,
        };
        let source = screens.0[1];
        let saved = WindowBounds::Windowed(source.bounds(native));
        assert_eq!(saved.get_bounds().origin.x, Points(2060.0));
        assert_eq!(saved.get_bounds().size.width, Points(500.0));
        let json = serde_json::to_string(&saved).unwrap();
        let saved: WindowBounds = serde_json::from_str(&json).unwrap();
        let target = screens.logical_screen(saved.get_bounds()).unwrap();
        assert_eq!(target.display.id, DisplayId(2));
        assert_eq!(target.native_bounds(saved.get_bounds()), native);
    }

    #[test]
    fn screen_bounds_round_trip_at_fractional_dpis_and_negative_monitor_positions() {
        for primary_scale in [1.0, 1.25, 1.5, 2.0] {
            for other_scale in [1.0, 1.25, 1.5, 2.0] {
                for native in [
                    RECT {
                        left: 1920,
                        top: 0,
                        right: 4480,
                        bottom: 1440,
                    },
                    RECT {
                        left: -2560,
                        top: 0,
                        right: 0,
                        bottom: 1440,
                    },
                    RECT {
                        left: 0,
                        top: -1440,
                        right: 2560,
                        bottom: 0,
                    },
                    RECT {
                        left: 0,
                        top: 1080,
                        right: 2560,
                        bottom: 2520,
                    },
                ] {
                    let screens = Screens::layout(vec![
                        screen(2, native, other_scale, false),
                        screen(
                            1,
                            RECT {
                                left: 0,
                                top: 0,
                                right: 1920,
                                bottom: 1080,
                            },
                            primary_scale,
                            true,
                        ),
                    ]);
                    assert!(screens.0[0].display.primary);
                    for source in &screens.0 {
                        let rect = RECT {
                            left: source.native.left + 100,
                            top: source.native.top + 100,
                            right: source.native.left + 500,
                            bottom: source.native.top + 400,
                        };
                        let bounds = source.bounds(rect);
                        let target = screens.logical_screen(bounds).unwrap();
                        assert_eq!(target.display.id, source.display.id);
                        assert_eq!(target.native_bounds(bounds), rect);
                        assert_eq!(
                            source.native_bounds(source.display.visible_bounds),
                            source.work
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn work_area_offsets_are_transformed_relative_to_their_monitor() {
        let mut right = screen(
            2,
            RECT {
                left: 1920,
                top: 0,
                right: 5760,
                bottom: 2160,
            },
            2.0,
            false,
        );
        right.work.left += 80;
        right.work.bottom -= 80;
        let screens = Screens::layout(vec![
            screen(
                1,
                RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                1.0,
                true,
            ),
            right,
        ]);
        let right = screens.0[1];
        assert_eq!(right.display.visible_bounds.origin.x, Points(1960.0));
        assert_eq!(right.display.visible_bounds.size.width, Points(1880.0));
        assert_eq!(
            right.native_bounds(right.display.visible_bounds),
            right.work
        );
    }
}

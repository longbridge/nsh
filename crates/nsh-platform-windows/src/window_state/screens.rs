//! Monitor-local pixel/DIP transforms in one desktop coordinate space.
//! Lay out disjoint monitor regions from the primary display, in DIPs. Scaling an
//! absolute screen origin by a window's DPI loses the monitor's offset.

use nsh_core::{
    desktop::{Display, DisplayId},
    geometry::{Bounds, Points, point, size},
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
        let x = axis_origins(&screens, primary, true);
        let y = axis_origins(&screens, primary, false);
        for (i, screen) in screens.iter_mut().enumerate() {
            screen.display.bounds = Bounds::new(
                point(x[i], y[i]),
                size(
                    Points((screen.native.right - screen.native.left) as f32 / screen.scale),
                    Points((screen.native.bottom - screen.native.top) as f32 / screen.scale),
                ),
            );
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

/// Keeps native edge ordering in points. Each interval uses the smallest DPI
/// scale of the monitors spanning it, giving every monitor at least the space
/// its own DIP size needs. Disjoint native rectangles then stay disjoint in
/// points, including siblings beside a differently scaled primary monitor.
fn axis_origins(screens: &[Screen], primary: Screen, horizontal: bool) -> Vec<Points> {
    let range = |screen: &Screen| {
        let r = screen.native;
        if horizontal {
            (r.left, r.right)
        } else {
            (r.top, r.bottom)
        }
    };
    let mut edges: Vec<_> = screens
        .iter()
        .flat_map(|s| {
            let (start, end) = range(s);
            [start, end]
        })
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut positions = vec![(edges[0], Points::ZERO)];
    for edge in edges.windows(2) {
        let scale = screens
            .iter()
            .filter(|s| {
                let (start, end) = range(s);
                start <= edge[0] && end >= edge[1]
            })
            .map(|s| s.scale)
            .min_by(f32::total_cmp)
            .unwrap_or(primary.scale);
        let offset =
            positions.last().unwrap().1 + Points((edge[1] as f32 - edge[0] as f32) / scale);
        positions.push((edge[1], offset));
    }
    let at = |edge| positions[positions.binary_search_by_key(&edge, |p| p.0).unwrap()].1;
    let anchor = at(range(&primary).0);
    screens.iter().map(|s| at(range(s).0) - anchor).collect()
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

    fn assert_disjoint(screens: &Screens) {
        for (i, a) in screens.0.iter().enumerate() {
            for b in &screens.0[i + 1..] {
                let a = a.display.bounds;
                let b = b.display.bounds;
                assert!(
                    a.origin.x + a.size.width <= b.origin.x
                        || b.origin.x + b.size.width <= a.origin.x
                        || a.origin.y + a.size.height <= b.origin.y
                        || b.origin.y + b.size.height <= a.origin.y,
                    "logical monitor regions overlap: {a:?}, {b:?}"
                );
            }
        }
    }

    fn assert_saved_round_trip(screens: &Screens, source: Screen, native: RECT) {
        let bounds = source.bounds(native);
        for saved in [
            WindowBounds::Windowed(bounds),
            WindowBounds::Maximized(bounds),
            WindowBounds::Fullscreen(bounds),
        ] {
            let json = serde_json::to_string(&saved).unwrap();
            let saved: WindowBounds = serde_json::from_str(&json).unwrap();
            let target = screens.logical_screen(saved.get_bounds()).unwrap();
            assert_eq!(target.display.id, source.display.id, "{json}");
            assert_eq!(target.native_bounds(saved.get_bounds()), native, "{json}");
        }
    }

    #[test]
    fn stacked_displays_beside_a_high_dpi_primary_restore_to_the_saved_display() {
        let screens = Screens::layout(vec![
            screen(
                1,
                RECT {
                    left: 0,
                    top: 0,
                    right: 3840,
                    bottom: 2160,
                },
                2.0,
                true,
            ),
            screen(
                2,
                RECT {
                    left: 3840,
                    top: 0,
                    right: 5760,
                    bottom: 1080,
                },
                1.0,
                false,
            ),
            screen(
                3,
                RECT {
                    left: 3840,
                    top: 1080,
                    right: 5760,
                    bottom: 2160,
                },
                1.0,
                false,
            ),
        ]);
        assert_eq!(
            screens.0[1].display.bounds.origin,
            point(Points(1920.0), Points::ZERO)
        );
        assert_eq!(
            screens.0[2].display.bounds.origin,
            point(Points(1920.0), Points(1080.0))
        );
        assert_disjoint(&screens);
        // The review's exact rectangle used to choose the lower display and
        // restore at y=1240 instead of y=700.
        assert_saved_round_trip(
            &screens,
            screens.0[1],
            RECT {
                left: 4000,
                top: 700,
                right: 4400,
                bottom: 1000,
            },
        );
        assert_saved_round_trip(
            &screens,
            screens.0[2],
            RECT {
                left: 4000,
                top: 1280,
                right: 4400,
                bottom: 1580,
            },
        );
    }

    #[test]
    fn three_monitor_layouts_remain_disjoint_and_round_trip_across_dpis_and_orders() {
        let primary = RECT {
            left: 0,
            top: 0,
            right: 3840,
            bottom: 2160,
        };
        for children in [
            [
                RECT {
                    left: 3840,
                    top: 0,
                    right: 5760,
                    bottom: 1080,
                },
                RECT {
                    left: 3840,
                    top: 1080,
                    right: 5760,
                    bottom: 2160,
                },
            ],
            [
                RECT {
                    left: -1920,
                    top: 0,
                    right: 0,
                    bottom: 1080,
                },
                RECT {
                    left: -1920,
                    top: 1080,
                    right: 0,
                    bottom: 2160,
                },
            ],
            [
                RECT {
                    left: 0,
                    top: 2160,
                    right: 1920,
                    bottom: 3240,
                },
                RECT {
                    left: 1920,
                    top: 2160,
                    right: 3840,
                    bottom: 3240,
                },
            ],
            [
                RECT {
                    left: 0,
                    top: -1080,
                    right: 1920,
                    bottom: 0,
                },
                RECT {
                    left: 1920,
                    top: -1080,
                    right: 3840,
                    bottom: 0,
                },
            ],
        ] {
            for a in [1.0, 1.25, 1.5, 2.0] {
                for b in [1.0, 1.25, 1.5, 2.0] {
                    for c in [1.0, 1.25, 1.5, 2.0] {
                        let mut input = vec![
                            screen(3, children[1], c, false),
                            screen(1, primary, a, true),
                            screen(2, children[0], b, false),
                        ];
                        let screens = Screens::layout(input.clone());
                        input.reverse();
                        assert_eq!(screens.displays(), Screens::layout(input).displays());
                        assert_disjoint(&screens);
                        for source in &screens.0 {
                            assert_saved_round_trip(
                                &screens,
                                *source,
                                RECT {
                                    left: source.native.left + 160,
                                    top: source.native.top + 60,
                                    right: source.native.left + 560,
                                    bottom: source.native.top + 360,
                                },
                            );
                            assert_eq!(
                                source.native_bounds(source.display.visible_bounds),
                                source.work
                            );
                        }
                    }
                }
            }
        }
    }
}

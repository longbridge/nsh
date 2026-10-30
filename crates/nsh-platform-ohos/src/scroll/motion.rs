//! Scale and offset motion evaluated by the surface's native frame clock.

/// Tracks reports already delivered to the engine. Applied scroll commands must
/// be acknowledged even when their final position is unchanged.
#[derive(Default)]
pub(crate) struct OffsetReport {
    last: std::cell::Cell<Option<((f32, f32), u32)>>,
}
impl OffsetReport {
    pub(crate) fn update(&self, offset: (f32, f32), applied: u32) -> bool {
        let next = Some((offset, applied));
        self.last.replace(next) != next
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ZoomFrame {
    pub(crate) scale: f32,
    pub(crate) offset: (f32, f32),
}

#[derive(Clone, Copy)]
pub(crate) struct ZoomMotion {
    pub(crate) from: ZoomFrame,
    pub(crate) to: ZoomFrame,
    pub(crate) start: f64,
}

impl ZoomMotion {
    pub(crate) fn sample(self, now: f64) -> (ZoomFrame, bool) {
        let t = ((now - self.start) / 250.0).clamp(0.0, 1.0) as f32;
        let eased = 1.0 - (1.0 - t).powi(3);
        let lerp = |a, b| a + (b - a) * eased;
        (
            ZoomFrame {
                scale: lerp(self.from.scale, self.to.scale),
                offset: (
                    lerp(self.from.offset.0, self.to.offset.0),
                    lerp(self.from.offset.1, self.to.offset.1),
                ),
            },
            t < 1.0,
        )
    }
}

/// Keeps a content point under the given viewport position while scale changes.
pub(crate) fn focal_offset(
    content: f32,
    viewport_point: f32,
    scale: f32,
    length: f32,
    viewport: f32,
) -> f32 {
    (content * scale - viewport_point).clamp(0.0, (length * scale - viewport).max(0.0))
}

/// Rubber-band zoom past a bound; release animates back to the mounted limits.
pub(crate) fn resisted_scale(scale: f32, min: f32, max: f32, bounce: bool) -> f32 {
    let bound = scale.clamp(min, max);
    if bounce {
        (bound + (scale - bound) * 0.25).clamp(min * 0.5, max * 1.5)
    } else {
        bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_duplicates_are_suppressed_without_losing_command_acknowledgements() {
        let report = OffsetReport::default();
        assert!(report.update((0.0, 0.0), 0));
        assert!(report.update((40.0, 0.0), 1)); // immediate programmatic result
        assert!(!report.update((40.0, 0.0), 1)); // delayed OnDidScroll
        assert!(!report.update((40.0, 0.0), 1)); // OnScrollStop
        assert!(report.update((40.0, 0.0), 2)); // clamped command still acknowledged
        assert!(!report.update((40.0, 0.0), 2));
        assert!(report.update((39.0, 0.0), 2)); // finger movement still delivered
    }

    #[test]
    fn zoom_interpolates_scale_and_offsets_and_finishes_exactly() {
        let motion = ZoomMotion {
            from: ZoomFrame {
                scale: 1.0,
                offset: (0.0, 10.0),
            },
            to: ZoomFrame {
                scale: 3.0,
                offset: (200.0, 250.0),
            },
            start: 20.0,
        };
        let (frame, running) = motion.sample(145.0);
        assert!(running);
        assert_eq!(frame.scale, 2.75);
        assert_eq!(frame.offset, (175.0, 220.0));
        let (frame, running) = motion.sample(270.0);
        assert!(!running);
        assert_eq!(frame.scale, 3.0);
        assert_eq!(frame.offset, (200.0, 250.0));
    }

    #[test]
    fn focal_point_stays_under_the_fingers_until_content_bounds() {
        assert_eq!(focal_offset(90.0, 30.0, 2.0, 400.0, 200.0), 150.0);
        assert_eq!(focal_offset(10.0, 80.0, 1.0, 400.0, 200.0), 0.0);
        assert_eq!(focal_offset(390.0, 30.0, 2.0, 400.0, 200.0), 600.0);
    }

    #[test]
    fn bounce_has_resistance_and_is_bounded() {
        assert_eq!(resisted_scale(5.0, 1.0, 3.0, false), 3.0);
        assert_eq!(resisted_scale(5.0, 1.0, 3.0, true), 3.5);
        assert_eq!(resisted_scale(0.0, 1.0, 3.0, true), 0.75);
        assert_eq!(resisted_scale(f32::MAX, 1.0, 3.0, true), 4.5);
    }
}

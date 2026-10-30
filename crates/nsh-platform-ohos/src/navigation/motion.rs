//! Pure push/pop interpolation and edge-swipe completion decisions.

pub(crate) struct Motion {
    start: f64,
    duration: f64,
    from: f32,
    pub(crate) to: f32,
}
impl Motion {
    pub(crate) fn new(start: f64, from: f32, to: f32, animated: bool) -> Self {
        Self {
            start,
            from,
            to,
            duration: if animated {
                nsh_core::navigation::SLIDE.as_secs_f64() * 1000.0 * (from - to).abs() as f64
            } else {
                0.0
            },
        }
    }
    pub(crate) fn step(&self, now: f64, reduced: bool) -> (f32, bool, f32) {
        let t = if self.duration <= 0.0 || reduced {
            1.0
        } else {
            ((now - self.start) / self.duration).clamp(0.0, 1.0) as f32
        };
        (
            self.from + (self.to - self.from) * nsh_core::navigation::ease(t as f64),
            t >= 1.0,
            self.to,
        )
    }
}
pub(crate) fn complete_swipe(distance: f32, width: f32, velocity: f32) -> bool {
    distance.is_finite()
        && width.is_finite()
        && velocity.is_finite()
        && width > 0.0
        && distance > 8.0
        && velocity > -600.0
        && (distance >= width * 0.35 || velocity > 600.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelling_a_partial_swipe_preserves_position_and_finishes_at_rest() {
        let cancel = Motion::new(100.0, 0.4, 0.0, true);
        assert_eq!(cancel.step(100.0, false), (0.4, false, 0.0));
        let middle = cancel.step(160.0, false);
        assert!(middle.0 > 0.0 && middle.0 < 0.4 && !middle.1);
        assert_eq!(cancel.step(220.0, false), (0.0, true, 0.0));
    }
    #[test]
    fn reduced_motion_and_no_animation_settle_the_current_destination() {
        assert_eq!(
            Motion::new(0.0, 0.2, 1.0, true).step(1.0, true),
            (1.0, true, 1.0)
        );
        assert_eq!(
            Motion::new(0.0, 1.0, 0.0, false).step(0.0, false),
            (0.0, true, 0.0)
        );
    }
    #[test]
    fn completion_accounts_for_fling_direction_and_invalid_geometry() {
        assert!(complete_swipe(150.0, 400.0, 0.0));
        assert!(complete_swipe(20.0, 400.0, 700.0));
        assert!(!complete_swipe(200.0, 400.0, -700.0));
        assert!(!complete_swipe(4.0, 400.0, 2000.0));
        assert!(!complete_swipe(200.0, 0.0, 0.0));
        assert!(!complete_swipe(f32::NAN, 400.0, 0.0));
    }
}

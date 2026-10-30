//! Bounded edge scrolling for a held text-selection contact, in viewport units.

pub(crate) fn velocity(position: f32, extent: f32) -> f32 {
    if !position.is_finite() || !extent.is_finite() || extent <= 0.0 {
        return 0.0;
    }
    let edge = 32.0_f32.min(extent * 0.25);
    let penetration = if position < edge {
        (position - edge) / edge
    } else if position > extent - edge {
        (position - extent + edge) / edge
    } else {
        0.0
    };
    penetration.clamp(-2.0, 2.0) * 300.0
}

pub(crate) fn offset(current: f32, maximum: f32, speed: f32, elapsed_ms: f64) -> f32 {
    if !elapsed_ms.is_finite() || !speed.is_finite() {
        return current;
    }
    (current + speed * elapsed_ms.clamp(0.0, 32.0) as f32 / 1000.0).clamp(0.0, maximum.max(0.0))
}

/// Gives native offsets time to settle, then lets an exhausted inner viewport yield.
#[derive(Default)]
pub(crate) struct Progress {
    last: Option<Step>,
}
#[derive(Clone, Copy)]
struct Step {
    offset: (f32, f32),
    direction: (i8, i8),
    stalled: u8,
}
impl Progress {
    pub(crate) fn advance(&mut self, offset: (f32, f32), target: (f32, f32)) -> bool {
        let sign = |value: f32| {
            if value > 0.0 {
                1
            } else if value < 0.0 {
                -1
            } else {
                0
            }
        };
        let direction = (sign(target.0 - offset.0), sign(target.1 - offset.1));
        let stalled = match self.last {
            Some(previous)
                if previous.direction == direction
                    && (previous.offset.0 - offset.0).abs() < 0.001
                    && (previous.offset.1 - offset.1).abs() < 0.001 =>
            {
                previous.stalled.saturating_add(1)
            }
            _ => 0,
        };
        self.last = Some(Step {
            offset,
            direction,
            stalled,
        });
        stalled < 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_accelerate_in_both_directions_without_scrolling_the_center() {
        assert_eq!(velocity(50.0, 100.0), 0.0);
        assert_eq!(velocity(0.0, 100.0), -300.0);
        assert_eq!(velocity(100.0, 100.0), 300.0);
        assert_eq!(velocity(-1000.0, 100.0), -600.0);
        assert_eq!(velocity(1000.0, 100.0), 600.0);
        assert_eq!(velocity(2.0, 8.0), 0.0);
        assert_eq!(velocity(f32::NAN, 100.0), 0.0);
        assert_eq!(velocity(0.0, 0.0), 0.0);
    }

    #[test]
    fn offsets_stop_at_content_limits_and_bound_resumed_frames() {
        assert_eq!(offset(20.0, 100.0, -300.0, 1000.0), 10.4);
        assert_eq!(offset(1.0, 100.0, -300.0, 16.0), 0.0);
        assert_eq!(offset(99.0, 100.0, 300.0, 16.0), 100.0);
        assert_eq!(offset(20.0, 100.0, 300.0, -1.0), 20.0);
        assert_eq!(offset(20.0, 100.0, 300.0, f64::NAN), 20.0);
    }

    #[test]
    fn native_rounding_at_an_inner_limit_yields_and_reversing_retries() {
        let mut progress = Progress::default();
        assert!(progress.advance((0.0, 9.8), (0.0, 10.0)));
        assert!(progress.advance((0.0, 9.8), (0.0, 10.0)));
        assert!(!progress.advance((0.0, 9.8), (0.0, 10.0)));
        assert!(progress.advance((0.0, 9.8), (0.0, 8.0)));
        assert!(progress.advance((0.0, 8.0), (0.0, 6.0)));
    }
}

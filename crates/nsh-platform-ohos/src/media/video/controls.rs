//! Native playback controls remain visible while paused and expire while playing.
#[derive(Clone, Copy)]
pub(super) struct Controls {
    pub(super) visible: bool,
    hide_at: Option<f64>,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            visible: true,
            hide_at: None,
        }
    }
}
impl Controls {
    pub(super) fn show(&mut self, playing: bool, now: f64) {
        self.visible = true;
        self.hide_at = playing.then_some(now + 3000.0);
    }
    pub(super) fn toggle(&mut self, playing: bool, now: f64) {
        if self.visible {
            self.visible = false;
            self.hide_at = None;
        } else {
            self.show(playing, now);
        }
    }
    pub(super) fn tick(&mut self, now: f64) -> bool {
        if self.hide_at.is_some_and(|deadline| now >= deadline) {
            self.hide_at = None;
            self.visible = false;
        }
        self.hide_at.is_some()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paused_controls_stay_visible_and_playing_controls_expire() {
        let mut controls = Controls::default();
        controls.show(false, 0.0);
        assert!(!controls.tick(60_000.0));
        assert!(controls.visible);
        controls.show(true, 60_000.0);
        assert!(controls.tick(62_999.0));
        assert!(!controls.tick(63_000.0));
        assert!(!controls.visible);
    }
    #[test]
    fn tapping_reveals_for_a_fresh_interval_and_can_hide_immediately() {
        let mut controls = Controls::default();
        controls.toggle(true, 0.0);
        assert!(!controls.visible);
        controls.toggle(true, 1000.0);
        assert!(controls.visible);
        assert!(controls.tick(3999.0));
        assert!(!controls.tick(4000.0));
        controls.toggle(false, 5000.0);
        assert!(controls.visible);
        assert!(!controls.tick(10000.0));
    }
}

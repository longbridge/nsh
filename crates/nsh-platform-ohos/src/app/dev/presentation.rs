//! Error-presentation lifetime, independent of an engine or native widget.

#[derive(Default)]
pub(super) struct Presentation {
    message: String,
    revision: u64,
    visible: bool,
    copying: bool,
    closed: bool,
}
impl Presentation {
    pub(super) fn show(&mut self, message: &str) -> bool {
        if self.closed {
            return false;
        }
        self.revision = self.revision.wrapping_add(1);
        self.visible = true;
        self.copying = false;
        self.message = message.to_owned();
        true
    }
    pub(super) fn hide(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.visible = false;
        self.copying = false;
    }
    pub(super) fn begin_copy(&mut self) -> Option<(u64, String)> {
        if self.closed || !self.visible || self.copying {
            return None;
        }
        self.copying = true;
        Some((self.revision, self.message.clone()))
    }
    pub(super) fn finish_copy(
        &mut self,
        revision: u64,
        result: Result<(), String>,
    ) -> Option<Result<(), String>> {
        if self.closed || !self.visible || !self.copying || revision != self.revision {
            return None;
        }
        self.copying = false;
        Some(result)
    }
    pub(super) fn closed(&self) -> bool {
        self.closed
    }
    pub(super) fn close(&mut self) {
        self.hide();
        self.closed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::Presentation;

    #[test]
    fn failed_write_keeps_the_original_error_and_can_be_retried() {
        let mut state = Presentation::default();
        let original = "TypeError: 中文 🦀\0\n    at App (App.tsx:42:7)";
        assert!(state.show(original));
        let (revision, text) = state.begin_copy().unwrap();
        assert_eq!(text, original);
        assert!(state.begin_copy().is_none());
        assert_eq!(
            state.finish_copy(revision, Err("clipboard denied".into())),
            Some(Err("clipboard denied".into()))
        );
        assert!(state.visible);
        assert_eq!(state.begin_copy().unwrap().1, original);
        assert_eq!(state.finish_copy(revision, Ok(())), Some(Ok(())));
    }

    #[test]
    fn dismiss_and_reopening_the_same_error_reject_the_old_completion() {
        let mut state = Presentation::default();
        state.show("same error");
        let (old, _) = state.begin_copy().unwrap();
        state.hide();
        assert!(state.finish_copy(old, Ok(())).is_none());
        assert!(state.show("same error"));
        let (current, _) = state.begin_copy().unwrap();
        assert_ne!(current, old);
        assert!(state.finish_copy(old, Ok(())).is_none());
        assert_eq!(
            state.finish_copy(current, Err("write rejected".into())),
            Some(Err("write rejected".into()))
        );
    }

    #[test]
    fn replacement_and_close_cannot_receive_an_earlier_copy_status() {
        let mut state = Presentation::default();
        state.show("first error");
        let (first, _) = state.begin_copy().unwrap();
        state.show("replacement error");
        let (replacement, text) = state.begin_copy().unwrap();
        assert_eq!(text, "replacement error");
        assert!(
            state
                .finish_copy(first, Err("late failure".into()))
                .is_none()
        );
        state.close();
        assert!(state.closed());
        assert!(state.finish_copy(replacement, Ok(())).is_none());
        assert!(state.begin_copy().is_none());
        assert!(!state.show("disposed error"));
    }
}

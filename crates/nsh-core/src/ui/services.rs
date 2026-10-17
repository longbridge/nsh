//! The platform's services for components, modules and tasks beyond
//! dialogs and the keyboard: screen reader announcements, toasts, the
//! clipboard, image information, display frames and the layout direction
//! ([`super::ambient`]'s functions, [`super::ModuleCx`]). Each is an
//! [`Action`] the core applies in order with the component's other changes.

use std::{
    future::Future,
    pin::Pin,
    task::{Context as TaskContext, Poll},
};

use super::{
    Answer, ImageSize, PropEnum,
    task::{Action, AnswerSlot, ClipboardRead},
};
use crate::{Command, ROOT_ID, time::Instant};

/// How long a toast shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PropEnum)]
pub enum ToastDuration {
    /// About 2 seconds.
    #[default]
    Short,
    /// About 3.5 seconds.
    Long,
}

/// Whether an image loaded ahead ([`super::ambient::prefetch_image`]): true
/// once its bytes are in memory.
#[must_use = "an answer does nothing unless awaited"]
#[derive(Debug)]
pub struct Prefetch(Answer<Result<ImageSize, String>>);

impl Future for Prefetch {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<bool> {
        Pin::new(&mut self.0).poll(cx).map(|size| size.is_ok())
    }
}

/// A change of the layout direction's policy (TS: `I18nManager`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// `forceRTL`: right to left whatever the language.
    Force(bool),
    /// `allowRTL`: right to left when the language is (default true).
    Allow(bool),
}

/// The screen reader reads `message`.
pub(crate) fn announce(message: String) -> Action {
    Action::Command(ROOT_ID, Command::Announce(message))
}

/// A toast (Android; nothing happens elsewhere).
pub(crate) fn toast(message: String, duration: ToastDuration) -> Action {
    Action::Command(
        ROOT_ID,
        Command::Toast {
            text: message,
            long: duration == ToastDuration::Long,
        },
    )
}

/// Puts `text` on the system clipboard.
pub(crate) fn set_clipboard(text: String) -> Action {
    Action::Command(ROOT_ID, Command::SetClipboard(text))
}

/// The text on the system clipboard (`""` when it holds none).
pub(crate) fn clipboard_string() -> (Answer<String>, Action) {
    let (answer, slot) = Answer::new();
    (answer, Action::ReadClipboard(ClipboardRead::Text(slot)))
}

/// Whether the system clipboard holds text (without reading it).
pub(crate) fn clipboard_has_string() -> (Answer<bool>, Action) {
    let (answer, slot) = Answer::new();
    (answer, Action::ReadClipboard(ClipboardRead::Has(slot)))
}

/// The size of the image at `uri`, its bytes loaded.
pub(crate) fn image_size(uri: String) -> (Answer<Result<ImageSize, String>>, Action) {
    let (answer, slot) = Answer::new();
    (answer, Action::ImageInfo(uri, slot))
}

/// Loads the image at `uri` ahead.
pub(crate) fn prefetch(uri: String) -> (Prefetch, Action) {
    let (answer, action) = image_size(uri);
    (Prefetch(answer), action)
}

/// The start of the next display frame.
pub(crate) fn frame() -> (Answer<Instant>, Action) {
    let (answer, slot): (_, AnswerSlot<Instant>) = Answer::new();
    (answer, Action::Frame(slot))
}

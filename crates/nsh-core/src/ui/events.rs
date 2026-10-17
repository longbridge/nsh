//! Typed native events: what the `on_*` methods of elements hand their
//! listeners, decoded from the platform's [`EventPayload`].

use std::{borrow::Cow, rc::Rc, sync::Arc};

use super::element::Listener;
use crate::{EventPayload, JsonValue, PayloadValue, Rect, Size, TouchEvent};

/// A scroll view moved (`on_scroll`): the offset, and the sizes it moves in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollEvent {
    /// Horizontal content offset.
    pub x: f32,
    /// Vertical content offset.
    pub y: f32,
    /// The content's width.
    pub content_width: f32,
    /// The content's height.
    pub content_height: f32,
    /// The scroll view's width.
    pub width: f32,
    /// The scroll view's height.
    pub height: f32,
    /// How far a zooming scroll view is zoomed (1: not zoomed; the content
    /// sizes are the zoomed ones).
    pub zoom_scale: f32,
}

impl Default for ScrollEvent {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            content_width: 0.0,
            content_height: 0.0,
            width: 0.0,
            height: 0.0,
            zoom_scale: 1.0,
        }
    }
}

/// A scroll view's offset (drags, momentum, rest).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScrollOffset {
    /// Horizontal content offset.
    pub x: f32,
    /// Vertical content offset.
    pub y: f32,
}

/// A drag (`on_pan_start` / `on_pan_end`): translation since it started and
/// velocity, in points (per second).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PanEvent {
    /// Horizontal translation.
    pub translation_x: f32,
    /// Vertical translation.
    pub translation_y: f32,
    /// Horizontal velocity.
    pub velocity_x: f32,
    /// Vertical velocity.
    pub velocity_y: f32,
    /// The bound horizontal native value now (`Interactive::pan`).
    pub value_x: Option<f32>,
    /// The bound vertical native value now.
    pub value_y: Option<f32>,
}

/// A pinch or a rotation: its amount (scale, or radians) and velocity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GestureEvent {
    /// The scale (pinches) or the rotation in radians.
    pub value: f32,
    /// Per second.
    pub velocity: f32,
}

/// An input's selection (UTF-16 units; equal for a caret).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    /// Start.
    pub start: u32,
    /// End.
    pub end: u32,
}

/// Where a web view is (`on_navigation_state_change`, `on_load`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WebNavigation {
    /// The page's URL.
    pub url: String,
    /// Its title.
    pub title: String,
    /// Still loading.
    pub loading: bool,
    /// `go_back` would do something.
    pub can_go_back: bool,
    /// `go_forward` would do something.
    pub can_go_forward: bool,
}

/// A video loaded (`on_load`): its length in seconds and size in pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VideoInfo {
    /// Seconds.
    pub duration: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// A video played on (`on_progress`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VideoProgress {
    /// Seconds played.
    pub current_time: f32,
    /// Seconds in all.
    pub duration: f32,
}

/// An image's download went on (`Image::on_progress`): bytes so far and in
/// all (0 when the server does not say).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ImageProgress {
    /// Bytes loaded.
    pub loaded: f64,
    /// Bytes in all, 0 when unknown.
    pub total: f64,
}

/// An event of a registered native component (`on_native_event`).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeEvent {
    /// Its name (`regionChange`).
    pub name: Arc<str>,
    /// Its data.
    pub data: JsonValue,
}

/// A field of a map payload.
fn field<'a>(payload: &'a EventPayload, key: &str) -> Option<&'a PayloadValue> {
    match payload {
        EventPayload::Map(entries) => entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn number(payload: &EventPayload, key: &str) -> f32 {
    match field(payload, key) {
        Some(PayloadValue::Number(n)) => *n as f32,
        _ => 0.0,
    }
}

fn flag(payload: &EventPayload, key: &str) -> bool {
    matches!(field(payload, key), Some(PayloadValue::Bool(true)))
}

fn string<'a>(payload: &'a EventPayload, key: &str) -> Option<&'a str> {
    match field(payload, key) {
        Some(PayloadValue::Str(s)) => Some(s),
        _ => None,
    }
}

/// How a typed event comes out of a payload (`None`: not this event's).
pub(crate) type Decode<T> = for<'a> fn(&'a EventPayload) -> Option<Cow<'a, T>>;

/// `listener` as a listener of raw payloads: it runs with what `decode`
/// makes of each payload.
pub(crate) fn typed<T: ?Sized + ToOwned + 'static>(
    listener: Listener<T>,
    decode: Decode<T>,
) -> Listener<EventPayload> {
    let inner = listener.0;
    Listener(Rc::new(move |payload: &EventPayload| {
        if let Some(value) = decode(payload) {
            inner(&value);
        }
    }))
}

/// Events without data.
pub(crate) fn unit(_: &EventPayload) -> Option<Cow<'_, ()>> {
    Some(Cow::Owned(()))
}

pub(crate) fn layout(p: &EventPayload) -> Option<Cow<'_, Rect>> {
    match *p {
        EventPayload::Layout {
            x,
            y,
            width,
            height,
        } => Some(Cow::Owned(Rect {
            x,
            y,
            width,
            height,
        })),
        _ => None,
    }
}

pub(crate) fn text(p: &EventPayload) -> Option<Cow<'_, str>> {
    match p {
        EventPayload::Text { text, .. } => Some(Cow::Borrowed(text)),
        _ => None,
    }
}

pub(crate) fn key(p: &EventPayload) -> Option<Cow<'_, str>> {
    string(p, "key").map(Cow::Borrowed)
}

pub(crate) fn action(p: &EventPayload) -> Option<Cow<'_, str>> {
    string(p, "actionName").map(Cow::Borrowed)
}

pub(crate) fn message(p: &EventPayload) -> Option<Cow<'_, str>> {
    match p {
        EventPayload::Error { message } => Some(Cow::Borrowed(message)),
        _ => string(p, "message").map(Cow::Borrowed),
    }
}

pub(crate) fn data(p: &EventPayload) -> Option<Cow<'_, str>> {
    string(p, "data").map(Cow::Borrowed)
}

pub(crate) fn selection(p: &EventPayload) -> Option<Cow<'_, Selection>> {
    Some(Cow::Owned(Selection {
        start: number(p, "start") as u32,
        end: number(p, "end") as u32,
    }))
}

pub(crate) fn size(p: &EventPayload) -> Option<Cow<'_, Size>> {
    match *p {
        EventPayload::Image { width, height } => Some(Cow::Owned(Size::new(width, height))),
        _ => Some(Cow::Owned(Size::new(
            number(p, "width"),
            number(p, "height"),
        ))),
    }
}

pub(crate) fn scroll(p: &EventPayload) -> Option<Cow<'_, ScrollEvent>> {
    Some(Cow::Owned(match *p {
        EventPayload::Scroll { x, y } => ScrollEvent {
            x,
            y,
            ..ScrollEvent::default()
        },
        _ => ScrollEvent {
            x: number(p, "x"),
            y: number(p, "y"),
            content_width: number(p, "contentWidth"),
            content_height: number(p, "contentHeight"),
            width: number(p, "width"),
            height: number(p, "height"),
            zoom_scale: match field(p, "zoomScale") {
                Some(PayloadValue::Number(n)) => *n as f32,
                _ => 1.0,
            },
        },
    }))
}

pub(crate) fn offset(p: &EventPayload) -> Option<Cow<'_, ScrollOffset>> {
    Some(Cow::Owned(match *p {
        EventPayload::Scroll { x, y } => ScrollOffset { x, y },
        _ => ScrollOffset {
            x: number(p, "x"),
            y: number(p, "y"),
        },
    }))
}

pub(crate) fn pan(p: &EventPayload) -> Option<Cow<'_, PanEvent>> {
    match *p {
        EventPayload::Pan {
            translation_x,
            translation_y,
            velocity_x,
            velocity_y,
            value_x,
            value_y,
        } => Some(Cow::Owned(PanEvent {
            translation_x,
            translation_y,
            velocity_x,
            velocity_y,
            value_x,
            value_y,
        })),
        _ => None,
    }
}

pub(crate) fn pinch(p: &EventPayload) -> Option<Cow<'_, GestureEvent>> {
    Some(Cow::Owned(GestureEvent {
        value: number(p, "scale"),
        velocity: number(p, "velocity"),
    }))
}

pub(crate) fn rotation(p: &EventPayload) -> Option<Cow<'_, GestureEvent>> {
    Some(Cow::Owned(GestureEvent {
        value: number(p, "rotation"),
        velocity: number(p, "velocity"),
    }))
}

pub(crate) fn touch(p: &EventPayload) -> Option<Cow<'_, TouchEvent>> {
    match p {
        EventPayload::Touch(touch) => Some(Cow::Borrowed(touch)),
        _ => None,
    }
}

pub(crate) fn navigation(p: &EventPayload) -> Option<Cow<'_, WebNavigation>> {
    Some(Cow::Owned(WebNavigation {
        url: string(p, "url").unwrap_or_default().to_owned(),
        title: string(p, "title").unwrap_or_default().to_owned(),
        loading: flag(p, "loading"),
        can_go_back: flag(p, "canGoBack"),
        can_go_forward: flag(p, "canGoForward"),
    }))
}

pub(crate) fn video(p: &EventPayload) -> Option<Cow<'_, VideoInfo>> {
    Some(Cow::Owned(VideoInfo {
        duration: number(p, "duration"),
        width: number(p, "width"),
        height: number(p, "height"),
    }))
}

pub(crate) fn progress(p: &EventPayload) -> Option<Cow<'_, VideoProgress>> {
    Some(Cow::Owned(VideoProgress {
        current_time: number(p, "currentTime"),
        duration: number(p, "duration"),
    }))
}

pub(crate) fn image_progress(p: &EventPayload) -> Option<Cow<'_, ImageProgress>> {
    let number = |key| match field(p, key) {
        Some(PayloadValue::Number(n)) => *n,
        _ => 0.0,
    };
    Some(Cow::Owned(ImageProgress {
        loaded: number("loaded"),
        total: number("total"),
    }))
}

pub(crate) fn buffering(p: &EventPayload) -> Option<Cow<'_, bool>> {
    Some(Cow::Owned(flag(p, "isBuffering")))
}

pub(crate) fn native(p: &EventPayload) -> Option<Cow<'_, NativeEvent>> {
    match p {
        EventPayload::Native { name, data } => Some(Cow::Owned(NativeEvent {
            name: name.clone(),
            data: data.clone(),
        })),
        _ => None,
    }
}

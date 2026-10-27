use nsh_core::{EventKind, EventPayload, NodeId, Point, Points, PressEvent, input::Press};
use nsh_engine::EngineControl;
use ratatui_core::layout::{Position, Rect};

use crate::{CellSize, mount::Mounter, render::Hit};

pub use nsh_core::keymap::Modifiers;

/// Whether the surface used an event. Ignored events remain the embedder's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// Used by this surface.
    Yes,
    /// Available for another widget or the embedding application.
    Ignored,
}

/// A terminal mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseButton {
    /// Primary button.
    #[default]
    Left,
    /// Secondary button.
    Right,
    /// Middle button.
    Middle,
}

/// A terminal pointer action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseKind {
    /// Button pressed.
    Down(MouseButton),
    /// Button released.
    Up(MouseButton),
    /// Motion, including motion with a button held.
    #[default]
    Move,
    /// Wheel steps, positive right and down; one step moves one Cell on the selected axis.
    Wheel {
        /// Horizontal steps.
        x: i16,
        /// Vertical steps.
        y: i16,
    },
}

/// Pointer coordinates are absolute terminal Cells, including the surface's offset.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct MouseInput {
    /// Action.
    pub kind: MouseKind,
    /// Column in the terminal.
    pub column: u16,
    /// Row in the terminal.
    pub row: u16,
    /// Modifier keys held.
    pub modifiers: Modifiers,
}

/// Input independent of the embedder's terminal backend. Resizing comes from rendering an area.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Input {
    /// Mouse input.
    Mouse(MouseInput),
    /// A key name as in NSH's keymap (`Enter`, `Escape`, or a character).
    Key {
        /// Key name.
        key: String,
        /// Held modifiers.
        modifiers: Modifiers,
        /// True for a release when the terminal reports it.
        release: bool,
    },
    /// A repeated key press identified by the terminal protocol.
    KeyRepeat {
        /// Key name, as for `Key`.
        key: String,
        /// Held modifiers.
        modifiers: Modifiers,
    },
    /// One bracketed paste.
    Paste(String),
    /// The embedding application gave or removed keyboard focus.
    Focus(bool),
}

#[derive(Default)]
pub(crate) struct Pointer {
    pressed: Option<(Press<NodeId>, Point<Points>)>,
    backdrop: Option<NodeId>,
}

impl Pointer {
    pub(crate) fn cancel(&mut self, control: Option<&EngineControl>) {
        self.backdrop = None;
        if let Some((mut press, _)) = self.pressed.take()
            && let Some(control) = control
        {
            for event in press.press_out(false) {
                control.dispatch_event(press.node, event, EventPayload::None);
            }
        }
    }

    pub(crate) fn handle(
        &mut self,
        mouse: MouseInput,
        area: Rect,
        cells: CellSize,
        hits: &[Hit],
        tree: &mut Mounter,
        control: &EngineControl,
    ) -> Handled {
        let scope = crate::navigation::focus_scope(tree);
        let modal = scope != nsh_core::ROOT_ID;
        let unused = if modal {
            Handled::Yes
        } else {
            Handled::Ignored
        };
        if self
            .pressed
            .as_ref()
            .is_some_and(|(press, _)| !crate::keyboard::contains(tree, scope, press.node))
        {
            self.cancel(Some(control));
        }
        let position = Position::new(mouse.column, mouse.row);
        let hit = hits.iter().rev().find(|hit| {
            hit.area.contains(position) && crate::keyboard::contains(tree, scope, hit.node)
        });
        let backdrop = modal && hit.is_none_or(|hit| hit.node == scope && hit.target.is_none());
        if matches!(mouse.kind, MouseKind::Up(MouseButton::Left))
            && let Some(id) = self.backdrop.take()
        {
            if scope == id && area.contains(position) && backdrop {
                crate::navigation::request_close(
                    tree,
                    control,
                    id,
                    crate::navigation::CloseReason::Backdrop,
                );
            }
            return Handled::Yes;
        }
        if !area.contains(position) {
            if matches!(mouse.kind, MouseKind::Up(MouseButton::Left)) {
                self.cancel(Some(control));
            }
            return Handled::Ignored;
        }
        let page = Point::new(
            cells.width() * (f32::from(mouse.column - area.x) + 0.5),
            cells.height() * (f32::from(mouse.row - area.y) + 0.5),
        );
        if backdrop {
            if matches!(mouse.kind, MouseKind::Down(MouseButton::Left)) {
                self.cancel(Some(control));
                self.backdrop = Some(scope);
            }
            return Handled::Yes;
        }
        match mouse.kind {
            MouseKind::Down(MouseButton::Left) => {
                self.cancel(Some(control));
                let Some((id, origin)) = hit.and_then(|hit| hit.target.map(|id| (id, hit.origin)))
                else {
                    return unused;
                };
                let Some(view) = tree.get(id) else {
                    return unused;
                };
                let mut press = Press::new(id, id, view.events, view.props.long_press_delay, 0)
                    .clicked(mouse.modifiers, 1);
                if let Some(event) = press.press_in() {
                    dispatch(control, &press, event, page, origin);
                }
                self.pressed = Some((press, origin));
                Handled::Yes
            }
            MouseKind::Up(MouseButton::Left) => {
                let Some((mut press, origin)) = self.pressed.take() else {
                    return unused;
                };
                if tree.get(press.node).is_none() {
                    return unused;
                }
                let fire = hit.is_some_and(|hit| hit.target == Some(press.node));
                for event in press.press_out(fire) {
                    dispatch(control, &press, event, page, origin);
                }
                Handled::Yes
            }
            _ => unused,
        }
    }
}

fn dispatch(
    control: &EngineControl,
    press: &Press<NodeId>,
    event: EventKind,
    page: Point<Points>,
    origin: Point<Points>,
) {
    control.dispatch_event(
        press.node,
        event,
        EventPayload::Press(PressEvent {
            position: page - origin,
            page_position: page,
            modifiers: press.modifiers,
            clicks: press.clicks,
        }),
    );
}

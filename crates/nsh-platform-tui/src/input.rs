use nsh_core::{EventKind, EventPayload, NodeId, Point, Points, PressEvent, input::Press};
use nsh_engine::EngineControl;
use ratatui_core::layout::{Position, Rect};

use crate::{CellSize, mount::Mounter, render::Hit};

pub use nsh_core::keymap::Modifiers;

pub(crate) const WHEEL_ROWS: f32 = 3.0;
pub(crate) const WHEEL_COLUMNS: f32 = 6.0;

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
    /// Wheel steps, positive right and down: three rows vertically, six columns horizontally.
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
    hovered: Vec<NodeId>,
    last: Option<MouseInput>,
    drag: Option<Drag>,
    active_states: Vec<NodeId>,
}

struct Drag {
    target: NodeId,
    origin: Point<Points>,
    start: Point<Points>,
    page: Point<Points>,
    started: std::time::Instant,
    path: Vec<NodeId>,
    candidates: Vec<(NodeId, nsh_core::input::Candidate)>,
    gate: Option<nsh_core::input::Gate>,
    active: Option<(NodeId, nsh_core::input::Candidate)>,
    velocity: nsh_core::input::VelocityTracker,
    activated: Option<NodeId>,
    scroll_start: Points,
}

impl Drag {
    fn report(&self, control: &EngineControl, phase: nsh_core::TouchPhase) {
        if self.path.is_empty() {
            return;
        }
        control.dispatch_event(
            nsh_core::ROOT_ID,
            EventKind::Touch,
            EventPayload::Touch(Box::new(nsh_core::TouchEvent {
                phase,
                timestamp: self.started.elapsed().as_secs_f64() * 1000.0,
                activated: self.activated,
                points: vec![nsh_core::TouchPoint {
                    id: 0,
                    target: self.target,
                    path: if phase == nsh_core::TouchPhase::Start {
                        self.path.clone()
                    } else {
                        Vec::new()
                    },
                    page: self.page,
                    location: self.page - self.origin,
                    changed: true,
                }],
            })),
        );
    }
}

impl Pointer {
    pub(crate) fn leave(&mut self, tree: &mut Mounter, control: &EngineControl) {
        self.last = None;
        self.set_hovered(Vec::new(), tree, control);
    }

    fn set_hovered(&mut self, hovered: Vec<NodeId>, tree: &mut Mounter, control: &EngineControl) {
        let (left, entered) = nsh_core::input::hover_changes(&self.hovered, &hovered);
        for (&id, active, event) in left
            .into_iter()
            .map(|id| (id, false, EventKind::HoverOut))
            .chain(entered.into_iter().map(|id| (id, true, EventKind::HoverIn)))
        {
            if let Some(view) = tree.get_mut(id) {
                let bit = nsh_core::tailwind::state::HOVERED;
                if active {
                    view.states |= bit;
                } else {
                    view.states &= !bit;
                }
                if view.tracked & bit != 0 {
                    control.set_states(id, view.states);
                }
                if view.events.contains(event) {
                    control.dispatch_event(id, event, EventPayload::None);
                }
            }
        }
        self.hovered = hovered;
    }

    pub(crate) fn rehover(
        &mut self,
        area: Rect,
        hits: &[Hit],
        tree: &mut Mounter,
        control: &EngineControl,
    ) {
        if self
            .drag
            .as_ref()
            .is_some_and(|drag| !visible_target(tree, drag.target))
        {
            self.cancel(tree, Some(control));
        }
        let Some(mouse) = self.last else {
            return;
        };
        let position = Position::new(mouse.column, mouse.row);
        let scope = crate::navigation::focus_scope(tree);
        let mut hovered = Vec::new();
        let mut node = area
            .contains(position)
            .then(|| {
                hits.iter().rev().find(|hit| {
                    hit.area.contains(position) && crate::keyboard::contains(tree, scope, hit.node)
                })
            })
            .flatten()
            .map(|hit| hit.node);
        while let Some(id) = node {
            let Some(view) = tree.get(id) else {
                break;
            };
            if nsh_core::input::hoverable(view.tracked, view.events) {
                hovered.push(id);
            }
            node = if id == scope { None } else { view.parent };
        }
        self.set_hovered(hovered, tree, control);
    }

    pub(crate) fn cancel(&mut self, tree: &mut Mounter, control: Option<&EngineControl>) {
        self.backdrop = None;
        self.release_states(tree, control);
        if let Some(drag) = self.drag.take()
            && let Some(control) = control
        {
            if let Some((id, nsh_core::input::Candidate::Pan { .. })) = drag.active {
                pan_event(
                    tree,
                    control,
                    id,
                    EventKind::PanEnd,
                    drag.page - drag.start,
                    &drag.velocity,
                );
            }
            drag.report(control, nsh_core::TouchPhase::Cancel);
        }
        if let Some((mut press, _)) = self.pressed.take()
            && let Some(control) = control
        {
            for event in press.press_out(false) {
                control.dispatch_event(press.node, event, EventPayload::None);
            }
        }
    }

    fn release_states(&mut self, tree: &mut Mounter, control: Option<&EngineControl>) {
        for id in self.active_states.drain(..) {
            if let Some(view) = tree.get_mut(id) {
                view.states &= !nsh_core::tailwind::state::PRESSED;
                if let Some(control) = control {
                    control.set_states(id, view.states);
                }
            }
        }
    }

    pub(crate) fn handle(
        &mut self,
        mouse: MouseInput,
        area: Rect,
        cells: CellSize,
        hits: &[Hit],
        geometry: &[crate::render::Geometry],
        tree: &mut Mounter,
        control: &EngineControl,
    ) -> Handled {
        self.last = Some(mouse);
        self.rehover(area, hits, tree, control);
        if matches!(mouse.kind, MouseKind::Move) {
            let page = Point::new(
                cells.width() * (f32::from(mouse.column) - f32::from(area.x) + 0.5),
                cells.height() * (f32::from(mouse.row) - f32::from(area.y) + 0.5),
            );
            for &id in &self.hovered {
                if tree
                    .get(id)
                    .is_some_and(|view| view.events.contains(EventKind::HoverMove))
                {
                    let origin = geometry
                        .iter()
                        .find(|g| g.id == id)
                        .map_or_else(Point::default, |g| g.frame.origin);
                    control.dispatch_event(
                        id,
                        EventKind::HoverMove,
                        EventPayload::Hover(nsh_core::HoverEvent {
                            position: page - origin,
                            page_position: page,
                            modifiers: mouse.modifiers,
                        }),
                    );
                }
            }
        }
        let scope = crate::navigation::focus_scope(tree);
        let modal = scope != nsh_core::ROOT_ID;
        let unused = if modal || !self.hovered.is_empty() {
            Handled::Yes
        } else {
            Handled::Ignored
        };
        if self
            .pressed
            .as_ref()
            .is_some_and(|(press, _)| !crate::keyboard::contains(tree, scope, press.node))
        {
            self.cancel(tree, Some(control));
        }
        let position = Position::new(mouse.column, mouse.row);
        let page = Point::new(
            cells.width() * (f32::from(mouse.column) - f32::from(area.x) + 0.5),
            cells.height() * (f32::from(mouse.row) - f32::from(area.y) + 0.5),
        );
        if self
            .drag
            .as_ref()
            .is_some_and(|drag| !crate::keyboard::contains(tree, scope, drag.target))
        {
            self.cancel(tree, Some(control));
        }
        if matches!(
            mouse.kind,
            MouseKind::Move | MouseKind::Up(MouseButton::Left)
        ) && let Some(mut drag) = self.drag.take()
        {
            drag.page = page;
            drag.velocity
                .add(drag.started.elapsed().as_secs_f64() * 1000.0, page);
            let moved = page - drag.start;
            if moved.x.abs().max(moved.y.abs()) > nsh_core::input::PRESS_SLOP {
                self.release_states(tree, Some(control));
                if let Some((mut press, origin)) = self.pressed.take() {
                    for event in press.press_out(false) {
                        dispatch(control, &press, event, page, origin);
                    }
                }
            }
            if tree.responder.is_some()
                && let Some((id, nsh_core::input::Candidate::Pan { .. })) = drag.active
            {
                pan_event(tree, control, id, EventKind::PanEnd, moved, &drag.velocity);
                drag.active = None;
            }
            drag.activated =
                nsh_core::input::Gate::step(&mut drag.gate, &mut drag.candidates, moved.x, moved.y);
            if drag.active.is_none()
                && tree.responder.is_none()
                && moved.x.abs().max(moved.y.abs()) > nsh_core::input::PAN_SLOP
            {
                drag.active = nsh_core::input::startable(
                    &drag.candidates,
                    drag.gate.as_ref(),
                    moved.x,
                    moved.y,
                )
                .copied();
                if let Some((id, nsh_core::input::Candidate::Scroll { horizontal })) = drag.active {
                    if let Some(scroll) = tree.get(id).and_then(|view| view.scroll.as_ref()) {
                        drag.scroll_start = if horizontal { scroll.x } else { scroll.y };
                    }
                    drag.report(control, nsh_core::TouchPhase::Cancel);
                    drag.path.clear();
                }
                if let Some((id, nsh_core::input::Candidate::Pan { .. })) = drag.active {
                    tree.animator.pan_began(id);
                    pan_event(
                        tree,
                        control,
                        id,
                        EventKind::PanStart,
                        Point::default(),
                        &drag.velocity,
                    );
                }
            }
            if let Some((id, candidate)) = drag.active {
                self.release_states(tree, Some(control));
                if let Some((mut press, origin)) = self.pressed.take() {
                    for event in press.press_out(false) {
                        dispatch(control, &press, event, page, origin);
                    }
                }
                match candidate {
                    nsh_core::input::Candidate::Pan { .. } => tree.animator.pan_moved(id, moved),
                    nsh_core::input::Candidate::Scroll { horizontal } => {
                        if tree.get(id).is_some_and(|view| view.scroll.is_some()) {
                            let to = drag.scroll_start - if horizontal { moved.x } else { moved.y };
                            crate::keyboard::move_scroll(tree, id, to);
                        }
                    }
                }
            }
            if matches!(mouse.kind, MouseKind::Move) {
                drag.report(control, nsh_core::TouchPhase::Move);
                self.drag = Some(drag);
                return Handled::Yes;
            }
            if let Some((id, nsh_core::input::Candidate::Pan { .. })) = drag.active {
                pan_event(tree, control, id, EventKind::PanEnd, moved, &drag.velocity);
            }
            self.release_states(tree, Some(control));
            drag.report(control, nsh_core::TouchPhase::End);
            if self.pressed.is_none() {
                return if !area.contains(position) && drag.path.is_empty() && drag.active.is_none()
                {
                    Handled::Ignored
                } else {
                    Handled::Yes
                };
            }
        }
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
                self.cancel(tree, Some(control));
            }
            return Handled::Ignored;
        }
        if backdrop {
            if matches!(mouse.kind, MouseKind::Down(MouseButton::Left)) {
                self.cancel(tree, Some(control));
                self.backdrop = Some(scope);
            }
            return Handled::Yes;
        }
        match mouse.kind {
            MouseKind::Down(MouseButton::Left) => {
                self.cancel(tree, Some(control));
                if let Some(hit) = hit {
                    let mut grab = nsh_core::input::Grab::default();
                    let mut node = Some(hit.node);
                    while let Some(id) = node {
                        let Some(view) = tree.get(id) else {
                            break;
                        };
                        if !view.props.accessibility.state.disabled {
                            grab.add(
                                &id,
                                &nsh_core::input::Link::new(
                                    id,
                                    view.events,
                                    view.tracked,
                                    view.gestures,
                                    view.scroll.as_ref().map(|s| s.horizontal),
                                    view.props.pan_activation,
                                ),
                                nsh_core::input::GrabOptions::new(true, false, false, true),
                            );
                        }
                        node = if id == scope { None } else { view.parent };
                    }
                    self.active_states = grab.pressed;
                    for &id in &self.active_states {
                        if let Some(view) = tree.get_mut(id) {
                            view.states |= nsh_core::tailwind::state::PRESSED;
                            control.set_states(id, view.states);
                        }
                    }
                    if !grab.path.is_empty()
                        || !grab.candidates.is_empty()
                        || hit.target.is_some()
                        || !self.active_states.is_empty()
                    {
                        let mut drag = Drag {
                            target: hit.node,
                            origin: hit.node_origin,
                            start: page,
                            page,
                            started: std::time::Instant::now(),
                            path: grab.path,
                            candidates: grab.candidates,
                            gate: grab.gate,
                            active: None,
                            velocity: Default::default(),
                            activated: None,
                            scroll_start: Points::ZERO,
                        };
                        drag.velocity.add(0.0, page);
                        drag.report(control, nsh_core::TouchPhase::Start);
                        self.drag = Some(drag);
                    }
                }
                let Some((id, origin)) = hit.and_then(|hit| hit.target.map(|id| (id, hit.origin)))
                else {
                    return if self.drag.is_some() {
                        Handled::Yes
                    } else {
                        unused
                    };
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

fn pan_event(
    tree: &Mounter,
    control: &EngineControl,
    id: NodeId,
    kind: EventKind,
    translation: Point<Points>,
    velocity: &nsh_core::input::VelocityTracker,
) {
    if !tree.get(id).is_some_and(|view| view.events.contains(kind)) {
        return;
    }
    let (value_x, value_y) = tree.animator.pan_values(id);
    let (velocity_x, velocity_y) = velocity.velocity();
    control.dispatch_event(
        id,
        kind,
        EventPayload::Pan {
            translation,
            velocity_x,
            velocity_y,
            value_x: value_x.map(Points),
            value_y: value_y.map(Points),
        },
    );
}

fn visible_target(tree: &Mounter, mut id: NodeId) -> bool {
    let scope = crate::navigation::focus_scope(tree);
    loop {
        let Some(view) = tree.get(id) else {
            return false;
        };
        if view.concealed || view.style.hidden || view.style.opacity <= 0.0 {
            return false;
        }
        if id == scope {
            return true;
        }
        let Some(parent) = view.parent else {
            return false;
        };
        id = parent;
    }
}

//! Keyboard focus uses the same projected geometry as drawing and pointer input.

use nsh_core::{
    EventKind, EventPayload, NodeId, PayloadValue, ROOT_ID, ViewKind,
    input::{has_press, next_states},
    keymap::{KeyMatch, Keystroke},
    tailwind::state::{FOCUS_WITHIN, FOCUSED},
};
use nsh_engine::EngineControl;
use ratatui_core::layout::Position;

use crate::{Handled, key_scheme::Action, mount::Mounter, render::Geometry};

#[derive(Default)]
pub(crate) struct Keyboard {
    pub(crate) focused: Option<NodeId>,
}

pub(crate) fn contains(tree: &Mounter, ancestor: NodeId, mut id: NodeId) -> bool {
    loop {
        if id == ancestor {
            return true;
        }
        let Some(parent) = tree.get(id).and_then(|view| view.parent) else {
            return false;
        };
        id = parent;
    }
}

impl Keyboard {
    pub(crate) fn candidates<'a>(
        &self,
        tree: &Mounter,
        geometry: &'a [Geometry],
    ) -> Vec<&'a Geometry> {
        let scope = crate::navigation::focus_scope(tree);
        let mut candidates: Vec<_> = geometry
            .iter()
            .filter(|g| {
                contains(tree, scope, g.id)
                    && tree.get(g.id).is_some_and(|view| {
                        let editable = view.input.as_ref().is_some_and(|config| config.editable);
                        !view.props.accessibility.state.disabled
                            && view.props.focus_stop(has_press(view.events) || editable)
                            && g.bounds.right > g.bounds.left
                            && g.bounds.bottom > g.bounds.top
                    })
            })
            .collect();
        candidates.sort_by_key(|g| (g.bounds.top, g.bounds.left));
        candidates
    }

    pub(crate) fn focus(
        &mut self,
        id: Option<NodeId>,
        tree: &mut Mounter,
        control: &EngineControl,
    ) {
        let old = self.focused;
        self.focused = id;
        self.sync_states(tree, control);
        if old != id {
            if let Some(old) = old {
                control.dispatch_event(old, EventKind::Blur, EventPayload::None);
            }
            if let Some(id) = id {
                control.dispatch_event(id, EventKind::Focus, EventPayload::None);
            }
        }
    }

    fn sync_states(&self, tree: &mut Mounter, control: &EngineControl) {
        for index in 0..tree.views.len() {
            let id = index as NodeId;
            let Some(view) = tree.get(id) else {
                continue;
            };
            if view.tracked & (FOCUSED | FOCUS_WITHIN) == 0 {
                continue;
            }
            let mut states = view.states;
            for (bit, on) in [
                (FOCUSED, self.focused == Some(id)),
                (
                    FOCUS_WITHIN,
                    self.focused
                        .is_some_and(|focused| contains(tree, id, focused)),
                ),
            ] {
                if let Some(next) = next_states(view.tracked, states, bit, on) {
                    states = next;
                }
            }
            if states != view.states {
                tree.get_mut(id).unwrap().states = states;
                control.set_states(id, states);
            }
        }
    }

    pub(crate) fn reconcile(
        &mut self,
        tree: &mut Mounter,
        geometry: &[Geometry],
        control: &EngineControl,
    ) {
        if self
            .focused
            .is_some_and(|id| !self.candidates(tree, geometry).iter().any(|g| g.id == id))
        {
            self.focus(None, tree, control);
        } else {
            self.sync_states(tree, control);
        }
    }

    pub(crate) fn cursor(&self, geometry: &[Geometry]) -> Option<Position> {
        let g = geometry
            .iter()
            .find(|g| Some(g.id) == self.focused && !g.visible.is_empty())?;
        Some(Position::new(g.visible.x, g.visible.y))
    }

    pub(crate) fn shortcut(
        &self,
        tree: &Mounter,
        stroke: Keystroke,
        repeat: bool,
        control: &EngineControl,
    ) -> bool {
        let editing = self
            .focused
            .and_then(|id| tree.get(id))
            .is_some_and(|v| v.kind == ViewKind::Input);
        let found = tree.keymap.find(stroke, editing, repeat);
        if let KeyMatch::Fire(id) = found {
            control.dispatch_event(
                ROOT_ID,
                EventKind::KeyCommand,
                EventPayload::Value(PayloadValue::Number(id as f64)),
            );
        }
        found.consumed()
    }

    pub(crate) fn action(
        &mut self,
        action: Action,
        tree: &mut Mounter,
        geometry: &[Geometry],
        control: &EngineControl,
        modifiers: nsh_core::keymap::Modifiers,
    ) -> Handled {
        match action {
            Action::Next | Action::Previous | Action::Left | Action::Right => {
                let candidates = self.candidates(tree, geometry);
                if candidates.is_empty() {
                    return Handled::Ignored;
                }
                let current = candidates.iter().position(|g| Some(g.id) == self.focused);
                let next = match action {
                    Action::Next | Action::Previous => nsh_core::input::tab_stop(
                        current,
                        candidates.len(),
                        matches!(action, Action::Previous),
                        true,
                    ),
                    _ => current
                        .and_then(|index| {
                            let from = candidates[index];
                            candidates
                                .iter()
                                .enumerate()
                                .filter(|(_, to)| {
                                    to.bounds.top < from.bounds.bottom
                                        && from.bounds.top < to.bounds.bottom
                                        && if matches!(action, Action::Right) {
                                            to.bounds.left > from.bounds.left
                                        } else {
                                            to.bounds.left < from.bounds.left
                                        }
                                })
                                .min_by_key(|(_, to)| {
                                    (to.bounds.left - from.bounds.left).unsigned_abs()
                                })
                                .map(|(index, _)| index)
                        })
                        .or(current.is_none().then_some(0)),
                };
                if let Some(index) = next {
                    let id = candidates[index].id;
                    self.focus(Some(id), tree, control);
                    reveal(tree, geometry, id);
                }
                Handled::Yes
            }
            Action::Activate => {
                let Some(g) = geometry.iter().find(|g| Some(g.id) == self.focused) else {
                    return Handled::Ignored;
                };
                let Some(view) = tree.get(g.id) else {
                    return Handled::Ignored;
                };
                if !has_press(view.events) {
                    return Handled::Ignored;
                }
                let position =
                    nsh_core::Point::new(g.frame.size.width / 2.0, g.frame.size.height / 2.0);
                let payload = EventPayload::Press(nsh_core::PressEvent {
                    position,
                    page_position: g.frame.origin + position,
                    modifiers,
                    clicks: 1,
                });
                for event in [EventKind::PressDown, EventKind::PressUp, EventKind::Press] {
                    if view.events.contains(event) {
                        control.dispatch_event(g.id, event, payload.clone());
                    }
                }
                Handled::Yes
            }
            Action::Back => {
                if crate::navigation::back(tree, control, true) {
                    Handled::Yes
                } else {
                    Handled::Ignored
                }
            }
            Action::PageDown | Action::PageUp | Action::Start | Action::End => {
                let target = self
                    .focused
                    .and_then(|id| scroll_ancestor(tree, id))
                    .or_else(|| {
                        geometry
                            .iter()
                            .find(|g| {
                                contains(tree, crate::navigation::focus_scope(tree), g.id)
                                    && tree.get(g.id).is_some_and(|v| v.scroll.is_some())
                            })
                            .map(|g| g.id)
                    });
                let Some(id) = target else {
                    return Handled::Ignored;
                };
                let view = tree.get_mut(id).unwrap();
                let scroll = view.scroll.as_mut().unwrap();
                let (offset, max, page) = if scroll.horizontal {
                    (
                        scroll.x,
                        scroll.max(view.frame.size).x,
                        view.frame.size.width,
                    )
                } else {
                    (
                        scroll.y,
                        scroll.max(view.frame.size).y,
                        view.frame.size.height,
                    )
                };
                let to = match action {
                    Action::PageDown => offset + page,
                    Action::PageUp => offset - page,
                    Action::Start => nsh_core::Points::ZERO,
                    Action::End => max,
                    _ => unreachable!(),
                };
                move_scroll(tree, id, to);
                Handled::Yes
            }
            Action::Quit => {
                tree.exit = true;
                Handled::Yes
            }
        }
    }
}

fn scroll_ancestor(tree: &Mounter, mut id: NodeId) -> Option<NodeId> {
    loop {
        let view = tree.get(id)?;
        if view.scroll.is_some() {
            return Some(id);
        }
        id = view.parent?;
    }
}

fn move_scroll(tree: &mut Mounter, id: NodeId, to: nsh_core::Points) -> nsh_core::Points {
    let view = tree.get_mut(id).unwrap();
    let scroll = view.scroll.as_mut().unwrap();
    let max = scroll.max(view.frame.size);
    let (offset, max) = if scroll.horizontal {
        (&mut scroll.x, max.x)
    } else {
        (&mut scroll.y, max.y)
    };
    let old = *offset;
    *offset = to.max(nsh_core::Points::ZERO).min(max);
    let delta = *offset - old;
    if delta != nsh_core::Points::ZERO && !tree.scrolled.contains(&id) {
        tree.scrolled.push(id);
    }
    delta
}

fn reveal(tree: &mut Mounter, geometry: &[Geometry], id: NodeId) {
    let Some(target) = geometry.iter().find(|g| g.id == id) else {
        return;
    };
    let mut origin = target.frame.origin;
    let mut parent = tree.get(id).and_then(|view| view.parent);
    while let Some(id) = parent {
        let view = tree.get(id).unwrap();
        parent = view.parent;
        let Some(scroll) = &view.scroll else {
            continue;
        };
        let Some(g) = geometry.iter().find(|g| g.id == id) else {
            continue;
        };
        let horizontal = scroll.horizontal;
        let relative = origin - g.frame.origin + g.scroll_origin - scroll.origin();
        let (start, len, viewport, offset) = if horizontal {
            (
                relative.x,
                target.frame.size.width,
                view.frame.size.width,
                scroll.x,
            )
        } else {
            (
                relative.y,
                target.frame.size.height,
                view.frame.size.height,
                scroll.y,
            )
        };
        if start < nsh_core::Points::ZERO || start + len > viewport {
            let delta = nsh_core::scroll::reveal_delta(start, start + len, viewport);
            let moved = move_scroll(tree, id, offset + delta);
            if horizontal {
                origin.x -= moved;
            } else {
                origin.y -= moved;
            }
        }
    }
}

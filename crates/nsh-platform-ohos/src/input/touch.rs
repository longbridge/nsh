//! Surface-local touch paths and native pan activation. No callback waits for JS.
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use nsh_core::{EventKind, EventPayload, PointerEvents, TouchEvent, TouchPhase, TouchPoint};
use ohos_arkui_binding::arkui_input_binding::{ArkUIInputEvent, UIInputAction};

use crate::render::node::{Node, Result};

struct Finger {
    point: TouchPoint,
    node: Weak<Node>,
    start: (f32, f32),
    pans: Vec<(Weak<Node>, bool)>,
}
struct KeyboardTap {
    finger: u32,
    start: (f32, f32),
    input: Weak<Node>,
}
struct LastTap {
    time_ms: f64,
    window: (f32, f32),
    clicks: u32,
    event_time_ns: i64,
}
struct BackSwipe {
    screen: Weak<crate::navigation::stack::Screen>,
    target: Weak<Node>,
    finger: u32,
    start: (f32, f32),
    local_x: f32,
    sign: f32,
    width: f32,
    active: bool,
    last: (f32, f64),
    velocity: f32,
}
struct SelectionDrag {
    owner: Weak<Node>,
    finger: u32,
    anchor: usize,
    start: (f32, f32),
    moved: bool,
    pointer: Option<(f32, f32)>,
    frame_time: f64,
    blocked: Vec<Weak<crate::scroll::controller::Scroller>>,
}
impl SelectionDrag {
    fn unblock(&self) -> Result<()> {
        let mut failure = None;
        for scroll in self.blocked.iter().filter_map(Weak::upgrade) {
            if let Err(error) = scroll.block(false) {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
#[derive(Default)]
pub(crate) struct Touches {
    fingers: Vec<Finger>,
    activated: Option<Weak<Node>>,
    timestamp: f64,
    keyboard_tap: Option<KeyboardTap>,
    suppressed_presses: Vec<nsh_core::NodeId>,
    back: Option<BackSwipe>,
    primary: Option<u32>,
    selection: Option<SelectionDrag>,
    last_tap: Option<LastTap>,
}
impl Touches {
    pub(crate) fn record_down(&mut self, scale: f32, input: &ArkUIInputEvent) {
        if input.action != UIInputAction::Down || input.pointer_count() != 1 {
            return;
        }
        let event_time_ns = input.event_time();
        if self
            .last_tap
            .as_ref()
            .is_some_and(|last| last.event_time_ns == event_time_ns)
        {
            return;
        }
        let time_ms = event_time_ns as f64 / 1_000_000.0;
        let window = (
            input.pointer_window_x_by_index(0),
            input.pointer_window_y_by_index(0),
        );
        let max_dist = 24.0 * scale.max(1.0);
        let clicks = match &self.last_tap {
            Some(last)
                if time_ms >= last.time_ms
                    && time_ms - last.time_ms <= 350.0
                    && (window.0 - last.window.0).hypot(window.1 - last.window.1) <= max_dist =>
            {
                (last.clicks % 3) + 1
            }
            _ => 1,
        };
        self.last_tap = Some(LastTap {
            time_ms,
            window,
            clicks,
            event_time_ns,
        });
    }
    pub(crate) fn clicks(&self) -> u32 {
        self.last_tap.as_ref().map_or(1, |tap| tap.clicks)
    }
    pub(crate) fn cancel(&mut self, root: &Node) -> Result<()> {
        self.primary = None;
        self.last_tap = None;
        if let Some(drag) = self.selection.take() {
            drag.unblock()?;
            if let Some(owner) = drag.owner.upgrade() {
                owner.selection_menu_show(false)?;
            }
        }
        root.clear_text_selection()?;
        if let Some(back) = self.back.take()
            && back.active
            && let Some(screen) = back.screen.upgrade()
        {
            screen.finish_swipe(false)?;
            if let Some(owner) = screen.owner() {
                block(&owner, false)?;
            }
        }
        self.keyboard_tap = None;
        self.suppressed_presses.clear();
        if !self.fingers.is_empty() {
            let points = self
                .fingers
                .iter_mut()
                .map(|f| {
                    f.point.changed = true;
                    f.point.clone()
                })
                .collect();
            root.control.dispatch_event(
                nsh_core::ROOT_ID,
                EventKind::Touch,
                EventPayload::Touch(Box::new(TouchEvent {
                    phase: TouchPhase::Cancel,
                    timestamp: self.timestamp,
                    points,
                    activated: None,
                })),
            );
            self.fingers.clear();
        }
        if let Some(node) = self.activated.take().and_then(|n| n.upgrade()) {
            block(&node, false)?;
        }
        Ok(())
    }

    pub(crate) fn input(&mut self, root: &Rc<Node>, input: &ArkUIInputEvent) -> Result<()> {
        self.record_down(root.scale, input);
        self.timestamp = input.event_time() as f64 / 1_000_000.0;
        let phase = match input.action {
            UIInputAction::Down => TouchPhase::Start,
            UIInputAction::Move => TouchPhase::Move,
            UIInputAction::Up => TouchPhase::End,
            UIInputAction::Cancel => TouchPhase::Cancel,
        };
        let count = input.pointer_count().min(32);
        if phase == TouchPhase::Start {
            self.keyboard_tap = None;
            if count == 1 {
                self.suppressed_presses.clear();
            }
        }
        let changed = if matches!(phase, TouchPhase::Start | TouchPhase::End) {
            input.get_changed_pointer_id().ok()
        } else {
            None
        };
        let phase = if matches!(phase, TouchPhase::Start | TouchPhase::End) && changed.is_none() {
            TouchPhase::Cancel
        } else {
            phase
        };
        let mut surface = root.clone();
        loop {
            let parent = surface.parent_node.borrow().upgrade();
            let Some(parent) = parent else { break };
            surface = parent;
        }
        let origin = surface.native.borrow().layout_position_in_window()?;
        if phase == TouchPhase::Start && count == 1 {
            self.primary = changed;
        }
        if self.selection_input(root, input, phase, changed, count)? {
            return Ok(());
        }
        if matches!(phase, TouchPhase::End | TouchPhase::Cancel) {
            self.primary = None;
        }
        if self.back_input(root, input, phase, changed, count)? {
            return Ok(());
        }
        // ArkUI returns coordinates in the target node's configured metric unit.
        // NSH nodes use vp; native layout-position helpers return physical pixels.
        for i in 0..count {
            let id = input.pointer_id(i) as u32;
            let local = (input.pointer_x_by_index(i), input.pointer_y_by_index(i));
            let window = (
                input.pointer_window_x_by_index(i),
                input.pointer_window_y_by_index(i),
            );
            let page = (
                window.0 - origin.x as f32 / root.scale,
                window.1 - origin.y as f32 / root.scale,
            );
            if phase == TouchPhase::Start && changed == Some(id) {
                let Some(node) = hit(root, local, &nsh_core::Matrix::IDENTITY)? else {
                    continue;
                };
                let consumes = count == 1 && self.start_keyboard_tap(&surface, &node, id, page);
                let mut parent = Some(node.clone());
                let mut path = Vec::new();
                let mut pans = Vec::new();
                while let Some(node) = parent {
                    if consumes {
                        self.suppressed_presses.push(node.id);
                    }
                    if node.interaction.mask().contains(EventKind::Touch) {
                        path.push(node.id);
                    }
                    if node.interaction.pan_activation().is_some() {
                        pans.push((Rc::downgrade(&node), false));
                    }
                    parent = node.parent_node.borrow().upgrade();
                }
                if !path.is_empty() && !consumes {
                    self.fingers.push(Finger {
                        point: TouchPoint {
                            id,
                            target: node.id,
                            path,
                            page: nsh_core::Point::new(
                                nsh_core::Points(page.0),
                                nsh_core::Points(page.1),
                            ),
                            location: nsh_core::Point::default(),
                            changed: true,
                        },
                        node: Rc::downgrade(&node),
                        start: page,
                        pans,
                    });
                }
            }
            if self.keyboard_tap.as_ref().is_some_and(|tap| {
                tap.finger == id
                    && ((page.0 - tap.start.0).abs() > 10.0 || (page.1 - tap.start.1).abs() > 10.0)
            }) {
                self.keyboard_tap = None;
            }
            if let Some(finger) = self.fingers.iter_mut().find(|f| f.point.id == id) {
                finger.point.page =
                    nsh_core::Point::new(nsh_core::Points(page.0), nsh_core::Points(page.1));
                if let Some(node) = finger.node.upgrade()
                    && let Some(matrix) = node.input_in_root(root)?
                    && let Some((x, y)) = crate::render::geometry::local_point(&matrix.0, local)
                {
                    finger.point.location =
                        nsh_core::Point::new(nsh_core::Points(x), nsh_core::Points(y));
                }
            }
        }
        if phase == TouchPhase::Cancel {
            self.keyboard_tap = None;
        }
        if phase == TouchPhase::End
            && self
                .keyboard_tap
                .as_ref()
                .is_some_and(|tap| Some(tap.finger) == changed)
            && let Some(tap) = self.keyboard_tap.take()
            && let Some(node) = tap.input.upgrade()
            && let Some(input) = node.input.borrow().as_ref()
        {
            input.focus(false)?;
        }
        let mut activated = None;
        for finger in &mut self.fingers {
            finger.point.changed = matches!(phase, TouchPhase::Move | TouchPhase::Cancel)
                || changed == Some(finger.point.id);
            if phase == TouchPhase::Move && self.activated.is_none() {
                for (node, failed) in &mut finger.pans {
                    if *failed {
                        continue;
                    }
                    let Some(node) = node.upgrade() else {
                        *failed = true;
                        continue;
                    };
                    match node.interaction.pan_activation().and_then(|p| {
                        p.decide(
                            nsh_core::Points(finger.point.page.x.0 - finger.start.0),
                            nsh_core::Points(finger.point.page.y.0 - finger.start.1),
                        )
                    }) {
                        Some(false) => *failed = true,
                        Some(true) => {
                            block(&node, true)?;
                            self.activated = Some(Rc::downgrade(&node));
                            activated = Some(node.id);
                            break;
                        }
                        None => {}
                    }
                }
            }
        }
        if !self.fingers.is_empty() {
            root.control.dispatch_event(
                nsh_core::ROOT_ID,
                EventKind::Touch,
                EventPayload::Touch(Box::new(TouchEvent {
                    phase,
                    timestamp: self.timestamp,
                    points: self.fingers.iter().map(|f| f.point.clone()).collect(),
                    activated,
                })),
            );
        }
        if phase == TouchPhase::End {
            self.fingers.retain(|f| !f.point.changed);
        }
        if phase == TouchPhase::Cancel {
            self.fingers.clear()
        }
        if self.fingers.is_empty()
            && let Some(node) = self.activated.take().and_then(|n| n.upgrade())
        {
            block(&node, false)?;
        }
        Ok(())
    }
    pub(crate) fn press_allowed(&self, id: nsh_core::NodeId) -> bool {
        !self.suppressed_presses.contains(&id)
    }
    pub(crate) fn selection_started(
        &mut self,
        owner: &Rc<Node>,
        range: std::ops::Range<usize>,
        point: (f32, f32),
    ) -> Result<()> {
        let Some(finger) = self.primary else {
            return Ok(());
        };
        let blocked = self.claim_selection(owner)?;
        self.selection = Some(SelectionDrag {
            owner: Rc::downgrade(owner),
            finger,
            anchor: range.start,
            start: point,
            moved: false,
            pointer: None,
            frame_time: owner.animator.now(),
            blocked,
        });
        Ok(())
    }
    pub(crate) fn selection_dragging(&self, id: nsh_core::NodeId) -> bool {
        self.selection.as_ref().is_some_and(|drag| {
            drag.moved && drag.owner.upgrade().is_some_and(|owner| owner.id == id)
        })
    }
    pub(crate) fn selection_held(&self) -> bool {
        self.selection.is_some()
    }
    fn claim_selection(
        &mut self,
        owner: &Rc<Node>,
    ) -> Result<Vec<Weak<crate::scroll::controller::Scroller>>> {
        self.keyboard_tap = None;
        self.back = None;
        if !self.fingers.is_empty() {
            owner.control.dispatch_event(
                nsh_core::ROOT_ID,
                EventKind::Touch,
                EventPayload::Touch(Box::new(TouchEvent {
                    phase: TouchPhase::Cancel,
                    timestamp: self.timestamp,
                    activated: None,
                    points: self
                        .fingers
                        .drain(..)
                        .map(|mut f| {
                            f.point.changed = true;
                            f.point
                        })
                        .collect(),
                })),
            );
        }
        if let Some(old) = self.activated.take().and_then(|n| n.upgrade()) {
            block(&old, false)?;
        }
        let mut node = Some(owner.clone());
        let mut blocked = Vec::new();
        while let Some(current) = node {
            self.suppressed_presses.push(current.id);
            if let Some(scroll) = current.scroll.borrow().as_ref() {
                blocked.push(Rc::downgrade(scroll));
            }
            node = current.parent_node.borrow().upgrade();
        }
        if let Err(error) = block(owner, true) {
            for scroll in blocked.iter().filter_map(Weak::upgrade) {
                let _ = scroll.block(false);
            }
            return Err(error);
        }
        Ok(blocked)
    }
    fn selection_input(
        &mut self,
        root: &Rc<Node>,
        input: &ArkUIInputEvent,
        phase: TouchPhase,
        changed: Option<u32>,
        count: u32,
    ) -> Result<bool> {
        if phase == TouchPhase::Start && count == 1 {
            // A popup can consume the preceding contact's release.
            if let Some(old) = self.selection.take() {
                old.unblock()?;
            }
            let selected = root.selected_text.borrow().as_ref().and_then(Weak::upgrade);
            if let Some(owner) = selected {
                let local = (input.pointer_x_by_index(0), input.pointer_y_by_index(0));
                let point = owner
                    .input_in_root(root)?
                    .and_then(|m| crate::render::geometry::local_point(&m.0, local));
                if owner.visible_in_tree()
                    && let Some(point) = point
                {
                    if let Some(anchor) = owner.selection_handle(point) {
                        let blocked = self.claim_selection(&owner)?;
                        owner.selection_menu_show(false)?;
                        self.selection = Some(SelectionDrag {
                            owner: Rc::downgrade(&owner),
                            finger: changed.unwrap_or(0),
                            anchor,
                            start: point,
                            moved: true,
                            pointer: Some(local),
                            frame_time: owner.animator.now(),
                            blocked,
                        });
                        owner.animator.track_selection(root);
                        return Ok(true);
                    }
                    if hit(root, local, &nsh_core::Matrix::IDENTITY)?
                        .is_some_and(|n| n.id == owner.id)
                    {
                        owner.selection_clear()?;
                        return Ok(false);
                    }
                }
                root.clear_text_selection()?;
            }
        }
        let Some(mut drag) = self.selection.take() else {
            return Ok(false);
        };
        let Some(owner) = drag.owner.upgrade() else {
            drag.unblock()?;
            return Ok(false);
        };
        let index = (0..count).find(|&i| input.pointer_id(i) as u32 == drag.finger);
        let cancelled = phase == TouchPhase::Cancel || count > 1 || !owner.visible_in_tree();
        if !cancelled && let Some(index) = index {
            let local = (
                input.pointer_x_by_index(index),
                input.pointer_y_by_index(index),
            );
            drag.pointer = Some(local);
            if let Some(point) = owner
                .input_in_root(root)?
                .and_then(|m| crate::render::geometry::local_point(&m.0, local))
            {
                if !drag.moved
                    && phase == TouchPhase::Move
                    && (point.0 - drag.start.0).hypot(point.1 - drag.start.1) > 8.0
                {
                    drag.moved = true;
                    owner.selection_menu_show(false)?;
                }
                if drag.moved && phase == TouchPhase::Move {
                    owner.selection_extend(drag.anchor, point)?;
                    owner.animator.track_selection(root);
                }
            }
        }
        if cancelled || phase == TouchPhase::End || index.is_none() {
            self.primary = None;
            drag.unblock()?;
            if cancelled {
                owner.selection_clear()?;
            } else {
                owner.selection_menu_show(true)?;
            }
        } else {
            self.selection = Some(drag);
        }
        Ok(true)
    }
    pub(crate) fn selection_tick(&mut self, root: &Rc<Node>, now: f64) -> Result<bool> {
        let Some(drag) = self.selection.as_mut().filter(|drag| drag.moved) else {
            return Ok(false);
        };
        let Some(owner) = drag.owner.upgrade() else {
            drag.unblock()?;
            self.selection = None;
            return Ok(false);
        };
        if !owner.visible_in_tree() || owner.selection.range.get().is_none() {
            drag.unblock()?;
            self.selection = None;
            return Ok(false);
        }
        let elapsed = now - drag.frame_time;
        drag.frame_time = now;
        let Some(pointer) = drag.pointer else {
            return Ok(false);
        };
        let mut parent = owner.parent_node.borrow().upgrade();
        let mut scrolling = false;
        while let Some(node) = parent {
            if let Some(scroll) = node.scroll.borrow().as_ref()
                && let Some(point) = node
                    .input_in_root(root)?
                    .and_then(|matrix| crate::render::geometry::local_point(&matrix.0, pointer))
                && scroll.selection_scroll(point, elapsed)?
            {
                scrolling = true;
                break;
            }
            if node.id == root.id {
                break;
            }
            parent = node.parent_node.borrow().upgrade();
        }
        if let Some(point) = owner
            .input_in_root(root)?
            .and_then(|matrix| crate::render::geometry::local_point(&matrix.0, pointer))
        {
            owner.selection_extend(drag.anchor, point)?;
        }
        Ok(scrolling)
    }
    fn back_input(
        &mut self,
        root: &Rc<Node>,
        input: &ArkUIInputEvent,
        phase: TouchPhase,
        changed: Option<u32>,
        count: u32,
    ) -> Result<bool> {
        if phase == TouchPhase::Start && count == 1 {
            self.suppressed_presses.clear();
            let local = (input.pointer_x_by_index(0), input.pointer_y_by_index(0));
            let target = hit(root, local, &nsh_core::Matrix::IDENTITY)?;
            let mut parent = target.clone();
            while let Some(node) = parent {
                if let Some(screen) = node.screen.borrow().as_ref()
                    && let Some(matrix) = node.input_in_root(root)?
                    && let Some((x, _)) = crate::render::geometry::local_point(&matrix.0, local)
                    && screen.can_swipe(x)
                {
                    self.back = Some(BackSwipe {
                        screen: Rc::downgrade(screen),
                        target: target.as_ref().map_or_else(Weak::new, Rc::downgrade),
                        finger: input.pointer_id(0) as u32,
                        start: (
                            input.pointer_window_x_by_index(0),
                            input.pointer_window_y_by_index(0),
                        ),
                        local_x: x,
                        sign: if node.platform.get().rtl { -1.0 } else { 1.0 },
                        width: node.frame.get().width.max(1.0),
                        active: false,
                        last: (0.0, self.timestamp),
                        velocity: 0.0,
                    });
                    break;
                }
                parent = node.parent_node.borrow().upgrade();
            }
        }
        let Some(mut back) = self.back.take() else {
            return Ok(false);
        };
        let Some(screen) = back.screen.upgrade() else {
            return Ok(false);
        };
        let index = (0..count).find(|&index| input.pointer_id(index) as u32 == back.finger);
        let cancelled = phase == TouchPhase::Cancel
            || count != 1
            || index.is_none()
            || !screen.can_swipe(back.local_x);
        if cancelled {
            if back.active {
                screen.finish_swipe(false)?;
                if let Some(owner) = screen.owner() {
                    block(&owner, false)?;
                }
            }
            return Ok(back.active);
        }
        let index = index.unwrap();
        let dx = (input.pointer_window_x_by_index(index) - back.start.0) * back.sign;
        let dy = input.pointer_window_y_by_index(index) - back.start.1;
        if !back.active && phase == TouchPhase::Move {
            if dy.abs() > 8.0 && dy.abs() >= dx.abs() || dx < -8.0 {
                return Ok(false);
            }
            if dx > 8.0 && dx > dy.abs() {
                back.active = true;
                self.keyboard_tap = None;
                // Cancel the JS responder before claiming the native edge gesture.
                if !self.fingers.is_empty() {
                    root.control.dispatch_event(
                        nsh_core::ROOT_ID,
                        EventKind::Touch,
                        EventPayload::Touch(Box::new(TouchEvent {
                            phase: TouchPhase::Cancel,
                            timestamp: self.timestamp,
                            activated: None,
                            points: self
                                .fingers
                                .iter()
                                .map(|finger| {
                                    let mut point = finger.point.clone();
                                    point.changed = true;
                                    point
                                })
                                .collect(),
                        })),
                    );
                    self.fingers.clear();
                }
                let mut target = back.target.upgrade();
                while let Some(node) = target {
                    self.suppressed_presses.push(node.id);
                    target = node.parent_node.borrow().upgrade();
                }
                if let Some(owner) = screen.owner() {
                    block(&owner, true)?;
                }
            }
        }
        if back.active {
            let elapsed = self.timestamp - back.last.1;
            if elapsed > 0.0 && dx != back.last.0 {
                back.velocity = (dx - back.last.0) * 1000.0 / elapsed as f32;
                back.last = (dx, self.timestamp);
            }
            screen.swipe(dx)?;
            if phase == TouchPhase::End && changed == Some(back.finger) {
                let recent_velocity = if self.timestamp - back.last.1 <= 100.0 {
                    back.velocity
                } else {
                    0.0
                };
                screen.finish_swipe(crate::navigation::motion::complete_swipe(
                    dx,
                    back.width,
                    recent_velocity,
                ))?;
                if let Some(owner) = screen.owner() {
                    block(&owner, false)?;
                }
                return Ok(true);
            }
        }
        let consumed = back.active;
        if phase != TouchPhase::End {
            self.back = Some(back);
        }
        Ok(consumed)
    }
    fn start_keyboard_tap(
        &mut self,
        root: &Rc<Node>,
        target: &Rc<Node>,
        finger: u32,
        start: (f32, f32),
    ) -> bool {
        use nsh_core::KeyboardTaps;
        let mut parent = Some(target.clone());
        let mut handled = false;
        let mut policy = None;
        while let Some(node) = parent {
            if node.kind == nsh_core::ViewKind::Input {
                return false;
            }
            handled |= [EventKind::Press, EventKind::LongPress, EventKind::Touch]
                .into_iter()
                .any(|event| node.interaction.mask().contains(event) || node.text_listens(event));
            if let Some(scroll) = node.scroll.borrow().as_ref() {
                policy = Some(scroll.keyboard_taps());
                break;
            }
            parent = node.parent_node.borrow().upgrade();
        }
        let dismiss = match policy {
            Some(KeyboardTaps::Never) => true,
            Some(KeyboardTaps::Handled) => !handled,
            _ => false,
        };
        if !dismiss {
            return false;
        }
        let focused = root
            .dispatcher
            .upgrade()
            .and_then(|route| route.keyboard.lock().unwrap().input);
        let Some(input) = focused
            .and_then(|id| find(root, id))
            .filter(|node| node.input.borrow().is_some())
        else {
            return false;
        };
        self.keyboard_tap = Some(KeyboardTap {
            finger,
            start,
            input: Rc::downgrade(&input),
        });
        policy == Some(KeyboardTaps::Never) && root.platform.get().keyboard_height.0 > 0.0
    }
}
fn find(root: &Rc<Node>, id: nsh_core::NodeId) -> Option<Rc<Node>> {
    if root.id == id {
        return Some(root.clone());
    }
    root.children
        .borrow()
        .iter()
        .filter_map(Weak::upgrade)
        .find_map(|child| find(&child, id))
}
fn block(node: &Rc<Node>, blocked: bool) -> Result<()> {
    let mut node = Some(node.clone());
    while let Some(current) = node {
        if let Some(scroll) = current.scroll.borrow().as_ref() {
            scroll.block(blocked)?;
        }
        node = current.parent_node.borrow().upgrade();
    }
    Ok(())
}
fn hit(root: &Rc<Node>, point: (f32, f32), world: &nsh_core::Matrix) -> Result<Option<Rc<Node>>> {
    if !root.visible_in_tree() {
        return Ok(None);
    }
    let pointer = root.interaction.pointer_events();
    if pointer == PointerEvents::None {
        return Ok(None);
    }
    let Some((x, y)) = crate::render::geometry::local_point(&world.0, point) else {
        return Ok(None);
    };
    let frame = root.frame.get();
    let area = nsh_core::input::hit_area(
        nsh_core::geometry::point(nsh_core::Points(x), nsh_core::Points(y)),
        nsh_core::geometry::size(
            nsh_core::Points(frame.width),
            nsh_core::Points(frame.height),
        ),
        root.interaction.hit_slop(),
        root.overflow.get().map(Into::into),
        root.clips_children(),
        nsh_core::input::laid_out(root.kind, root.id),
    );
    if pointer != PointerEvents::BoxOnly && area.children {
        // Sticky cells are raised above their siblings in the native tree.
        for priority in [true, false] {
            for child in root
                .children
                .borrow()
                .iter()
                .rev()
                .filter_map(Weak::upgrade)
                .filter(|child| child.kind != nsh_core::ViewKind::Modal)
            {
                if child.input_priority() == priority {
                    let matrix = child.input_to_parent(root)?.mul(world);
                    if let Some(node) = hit(&child, point, &matrix)? {
                        return Ok(Some(node));
                    }
                }
            }
        }
    }
    Ok((area.in_slop && pointer != PointerEvents::BoxNone).then(|| root.clone()))
}
// Each native callback takes one short-lived borrow; the engine owns responder negotiation.
pub(crate) type State = RefCell<Touches>;

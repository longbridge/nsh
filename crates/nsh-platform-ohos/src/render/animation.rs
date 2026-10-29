//! One native animation graph per surface, evaluated on the UI thread.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::Instant,
};

use nsh_core::animation::{AnimationOp, AnimationProp, AnimationValue, Finished, Graph};
use nsh_engine::EngineControl;

use crate::render::node::{Node, Result};

type Bindings = (Weak<Node>, Vec<(AnimationProp, u32)>);

#[derive(Default)]
struct State {
    graph: Graph,
    finished: Vec<Finished>,
}

pub(crate) struct Animator {
    state: RefCell<State>,
    dirty: std::cell::Cell<bool>,
    bindings: RefCell<Vec<Option<Bindings>>>,
    motion: RefCell<Vec<Option<Weak<Node>>>>,
    scrolls: RefCell<Vec<Option<Weak<Node>>>>,
    sticky: RefCell<Vec<Option<Weak<Node>>>>,
    selection: RefCell<Weak<Node>>,
    removals: RefCell<Vec<u32>>,
    control: EngineControl,
    dispatcher: std::sync::Weak<crate::app::runtime::Dispatcher>,
    clock: Instant,
}

impl Animator {
    pub(crate) fn new(
        control: EngineControl,
        dispatcher: std::sync::Weak<crate::app::runtime::Dispatcher>,
    ) -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::default(),
            dirty: std::cell::Cell::new(false),
            bindings: RefCell::default(),
            motion: RefCell::default(),
            scrolls: RefCell::default(),
            sticky: RefCell::default(),
            selection: RefCell::default(),
            removals: RefCell::default(),
            control,
            dispatcher,
            clock: Instant::now(),
        })
    }
    pub(crate) fn request_frame(&self) {
        if let Some(dispatcher) = self.dispatcher.upgrade() {
            dispatcher.request_frame();
        }
    }
    pub(crate) fn mark_dirty(&self) {
        self.dirty.set(true);
    }
    pub(crate) fn apply(&self, op: AnimationOp) {
        self.dirty.set(true);
        let mut state = self.state.borrow_mut();
        let State { graph, finished } = &mut *state;
        graph.apply(op, finished);
    }
    pub(crate) fn bind(&self, owner: &Rc<Node>, values: Vec<(AnimationProp, u32)>) -> Result<()> {
        self.dirty.set(true);
        let mut bindings = self.bindings.borrow_mut();
        if bindings.len() <= owner.id as usize {
            bindings.resize_with(owner.id as usize + 1, || None);
        }
        bindings[owner.id as usize] = (!values.is_empty()).then(|| (Rc::downgrade(owner), values));
        drop(bindings);
        owner.component_animated.borrow_mut().clear();
        owner.reset_text_tint()?;
        owner.restore_style()
    }
    pub(crate) fn apply_node(&self, owner: &Node) -> Result<()> {
        let values = {
            let bindings = self.bindings.borrow();
            let Some((_, props)) = bindings
                .get(owner.id as usize)
                .and_then(|slot| slot.as_ref())
            else {
                return Ok(());
            };
            let state = self.state.borrow();
            props
                .iter()
                .map(|(prop, node)| (*prop, state.graph.eval(*node)))
                .collect::<Vec<_>>()
        };
        owner.animate(&values)
    }
    pub(crate) fn remove(&self, id: u32) {
        if let Some(slot) = self.bindings.borrow_mut().get_mut(id as usize) {
            *slot = None;
        }
        if let Some(slot) = self.motion.borrow_mut().get_mut(id as usize) {
            *slot = None;
        }
        if let Some(slot) = self.scrolls.borrow_mut().get_mut(id as usize) {
            *slot = None;
        }
        if let Some(slot) = self.sticky.borrow_mut().get_mut(id as usize) {
            *slot = None;
        }
    }
    pub(crate) fn set_sticky(&self, owner: &Rc<Node>, animated: bool) {
        if animated {
            self.dirty.set(true);
        }
        let mut sticky = self.sticky.borrow_mut();
        if sticky.len() <= owner.id as usize {
            sticky.resize_with(owner.id as usize + 1, || None);
        }
        sticky[owner.id as usize] = animated.then(|| Rc::downgrade(owner));
    }
    pub(crate) fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.0
    }
    pub(crate) fn track(&self, owner: &Rc<Node>) {
        let mut motion = self.motion.borrow_mut();
        if motion.len() <= owner.id as usize {
            motion.resize_with(owner.id as usize + 1, || None);
        }
        motion[owner.id as usize] = Some(Rc::downgrade(owner));
        self.request_frame();
    }
    pub(crate) fn track_scroll(&self, owner: &Rc<Node>) {
        let mut scrolls = self.scrolls.borrow_mut();
        if scrolls.len() <= owner.id as usize {
            scrolls.resize_with(owner.id as usize + 1, || None);
        }
        scrolls[owner.id as usize] = Some(Rc::downgrade(owner));
        self.request_frame();
    }
    pub(crate) fn track_selection(&self, root: &Rc<Node>) {
        *self.selection.borrow_mut() = Rc::downgrade(root);
        self.request_frame();
    }
    pub(crate) fn take_removals(&self) -> Vec<u32> {
        std::mem::take(&mut self.removals.borrow_mut())
    }
    pub(crate) fn set(&self, node: u32, value: f32) -> Result<()> {
        {
            let mut state = self.state.borrow_mut();
            let State { graph, finished } = &mut *state;
            graph.set(node, value, finished);
        }
        self.write()
    }
    pub(crate) fn value(&self, node: u32) -> f32 {
        self.state.borrow().graph.value(node).unwrap_or_default()
    }
    pub(crate) fn tick(&self) -> Result<bool> {
        let (was_animating, mut animating) = {
            let mut state = self.state.borrow_mut();
            let State { graph, finished } = &mut *state;
            let was = graph.is_animating();
            if was || self.dirty.get() {
                graph.tick(self.clock.elapsed().as_secs_f64(), finished);
            }
            (was, graph.is_animating())
        };
        let now = self.now();
        let selection_root = self.selection.borrow().upgrade();
        if let Some(root) = selection_root {
            let mut touches = root.touches.borrow_mut();
            let active = touches.selection_tick(&root, now)?;
            animating |= active;
            // Keep a weak cleanup route at an edge without scheduling idle frames.
            if !touches.selection_held() {
                *self.selection.borrow_mut() = Weak::new();
            }
        }
        for slot in self.scrolls.borrow_mut().iter_mut() {
            let active = if let Some(owner) = slot.as_ref().and_then(Weak::upgrade)
                && let Some(scroll) = owner.scroll.borrow().as_ref()
            {
                scroll.tick(now)?
            } else {
                false
            };
            animating |= active;
            if !active {
                *slot = None;
            }
        }
        for slot in self.motion.borrow_mut().iter_mut() {
            let Some(owner) = slot.as_ref().and_then(Weak::upgrade) else {
                *slot = None;
                continue;
            };
            let (active, removed) = owner.tick_transition(now)?;
            animating |= active;
            if removed {
                self.removals.borrow_mut().push(owner.id);
            }
            if !active {
                *slot = None;
            }
        }
        if was_animating || animating || self.dirty.get() {
            self.write()?;
        }
        Ok(animating)
    }
    fn write(&self) -> Result<()> {
        self.dirty.set(false);
        let mut values = Vec::new();
        for binding in self.bindings.borrow().iter().flatten() {
            let Some(owner) = binding.0.upgrade() else {
                continue;
            };
            values.clear();
            {
                let state = self.state.borrow();
                values.extend(
                    binding
                        .1
                        .iter()
                        .map(|(prop, node)| (*prop, state.graph.eval(*node))),
                );
            }
            owner.animate(&values)?;
        }
        for owner in self
            .sticky
            .borrow()
            .iter()
            .flatten()
            .filter_map(Weak::upgrade)
        {
            if let Some(scroll) = owner.scroll.borrow().as_ref() {
                scroll.pin_sticky()?;
            }
        }
        let finished = std::mem::take(&mut self.state.borrow_mut().finished);
        if !finished.is_empty() {
            self.control.animations_finished(finished);
        }
        Ok(())
    }
}

pub(crate) fn color(value: AnimationValue) -> nsh_core::Color {
    nsh_core::Color(match value {
        AnimationValue::Color(color) => color,
        other => other.number() as u32,
    })
}

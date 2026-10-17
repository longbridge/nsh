//! The core's side of the platform services Rust components ask for
//! ([`crate::ui::ambient`]'s functions) beyond commands for the
//! platform: display frames and the layout direction.
//!
//! The surface's direction is its root's (`Prop::LayoutDirection` on the
//! root, which JS's `I18nManager` sets), else the platform's; components
//! read it ([`crate::ui::ambient::rtl`]). Once a component sets a policy
//! (`force_rtl`, `allow_rtl`), and from the start in a Rust app, the core
//! sets the root's direction from the policy and the platform's language,
//! again whenever the language changes. In an app whose JS also sets it,
//! the last one to set it wins.

use super::Core;
use crate::{
    Op, Prop, ROOT_ID, TextEngine,
    time::Instant,
    ui::{AnswerSlot, services::Direction},
};

/// What the core keeps for the services.
pub(super) struct Services {
    /// The tasks waiting for the next display frame.
    frames: Vec<AnswerSlot<Instant>>,
    /// The policy of the layout direction.
    direction: DirectionPolicy,
}

impl Default for Services {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            direction: DirectionPolicy {
                allow: true,
                force: false,
                platform: false,
                active: false,
            },
        }
    }
}

/// `I18nManager`'s flags, as Rust components set them.
struct DirectionPolicy {
    /// A right-to-left language lays out right to left.
    allow: bool,
    /// Right to left whatever the language.
    force: bool,
    /// The platform's language is written right to left.
    platform: bool,
    /// The core sets the root's direction (a component set a policy, or
    /// the app is Rust).
    active: bool,
}

impl DirectionPolicy {
    fn rtl(&self) -> bool {
        (self.allow && self.platform) || self.force
    }
}

impl<M: TextEngine> Core<M> {
    /// Whether a component waits for a display frame: ask the platform for
    /// one, then call [`Self::run_frame`] when it begins.
    pub fn wants_frame(&self) -> bool {
        // A slot only the core holds is a task that is gone.
        self.components
            .services
            .frames
            .iter()
            .any(|slot| std::rc::Rc::strong_count(slot) > 1)
    }

    /// A display frame began at `at`: the tasks waiting for it go on (at
    /// the next [`Self::run_component_tasks`]).
    pub fn run_frame(&mut self, at: Instant) {
        for slot in std::mem::take(&mut self.components.services.frames) {
            slot.borrow_mut().deliver(at);
        }
    }

    pub(super) fn wait_frame(&mut self, slot: AnswerSlot<Instant>) {
        self.components.services.frames.push(slot);
    }

    /// The direction components read, the platform's language being
    /// `platform`; the root follows the policy when the core sets it.
    pub(super) fn surface_rtl(&mut self, platform: bool) -> bool {
        let policy = &mut self.components.services.direction;
        policy.platform = platform;
        if policy.active {
            let rtl = policy.rtl();
            self.set_root_direction(rtl);
            rtl
        } else {
            self.nodes[self.root as usize].direction.unwrap_or(platform)
        }
    }

    /// A component changed the policy.
    pub(super) fn change_direction(&mut self, change: Direction) {
        let policy = &mut self.components.services.direction;
        policy.active = true;
        match change {
            Direction::Force(force) => policy.force = force,
            Direction::Allow(allow) => policy.allow = allow,
        }
        let rtl = policy.rtl();
        self.set_root_direction(rtl);
    }

    /// A Rust app: the core sets the surface's direction (no JS does).
    pub(super) fn own_direction(&mut self) {
        let policy = &mut self.components.services.direction;
        policy.active = true;
        let rtl = policy.rtl();
        self.set_root_direction(rtl);
    }

    fn set_root_direction(&mut self, rtl: bool) {
        let _ = self.apply_op(Op::SetProp {
            id: ROOT_ID,
            prop: Prop::LayoutDirection(rtl),
        });
    }

    /// The root's direction changed (JS's or the policy's): what read it
    /// runs again.
    pub(super) fn root_direction_changed(&mut self, rtl: bool) {
        if self.components.environment.rtl != rtl {
            let environment = crate::ui::Environment {
                rtl,
                ..self.components.environment
            };
            self.apply_environment(environment);
        }
    }
}

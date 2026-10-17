//! Tailwind classes on nodes: resolving them into styles (with the inline
//! `style` applied on top) and resolving them again when what they depend
//! on changes: the appearance, the window size, the layout direction, the
//! node's interaction states, its group's states or its sibling position.

use std::sync::Arc;

use super::{Core, Key, Kind, NO_KEY, dirty, pressable};
use crate::{
    mount::MountItem,
    ops::NodeId,
    platform::{Appearance, ColorScheme},
    style::{Color, Style, ViewStyle},
    tailwind::{ClassEnvironment, ClassList, ClassProps, Context, Theme, deps, position, state},
    text::TextEngine,
};

/// The states the platform reports ([`Core::set_states`]).
const REPORTED: u8 = state::PRESSED | state::HOVERED | state::FOCUSED;

/// The `class` and `classList` props as set.
#[derive(Debug, Default)]
pub(super) struct ClassNames {
    class: Box<str>,
    list: Vec<Box<str>>,
}

impl<M: TextEngine> Core<M> {
    /// Sets the environment class variants depend on (`dark:`, `ios:`,
    /// `motion-reduce:`): nodes whose classes depend on it are styled again.
    pub fn set_class_environment(&mut self, mut env: ClassEnvironment) {
        self.system_dark = env.dark;
        env.dark = self.shown_dark();
        self.apply_class_environment(env);
    }

    /// Dark appearance as shown: the app's, else the system's.
    pub(super) fn shown_dark(&self) -> bool {
        let system = if self.system_dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
        self.appearance.resolve(system) == ColorScheme::Dark
    }

    /// The appearance the app asks for (`nsh.json` `"appearance"`, TS
    /// `setColorScheme`, [`crate::ui::ambient::set_color_scheme`]): what
    /// `dark:` variants and the components' theme follow in place of the
    /// system's; the platform applies it to its own controls
    /// ([`MountItem::Appearance`]).
    pub fn set_appearance(&mut self, appearance: Appearance) {
        if self.appearance == appearance {
            return;
        }
        self.appearance = appearance;
        self.mount.push(MountItem::Appearance { appearance });
        let dark = self.shown_dark();
        let mut env = self.class_env;
        env.dark = dark;
        self.apply_class_environment(env);
        let mut environment = self.components.environment;
        environment.dark = dark;
        self.apply_environment(environment);
    }

    fn apply_class_environment(&mut self, env: ClassEnvironment) {
        if self.class_env == env {
            return;
        }
        let changed = (if env.dark != self.class_env.dark { deps::DARK } else { 0 })
            | (if env.reduce_motion != self.class_env.reduce_motion { deps::MOTION } else { 0 })
            // `ios:` / `android:` never change at run time, but a new
            // platform must reach classes resolved before it was known.
            | (if env.platform != self.class_env.platform { u16::MAX } else { 0 });
        self.class_env = env;
        self.restyle_where(changed);
    }

    /// The platform reports interaction states of view `id` (`active:`,
    /// `hover:`, `focus:`): state bits for the states it
    /// tracks ([`MountItem::TrackStates`]); see [`crate::tailwind::state`].
    pub fn set_states(&mut self, id: NodeId, states: u8) {
        let Ok(key) = self.key(id) else { return };
        self.set_node_states(key, states);
    }

    /// [`Self::set_states`] by key.
    fn set_node_states(&mut self, key: Key, states: u8) {
        let node = &mut self.nodes[key as usize];
        let new = node.states & !REPORTED | states & REPORTED;
        if new == node.states {
            return;
        }
        let focus_changed = (new ^ node.states) & state::FOCUSED != 0;
        node.states = new;
        self.states_changed(key);
        if focus_changed {
            // `focus-within:` on the node and its ancestors.
            let focused = new & state::FOCUSED != 0;
            let mut k = key;
            while k != NO_KEY {
                let node = &mut self.nodes[k as usize];
                let before = node.states;
                if focused {
                    node.states |= state::FOCUS_WITHIN;
                } else {
                    node.states &= !state::FOCUS_WITHIN;
                }
                let parent = node.parent;
                if node.states != before && k != key {
                    self.states_changed(k);
                }
                k = parent;
            }
        }
    }

    /// `disabled` on a host element: the `disabled:` variant.
    pub(super) fn set_disabled(&mut self, key: Key, disabled: bool) {
        let node = &mut self.nodes[key as usize];
        let before = node.states;
        if disabled {
            node.states |= state::DISABLED;
        } else {
            node.states &= !state::DISABLED;
        }
        if node.states != before {
            if pressable(node.events) {
                self.mark(key, dirty::EVENTS);
            }
            self.states_changed(key);
        }
    }

    /// Restyles `key` if its classes read its states, and the nodes inside
    /// it that read its group states.
    fn states_changed(&mut self, key: Key) {
        if self.nodes[key as usize].underlay.is_some() {
            self.mark_underlay(key);
        }
        let Some(classes) = self.nodes[key as usize].classes.clone() else {
            return;
        };
        if classes.deps() & deps::STATE != 0 {
            self.restyle(key);
        }
        if !classes.groups().is_empty() {
            self.restyle_subtree(key, deps::GROUP);
        }
    }

    /// `underlayColor` / `underlayOpacity` of `key` changed.
    pub(super) fn underlay_changed(&mut self, key: Key) {
        self.update_tracking(key);
        self.mark_underlay(key);
    }

    /// Restyles `key` and its children, which an underlay changes.
    fn mark_underlay(&mut self, key: Key) {
        self.mark(key, dirty::VIEW);
        for i in 0..self.nodes[key as usize].children.len() {
            let child = self.nodes[key as usize].children[i];
            self.mark(child, dirty::VIEW);
        }
    }

    /// The underlay of `key` while it shows: pressed and not disabled.
    fn showing_underlay(&self, key: Key) -> Option<(Color, f32)> {
        let node = &self.nodes[key as usize];
        let u = node.underlay.as_deref()?;
        (node.states & (state::PRESSED | state::DISABLED) == state::PRESSED).then_some(*u)
    }

    /// The view style of `key` as platforms take it: fractional
    /// translations resolved, an underlay shown while pressed (in place of
    /// the background color and gradients).
    pub(super) fn mounted_view_style(&self, key: Key) -> Box<ViewStyle> {
        let node = &self.nodes[key as usize];
        let mut style = node.mounted_view_style();
        if let Some((color, _)) = self.showing_underlay(key) {
            // The underlay replaces the whole background, gradients too:
            // drawn over the color, they would hide the feedback.
            style.background_color = color;
            style.clear_background_images();
        }
        if node.parent != NO_KEY
            && let Some((_, opacity)) = self.showing_underlay(node.parent)
        {
            style.opacity *= opacity;
        }
        style
    }

    /// Sets `class` (`list == false`) or `classList` of `key`.
    pub(super) fn set_classes(
        &mut self,
        key: Key,
        names: Option<Box<str>>,
        list: Option<Vec<Box<str>>>,
    ) {
        let node = &mut self.nodes[key as usize];
        let current = node.class_names.get_or_insert_default();
        if let Some(n) = names {
            current.class = n;
        }
        if let Some(l) = list {
            current.list = l;
        }
        let mut combined = String::with_capacity(current.class.len() + 16 * current.list.len());
        combined.push_str(&current.class);
        for name in &current.list {
            combined.push(' ');
            combined.push_str(name);
        }
        let classes = if combined.trim().is_empty() {
            None
        } else {
            let (classes, fresh) = self.tailwind.get(&combined);
            if fresh && !classes.unknown().is_empty() {
                log::warn!(
                    "unknown classes: {}",
                    classes
                        .unknown()
                        .iter()
                        .map(|s| &**s)
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
            Some(classes)
        };
        self.set_class_list(key, classes);
    }

    /// Sets the resolved classes of `key` (a Rust component's styles go
    /// straight here).
    pub(super) fn set_class_list(&mut self, key: Key, classes: Option<Arc<ClassList>>) {
        let node = &mut self.nodes[key as usize];
        let had_groups = node
            .classes
            .as_ref()
            .is_some_and(|c| !c.groups().is_empty());
        node.classes = classes;
        let has_groups = node
            .classes
            .as_ref()
            .is_some_and(|c| !c.groups().is_empty());
        self.update_tracking(key);
        self.restyle(key);
        if had_groups || has_groups {
            self.restyle_subtree(key, deps::GROUP);
        }
    }

    /// Replaces the theme: every class is resolved again.
    pub(super) fn set_tailwind_theme(&mut self, json: &str) {
        let theme = match Theme::from_json(json) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("tailwind theme: {e}");
                return;
            }
        };
        self.tailwind.set_theme(theme);
        let keys: Vec<Key> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.class_names.is_some())
            .map(|(k, _)| k as Key)
            .collect();
        for key in keys {
            self.set_classes(key, None, None);
        }
    }

    /// Tells the platform which states of `key` to report.
    pub(super) fn update_tracking(&mut self, key: Key) {
        let node = &mut self.nodes[key as usize];
        let tracked = node.classes.as_ref().map_or(0, |c| c.tracked_states())
            | if node.underlay.is_some() {
                state::PRESSED
            } else {
                0
            };
        if tracked != node.tracked {
            node.tracked = tracked;
            // The platform stops reporting the states it no longer tracks,
            // and keeps the rest as they are: forget them here too, or a
            // state set before (a hover, a press) would outlive the pointer
            // and come back when the classes read it again.
            let stale = node.states & REPORTED & !tracked;
            let states = node.states & REPORTED & tracked;
            if node.created {
                let id = node.id;
                self.mount.push(MountItem::TrackStates {
                    id,
                    states: tracked,
                });
            }
            if stale != 0 {
                self.set_node_states(key, states);
            }
        }
    }

    /// The tracking item of a view that was just created.
    pub(super) fn creation_tracking(&self, key: Key) -> Option<MountItem> {
        let node = &self.nodes[key as usize];
        (node.tracked != 0).then(|| MountItem::TrackStates {
            id: node.id,
            states: node.tracked,
        })
    }

    /// Restyles the nodes whose classes depend on any of `changed`.
    pub(super) fn restyle_where(&mut self, changed: u16) {
        if changed == 0 {
            return;
        }
        let keys: Vec<Key> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.classes.as_ref().is_some_and(|c| c.deps() & changed != 0))
            .map(|(k, _)| k as Key)
            .collect();
        for key in keys {
            self.restyle(key);
        }
    }

    /// Restyles the nodes inside `key` whose classes depend on `changed`.
    fn restyle_subtree(&mut self, key: Key, changed: u16) {
        let mut stack: Vec<Key> = self.nodes[key as usize].children.clone();
        while let Some(k) = stack.pop() {
            let node = &self.nodes[k as usize];
            stack.extend_from_slice(&node.children);
            if node
                .classes
                .as_ref()
                .is_some_and(|c| c.deps() & changed != 0)
            {
                self.restyle(k);
            }
        }
    }

    /// Restyles the children of `parent` that depend on their position.
    pub(super) fn positions_changed(&mut self, parent: Key) {
        let children = self.nodes[parent as usize].children.clone();
        for c in children {
            if self.nodes[c as usize]
                .classes
                .as_ref()
                .is_some_and(|c| c.deps() & deps::POSITION != 0)
            {
                self.restyle(c);
            }
        }
    }

    /// Whether `key`'s classes depend on the layout direction.
    pub(super) fn reads_direction(&self, key: Key) -> bool {
        self.nodes[key as usize]
            .classes
            .as_ref()
            .is_some_and(|c| c.deps() & deps::RTL != 0)
    }

    /// [`position`] bits of `key` among its parent's element children.
    fn position_bits(&self, key: Key) -> u8 {
        let parent = self.nodes[key as usize].parent;
        if parent == NO_KEY {
            return position::FIRST | position::LAST | position::ONLY | position::ODD;
        }
        let siblings: Vec<Key> = self.nodes[parent as usize]
            .children
            .iter()
            .copied()
            .filter(|&c| self.nodes[c as usize].kind != Kind::RawText)
            .collect();
        let i = siblings.iter().position(|&c| c == key).unwrap_or(0);
        let n = siblings.len();
        let mut bits = if i % 2 == 0 {
            position::ODD
        } else {
            position::EVEN
        };
        if i == 0 {
            bits |= position::FIRST;
        }
        if i + 1 == n {
            bits |= position::LAST;
        }
        if n == 1 {
            bits |= position::ONLY;
        }
        bits
    }

    /// The states of the nearest ancestor of `key` that is group `name`.
    fn group_states(&self, key: Key, name: &str) -> u8 {
        let mut k = self.nodes[key as usize].parent;
        while k != NO_KEY {
            let node = &self.nodes[k as usize];
            if node
                .classes
                .as_ref()
                .is_some_and(|c| c.groups().iter().any(|g| **g == *name))
            {
                return node.states;
            }
            k = node.parent;
        }
        0
    }

    /// Resolves `key`'s style from its classes and inline style.
    pub(super) fn restyle(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let mut style = Style::default();
        let mut props = ClassProps::default();
        if let Some(classes) = node.classes.clone() {
            let group = |name: &str| self.group_states(key, name);
            let pos = if classes.deps() & deps::POSITION != 0 {
                self.position_bits(key)
            } else {
                0
            };
            let positional = pos != 0 && node.parent != NO_KEY;
            let window = (self.viewport.width, self.viewport.height);
            let cx = Context {
                env: self.class_env,
                window,
                rtl: node.rtl,
                states: node.states,
                position: pos,
                group: &group,
            };
            let inline = &node.inline_style;
            classes.apply(&cx, self.tailwind.theme(), &mut style, &mut props, |s| {
                inline.apply(s)
            });
            if positional {
                let parent = self.nodes[key as usize].parent;
                self.nodes[parent as usize].positional_children = true;
            }
        } else {
            node.inline_style.apply(&mut style);
        }
        self.set_class_props(key, props);
        let id = self.nodes[key as usize].id;
        let _ = self.set_style(id, style);
    }

    /// Applies the node props classes set (explicit props win).
    fn set_class_props(&mut self, key: Key, props: ClassProps) {
        let node = &mut self.nodes[key as usize];
        let old = node.class_props.take().map(|b| *b).unwrap_or_default();
        if old == props {
            node.class_props = (props != ClassProps::default()).then(|| Box::new(props));
            return;
        }
        let paragraph = old.number_of_lines != props.number_of_lines
            || old.selectable != props.selectable
            || old.ellipsize != props.ellipsize
            || old.no_wrap != props.no_wrap;
        let view_props = old.pointer_events != props.pointer_events;
        let image = old.resize_mode != props.resize_mode;
        let input = old.placeholder_color != props.placeholder_color
            || old.caret_color != props.caret_color;
        node.class_props = (props != ClassProps::default()).then(|| Box::new(props));
        let kind = node.kind;
        if paragraph && kind == Kind::Text {
            self.mark_text_block(key);
        }
        if view_props {
            self.mark(key, dirty::VIEW_PROPS);
        }
        if image {
            self.mark(key, dirty::IMAGE);
        }
        if input && kind == Kind::Input {
            self.mark(key, dirty::INPUT);
        }
    }
}

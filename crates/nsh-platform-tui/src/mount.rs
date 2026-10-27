use std::{
    collections::{HashMap, HashSet},
    mem::{Discriminant, discriminant},
};

use nsh_core::{Color, Command, MountBatch, MountItem, NodeId, ROOT_ID, ViewKind};

use crate::{Appearance, ComponentFactory, view::NshView};

pub(crate) struct Mounter {
    pub(crate) views: Vec<Option<NshView>>,
    pub(crate) components: HashMap<String, ComponentFactory>,
    pub(crate) appearance: Option<Appearance>,
    pub(crate) exit: bool,
    pub(crate) page_backgrounds: Vec<Color>,
    unhandled: HashSet<Discriminant<MountItem>>,
    placeholders: HashSet<String>,
}

impl Default for Mounter {
    fn default() -> Self {
        let mut this = Self {
            views: Vec::new(),
            components: HashMap::new(),
            appearance: None,
            exit: false,
            page_backgrounds: Vec::new(),
            unhandled: HashSet::new(),
            placeholders: HashSet::new(),
        };
        this.insert(ROOT_ID, NshView::new(ViewKind::View));
        this
    }
}

impl Mounter {
    fn insert(&mut self, id: NodeId, view: NshView) {
        self.views
            .resize_with(self.views.len().max(id as usize + 1), || None);
        self.views[id as usize] = Some(view);
    }

    pub(crate) fn get(&self, id: NodeId) -> Option<&NshView> {
        self.views.get(id as usize)?.as_ref()
    }

    pub(crate) fn get_mut(&mut self, id: NodeId) -> Option<&mut NshView> {
        self.views.get_mut(id as usize)?.as_mut()
    }

    fn detach(&mut self, id: NodeId) {
        let parent = self.get_mut(id).and_then(|v| v.parent.take());
        if let Some(parent) = parent.and_then(|p| self.get_mut(p)) {
            parent.children.retain(|&child| child != id);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.views.clear();
        self.insert(ROOT_ID, NshView::new(ViewKind::View));
        self.appearance = None;
        self.exit = false;
        self.unhandled.clear();
        self.placeholders.clear();
        self.page_backgrounds.clear();
    }

    pub(crate) fn apply(&mut self, batch: MountBatch) {
        for item in batch.items {
            match item {
                MountItem::Create { id, kind } => {
                    if !matches!(kind, ViewKind::View | ViewKind::Text | ViewKind::Native)
                        && self.placeholders.insert(format!("{kind:?}"))
                    {
                        log::warn!("terminal: {kind:?} shown as a placeholder");
                    }
                    self.insert(id, NshView::new(kind));
                }
                MountItem::Insert {
                    parent,
                    child,
                    before,
                } => {
                    self.detach(child);
                    if self.get(parent).is_none() || self.get(child).is_none() {
                        continue;
                    }
                    self.get_mut(child).unwrap().parent = Some(parent);
                    let siblings = &mut self.get_mut(parent).unwrap().children;
                    let index = before
                        .and_then(|b| siblings.iter().position(|&id| id == b))
                        .unwrap_or(siblings.len());
                    siblings.insert(index, child);
                }
                MountItem::Remove { id } => self.detach(id),
                MountItem::Delete { id } => {
                    self.detach(id);
                    if let Some(slot) = self.views.get_mut(id as usize) {
                        *slot = None;
                    }
                }
                MountItem::Frame { id, frame } => {
                    if let Some(view) = self.get_mut(id) {
                        view.frame = frame;
                    }
                }
                MountItem::ViewStyle { id, style } => {
                    if let Some(view) = self.get_mut(id) {
                        view.style = *style;
                    }
                }
                MountItem::ViewProps { id, props } => {
                    if let Some(view) = self.get_mut(id) {
                        view.props = *props;
                    }
                }
                MountItem::Events { id, events } => {
                    if let Some(view) = self.get_mut(id) {
                        view.events = events;
                    }
                }
                MountItem::Text {
                    id,
                    layout,
                    content,
                    ..
                } => {
                    if let Some(view) = self.get_mut(id) {
                        view.text = Some((layout, content));
                    }
                }
                MountItem::Appearance { appearance } => self.appearance = Some(appearance),
                MountItem::Component { id, name } => {
                    let component = self.components.get(name.as_ref()).map(|factory| factory());
                    if component.is_none() && self.placeholders.insert(name.to_string()) {
                        log::warn!("terminal: native component {name} shown as a placeholder");
                    }
                    if let Some(view) = self.get_mut(id) {
                        view.component = component;
                        view.component_name = Some(name.to_string());
                    }
                }
                MountItem::ComponentProps { id, props } => {
                    if let Some(component) = self.get_mut(id).and_then(|v| v.component.as_mut()) {
                        component.set_props(&props);
                    }
                }
                MountItem::Command {
                    command: Command::ExitApp,
                    ..
                } => self.exit = true,
                MountItem::Command { id, ref command }
                    if self
                        .get_mut(id)
                        .and_then(|v| v.component.as_mut())
                        .is_some_and(|c| c.command(command)) => {}
                MountItem::AccessibilityLabel { id, label } => {
                    if let Some(view) = self.get_mut(id) {
                        view.props.accessibility.label = label;
                    }
                }
                // Text, layout and accessibility already have their core representation.
                MountItem::Overflow { .. } | MountItem::LayoutDirection { .. } => {}
                other => {
                    if self.unhandled.insert(discriminant(&other)) {
                        log::warn!("terminal: mount operation not implemented: {other:?}");
                    }
                }
            }
        }
        self.page_backgrounds.clear();
        collect_page_backgrounds(&self.views, ROOT_ID, &mut self.page_backgrounds);
    }
}

fn collect_page_backgrounds(views: &[Option<NshView>], id: NodeId, colors: &mut Vec<Color>) {
    let Some(view) = views.get(id as usize).and_then(Option::as_ref) else {
        return;
    };
    if id == ROOT_ID
        || view.parent == Some(ROOT_ID)
        || matches!(view.kind, ViewKind::Screen | ViewKind::Modal)
    {
        let fill = view.style.background_color;
        if !fill.is_transparent() && !colors.contains(&fill) {
            colors.push(fill);
        }
    }
    for &child in &view.children {
        collect_page_backgrounds(views, child, colors);
    }
}

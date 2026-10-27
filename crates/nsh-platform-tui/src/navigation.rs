//! Native stack decisions applied at complete mount-batch boundaries.

use nsh_core::{
    EventKind, NodeId, ROOT_ID, ScreenConfig, ViewKind,
    navigation::{Reported, insets_payload},
};
use nsh_engine::EngineControl;

use crate::mount::Mounter;

#[derive(Default)]
pub(crate) struct Screen {
    pub(crate) config: ScreenConfig,
    reported: Reported,
}

pub(crate) fn end_batch(tree: &mut Mounter) {
    let mut dirty = std::mem::take(&mut tree.dirty_stacks);
    for &id in &dirty {
        let Some(stack) = tree.get(id) else {
            continue;
        };
        let screens: Vec<_> = stack
            .children
            .iter()
            .copied()
            .filter(|&child| {
                tree.get(child)
                    .is_some_and(|view| view.kind == ViewKind::Screen)
            })
            .collect();
        let Some(state) = tree.get_mut(id).and_then(|view| view.stack.as_mut()) else {
            continue;
        };
        state.reconcile(&screens);
        let top = state.top();
        for screen in screens {
            if let Some(view) = tree.get_mut(screen) {
                view.concealed = Some(screen) != top;
            }
        }
    }
    dirty.clear();
    tree.dirty_stacks = dirty;
    let mut dirty = std::mem::take(&mut tree.dirty_screens);
    for &id in &dirty {
        let Some(view) = tree.get(id) else {
            continue;
        };
        let Some(screen) = &view.screen else {
            continue;
        };
        let selected = screen.config.header_view;
        let headers: Vec<_> = view
            .children
            .iter()
            .copied()
            .filter(|&child| {
                tree.get(child)
                    .is_some_and(|view| view.kind == ViewKind::HeaderItem)
            })
            .collect();
        for &header in &headers {
            if let Some(view) = tree.get_mut(header) {
                view.concealed = Some(header) != selected;
            }
        }
        if let Some(header) = selected.filter(|id| headers.contains(id)) {
            let children = &mut tree.get_mut(id).unwrap().children;
            if children.last() != Some(&header) {
                children.retain(|&id| id != header);
                children.push(header);
            }
        }
    }
    dirty.clear();
    tree.dirty_screens = dirty;
}

pub(crate) fn report_shown(tree: &mut Mounter, control: &EngineControl) {
    report_in(tree, ROOT_ID, control);
}

fn report_in(tree: &mut Mounter, id: NodeId, control: &EngineControl) {
    let Some(view) = tree.get(id) else {
        return;
    };
    if view.concealed || view.style.hidden || view.style.opacity <= 0.0 {
        return;
    }
    let children = view.children.len();
    if view.kind == ViewKind::Screen {
        let size = view
            .parent
            .and_then(|parent| tree.get(parent))
            .filter(|parent| parent.kind == ViewKind::Stack)
            .map(|stack| stack.frame.size);
        if let Some(size) = size {
            let screen = tree.get_mut(id).unwrap().screen.as_mut().unwrap();
            let insets = Default::default();
            if let Some(insets_changed) = screen.reported.update(size, insets) {
                control.set_root_size(id, size, insets);
                if insets_changed {
                    control.dispatch_event(id, EventKind::Insets, insets_payload(insets));
                }
            }
        }
    }
    for index in 0..children {
        let child = tree.get(id).unwrap().children[index];
        report_in(tree, child, control);
    }
}

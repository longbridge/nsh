//! One NSH SDK call per mount or native input turn; failed retirement keeps its owner.

use crate::interop::{
    attachment::NativeAttachment,
    sdk::{Batch, EditorCommand, EditorSettings, ViewOptions, WebCommand, WebSettings},
};

impl crate::render::mount::UiContext {
    pub(crate) fn editor_update(&self, settings: EditorSettings) {
        let mut pending = self.arkui.borrow_mut();
        let wake = pending.is_empty();
        if let Some(old) = pending
            .editors
            .iter_mut()
            .find(|old| old.target_id == settings.target_id)
        {
            *old = settings;
        } else {
            pending.editors.push(settings);
        }
        drop(pending);
        if wake {
            self.editor_wake();
        }
    }
    pub(crate) fn editor_command(&self, command: EditorCommand) {
        let mut pending = self.arkui.borrow_mut();
        let wake = pending.is_empty();
        pending.commands.push(command);
        drop(pending);
        if wake {
            self.editor_wake();
        }
    }
    pub(crate) fn editor_remove(&self, target: NativeAttachment) {
        let id = target.id();
        let mut pending = self.arkui.borrow_mut();
        pending.editors.retain(|update| update.target_id != id);
        pending.commands.retain(|command| command.target_id != id);
        drop(pending);
        let target = self.retire_unattached(target);
        if let Some(target) = target {
            self.arkui.borrow_mut().remove_editors.push(id);
            self.arkui_views.borrow_mut().push(target);
        }
        self.editor_wake();
    }
    pub(crate) fn web_update(&self, settings: WebSettings) {
        let mut pending = self.arkui.borrow_mut();
        let wake = pending.is_empty();
        if let Some(old) = pending
            .webs
            .iter_mut()
            .find(|old| old.target_id == settings.target_id)
        {
            *old = settings;
        } else {
            pending.webs.push(settings);
        }
        drop(pending);
        if wake {
            self.editor_wake();
        }
    }
    pub(crate) fn web_command(&self, command: WebCommand) {
        let mut pending = self.arkui.borrow_mut();
        let wake = pending.is_empty();
        pending.web_commands.push(command);
        drop(pending);
        if wake {
            self.editor_wake();
        }
    }
    pub(crate) fn web_remove(&self, target: NativeAttachment) {
        let id = target.id();
        let mut pending = self.arkui.borrow_mut();
        pending.webs.retain(|update| update.target_id != id);
        pending
            .web_commands
            .retain(|command| command.target_id != id);
        drop(pending);
        let target = self.retire_unattached(target);
        if let Some(target) = target {
            self.arkui.borrow_mut().remove_webs.push(id);
            self.arkui_views.borrow_mut().push(target);
        }
        self.editor_wake();
    }
    fn retire_unattached(&self, target: NativeAttachment) -> Option<NativeAttachment> {
        if self.host.has_ui_context() && !matches!(target.with_child(|_| ()), Ok(None)) {
            return Some(target);
        }
        // No SDK owner needs a removal turn in these states. Release the
        // attachment's strong node handles before Node's recursive disposal.
        if let Err(error) = target.invalidate() {
            log::error!("OHOS attachment retirement failed; retaining owner: {error}");
            return Some(target);
        }
        None
    }
    fn editor_wake(&self) {
        if !self.flushing_batch.get()
            && let Some(route) = self.dispatcher.upgrade()
        {
            route.send(crate::app::runtime::Message::Arkui);
        }
    }
    pub(crate) fn flush_arkui(&self) -> crate::render::node::Result<()> {
        if !self.host.has_ui_context() {
            return Ok(());
        }
        if let Some(scene) = self.scene.borrow().as_ref() {
            scene.prepare()?;
        }
        let owners: Vec<_> = {
            let mut nodes = self.composite_nodes.borrow_mut();
            nodes.retain(|node| node.strong_count() != 0);
            nodes.iter().filter_map(std::rc::Weak::upgrade).collect()
        };
        for owner in owners {
            owner.update_wrapper()?;
        }
        loop {
            let dirty = std::mem::take(&mut *self.accessibility_dirty.borrow_mut());
            if dirty.is_empty() {
                break;
            }
            for owner in dirty.into_iter().filter_map(|node| node.upgrade()) {
                owner.accessibility_state.flush(&owner)?;
            }
        }
        let batch = std::mem::take(&mut *self.arkui.borrow_mut());
        if !batch.is_empty() {
            // Take only this call's owners; synchronous callbacks may enqueue a later batch.
            let retired = std::mem::take(&mut *self.arkui_views.borrow_mut());
            if let Err(error) = crate::interop::sdk::apply(self, batch.clone()) {
                // A synchronous native callback may have queued fresher state during this call.
                let mut pending = self.arkui.borrow_mut();
                for update in batch.editors {
                    if !pending
                        .editors
                        .iter()
                        .any(|next| next.target_id == update.target_id)
                        && !pending.remove_editors.contains(&update.target_id)
                    {
                        pending.editors.push(update);
                    }
                }
                for update in batch.views {
                    if !pending
                        .views
                        .iter()
                        .any(|next| next.target_id == update.target_id)
                        && !pending.remove_views.contains(&update.target_id)
                    {
                        pending.views.push(update);
                    }
                }
                for update in batch.webs {
                    if !pending
                        .webs
                        .iter()
                        .any(|next| next.target_id == update.target_id)
                        && !pending.remove_webs.contains(&update.target_id)
                    {
                        pending.webs.push(update);
                    }
                }
                let mut commands = batch.commands;
                commands.append(&mut pending.commands);
                pending.commands = commands;
                let mut web_commands = batch.web_commands;
                web_commands.append(&mut pending.web_commands);
                pending.web_commands = web_commands;
                pending.remove_editors.extend(batch.remove_editors);
                pending.remove_views.extend(batch.remove_views);
                pending.remove_webs.extend(batch.remove_webs);
                self.arkui_views.borrow_mut().extend(retired);
                return Err(error);
            }
            drop(retired);
        }
        let focus = std::mem::take(&mut *self.accessibility_focus.borrow_mut());
        for owner in focus.into_iter().filter_map(|owner| owner.upgrade()) {
            if owner.visible_in_tree() {
                let id = owner.accessibility_id()?;
                self.host.accessibility_event(
                    self.dispatcher.clone(),
                    None,
                    Some(i64::from(id)),
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn view_update(&self, settings: ViewOptions) {
        let mut pending = self.arkui.borrow_mut();
        let wake = pending.is_empty();
        if let Some(old) = pending
            .views
            .iter_mut()
            .find(|old| old.target_id == settings.target_id)
        {
            *old = settings;
        } else {
            pending.views.push(settings);
        }
        drop(pending);
        if wake {
            self.editor_wake();
        }
    }
    pub(crate) fn view_remove(&self, target: NativeAttachment) {
        let id = target.id();
        let mut pending = self.arkui.borrow_mut();
        pending.views.retain(|update| update.target_id != id);
        drop(pending);
        let target = self.retire_unattached(target);
        if let Some(target) = target {
            self.arkui.borrow_mut().remove_views.push(id);
            self.arkui_views.borrow_mut().push(target);
        }
        self.editor_wake();
    }
}

pub(crate) fn empty_batch() -> std::cell::RefCell<Batch> {
    std::cell::RefCell::default()
}

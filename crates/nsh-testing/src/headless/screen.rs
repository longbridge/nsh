//! A model of the native views a headless app builds, from its mount
//! batches: their tree, what they show, which events they report, what
//! inputs hold and where scroll views are. It answers the selectors of the
//! steps ([`Selector`]) as a user's eyes would: the top screen of a stack
//! and the top modal first.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use nsh_core::{
    AccessibilityRole, Command, Dialog, EventKind, EventMask, InputConfig, MountBatch, MountItem,
    NodeId, Rect, Size, ViewKind, ui::PropEnum,
};

use super::selector::{Matcher, Selector, TextMatch};

/// The surface's root view: every app view hangs below it.
const ROOT: NodeId = 1;

/// Whether view `v` shows a text `want` matches: its own, or an input's
/// (what it holds, or while empty its placeholder).
fn shows_text(v: &View, want: &TextMatch) -> bool {
    v.text.as_deref().is_some_and(|t| want.matches(t))
        || v.input.as_ref().is_some_and(|i| {
            want.matches(&i.text) || (i.text.is_empty() && want.matches(&i.config.placeholder))
        })
}

/// What a back does ([`Screen::back_target`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Back {
    /// The stack pops this screen.
    Pop(NodeId),
    /// This screen prevents going back: it is asked.
    Ask(NodeId),
    /// The modal asks to close, and closes when the user may close it.
    Close { modal: NodeId, dismissible: bool },
    /// Nothing to go back from.
    Nothing,
}

/// The native views as the mount items built them: their tree, what they
/// show (text, test id, accessibility label), the events they report,
/// inputs' text, scroll views' offsets, the screens and modals a back
/// took away, the dialogs shown, the clipboard, and the commands sent
/// (focus, scrolling, ...).
#[derive(Debug, Default)]
pub struct Screen {
    nodes: HashMap<NodeId, View>,
    /// Children of each view, bottom to top.
    children: HashMap<NodeId, Vec<NodeId>>,
    commands: Vec<(NodeId, Command)>,
    /// The input that has the focus.
    focused: Option<NodeId>,
    /// Scroll views a command moved, whose offset the engine is told
    /// ([`Screen::take_scrolled`]), as a platform reports it.
    scrolled: Vec<NodeId>,
    /// Screens a back popped natively, until the app removes them.
    popped: HashSet<NodeId>,
    /// Modals a back closed natively, until the app removes them.
    closed: HashSet<NodeId>,
    /// Modals presented since the last call (`onShow`).
    presented: Vec<NodeId>,
    /// The dialogs shown, in order, and how many were answered.
    dialogs: Vec<Dialog>,
    answered: usize,
    /// The clipboard's text.
    clipboard: Option<String>,
    /// Clipboard reads to answer: the request, and whether it asks only if
    /// the clipboard holds text.
    clipboard_reads: Vec<(u32, bool)>,
    /// What the app asked that only a real platform answers, since the
    /// last call.
    unsupported: Vec<&'static str>,
    /// The app asked for a back (`Command::Back`, what no listener took).
    back_asked: bool,
}

#[derive(Debug, Default)]
struct View {
    kind: Option<ViewKind>,
    parent: Option<NodeId>,
    events: EventMask,
    test_id: Option<Arc<str>>,
    label: Option<Arc<str>>,
    text: Option<String>,
    frame: Rect,
    input: Option<Input>,
    scroll: Option<Scroll>,
    /// A screen that asks before going back (`preventBack`).
    prevent_back: bool,
    /// A modal the user can close natively (`ModalConfig::dismissible`).
    dismissible: bool,
    /// Its `accessibilityRole`.
    role: AccessibilityRole,
    /// It takes no presses (`disabled`, its accessibility state).
    disabled: bool,
    /// How long a press must last to be a long one (`delayLongPress`), in
    /// milliseconds.
    long_press_delay: Option<f32>,
}

/// Which views a selector matched, for a step that acts on one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pick {
    /// The view.
    One(NodeId),
    /// None matched.
    None,
    /// Several did: how many.
    Many(usize),
    /// The match takes no presses (it, or the view between it and the one
    /// that would hear them, is disabled).
    Disabled,
}

/// What an input holds, as its platform view keeps it.
#[derive(Debug, Default)]
pub(crate) struct Input {
    pub(crate) text: String,
    /// The edits reported so far ([`nsh_core::EventPayload::Text`]).
    pub(crate) count: u32,
    pub(crate) config: Box<InputConfig>,
}

/// Where a scroll view is, as its platform view keeps it.
#[derive(Debug, Default)]
pub(crate) struct Scroll {
    /// The content offset.
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// The scrolls the core asked for (`ScrollBy`, `ScrollTo`) applied.
    pub(crate) applied: u32,
    pub(crate) horizontal: bool,
    content: Option<Size>,
}

impl Screen {
    /// Applies `batch`.
    pub fn apply(&mut self, batch: &MountBatch) {
        for item in &batch.items {
            self.apply_item(item);
        }
    }

    fn apply_item(&mut self, item: &MountItem) {
        match item {
            MountItem::Create { id, kind } => {
                let view = View {
                    kind: Some(*kind),
                    input: (*kind == ViewKind::Input).then(Input::default),
                    scroll: (*kind == ViewKind::Scroll).then(Scroll::default),
                    ..View::default()
                };
                self.nodes.insert(*id, view);
            }
            MountItem::Insert {
                parent,
                child,
                before,
            } => {
                self.detach(*child);
                let siblings = self.children.entry(*parent).or_default();
                let at = before
                    .and_then(|b| siblings.iter().position(|s| *s == b))
                    .unwrap_or(siblings.len());
                siblings.insert(at, *child);
                self.nodes.entry(*child).or_default().parent = Some(*parent);
                if self.kind(*child) == Some(ViewKind::Modal) {
                    self.presented.push(*child);
                }
            }
            MountItem::Remove { id } => {
                self.detach(*id);
                self.popped.remove(id);
                self.closed.remove(id);
            }
            MountItem::Delete { id } => {
                self.detach(*id);
                self.nodes.remove(id);
                self.children.remove(id);
                self.popped.remove(id);
                self.closed.remove(id);
                if self.focused == Some(*id) {
                    self.focused = None;
                }
            }
            MountItem::ScreenConfig { id, config } => {
                self.nodes.entry(*id).or_default().prevent_back = config.prevent_back;
            }
            MountItem::ModalConfig { id, config } => {
                self.nodes.entry(*id).or_default().dismissible = config.dismissible;
            }
            MountItem::Frame { id, frame } => self.nodes.entry(*id).or_default().frame = *frame,
            MountItem::Events { id, events } => self.nodes.entry(*id).or_default().events = *events,
            MountItem::ViewProps { id, props } => {
                let view = self.nodes.entry(*id).or_default();
                view.test_id = props.test_id.clone();
                view.label = props.accessibility.label.clone();
                view.role = props.accessibility.role;
                view.disabled = props.accessibility.state.disabled;
                view.long_press_delay = props.long_press_delay;
            }
            MountItem::AccessibilityLabel { id, label } => {
                self.nodes.entry(*id).or_default().label = label.clone();
            }
            MountItem::Text { id, text, .. } => {
                self.nodes.entry(*id).or_default().text = Some(text.text.clone());
            }
            MountItem::Input { id, config } => {
                if let Some(input) = self.input_mut(*id) {
                    input.config = config.clone();
                }
            }
            MountItem::InputValue { id, text, count } => {
                // A value based on fewer edits than the user made is stale.
                if let Some(input) = self.input_mut(*id)
                    && *count >= input.count
                {
                    input.text = text.clone();
                }
            }
            MountItem::ScrollConfig { id, horizontal, .. } => {
                if let Some(scroll) = self.scroll_mut(*id) {
                    scroll.horizontal = *horizontal;
                }
            }
            MountItem::ScrollContent { id, size } => {
                if let Some(scroll) = self.scroll_mut(*id) {
                    scroll.content = Some(*size);
                }
            }
            MountItem::ScrollBy { id, dx, dy } => {
                // Not clamped: the core knows its content.
                if let Some(scroll) = self.scroll_mut(*id) {
                    scroll.applied = scroll.applied.wrapping_add(1);
                    if scroll.horizontal {
                        scroll.x += dx;
                    } else {
                        scroll.y += dy;
                    }
                }
            }
            MountItem::Command { id, command } => {
                self.command(*id, command);
                self.commands.push((*id, command.clone()));
            }
            _ => {}
        }
    }

    /// What a platform view does with `command`, as far as the model
    /// keeps it.
    fn command(&mut self, id: NodeId, command: &Command) {
        match command {
            Command::Dialog(dialog) => self.dialogs.push((**dialog).clone()),
            Command::SetClipboard(text) => self.clipboard = Some(text.clone()),
            // An image replaces the text.
            Command::SetClipboardImage(_) => self.clipboard = None,
            Command::ReadClipboard { id, has_only } => self.clipboard_reads.push((*id, *has_only)),
            Command::ReadClipboardImage { .. } => {
                self.unsupported.push("reading an image from the clipboard");
            }
            Command::ReadClipboardFiles { .. } => {
                self.unsupported.push("reading files from the clipboard");
            }
            Command::ImageInfo { .. } => {
                self.unsupported
                    .push("an image's size (`Image.getSize`, `Image.prefetch`)");
            }
            Command::Back => self.back_asked = true,
            Command::Focus if self.input(id).is_some() => self.focused = Some(id),
            Command::Blur if id == ROOT || self.focused == Some(id) => self.focused = None,
            Command::ScrollTo { x, y, .. } => {
                let end = self.scroll_end(id);
                if let Some(scroll) = self.scroll_mut(id) {
                    if scroll.horizontal {
                        scroll.x = x.clamp(0.0, end);
                    } else {
                        scroll.y = y.clamp(0.0, end);
                    }
                    scroll.applied = scroll.applied.wrapping_add(1);
                    self.scrolled.push(id);
                }
            }
            Command::ScrollToEnd { .. } => {
                let end = self.scroll_end(id);
                if let Some(scroll) = self.scroll_mut(id) {
                    if scroll.horizontal {
                        scroll.x = end;
                    } else {
                        scroll.y = end;
                    }
                    scroll.applied = scroll.applied.wrapping_add(1);
                    self.scrolled.push(id);
                }
            }
            _ => {}
        }
    }

    fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self.nodes.get_mut(&id).and_then(|v| v.parent.take())
            && let Some(siblings) = self.children.get_mut(&parent)
        {
            siblings.retain(|s| *s != id);
        }
    }

    /// The views on screen, in tree order: below the root, of a stack
    /// only its top screen (the others are covered), without what a back
    /// took away.
    fn attached(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![ROOT];
        while let Some(id) = stack.pop() {
            if self.closed.contains(&id) {
                continue;
            }
            if id != ROOT {
                out.push(id);
            }
            if self.kind(id) == Some(ViewKind::Stack) {
                stack.extend(self.screens(id).last());
            } else if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().rev());
            }
        }
        out
    }

    /// The screens stack `id` shows, bottom to top (without those a back
    /// popped).
    fn screens(&self, id: NodeId) -> Vec<NodeId> {
        self.children
            .get(&id)
            .map(|c| {
                c.iter()
                    .copied()
                    .filter(|s| !self.popped.contains(s))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// What going back does now, as a platform's back does it: in the
    /// newest modal (else the whole screen) the innermost stack that can go
    /// back pops its top screen, or asks it when it prevents going back;
    /// without one, the newest modal asks to close.
    pub(crate) fn back_target(&self) -> Back {
        let attached = self.attached();
        let modal = attached
            .iter()
            .rev()
            .copied()
            .find(|id| self.kind(*id) == Some(ViewKind::Modal));
        let can_go_back = |stack: NodeId| {
            let screens = self.screens(stack);
            screens.len() >= 2
                || screens
                    .last()
                    .is_some_and(|s| self.nodes.get(s).is_some_and(|v| v.prevent_back))
        };
        let stacks: Vec<NodeId> = attached
            .iter()
            .copied()
            .filter(|id| {
                self.kind(*id) == Some(ViewKind::Stack)
                    && modal.is_none_or(|m| self.is_inside(*id, m))
                    && can_go_back(*id)
            })
            .collect();
        let innermost = stacks
            .iter()
            .copied()
            .find(|s| !stacks.iter().any(|o| o != s && self.is_inside(*o, *s)));
        if let Some(stack) = innermost
            && let Some(&top) = self.screens(stack).last()
        {
            return if self.nodes.get(&top).is_some_and(|v| v.prevent_back) {
                Back::Ask(top)
            } else {
                Back::Pop(top)
            };
        }
        match modal {
            Some(modal) => Back::Close {
                modal,
                dismissible: self.nodes.get(&modal).is_some_and(|v| v.dismissible),
            },
            None => Back::Nothing,
        }
    }

    /// A back popped `screen` natively: it is gone until the app removes it.
    pub(crate) fn pop(&mut self, screen: NodeId) {
        self.popped.insert(screen);
    }

    /// A back closed `modal` natively.
    pub(crate) fn close(&mut self, modal: NodeId) {
        self.closed.insert(modal);
    }

    /// The modals presented since the last call.
    pub(crate) fn take_presented(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.presented)
    }

    /// Whether the app asked for a back since the last call.
    pub(crate) fn take_back_asked(&mut self) -> bool {
        std::mem::take(&mut self.back_asked)
    }

    /// The dialogs shown so far, in order (`Alert.alert`, action sheets).
    pub fn dialogs(&self) -> &[Dialog] {
        &self.dialogs
    }

    /// The oldest dialog not answered yet.
    pub(crate) fn pending_dialog(&self) -> Option<&Dialog> {
        self.dialogs.get(self.answered)
    }

    /// The oldest dialog was answered.
    pub(crate) fn answered(&mut self) {
        self.answered = (self.answered + 1).min(self.dialogs.len());
    }

    /// The clipboard's text.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    pub(crate) fn set_clipboard(&mut self, text: Option<String>) {
        self.clipboard = text;
    }

    /// The clipboard reads asked since the last call.
    pub(crate) fn take_clipboard_reads(&mut self) -> Vec<(u32, bool)> {
        std::mem::take(&mut self.clipboard_reads)
    }

    /// What the app asked that only a real platform answers, since the
    /// last call.
    pub(crate) fn take_unsupported(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.unsupported)
    }

    /// The views on screen in the order a user's eyes find them: the
    /// topmost modal's first (the last presented), then the rest.
    fn by_layer(&self) -> Vec<NodeId> {
        let attached = self.attached();
        let modals: Vec<NodeId> = attached
            .iter()
            .copied()
            .filter(|id| self.kind(*id) == Some(ViewKind::Modal))
            .collect();
        let Some(top) = modals.last() else {
            return attached;
        };
        let (mut first, rest): (Vec<NodeId>, Vec<NodeId>) = attached
            .into_iter()
            .partition(|id| *id == *top || self.is_inside(*id, *top));
        first.extend(rest);
        first
    }

    fn is_inside(&self, mut id: NodeId, ancestor: NodeId) -> bool {
        while let Some(parent) = self.nodes.get(&id).and_then(|v| v.parent) {
            if parent == ancestor {
                return true;
            }
            id = parent;
        }
        false
    }

    fn kind(&self, id: NodeId) -> Option<ViewKind> {
        self.nodes.get(&id)?.kind
    }

    /// The texts on screen, in tree order.
    pub fn texts(&self) -> Vec<&str> {
        self.attached()
            .into_iter()
            .filter_map(|id| self.nodes.get(&id)?.text.as_deref())
            .collect()
    }

    /// Whether a view on screen shows the text `text` (spaces around it
    /// aside).
    pub fn shows(&self, text: &str) -> bool {
        self.texts().iter().any(|t| t.trim() == text)
    }

    /// Whether a view on screen matches `selector` (the module
    /// `selector`'s: `id:<testID>`, `label:<label>`, `text:<text>`,
    /// `role:<role>[<name>]`, `view:<id>`, a bare string; ` >> ` looks
    /// inside, `nth=<n>` picks); an error for a selector that does not
    /// parse.
    pub fn matches(&self, selector: &str) -> Result<bool, String> {
        self.count(selector).map(|n| n > 0)
    }

    /// How many views on screen match `selector` ([`Screen::matches`]).
    pub fn count(&self, selector: &str) -> Result<usize, String> {
        Ok(self.resolve(&Selector::parse(selector)?).len())
    }

    /// The commands sent so far, in order, with the view each went to.
    pub fn commands(&self) -> &[(NodeId, Command)] {
        &self.commands
    }

    /// Whether the command `command` was sent.
    pub fn sent(&self, command: &Command) -> bool {
        self.commands.iter().any(|(_, c)| c == command)
    }

    /// The view a press on `selector` goes to ([`Screen::matches`]'s
    /// selectors): the matching view, or its nearest ancestor that hears
    /// presses (a text inside a button). `None` when none matches, or it is
    /// disabled; an error for a selector that does not parse or matches
    /// views a press would reach apart.
    pub fn press_target(&self, selector: &str) -> Result<Option<NodeId>, String> {
        let parsed = Selector::parse(selector)?;
        match self.pressable(&parsed) {
            Pick::One(id) => Ok(Some(id)),
            Pick::None | Pick::Disabled => Ok(None),
            Pick::Many(n) => Err(format!("`{selector}` matches {n} views")),
        }
    }

    /// The text of the input `selector` names ([`Screen::matches`]'s
    /// selectors; `text:` matches an input's text or placeholder); an
    /// error when it names several.
    pub fn input_text(&self, selector: &str) -> Result<Option<&str>, String> {
        let parsed = Selector::parse(selector)?;
        match self.input_view(&parsed) {
            Pick::One(id) => Ok(self.input(id).map(|input| input.text.as_str())),
            Pick::Many(n) => Err(format!("`{selector}` matches {n} inputs")),
            Pick::None | Pick::Disabled => Ok(None),
        }
    }

    /// Whether the view `selector` names takes input: neither it nor a view
    /// around it is disabled, and an input is editable; an error when none
    /// or several match.
    pub fn enabled(&self, selector: &str) -> Result<bool, String> {
        let parsed = Selector::parse(selector)?;
        match self.resolve(&parsed)[..] {
            [id] => {
                Ok(!self.disabled_from(id, None)
                    && self.input(id).is_none_or(|i| i.config.editable))
            }
            [] => Err(format!("no view matches `{selector}`")),
            ref all => Err(format!("`{selector}` matches {} views", all.len())),
        }
    }

    /// The views on screen `selector` matches, in the order a user's eyes
    /// find them ([`Screen::by_layer`]): each part looks inside what the
    /// one before matched.
    pub(crate) fn resolve(&self, selector: &Selector) -> Vec<NodeId> {
        let layers = self.by_layer();
        let mut scope: Option<Vec<NodeId>> = None;
        for part in &selector.parts {
            let inside = |id: NodeId| {
                scope
                    .as_ref()
                    .is_none_or(|scope| scope.iter().any(|s| self.is_inside(id, *s)))
            };
            let matching = |test: &dyn Fn(NodeId, &View) -> bool| -> Vec<NodeId> {
                layers
                    .iter()
                    .copied()
                    .filter(|id| inside(*id) && self.nodes.get(id).is_some_and(|v| test(*id, v)))
                    .collect()
            };
            let is = |value: &Option<Arc<str>>, want: &str| value.as_deref() == Some(want);
            let mut found = match &part.matcher {
                Matcher::TestId(want) => matching(&|_, v| is(&v.test_id, want)),
                Matcher::Label(want) => {
                    matching(&|_, v| v.label.as_deref().is_some_and(|l| want.matches(l)))
                }
                Matcher::Text(want) => matching(&|_, v| shows_text(v, want)),
                Matcher::Role { role, name } => matching(&|id, v| {
                    v.role != AccessibilityRole::None
                        && v.role.js_name() == role
                        && name
                            .as_ref()
                            .is_none_or(|name| name.matches(&self.name(id)))
                }),
                Matcher::View(id) => layers
                    .iter()
                    .copied()
                    .filter(|v| v == id && inside(*v))
                    .collect(),
                Matcher::Any(want) => {
                    let exact = TextMatch::Exact(want.trim().to_owned());
                    let ids = matching(&|_, v| is(&v.test_id, want));
                    let ids = if ids.is_empty() {
                        matching(&|_, v| is(&v.label, want))
                    } else {
                        ids
                    };
                    if ids.is_empty() {
                        matching(&|_, v| shows_text(v, &exact))
                    } else {
                        ids
                    }
                }
            };
            if let Some(n) = part.nth {
                found = found.get(n).copied().into_iter().collect();
            }
            scope = Some(found);
        }
        scope.unwrap_or_default()
    }

    /// The one view the matches of `selector` lead to through `target`
    /// (several matches leading to one view are one).
    fn pick(&self, selector: &Selector, target: impl Fn(NodeId) -> Option<NodeId>) -> Pick {
        let mut targets: Vec<NodeId> = Vec::new();
        for id in self.resolve(selector) {
            if let Some(t) = target(id)
                && !targets.contains(&t)
            {
                targets.push(t);
            }
        }
        match targets[..] {
            [] => Pick::None,
            [one] => Pick::One(one),
            ref all => Pick::Many(all.len()),
        }
    }

    /// The view a press on the selector's match goes to: it, or its nearest
    /// pressable ancestor (a text inside a button); [`Pick::Disabled`] when
    /// it or a view on the way is disabled.
    pub(crate) fn pressable(&self, selector: &Selector) -> Pick {
        let matches = self.resolve(selector);
        if !matches.is_empty()
            && matches.iter().all(|id| {
                let target = self.up_to(*id, |v| v.events.contains(EventKind::Press));
                self.disabled_from(*id, target)
            })
        {
            return Pick::Disabled;
        }
        self.pick(selector, |id| {
            let target = self.up_to(id, |v| v.events.contains(EventKind::Press))?;
            (!self.disabled_from(id, Some(target))).then_some(target)
        })
    }

    /// Whether `id` or a view around it, up to `until` (else the root), is
    /// disabled.
    fn disabled_from(&self, mut id: NodeId, until: Option<NodeId>) -> bool {
        loop {
            let Some(view) = self.nodes.get(&id) else {
                return false;
            };
            if view.disabled {
                return true;
            }
            if Some(id) == until {
                return false;
            }
            match view.parent {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }

    /// The input the selector names: the matching view, or the first input
    /// in it (a text field's frame).
    pub(crate) fn input_view(&self, selector: &Selector) -> Pick {
        self.pick(selector, |id| self.down_to(id, |v| v.input.is_some()))
    }

    /// The scroll view the selector names: the matching view, or the
    /// nearest scroll view around it (a row of a list), else the first in
    /// it.
    pub(crate) fn scroll_view(&self, selector: &Selector) -> Pick {
        let is_scroll = |v: &View| v.scroll.is_some();
        self.pick(selector, |id| {
            self.up_to(id, is_scroll)
                .or_else(|| self.down_to(id, is_scroll))
        })
    }

    /// The view the selector names, for a long press: it, or its nearest
    /// ancestor that hears long presses or presses.
    pub(crate) fn long_pressable(&self, selector: &Selector) -> Pick {
        self.pick(selector, |id| {
            let target = self.up_to(id, |v| {
                v.events.contains(EventKind::LongPress) || v.events.contains(EventKind::Press)
            })?;
            (!self.disabled_from(id, Some(target))).then_some(target)
        })
    }

    /// How long a press on `id` must last to be a long one, in milliseconds.
    pub(crate) fn long_press_delay(&self, id: NodeId) -> Option<f32> {
        self.nodes.get(&id)?.long_press_delay
    }

    /// A view's accessible name: its label, else the texts in it.
    fn name(&self, id: NodeId) -> String {
        if let Some(label) = self.nodes.get(&id).and_then(|v| v.label.as_deref()) {
            return label.to_owned();
        }
        let mut texts = Vec::new();
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if let Some(text) = self.nodes.get(&id).and_then(|v| v.text.as_deref()) {
                texts.push(text.trim());
            }
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().rev());
            }
        }
        texts.join(" ")
    }

    /// `id`, or its nearest ancestor, that `test` accepts.
    fn up_to(&self, mut id: NodeId, test: impl Fn(&View) -> bool) -> Option<NodeId> {
        loop {
            let view = self.nodes.get(&id)?;
            if test(view) {
                return Some(id);
            }
            id = view.parent?;
        }
    }

    /// `id`, or the first view in it, in tree order, that `test` accepts.
    fn down_to(&self, id: NodeId, test: impl Fn(&View) -> bool) -> Option<NodeId> {
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if self.nodes.get(&id).is_some_and(&test) {
                return Some(id);
            }
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().rev());
            }
        }
        None
    }

    /// Whether view `id` reports `event`.
    pub(crate) fn hears(&self, id: NodeId, event: EventKind) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(|v| v.events.contains(event))
    }

    pub(crate) fn input(&self, id: NodeId) -> Option<&Input> {
        self.nodes.get(&id)?.input.as_ref()
    }

    pub(crate) fn input_mut(&mut self, id: NodeId) -> Option<&mut Input> {
        self.nodes.get_mut(&id)?.input.as_mut()
    }

    /// The input that has the focus.
    pub(crate) fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    pub(crate) fn set_focused(&mut self, id: Option<NodeId>) {
        self.focused = id;
    }

    pub(crate) fn scroll(&self, id: NodeId) -> Option<&Scroll> {
        self.nodes.get(&id)?.scroll.as_ref()
    }

    pub(crate) fn scroll_mut(&mut self, id: NodeId) -> Option<&mut Scroll> {
        self.nodes.get_mut(&id)?.scroll.as_mut()
    }

    /// How far scroll view `id` goes along its axis: its content past its
    /// frame (unbounded while the content's size is not known).
    pub(crate) fn scroll_end(&self, id: NodeId) -> f32 {
        let Some(view) = self.nodes.get(&id) else {
            return 0.0;
        };
        let Some(scroll) = &view.scroll else {
            return 0.0;
        };
        match scroll.content {
            Some(content) if scroll.horizontal => (content.width - view.frame.width).max(0.0),
            Some(content) => (content.height - view.frame.height).max(0.0),
            None => f32::INFINITY,
        }
    }

    /// The scroll views a command moved since the last call.
    pub(crate) fn take_scrolled(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.scrolled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(items: Vec<MountItem>) -> MountBatch {
        MountBatch { items }
    }

    fn text(id: NodeId, value: &str) -> MountItem {
        MountItem::Text {
            id,
            text: Arc::new(nsh_core::AttributedText {
                text: value.into(),
                ..Default::default()
            }),
            layout: nsh_core::TextLayout(Arc::new(())),
            content: Rect::default(),
        }
    }

    #[test]
    fn only_the_top_screen_of_a_stack_and_the_top_modal_first_are_seen() {
        let mut screen = Screen::default();
        let create = |id, kind| MountItem::Create { id, kind };
        let insert = |parent, child| MountItem::Insert {
            parent,
            child,
            before: None,
        };
        screen.apply(&batch(vec![
            create(2, ViewKind::Stack),
            create(4, ViewKind::Screen),
            create(6, ViewKind::Text),
            create(8, ViewKind::Screen),
            create(10, ViewKind::Text),
            create(12, ViewKind::Modal),
            create(14, ViewKind::Text),
            text(6, "Home"),
            text(10, "Details"),
            text(14, "Close"),
            insert(1, 2),
            insert(2, 4),
            insert(4, 6),
            insert(2, 8),
            insert(8, 10),
        ]));
        assert_eq!(screen.texts(), ["Details"]);
        screen.apply(&batch(vec![
            text(10, "Close"),
            insert(1, 12),
            insert(12, 14),
        ]));
        assert_eq!(screen.texts(), ["Close", "Close"]);
        // The modal's text comes first, though the stack is before it.
        assert_eq!(
            screen.resolve(&Selector::parse("text:Close").unwrap()),
            [14, 10]
        );
        screen.apply(&batch(vec![MountItem::Remove { id: 8 }]));
        assert_eq!(screen.texts(), ["Home", "Close"]);
    }
}

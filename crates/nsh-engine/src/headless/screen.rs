//! A model of the native views a headless app builds, from its mount
//! batches: their tree, what they show, which events they report, what
//! inputs hold and where scroll views are. It answers the selectors of the
//! steps ([`Selector`]) as a user's eyes would: the top screen of a stack
//! and the top modal first.

use std::{collections::HashMap, sync::Arc};

use nsh_core::{
    Command, EventKind, EventMask, InputConfig, MountBatch, MountItem, NodeId, Rect, Size, ViewKind,
};

/// The surface's root view: every app view hangs below it.
const ROOT: NodeId = 1;

/// What a step names: `id:<testID>`, `label:<accessibilityLabel>`,
/// `text:<text>`, `view:<node id>`, or a bare string, matched as each of the
/// first three in turn.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Selector {
    TestId(String),
    Label(String),
    Text(String),
    View(NodeId),
    Any(String),
}

impl Selector {
    pub(crate) fn parse(s: &str) -> Result<Self, String> {
        Ok(match s.split_once(':') {
            Some(("id", v)) => Self::TestId(v.into()),
            Some(("label", v)) => Self::Label(v.into()),
            Some(("text", v)) => Self::Text(v.into()),
            Some(("view", v)) => Self::View(
                v.parse()
                    .map_err(|e| format!("`{s}`: not a view id: {e}"))?,
            ),
            _ => Self::Any(s.into()),
        })
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TestId(v) => write!(f, "id:{v}"),
            Self::Label(v) => write!(f, "label:{v}"),
            Self::Text(v) => write!(f, "text:{v}"),
            Self::View(v) => write!(f, "view:{v}"),
            Self::Any(v) => f.write_str(v),
        }
    }
}

/// The native views as the mount items built them: their tree, what they
/// show (text, test id, accessibility label), the events they report,
/// inputs' text, scroll views' offsets, and the commands sent (the
/// clipboard, focus, scrolling, ...).
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
            }
            MountItem::Remove { id } => self.detach(*id),
            MountItem::Delete { id } => {
                self.detach(*id);
                self.nodes.remove(id);
                self.children.remove(id);
                if self.focused == Some(*id) {
                    self.focused = None;
                }
            }
            MountItem::Frame { id, frame } => self.nodes.entry(*id).or_default().frame = *frame,
            MountItem::Events { id, events } => self.nodes.entry(*id).or_default().events = *events,
            MountItem::ViewProps { id, props } => {
                let view = self.nodes.entry(*id).or_default();
                view.test_id = props.test_id.clone();
                view.label = props.accessibility.label.clone();
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

    /// The views on screen, in tree order: below the root, and of a
    /// stack only its top screen (the others are covered).
    fn attached(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![ROOT];
        while let Some(id) = stack.pop() {
            if id != ROOT {
                out.push(id);
            }
            let Some(children) = self.children.get(&id) else {
                continue;
            };
            if self.kind(id) == Some(ViewKind::Stack) {
                stack.extend(children.last());
            } else {
                stack.extend(children.iter().rev());
            }
        }
        out
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

    /// Whether a view on screen matches `selector` (`id:<testID>`,
    /// `label:<label>`, `text:<text>`, `view:<id>`, or a bare string
    /// matched as each of the first three in turn); an error for a
    /// selector that does not parse.
    pub fn matches(&self, selector: &str) -> Result<bool, String> {
        let selector = Selector::parse(selector)?;
        Ok(self.first(&selector, Some).is_some())
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
    /// selectors): the first matching view on screen, or its nearest
    /// ancestor that hears presses (a text inside a button). `None` when
    /// none matches; an error for a selector that does not parse.
    pub fn press_target(&self, selector: &str) -> Result<Option<NodeId>, String> {
        Selector::parse(selector).map(|s| self.pressable(&s))
    }

    /// The text of the input `selector` names ([`Screen::matches`]'s
    /// selectors; `text:` matches an input's text or placeholder).
    pub fn input_text(&self, selector: &str) -> Result<Option<&str>, String> {
        let selector = Selector::parse(selector)?;
        Ok(self
            .input_view(&selector)
            .and_then(|id| self.input(id))
            .map(|input| input.text.as_str()))
    }

    /// The first view on screen that matches `selector`, where `find` (the
    /// view and its id) gives what is wanted of it.
    fn first<T>(&self, selector: &Selector, find: impl Fn(NodeId) -> Option<T>) -> Option<T> {
        let layers = self.by_layer();
        let matching = |test: &dyn Fn(&View) -> bool| {
            layers.iter().find_map(|id| {
                self.nodes
                    .get(id)
                    .filter(|v| test(v))
                    .and_then(|_| find(*id))
            })
        };
        let is = |value: &Option<Arc<str>>, want: &str| value.as_deref() == Some(want);
        // Spaces around a text are not seen, on either side.
        let text = |v: &View, want: &str| {
            let want = want.trim();
            v.text.as_deref().map(str::trim) == Some(want)
                || v.input.as_ref().is_some_and(|i| {
                    i.text.trim() == want || (i.text.is_empty() && i.config.placeholder == want)
                })
        };
        match selector {
            Selector::TestId(want) => matching(&|v| is(&v.test_id, want)),
            Selector::Label(want) => matching(&|v| is(&v.label, want)),
            Selector::Text(want) => matching(&|v| text(v, want)),
            Selector::View(id) => layers.contains(id).then(|| find(*id)).flatten(),
            Selector::Any(want) => matching(&|v| is(&v.test_id, want))
                .or_else(|| matching(&|v| is(&v.label, want)))
                .or_else(|| matching(&|v| text(v, want))),
        }
    }

    /// The first view on screen the selector matches, or its nearest
    /// pressable ancestor (a text inside a button).
    pub(crate) fn pressable(&self, selector: &Selector) -> Option<NodeId> {
        self.first(selector, |id| {
            self.up_to(id, |v| v.events.contains(EventKind::Press))
        })
    }

    /// The input the selector names: the matching view, or the first input
    /// in it (a text field's frame).
    pub(crate) fn input_view(&self, selector: &Selector) -> Option<NodeId> {
        self.first(selector, |id| self.down_to(id, |v| v.input.is_some()))
    }

    /// The scroll view the selector names: the matching view, or the
    /// nearest scroll view around it (a row of a list), else the first in
    /// it.
    pub(crate) fn scroll_view(&self, selector: &Selector) -> Option<NodeId> {
        let is_scroll = |v: &View| v.scroll.is_some();
        self.first(selector, |id| {
            self.up_to(id, is_scroll)
                .or_else(|| self.down_to(id, is_scroll))
        })
    }

    /// The first view on screen that hears presses.
    pub(crate) fn first_pressable(&self) -> Option<NodeId> {
        self.by_layer()
            .into_iter()
            .find(|id| self.hears(*id, EventKind::Press))
    }

    /// The first scroll view on screen.
    pub(crate) fn first_scroll_view(&self) -> Option<NodeId> {
        self.by_layer()
            .into_iter()
            .find(|id| self.scroll(*id).is_some())
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

    #[test]
    fn selectors_parse() {
        assert_eq!(
            Selector::parse("id:save").unwrap(),
            Selector::TestId("save".into())
        );
        assert_eq!(
            Selector::parse("label:Close").unwrap(),
            Selector::Label("Close".into())
        );
        assert_eq!(
            Selector::parse("text:+1").unwrap(),
            Selector::Text("+1".into())
        );
        assert_eq!(Selector::parse("view:20").unwrap(), Selector::View(20));
        assert_eq!(Selector::parse("a:b").unwrap(), Selector::Any("a:b".into()));
        assert_eq!(
            Selector::parse("view:x").unwrap_err(),
            "`view:x`: not a view id: invalid digit found in string"
        );
    }

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
            screen.first(&Selector::Text("Close".into()), Some),
            Some(14)
        );
        screen.apply(&batch(vec![MountItem::Remove { id: 8 }]));
        assert_eq!(screen.texts(), ["Home", "Close"]);
    }
}

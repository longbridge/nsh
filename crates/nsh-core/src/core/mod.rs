mod classes;
mod components;
mod list;
mod measure;
mod renders;
mod services;
#[cfg(test)]
mod tests;
mod touch;
mod views;

use std::{rc::Rc, sync::Arc};

use slab::Slab;
use taffy::{AvailableSpace, NodeId as LayoutId, Overflow, Position, TaffyTree};

use self::list::{ExtraKind, Follow, ListState, PinTarget};
pub use self::measure::Measurement;
use crate::{
    animation::AnimationProp,
    geometry::{Bounds, Percentage, Point, Points, Size, rect, size_pt},
    mount::{
        InputConfig, ModalConfig, MountBatch, MountItem, NativeProps, ScreenConfig, ScrollBehavior,
        StickyCell, VideoProps, ViewKind,
    },
    ops::{
        CellSlot, Command, EventKind, EventMask, EventPayload, HeaderSlot, ImageProp, ImageSource,
        KeyboardDismissMode, ModalAnimation, ModalProp, NativeProp, NodeId, Op, PayloadValue, Prop,
        ROOT_ID, ResizeMode, ScreenAnimation, ScreenProp, ScrollProp, Tag,
    },
    props::{AccessibilityProp, EdgeInsets, GestureValues, LayoutAnimation, ViewProps},
    style::{EllipsizeMode, Style, StylePatch, TextAlign, TextStyle, ViewStyle},
    tailwind::{ClassEnvironment, ClassList, ClassProps, Tailwind},
    text::{AttributedText, MeasureCache, ResolvedTextStyle, TextEngine, TextLayout},
};

/// Slab index of a node. Internal links use keys, never ids, to avoid the
/// id -> key indirection on hot paths.
type Key = u32;

const NO_KEY: Key = Key::MAX;

/// Rounds of binding the components' own lists per commit, at most.
const MAX_VIRTUAL_ROUNDS: u32 = 8;
/// Dirty flags, processed once per commit.
mod dirty {
    /// Emit [`crate::MountItem::ViewStyle`].
    pub(super) const VIEW: u32 = 1 << 0;
    /// Emit [`crate::MountItem::Events`].
    pub(super) const EVENTS: u32 = 1 << 1;
    /// Rebuild the attributed text of a text block.
    pub(super) const TEXT_BUILD: u32 = 1 << 2;
    /// Lay out the text block again and emit [`crate::MountItem::Text`].
    pub(super) const TEXT_MOUNT: u32 = 1 << 3;
    /// Emit [`crate::MountItem::Image`].
    pub(super) const IMAGE: u32 = 1 << 4;
    /// Emit [`crate::MountItem::ScrollConfig`].
    pub(super) const SCROLL: u32 = 1 << 5;
    /// Emit [`crate::MountItem::Refreshing`].
    pub(super) const REFRESHING: u32 = 1 << 6;
    /// Emit [`crate::MountItem::AnimationBindings`].
    pub(super) const ANIMATION: u32 = 1 << 7;
    /// Emit [`crate::MountItem::PanValues`].
    pub(super) const PAN: u32 = 1 << 8;
    /// Emit [`crate::MountItem::Input`].
    pub(super) const INPUT: u32 = 1 << 9;
    /// Emit [`crate::MountItem::InputValue`].
    pub(super) const INPUT_VALUE: u32 = 1 << 10;
    /// Emit [`crate::MountItem::ViewProps`].
    pub(super) const VIEW_PROPS: u32 = 1 << 11;
    /// Emit [`crate::MountItem::ScreenConfig`].
    pub(super) const SCREEN: u32 = 1 << 12;
    /// Emit [`crate::MountItem::ModalConfig`].
    pub(super) const MODAL: u32 = 1 << 13;
    /// Emit [`crate::MountItem::NativeProps`].
    pub(super) const NATIVE: u32 = 1 << 14;
    /// Emit [`crate::MountItem::ScrollBehavior`].
    pub(super) const SCROLL_BEHAVIOR: u32 = 1 << 15;
    /// Emit [`crate::MountItem::Canvas`].
    pub(super) const CANVAS: u32 = 1 << 16;
    /// Emit [`crate::MountItem::ComponentProps`].
    pub(super) const COMPONENT: u32 = 1 << 17;
    /// Emit [`crate::MountItem::AccessibilityLabel`] (unless
    /// [`VIEW_PROPS`] sends everything).
    pub(super) const LABEL: u32 = 1 << 18;
}

/// A layout root other than the surface's: screens, modals and header
/// items are laid out on their own, at a size their container gives them.
#[derive(Debug, Default)]
struct RootState {
    /// Size the platform reported for the root (modals).
    reported: Option<Size<Points>>,
    /// Safe-area insets the platform reported for the root (modals).
    insets: EdgeInsets,
    /// Height of the header the platform measured above the content
    /// (screens; until then, estimated from the bar's height and the safe
    /// area).
    header: Option<f32>,
    /// The size and header height the root was last laid out with.
    laid_out: Option<(Size<Points>, f32)>,
    /// Screen options (screens).
    screen: ScreenConfig,
    /// Modal options (modals).
    modal: ModalConfig,
    /// Where a header item goes (header items).
    slot: HeaderSlot,
    /// `nativeID` (input accessories).
    native_id: Option<Box<str>>,
    /// `statusBar` (screens and modals).
    status_bar: Option<crate::StatusBarSetting>,
    /// Its entry in the surface's status bars, while the node is in the
    /// tree ([`Core::sync_status_bars`]).
    status_bar_entry: Option<crate::ui::status_bar::StatusBarEntry>,
}

/// Paragraph settings of a text block (or input).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Paragraph {
    /// `numberOfLines`; 0 means no limit.
    lines: u32,
    /// `ellipsizeMode`.
    ellipsize: EllipsizeMode,
    /// `allowFontScaling`.
    allow_font_scaling: bool,
    /// `maxFontSizeMultiplier`; 0 means no limit.
    max_font_multiplier: f32,
    /// `ellipsizeMode` was set (it then wins over classes).
    ellipsize_set: bool,
    /// `selectable` as set (it wins over `select-*` classes).
    selectable: Option<bool>,
    /// `selectionColor` of selectable text.
    selection_color: Option<crate::Color>,
    /// `adjustsFontSizeToFit`.
    fit: bool,
    /// `minimumFontScale`.
    min_font_scale: f32,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            lines: 0,
            ellipsize: EllipsizeMode::Tail,
            allow_font_scaling: true,
            max_font_multiplier: 0.0,
            ellipsize_set: false,
            selectable: None,
            selection_color: None,
            fit: false,
            min_font_scale: 0.0,
        }
    }
}

impl Paragraph {
    /// The factor applied to font sizes for the user's text size setting.
    fn font_multiplier(&self, font_scale: f32) -> f32 {
        if !self.allow_font_scaling {
            return 1.0;
        }
        if self.max_font_multiplier >= 1.0 {
            font_scale.min(self.max_font_multiplier)
        } else {
            font_scale
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    View,
    Text,
    RawText,
    Image,
    Scroll,
    List,
    Cell,
    Input,
    Stack,
    Screen,
    Modal,
    HeaderItem,
    InputAccessory,
    Canvas,
    WebView,
    Video,
    Native,
}

impl Kind {
    /// Kinds whose native view hosts child views.
    fn is_container(self) -> bool {
        matches!(
            self,
            Kind::View
                | Kind::Scroll
                | Kind::List
                | Kind::Cell
                | Kind::Stack
                | Kind::Screen
                | Kind::Modal
                | Kind::HeaderItem
                | Kind::InputAccessory
        )
    }

    /// Kinds laid out as their own root, not as part of their parent.
    fn is_root(self) -> bool {
        matches!(
            self,
            Kind::Screen | Kind::Modal | Kind::HeaderItem | Kind::InputAccessory
        )
    }

    fn view_kind(self) -> ViewKind {
        match self {
            Kind::Text => ViewKind::Text,
            Kind::Image => ViewKind::Image,
            Kind::Scroll | Kind::List => ViewKind::Scroll,
            Kind::Input => ViewKind::Input,
            Kind::Stack => ViewKind::Stack,
            Kind::Screen => ViewKind::Screen,
            Kind::Modal => ViewKind::Modal,
            Kind::HeaderItem => ViewKind::HeaderItem,
            Kind::InputAccessory => ViewKind::InputAccessory,
            Kind::Canvas => ViewKind::Canvas,
            Kind::WebView => ViewKind::WebView,
            Kind::Video => ViewKind::Video,
            Kind::Native => ViewKind::Native,
            Kind::View | Kind::Cell | Kind::RawText => ViewKind::View,
        }
    }

    /// Kinds whose taffy style is derived from the JS style (and must keep
    /// the JS style around to re-derive it).
    fn of(tag: Tag) -> Self {
        match tag {
            Tag::View => Kind::View,
            Tag::Text => Kind::Text,
            Tag::Image => Kind::Image,
            Tag::Scroll => Kind::Scroll,
            Tag::List => Kind::List,
            Tag::Cell => Kind::Cell,
            Tag::Input => Kind::Input,
            Tag::Stack => Kind::Stack,
            Tag::Screen => Kind::Screen,
            Tag::Modal => Kind::Modal,
            Tag::HeaderItem => Kind::HeaderItem,
            Tag::InputAccessory => Kind::InputAccessory,
            Tag::Canvas => Kind::Canvas,
            Tag::WebView => Kind::WebView,
            Tag::Video => Kind::Video,
            Tag::Native => Kind::Native,
        }
    }

    fn derives_layout(self) -> bool {
        matches!(
            self,
            Kind::Text | Kind::Scroll | Kind::List | Kind::Cell | Kind::Screen | Kind::Modal
        )
    }
}

/// A registered native component: its name and properties, as JS set them.
#[derive(Debug, Default)]
struct ComponentState {
    name: Option<Arc<str>>,
    props: Vec<(Arc<str>, crate::JsonValue)>,
    /// Properties changed since the last commit.
    changed: Vec<Arc<str>>,
}

#[derive(Debug)]
struct ScrollState {
    horizontal: bool,
    shows_indicator: bool,
    refreshing: bool,
    scroll_value: Option<crate::animation::NodeId>,
    keyboard_dismiss: KeyboardDismissMode,
    /// Last content size sent to the platform.
    content: Option<Size<Points>>,
    /// Where in the mount items the last `ScrollContent` was queued: a
    /// later size in the same batch replaces it, so a row relaid in two
    /// passes never sends the platform a transient size (which would clamp
    /// the offset before the content grows back). With the batch it is in
    /// ([`Core::batches`]).
    content_item: Option<(u64, usize)>,
    behavior: ScrollBehavior,
    /// Minimum milliseconds between `onScroll` events.
    event_throttle_ms: f32,
    /// The zoom scale the platform reported.
    zoom: f32,
    /// The content offset the platform reported.
    offset: (f32, f32),
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            horizontal: false,
            // React Native shows scroll indicators by default.
            shows_indicator: true,
            refreshing: false,
            scroll_value: None,
            keyboard_dismiss: KeyboardDismissMode::None,
            content: None,
            content_item: None,
            behavior: ScrollBehavior::default(),
            event_throttle_ms: 0.0,
            zoom: 1.0,
            offset: (0.0, 0.0),
        }
    }
}

#[derive(Debug, Default)]
struct InputState {
    value: String,
    /// JS set `value` (or `defaultValue`) at least once.
    value_set: bool,
    /// Edits reported by the platform.
    count: u32,
    /// Everything but the text style, which comes from the node's style.
    config: InputConfig,
    /// Last content box sent to the platform.
    content: Option<Bounds<Points>>,
    /// Last size `onContentSizeChange` reported.
    content_size: Option<Size<Points>>,
    /// The input is measured with its placeholder (its value is empty).
    placeholder_measured: bool,
    /// The size of an empty line in the style of the measured placeholder,
    /// by the placeholder text's `layout_hash`: what `onContentSizeChange`
    /// reports for an empty input.
    empty_line: Option<(u64, Size<Points>)>,
}

#[derive(Debug, Default)]
struct ImageState {
    source: Option<ImageSource>,
    resize_mode: ResizeMode,
    options: crate::mount::ImageOptions,
}

struct Node {
    id: NodeId,
    kind: Kind,
    /// Inside a text block: a nested `<text>` span or a raw text run.
    inline: bool,
    parent: Key,
    children: Vec<Key>,
    /// Layout node. Present exactly for nodes that render a native view.
    /// Raw text and spans have none.
    layout: Option<LayoutId>,
    /// The style JS set, for kinds whose taffy style is derived from it.
    base_layout: Option<Box<taffy::Style>>,
    view_style: ViewStyle,
    text_style: TextStyle,
    animated: Vec<(AnimationProp, crate::animation::NodeId)>,
    /// What a canvas draws.
    drawing: Option<Arc<crate::canvas::Drawing>>,
    /// The `style` prop as written, applied over the class styles.
    inline_style: StylePatch,
    /// `class` / `classList` as set.
    class_names: Option<Box<classes::ClassNames>>,
    /// Their parsed form.
    classes: Option<Arc<ClassList>>,
    /// Node props the classes set.
    class_props: Option<Box<ClassProps>>,
    /// Interaction states (`tailwind::state` bits).
    states: u8,
    /// States the platform reports for the view.
    tracked: u8,
    /// What only some nodes have: state styles, the styles of a page's
    /// header or a list's content ([`classes::NodeParts`]).
    parts: Option<Box<classes::NodeParts>>,
    /// A child's classes read their position among siblings.
    positional_children: bool,
    /// The inset padding and margin components set: added to the style's.
    insets: Option<Box<[EdgeInsets; 2]>>,
    events: EventMask,
    /// `zIndex`: native views are ordered by it (stable) among siblings.
    z_index: i32,
    /// Laid out right to left (the resolved `direction`).
    rtl: bool,
    /// `direction`: `None` inherits.
    direction: Option<bool>,
    /// Start / end properties, if any.
    logical: Option<Box<crate::style::Logical>>,
    /// Touch, accessibility and test properties; most views have none.
    props: Option<Box<ViewProps>>,
    /// Raw text content.
    text: String,
    /// Paragraph settings of a text block or input.
    paragraph: Paragraph,
    image: Option<Box<ImageState>>,
    scroll: Option<Box<ScrollState>>,
    list: Option<Box<ListState>>,
    input: Option<Box<InputState>>,
    /// Screens, modals and header items.
    root: Option<Box<RootState>>,
    /// Switches, sliders, activity indicators, web views and videos.
    control: Option<Box<NativeProps>>,
    /// A registered native component (`<native>`).
    component: Option<Box<ComponentState>>,
    /// Recycling slot (or role) of a list cell.
    slot: Option<CellSlot>,
    /// Animated values driven by gestures on the view.
    gestures: GestureValues,
    /// The native view exists.
    created: bool,
    /// Last frame sent to the platform.
    frame: Bounds<Points>,
    /// The area descendants cover outside the bounds, as last mounted
    /// (`MountItem::Overflow`).
    overflow: Option<Bounds<Points>>,
    dirty: u32,
    queued: bool,
    /// In `Core::pending_text`.
    text_queued: bool,
    /// A list cell whose subtree was laid out again this commit.
    relaid: bool,
    /// `android={{ elevation }}`.
    elevation: Points,
    /// A text block: its attachments (inline views and images, in text
    /// order) whose native views are inside its own.
    attached: Vec<NodeId>,
}

impl Node {
    /// The listeners as platforms take them: a `disabled` view takes no
    /// presses (React Native's `Pressable` / touchables), so it shows no
    /// press feedback either, and opens no context menu: a secondary click
    /// on it goes on to the nearest enabled listener around it.
    fn mounted_events(&self) -> EventMask {
        let mut events = self.events;
        if self.states & crate::tailwind::state::DISABLED != 0 {
            for e in DISABLED_OFF {
                events.set(e, false);
            }
        }
        events
    }

    /// The view style as platforms take it: translations by a fraction of
    /// the size resolved with the current frame.
    fn mounted_view_style(&self) -> Box<ViewStyle> {
        let mut style = Box::new(self.view_style.clone());
        style.elevation = self.elevation;
        if style.transform.has_fractions() {
            let size = if self.frame.size.width.is_nan() {
                Size::default()
            } else {
                self.frame.size
            };
            style.transform = style.transform.resolve_fractions(size);
        }
        style
    }

    fn new(id: NodeId, kind: Kind) -> Self {
        Self {
            id,
            kind,
            inline: false,
            parent: NO_KEY,
            children: Vec::new(),
            layout: None,
            base_layout: None,
            view_style: ViewStyle::default(),
            text_style: TextStyle::default(),
            animated: Vec::new(),
            drawing: None,
            inline_style: StylePatch::default(),
            class_names: None,
            classes: None,
            class_props: None,
            states: 0,
            tracked: 0,
            parts: None,
            positional_children: false,
            insets: None,
            events: EventMask::default(),
            z_index: 0,
            rtl: false,
            direction: None,
            logical: None,
            props: None,
            text: String::new(),
            paragraph: Paragraph::default(),
            image: matches!(kind, Kind::Image).then(Box::default),
            scroll: matches!(kind, Kind::Scroll | Kind::List).then(Box::default),
            list: matches!(kind, Kind::List).then(|| Box::new(ListState::new())),
            input: matches!(kind, Kind::Input).then(Box::default),
            root: kind.is_root().then(Box::default),
            control: match kind {
                Kind::WebView => Some(NativeProps::WebView(Box::default())),
                Kind::Video => Some(NativeProps::Video(Box::<VideoProps>::default())),
                _ => None,
            }
            .map(Box::new),
            component: matches!(kind, Kind::Native).then(Box::default),
            slot: None,
            gestures: GestureValues::default(),
            created: false,
            frame: Bounds::UNSET,
            overflow: None,
            attached: Vec::new(),
            elevation: Points::ZERO,
            dirty: 0,
            queued: false,
            text_queued: false,
            relaid: false,
        }
    }
}

/// Taffy context of a text block.
struct TextContext<P> {
    text: Arc<AttributedText>,
    /// `text.layout_hash()`, computed once per content.
    layout_hash: u64,
    /// Platform typesetting state, created on first use.
    prepared: Option<P>,
    /// Last measurement: `(max_width, size)`.
    cache: Option<(Option<f32>, Size<Points>)>,
    /// Last final layout sent to the platform and the content box it was
    /// made for (where in the view the text goes).
    laid_out: Option<(Bounds<Points>, TextLayout)>,
}

impl<P> TextContext<P> {
    fn new(text: Arc<AttributedText>) -> Self {
        Self {
            layout_hash: text.layout_hash(),
            text,
            prepared: None,
            cache: None,
            laid_out: None,
        }
    }
}

/// Measured leaves: text blocks and images with a known natural size.
enum LeafContext<P> {
    Text(TextContext<P>),
    Image(Option<Size<Points>>),
    /// A registered native component's own size, once it reported one.
    Native(Option<Size<Points>>),
}

/// A request for the JS runtime to render list items into cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListBind {
    /// The list.
    pub list: NodeId,
    /// `(slot, item index)` pairs.
    pub binds: Vec<(u32, u32)>,
}

/// An op referenced a node in an impossible way. This is a renderer bug;
/// the op is skipped and the tree stays consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyError {
    /// The id is unknown (never created or already released).
    UnknownNode(NodeId),
    /// The id is already in use.
    DuplicateNode(NodeId),
    /// The child is not a child of the given parent.
    NotAChild(NodeId),
    /// The `before` sibling is not a child of the given parent.
    BadAnchor(NodeId),
    /// The insert would create a cycle.
    Cycle(NodeId),
    /// Creating the node would exceed the node quota ([`Core::set_max_nodes`]).
    TooManyNodes(usize),
    /// A new id far beyond the ids in use (ids index a table, so they must
    /// stay dense).
    IdOutOfRange(NodeId),
}

impl ApplyError {
    /// The app went over a quota or sent what no runtime sends: it should
    /// be stopped rather than continue.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            ApplyError::TooManyNodes(_) | ApplyError::IdOutOfRange(_)
        )
    }
}

/// The shadow tree of one surface.
///
/// Feed it [`Op`]s with [`Core::apply`], then call [`Core::commit`] once per
/// JS turn to get the [`MountBatch`] for the platform. Lists may then ask
/// the runtime to render more items ([`Core::take_list_binds`]); the engine
/// runs those and commits again within the same frame.
pub struct Core<M: TextEngine> {
    nodes: Slab<Node>,
    /// Node id -> key + 1 (0 = none). Ids are dense, so this is a flat vec.
    slots: Vec<u32>,
    layout: TaffyTree<LeafContext<M::Prepared>>,
    text_engine: Rc<M>,
    measure_cache: MeasureCache,
    /// Text blocks whose final layout must be (re)made after layout.
    pending_text: Vec<Key>,
    /// Text blocks with attachments (checked for attachments laid out
    /// again).
    attachment_blocks: Vec<Key>,
    /// Live list nodes.
    lists: Vec<Key>,
    /// Live screens, modals and header items.
    roots: Vec<Key>,
    /// Attached input accessories by `nativeID`.
    accessories: rustc_hash::FxHashMap<Box<str>, NodeId>,
    /// Height of the native header bar of screens: normal and large title.
    header_heights: (Points, Points),
    /// Natural sizes of the platform's controls.
    /// The surface's safe-area insets (stacks at the top extend their
    /// headers under the status bar).
    safe_area: EdgeInsets,
    list_binds: Vec<ListBind>,
    /// The events for JS, in order (the platform's and the core's events,
    /// Rust components' events, calls' results).
    events: Vec<crate::ui::JsEvent>,
    /// Reused buffer of `(item index, cell)` for list updates.
    cells_scratch: Vec<(u32, Key)>,
    root: Key,
    viewport: Size<Points>,
    mount: Vec<MountItem>,
    /// Batches handed out so far: tells this batch's mount indexes from
    /// earlier ones'.
    batches: u64,
    /// Imperative calls, run after everything else in the batch.
    commands: Vec<(NodeId, Command)>,
    /// Animates the layout changes of the next commit.
    layout_animation: Option<LayoutAnimation>,
    /// The screens and modals with a `statusBar`, in the order they got
    /// it: few, so attaching and detaching only flag them for the commit.
    status_bar_nodes: Vec<Key>,
    /// The tree changed with `status_bar_nodes` in it: the commit checks
    /// which are in the tree.
    status_bars_moved: bool,
    /// The user's preferred text size (1.0 is the default).
    font_scale: f32,
    queue: Vec<Key>,
    needs_layout: bool,
    /// Parsed classes and the theme.
    tailwind: Tailwind,
    /// What class variants read from the environment (`dark` as shown:
    /// [`Core::set_appearance`]).
    class_env: ClassEnvironment,
    /// The appearance the app asked for.
    appearance: crate::platform::Appearance,
    /// The platform reports dark appearance.
    system_dark: bool,
    /// [`Core::set_max_nodes`].
    max_nodes: usize,
    /// What the app may reach ([`Core::set_access`]).
    access: Arc<crate::access::Access>,
    /// Denials already logged (each once).
    denied: rustc_hash::FxHashSet<String>,
    /// Rust components and what they rendered.
    components: components::Components,
}

impl<M: TextEngine + 'static> Core<M> {
    /// Creates a core whose root view (id [`ROOT_ID`]) fills `viewport`.
    pub fn new(text_engine: M, viewport: Size<Points>) -> Self {
        let core = Self::with_engine(text_engine, viewport);
        // Components that run once measure text with it.
        let engine = core.text_engine.clone();
        core.components
            .runtime
            .set_measure(Rc::new(move |font, text| {
                crate::canvas::measure_line(&*engine, font, text).0
            }));
        core
    }
}

impl<M: TextEngine> Core<M> {
    fn with_engine(text_engine: M, viewport: Size<Points>) -> Self {
        let mut layout = TaffyTree::with_capacity(256);
        let root_layout = layout.new_leaf(root_style(viewport)).expect("taffy root");
        let mut nodes = Slab::with_capacity(256);
        let mut root = Node::new(ROOT_ID, Kind::View);
        root.layout = Some(root_layout);
        // The platform owns the root view.
        root.created = true;
        let root = nodes.insert(root) as Key;
        let mut slots = vec![0; 256];
        slots[ROOT_ID as usize] = root + 1;
        Self {
            nodes,
            slots,
            layout,
            text_engine: Rc::new(text_engine),
            measure_cache: MeasureCache::default(),
            pending_text: Vec::new(),
            attachment_blocks: Vec::new(),
            max_nodes: usize::MAX,
            access: Arc::default(),
            denied: Default::default(),
            components: components::Components::for_viewport(viewport),
            lists: Vec::new(),
            roots: Vec::new(),
            accessories: Default::default(),
            header_heights: (Points(44.0), Points(96.0)),
            safe_area: EdgeInsets::default(),
            list_binds: Vec::new(),
            events: Vec::new(),
            cells_scratch: Vec::new(),
            root,
            viewport,
            mount: Vec::new(),
            batches: 0,
            commands: Vec::new(),
            layout_animation: None,
            status_bar_nodes: Vec::new(),
            status_bars_moved: false,
            font_scale: 1.0,
            queue: Vec::new(),
            needs_layout: true,
            tailwind: Tailwind::new(),
            class_env: ClassEnvironment::default(),
            appearance: crate::platform::Appearance::System,
            system_dark: false,
        }
    }

    /// Sets the height of the platform's screen header bar, normal and with
    /// a large title (status bar excluded).
    pub fn set_header_heights(&mut self, normal: Points, large: Points) {
        if self.header_heights != (normal, large) {
            self.header_heights = (normal, large);
            self.needs_layout = true;
        }
    }

    /// Sets the surface's safe-area insets.
    pub fn set_safe_area(&mut self, insets: EdgeInsets) {
        if self.safe_area != insets {
            self.safe_area = insets;
            self.needs_layout = true;
        }
    }

    /// The platform presents root `id` (a modal) at `size`, with `insets`
    /// of it covered by system UI.
    pub fn set_root_size(&mut self, id: NodeId, size: Size<Points>, insets: EdgeInsets) {
        let Ok(key) = self.key(id) else { return };
        if let Some(root) = self.nodes[key as usize].root.as_mut()
            && (root.reported != Some(size) || root.insets != insets)
        {
            root.reported = Some(size);
            root.insets = insets;
            self.needs_layout = true;
        }
    }

    /// The platform measured where the content of screen `id` starts below
    /// the header it shows (`None`: estimate it from the bar's height and
    /// the safe area). Used for screens that lay their content out below
    /// the header (not large titles or transparent headers).
    pub fn set_screen_header(&mut self, id: NodeId, height: Option<Points>) {
        let Ok(key) = self.key(id) else { return };
        let height = height.map(|h| h.0).filter(|h| h.is_finite() && *h >= 0.0);
        if let Some(root) = self.nodes[key as usize].root.as_mut()
            && root.header != height
        {
            root.header = height;
            self.needs_layout = true;
        }
    }

    /// Sets the user's preferred text size (1.0 is the default): text that
    /// allows font scaling is laid out again.
    pub fn set_font_scale(&mut self, scale: f32) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        if scale == self.font_scale {
            return;
        }
        self.font_scale = scale;
        let blocks: Vec<(Key, Kind)> = self
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, Kind::Text | Kind::Input) && !n.inline)
            .map(|(k, n)| (k as Key, n.kind))
            .collect();
        for (key, kind) in blocks {
            if kind == Kind::Text {
                self.mark(key, dirty::TEXT_BUILD);
            } else {
                self.measure_input(key);
                self.mark(key, dirty::INPUT);
            }
        }
    }

    /// The pixel ratio went from `old` to `new`: the images whose variant
    /// changes are sent again (their layout does not change).
    fn repick_images(&mut self, old: f32, new: f32) {
        let changes = |s: &Option<ImageSource>| {
            s.as_ref()
                .is_some_and(|s| !s.scales.is_empty() && s.variant(old) != s.variant(new))
        };
        let images: Vec<Key> = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                n.image
                    .as_ref()
                    .is_some_and(|i| changes(&i.source) || changes(&i.options.placeholder))
            })
            .map(|(k, _)| k as Key)
            .collect();
        for key in images {
            self.mark(key, dirty::IMAGE);
        }
    }

    /// Number of live nodes, including the root.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Sets what the app may reach: remote images, canvas images, video
    /// sources, web views and the status bar are checked against it.
    pub fn set_access(&mut self, access: Arc<crate::access::Access>) {
        self.access = access;
    }

    /// Keeps the app's storage (`nsh/storage`) in `backend`; `None`: in
    /// memory, for the core's life.
    pub fn set_storage(&mut self, backend: Option<Arc<dyn crate::storage::StorageBackend>>) {
        self.components
            .runtime
            .set_storage(crate::storage::Store::new(backend));
    }

    /// Logs (once) that the app was refused `what`.
    fn deny(&mut self, what: String) {
        if self.denied.len() < 256 && self.denied.insert(what.clone()) {
            log::warn!(target: "js", "{what}");
        }
    }

    /// Caps the number of nodes (each may hold a native view, whose memory
    /// no JS heap limit covers): creating more is a fatal [`ApplyError`].
    pub fn set_max_nodes(&mut self, max: usize) {
        self.max_nodes = max.max(1);
    }

    /// Changes the size of the root view.
    pub fn set_viewport(&mut self, viewport: Size<Points>) {
        if viewport != self.viewport {
            self.viewport = viewport;
            let root = self.nodes[self.root as usize].layout.expect("root layout");
            self.layout
                .set_style(root, root_style(viewport))
                .expect("taffy root");
            self.needs_layout = true;
            self.restyle_where(crate::tailwind::deps::WIDTH | crate::tailwind::deps::HEIGHT);
            self.components_viewport(viewport);
        }
    }

    /// The platform scrolled a scroll view or list to `(x, y)`, having
    /// applied `applied` of the scrolls the core issued to it
    /// (`MountItem::ScrollBy` and `Command::ScrollTo` of lists).
    pub fn set_scroll_offset(&mut self, id: NodeId, offset: Point<Points>, applied: u32) {
        let (x, y) = (offset.x.0, offset.y.0);
        let Ok(key) = self.key(id) else { return };
        let node = &mut self.nodes[key as usize];
        let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
        if let Some(scroll) = node.scroll.as_mut() {
            scroll.offset = (x, y);
        }
        if let Some(list) = node.list.as_mut() {
            list.reported(if horizontal { x } else { y }, applied);
        }
    }

    /// The platform zoomed scroll view `id` to `scale`.
    pub fn set_zoom(&mut self, id: NodeId, scale: f32) {
        let Ok(key) = self.key(id) else { return };
        if let Some(scroll) = self.nodes[key as usize].scroll.as_mut() {
            scroll.zoom = scale;
        }
    }

    /// A registered native component reported its own size: it is laid out
    /// at it where its style leaves a dimension open.
    pub fn set_component_size(&mut self, id: NodeId, size: Size<Points>) {
        let Ok(key) = self.key(id) else { return };
        let Some(layout) = self.nodes[key as usize].layout else {
            return;
        };
        if let Some(LeafContext::Native(own)) = self.layout.get_node_context_mut(layout)
            && *own != Some(size)
        {
            *own = Some(size);
            self.layout.mark_dirty(layout).expect("taffy dirty");
            self.needs_layout = true;
        }
    }

    /// What `onScroll` reports for scroll view `id`, if it listens to it:
    /// the minimum milliseconds between events, the content size (zoomed),
    /// the viewport size and the zoom scale.
    pub fn scroll_listener(&self, id: NodeId) -> Option<(f32, Size<Points>, Size<Points>, f32)> {
        let node = &self.nodes[self.key(id).ok()? as usize];
        if !node.events.contains(EventKind::Scroll) {
            return None;
        }
        let scroll = node.scroll.as_ref()?;
        let frame = node.frame.size;
        let content = scroll.content.unwrap_or(frame);
        let z = scroll.zoom;
        Some((scroll.event_throttle_ms, content * z, frame, z))
    }

    /// Moves list render requests produced by the last commit into `out`.
    pub fn take_list_binds(&mut self, out: &mut Vec<ListBind>) {
        out.append(&mut self.list_binds);
    }

    /// Moves what the core hands JS (events it raised, such as
    /// `onEndReached` and `onLayout`, Rust components' events, calls'
    /// results) into `out`, in order.
    pub fn take_events(&mut self, out: &mut Vec<crate::ui::JsEvent>) {
        out.append(&mut self.events);
    }

    /// Registers a custom font (TrueType / OpenType data) with the text
    /// engine. Text measured before is measured again.
    pub fn register_font(&mut self, data: &[u8]) -> Result<(), String> {
        self.text_engine.register_font(data)?;
        self.fonts_changed();
        Ok(())
    }

    /// The text engine's fonts changed (a font the platform loads
    /// asynchronously became usable): text measured before is measured
    /// again.
    pub fn fonts_changed(&mut self) {
        self.measure_cache = MeasureCache::default();
        let blocks: Vec<Key> = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                matches!(n.kind, Kind::Text | Kind::Input) && !n.inline && n.layout.is_some()
            })
            .map(|(k, _)| k as Key)
            .collect();
        for key in blocks {
            let layout = self.nodes[key as usize].layout.expect("text layout");
            let cx = self.text_cx(layout);
            cx.prepared = None;
            cx.cache = None;
            cx.laid_out = None;
            self.layout.mark_dirty(layout).expect("taffy dirty");
            self.needs_layout = true;
            if self.nodes[key as usize].kind == Kind::Text {
                self.queue_text(key);
            } else {
                // The platform resolves the input's font again.
                self.mark(key, dirty::INPUT);
            }
        }
    }

    /// The user edited input `id` (before JS sees the edit): keeps the text
    /// for layout and for stale-value detection.
    pub fn input_changed(&mut self, id: NodeId, text: &str, count: u32) {
        let Ok(key) = self.key(id) else { return };
        let Some(input) = self.nodes[key as usize].input.as_mut() else {
            return;
        };
        input.count = count;
        if input.value != text {
            input.value.clear();
            input.value.push_str(text);
            self.measure_input(key);
        }
    }

    /// Applies one op from JS.
    pub fn apply(&mut self, op: Op) -> Result<(), ApplyError> {
        let _entered = self.components.runtime.enter();
        // Ops on a component's host go to its component, calls of a UI
        // set's modules to the module; the components learn whether JS
        // listens to the back button.
        if self.components.instances.is_empty()
            && !matches!(
                op,
                Op::Command {
                    command: Command::Native { .. },
                    ..
                } | Op::Listen {
                    id: ROOT_ID,
                    event: EventKind::BackPress,
                    ..
                }
            )
        {
            return self.apply_op(op);
        }
        match self.intercept(op)? {
            Some(op) => self.apply_op(op),
            None => Ok(()),
        }
    }

    /// Applies one op as is (also for the nodes components expand).
    fn apply_op(&mut self, op: Op) -> Result<(), ApplyError> {
        match op {
            Op::Rust(op) => {
                self.run_rust(op);
                Ok(())
            }
            Op::Create { id, tag } => self.create(id, tag),
            Op::CreateText { id, text } => {
                let key = self.alloc(id, Kind::RawText)?;
                self.nodes[key as usize].text = text;
                Ok(())
            }
            Op::SetText { id, text } => {
                let key = self.key(id)?;
                let node = &mut self.nodes[key as usize];
                if node.text != text {
                    node.text = text;
                    if node.inline {
                        self.mark_text_block(key);
                    }
                }
                Ok(())
            }
            Op::SetStyle { id, style, patch } => self.set_inline_style(id, style, patch),
            Op::SetProp { id, prop } => self.set_prop(id, prop),
            Op::Listen { id, event, enabled } => {
                let key = self.key(id)?;
                let node = &mut self.nodes[key as usize];
                let before = node.events;
                node.events.set(event, enabled);
                // A listener added after layout still learns the frame.
                if event == EventKind::Layout && enabled && !before.contains(event) {
                    let f = node.frame;
                    if !f.origin.x.0.is_nan() {
                        self.events.push(crate::ui::JsEvent::Event(
                            id,
                            event,
                            EventPayload::Layout(f),
                        ));
                    }
                }
                if node.events != before {
                    let relink = node.inline && (pressable(before) || pressable(node.events));
                    self.mark(key, dirty::EVENTS);
                    // Pressable spans are part of their block's text.
                    if relink {
                        self.mark_text_block(key);
                    }
                }
                Ok(())
            }
            Op::Insert {
                parent,
                child,
                before,
            } => self.insert(parent, child, before),
            Op::Remove { parent, child } => {
                let parent = self.key(parent)?;
                let child = self.key(child)?;
                if self.nodes[child as usize].parent != parent {
                    return Err(ApplyError::NotAChild(self.nodes[child as usize].id));
                }
                self.detach(child);
                Ok(())
            }
            Op::Release { id } => self.release(id),
            Op::Animation(op) => {
                self.mount.push(MountItem::Animation(op));
                Ok(())
            }
            Op::Draw { id, mut drawing } => {
                let key = self.key(id)?;
                if self.nodes[key as usize].kind != Kind::Canvas {
                    return Ok(());
                }
                if !self.access.is_unrestricted() {
                    let access = self.access.clone();
                    let mut refused = Vec::new();
                    drawing.ops.retain(|op| match op {
                        crate::canvas::CanvasOp::DrawImage { image, .. }
                            if !access.allows_media(&image.uri) =>
                        {
                            refused.push(image.uri.clone());
                            false
                        }
                        _ => true,
                    });
                    for uri in refused {
                        self.deny(not_granted("a canvas image", &uri, "network"));
                    }
                }
                let node = &mut self.nodes[key as usize];
                let slots_changed = node
                    .drawing
                    .as_ref()
                    .map_or(!drawing.slots.is_empty(), |d| d.slots != drawing.slots);
                if node.drawing.as_deref() == Some(&*drawing) {
                    return Ok(());
                }
                node.drawing = Some(Arc::from(drawing));
                let flags = dirty::CANVAS | if slots_changed { dirty::ANIMATION } else { 0 };
                self.mark(key, flags);
                Ok(())
            }
            Op::Command {
                command: Command::LayoutAnimation(animation),
                ..
            } => {
                self.layout_animation = Some(animation);
                Ok(())
            }
            Op::Command {
                id: crate::ROOT_ID,
                command: Command::SetStatusBar(patch),
            } => {
                self.components.status_bars.set_base(patch);
                Ok(())
            }
            // JS's responder blocks native scroll views as the components'
            // does: the core sends both sides' blocks.
            Op::Command {
                id,
                command: Command::SetJsResponder { block },
            } => {
                self.key(id)?;
                self.block_native(touch::Side::Js, block.then_some(id));
                Ok(())
            }
            Op::Command {
                command: Command::ClearJsResponder,
                ..
            } => {
                self.block_native(touch::Side::Js, None);
                Ok(())
            }
            // A released node: no layout, but the call is answered.
            Op::Command {
                id,
                command: Command::Measure { call },
            } if self.key(id).is_err() => {
                self.answer_measure(id, call);
                Ok(())
            }
            Op::Command { id, command } => {
                self.key(id)?;
                self.commands.push((id, command));
                Ok(())
            }
        }
    }

    /// Finishes a turn: rebuilds text, runs layout, updates lists and
    /// returns the mount items, or `None` when nothing changed.
    pub fn commit(&mut self) -> Option<MountBatch> {
        self.commit_round();
        // The components' own lists bind the cells the layout asked for
        // here, and lay out again (JS's lists ask JS, through the engine).
        for _ in 0..MAX_VIRTUAL_ROUNDS {
            if !self.bind_virtual_lists() {
                break;
            }
            self.commit_round();
        }
        if self.mount.is_empty() || matches!(self.mount[..], [MountItem::LayoutAnimation(_)]) {
            self.mount.clear();
            None
        } else {
            self.batches += 1;
            Some(MountBatch {
                items: std::mem::take(&mut self.mount),
            })
        }
    }

    /// One round of a commit: views, layout, lists, commands, into the
    /// mount items.
    fn commit_round(&mut self) {
        // The views' patches and the views to make first.
        self.render_components();
        self.sync_back();
        self.sync_keys();
        // Platforms animate the frames, insertions and removals that follow
        // in the batch; ops may already have queued some.
        if let Some(animation) = self.layout_animation.take() {
            self.mount.insert(0, MountItem::LayoutAnimation(animation));
        }
        self.flush_dirty();
        let relayout = self.needs_layout;
        if relayout {
            self.needs_layout = false;
            // Only walk the main tree when it was laid out again (a change
            // inside a list cell leaves it untouched).
            if self.layout_main() {
                let root = self.root;
                let children = std::mem::take(&mut self.nodes[root as usize].children);
                for &child in &children {
                    if !self.nodes[child as usize].kind.is_root() {
                        self.emit_frames(child);
                    }
                }
                self.nodes[root as usize].children = children;
            }
            // Screens and modals, then list cells (lists may live in them).
            self.layout_roots();
            self.layout_cells();
        }
        self.update_lists(relayout);
        self.flush_text();
        // What this turn's status bar settings add up to, once.
        self.sync_status_bars();
        if let Some(config) = self.components.status_bars.take_changed() {
            self.commands
                .push((crate::ROOT_ID, Command::StatusBar(config)));
        }
        // The components' measurements, with this commit's layout as JS's.
        for (id, answer) in std::mem::take(&mut self.components.measures) {
            answer.borrow_mut().deliver(self.measure(id));
        }
        // On laid out views; dropped for views that were never shown.
        for (id, mut command) in std::mem::take(&mut self.commands) {
            // Answered here, with this commit's layout (null for views that
            // are gone or not shown).
            if let Command::Measure { call } = command {
                self.answer_measure(id, call);
                continue;
            }
            let Ok(key) = self.key(id) else { continue };
            let node = &self.nodes[key as usize];
            if !node.created {
                continue;
            }
            // Lists resolved theirs in `update_lists`; a scroll view can
            // still scroll to its end.
            match &command {
                Command::ImageInfo { id: request, uri } if !self.access.allows_media(uri) => {
                    let message = not_granted("Image.getSize / prefetch of", uri, "network");
                    self.answer_image_info(*request, Err(message.clone()));
                    self.deny(message);
                    continue;
                }
                Command::SetClipboard(_)
                | Command::ReadClipboard { .. }
                | Command::SetClipboardImage(_)
                | Command::ReadClipboardImage { .. }
                | Command::ReadClipboardFiles { .. }
                    if !self.access.allows_clipboard() =>
                {
                    if let Command::ReadClipboard { id: request, .. }
                    | Command::ReadClipboardImage { id: request, .. }
                    | Command::ReadClipboardFiles { id: request } = &command
                    {
                        self.answer_clipboard(*request, components::ClipboardAnswer::default());
                    }
                    self.deny("the clipboard is not granted: add `\"clipboard\": true` to capabilities in nsh.json".into());
                    continue;
                }
                Command::StatusBar(_) if !self.access.allows_status_bar() => {
                    self.deny("the status bar is not granted: add `\"statusBar\": true` to capabilities in nsh.json".into());
                    continue;
                }
                _ => {}
            }
            let node = &self.nodes[key as usize];
            let id = node.id;
            match command {
                Command::ScrollToEnd { animated } => {
                    let Some(content) = node.scroll.as_ref().and_then(|s| s.content) else {
                        continue;
                    };
                    let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
                    let frame = node.frame;
                    let (x, y) = if horizontal {
                        (
                            (content.width - frame.size.width).max(Points::ZERO),
                            Points::ZERO,
                        )
                    } else {
                        (
                            Points::ZERO,
                            (content.height - frame.size.height).max(Points::ZERO),
                        )
                    };
                    command = Command::ScrollTo { x, y, animated };
                }
                Command::ScrollToIndex { .. } => continue,
                _ => {}
            }
            self.mount.push(MountItem::Command { id, command });
        }
    }

    #[inline]
    fn key(&self, id: NodeId) -> Result<Key, ApplyError> {
        match self.slots.get(id as usize) {
            Some(&k) if k != 0 => Ok(k - 1),
            _ => Err(ApplyError::UnknownNode(id)),
        }
    }

    fn alloc(&mut self, id: NodeId, kind: Kind) -> Result<Key, ApplyError> {
        let idx = id as usize;
        if self.nodes.len() >= self.max_nodes {
            return Err(ApplyError::TooManyNodes(self.max_nodes));
        }
        if idx > self.slots.len().saturating_add(self.max_nodes) {
            return Err(ApplyError::IdOutOfRange(id));
        }
        if idx >= self.slots.len() {
            self.slots.resize((idx + 1).next_power_of_two(), 0);
        } else if self.slots[idx] != 0 {
            return Err(ApplyError::DuplicateNode(id));
        }
        let key = self.nodes.insert(Node::new(id, kind)) as Key;
        self.slots[idx] = key + 1;
        Ok(key)
    }

    /// Makes `alias` another name of node `id`: a component whose view's
    /// root is another component makes that one on its own host, under a
    /// name of its own (the nested component's host), with no node of its
    /// own. What the core sends names the node by its own id.
    pub(super) fn alias(&mut self, alias: NodeId, id: NodeId) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        let idx = alias as usize;
        if idx > self.slots.len().saturating_add(self.max_nodes) {
            return Err(ApplyError::IdOutOfRange(alias));
        }
        if idx >= self.slots.len() {
            self.slots.resize((idx + 1).next_power_of_two(), 0);
        } else if self.slots[idx] != 0 {
            return Err(ApplyError::DuplicateNode(alias));
        }
        self.slots[idx] = key + 1;
        Ok(())
    }

    /// Whether `id` is another name of a node ([`Self::alias`]).
    pub(super) fn is_alias(&self, id: NodeId) -> bool {
        self.key(id).is_ok_and(|k| self.nodes[k as usize].id != id)
    }

    /// Forgets alias `id` (its node stays).
    pub(super) fn unalias(&mut self, id: NodeId) {
        if self.is_alias(id) {
            self.slots[id as usize] = 0;
        }
    }

    /// The id node `id` has natively (another for an alias).
    pub(super) fn native_id(&self, id: NodeId) -> NodeId {
        self.key(id).map_or(id, |k| self.nodes[k as usize].id)
    }

    fn create(&mut self, id: NodeId, tag: Tag) -> Result<(), ApplyError> {
        let kind = Kind::of(tag);
        let key = self.alloc(id, kind)?;
        let mut style = Style::default().layout;
        crate::style::resolve_normal_alignment(&mut style);
        let layout = match leaf_context(kind) {
            Some(context) => self.layout.new_leaf_with_context(style.clone(), context),
            None => self.layout.new_leaf(style.clone()),
        }
        .expect("taffy leaf");
        self.nodes[key as usize].layout = Some(layout);
        self.became(key, style);
        Ok(())
    }

    /// What a node of its kind needs once made, its layout style `style`.
    fn became(&mut self, key: Key, style: taffy::Style) {
        let kind = self.nodes[key as usize].kind;
        let node = &mut self.nodes[key as usize];
        if kind == Kind::Text {
            node.dirty |= dirty::TEXT_BUILD;
        }
        if kind.derives_layout() {
            node.base_layout = Some(Box::new(style));
        }
        if kind == Kind::List {
            self.lists.push(key);
        }
        if kind.is_root() {
            self.roots.push(key);
        }
        if kind == Kind::Input {
            self.measure_input(key);
        }
        self.sync_layout_style(key);
    }

    /// Node `id`, a view no child is in yet, becomes a `tag` (a component's
    /// root of another kind on its host): as if it had been made one. Only
    /// while its native view is not sent yet (it was made in this batch);
    /// whether it did.
    pub(super) fn retag(&mut self, id: NodeId, tag: Tag) -> bool {
        let Ok(key) = self.key(id) else {
            return false;
        };
        let kind = Kind::of(tag);
        let node = &self.nodes[key as usize];
        if node.kind == kind {
            return true;
        }
        let id = node.id;
        let unsent = self
            .mount
            .iter()
            .any(|m| matches!(m, MountItem::Create { id: c, .. } if *c == id));
        if node.kind != Kind::View || !node.children.is_empty() || (node.created && !unsent) {
            return false;
        }
        // Out of its parent, its native view never made.
        let parent = node.parent;
        let at = (parent != NO_KEY).then(|| {
            let siblings = &self.nodes[parent as usize].children;
            let i = siblings.iter().position(|&c| c == key).expect("child");
            let next = siblings.get(i + 1).map(|&c| self.nodes[c as usize].id);
            (self.nodes[parent as usize].id, next)
        });
        if parent != NO_KEY {
            self.unlink(key, false);
        }
        self.mount.retain(|m| {
            !matches!(m, MountItem::Create { id: c, .. } | MountItem::Insert { child: c, .. } if *c == id)
        });
        // The kind's own state and layout.
        let fresh = Node::new(id, kind);
        let node = &mut self.nodes[key as usize];
        node.created = false;
        node.kind = kind;
        node.image = fresh.image;
        node.scroll = fresh.scroll;
        node.list = fresh.list;
        node.input = fresh.input;
        node.root = fresh.root;
        node.control = fresh.control;
        node.component = fresh.component;
        let layout = node.layout.expect("a view has a layout");
        let _ = self.layout.set_node_context(layout, leaf_context(kind));
        let style = self.layout.style(layout).cloned().unwrap_or_default();
        self.became(key, style);
        if let Some((parent, before)) = at {
            let _ = self.insert(parent, id, before);
        }
        true
    }

    /// Rebuilds the text an input is measured with: its value, else its
    /// placeholder, else an empty line.
    fn measure_input(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (Some(input), Some(layout)) = (&node.input, node.layout) else {
            return;
        };
        let placeholder = input.value.is_empty() && !input.config.placeholder.is_empty();
        let text = self.input_text(
            key,
            if placeholder {
                &input.config.placeholder
            } else {
                &input.value
            },
        );
        let cx = self.text_cx(layout);
        let same = cx.text.layout_eq(&text);
        let input = self.nodes[key as usize]
            .input
            .as_mut()
            .expect("input state");
        // A value that reads as the placeholder measures the same, but
        // its content size is the value's: laid out again to report it.
        if same && input.placeholder_measured == placeholder {
            return;
        }
        input.placeholder_measured = placeholder;
        if !same {
            *self.text_cx(layout) = TextContext::new(Arc::new(text));
        }
        self.layout.mark_dirty(layout).expect("taffy dirty");
        self.needs_layout = true;
    }

    /// `content` in the text style of input `key`, as it is measured.
    fn input_text(&self, key: Key, content: &str) -> AttributedText {
        let node = &self.nodes[key as usize];
        let multiline = node.input.as_ref().is_some_and(|i| i.config.multiline);
        let mut text = AttributedText {
            max_lines: if multiline { 0 } else { 1 },
            ..AttributedText::default()
        };
        let style = scaled(
            self.inherited_text_style(key).inherit(&node.text_style),
            node.paragraph.font_multiplier(self.font_scale),
        );
        text.push(content, style);
        // An empty last line still takes space (a trailing line break, or no
        // text at all).
        if content.is_empty() || (multiline && content.ends_with('\n')) {
            text.push("\u{200b}", style);
        }
        text
    }

    /// Pushes the effective taffy style of `key`: the JS style, adjusted
    /// for scroll containers and list cells.
    fn sync_layout_style(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (Some(layout), Some(base)) = (node.layout, node.base_layout.as_deref()) else {
            return;
        };
        let mut style = base.clone();
        let kind = node.kind;
        if let Some(root) = self.nodes[key as usize].root.as_mut() {
            // Laid out again with its size by `layout_roots`.
            root.laid_out = None;
        }
        let node = &self.nodes[key as usize];
        match kind {
            Kind::Scroll => {
                // Children may overflow along the scroll axis; a horizontal
                // scroll lays its content out in a row.
                let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
                if horizontal {
                    style.flex_direction = taffy::FlexDirection::Row;
                    style.overflow.x = Overflow::Scroll;
                    if node.rtl {
                        // Laid out left to right (with the authored sides)
                        // and mirrored by `emit_frames`: right to left, a
                        // row would overflow to negative offsets.
                        std::mem::swap(&mut style.padding.left, &mut style.padding.right);
                        std::mem::swap(&mut style.border.left, &mut style.border.right);
                        style.direction = taffy::Direction::Ltr;
                    }
                } else {
                    style.overflow.y = Overflow::Scroll;
                }
            }
            Kind::List => {
                style.overflow = taffy::Point {
                    x: Overflow::Hidden,
                    y: Overflow::Hidden,
                }
            }
            Kind::Cell => {
                // Cells are their own layout roots (laid out at the list's
                // cross size, positioned by the list): a change in one row
                // never relayouts the rest of the tree.
                let horizontal = self
                    .nodes
                    .get(node.parent as usize)
                    .and_then(|p| p.scroll.as_ref())
                    .is_some_and(|s| s.horizontal);
                style.position = Position::Relative;
                if horizontal {
                    style.size.height = taffy::Dimension::percent(1.0);
                } else {
                    style.size.width = taffy::Dimension::percent(1.0);
                }
            }
            _ => {}
        }
        self.layout.set_style(layout, style).expect("taffy style");
        self.needs_layout = true;
    }

    fn set_inline_style(
        &mut self,
        id: NodeId,
        style: Box<Style>,
        patch: StylePatch,
    ) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        let node = &mut self.nodes[key as usize];
        node.inline_style = patch;
        if node.classes.is_some() {
            self.restyle(key);
            return Ok(());
        }
        self.set_style(id, *style)
    }

    fn set_style(&mut self, id: NodeId, style: Style) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        let node = &mut self.nodes[key as usize];
        let Style {
            mut layout,
            mut view,
            text,
            animated,
            z_index,
            direction,
            logical,
            pending: _,
        } = style;
        if animated
            .iter()
            .any(|(p, _)| matches!(p, AnimationProp::BackgroundColor))
        {
            // The animated color must show: a gradient drawn over it would
            // hide it.
            view.clear_background_images();
        }
        if key != self.root {
            node.direction = direction;
        }
        node.logical = (!logical.is_empty()).then(|| Box::new(logical));
        let rtl = self.resolved_rtl(key);
        crate::style::resolve_normal_alignment(&mut layout);
        crate::style::apply_direction(&mut layout, &mut view, &logical, rtl);
        let node = &mut self.nodes[key as usize];
        if let Some([padding, margin]) = node.insets.as_deref() {
            // On top of points; percentages cannot take an addition.
            let add = |p: &mut taffy::LengthPercentage, d: f32| {
                if let taffy::style::ExpandedLengthPercentage::Length(v) = p.expand() {
                    *p = taffy::LengthPercentage::length(v + d);
                }
            };
            add(&mut layout.padding.top, padding.top.0);
            add(&mut layout.padding.right, padding.right.0);
            add(&mut layout.padding.bottom, padding.bottom.0);
            add(&mut layout.padding.left, padding.left.0);
            // `auto` margins stay automatic.
            let add = |m: &mut taffy::LengthPercentageAuto, d: f32| {
                if let taffy::style::ExpandedLengthPercentageAuto::Length(v) = m.expand() {
                    *m = taffy::LengthPercentageAuto::length(v + d);
                }
            };
            add(&mut layout.margin.top, margin.top.0);
            add(&mut layout.margin.right, margin.right.0);
            add(&mut layout.margin.bottom, margin.bottom.0);
            add(&mut layout.margin.left, margin.left.0);
        }
        let flipped = node.rtl != rtl;
        node.rtl = rtl;
        let view_changed = node.view_style != view;
        // A span's background is part of its block's text.
        let span_background_changed = node.inline
            && node.kind == Kind::Text
            && span_background(&node.view_style, node.id) != span_background(&view, node.id);
        let text_changed = node.text_style != text || span_background_changed;
        let animation_changed = node.animated != animated;
        let z_changed = node.z_index != z_index;
        node.view_style = view;
        node.text_style = text;
        node.animated = animated;
        node.z_index = z_index;
        if z_changed {
            self.restack(key);
        }
        let node = &mut self.nodes[key as usize];
        if node.kind.derives_layout() {
            node.base_layout = Some(Box::new(layout));
            self.sync_layout_style(key);
        } else if let Some(l) = node.layout {
            self.layout.set_style(l, layout).expect("taffy style");
            self.needs_layout = true;
        }
        let mut flags = 0;
        if view_changed {
            flags |= dirty::VIEW;
        }
        if animation_changed {
            flags |= dirty::ANIMATION;
        }
        if flags != 0 {
            self.mark(key, flags);
        }
        if text_changed || flipped {
            match self.nodes[key as usize].kind {
                Kind::Text => self.mark_text_block(key),
                Kind::Input => {
                    self.measure_input(key);
                    self.mark(key, dirty::INPUT);
                }
                // Text inside inherits it (CSS).
                _ if text_changed => self.mark_inheriting_text(key),
                _ => {}
            }
        }
        if flipped {
            self.update_children_direction(key);
        }
        Ok(())
    }

    /// The direction `key` lays out in: its own, else its parent's (the
    /// root's while detached).
    fn resolved_rtl(&self, key: Key) -> bool {
        let node = &self.nodes[key as usize];
        if let Some(rtl) = node.direction {
            return rtl;
        }
        match node.parent {
            NO_KEY => key != self.root && self.nodes[self.root as usize].rtl,
            parent => self.nodes[parent as usize].rtl,
        }
    }

    /// Re-resolves the direction of `key` (moved, or its parent's changed)
    /// and of the nodes inheriting it.
    fn update_direction(&mut self, key: Key) {
        let rtl = self.resolved_rtl(key);
        if self.nodes[key as usize].rtl == rtl {
            return;
        }
        self.flip_direction(key, rtl);
        self.update_children_direction(key);
    }

    fn update_children_direction(&mut self, key: Key) {
        let children = self.nodes[key as usize].children.clone();
        for child in children {
            self.update_direction(child);
        }
    }

    /// Lays `key` out in the other direction: its left and right properties
    /// trade places (their start / end ones with them).
    fn flip_direction(&mut self, key: Key, rtl: bool) {
        let direction = if rtl {
            taffy::Direction::Rtl
        } else {
            taffy::Direction::Ltr
        };
        let node = &mut self.nodes[key as usize];
        node.rtl = rtl;
        let mut view = node.view_style.clone();
        let kind = node.kind;
        if let Some(base) = node.base_layout.as_deref_mut() {
            crate::style::swap_sides(base, &mut view);
            base.direction = direction;
            node.view_style = view;
            self.sync_layout_style(key);
        } else if let Some(l) = node.layout {
            let mut layout = self.layout.style(l).expect("taffy style").clone();
            crate::style::swap_sides(&mut layout, &mut view);
            layout.direction = direction;
            self.nodes[key as usize].view_style = view;
            self.layout.set_style(l, layout).expect("taffy style");
            self.needs_layout = true;
        }
        self.mark(key, dirty::VIEW);
        if self.reads_direction(key) {
            self.restyle(key);
        }
        let node = &mut self.nodes[key as usize];
        let id = node.id;
        if let Some(scroll) = node.scroll.as_mut()
            && scroll.horizontal
        {
            // Back to the start side: the right end right to left (once the
            // content is known again).
            scroll.content = None;
            if !rtl {
                self.commands.push((
                    id,
                    Command::ScrollTo {
                        x: Points::ZERO,
                        y: Points::ZERO,
                        animated: false,
                    },
                ));
            }
            self.mark(key, dirty::SCROLL);
        }
        match kind {
            Kind::Text => self.mark_text_block(key),
            Kind::Input => {
                self.measure_input(key);
                self.mark(key, dirty::INPUT);
            }
            _ => {}
        }
    }

    /// `prop` with what the app may not reach removed: a refused image
    /// fails (`onError`), a refused video or web view source shows nothing.
    fn check_prop(&mut self, key: Key, prop: Prop) -> Prop {
        if self.access.is_unrestricted() {
            return prop;
        }
        let node = &self.nodes[key as usize];
        let (id, kind) = (node.id, node.kind);
        // Always reported: the `onError` listener may be set after the
        // source (JS finds the handler when the event is delivered).
        let refuse = |core: &mut Self, what: &str, uri: &str, capability: &str| {
            let message = not_granted(what, uri, capability);
            core.events.push(crate::ui::JsEvent::Event(
                id,
                EventKind::Error,
                EventPayload::Error {
                    message: message.clone(),
                },
            ));
            core.deny(message);
        };
        match prop {
            Prop::Source(Some(source)) if !self.access.allows_media(&source.uri) => {
                refuse(self, "the image", &source.uri, "network");
                Prop::Source(None)
            }
            Prop::Image(ImageProp::DefaultSource(Some(source)))
                if !self.access.allows_media(&source.uri) =>
            {
                refuse(self, "the placeholder", &source.uri, "network");
                Prop::Image(ImageProp::DefaultSource(None))
            }
            Prop::Native(NativeProp::Source(crate::WebSource::Uri(uri)))
                if kind == Kind::Video && !self.access.allows_media(&uri) =>
            {
                refuse(self, "the video", &uri, "network");
                Prop::Native(NativeProp::Source(crate::WebSource::None))
            }
            Prop::Native(NativeProp::Source(crate::WebSource::Uri(uri)))
                if kind == Kind::WebView && !self.access.allows_webview(&uri) =>
            {
                refuse(self, "the web page", &uri, "webview.hosts");
                Prop::Native(NativeProp::Source(crate::WebSource::None))
            }
            Prop::Native(NativeProp::Source(crate::WebSource::Html { .. }))
                if kind == Kind::WebView && !self.access.allows_webviews() =>
            {
                self.deny(
                    "web views are not granted: add `webview` to capabilities in nsh.json".into(),
                );
                Prop::Native(NativeProp::Source(crate::WebSource::None))
            }
            prop => prop,
        }
    }

    fn set_prop(&mut self, id: NodeId, prop: Prop) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        // Rust components: `component` makes a view a host, its props
        // arrive decoded (`RustProp`; platform components keep JSON).
        let prop = match prop {
            Prop::Component(name) if self.nodes[key as usize].kind == Kind::View => {
                self.create_instance(id, name);
                return Ok(());
            }
            Prop::ComponentSlot(name) => {
                self.set_slot_name(id, name);
                return Ok(());
            }
            Prop::ComponentRender(slot) => {
                self.set_render_holder(id, slot);
                return Ok(());
            }
            Prop::RustProp(name, value) => {
                if !self.set_component_prop(id, name, value) {
                    log::warn!(target: "js", "a Rust component's prop on node {id}, which hosts none");
                }
                return Ok(());
            }
            prop => prop,
        };
        let prop = self.check_prop(key, prop);
        if let Prop::Screen(ScreenProp::StatusBar(setting)) = prop {
            self.set_status_bar(key, setting);
            return Ok(());
        }
        let node = &mut self.nodes[key as usize];
        let mut flags = 0;
        match prop {
            Prop::Source(source) => {
                if let Some(image) = node.image.as_mut() {
                    let size = source.as_ref().and_then(|s| s.size);
                    image.source = source;
                    flags |= dirty::IMAGE;
                    let layout = node.layout.expect("image layout");
                    if let Some(LeafContext::Image(natural)) =
                        self.layout.get_node_context_mut(layout)
                        && *natural != size
                    {
                        *natural = size;
                        self.layout.mark_dirty(layout).expect("taffy dirty");
                        self.needs_layout = true;
                    }
                }
            }
            Prop::Image(option) => {
                if let Some(image) = node.image.as_mut() {
                    match option {
                        ImageProp::DefaultSource(source) => image.options.placeholder = source,
                        ImageProp::BlurRadius(r) => image.options.blur_radius = r.max(Points::ZERO),
                        ImageProp::FadeDuration(ms) => image.options.fade_ms = ms,
                    }
                    flags |= dirty::IMAGE;
                }
            }
            Prop::ResizeMode(mode) => {
                if let Some(image) = node.image.as_mut() {
                    image.resize_mode = mode;
                    flags |= dirty::IMAGE;
                } else if let Some(control) = node.control.as_mut() {
                    apply_native(control, NativeProp::VideoResizeMode(mode));
                    flags |= dirty::NATIVE;
                }
            }
            Prop::Horizontal(horizontal) => {
                if let Some(scroll) = node.scroll.as_mut()
                    && scroll.horizontal != horizontal
                {
                    scroll.horizontal = horizontal;
                    flags |= dirty::SCROLL;
                    if let Some(list) = node.list.as_mut() {
                        list.set_horizontal(horizontal);
                    }
                    let children = node.children.clone();
                    self.sync_layout_style(key);
                    for child in children {
                        self.sync_layout_style(child);
                    }
                }
            }
            Prop::ShowsIndicator(shows) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.shows_indicator = shows;
                    flags |= dirty::SCROLL;
                }
            }
            Prop::Scroll(prop) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    let b = &mut scroll.behavior;
                    match prop {
                        ScrollProp::Paging(on) => b.paging = on,
                        ScrollProp::SnapInterval(v) => b.snap_interval = v.max(Points::ZERO),
                        ScrollProp::SnapOffsets(v) => b.snap_offsets = v,
                        ScrollProp::SnapAlignment(a) => b.snap_alignment = a,
                        ScrollProp::Deceleration(r) => b.deceleration = r,
                        ScrollProp::Bounces(on) => b.bounces = on,
                        ScrollProp::ContentInset(i) => b.content_inset = i,
                        ScrollProp::KeyboardTaps(t) => b.keyboard_taps = t,
                        ScrollProp::DisableIntervalMomentum(on) => b.one_step = on,
                        ScrollProp::MinZoom(v) => b.min_zoom = v.max(0.01),
                        ScrollProp::MaxZoom(v) => b.max_zoom = v.max(0.01),
                        ScrollProp::BouncesZoom(on) => b.bounces_zoom = on,
                        ScrollProp::Pinch(on) => b.pinch = on,
                        ScrollProp::CenterContent(on) => b.center_content = on,
                        ScrollProp::Zoom(scale) => {
                            // After the limits (sent with this commit's
                            // behavior) are applied.
                            let id = node.id;
                            self.commands.push((
                                id,
                                Command::ZoomTo {
                                    scale,
                                    center: None,
                                    animated: false,
                                },
                            ));
                        }
                        ScrollProp::EventThrottle(ms) => scroll.event_throttle_ms = ms.max(0.0),
                        ScrollProp::ContentOffset(offset) => {
                            // Where it starts: scrolled there after layout.
                            let id = node.id;
                            self.commands.push((
                                id,
                                Command::ScrollTo {
                                    x: offset.x,
                                    y: offset.y,
                                    animated: false,
                                },
                            ));
                        }
                    }
                    flags |= dirty::SCROLL_BEHAVIOR;
                }
            }
            Prop::Refreshing(refreshing) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.refreshing = refreshing;
                    flags |= dirty::REFRESHING;
                }
            }
            Prop::ScrollValue(value) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.scroll_value = value;
                    flags |= dirty::SCROLL;
                }
            }
            Prop::Count(count) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_count(count);
                }
            }
            Prop::EstimatedItemSize(size) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_estimated(size.0);
                }
            }
            Prop::Slot(slot) => {
                node.slot = Some(slot);
                let parent = node.parent;
                if let Some(list) = self
                    .nodes
                    .get_mut(parent as usize)
                    .and_then(|p| p.list.as_mut())
                {
                    list.add_slot_cell(slot, key);
                }
            }
            Prop::Splice(at, removed, inserted) => {
                if let Some(list) = node.list.as_mut() {
                    list.splice(at, removed, inserted);
                }
            }
            Prop::Columns(columns) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_columns(columns);
                }
            }
            Prop::Inverted(inverted) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_inverted(inverted);
                }
            }
            Prop::FollowEnd(threshold) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_follow_end(threshold.map(|t| t.0));
                }
            }
            Prop::StickyIndices(indices) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_sticky(indices);
                }
            }
            Prop::ListContent(content) => {
                if let Some(list) = node.list.as_mut() {
                    list.set_content(content);
                }
            }
            Prop::Viewability(config) => {
                if let Some(list) = node.list.as_mut()
                    && list.viewability != config
                {
                    list.viewability = config;
                    list.dirty = true;
                }
            }
            Prop::EndReachedThreshold(threshold) => {
                if let Some(list) = node.list.as_mut() {
                    list.end_threshold = threshold.max(0.0);
                }
            }
            Prop::CellIndex(index) => {
                let parent = node.parent;
                if let Some(list) = self
                    .nodes
                    .get_mut(parent as usize)
                    .and_then(|p| p.list.as_mut())
                {
                    list.rendered(key, index);
                }
            }
            Prop::PanValues(x, y) => {
                (node.gestures.pan_x, node.gestures.pan_y) = (x, y);
                flags |= dirty::PAN;
            }
            Prop::PinchValue(value) => {
                node.gestures.pinch = value;
                flags |= dirty::PAN;
            }
            Prop::RotationValue(value) => {
                node.gestures.rotation = value;
                flags |= dirty::PAN;
            }
            Prop::NumberOfLines(lines) => {
                if node.paragraph.lines != lines {
                    node.paragraph.lines = lines;
                    self.mark_text_block(key);
                }
            }
            Prop::EllipsizeMode(mode) => {
                node.paragraph.ellipsize_set = true;
                if node.paragraph.ellipsize != mode {
                    node.paragraph.ellipsize = mode;
                    self.mark_text_block(key);
                }
            }
            Prop::AllowFontScaling(allow) => {
                if node.paragraph.allow_font_scaling != allow {
                    node.paragraph.allow_font_scaling = allow;
                    self.font_settings_changed(key);
                }
            }
            Prop::Selectable(on) => {
                if node.paragraph.selectable != on {
                    node.paragraph.selectable = on;
                    self.mark_text_block(key);
                }
            }
            Prop::SelectionColor(color) if node.input.is_none() => {
                if node.paragraph.selection_color != color {
                    node.paragraph.selection_color = color;
                    self.mark_text_block(key);
                }
            }
            Prop::AdjustsFontSizeToFit(_) | Prop::MinimumFontScale(_) => {
                let before = (node.paragraph.fit, node.paragraph.min_font_scale);
                match prop {
                    Prop::AdjustsFontSizeToFit(on) => node.paragraph.fit = on,
                    Prop::MinimumFontScale(min) => node.paragraph.min_font_scale = min,
                    _ => {}
                }
                if (node.paragraph.fit, node.paragraph.min_font_scale) != before {
                    self.queue_text(key);
                    if let Some(layout) = self.nodes[key as usize].layout
                        && let Some(LeafContext::Text(cx)) =
                            self.layout.get_node_context_mut(layout)
                    {
                        cx.laid_out = None;
                    }
                }
            }
            Prop::MaxFontSizeMultiplier(max) => {
                if node.paragraph.max_font_multiplier != max {
                    node.paragraph.max_font_multiplier = max;
                    self.font_settings_changed(key);
                }
            }
            Prop::PointerEvents(value) => {
                let props = node.props.get_or_insert_default();
                if props.pointer_events != value {
                    props.pointer_events = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::HitSlop(value) => {
                let props = node.props.get_or_insert_default();
                if props.hit_slop != value {
                    props.hit_slop = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::PanActivation(value) => {
                let props = node.props.get_or_insert_default();
                if props.pan_activation != value {
                    props.pan_activation = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::InsetPadding(insets) | Prop::InsetMargin(insets) => {
                let slot = usize::from(matches!(prop, Prop::InsetMargin(_)));
                let mut all = node.insets.as_deref().copied().unwrap_or_default();
                if all[slot] != insets {
                    all[slot] = insets;
                    node.insets = (all != <[EdgeInsets; 2]>::default()).then(|| Box::new(all));
                    self.restyle(key);
                }
                return Ok(());
            }
            Prop::Class(names) => {
                self.set_classes(key, Some(names), None);
                return Ok(());
            }
            Prop::ClassList(list) => {
                self.set_classes(key, None, Some(list));
                return Ok(());
            }
            Prop::TailwindTheme(json) => {
                self.set_tailwind_theme(&json);
                return Ok(());
            }
            Prop::UiTheme(json) => {
                self.set_ui_theme(&json);
                return Ok(());
            }
            Prop::LayoutDirection(rtl) => {
                // The surface's direction (the root's): JS's `I18nManager`.
                if key == self.root && node.direction != Some(rtl) {
                    node.direction = Some(rtl);
                    self.mount.push(MountItem::LayoutDirection { rtl });
                    self.update_direction(key);
                    self.root_direction_changed(rtl);
                }
                return Ok(());
            }
            Prop::Appearance(appearance) => {
                // The app's appearance (the root's): `nsh.json`,
                // `setColorScheme`.
                if key == self.root {
                    self.set_appearance(appearance);
                }
                return Ok(());
            }
            Prop::Keymap(keymap) => {
                // The surface's shortcuts (the root's): `nsh/keymap`.
                if key == self.root {
                    self.mount.push(MountItem::Keymap(keymap));
                }
                return Ok(());
            }
            Prop::LongPressDelay(value) => {
                let props = node.props.get_or_insert_default();
                if props.long_press_delay != value {
                    props.long_press_delay = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::Ripple(value) => {
                let props = node.props.get_or_insert_default();
                if props.ripple != value {
                    props.ripple = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::Elevation(value) => {
                let value = value.max(Points::ZERO);
                if node.elevation != value {
                    node.elevation = value;
                    flags |= dirty::VIEW;
                }
            }
            Prop::ActiveStyle(patch) => {
                self.set_part_style(key, classes::Part::Active, patch);
                return Ok(());
            }
            Prop::HoverStyle(patch) => {
                self.set_part_style(key, classes::Part::Hover, patch);
                return Ok(());
            }
            Prop::FocusStyle(patch) => {
                self.set_part_style(key, classes::Part::Focus, patch);
                return Ok(());
            }
            Prop::DisabledStyle(patch) => {
                self.set_part_style(key, classes::Part::Disabled, patch);
                return Ok(());
            }
            Prop::SelectedStyle(patch) => {
                self.set_part_style(key, classes::Part::Selected, patch);
                return Ok(());
            }
            Prop::CheckedStyle(patch) => {
                self.set_part_style(key, classes::Part::Checked, patch);
                return Ok(());
            }
            Prop::HeaderStyle(patch) => {
                self.set_part_style(key, classes::Part::Header, patch);
                return Ok(());
            }
            Prop::HeaderClass(names) => {
                self.set_part_class(key, classes::Part::Header, &names);
                return Ok(());
            }
            Prop::HeaderClassList(list) => {
                self.set_part_class_list(key, classes::Part::Header, list);
                return Ok(());
            }
            Prop::ListContentClass(names) => {
                self.set_part_class(key, classes::Part::Content, &names);
                return Ok(());
            }
            Prop::TestId(value) => {
                let props = node.props.get_or_insert_default();
                if props.test_id != value {
                    props.test_id = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::WindowDrag(value) => {
                let props = node.props.get_or_insert_default();
                if props.window_drag != value {
                    props.window_drag = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::Focusable(value) => {
                let props = node.props.get_or_insert_default();
                if props.focusable != value {
                    props.focusable = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::SelectionArea(value) => {
                let props = node.props.get_or_insert_default();
                if props.selection_area != value {
                    props.selection_area = value;
                    flags |= dirty::VIEW_PROPS;
                }
            }
            Prop::Accessibility(prop) => {
                // A label changing on mounted props goes alone: it is
                // the one that follows app state (a count, a value).
                let label_only =
                    node.props.is_some() && matches!(prop, AccessibilityProp::Label(_));
                let props = node.props.get_or_insert_default();
                let a = &mut props.accessibility;
                let before = a.clone();
                match prop {
                    AccessibilityProp::Accessible(v) => a.accessible = v,
                    AccessibilityProp::Label(v) => a.label = v,
                    AccessibilityProp::Hint(v) => a.hint = v,
                    AccessibilityProp::Role(v) => a.role = v,
                    AccessibilityProp::State(v) => a.state = v,
                    AccessibilityProp::Value(v) => a.value = v,
                    AccessibilityProp::Hidden(v) => a.hidden = v,
                    AccessibilityProp::Important(accessible, hidden) => {
                        a.accessible = accessible;
                        a.hidden = hidden;
                    }
                    AccessibilityProp::LiveRegion(v) => a.live_region = v,
                    AccessibilityProp::Actions(v) => a.actions = v,
                }
                let selected =
                    (a.state.selected != before.state.selected).then_some(a.state.selected);
                let is_checked =
                    |s: &crate::AccessibilityState| s.checked == Some(crate::Checked::True);
                let checked = (is_checked(&a.state) != is_checked(&before.state))
                    .then(|| is_checked(&a.state));
                if *a != before {
                    flags |= if label_only {
                        dirty::LABEL
                    } else {
                        dirty::VIEW_PROPS
                    };
                }
                if selected.is_some() || checked.is_some() {
                    if flags != 0 {
                        self.mark(key, flags);
                    }
                    if let Some(selected) = selected {
                        self.set_selected(key, selected);
                    }
                    if let Some(checked) = checked {
                        self.set_checked(key, checked);
                    }
                    return Ok(());
                }
            }
            Prop::KeyboardDismissMode(mode) => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.keyboard_dismiss = mode;
                    flags |= dirty::SCROLL;
                }
            }
            // `animation` on a dialog: the value pages and dialogs share.
            Prop::Screen(ScreenProp::Animation(a)) if node.kind == Kind::Modal => {
                if let Some(root) = node.root.as_mut() {
                    root.modal.animation = match a {
                        ScreenAnimation::Default => ModalAnimation::Slide,
                        ScreenAnimation::Fade => ModalAnimation::Fade,
                        ScreenAnimation::None => ModalAnimation::None,
                    };
                    flags |= dirty::MODAL;
                }
            }
            Prop::Screen(prop) => {
                if let Some(root) = node.root.as_mut() {
                    let c = &mut root.screen;
                    match prop {
                        ScreenProp::Title(v) => c.title = v,
                        ScreenProp::HeaderShown(v) => c.header_shown = v,
                        ScreenProp::HeaderLargeTitle(v) => c.large_title = v,
                        ScreenProp::HeaderTransparent(v) => c.header_transparent = v,
                        ScreenProp::HeaderBackTitle(v) => c.back_title = v,
                        ScreenProp::HeaderBackVisible(v) => c.back_visible = v,
                        ScreenProp::PopOnSwipe(v) => c.gesture_enabled = v,
                        ScreenProp::PopOnBack(v) => c.prevent_back = !v,
                        ScreenProp::Animation(v) => c.animation = v,
                        // Taken by `set_prop`.
                        ScreenProp::StatusBar(_) => {}
                    }
                    // Header changes move the content.
                    root.laid_out = None;
                    self.needs_layout = true;
                    flags |= dirty::SCREEN;
                }
            }
            Prop::NativeId(name) => {
                if node.kind == Kind::InputAccessory {
                    let attached = node.parent != NO_KEY;
                    if attached {
                        self.accessory_attached(key, false);
                    }
                    if let Some(root) = self.nodes[key as usize].root.as_mut() {
                        root.native_id = name;
                    }
                    if attached {
                        self.accessory_attached(key, true);
                    }
                }
                return Ok(());
            }
            Prop::Modal(prop) => {
                if let Some(root) = node.root.as_mut() {
                    let c = &mut root.modal;
                    match prop {
                        ModalProp::Animation(v) => c.animation = v,
                        ModalProp::Presentation(v) => c.presentation = v,
                        ModalProp::StatusBarTranslucent(v) => c.status_bar_translucent = v,
                        ModalProp::Orientations(v) => c.orientations = v,
                        ModalProp::CloseOnEscape(v) => c.close_on_escape = v,
                        ModalProp::CloseOnBackdropPress(v) => c.close_on_backdrop_press = v,
                        ModalProp::CloseOnSwipe(v) => c.close_on_swipe = v,
                    }
                    flags |= dirty::MODAL;
                }
            }
            Prop::HeaderSlot(slot) => {
                if let Some(root) = node.root.as_mut()
                    && root.slot != slot
                {
                    root.slot = slot;
                    let parent = node.parent;
                    if parent != NO_KEY {
                        self.mark(parent, dirty::SCREEN);
                    }
                }
            }
            // `scrollEnabled` of scroll views and lists.
            Prop::Native(NativeProp::ScrollEnabled(on)) if node.scroll.is_some() => {
                if let Some(scroll) = node.scroll.as_mut() {
                    scroll.behavior.enabled = on;
                    flags |= dirty::SCROLL_BEHAVIOR;
                }
            }
            Prop::Component(name) => {
                if let Some(component) = node.component.as_mut() {
                    match &component.name {
                        None => component.name = Some(name),
                        Some(old) if *old != name => log::warn!(
                            target: "js",
                            "a native component cannot change from `{old}` to `{name}`; render a new element"
                        ),
                        Some(_) => {}
                    }
                }
            }
            Prop::ComponentProp(name, value) => {
                if let Some(component) = node.component.as_mut() {
                    let at = component.props.iter().position(|(k, _)| *k == name);
                    match (at, value) {
                        (Some(i), Some(v)) => component.props[i].1 = v,
                        (None, Some(v)) => component.props.push((name.clone(), v)),
                        (Some(i), None) => {
                            component.props.swap_remove(i);
                        }
                        (None, None) => {}
                    }
                    if !component.changed.contains(&name) {
                        component.changed.push(name);
                    }
                    flags |= dirty::COMPONENT;
                }
            }
            Prop::Native(prop) => {
                if let NativeProp::Disabled(d) = prop {
                    // `disabled:` classes, on any element.
                    self.set_disabled(key, d);
                }
                let node = &mut self.nodes[key as usize];
                if let Some(control) = node.control.as_mut() {
                    apply_native(control, prop);
                    flags |= dirty::NATIVE;
                }
            }
            Prop::Value(value) => {
                if let Some(input) = node.input.as_mut() {
                    input.value_set = true;
                    if input.value != value {
                        input.value = value;
                        flags |= dirty::INPUT_VALUE;
                        self.measure_input(key);
                    }
                }
            }
            Prop::DefaultValue(value) => {
                if let Some(input) = node.input.as_mut()
                    && !input.value_set
                {
                    input.value_set = true;
                    input.value = value;
                    flags |= dirty::INPUT_VALUE;
                    self.measure_input(key);
                }
            }
            prop => {
                let Some(input) = node.input.as_mut() else {
                    return Ok(());
                };
                let c = &mut input.config;
                let remeasure = match prop {
                    Prop::Placeholder(text) => {
                        c.placeholder = text;
                        true
                    }
                    Prop::Multiline(multiline) => {
                        c.multiline = multiline;
                        true
                    }
                    Prop::PlaceholderColor(color) => {
                        c.placeholder_color = color;
                        false
                    }
                    Prop::Secure(secure) => {
                        c.secure = secure;
                        false
                    }
                    Prop::KeyboardType(keyboard) => {
                        c.keyboard = keyboard;
                        false
                    }
                    Prop::KeyboardAppearance(appearance) => {
                        c.keyboard_appearance = appearance;
                        false
                    }
                    Prop::ReturnKeyType(key) => {
                        c.return_key = key;
                        false
                    }
                    Prop::AutoCapitalize(mode) => {
                        c.auto_capitalize = mode;
                        false
                    }
                    Prop::AutoCorrect(on) => {
                        c.auto_correct = on;
                        false
                    }
                    Prop::MaxLength(len) => {
                        c.max_length = len;
                        false
                    }
                    Prop::Readonly(on) => {
                        c.editable = !on;
                        false
                    }
                    Prop::SubmitBehavior(behavior) => {
                        c.submit_behavior = behavior;
                        false
                    }
                    Prop::AutoFocus(on) => {
                        c.auto_focus = on;
                        false
                    }
                    Prop::CaretHidden(on) => {
                        c.caret_hidden = on;
                        false
                    }
                    Prop::InputAccessoryViewId(name) => {
                        c.accessory_id = name;
                        false
                    }
                    Prop::SelectionColor(color) => {
                        c.selection_color = color;
                        false
                    }
                    Prop::AutoFill(fill) => {
                        c.auto_fill = fill;
                        false
                    }
                    Prop::Selection(start, end) => {
                        // Applied after the batch's value.
                        let id = node.id;
                        self.commands
                            .push((id, Command::SetSelection { start, end }));
                        return Ok(());
                    }
                    _ => return Ok(()),
                };
                flags |= dirty::INPUT;
                if remeasure {
                    self.measure_input(key);
                }
            }
        }
        if flags != 0 {
            self.mark(key, flags);
        }
        Ok(())
    }

    fn insert(
        &mut self,
        parent_id: NodeId,
        child_id: NodeId,
        before: Option<NodeId>,
    ) -> Result<(), ApplyError> {
        let parent = self.key(parent_id)?;
        let child = self.key(child_id)?;
        let before = before.map(|b| self.key(b)).transpose()?;
        if child == self.root || self.is_ancestor(child, parent) {
            return Err(ApplyError::Cycle(child_id));
        }
        if let Some(b) = before
            && (b == child || self.nodes[b as usize].parent != parent)
        {
            return Err(ApplyError::BadAnchor(self.nodes[b as usize].id));
        }

        // Detach from the current position. A native view that stays visible
        // is moved by the insert below, so only emit a remove when the new
        // parent has no native view.
        let parent_created = self.nodes[parent as usize].created;
        if self.nodes[child as usize].parent != NO_KEY {
            self.unlink(child, !parent_created);
        }

        let p = &self.nodes[parent as usize];
        // Inside a block or a span (not inside an attachment: its children
        // are its own layout's).
        let inline = p.kind == Kind::Text;
        let pos = match before {
            Some(b) => p
                .children
                .iter()
                .position(|&c| c == b)
                .expect("anchor is a child"),
            None => p.children.len(),
        };
        self.set_inline(child, inline);
        self.nodes[child as usize].parent = parent;
        self.nodes[parent as usize].children.insert(pos, child);
        self.status_bars_moved |= !self.status_bar_nodes.is_empty();
        self.update_direction(child);
        self.groups_changed(child, parent);
        if self.nodes[child as usize]
            .classes
            .as_ref()
            .is_some_and(|c| c.deps() & crate::tailwind::deps::POSITION != 0)
        {
            self.nodes[parent as usize].positional_children = true;
        }
        if self.nodes[parent as usize].positional_children {
            self.positions_changed(parent);
        }

        if inline {
            self.mark_text_block(parent);
            return Ok(());
        }
        // A cell joining a list: register its slot and derive its style
        // from the list orientation.
        if self.nodes[parent as usize].list.is_some() {
            if let Some(slot) = self.nodes[child as usize].slot {
                self.nodes[parent as usize]
                    .list
                    .as_mut()
                    .expect("list")
                    .add_slot_cell(slot, child);
            }
            self.sync_layout_style(child);
        }
        let child_kind = self.nodes[child as usize].kind;
        if child_kind == Kind::InputAccessory {
            self.accessory_attached(child, true);
        }
        if child_kind == Kind::HeaderItem {
            self.mark(parent, dirty::SCREEN);
        }
        if let Some(child_layout) = self.nodes[child as usize].layout {
            // List cells, screens, modals and header items stay independent
            // layout roots.
            if self.nodes[parent as usize].list.is_none() && !child_kind.is_root() {
                let index = self.render_index(parent, pos);
                let parent_layout = self.nodes[parent as usize]
                    .layout
                    .expect("block parent has layout");
                self.layout
                    .insert_child_at_index(parent_layout, index as usize, child_layout)
                    .expect("taffy insert");
            }
            self.needs_layout = true;
            if parent_created {
                self.ensure_created(child);
                let before = self.native_before(parent, child);
                // By the nodes' own ids (the ops may name an alias).
                self.mount.push(MountItem::Insert {
                    parent: self.nodes[parent as usize].id,
                    child: self.nodes[child as usize].id,
                    before,
                });
            }
        }
        Ok(())
    }

    /// Detaches `key` from its parent (Remove op).
    fn detach(&mut self, key: Key) {
        self.unlink(key, true);
    }

    /// Unlinks `key` from its parent's children, layout and (optionally)
    /// native hierarchy.
    fn unlink(&mut self, key: Key, emit_remove: bool) {
        let parent = self.nodes[key as usize].parent;
        let siblings = &mut self.nodes[parent as usize].children;
        let pos = siblings
            .iter()
            .position(|&c| c == key)
            .expect("child of parent");
        siblings.remove(pos);
        if let Some(list) = self.nodes[parent as usize].list.as_mut() {
            list.remove_cell(key);
        }
        if self.nodes[parent as usize].positional_children {
            self.positions_changed(parent);
        }
        self.nodes[key as usize].parent = NO_KEY;
        self.groups_changed(key, parent);
        self.status_bars_moved |= !self.status_bar_nodes.is_empty();
        let node = &self.nodes[key as usize];
        let (inline, layout, created, id) = (node.inline, node.layout, node.created, node.id);
        if inline {
            self.mark_text_block(parent);
            return;
        }
        let kind = self.nodes[key as usize].kind;
        if kind == Kind::InputAccessory {
            self.accessory_attached(key, false);
        }
        if kind == Kind::HeaderItem {
            self.mark(parent, dirty::SCREEN);
        }
        if let Some(layout) = layout {
            if self.nodes[parent as usize].list.is_none() && !kind.is_root() {
                let parent_layout = self.nodes[parent as usize]
                    .layout
                    .expect("block parent has layout");
                self.layout
                    .remove_child(parent_layout, layout)
                    .expect("taffy remove");
                self.forget_hoisted(parent);
            }
            self.needs_layout = true;
            if emit_remove && created && self.nodes[parent as usize].created {
                self.mount.push(MountItem::Remove { id });
            }
        }
    }

    /// Taffy keeps the absolutely placed children a node laid out last
    /// (its "hoisted" children) until it lays the node out again with
    /// children, which a node left without any never is: an absolute child
    /// removed from it stays listed, and once its node is freed the next
    /// layout fails. Such a node gets a fresh layout node.
    fn forget_hoisted(&mut self, key: Key) {
        let Some(old) = self.nodes[key as usize].layout else {
            return;
        };
        let childless = self.layout.children(old).is_ok_and(|c| c.is_empty());
        let hoisting = self
            .layout
            .hoisted_children(old)
            .is_ok_and(|h| !h.is_empty());
        if !childless || !hoisting {
            return;
        }
        let style = self.layout.style(old).expect("taffy style").clone();
        let fresh = match self.layout.get_node_context_mut(old) {
            Some(context) => {
                let context = std::mem::replace(context, LeafContext::Image(None));
                self.layout.new_leaf_with_context(style, context)
            }
            None => self.layout.new_leaf(style),
        }
        .expect("taffy leaf");
        if let Some(parent) = self.layout.parent(old) {
            let index = self
                .layout
                .children(parent)
                .expect("taffy children")
                .iter()
                .position(|&c| c == old)
                .expect("taffy child");
            self.layout
                .replace_child_at_index(parent, index, fresh)
                .expect("taffy replace");
        }
        self.layout.remove(old).expect("taffy remove");
        self.nodes[key as usize].layout = Some(fresh);
        self.needs_layout = true;
    }

    /// A page's or dialog's `statusBar`: changed in place while the node
    /// is in the tree (so it keeps its place among the others); its entry
    /// is added or removed by [`Core::sync_status_bars`].
    fn set_status_bar(&mut self, key: Key, setting: Option<crate::StatusBarSetting>) {
        let Some(root) = self.nodes[key as usize].root.as_mut() else {
            return;
        };
        root.status_bar = setting;
        let entry = root.status_bar_entry;
        match (setting, entry) {
            (Some(s), Some(e)) => self.components.status_bars.set(e, s),
            (Some(_), None) => {
                if !self.status_bar_nodes.contains(&key) {
                    self.status_bar_nodes.push(key);
                }
                self.status_bars_moved = true;
            }
            (None, entry) => {
                if let Some(e) = entry {
                    self.components.status_bars.remove(e);
                    if let Some(root) = self.nodes[key as usize].root.as_mut() {
                        root.status_bar_entry = None;
                    }
                }
                self.status_bar_nodes.retain(|&k| k != key);
            }
        }
    }

    /// The `statusBar` of each screen and modal in the tree has an entry,
    /// added in the order they got their setting; the others have none
    /// (once per commit, after the tree changed).
    fn sync_status_bars(&mut self) {
        if !std::mem::take(&mut self.status_bars_moved) {
            return;
        }
        for i in 0..self.status_bar_nodes.len() {
            let key = self.status_bar_nodes[i];
            let in_tree = self.is_ancestor(self.root, key);
            let Some(root) = self.nodes[key as usize].root.as_mut() else {
                continue;
            };
            match (in_tree, root.status_bar_entry, root.status_bar) {
                (true, None, Some(setting)) => {
                    root.status_bar_entry = Some(self.components.status_bars.add(setting));
                }
                (false, Some(entry), _) => {
                    root.status_bar_entry = None;
                    self.components.status_bars.remove(entry);
                }
                _ => {}
            }
        }
    }

    fn release(&mut self, id: NodeId) -> Result<(), ApplyError> {
        let key = self.key(id)?;
        if key == self.root {
            return Ok(());
        }
        if self.nodes[key as usize].parent != NO_KEY {
            // Deleting the view also detaches it natively.
            self.unlink(key, false);
        }
        let node = self.nodes.remove(key as usize);
        self.slots[id as usize] = 0;
        if node.kind == Kind::List {
            self.lists.retain(|&k| k != key);
        }
        if node.kind == Kind::Text {
            self.attachment_blocks.retain(|&k| k != key);
        }
        if node.kind.is_root() {
            self.roots.retain(|&k| k != key);
        }
        if let Some(root) = node.root.as_ref()
            && root.status_bar.is_some()
        {
            if let Some(entry) = root.status_bar_entry {
                self.components.status_bars.remove(entry);
            }
            self.status_bar_nodes.retain(|&k| k != key);
        }
        // Children are released by their own ops (the whole subtree became
        // unreachable in JS); orphan them so they never point at a dead key.
        for &c in &node.children {
            if let Some(child) = self.nodes.get_mut(c as usize) {
                child.parent = NO_KEY;
            }
        }
        if let Some(layout) = node.layout {
            self.layout.remove(layout).expect("taffy remove");
        }
        if node.created {
            self.mount.push(MountItem::Delete { id });
        }
        Ok(())
    }

    fn is_ancestor(&self, ancestor: Key, mut key: Key) -> bool {
        while key != NO_KEY {
            if key == ancestor {
                return true;
            }
            key = self.nodes[key as usize].parent;
        }
        false
    }

    /// The native sibling `child` goes below: native views are stacked by
    /// `zIndex`, then in tree order.
    fn native_before(&self, parent: Key, child: Key) -> Option<NodeId> {
        let children = &self.nodes[parent as usize].children;
        let z = self.nodes[child as usize].z_index;
        let pos = children.iter().position(|&c| c == child)?;
        let mut best: Option<(i32, usize, NodeId)> = None;
        for (i, &c) in children.iter().enumerate() {
            let n = &self.nodes[c as usize];
            if c == child || !n.created || n.layout.is_none() {
                continue;
            }
            if (n.z_index, i) > (z, pos) && best.is_none_or(|b| (n.z_index, i) < (b.0, b.1)) {
                best = Some((n.z_index, i, n.id));
            }
        }
        best.map(|b| b.2)
    }

    /// `key`'s `zIndex` changed: moves its native view among its siblings.
    fn restack(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let (parent, id) = (node.parent, node.id);
        if parent == NO_KEY || !node.created || node.layout.is_none() || node.inline {
            return;
        }
        let p = &self.nodes[parent as usize];
        if !p.created || p.list.is_some() {
            return;
        }
        let before = self.native_before(parent, key);
        self.mount.push(MountItem::Insert {
            parent: p.id,
            child: id,
            before,
        });
    }

    /// `allowFontScaling` / `maxFontSizeMultiplier` changed on `key`.
    fn font_settings_changed(&mut self, key: Key) {
        match self.nodes[key as usize].kind {
            Kind::Text => self.mark_text_block(key),
            Kind::Input => {
                self.measure_input(key);
                self.mark(key, dirty::INPUT);
            }
            _ => {}
        }
    }

    /// Index among rendering (layout-backed) children of `parent` for a
    /// child at `pos` in the full children list.
    fn render_index(&self, parent: Key, pos: usize) -> u32 {
        self.nodes[parent as usize].children[..pos]
            .iter()
            .map(|&c| &self.nodes[c as usize])
            .filter(|c| c.layout.is_some() && !c.kind.is_root())
            .count() as u32
    }

    /// Moves `key` (and its subtree) into or out of an inline context.
    fn set_inline(&mut self, key: Key, inline: bool) {
        let node = &mut self.nodes[key as usize];
        if node.inline == inline {
            return;
        }
        node.inline = inline;
        if node.kind != Kind::Text {
            // Other elements inside text are not supported; they are ignored.
            return;
        }
        if inline {
            // Block -> span: spans have no layout node or native view.
            if let Some(l) = node.layout.take() {
                self.layout.remove(l).expect("taffy remove");
            }
            if node.created {
                node.created = false;
                self.mount.push(MountItem::Delete { id: node.id });
            }
        } else {
            let style = node.base_layout.as_deref().cloned().unwrap_or_default();
            let cx = LeafContext::Text(TextContext::new(Arc::default()));
            node.layout = Some(
                self.layout
                    .new_leaf_with_context(style, cx)
                    .expect("taffy leaf"),
            );
            self.mark(key, dirty::TEXT_BUILD);
        }
    }

    /// Creates the native views of `key`'s subtree if needed, wiring up the
    /// children in order.
    fn ensure_created(&mut self, key: Key) {
        let has_attachments = self.attachment_blocks.contains(&key);
        let node = &mut self.nodes[key as usize];
        if node.created || node.layout.is_none() {
            return;
        }
        node.created = true;
        node.frame = Bounds::UNSET;
        if let Some(scroll) = node.scroll.as_mut() {
            scroll.content = None;
        }
        let id = node.id;
        let kind = node.kind;
        // A new native view already has the default style and no listeners.
        let mut flags = 0;
        if node.view_style != ViewStyle::default() || node.elevation != Points::ZERO {
            flags |= dirty::VIEW;
        }
        if !node.events.is_empty() {
            flags |= dirty::EVENTS;
        }
        if kind == Kind::Text {
            flags |= dirty::TEXT_MOUNT;
            if has_attachments {
                // Its attachments' views go inside the new view.
                node.attached.clear();
                flags |= dirty::TEXT_BUILD;
            }
        }
        if node.image.as_ref().is_some_and(|i| i.source.is_some()) {
            flags |= dirty::IMAGE;
        }
        if let Some(scroll) = &node.scroll {
            flags |= dirty::SCROLL;
            if scroll.refreshing {
                flags |= dirty::REFRESHING;
            }
        }
        if !node.animated.is_empty() || node.drawing.as_ref().is_some_and(|d| !d.slots.is_empty()) {
            flags |= dirty::ANIMATION;
        }
        if node.drawing.is_some() {
            flags |= dirty::CANVAS;
        }
        if !node.gestures.is_empty() {
            flags |= dirty::PAN;
        }
        if node.props.is_some() {
            flags |= dirty::VIEW_PROPS;
        }
        match kind {
            Kind::Screen => flags |= dirty::SCREEN,
            Kind::Modal => flags |= dirty::MODAL,
            _ => {}
        }
        if node.control.is_some() {
            flags |= dirty::NATIVE;
        }
        if let Some(list) = node.list.as_mut() {
            list.dirty = true;
        }
        if let Some(input) = node.input.as_mut() {
            input.content = None;
            flags |= dirty::INPUT;
            if !input.value.is_empty() {
                flags |= dirty::INPUT_VALUE;
            }
        }
        self.mount.push(MountItem::Create {
            id,
            kind: kind.view_kind(),
        });
        if let Some(component) = self.nodes[key as usize].component.as_mut() {
            match &component.name {
                Some(name) => self.mount.push(MountItem::Component {
                    id,
                    name: name.clone(),
                }),
                None => log::warn!(target: "js", "a <native> element without `component`"),
            }
            // The new component gets every property.
            component.changed = component.props.iter().map(|(k, _)| k.clone()).collect();
            flags |= dirty::COMPONENT;
        }
        if let Some(item) = self.creation_tracking(key) {
            self.mount.push(item);
        }
        self.mark(key, flags);
        if !kind.is_container() {
            return;
        }
        // Bottom to top: by `zIndex`, then in order.
        let mut children = std::mem::take(&mut self.nodes[key as usize].children);
        let original = children.clone();
        if children
            .iter()
            .any(|&c| self.nodes[c as usize].z_index != 0)
        {
            children.sort_by_key(|&c| self.nodes[c as usize].z_index);
        }
        for &child in &children {
            if self.nodes[child as usize].layout.is_some() {
                self.ensure_created(child);
                let child_id = self.nodes[child as usize].id;
                self.mount.push(MountItem::Insert {
                    parent: id,
                    child: child_id,
                    before: None,
                });
            }
        }
        self.nodes[key as usize].children = original;
    }

    #[inline]
    fn mark(&mut self, key: Key, flags: u32) {
        let node = &mut self.nodes[key as usize];
        node.dirty |= flags;
        if !node.queued {
            node.queued = true;
            self.queue.push(key);
        }
    }

    /// Marks the text block containing `key` (or `key` itself) for rebuild.
    fn mark_text_block(&mut self, mut key: Key) {
        loop {
            let node = &self.nodes[key as usize];
            if !node.inline {
                if node.kind == Kind::Text {
                    self.mark(key, dirty::TEXT_BUILD);
                }
                return;
            }
            if node.parent == NO_KEY {
                // A detached span: rebuilt when it is inserted into a block.
                return;
            }
            key = node.parent;
        }
    }

    fn flush_dirty(&mut self) {
        loop {
            self.flush_dirty_once();
            // Text blocks create the views of new attachments while
            // flushing, which queues them; texts inside attachments built
            // after their block resize them, which builds the block again.
            self.check_attachments();
            if self.queue.is_empty() {
                break;
            }
        }
    }

    fn flush_dirty_once(&mut self) {
        let mut queue = std::mem::take(&mut self.queue);
        for &key in &queue {
            let Some(node) = self.nodes.get_mut(key as usize) else {
                continue;
            };
            let mut flags = std::mem::take(&mut node.dirty);
            node.queued = false;
            if flags & dirty::TEXT_BUILD != 0 && node.layout.is_some() && self.rebuild_text(key) {
                flags |= dirty::TEXT_MOUNT;
            }
            let node = &self.nodes[key as usize];
            if !node.created {
                continue;
            }
            let id = node.id;
            if flags & dirty::VIEW != 0 {
                let style = self.mounted_view_style(key);
                self.mount.push(MountItem::ViewStyle { id, style });
            }
            if flags & dirty::EVENTS != 0 {
                self.mount.push(MountItem::Events {
                    id,
                    events: node.mounted_events(),
                });
            }
            if flags & dirty::IMAGE != 0
                && let Some(image) = &node.image
            {
                // An explicit `resizeMode` wins over `object-*` classes.
                let resize_mode = match node.class_props.as_ref().and_then(|c| c.resize_mode) {
                    Some(m) if image.resize_mode == ResizeMode::default() => m,
                    _ => image.resize_mode,
                };
                // Bundled assets with variants: the one for the screen.
                let ratio = self.components.environment.pixel_ratio;
                self.mount.push(MountItem::Image {
                    id,
                    source: image.source.as_ref().map(|s| s.resolved(ratio)),
                    resize_mode,
                    options: crate::mount::ImageOptions {
                        placeholder: image
                            .options
                            .placeholder
                            .as_ref()
                            .map(|s| s.resolved(ratio)),
                        blur_radius: image.options.blur_radius,
                        fade_ms: image.options.fade_ms,
                    },
                });
            }
            if let Some(scroll) = &node.scroll {
                if flags & dirty::SCROLL != 0 {
                    self.mount.push(MountItem::ScrollConfig {
                        id,
                        horizontal: scroll.horizontal,
                        shows_indicator: scroll.shows_indicator,
                        scroll_value: scroll.scroll_value,
                        keyboard_dismiss: scroll.keyboard_dismiss,
                        mirrored: node.kind == Kind::List && scroll.horizontal && node.rtl,
                    });
                }
                if flags & dirty::SCROLL_BEHAVIOR != 0 {
                    self.mount.push(MountItem::ScrollBehavior {
                        id,
                        behavior: Box::new(scroll.behavior.clone()),
                    });
                }
                if flags & dirty::REFRESHING != 0 {
                    self.mount.push(MountItem::Refreshing {
                        id,
                        refreshing: scroll.refreshing,
                    });
                }
            }
            if flags & dirty::ANIMATION != 0 {
                let mut bindings = node.animated.clone();
                if let Some(d) = &node.drawing {
                    bindings.extend(
                        d.slots
                            .iter()
                            .enumerate()
                            .map(|(i, &n)| (AnimationProp::CanvasArg(i as u16), n)),
                    );
                }
                self.mount
                    .push(MountItem::AnimationBindings { id, bindings });
            }
            if flags & dirty::CANVAS != 0
                && let Some(drawing) = &node.drawing
            {
                self.mount.push(MountItem::Canvas {
                    id,
                    drawing: drawing.clone(),
                });
            }
            if flags & dirty::PAN != 0 {
                self.mount.push(MountItem::Gestures {
                    id,
                    values: node.gestures,
                });
            }
            if flags & dirty::VIEW_PROPS != 0 {
                let mut props = node.props.clone().unwrap_or_default();
                if let Some(p) = node.class_props.as_ref().and_then(|c| c.pointer_events)
                    && props.pointer_events == crate::props::PointerEvents::Auto
                {
                    props.pointer_events = p;
                }
                // A `disabled` view is so for screen readers too, as a Rust
                // component's `disabled` and React Native's `Pressable`.
                if node.states & crate::tailwind::state::DISABLED != 0 {
                    props.accessibility.state.disabled = true;
                }
                self.mount.push(MountItem::ViewProps { id, props });
            } else if flags & dirty::LABEL != 0 {
                let label = node
                    .props
                    .as_ref()
                    .and_then(|p| p.accessibility.label.clone());
                self.mount.push(MountItem::AccessibilityLabel { id, label });
            }
            if flags & dirty::SCREEN != 0
                && let Some(config) = self.screen_config(key)
            {
                self.mount.push(MountItem::ScreenConfig {
                    id,
                    config: Box::new(config),
                });
            }
            if flags & dirty::COMPONENT != 0
                && let Some(component) = self.nodes[key as usize].component.as_mut()
                && !component.changed.is_empty()
            {
                let props = std::mem::take(&mut component.changed)
                    .into_iter()
                    .map(|name| {
                        let value = component
                            .props
                            .iter()
                            .find(|(k, _)| *k == name)
                            .map(|(_, v)| v.clone());
                        (name, value)
                    })
                    .collect();
                self.mount.push(MountItem::ComponentProps { id, props });
            }
            let node = &self.nodes[key as usize];
            if flags & dirty::NATIVE != 0
                && let Some(control) = &node.control
            {
                self.mount.push(MountItem::NativeProps {
                    id,
                    props: (**control).clone(),
                });
            }
            if flags & dirty::MODAL != 0
                && let Some(root) = &node.root
            {
                let config = root.modal;
                self.mount.push(MountItem::ModalConfig { id, config });
            }
            if let Some(input) = &node.input {
                if flags & dirty::INPUT != 0 {
                    let mut config = input.config.clone();
                    config.accessory = config
                        .accessory_id
                        .as_deref()
                        .and_then(|name| self.accessories.get(name).copied());
                    // An explicit prop (Rust), then the style key, then
                    // the classes (`placeholder:text-*`, `caret-*`,
                    // `selection:text-*`).
                    config.placeholder_color = config
                        .placeholder_color
                        .or(node.text_style.placeholder_color);
                    config.selection_color =
                        config.selection_color.or(node.text_style.selection_color);
                    if let Some(c) = node.class_props.as_deref() {
                        config.placeholder_color = config.placeholder_color.or(c.placeholder_color);
                        config.selection_color = config
                            .selection_color
                            .or(c.selection_color)
                            .or(c.caret_color);
                    }
                    config.style = scaled(
                        self.inherited_text_style(key).inherit(&node.text_style),
                        node.paragraph.font_multiplier(self.font_scale),
                    );
                    config.align =
                        directed_align(node.text_style.text_align.unwrap_or_default(), node.rtl);
                    config.rtl = node.text_style.writing_direction.unwrap_or(node.rtl);
                    self.mount.push(MountItem::Input {
                        id,
                        config: Box::new(config),
                    });
                }
                if flags & dirty::INPUT_VALUE != 0 {
                    self.mount.push(MountItem::InputValue {
                        id,
                        text: input.value.clone(),
                        count: input.count,
                    });
                }
            }
            if flags & dirty::TEXT_MOUNT != 0 {
                self.queue_text(key);
            }
        }
        queue.clear();
        // Keep the allocation (unless flushing queued more).
        if self.queue.is_empty() {
            self.queue = queue;
        }
    }

    /// `onContentSizeChange` of input `key` if the size of its value (at the
    /// content box's width when multiline) changed. An empty input reports
    /// an empty line, as React Native does, though it is measured (sized)
    /// with its placeholder.
    fn report_content_size(&mut self, key: Key, width: f32) {
        let node = &self.nodes[key as usize];
        let (Some(input), Some(layout)) = (node.input.as_ref(), node.layout) else {
            return;
        };
        let max = input.config.multiline.then_some(width);
        let (placeholder, empty_line) = (input.placeholder_measured, input.empty_line);
        let Some(LeafContext::Text(cx)) = self.layout.get_node_context_mut(layout) else {
            return;
        };
        let size = if !placeholder {
            measure_text(&self.text_engine, &mut self.measure_cache, cx, max)
        } else if let Some((hash, size)) = empty_line
            && hash == cx.layout_hash
        {
            size
        } else {
            // Measured once per placeholder and style.
            let hash = cx.layout_hash;
            let mut empty = TextContext::new(Arc::new(self.input_text(key, "")));
            let size = measure_text(&self.text_engine, &mut self.measure_cache, &mut empty, max);
            let input = self.nodes[key as usize]
                .input
                .as_mut()
                .expect("input state");
            input.empty_line = Some((hash, size));
            size
        };
        let node = &mut self.nodes[key as usize];
        let input = node.input.as_mut().expect("input state");
        if input.content_size == Some(size) {
            return;
        }
        input.content_size = Some(size);
        let id = node.id;
        self.events.push(crate::ui::JsEvent::Event(
            id,
            EventKind::ContentSizeChange,
            EventPayload::Map(vec![
                ("width", PayloadValue::Number(size.width.0 as f64)),
                ("height", PayloadValue::Number(size.height.0 as f64)),
            ]),
        ));
    }

    fn text_cx(&mut self, layout: LayoutId) -> &mut TextContext<M::Prepared> {
        match self.layout.get_node_context_mut(layout) {
            Some(LeafContext::Text(cx)) => cx,
            _ => unreachable!("text block without text context"),
        }
    }

    /// The text style `key` inherits from the views around it, as CSS
    /// does: `color` / `text-white` on a container styles the text in it
    /// (React Native views ignore text styles).
    fn inherited_text_style(&self, key: Key) -> ResolvedTextStyle {
        let mut chain: Vec<Key> = Vec::new();
        let mut k = self.nodes[key as usize].parent;
        while k != NO_KEY {
            let node = &self.nodes[k as usize];
            if node.kind != Kind::Text && node.text_style != TextStyle::default() {
                chain.push(k);
            }
            k = node.parent;
        }
        chain
            .iter()
            .rev()
            .fold(ResolvedTextStyle::default(), |style, &k| {
                style.inherit(&self.nodes[k as usize].text_style)
            })
    }

    /// The nearest `textAlign` set on a view around `key`.
    fn inherited_text_align(&self, key: Key) -> Option<TextAlign> {
        let mut k = self.nodes[key as usize].parent;
        while k != NO_KEY {
            let node = &self.nodes[k as usize];
            if let Some(a) = node.text_style.text_align {
                return Some(a);
            }
            k = node.parent;
        }
        None
    }

    /// Text blocks and inputs inside `key` inherit its new text style.
    fn mark_inheriting_text(&mut self, key: Key) {
        let mut stack: Vec<Key> = self.nodes[key as usize].children.clone();
        while let Some(k) = stack.pop() {
            match self.nodes[k as usize].kind {
                Kind::Text => self.mark_text_block(k),
                Kind::Input => {
                    self.measure_input(k);
                    self.mark(k, dirty::INPUT);
                }
                Kind::RawText => {}
                _ => stack.extend_from_slice(&self.nodes[k as usize].children),
            }
        }
    }

    /// Rebuilds the attributed text of a block. Returns true if it changed.
    ///
    /// Only layout-relevant changes invalidate measurement and layout; a
    /// color change keeps both and just needs a new final layout.
    fn rebuild_text(&mut self, key: Key) -> bool {
        let node = &self.nodes[key as usize];
        let class = node.class_props.as_deref();
        // Explicit props win over classes (`numberOfLines` over
        // `line-clamp-*`).
        let max_lines = match (node.paragraph.lines, class.and_then(|c| c.number_of_lines)) {
            (0, Some(n)) => n,
            (n, _) => n,
        };
        let ellipsize = match class.and_then(|c| c.ellipsize) {
            Some(m) if !node.paragraph.ellipsize_set => m,
            _ => node.paragraph.ellipsize,
        };
        let inherited = self.inherited_text_style(key);
        let mut text = AttributedText {
            align: directed_align(
                node.text_style
                    .text_align
                    .or(self.inherited_text_align(key))
                    .unwrap_or_default(),
                node.rtl,
            ),
            writing_direction: node.text_style.writing_direction,
            rtl: node.rtl,
            max_lines,
            ellipsize,
            // A line clamp already keeps lines from wrapping past it, and
            // must ellipsize at the box (`truncate`).
            no_wrap: max_lines == 0 && class.and_then(|c| c.no_wrap).unwrap_or(false),
            shadow: node.text_style.shadow(),
            selectable: node
                .paragraph
                .selectable
                .or(class.and_then(|c| c.selectable)),
            selection_color: node
                .paragraph
                .selection_color
                .or(node.text_style.selection_color)
                .or(class.and_then(|c| c.selection_color)),
            ..AttributedText::default()
        };
        let base = inherited.inherit(&node.text_style);
        let multiplier = node.paragraph.font_multiplier(self.font_scale);
        let inserted = self.update_attachments(key);
        self.collect_text(key, base, &mut text);
        if multiplier != 1.0 {
            for span in &mut text.spans {
                span.style = scaled(span.style, multiplier);
            }
        }
        let layout = self.nodes[key as usize]
            .layout
            .expect("text block has layout");
        let cx = self.text_cx(layout);
        if *cx.text == text {
            if inserted {
                // New attachment views: placed with the next final layout.
                cx.laid_out = None;
            }
            return inserted;
        }
        let same_layout = cx.text.layout_eq(&text);
        cx.text = Arc::new(text);
        // Platform typesetting state embeds colors too: always rebuild it.
        cx.prepared = None;
        cx.laid_out = None;
        if !same_layout {
            cx.layout_hash = cx.text.layout_hash();
            cx.cache = None;
            self.layout.mark_dirty(layout).expect("taffy dirty");
            self.needs_layout = true;
        }
        true
    }

    fn collect_text(&self, key: Key, style: ResolvedTextStyle, out: &mut AttributedText) {
        for &c in &self.nodes[key as usize].children {
            let child = &self.nodes[c as usize];
            match child.kind {
                Kind::RawText => out.push(&child.text, style),
                Kind::Text => {
                    let start = out.len_utf16();
                    let mut inner = style.inherit(&child.text_style);
                    if let Some(b) = span_background(&child.view_style, child.id) {
                        inner.background = Some(b);
                    }
                    self.collect_text(c, inner, out);
                    if pressable(child.events) {
                        out.links.push(crate::text::TextLink {
                            start,
                            end: out.len_utf16(),
                            node: child.id,
                            events: child.events,
                        });
                    }
                }
                // An attachment, laid out by `update_attachments`.
                _ => {
                    if let Some(l) = child.layout {
                        let size = self.layout.layout(l).expect("taffy layout").size;
                        out.push_attachment(
                            crate::geometry::size_pt(size.width, size.height),
                            style,
                        );
                    }
                }
            }
        }
    }

    /// The attachments of text block `key` (views and images inside it,
    /// also inside its spans), in text order.
    fn attachments_of(&self, key: Key, out: &mut Vec<Key>) {
        for &c in &self.nodes[key as usize].children {
            let child = &self.nodes[c as usize];
            match child.kind {
                Kind::RawText => {}
                Kind::Text => self.attachments_of(c, out),
                _ if child.layout.is_some() => out.push(c),
                _ => {}
            }
        }
    }

    /// The text block an attachment is in, if it is one.
    fn attachment_block(&self, key: Key) -> Option<Key> {
        let node = &self.nodes[key as usize];
        if !node.inline || node.layout.is_none() || node.kind == Kind::Text {
            return None;
        }
        let mut k = node.parent;
        while k != NO_KEY {
            let n = &self.nodes[k as usize];
            if !n.inline {
                return (n.kind == Kind::Text).then_some(k);
            }
            k = n.parent;
        }
        None
    }

    /// Lays out the attachments of text block `key` (each a layout root
    /// at its own content size) and keeps their native views inside the
    /// block's. Returns true if views were inserted (they need placing).
    fn update_attachments(&mut self, key: Key) -> bool {
        let mut keys = Vec::new();
        self.attachments_of(key, &mut keys);
        let block = &self.nodes[key as usize];
        if keys.is_empty() && block.attached.is_empty() {
            return false;
        }
        let (block_id, created) = (block.id, block.created);
        let old = std::mem::take(&mut self.nodes[key as usize].attached);
        let mut inserted = false;
        for &a in &keys {
            let id = self.nodes[a as usize].id;
            let new = !old.contains(&id);
            if created && new {
                self.ensure_created(a);
                self.mount.push(MountItem::Insert {
                    parent: block_id,
                    child: id,
                    before: None,
                });
                inserted = true;
            }
            let layout = self.nodes[a as usize].layout.expect("attachment layout");
            let dirty = self.layout.dirty(layout).unwrap_or(true);
            if dirty {
                self.layout_root(
                    layout,
                    taffy::Size {
                        width: AvailableSpace::MaxContent,
                        height: AvailableSpace::MaxContent,
                    },
                );
            }
            if created && (dirty || new) {
                // Its own frame follows the text; its subtree's now.
                let children = std::mem::take(&mut self.nodes[a as usize].children);
                for &child in &children {
                    self.emit_frames(child);
                }
                self.nodes[a as usize].children = children;
            }
        }
        // Views that left the block and were not moved natively elsewhere.
        for id in old {
            let Ok(k) = self.key(id) else { continue };
            let n = &self.nodes[k as usize];
            let moved = n.parent != NO_KEY && !n.inline && self.nodes[n.parent as usize].created;
            if n.created && !moved && self.attachment_block(k).is_none() {
                self.mount.push(MountItem::Remove { id });
            }
        }
        let ids: Vec<NodeId> = keys.iter().map(|&a| self.nodes[a as usize].id).collect();
        let has = !ids.is_empty();
        self.nodes[key as usize].attached = if created { ids } else { Vec::new() };
        match (has, self.attachment_blocks.contains(&key)) {
            (true, false) => self.attachment_blocks.push(key),
            (false, true) => self.attachment_blocks.retain(|&k| k != key),
            _ => {}
        }
        inserted
    }

    /// Text blocks whose attachments' content changed are built again.
    fn check_attachments(&mut self) {
        let mut i = 0;
        while i < self.attachment_blocks.len() {
            let key = self.attachment_blocks[i];
            i += 1;
            let Some(node) = self.nodes.get(key as usize) else {
                continue;
            };
            let dirty = node.attached.iter().any(|&id| {
                self.key(id).is_ok_and(|k| {
                    self.nodes[k as usize]
                        .layout
                        .is_some_and(|l| self.layout.dirty(l).unwrap_or(true))
                })
            });
            if dirty {
                self.mark(key, dirty::TEXT_BUILD);
            }
        }
    }

    /// Lays out the main tree if it changed. Returns true if it did.
    fn layout_main(&mut self) -> bool {
        let root = self.nodes[self.root as usize].layout.expect("root layout");
        let available = taffy::Size {
            width: AvailableSpace::Definite(self.viewport.width.0),
            height: AvailableSpace::Definite(self.viewport.height.0),
        };
        // Even a clean root costs a full rounding walk in taffy: skip it.
        let main = self.layout.dirty(root).unwrap_or(true);
        if main {
            self.layout_root(root, available);
        }
        main
    }

    /// Lays out screens, modals and header items whose size or content
    /// changed, outer ones first (a stack may live in a modal), and emits
    /// their frames.
    fn layout_roots(&mut self) {
        if self.roots.is_empty() {
            return;
        }
        let mut order: Vec<(u32, Key)> = self
            .roots
            .iter()
            .filter(|&&k| {
                let n = &self.nodes[k as usize];
                n.created && n.parent != NO_KEY
            })
            .map(|&k| (self.root_depth(k), k))
            .collect();
        order.sort_unstable();
        for (_, key) in order {
            self.layout_detached(key);
        }
    }

    /// Input accessory `key` was attached to the tree (or detached): the
    /// inputs that name it are configured again.
    fn accessory_attached(&mut self, key: Key, attached: bool) {
        let node = &self.nodes[key as usize];
        let Some(name) = node.root.as_ref().and_then(|r| r.native_id.clone()) else {
            return;
        };
        let id = node.id;
        if attached {
            self.accessories.insert(name.clone(), id);
        } else if self.accessories.get(&name) == Some(&id) {
            self.accessories.remove(&name);
        } else {
            return;
        }
        let inputs: Vec<Key> = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                n.input
                    .as_ref()
                    .is_some_and(|i| i.config.accessory_id.as_deref() == Some(&*name))
            })
            .map(|(k, _)| k as Key)
            .collect();
        for k in inputs {
            self.mark(k, dirty::INPUT);
        }
    }

    /// Number of screens / modals / header items above `key`.
    fn root_depth(&self, mut key: Key) -> u32 {
        let mut depth = 0;
        key = self.nodes[key as usize].parent;
        while key != NO_KEY {
            let n = &self.nodes[key as usize];
            if n.kind.is_root() {
                depth += 1;
            }
            key = n.parent;
        }
        depth
    }

    /// How far the system UI at the top (status bar) reaches into `key`:
    /// its root's top inset minus its offset in that root.
    fn top_overlap(&self, key: Key) -> f32 {
        let mut y = 0.0;
        let mut k = key;
        loop {
            let n = &self.nodes[k as usize];
            if k == self.root {
                return (self.safe_area.top.0 - y).max(0.0);
            }
            if n.kind.is_root() {
                let inset = match n.kind {
                    Kind::Modal => n.root.as_ref().map_or(0.0, |r| r.insets.top.0),
                    _ => 0.0,
                };
                return (inset - y).max(0.0);
            }
            if !n.frame.origin.y.0.is_nan() {
                y += n.frame.origin.y.0;
            }
            if n.parent == NO_KEY {
                return 0.0;
            }
            k = n.parent;
        }
    }

    /// The platform draws a header of its own above screens (iOS,
    /// Android); without one ([`crate::NO_NATIVE_HEADER`]) a screen's
    /// `header` slot is placed at its top.
    fn native_header(&self) -> bool {
        self.header_heights != crate::NO_NATIVE_HEADER
    }

    /// The size of screen `key`: its stack's (`None` outside a stack).
    fn screen_size(&self, key: Key) -> Option<Size<Points>> {
        let parent = self.nodes[key as usize].parent;
        let p = self.nodes.get(parent as usize)?;
        let stack_layout = p.layout.filter(|_| p.kind == Kind::Stack)?;
        let s = self.layout.layout(stack_layout).ok()?.size;
        Some(size_pt(s.width, s.height))
    }

    /// The `header` slot item of screen `key`, once created.
    fn header_slot(&self, key: Key) -> Option<Key> {
        self.nodes[key as usize]
            .children
            .iter()
            .copied()
            .find(|&c| {
                let n = &self.nodes[c as usize];
                n.kind == Kind::HeaderItem
                    && n.created
                    && n.root
                        .as_ref()
                        .is_some_and(|r| r.slot == HeaderSlot::Header)
            })
    }

    /// Without a native header: the height of screen `key`'s `header` slot,
    /// laid out first (as wide as the screen), 0 without one.
    fn drawn_header(&mut self, key: Key) -> f32 {
        let Some(item) = self.header_slot(key) else {
            return 0.0;
        };
        self.layout_detached(item);
        self.nodes[item as usize]
            .layout
            .and_then(|l| self.layout.layout(l).ok())
            .map_or(0.0, |l| l.size.height)
    }

    fn layout_detached(&mut self, key: Key) {
        let native_header = self.native_header();
        let drawn_header = if !native_header && self.nodes[key as usize].kind == Kind::Screen {
            self.drawn_header(key)
        } else {
            0.0
        };
        let node = &self.nodes[key as usize];
        let (Some(layout), kind, parent) = (node.layout, node.kind, node.parent) else {
            return;
        };
        let root = node.root.as_ref().expect("root state");
        let (size, header, available) = match kind {
            Kind::Screen => {
                let Some(size) = self.screen_size(key) else {
                    return;
                };
                let c = &root.screen;
                let measured = root
                    .header
                    .filter(|_| !c.large_title && !c.header_transparent);
                let header = if !native_header {
                    drawn_header
                } else if !c.header_shown {
                    0.0
                } else if let Some(measured) = measured {
                    measured
                } else {
                    let bar = if c.large_title {
                        self.header_heights.1
                    } else {
                        self.header_heights.0
                    };
                    bar.0 + self.top_overlap(parent)
                };
                (size, header, definite(size))
            }
            // A screen's `header` without a native header: as wide as the
            // screen, as tall as its content.
            Kind::HeaderItem if !native_header && root.slot == HeaderSlot::Header => {
                let Some(screen) = self.screen_size(parent) else {
                    return;
                };
                (
                    size_pt(screen.width.0, 0.0),
                    0.0,
                    taffy::Size {
                        width: AvailableSpace::Definite(screen.width.0),
                        height: AvailableSpace::MaxContent,
                    },
                )
            }
            Kind::Modal => {
                let size = root.reported.unwrap_or(self.viewport);
                (size, 0.0, definite(size))
            }
            // As wide as the window, as tall as its content.
            Kind::InputAccessory => (
                size_pt(self.viewport.width.0, 0.0),
                0.0,
                taffy::Size {
                    width: AvailableSpace::Definite(self.viewport.width.0),
                    height: AvailableSpace::MaxContent,
                },
            ),
            _ => (
                Size::default(),
                0.0,
                taffy::Size {
                    width: AvailableSpace::MaxContent,
                    height: AvailableSpace::MaxContent,
                },
            ),
        };
        let changed = root.laid_out != Some((size, header));
        if !changed && !self.layout.dirty(layout).unwrap_or(true) {
            return;
        }
        if kind == Kind::InputAccessory || (kind == Kind::HeaderItem && size.width > Points::ZERO) {
            let mut style = self.layout.style(layout).expect("taffy style").clone();
            style.size.width = taffy::Dimension::length(size.width.0);
            self.layout.set_style(layout, style).expect("taffy style");
        } else if kind != Kind::HeaderItem {
            let config_padding = match kind {
                // Large titles and transparent headers lay the content out
                // under the native header (scroll views inset themselves
                // natively).
                Kind::Screen
                    if native_header
                        && (root.screen.header_transparent || root.screen.large_title) =>
                {
                    0.0
                }
                _ => header,
            };
            let mut style = self.nodes[key as usize]
                .base_layout
                .as_deref()
                .cloned()
                .unwrap_or_default();
            style.size = taffy::Size {
                width: taffy::Dimension::length(size.width.0),
                height: taffy::Dimension::length(size.height.0),
            };
            if config_padding > 0.0 {
                let base = match style.padding.top.into_raw() {
                    raw if raw.tag() == taffy::CompactLength::LENGTH_TAG => raw.value(),
                    _ => 0.0,
                };
                style.padding.top = taffy::LengthPercentage::length(base + config_padding);
            }
            self.layout.set_style(layout, style).expect("taffy style");
        }
        let header_changed = root.laid_out.is_none_or(|(_, h)| h != header);
        self.nodes[key as usize]
            .root
            .as_mut()
            .expect("root state")
            .laid_out = Some((size, header));
        self.layout_root(layout, available);
        self.emit_frames(key);
        if kind == Kind::Screen && header_changed {
            let id = self.nodes[key as usize].id;
            if let Some(config) = self.screen_config(key) {
                self.mount.push(MountItem::ScreenConfig {
                    id,
                    config: Box::new(config),
                });
            }
        }
    }

    /// The options of screen `key`, with its header items.
    fn screen_config(&self, key: Key) -> Option<ScreenConfig> {
        let node = &self.nodes[key as usize];
        let root = node.root.as_ref().filter(|_| node.kind == Kind::Screen)?;
        let mut config = root.screen.clone();
        config.header_height = Points(root.laid_out.map_or(0.0, |(_, h)| h));
        for &c in &node.children {
            let child = &self.nodes[c as usize];
            if child.kind != Kind::HeaderItem || !child.created {
                continue;
            }
            let slot = child.root.as_ref().map_or(HeaderSlot::Left, |r| r.slot);
            let target = match slot {
                HeaderSlot::Left => &mut config.left,
                HeaderSlot::Right => &mut config.right,
                HeaderSlot::Title => &mut config.title_view,
                HeaderSlot::Header => &mut config.header_view,
            };
            *target = Some(child.id);
        }
        if let Some(style) = self.part_resolved(key, classes::Part::Header) {
            let background = style.view.background_color;
            config.background_color = (background.0 & 0xff != 0).then_some(background);
            config.title_color = style.text.color;
            config.tint_color = style.view.tint_color;
        }
        Some(config)
    }

    /// Lays out the list cells that changed: items at their column's
    /// width, the header, footer and empty placeholder at the content's.
    fn layout_cells(&mut self) {
        for i in 0..self.lists.len() {
            let key = self.lists[i];
            let list = &self.nodes[key as usize];
            let Some(list_layout) = list.layout else {
                continue;
            };
            let state = list.list.as_ref().expect("list");
            let horizontal = state.horizontal;
            let size = self.layout.layout(list_layout).expect("taffy layout").size;
            let cross = if horizontal { size.height } else { size.width };
            let item_cross = state.column(cross, 0).1;
            let extra_cross = state.full_width(cross).1;
            let available = |c: f32| {
                if horizontal {
                    taffy::Size {
                        width: AvailableSpace::MaxContent,
                        height: AvailableSpace::Definite(c),
                    }
                } else {
                    taffy::Size {
                        width: AvailableSpace::Definite(c),
                        height: AvailableSpace::MaxContent,
                    }
                }
            };
            // Laying out even a clean root costs a tree walk (taffy rounds the
            // whole subtree), so only dirty cells are laid out again — all
            // of them when the list's cross size or columns changed.
            let list_state = self.nodes[key as usize].list.as_mut().expect("list");
            let resized = list_state.cells_cross != cross;
            list_state.cells_cross = cross;
            let cells = std::mem::take(&mut self.nodes[key as usize].children);
            for &cell in &cells {
                let node = &self.nodes[cell as usize];
                let Some(cell_layout) = node.layout else {
                    continue;
                };
                let c = match node.slot {
                    Some(CellSlot::Item(_)) | None => item_cross,
                    Some(_) => extra_cross,
                };
                if resized || self.layout.dirty(cell_layout).unwrap_or(true) {
                    self.layout_root(cell_layout, available(c));
                    self.nodes[cell as usize].relaid = true;
                }
            }
            self.nodes[key as usize].children = cells;
        }
    }

    /// Lays out the tree under `root` within `available`.
    fn layout_root(&mut self, root: LayoutId, available: taffy::Size<AvailableSpace>) {
        let engine = &self.text_engine;
        let cache = &mut self.measure_cache;
        self.layout
            .compute_layout_with_measure(root, available, |input, _, cx, style| {
                // Taffy resolves style sizes, padding, border, min/max and
                // aspect ratio; only content is measured here.
                taffy::compute_leaf_layout(
                    input,
                    style,
                    |_, _| 0.0,
                    |known, available| match cx {
                        Some(LeafContext::Text(cx)) => {
                            let max_width = if cx.text.no_wrap {
                                None
                            } else {
                                known.width.or(match available.width {
                                    AvailableSpace::Definite(w) => Some(w),
                                    // Text has no cheap min-content; use the natural width.
                                    AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
                                })
                            };
                            let measured = measure_text(engine, cache, cx, max_width);
                            taffy::Size {
                                width: known.width.unwrap_or(measured.width.0),
                                height: known.height.unwrap_or(measured.height.0),
                            }
                        }
                        Some(LeafContext::Image(Some(natural))) => {
                            image_size(*natural, known.width, known.height)
                        }
                        Some(LeafContext::Native(Some(own))) => taffy::Size {
                            width: known.width.unwrap_or(own.width.0),
                            height: known.height.unwrap_or(own.height.0),
                        },
                        _ => taffy::Size::ZERO,
                    },
                )
            })
            .expect("taffy layout");
    }

    #[inline]
    fn queue_text(&mut self, key: Key) {
        let node = &mut self.nodes[key as usize];
        if !node.text_queued {
            node.text_queued = true;
            self.pending_text.push(key);
        }
    }

    /// Makes the final layout of every queued text block whose content or
    /// box changed, and emits it.
    fn flush_text(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_text);
        // Attachment views to place: (block, its final layout, content box).
        let mut placements: Vec<(Key, TextLayout, Bounds<Points>)> = Vec::new();
        for &key in &pending {
            let Some(node) = self.nodes.get_mut(key as usize) else {
                continue;
            };
            node.text_queued = false;
            let align_vertical = node.text_style.align_vertical.unwrap_or_default();
            let report_lines = node.events.contains(EventKind::TextLayout);
            // Shrinking stops at a tenth: smaller is unreadable anyway.
            let fit = node
                .paragraph
                .fit
                .then_some(node.paragraph.min_font_scale.max(0.1));
            let (id, created, Some(layout)) = (node.id, node.created, node.layout) else {
                continue;
            };
            if !created {
                continue;
            }
            let mut content = content_box(self.layout.layout(layout).expect("taffy layout"));
            let engine = &self.text_engine;
            let cache = &mut self.measure_cache;
            let Some(LeafContext::Text(cx)) = self.layout.get_node_context_mut(layout) else {
                continue;
            };
            if cx.text.no_wrap {
                // One line at its natural width, past the box if need be.
                content.size.width = content
                    .size
                    .width
                    .max(measure_text(engine, cache, cx, None).width);
            }
            if align_vertical > Percentage(0.0) {
                // Text shorter than its box: moved down within it.
                let height = measure_text(engine, cache, cx, Some(content.size.width.0)).height;
                if height < content.size.height {
                    content.origin.y += align_vertical.of(content.size.height - height);
                    content.size.height = height;
                }
            }
            let size = content.size;
            if let Some(min) = fit {
                // Shrunk to fit: its own text, laid out on its own.
                if matches!(&cx.laid_out, Some((r, _)) if *r == content) {
                    continue;
                }
                let text = Arc::new(fitted(engine, &cx.text, size, min));
                let prepared = engine.prepare(&text);
                let text_layout = engine.layout(&prepared, size);
                cx.laid_out = Some((content, text_layout.clone()));
                if report_lines {
                    self.events.push(crate::ui::JsEvent::Event(
                        id,
                        EventKind::TextLayout,
                        text_lines(engine, &text, &text_layout),
                    ));
                }
                if !text.attachments.is_empty() {
                    placements.push((key, text_layout.clone(), content));
                }
                self.mount.push(MountItem::Text {
                    id,
                    text,
                    layout: text_layout,
                    content,
                });
                continue;
            }
            let text_layout = match &cx.laid_out {
                Some((r, _)) if *r == content => continue,
                // Only moved (padding, vertical alignment): the same layout.
                Some((r, l)) if r.size == size => l.clone(),
                _ => {
                    let prepared = cx.prepared.get_or_insert_with(|| engine.prepare(&cx.text));
                    let layout = engine.layout(prepared, size);
                    if report_lines {
                        self.events.push(crate::ui::JsEvent::Event(
                            id,
                            EventKind::TextLayout,
                            text_lines(engine, &cx.text, &layout),
                        ));
                    }
                    layout
                }
            };
            cx.laid_out = Some((content, text_layout.clone()));
            if !cx.text.attachments.is_empty() {
                placements.push((key, text_layout.clone(), content));
            }
            self.mount.push(MountItem::Text {
                id,
                text: cx.text.clone(),
                layout: text_layout,
                content,
            });
        }
        pending.clear();
        self.pending_text = pending;
        for (key, layout, content) in placements {
            self.place_attachments(key, &layout, content);
        }
    }

    /// Moves the views of text block `key`'s attachments onto their
    /// placeholders in `layout` (content box `content`); cut off ones out
    /// of sight.
    fn place_attachments(&mut self, key: Key, layout: &TextLayout, content: Bounds<Points>) {
        let points = self.text_engine.attachments(layout);
        let attached = std::mem::take(&mut self.nodes[key as usize].attached);
        for (i, &id) in attached.iter().enumerate() {
            let Ok(a) = self.key(id) else { continue };
            let Some(l) = self.nodes[a as usize].layout else {
                continue;
            };
            let size = self.layout.layout(l).expect("taffy layout").size;
            let (x, y) = match points.get(i).copied().flatten() {
                Some(p) => (content.origin.x.0 + p.x.0, content.origin.y.0 + p.y.0),
                // Far away, like unbound list cells.
                None => (-100_000.0, 0.0),
            };
            self.set_frame(a, rect(x, y, size.width, size.height));
        }
        self.nodes[key as usize].attached = attached;
    }

    /// Emits the frame of `key` and its subtree (if changed).
    fn emit_frames(&mut self, key: Key) {
        let node = &self.nodes[key as usize];
        let Some(layout) = node.layout else { return };
        if !node.created {
            return;
        }
        let l = self.layout.layout(layout).expect("taffy layout");
        let mut frame = rect(l.location.x, l.location.y, l.size.width, l.size.height);
        // Children of a right-to-left horizontal scroll view: mirrored in
        // its content.
        if let Some(parent) = self.nodes.get(node.parent as usize)
            && parent.rtl
            && let Some(ScrollState {
                horizontal: true,
                content: Some(content),
                ..
            }) = parent.scroll.as_deref()
            && parent.kind == Kind::Scroll
        {
            frame.origin.x = content.width - frame.origin.x - frame.size.width;
        }
        self.set_frame(key, frame);
        let kind = self.nodes[key as usize].kind;
        if kind == Kind::Input {
            let content = content_box(self.layout.layout(layout).expect("taffy layout"));
            let node = &mut self.nodes[key as usize];
            let input = node.input.as_mut().expect("input state");
            if input.content != Some(content) {
                input.content = Some(content);
                self.mount.push(MountItem::InputContent {
                    id: node.id,
                    content,
                });
            }
            if node.events.contains(EventKind::ContentSizeChange) {
                self.report_content_size(key, content.size.width.0);
            }
        }
        if kind == Kind::Scroll {
            // Scrollable extent: the overflow rectangle is measured from the
            // padding box origin and stops at the children's border boxes, so
            // add the leading border and trailing padding.
            let l = self.layout.layout(layout).expect("taffy layout");
            let o = l.scrollable_overflow_rect;
            let size = size_pt(
                l.size.width.max(l.border.left + o.right + l.padding.right),
                l.size
                    .height
                    .max(l.border.top + o.bottom + l.padding.bottom),
            );
            self.set_scroll_content(key, size);
        }
        // List cells are positioned by `update_lists`, roots by
        // `layout_roots`.
        if kind.is_container() && kind != Kind::List {
            let children = std::mem::take(&mut self.nodes[key as usize].children);
            for &child in &children {
                if !self.nodes[child as usize].kind.is_root() {
                    self.emit_frames(child);
                }
            }
            self.nodes[key as usize].children = children;
            self.update_overflow(key);
        }
    }

    fn set_frame(&mut self, key: Key, frame: Bounds<Points>) {
        let node = &mut self.nodes[key as usize];
        if node.frame != frame {
            node.frame = frame;
            let (id, is_text) = (node.id, node.kind == Kind::Text);
            if node.events.contains(EventKind::Layout) {
                self.events.push(crate::ui::JsEvent::Event(
                    id,
                    EventKind::Layout,
                    EventPayload::Layout(frame),
                ));
            }
            self.mount.push(MountItem::Frame { id, frame });
            let node = &self.nodes[key as usize];
            // Translations by a fraction of the size follow it.
            if node.view_style.transform.has_fractions() {
                let style = self.mounted_view_style(key);
                self.mount.push(MountItem::ViewStyle { id, style });
            }
            if is_text {
                self.queue_text(key);
            }
        }
    }

    fn set_scroll_content(&mut self, key: Key, size: Size<Points>) {
        let node = &mut self.nodes[key as usize];
        let (id, rtl, kind, viewport) = (node.id, node.rtl, node.kind, node.frame.size.width.0);
        if let Some(scroll) = node.scroll.as_mut()
            && scroll.content != Some(size)
        {
            let old = scroll.content.replace(size);
            // Items are only inserted before it since (a `ScrollBy`, the
            // layout animation), so it is at its index or later.
            let batch = self.batches;
            let queued = scroll
                .content_item
                .filter(|&(b, _)| b == batch)
                .and_then(|(_, i)| {
                    let at = self.mount.get(i..)?.iter().position(
                        |m| matches!(m, MountItem::ScrollContent { id: queued, .. } if *queued == id),
                    )?;
                    Some(i + at)
                });
            if let Some(j) = queued {
                self.mount[j] = MountItem::ScrollContent { id, size };
            }
            // A scroll queued after it must still come before the new size.
            let scrolled_since = queued.is_some_and(|j| {
                self.mount[j + 1..]
                    .iter()
                    .any(|m| matches!(m, MountItem::ScrollBy { id: s, .. } if *s == id))
            });
            if queued.is_none() || scrolled_since {
                scroll.content_item = Some((batch, self.mount.len()));
                self.mount.push(MountItem::ScrollContent { id, size });
            }
            // Right to left, a horizontal scroll view starts at its right
            // end and keeps its distance from it as the content changes.
            if kind == Kind::Scroll && scroll.horizontal && rtl {
                match old {
                    None => self.commands.push((
                        id,
                        Command::ScrollTo {
                            x: (size.width - Points(viewport)).max(Points::ZERO),
                            y: Points::ZERO,
                            animated: false,
                        },
                    )),
                    Some(old) if old.width != size.width => self.mount.push(MountItem::ScrollBy {
                        id,
                        dx: size.width - old.width,
                        dy: Points::ZERO,
                    }),
                    Some(_) => {}
                }
            }
        }
    }

    /// Measures rendered cells, keeps the visible content anchored, picks
    /// the items to render and positions the cells of every list.
    fn update_lists(&mut self, relayout: bool) {
        for i in 0..self.lists.len() {
            let key = self.lists[i];
            let node = &self.nodes[key as usize];
            if !node.created {
                continue;
            }
            let list = node.list.as_ref().expect("list");
            if !(relayout || list.dirty || self.commands.iter().any(|c| c.0 == node.id)) {
                continue;
            }
            let (id, events) = (node.id, node.events);
            let horizontal = list.horizontal;
            let l = *self
                .layout
                .layout(node.layout.expect("list layout"))
                .expect("taffy layout");
            let (main, cross) = if horizontal {
                (l.size.width, l.size.height)
            } else {
                (l.size.height, l.size.width)
            };
            let main_of = |s: taffy::Size<f32>| if horizontal { s.width } else { s.height };

            // Sizes of cells showing their item, and of the extras.
            let mut ready = std::mem::take(&mut self.cells_scratch);
            ready.clear();
            ready.extend(
                list.slots
                    .iter()
                    .filter_map(|s| s.ready().map(|i| (i, s.cell))),
            );
            let extras = list.extras;
            let mut anchor = 0.0;
            for &(index, cell) in &ready {
                if let Some(cell_layout) = self.nodes.get(cell as usize).and_then(|c| c.layout) {
                    let size = main_of(self.layout.layout(cell_layout).expect("taffy layout").size);
                    let list = self.nodes[key as usize].list.as_mut().expect("list");
                    anchor += list.measured(index, size);
                }
            }
            for (kind, extra) in [ExtraKind::Header, ExtraKind::Footer, ExtraKind::Empty]
                .into_iter()
                .zip(extras)
            {
                if let Some(cell_layout) =
                    self.nodes.get(extra.cell as usize).and_then(|c| c.layout)
                {
                    let size = main_of(self.layout.layout(cell_layout).expect("taffy layout").size);
                    let list = self.nodes[key as usize].list.as_mut().expect("list");
                    anchor += list.measured_extra(kind, size);
                }
            }

            let list = self.nodes[key as usize].list.as_mut().expect("list");
            // Not on the first layout: there is nothing to clamp yet.
            let grew = list.viewport > 0.0 && main > list.viewport;
            list.viewport = main;
            list.offset += anchor;
            // A splice above the viewport already moved the offset.
            anchor += list.take_splice_anchor();
            // Inverted lists keep their distance from the end instead;
            // `maintainVisibleContentPosition` follows the end near it.
            anchor += list.anchor_end();
            // The last scroll command's target moves as its cells are
            // measured: the list follows it.
            let mut retarget = None;
            match list.follow_pin(anchor) {
                Follow::Stay => {}
                Follow::By(delta) => anchor += delta,
                Follow::To(offset) => retarget = Some(offset),
            }

            // Scroll commands for the list, now that sizes are known: the
            // window follows a jump in this same commit.
            for (target, command) in &mut self.commands {
                if *target != id {
                    continue;
                }
                let list = self.nodes[key as usize].list.as_mut().expect("list");
                let (pin, animated) = match *command {
                    Command::ScrollToIndex {
                        index,
                        animated,
                        view_position,
                        view_offset,
                    } => (
                        Some(PinTarget::Index {
                            index,
                            view_position: view_position.0,
                            view_offset: view_offset.0,
                        }),
                        animated,
                    ),
                    Command::ScrollToEnd { animated } => (Some(PinTarget::End), animated),
                    Command::ScrollTo { animated, .. } => (None, animated),
                    _ => continue,
                };
                let offset = match (pin, &*command) {
                    (Some(pin), _) => list.target_offset(pin),
                    (None, &Command::ScrollTo { x, y, .. }) => {
                        if horizontal {
                            x.0
                        } else {
                            y.0
                        }
                    }
                    _ => continue,
                };
                // An offset of the app's own ends the pin.
                list.pin(pin, offset, animated);
                // A new command replaces the pin's retargeting.
                retarget = None;
                if !animated {
                    list.jump(offset);
                }
                list.issue_scroll_to(offset);
                let (x, y) = if horizontal {
                    (Points(offset), Points::ZERO)
                } else {
                    (Points::ZERO, Points(offset))
                };
                *command = Command::ScrollTo { x, y, animated };
            }
            if let Some(offset) = retarget {
                self.nodes[key as usize]
                    .list
                    .as_mut()
                    .expect("list")
                    .issue_scroll_to(offset);
                let (x, y) = if horizontal {
                    (Points(offset), Points::ZERO)
                } else {
                    (Points::ZERO, Points(offset))
                };
                let command = Command::ScrollTo {
                    x,
                    y,
                    animated: true,
                };
                self.commands.push((id, command));
            }

            let list = self.nodes[key as usize].list.as_mut().expect("list");
            let mut binds = Vec::new();
            if list.update_window(&mut binds) {
                self.events.push(crate::ui::JsEvent::Event(
                    id,
                    EventKind::EndReached,
                    EventPayload::None,
                ));
            }
            if !binds.is_empty() {
                self.list_binds.push(ListBind { list: id, binds });
            }
            if events.contains(EventKind::ViewableItemsChanged) {
                let viewable = list.viewable_items();
                if viewable != list.viewable {
                    list.viewable = viewable;
                    let (first, last) =
                        viewable.map_or((-1.0, -1.0), |(f, l)| (f as f64, l as f64));
                    self.events.push(crate::ui::JsEvent::Event(
                        id,
                        EventKind::ViewableItemsChanged,
                        EventPayload::Map(vec![
                            ("first", PayloadValue::Number(first)),
                            ("last", PayloadValue::Number(last)),
                        ]),
                    ));
                }
            }

            // Position cells that show their item (the window may have
            // changed which ones). Descendant frames only change when the
            // cell was laid out again.
            let list = self.nodes[key as usize].list.as_ref().expect("list");
            let content_len = list.content_len();
            let blocks = list.blocks();
            let sticky = list.sticky_enabled();
            ready.clear();
            ready.extend(
                list.slots
                    .iter()
                    .filter_map(|s| s.ready().map(|i| (i, s.cell))),
            );
            // Cells left without an item still show their last one. Those
            // in the render window (the data shrank, the list jumped) would
            // show through: move them out of sight. Cells the window left
            // behind while scrolling are out of sight already.
            let (window_start, window_end) = list.render_window();
            let hidden: Vec<Key> = list
                .slots
                .iter()
                .filter(|s| s.index.is_none() && s.cell != u32::MAX)
                .map(|s| s.cell)
                .collect();
            for cell in hidden {
                let frame = self.nodes[cell as usize].frame;
                if frame.size.width.0.is_nan() {
                    // Never placed: nothing to see yet.
                    continue;
                }
                let (start, size) = if horizontal {
                    (frame.origin.x.0, frame.size.width.0)
                } else {
                    (frame.origin.y.0, frame.size.height.0)
                };
                if start + size <= window_start || start >= window_end {
                    continue;
                }
                let away = if horizontal {
                    Bounds {
                        origin: Point::new(Points(-HIDDEN), frame.origin.y),
                        ..frame
                    }
                } else {
                    Bounds {
                        origin: Point::new(frame.origin.x, Points(-HIDDEN)),
                        ..frame
                    }
                };
                if frame != away {
                    self.set_frame(cell, away);
                }
            }
            let mut sticky_cells = Vec::new();
            let place = |this: &mut Self, cell: Key, at: f32, row: f32, (x, width): (f32, f32)| {
                let cell_layout = this.nodes.get(cell as usize).and_then(|c| c.layout)?;
                let size = this.layout.layout(cell_layout).expect("taffy layout").size;
                let list = this.nodes[key as usize].list.as_ref().expect("list");
                let main_at = list.to_content(at, row.max(main_of(size)));
                let frame = if horizontal {
                    rect(main_at, x, size.width, width)
                } else {
                    rect(x, main_at, width, size.height)
                };
                this.set_frame(cell, frame);
                if std::mem::take(&mut this.nodes[cell as usize].relaid) {
                    let children = std::mem::take(&mut this.nodes[cell as usize].children);
                    for &child in &children {
                        this.emit_frames(child);
                    }
                    this.nodes[cell as usize].children = children;
                    this.update_overflow(cell);
                }
                Some(main_of(size))
            };
            for &(index, cell) in &ready {
                let list = self.nodes[key as usize].list.as_ref().expect("list");
                let (at, row) = (list.item_at(index), list.row_extent(index));
                let column = list.column(cross, list.column_of(index));
                let Some(size) = place(self, cell, at, row, column) else {
                    continue;
                };
                let list = self.nodes[key as usize].list.as_ref().expect("list");
                if sticky && list.sticky.binary_search(&index).is_ok() {
                    let range = list.sticky_range(index, self.nodes[cell as usize].id, size);
                    sticky_cells.push(range);
                }
            }
            let full = self.nodes[key as usize]
                .list
                .as_ref()
                .expect("list")
                .full_width(cross);
            for (extra, at) in extras
                .iter()
                .zip([blocks.header, blocks.footer, blocks.body])
            {
                if extra.cell != u32::MAX {
                    place(self, extra.cell, at, 0.0, full);
                }
            }
            self.cells_scratch = ready;

            sticky_cells.sort_by_key(|c| c.cell);
            let list = self.nodes[key as usize].list.as_mut().expect("list");
            if sticky_cells != list.sent_sticky {
                let cells = sticky_cells
                    .iter()
                    .map(|r| StickyCell {
                        cell: r.cell,
                        start: Points(r.start),
                        end: Points(r.end),
                    })
                    .collect();
                list.sent_sticky = sticky_cells;
                self.mount.push(MountItem::StickyCells { id, cells });
            }

            let content = if horizontal {
                size_pt(content_len, cross)
            } else {
                size_pt(cross, content_len)
            };
            // Platforms clamp the offset to the content: grow the content
            // before scrolling, scroll before shrinking it.
            let old_len = self.nodes[key as usize]
                .scroll
                .as_ref()
                .and_then(|s| s.content)
                .map_or(0.0, |c| if horizontal { c.width.0 } else { c.height.0 });
            if anchor != 0.0 {
                self.nodes[key as usize]
                    .list
                    .as_mut()
                    .expect("list")
                    .issue_scroll_by(anchor);
            }
            let scroll_by = (anchor != 0.0).then(|| {
                let (dx, dy) = if horizontal {
                    (Points(anchor), Points::ZERO)
                } else {
                    (Points::ZERO, Points(anchor))
                };
                MountItem::ScrollBy { id, dx, dy }
            });
            // Platforms clamp the offset to the content and the viewport:
            // scroll before the viewport grows or the content shrinks, and
            // after the content grows (a shrinking viewport's frame comes
            // first anyway).
            let frame_at = grew
                .then(|| {
                    self.mount
                        .iter()
                        .position(|m| matches!(m, MountItem::Frame { id: f, .. } if *f == id))
                })
                .flatten();
            match (scroll_by, frame_at) {
                (Some(item), Some(at)) => {
                    self.mount.insert(at, item);
                    self.set_scroll_content(key, content);
                }
                (Some(item), None) if content_len < old_len => {
                    self.mount.push(item);
                    self.set_scroll_content(key, content);
                }
                (item, _) => {
                    self.set_scroll_content(key, content);
                    self.mount.extend(item);
                }
            }
        }
    }
}

/// How far out of sight unbound list cells are moved.
const HIDDEN: f32 = 1.0e6;

/// The content box (inside padding and border) of a laid-out node.
fn content_box(l: &taffy::Layout) -> Bounds<Points> {
    rect(
        l.padding.left + l.border.left,
        l.padding.top + l.border.top,
        (l.size.width - l.padding.left - l.padding.right - l.border.left - l.border.right).max(0.0),
        (l.size.height - l.padding.top - l.padding.bottom - l.border.top - l.border.bottom)
            .max(0.0),
    )
}

/// Size of an image with a known natural size: fill a known dimension and
/// keep the aspect ratio for the other.
fn image_size(natural: Size<Points>, w: Option<f32>, h: Option<f32>) -> taffy::Size<f32> {
    let ratio = if natural.height > Points::ZERO {
        natural.width / natural.height
    } else {
        1.0
    };
    let (width, height) = match (w, h) {
        (Some(w), None) => (w, w / ratio),
        (None, Some(h)) => (h * ratio, h),
        (Some(w), Some(h)) => (w, h),
        (None, None) => (natural.width.0, natural.height.0),
    };
    taffy::Size { width, height }
}

/// Measures a text block: per-node cache, then the shared content-addressed
/// cache, then the platform engine.
/// The lines of `layout` with their characters.
fn text_lines<M: TextEngine>(
    engine: &M,
    text: &AttributedText,
    layout: &TextLayout,
) -> EventPayload {
    let units: Vec<u16> = text.text.encode_utf16().collect();
    let mut lines = engine.lines(layout);
    for line in &mut lines {
        let (start, end) = (line.start as usize, (line.end as usize).min(units.len()));
        if start < end {
            line.text = String::from_utf16_lossy(&units[start..end]);
        }
    }
    EventPayload::TextLayout(lines)
}

/// `text` with its fonts scaled down (to at least `min`) until it fits
/// `size` within its line limit: the largest scale that fits, by
/// bisection.
fn fitted<M: TextEngine>(
    engine: &M,
    text: &AttributedText,
    size: Size<Points>,
    min: f32,
) -> AttributedText {
    let scaled = |scale: f32| {
        let mut t = text.clone();
        for span in &mut t.spans {
            span.style.font_size *= scale;
            span.style.line_height *= scale;
            span.style.letter_spacing *= scale;
        }
        t
    };
    let fits = |t: &AttributedText| {
        // Untruncated, the text must be no taller than its truncated self
        // (nothing cut) and than the box.
        let all = AttributedText {
            max_lines: 0,
            ..t.clone()
        };
        let full = engine.measure(&engine.prepare(&all), Some(size.width));
        let limited = if t.max_lines > 0 {
            engine.measure(&engine.prepare(t), Some(size.width)).height
        } else {
            Points(f32::INFINITY)
        };
        full.height <= size.height.min(limited) + Points(0.5)
            && full.width <= size.width + Points(0.5)
    };
    if fits(text) {
        return text.clone();
    }
    let (mut lo, mut hi) = (min, 1.0f32);
    if !fits(&scaled(lo)) {
        return scaled(lo);
    }
    for _ in 0..7 {
        let mid = (lo + hi) / 2.0;
        if fits(&scaled(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    scaled(lo)
}

fn measure_text<M: TextEngine>(
    engine: &M,
    cache: &mut MeasureCache,
    cx: &mut TextContext<M::Prepared>,
    max_width: Option<f32>,
) -> Size<Points> {
    if let Some((w, size)) = cx.cache
        && w == max_width
    {
        return size;
    }
    let typed = max_width.map(Points);
    let size = match cache.get(cx.layout_hash, typed) {
        Some(size) => size,
        None => {
            let prepared = cx.prepared.get_or_insert_with(|| engine.prepare(&cx.text));
            let size = engine.measure(prepared, typed);
            // Round up so rendering never truncates the last glyph.
            let size = size.map(Points::ceil);
            cache.insert(cx.layout_hash, typed, size);
            size
        }
    };
    cx.cache = Some((max_width, size));
    size
}

fn definite(size: Size<Points>) -> taffy::Size<AvailableSpace> {
    taffy::Size {
        width: AvailableSpace::Definite(size.width.0),
        height: AvailableSpace::Definite(size.height.0),
    }
}

/// The message for a refused URL.
fn not_granted(what: &str, uri: &str, capability: &str) -> String {
    let host = crate::access::parse_url(uri).map_or_else(|| uri.to_owned(), |u| u.host);
    format!("{what} {uri} is not granted: add `{host}` to capabilities.{capability} in nsh.json")
}

/// Applies one property to a control.
fn apply_native(control: &mut NativeProps, prop: NativeProp) {
    use NativeProp as P;
    match (control, prop) {
        (NativeProps::WebView(p), P::Source(v)) => p.source = v,
        (NativeProps::WebView(p), P::InjectedJavaScript(v)) => p.injected_js = v,
        (NativeProps::WebView(p), P::InjectedJavaScriptBefore(v)) => p.injected_js_before = v,
        (NativeProps::WebView(p), P::UserAgent(v)) => p.user_agent = v,
        (NativeProps::WebView(p), P::ScrollEnabled(v)) => p.scroll_enabled = v,
        (NativeProps::WebView(p), P::JavaScriptEnabled(v)) => p.javascript_enabled = v,
        (NativeProps::WebView(p), P::BackForwardGestures(v)) => p.back_forward_gestures = v,
        (NativeProps::Video(p), P::Source(v)) => {
            p.source = match v {
                crate::WebSource::Uri(uri) => Some(uri),
                _ => None,
            }
        }
        (NativeProps::Video(p), P::Paused(v)) => p.paused = v,
        (NativeProps::Video(p), P::Muted(v)) => p.muted = v,
        (NativeProps::Video(p), P::Volume(v)) => p.volume = v.clamp(0.0, 1.0),
        (NativeProps::Video(p), P::Rate(v)) => p.rate = v.max(0.0),
        (NativeProps::Video(p), P::Repeat(v)) => p.repeat = v,
        (NativeProps::Video(p), P::VideoResizeMode(v)) => p.resize_mode = v,
        (NativeProps::Video(p), P::Controls(v)) => p.controls = v,
        (NativeProps::Video(p), P::ProgressInterval(v)) => p.progress_interval_ms = v.max(16.0),
        _ => {}
    }
}

/// `style` with its font metrics multiplied for the user's text size.
fn scaled(mut style: ResolvedTextStyle, multiplier: f32) -> ResolvedTextStyle {
    if multiplier != 1.0 {
        style.font_size *= multiplier;
        style.line_height *= multiplier;
    }
    style
}

fn root_style(viewport: Size<Points>) -> taffy::Style {
    let mut style = taffy::Style {
        size: taffy::Size {
            width: taffy::Dimension::length(viewport.width.0),
            height: taffy::Dimension::length(viewport.height.0),
        },
        ..Style::default().layout
    };
    crate::style::resolve_normal_alignment(&mut style);
    style
}

/// `align` in a layout of direction `rtl`: left and right trade places
/// right to left, as all left / right properties do.
fn directed_align(align: TextAlign, rtl: bool) -> TextAlign {
    match (align, rtl) {
        (TextAlign::Left, true) => TextAlign::Right,
        (TextAlign::Right, true) => TextAlign::Left,
        (align, _) => align,
    }
}

/// The press events.
const PRESSES: [EventKind; 4] = [
    EventKind::Press,
    EventKind::LongPress,
    EventKind::PressDown,
    EventKind::PressUp,
];

/// The events a `disabled` view does not mount ([`Node::mounted_events`]).
const DISABLED_OFF: [EventKind; 8] = [
    EventKind::Press,
    EventKind::LongPress,
    EventKind::PressDown,
    EventKind::PressUp,
    EventKind::ContextMenu,
    EventKind::FileDragEnter,
    EventKind::FileDragLeave,
    EventKind::FileDrop,
];

/// Whether `disabled` changes what a view with `events` mounts.
fn disabling_changes(events: EventMask) -> bool {
    DISABLED_OFF.iter().any(|&e| events.contains(e))
}

/// The background a nested text's view style gives its characters.
fn span_background(style: &ViewStyle, node: NodeId) -> Option<crate::text::TextBackground> {
    (style.background_color.0 & 0xff != 0).then_some(crate::text::TextBackground {
        color: style.background_color,
        radius: style.border_radius.max(Points::ZERO),
        node,
    })
}

/// Whether a span with `events` takes presses (a link in its text block).
fn pressable(events: EventMask) -> bool {
    PRESSES.iter().any(|&e| events.contains(e))
}

/// What a leaf of `kind` measures with, if anything: texts and inputs (an
/// input measures like a text block holding its text or placeholder) their
/// text, images their image, native components the size they report.
fn leaf_context<P>(kind: Kind) -> Option<LeafContext<P>> {
    match kind {
        Kind::Text | Kind::Input => Some(LeafContext::Text(TextContext::new(Arc::default()))),
        Kind::Image => Some(LeafContext::Image(None)),
        Kind::Native => Some(LeafContext::Native(None)),
        _ => None,
    }
}

//! Main-thread ArkUI nodes. Callbacks hold weak references; the mounter owns
//! the nodes through detachment and frame-delayed retirement.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use nsh_core::{EventKind, EventMask, EventPayload, NodeId, Points, ViewKind, ViewStyle};
use ohos_arkui_binding::{
    api::node_custom_event::NodeCustomEvent,
    common::{attribute::ArkUINodeAttributeNumber as Number, node::ArkUINode},
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::Custom,
    },
    types::{
        advanced::{LengthMetricUnit, NodeDirtyFlag},
        attribute::ArkUINodeAttributeType as Attribute,
    },
};
use ohos_drawing_binding::Canvas;

use crate::{
    input::{editor::Input, events::Interaction},
    render::geometry::{Rect, Size},
    scroll::controller::Scroller,
    text::typography::{NativeText, argb},
};

pub(crate) type NativeNode = Rc<RefCell<ArkUINode>>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("invalid OHOS mount node {0}")]
    InvalidNode(NodeId),
    #[error("{0} at {1}")]
    ArkUi(
        ohos_arkui_binding::common::error::ArkUIError,
        &'static std::panic::Location<'static>,
    ),
    #[cfg(feature = "api-23")]
    #[error(transparent)]
    Accessibility(#[from] ohos_accessibility_binding::AccessibilityError),
    #[error(transparent)]
    Host(#[from] napi_ohos::Error),
    #[error("OHOS native VSync request failed: {0}")]
    Vsync(i32),
    #[error("OHOS backend does not support {0}")]
    Unsupported(&'static str),
}
impl From<ohos_arkui_binding::common::error::ArkUIError> for Error {
    #[track_caller]
    fn from(err: ohos_arkui_binding::common::error::ArkUIError) -> Self {
        Self::ArkUi(err, std::panic::Location::caller())
    }
}
pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy)]
struct AppliedStyle {
    visual_ptr: usize,
    background_color: u32,
    border_radius: [f32; 4],
    border_width: [f32; 4],
    border_color: [u32; 4],
    border_style: i32,
    shadow: Option<(f32, f32, f32, u32)>,
    ink_outset: f32,
    clip: bool,
    visibility: i32,
    transform: Option<[f32; 16]>,
    transform_center: [f32; 3],
}
impl Default for AppliedStyle {
    fn default() -> Self {
        Self {
            visual_ptr: 0,
            background_color: 0,
            border_radius: [0.0; 4],
            border_width: [0.0; 4],
            border_color: [0xff000000; 4],
            border_style: 0,
            shadow: None,
            ink_outset: 0.0,
            clip: false,
            visibility: 0,
            transform: None,
            transform_center: [f32::NAN; 3],
        }
    }
}

pub(crate) struct Node {
    weak: Weak<Node>,
    pub(crate) accessibility_state: crate::accessibility::state::State,
    #[cfg(feature = "api-23")]
    pub(crate) accessibility_provider: crate::accessibility::provider::State,
    pub(crate) native: NativeNode,
    pub(crate) wrapper: RefCell<Option<crate::interop::view::Wrapper>>,
    pub(crate) mount_parent: RefCell<Weak<RefCell<ArkUINode>>>,
    pub(crate) id: NodeId,
    pub(crate) kind: ViewKind,
    pub(crate) pending_delete: Cell<bool>,
    pub(crate) scene_prepared: Cell<bool>,
    pub(crate) modal: RefCell<Option<Rc<crate::navigation::modal::Modal>>>,
    pub(crate) screen: RefCell<Option<Rc<crate::navigation::stack::Screen>>>,
    pub(crate) raw_frame: Cell<Rect>,
    pub(crate) frame: Cell<Rect>,
    pub(crate) parent: Cell<Option<NodeId>>,
    pub(crate) parent_node: RefCell<Weak<Node>>,
    pub(crate) children: RefCell<Vec<Weak<Node>>>,
    context: Rc<crate::render::mount::UiContext>,
    pub(crate) input: RefCell<Option<Rc<Input>>>,
    pub(crate) scroll: RefCell<Option<Rc<Scroller>>>,
    pub(crate) canvas: RefCell<Option<crate::render::canvas::CanvasView>>,
    pub(crate) image: RefCell<Option<Rc<crate::media::image::ImageView>>>,
    pub(crate) component: RefCell<Option<crate::interop::component::Instance>>,
    pub(crate) component_slots: RefCell<Vec<std::sync::Arc<str>>>,
    pub(crate) component_animated: RefCell<Vec<Option<nsh_core::animation::AnimatedPropValue>>>,
    pub(crate) video: RefCell<Option<crate::media::video::Video>>,
    pub(crate) web: RefCell<Option<crate::media::webview::WebView>>,
    active: Cell<bool>,
    backface_away: Cell<bool>,
    pub(crate) interaction: Interaction,
    pub(crate) style: RefCell<ViewStyle>,
    applied: Cell<AppliedStyle>,
    styled: Cell<bool>,
    pub(crate) live: RefCell<Option<Box<crate::render::transition::Live>>>,
    /// Leaving (an exit transition, a popped screen): changed only through
    /// [`Node::set_leaving`], which keeps the surface's count of them.
    leaving: Cell<bool>,
    extra: Cell<(f32, crate::render::transition::Extra)>,
    navigation_presentation: Cell<(f32, f32)>,
    decoration: crate::render::decor::Decoration,
    layout_callbacks: Cell<bool>,
    drawing: Cell<bool>,
    outlined: Cell<bool>,
    shadow_layer: RefCell<Option<Rc<super::shadow::ShadowLayer>>>,
    pub(crate) text_drawing: RefCell<Option<crate::text::typography::TextDrawing>>,
    pub(crate) raster: RefCell<Option<nsh_core::TextLayout>>,
    pub(crate) selection: crate::text::selection::state::State,
    tint: Cell<Option<nsh_core::Color>>,
    pub(crate) content: Cell<Rect>,
    pub(crate) overflow: Cell<Option<Rect>>,
    raster_origin_y: Cell<f32>,
    presentation: Cell<(f32, f32)>,
    pinned: Cell<(f32, f32)>,
    input_transform: Cell<nsh_core::Matrix>,
    applied_layout_size: Cell<(usize, i32, i32)>,
    applied_position: Cell<(usize, i32, i32)>,
    has_text_links: Cell<bool>,
}

impl std::ops::Deref for Node {
    type Target = crate::render::mount::UiContext;
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl Node {
    pub(crate) fn new(
        id: NodeId,
        kind: ViewKind,
        context: Rc<crate::render::mount::UiContext>,
    ) -> Result<Rc<Self>> {
        let native = Custom::new()?.into_node();
        native
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        // Every engine node is a native measure boundary, including the
        // zero-sized containers that position overflowing virtual children.
        native.borrow().set_layout_rect([0, 0, 0, 0])?;
        let visual_ptr = Rc::as_ptr(&native) as usize;
        let node = Rc::new_cyclic(|weak| Self {
            weak: weak.clone(),
            accessibility_state: crate::accessibility::state::State::default(),
            #[cfg(feature = "api-23")]
            accessibility_provider: crate::accessibility::provider::State::default(),
            native,
            wrapper: RefCell::default(),
            mount_parent: RefCell::default(),
            id,
            kind,
            pending_delete: Cell::new(false),
            scene_prepared: Cell::new(false),
            modal: RefCell::new(None),
            screen: RefCell::new(None),
            raw_frame: Cell::new(Rect::default()),
            frame: Cell::new(Rect::default()),
            parent: Cell::new(None),
            parent_node: RefCell::default(),
            children: RefCell::new(Vec::new()),
            context,
            input: RefCell::new(None),
            scroll: RefCell::new(None),
            image: RefCell::new(None),
            canvas: RefCell::new(None),
            component: RefCell::new(None),
            component_slots: RefCell::default(),
            component_animated: RefCell::default(),
            web: RefCell::default(),
            video: RefCell::default(),
            active: Cell::new(true),
            backface_away: Cell::new(false),
            interaction: Interaction::default(),
            style: RefCell::new(ViewStyle::default()),
            applied: Cell::new(AppliedStyle {
                visual_ptr,
                ..AppliedStyle::default()
            }),
            styled: Cell::new(false),
            live: RefCell::default(),
            leaving: Cell::new(false),
            extra: Cell::new((1.0, crate::render::transition::Extra::NONE)),
            navigation_presentation: Cell::new((1.0, 0.0)),
            decoration: crate::render::decor::Decoration::default(),
            layout_callbacks: Cell::new(false),
            drawing: Cell::new(false),
            outlined: Cell::new(false),
            shadow_layer: RefCell::default(),
            text_drawing: RefCell::new(None),
            raster: RefCell::new(None),
            selection: Default::default(),
            tint: Cell::new(None),
            content: Cell::new(Rect::default()),
            overflow: Cell::new(None),
            raster_origin_y: Cell::new(0.0),
            presentation: Cell::new((1.0, 0.0)),
            pinned: Cell::new((0.0, 0.0)),
            input_transform: Cell::new(nsh_core::Matrix::IDENTITY),
            applied_layout_size: Cell::new((visual_ptr, 0, 0)),
            applied_position: Cell::new((0, 0, 0)),
            has_text_links: Cell::new(false),
        });
        match kind {
            ViewKind::Modal => {
                *node.modal.borrow_mut() =
                    Some(crate::navigation::modal::Modal::new(Rc::downgrade(&node))?)
            }
            ViewKind::Image => {
                *node.image.borrow_mut() = Some(crate::media::image::ImageView::new(
                    Rc::downgrade(&node),
                    node.images.clone(),
                )?)
            }
            ViewKind::Input => *node.input.borrow_mut() = Some(Input::new(Rc::downgrade(&node))?),
            ViewKind::Scroll => {
                *node.scroll.borrow_mut() = Some(Scroller::new(Rc::downgrade(&node))?)
            }
            ViewKind::Canvas => {
                *node.canvas.borrow_mut() = Some(crate::render::canvas::CanvasView::new(
                    Rc::downgrade(&node),
                    node.canvas_bytes.clone(),
                ));
            }
            ViewKind::Video => {
                *node.video.borrow_mut() =
                    Some(crate::media::video::Video::new(Rc::downgrade(&node))?);
            }
            ViewKind::WebView => {
                *node.web.borrow_mut() =
                    Some(crate::media::webview::WebView::new(Rc::downgrade(&node))?);
            }
            ViewKind::Screen => {
                let screen = crate::navigation::stack::Screen::new(Rc::downgrade(&node))?;
                screen.install(&node)?;
                *node.screen.borrow_mut() = Some(screen);
            }
            ViewKind::View
            | ViewKind::Text
            | ViewKind::Native
            | ViewKind::Stack
            | ViewKind::HeaderItem
            | ViewKind::InputAccessory => {}
        }
        if !matches!(
            kind,
            ViewKind::View
                | ViewKind::Text
                | ViewKind::Stack
                | ViewKind::HeaderItem
                | ViewKind::InputAccessory
        ) {
            node.enable_layout();
        }
        if node.canvas.borrow().is_some() {
            node.enable_drawing();
        }
        if kind == ViewKind::Modal {
            node.ensure_wrapper()?;
        }
        Ok(node)
    }
    /// Whether it is leaving (an exit transition, a popped screen).
    pub(crate) fn is_leaving(&self) -> bool {
        self.leaving.get()
    }
    /// Marks this node leaving or not, keeping the count of leaving nodes
    /// the mounter skips its ancestor walks by. Returns whether it was.
    pub(crate) fn set_leaving(&self, leaving: bool) -> bool {
        let was = self.leaving.replace(leaving);
        if was != leaving {
            let count = self.leaving_nodes.get();
            debug_assert!(leaving || count > 0, "leaving count underflow");
            self.leaving_nodes.set(if leaving {
                count + 1
            } else {
                count.saturating_sub(1)
            });
        }
        was
    }
    /// LayoutRect gives leaf nodes their native size and placement. Custom
    /// layout is needed only for a node's children or platform-owned content.
    pub(crate) fn enable_layout(&self) {
        if self.layout_callbacks.replace(true) {
            return;
        }
        let weak = self.weak.clone();
        self.native.borrow_mut().on_custom_measure(move |event| {
            callback(|| {
                if let Some(node) = weak.upgrade() {
                    node.measure(event)?;
                }
                Ok(())
            })
        });
        let weak = self.weak.clone();
        self.native.borrow_mut().on_custom_layout(move |event| {
            callback(|| {
                if let Some(node) = weak.upgrade() {
                    node.layout(event)?;
                    node.invalidate_scene();
                }
                Ok(())
            })
        });
    }
    /// Plain native containers need no recording canvas. Install custom paint
    /// only when content needs it, as text/canvas views do on UIKit. Keep the
    /// callback thereafter so removing that content clears the old recording.
    pub(crate) fn enable_drawing(&self) {
        if self.drawing.replace(true) {
            return;
        }
        let weak = self.weak.clone();
        self.native.borrow_mut().on_custom_draw(move |event| {
            if let Some(node) = weak.upgrade() {
                node.draw(event);
            }
        });
    }
    pub(crate) fn input_to_parent(&self, parent: &Node) -> Result<nsh_core::Matrix> {
        use nsh_core::{Matrix, TransformKind as K};
        let (x, y) = if let Some(scroll) = parent.scroll.borrow().as_ref() {
            let position = self.mounted().borrow().layout_position()?;
            let content = scroll.content.borrow().layout_position_in_window()?;
            let origin = parent.native.borrow().layout_position_in_window()?;
            let offset = (
                (content.x - origin.x) as f64 / self.scale as f64 + scroll.centering().0 as f64,
                (content.y - origin.y) as f64 / self.scale as f64 + scroll.centering().1 as f64,
            );
            let matrix = self
                .input_transform
                .get()
                .mul(&Matrix::of(
                    K::TranslateX,
                    position.x as f64 / self.scale as f64,
                ))
                .mul(&Matrix::of(
                    K::TranslateY,
                    position.y as f64 / self.scale as f64,
                ))
                .mul(&Matrix::of(K::Scale, scroll.scale() as f64))
                .mul(&Matrix::of(K::TranslateX, offset.0))
                .mul(&Matrix::of(K::TranslateY, offset.1));
            return Ok(if scroll.mirrored() {
                matrix
                    .mul(&Matrix::of(K::ScaleX, -1.0))
                    .mul(&Matrix::of(K::TranslateX, parent.frame.get().width as f64))
            } else {
                matrix
            });
        } else {
            // Native containers can add layout offsets (for example a screen header).
            let origin = parent.native.borrow().layout_position_in_window()?;
            let child = self.mounted().borrow().layout_position_in_window()?;
            ((child.x - origin.x) as f64, (child.y - origin.y) as f64)
        };
        Ok(self
            .input_transform
            .get()
            .mul(&Matrix::of(K::TranslateX, x / self.scale as f64))
            .mul(&Matrix::of(K::TranslateY, y / self.scale as f64)))
    }
    pub(crate) fn input_in_root(&self, root: &Node) -> Result<Option<nsh_core::Matrix>> {
        if self.id == root.id {
            return Ok(Some(nsh_core::Matrix::IDENTITY));
        }
        let Some(parent) = self.parent_node.borrow().upgrade() else {
            return Ok(None);
        };
        parent
            .input_in_root(root)?
            .map(|world| self.input_to_parent(&parent).map(|m| m.mul(&world)))
            .transpose()
    }
    pub(crate) fn input_priority(&self) -> bool {
        self.pinned.get() != (0.0, 0.0)
    }
    pub(crate) fn top_overlap(&self) -> f32 {
        let mut y = self.frame.get().y;
        let mut parent = self.parent_node.borrow().upgrade();
        while let Some(node) = parent {
            if node.kind == ViewKind::Modal {
                return node
                    .modal
                    .borrow()
                    .as_ref()
                    .map_or(0.0, |modal| (modal.safe_top() - y).max(0.0));
            }
            y += node.frame.get().y;
            parent = node.parent_node.borrow().upgrade();
        }
        (self.platform.get().safe_area.top.0 - y).max(0.0)
    }
    /// The corner radii its native node was given (top-left first), which
    /// platform-owned children that fill it clip to.
    pub(crate) fn border_radii(&self) -> [f32; 4] {
        self.applied.get().border_radius
    }
    pub(crate) fn clips_children(&self) -> bool {
        self.style.borrow().overflow_hidden
            || self.kind == ViewKind::Scroll
            || self.modal.borrow().as_ref().is_some_and(|modal| {
                modal.presentation() != nsh_core::ModalPresentation::FullScreen
            })
    }
    pub(crate) fn scene_active(&self) -> bool {
        self.scene
            .borrow()
            .as_ref()
            .is_some_and(|scene| scene.active())
    }
    pub(crate) fn invalidate_scene(&self) {
        if let Some(scene) = self.scene.borrow().as_ref() {
            scene.invalidate();
        }
    }
    pub(crate) fn scene_opacity(&self) -> f32 {
        self.style.borrow().opacity
            * self.presentation.get().0
            * self.extra.get().0
            * self.navigation_presentation.get().0
    }
    pub(crate) fn has_native_paint(&self) -> bool {
        let style = self.style.borrow();
        self.id == nsh_core::ROOT_ID
            || self.kind != ViewKind::View
            || self.shaders.contains(self.id)
            || !style.background_color.is_transparent()
            || style.borders.widths.any(|v| v.0 > 0.0)
            || style.outline.is_visible()
            || style.effects.as_ref().is_some_and(|effects| {
                !effects.background_images.is_empty() || !effects.box_shadows.is_empty()
            })
    }
    pub(crate) fn capture_bounds(
        &self,
        depth: usize,
    ) -> Result<crate::render::composition::geometry::Bounds> {
        use crate::render::composition::geometry::Bounds;
        if depth > 256 {
            return Err(Error::Unsupported(
                "compositor hierarchy exceeds 256 levels",
            ));
        }
        let frame = self.frame.get();
        let mut bounds = Bounds::new(frame.width, frame.height).own(&self.style.borrow());
        if !self.clips_children() {
            for child in self.children.borrow().iter().filter_map(Weak::upgrade) {
                if child.kind == ViewKind::Modal || child.style.borrow().hidden {
                    continue;
                }
                let child_frame = child.frame.get();
                let filters = child
                    .style
                    .borrow()
                    .effects
                    .as_ref()
                    .map(|effects| effects.filters.clone())
                    .unwrap_or_default();
                let matrix = child
                    .input_transform
                    .get()
                    .mul(&nsh_core::Matrix::of(
                        nsh_core::TransformKind::TranslateX,
                        child_frame.x as f64,
                    ))
                    .mul(&nsh_core::Matrix::of(
                        nsh_core::TransformKind::TranslateY,
                        child_frame.y as f64,
                    ));
                let child = child
                    .capture_bounds(depth + 1)?
                    .filters(&filters)
                    .transformed(matrix)
                    .ok_or(Error::Unsupported("unbounded projected compositor capture"))?;
                bounds.union(child);
            }
        }
        Ok(bounds)
    }
    pub(crate) fn header_item_visible(&self) -> bool {
        if self.kind != ViewKind::HeaderItem {
            return true;
        }
        let Some(parent) = self.parent_node.borrow().upgrade() else {
            return false;
        };
        let Some(screen) = parent.screen.borrow().as_ref().cloned() else {
            return false;
        };
        let c = screen.config.borrow();
        c.header_visible
            && (c.left == Some(self.id)
                || c.right == Some(self.id)
                || c.title_view == Some(self.id))
    }
    pub(crate) fn visible_in_tree(&self) -> bool {
        if !self.active.get()
            || self.leaving.get()
            || self.backface_away.get()
            || self.style.borrow().hidden
            || !self.header_item_visible()
        {
            return false;
        }
        if self.screen.borrow().as_ref().is_some_and(|s| !s.is_top()) {
            return false;
        }
        self.parent_node
            .borrow()
            .upgrade()
            .is_none_or(|parent| parent.visible_in_tree())
    }
    pub(crate) fn paint_visible(&self) -> bool {
        self.active.get()
            && !self.backface_away.get()
            && !self.style.borrow().hidden
            && self.header_item_visible()
            && self
                .screen
                .borrow()
                .as_ref()
                .is_none_or(|screen| screen.paint_visible())
            && self
                .parent_node
                .borrow()
                .upgrade()
                .is_none_or(|parent| parent.paint_visible())
    }
    pub(crate) fn deactivate(&self) {
        if !self.active.replace(false) {
            return;
        }
        callback(|| self.selection_clear());
        self.invalidate_scene();
        self.accessibility_state.deactivate(self);
        #[cfg(feature = "api-23")]
        self.accessibility_provider.clear();
        if let Some(input) = self.input.borrow().as_ref() {
            callback(|| input.close());
        }
        if let Some(scroll) = self.scroll.borrow().as_ref()
            && let Some(plane) = scroll.overlay.borrow().as_ref()
        {
            callback(|| plane.close(self));
        }
        if let Some(wrapper) = self.wrapper.borrow().as_ref() {
            self.accessibility_wrappers
                .set(self.accessibility_wrappers.get().saturating_sub(1));
            callback(|| wrapper.close(self));
        }
        if let Some(web) = self.web.borrow().as_ref() {
            callback(|| web.close());
        }
        if let Some(video) = self.video.borrow().as_ref() {
            callback(|| video.close());
        }
        if let Some(route) = self.dispatcher.upgrade() {
            let mut keyboard = route.keyboard.lock().unwrap();
            if keyboard.focused == Some(self.id) {
                keyboard.focused = None;
            }
            if keyboard.input == Some(self.id) {
                keyboard.input = None;
            }
        }
        if let Some(component) = self.component.borrow().as_ref() {
            component.deactivate();
        }
    }
    pub(crate) fn emit(&self, event: EventKind, payload: EventPayload) {
        if self.leaving.get() && event != EventKind::Dismiss {
            return;
        }
        if self.active.get() || event == EventKind::Dismiss {
            if matches!(
                event,
                EventKind::Focus | EventKind::PressDown | EventKind::Press
            ) {
                self.host.activate(self.dispatcher.clone());
            }
            if matches!(event, EventKind::Focus | EventKind::Blur) {
                if matches!(self.kind, ViewKind::Input | ViewKind::Native)
                    && let Some(route) = self.dispatcher.upgrade()
                {
                    let mut keyboard = route.keyboard.lock().unwrap();
                    if event == EventKind::Focus {
                        keyboard.focused = Some(self.id);
                        if self.kind == ViewKind::Input {
                            keyboard.input = Some(self.id);
                        }
                    } else {
                        if keyboard.focused == Some(self.id) {
                            keyboard.focused = None;
                        }
                        if keyboard.input == Some(self.id) {
                            keyboard.input = None;
                        }
                    }
                }
                callback(|| {
                    self.interaction.state(
                        self,
                        nsh_core::tailwind::state::FOCUSED,
                        event == EventKind::Focus,
                    )
                });
            }
            self.control.dispatch_event(self.id, event, payload);
        }
    }
    pub(crate) fn child_container(&self) -> NativeNode {
        self.scroll
            .borrow()
            .as_ref()
            .map_or_else(|| self.native.clone(), |scroll| scroll.content.clone())
    }
    pub(crate) fn child_index(&self, index: usize) -> usize {
        self.children.borrow()[..index]
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|child| child.kind != ViewKind::Modal)
            .count()
            + usize::from(self.component.borrow().is_some())
            + usize::from(self.screen.borrow().is_some())
            + usize::from(self.web.borrow().is_some())
            + usize::from(
                self.video.borrow().as_ref().is_some_and(|video| {
                    !video.is_fullscreen() && video.native().borrow().is_owned()
                }),
            )
            + usize::from(self.shaders.contains(self.id))
    }
    pub(crate) fn attach_component(
        &self,
        component: crate::interop::component::Instance,
    ) -> Result<()> {
        if let Some(old) = self.component.borrow_mut().take() {
            self.native.borrow_mut().remove_child(0)?;
            old.dispose()?;
        }
        let frame = self.frame.get();
        component.native.borrow().width(frame.width)?;
        component.native.borrow().height(frame.height)?;
        self.native
            .borrow_mut()
            .insert_child(component.native.clone(), 0)?;
        *self.component.borrow_mut() = Some(component);
        self.native
            .borrow()
            .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
        Ok(())
    }
    pub(crate) fn set_events(self: &Rc<Self>, events: EventMask) -> Result<()> {
        self.interaction.events(self, events)
    }
    fn adjusted_frame(&self, mut frame: Rect) -> Rect {
        if let Some(modal) = self.modal.borrow().as_ref()
            && let Some(size) = modal.sync_metrics()
        {
            frame.width = size.width;
            frame.height = size.height;
        }
        if self.kind != ViewKind::HeaderItem
            && let Some(parent) = self.parent_node.borrow().upgrade()
            && let Some(screen) = parent.screen.borrow().as_ref()
        {
            let offset = screen.large_title_offset();
            if offset > 0.0 && frame.y < offset {
                let shift = offset - frame.y;
                frame.y += shift;
                frame.height = (frame.height - shift).max(0.0);
            }
        }
        frame
    }
    pub(crate) fn set_frame(&self, raw: Rect) -> Result<()> {
        self.raw_frame.set(raw);
        let frame = self.adjusted_frame(raw);
        let old = self.frame.replace(frame);
        if old == frame {
            return Ok(());
        }
        self.update_layout_bounds()?;
        self.invalidate_ink(self.applied.get().ink_outset)?;
        let size_changed = old.width != frame.width || old.height != frame.height;
        if size_changed {
            if self.shaders.contains(self.id) {
                self.animator.request_frame();
            }
            #[cfg(feature = "api-23")]
            self.accessibility_provider.bounds(self)?;
            if let Some(canvas) = self.canvas.borrow().as_ref() {
                canvas.invalidate()?;
            }
            self.update_wrapper()?;
            self.interaction.hit_region(self)?;
            if let Some(component) = self.component.borrow().as_ref() {
                component.native.borrow().width(frame.width)?;
                component.native.borrow().height(frame.height)?;
            }
            if let Some(image) = self.image.borrow().as_ref() {
                image.frame(Size::new(frame.width, frame.height))?;
            }
            if let Some(web) = self.web.borrow().as_ref() {
                web.frame(Size::new(frame.width, frame.height))?;
            }
            if let Some(video) = self.video.borrow().as_ref() {
                video.frame(Size::new(frame.width, frame.height))?;
            }
            let style = self.style.borrow();
            self.apply_borders(&style)?;
            if let Some(layer) = self.shadow_layer.borrow().as_ref() {
                layer.update(self, &style)?;
            }
            if !style.transform.is_identity()
                || self.extra.get().1 != crate::render::transition::Extra::NONE
            {
                self.transform(&style, |_| None)?;
            }
            drop(style);
            self.animator.apply_node(self)?;
            if let Some(scroll) = self.scroll.borrow().as_ref() {
                scroll.viewport(Size::new(frame.width, frame.height))?;
            }
            if let Some(screen) = self.screen.borrow().as_ref() {
                screen.update(screen.has_back())?;
            }
            if self.text_drawing.borrow().is_some()
                || self.canvas.borrow().is_some()
                || self.style.borrow().effects.is_some()
                || self.outlined.get()
            {
                self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
            }
            if self.wrapper.borrow().is_some() {
                // The outer node owns LayoutRect. Its native child still needs
                // to resize when the content area changes.
                self.native
                    .borrow()
                    .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
            }
        }
        // Size changes request measure/layout through LayoutRect. Position is a
        // native render property, so moving a node does not measure its subtree.
        Ok(())
    }
    pub(crate) fn invalidate_mounted_geometry(&self) {
        self.applied_layout_size.set((0, 0, 0));
        self.applied_position.set((0, 0, 0));
    }
    pub(crate) fn update_layout_bounds(&self) -> Result<()> {
        let frame = self.frame.get();
        self.place_bounds(
            (frame.x * self.scale).round() as i32,
            (frame.y * self.scale).round() as i32,
        )
    }
    fn place_bounds(&self, x: i32, y: i32) -> Result<()> {
        let frame = self.frame.get();
        let mut width = (frame.width * self.scale).round() as i32;
        let mut height = (frame.height * self.scale).round() as i32;
        if !self.clips_children() && !self.children.borrow().is_empty() {
            width = width.max(1);
            height = height.max(1);
        }
        // The engine owns this geometry. Keep the measure boundary at a local
        // origin: changing LayoutRect's origin also requests measurement, and
        // ArkUI's SetLayoutRect marks NeedMeasure even when given the same box.
        // Native Position places it relative to the parent without reshaping
        // its children, like moving UIView/Android bounds at an unchanged size.
        let mounted = self.mounted();
        let ptr = Rc::as_ptr(&mounted) as usize;
        let native = mounted.borrow();
        if self.applied_layout_size.get() != (ptr, width, height) {
            native.set_layout_rect([0, 0, width, height])?;
            self.applied_layout_size.set((ptr, width, height));
        }
        if self.applied_position.get() != (ptr, x, y) {
            native.set_position([x as f32 / self.scale, y as f32 / self.scale])?;
            self.applied_position.set((ptr, x, y));
        }
        Ok(())
    }
    pub(crate) fn place(&self, event: &NodeCustomEvent, x: i32, y: i32) -> Result<()> {
        let frame = self.frame.get();
        if x != (frame.x * self.scale).round() as i32 || y != (frame.y * self.scale).round() as i32
        {
            self.place_bounds(x, y)?;
        }
        event.layout_child(&self.mounted().borrow(), x, y)?;
        Ok(())
    }
    fn navigated(&self) -> bool {
        self.id == nsh_core::ROOT_ID
            || matches!(
                self.kind,
                ViewKind::Stack
                    | ViewKind::Screen
                    | ViewKind::Modal
                    | ViewKind::HeaderItem
                    | ViewKind::InputAccessory
            )
            || self
                .parent_node
                .borrow()
                .upgrade()
                .is_some_and(|p| p.kind == ViewKind::Stack)
    }
    pub(crate) fn animated_frame(
        self: &Rc<Self>,
        raw: Rect,
        animation: Option<&nsh_core::LayoutAnimation>,
    ) -> Result<()> {
        self.raw_frame.set(raw);
        let frame = self.adjusted_frame(raw);
        let target = self.live.borrow().as_ref().and_then(|l| l.frame_target());
        if target.unwrap_or(self.frame.get()) == frame {
            return Ok(());
        }
        let animate = animation.filter(|_| !self.navigated() && !self.platform.get().reduce_motion);
        if animate.is_some() || self.live.borrow().is_some() {
            let moving = self
                .live
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .frame(self.frame.get(), frame, animate);
            if moving {
                self.animator.track(self);
                return Ok(());
            }
        }
        self.set_frame(raw)
    }
    pub(crate) fn inserted(
        self: &Rc<Self>,
        animation: Option<&nsh_core::LayoutAnimation>,
    ) -> Result<()> {
        if self.set_leaving(false) {
            if let Some(live) = self.live.borrow_mut().as_mut() {
                live.cancel_removal();
            }
            self.extra
                .set((1.0, crate::render::transition::Extra::NONE));
            self.interaction.pointer(
                self,
                self.parent_node
                    .borrow()
                    .upgrade()
                    .is_some_and(|p| p.interaction.blocks_children()),
            )?;
            self.restore_style()?;
        }
        if self.frame.get() != Rect::default()
            || self.navigated()
            || self.platform.get().reduce_motion
        {
            return Ok(());
        }
        if let Some(transition) = animation.and_then(|a| a.create) {
            let initial = self
                .live
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .appear(transition);
            self.extra.set(initial);
            self.restore_style()?;
            self.animator.track(self);
        }
        Ok(())
    }
    pub(crate) fn removing(
        self: &Rc<Self>,
        animation: Option<&nsh_core::LayoutAnimation>,
    ) -> Result<bool> {
        if self.leaving.get() {
            return Ok(true);
        }
        // No ancestor can be leaving while none is.
        if self.leaving_nodes.get() > 0 {
            let mut parent = self.parent_node.borrow().upgrade();
            while let Some(ancestor) = parent {
                if ancestor.leaving.get() {
                    return Ok(true);
                }
                parent = ancestor.parent_node.borrow().upgrade();
            }
        }
        if let Some(screen) = self.screen.borrow().as_ref()
            && screen.remove()?
        {
            return Ok(true);
        }
        if self.parent.get().is_none() || self.navigated() || self.platform.get().reduce_motion {
            return Ok(false);
        }
        let Some(transition) = animation.and_then(|a| a.delete) else {
            return Ok(false);
        };
        self.set_leaving(true);
        self.native.borrow().set_i32_attribute(
            Attribute::HitTestBehavior,
            ohos_arkui_binding::types::view::HitTestMode::None.into(),
        )?;
        self.live
            .borrow_mut()
            .get_or_insert_with(Default::default)
            .disappear(transition);
        self.animator.track(self);
        Ok(true)
    }
    pub(crate) fn tick_transition(&self, now: f64) -> Result<(bool, bool)> {
        let (navigation_active, navigation_removed) = self
            .screen
            .borrow()
            .as_ref()
            .map(|screen| screen.frame(now))
            .transpose()?
            .unwrap_or_default();
        let image_active = self
            .image
            .borrow()
            .as_ref()
            .map(|image| image.tick(now))
            .transpose()?
            .unwrap_or(false);
        let video_active = self
            .video
            .borrow()
            .as_ref()
            .map(|video| video.tick(now))
            .transpose()?
            .unwrap_or(false);
        let Some(mut live) = self.live.borrow_mut().take() else {
            return Ok((
                image_active || navigation_active || video_active,
                navigation_removed,
            ));
        };
        let step = if self.platform.get().reduce_motion {
            live.finish(&mut self.style.borrow_mut())
        } else {
            live.step(now, self.frame.get(), &mut self.style.borrow_mut())
        };
        if step.active {
            *self.live.borrow_mut() = Some(live);
        }
        if let Some(frame) = step.frame {
            self.set_frame(frame)?;
        }
        self.extra.set((step.opacity, step.extra));
        self.restore_style()?;
        self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        Ok((
            step.active || image_active || navigation_active || video_active,
            step.removed || navigation_removed,
        ))
    }
    pub(crate) fn finish_removal(&self) -> Result<()> {
        self.set_leaving(false);
        self.extra
            .set((1.0, crate::render::transition::Extra::NONE));
        self.restore_style()
    }
    pub(crate) fn style(self: &Rc<Self>, style: &ViewStyle) -> Result<()> {
        if style.outline.width.0 > 0.0 && !self.outlined.replace(true) {
            let weak = Rc::downgrade(self);
            self.native
                .borrow_mut()
                .on_custom_foreground_draw(move |event| {
                    callback(|| {
                        if let Some(node) = weak.upgrade()
                            && !node.scene_active()
                            && let Some(raw) =
                                event.draw_context_in_draw().and_then(|ctx| ctx.canvas())
                        {
                            // SAFETY: ArkUI retains this drawing context for this callback.
                            let canvas = unsafe { Canvas::from_raw_borrowed(raw) };
                            let frame = node.frame.get();
                            node.decoration.outline(
                                &canvas,
                                &node.style.borrow(),
                                Size::new(frame.width, frame.height),
                                node.scale,
                            );
                        }
                        Ok(())
                    })
                });
        }
        if self.style.borrow().effects != style.effects
            || self.style.borrow().outline != style.outline
        {
            self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        }
        let mut target = style.clone();
        if self.live.borrow().is_some() || target.transition.is_some() || target.animation.is_some()
        {
            let old = self.style.borrow().clone();
            self.live
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .style(
                    &mut target,
                    old,
                    self.platform.get().reduce_motion,
                    self.styled.get(),
                );
            self.animator.track(self);
        }
        self.styled.set(true);
        *self.style.borrow_mut() = target;
        if let Some(scene) = self.scene.borrow().as_ref() {
            scene.require(self);
        }
        self.ensure_wrapper()?;
        self.restore_style()
    }
    fn sync_visual_node(&self, visual: &NativeNode) {
        let ptr = Rc::as_ptr(visual) as usize;
        let mut applied = self.applied.get();
        if applied.visual_ptr != ptr {
            applied.visual_ptr = ptr;
            applied.visibility = 0;
            applied.transform = None;
            applied.transform_center = [f32::NAN; 3];
            self.applied.set(applied);
            self.interaction.invalidate_opacity();
            self.decoration.invalidate_native();
        }
    }
    fn apply_opacity(&self, style_opacity: f32) -> Result<()> {
        let visual = self.visual_node();
        self.sync_visual_node(&visual);
        self.interaction.opacity(
            self,
            if self.scene_active() {
                1.0
            } else {
                style_opacity
                    * self.presentation.get().0
                    * self.extra.get().0
                    * self.navigation_presentation.get().0
            },
        )
    }
    fn apply_background_color(&self, color: nsh_core::Color) -> Result<()> {
        let default_surface_bg = self.kind == ViewKind::Screen
            || self.modal.borrow().as_ref().is_some_and(|modal| {
                modal.presentation() != nsh_core::ModalPresentation::FullScreen
            });
        let bg = if default_surface_bg && color.is_transparent() {
            if self
                .appearance
                .get()
                .resolve(self.platform.get().color_scheme)
                == nsh_engine::ColorScheme::Dark
            {
                0xff121212
            } else {
                0xffffffff
            }
        } else {
            argb(color)
        };
        let mut applied = self.applied.get();
        if applied.background_color != bg {
            self.native.borrow().background_color(bg)?;
            applied.background_color = bg;
            self.applied.set(applied);
        }
        Ok(())
    }
    fn apply_borders(&self, style: &ViewStyle) -> Result<()> {
        let corners = style
            .corner_radius
            .map(|r| r.unwrap_or(style.border_radius).0);
        let mut radius = [
            corners.top_left,
            corners.top_right,
            corners.bottom_left,
            corners.bottom_right,
        ];
        if let Some(modal) = self.modal.borrow().as_ref()
            && radius == [0.0; 4]
        {
            radius = match modal.presentation() {
                nsh_core::ModalPresentation::FullScreen => [0.0; 4],
                nsh_core::ModalPresentation::PageSheet => [16.0, 16.0, 0.0, 0.0],
                nsh_core::ModalPresentation::FormSheet => [16.0; 4],
            };
        }
        let frame = self.frame.get();
        if frame.width > 0.0 && frame.height > 0.0 {
            let short = frame.width.min(frame.height);
            let half = short * 0.5;
            let snapped_half = ((short * self.scale).round() * 0.5) / self.scale;
            for r in &mut radius {
                if *r >= half - 1e-4 && *r < snapped_half {
                    *r = snapped_half;
                }
            }
        }
        let widths = style.borders.widths.to_array().map(|v| v.0);
        let colors = style.borders.colors.map(argb);
        let border_style = match style.borders.style {
            nsh_core::BorderStyle::Solid => 0,
            nsh_core::BorderStyle::Dashed => 1,
            nsh_core::BorderStyle::Dotted => 2,
        };
        let mut applied = self.applied.get();
        let node = self.native.borrow();
        if applied.border_radius != radius {
            node.set_number_attribute(
                Attribute::BorderRadius,
                radius.into_iter().map(Number::Float).collect(),
            )?;
            applied.border_radius = radius;
        }
        if applied.border_width != widths {
            node.set_number_attribute(
                Attribute::BorderWidth,
                widths.into_iter().map(Number::Float).collect(),
            )?;
            applied.border_width = widths;
        }
        if applied.border_color != colors {
            node.set_number_attribute(
                Attribute::BorderColor,
                colors.into_iter().map(Number::Uint).collect(),
            )?;
            applied.border_color = colors;
        }
        if applied.border_style != border_style {
            node.set_i32_attribute(Attribute::BorderStyle, border_style)?;
            applied.border_style = border_style;
        }
        self.applied.set(applied);
        Ok(())
    }
    /// Outlines still use the foreground callback; box shadows own bounded nodes.
    fn apply_ink_outset(&self, style: &ViewStyle) -> Result<()> {
        let mut outset = 0.0f32;
        if style.outline.is_visible() {
            outset = outset.max(style.outline.width.0 + style.outline.offset.0);
        }
        let outset = outset.max(0.0).ceil();
        let mut applied = self.applied.get();
        if applied.ink_outset != outset {
            applied.ink_outset = outset;
            self.applied.set(applied);
            self.invalidate_ink(outset.max(1.0))?;
        }
        Ok(())
    }
    /// A view whose paint reaches outside its bounds moved: ArkUI repaints
    /// only the view's own region, so its parent repaints the area the paint
    /// left (inside the parent's bounds).
    fn invalidate_ink(&self, outset: f32) -> Result<()> {
        if outset > 0.0
            && let Some(parent) = self.parent_node.borrow().upgrade()
        {
            parent
                .child_container()
                .borrow()
                .mark_dirty(NodeDirtyFlag::NeedRender)?;
        }
        Ok(())
    }
    fn apply_shadow(&self, style: &ViewStyle) -> Result<()> {
        use super::shadow_geometry::{native_box_shadow, native_radius};
        let next = native_box_shadow(style).map(|shadow| {
            (
                native_radius(shadow.blur.0 * self.scale * 0.5),
                shadow.offset_x.0 * self.scale,
                shadow.offset_y.0 * self.scale,
                argb(shadow.color),
            )
        });
        let mut applied = self.applied.get();
        if applied.shadow != next {
            let node = self.native.borrow();
            if let Some((radius, ox, oy, color)) = next {
                node.set_number_slice_attribute(
                    Attribute::CustomShadow,
                    &[
                        Number::Float(radius),
                        Number::Int(0),
                        Number::Float(ox),
                        Number::Float(oy),
                        Number::Int(0),
                        Number::Uint(color),
                        Number::Uint(0),
                    ],
                )?;
            } else {
                node.reset_attribute(Attribute::CustomShadow)?;
            }
            applied.shadow = next;
            self.applied.set(applied);
        }
        Ok(())
    }
    pub(crate) fn apply_clip(&self) -> Result<()> {
        let clip = self.clips_children();
        let mut applied = self.applied.get();
        if applied.clip != clip {
            self.native
                .borrow()
                .set_bool_attribute(Attribute::Clip, clip)?;
            applied.clip = clip;
            self.applied.set(applied);
            let frame = self.frame.get();
            if frame.width <= 0.0 || frame.height <= 0.0 {
                self.update_layout_bounds()?;
                self.native
                    .borrow()
                    .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
            }
        }
        Ok(())
    }
    fn ensure_shadow_layer(&self, style: &ViewStyle) -> Result<()> {
        if self.shadow_layer.borrow().is_none()
            && super::shadow_geometry::native_box_shadow(style).is_none()
            && style
                .effects
                .as_ref()
                .is_some_and(|e| e.box_shadows.iter().any(|s| !s.inset && s.is_visible()))
            && let Some(owner) = self.weak.upgrade()
        {
            self.enable_layout();
            *self.shadow_layer.borrow_mut() = Some(super::shadow::ShadowLayer::new(&owner)?);
        }
        Ok(())
    }
    fn apply_style(&self, style: &ViewStyle) -> Result<()> {
        if style.effects.as_ref().is_some_and(|effects| {
            !effects.background_images.is_empty()
                || effects
                    .box_shadows
                    .iter()
                    .any(|shadow| shadow.inset && shadow.is_visible())
        }) {
            self.enable_drawing();
        }
        self.ensure_shadow_layer(style)?;
        let visual = self.visual_node();
        self.sync_visual_node(&visual);
        if let Some(wrapper) = self.wrapper.borrow().as_ref()
            && let Some(composite) = wrapper.composite.borrow().as_ref()
        {
            composite.update(self)?;
        }
        if let Some(image) = self.image.borrow().as_ref() {
            image.style(style)?;
        }
        self.decoration.native(self, style)?;
        self.apply_opacity(style.opacity)?;
        self.transform(style, |_| None)?;
        self.apply_background_color(style.background_color)?;
        self.apply_borders(style)?;
        self.apply_shadow(style)?;
        if let Some(layer) = self.shadow_layer.borrow().as_ref() {
            layer.update(self, style)?;
        }
        self.apply_ink_outset(style)?;
        self.apply_clip()?;
        self.animator.apply_node(self)?;
        Ok(())
    }
    pub(crate) fn pin(&self, x: f32, y: f32) -> Result<()> {
        if self.pinned.replace((x, y)) != (x, y) {
            self.native.borrow().set_f32_attribute(
                Attribute::ZIndex,
                if x != 0.0 || y != 0.0 {
                    1_000_000.0
                } else {
                    0.0
                },
            )?;
            self.transform(&self.style.borrow(), |_| None)?;
            self.animator.apply_node(self)?;
        }
        Ok(())
    }
    pub(crate) fn presentation(&self, alpha: f32, y: f32) -> Result<()> {
        let old = self.presentation.replace((alpha, y));
        if old != (alpha, y) {
            let style = self.style.borrow();
            if old.0 != alpha {
                self.apply_opacity(style.opacity)?;
            }
            if old.1 != y {
                self.transform(&style, |_| None)?;
            }
            drop(style);
            self.animator.apply_node(self)?;
        }
        Ok(())
    }
    pub(crate) fn reset_text_tint(&self) -> Result<()> {
        if self.tint.take().is_some() {
            self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        }
        Ok(())
    }
    pub(crate) fn restore_style(&self) -> Result<()> {
        self.apply_style(&self.style.borrow())
    }
    pub(crate) fn animate(
        &self,
        values: &[(
            nsh_core::animation::AnimationProp,
            nsh_core::animation::AnimationValue,
        )],
    ) -> Result<()> {
        use nsh_core::animation::AnimationProp as P;
        let style = self.style.borrow();
        let mut opacity = None;
        let mut bg = None;
        let mut border_color = None;
        let mut has_transform = false;
        let mut component_props = Vec::new();
        for (prop, value) in values {
            match prop {
                P::CanvasArg(slot) => {
                    self.shaders.slot(self.id, *slot, *value);
                    if let Some(canvas) = self.canvas.borrow().as_ref() {
                        canvas.slot(*slot, *value)?;
                    }
                }
                P::ComponentSlot(slot) => {
                    let idx = *slot as usize;
                    if let Some(name) = self.component_slots.borrow().get(idx).cloned() {
                        let next = nsh_core::animation::AnimatedPropValue::from(*value);
                        let mut cache = self.component_animated.borrow_mut();
                        if cache.len() <= idx {
                            cache.resize(idx + 1, None);
                        }
                        if cache[idx] != Some(next) {
                            cache[idx] = Some(next);
                            component_props.push((
                                name,
                                Some(match next {
                                    nsh_core::animation::AnimatedPropValue::Number(n) => {
                                        serde_json::Number::from_f64(n).map_or(
                                            nsh_core::JsonValue::Null,
                                            nsh_core::JsonValue::Number,
                                        )
                                    }
                                    nsh_core::animation::AnimatedPropValue::Color(argb) => {
                                        nsh_core::JsonValue::from(argb)
                                    }
                                    _ => nsh_core::JsonValue::Null,
                                }),
                            ));
                        }
                    }
                }
                P::Color => {
                    let color = Some(crate::render::animation::color(*value));
                    if self.tint.replace(color) != color {
                        self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
                    }
                }
                P::Opacity => opacity = Some(value.number().clamp(0.0, 1.0)),
                P::BackgroundColor => bg = Some(crate::render::animation::color(*value)),
                P::BorderColor => border_color = Some(crate::render::animation::color(*value)),
                P::Transform(_) => has_transform = true,
                _ => {}
            }
        }
        if let Some(opacity) = opacity {
            self.apply_opacity(opacity)?;
        }
        if let Some(bg) = bg {
            self.apply_background_color(bg)?;
        }
        if let Some(color) = border_color {
            let colors = [argb(color); 4];
            let mut applied = self.applied.get();
            if applied.border_color != colors {
                self.native.borrow().set_number_attribute(
                    Attribute::BorderColor,
                    colors.into_iter().map(Number::Uint).collect(),
                )?;
                applied.border_color = colors;
                self.applied.set(applied);
            }
        }
        if bg.is_some() {
            let mut presented = style.clone();
            if let Some(bg) = bg {
                presented.background_color = bg;
            }
            self.ensure_shadow_layer(&presented)?;
            self.apply_shadow(&presented)?;
            if let Some(layer) = self.shadow_layer.borrow().as_ref() {
                layer.update(self, &presented)?;
            }
        }
        if has_transform {
            self.transform(&style, |i| {
                values
                    .iter()
                    .find_map(|(p, v)| (*p == P::Transform(i as u8)).then(|| v.number()))
            })?;
        }
        if let Some(wrapper) = self.wrapper.borrow().as_ref()
            && let Some(composite) = wrapper.composite.borrow().as_ref()
            && (opacity.is_some() || bg.is_some() || border_color.is_some() || has_transform)
        {
            composite.update(self)?;
        }
        if let Some(image) = self.image.borrow().as_ref()
            && (bg.is_some() || border_color.is_some())
        {
            let mut animated_style = style.clone();
            if let Some(bg) = bg {
                animated_style.background_color = bg;
            }
            if let Some(color) = border_color {
                animated_style.borders.colors.fill(color);
            }
            image.style(&animated_style)?;
        }
        drop(style);
        if !component_props.is_empty() {
            if let Some(component) = self.component.borrow().as_ref() {
                component.props(component_props)?;
            } else {
                self.component_animated.borrow_mut().clear();
            }
        }
        Ok(())
    }
    pub(crate) fn transform(
        &self,
        style: &ViewStyle,
        value: impl Fn(usize) -> Option<f32>,
    ) -> Result<()> {
        use nsh_core::{Matrix, TransformKind as K};
        let visual = self.visual_node();
        self.sync_visual_node(&visual);
        let extra = self.extra.get().1;
        let pinned = self.pinned.get();
        let nav_x = self.navigation_presentation.get().1;
        let pres_y = self.presentation.get().1;
        let parent_mirrored = self.parent_node.borrow().upgrade().is_some_and(|parent| {
            parent
                .scroll
                .borrow()
                .as_ref()
                .is_some_and(|scroll| scroll.mirrored())
        });
        let is_identity = style.transform.ops().is_empty()
            && !parent_mirrored
            && extra == crate::render::transition::Extra::NONE
            && pinned == (0.0, 0.0)
            && nav_x == 0.0
            && pres_y == 0.0;
        let mut applied = self.applied.get();
        let native = visual.borrow();
        if is_identity {
            if applied.transform.is_some() {
                native.reset_attribute(Attribute::Transform)?;
                applied.transform = None;
                self.invalidate_ink(applied.ink_outset)?;
            }
            self.backface_away.set(false);
            let visibility = if !self.header_item_visible() {
                2
            } else if self
                .screen
                .borrow()
                .as_ref()
                .is_some_and(|screen| !screen.paint_visible())
            {
                1
            } else {
                i32::from(style.hidden)
            };
            if applied.visibility != visibility {
                native.set_i32_attribute(Attribute::Visibility, visibility)?;
                applied.visibility = visibility;
            }
            self.applied.set(applied);
            self.input_transform.set(Matrix::IDENTITY);
            if self.scene_active() {
                self.invalidate_scene();
            }
            return Ok(());
        }
        let mut m = style.transform.matrix(value);
        if parent_mirrored {
            m = Matrix::of(K::ScaleX, -1.0).mul(&m);
        }
        let m = if extra == crate::render::transition::Extra::NONE {
            m
        } else {
            Matrix::of(K::Scale, extra.scale as f64)
                .mul(&Matrix::of(K::Rotate, extra.rotate.to_degrees() as f64))
                .mul(&Matrix::of(
                    K::TranslateY,
                    (extra.shift_y.0 * self.frame.get().height) as f64,
                ))
                .mul(&m)
        };
        let mut matrix = m.0;
        matrix[12] += (pinned.0 + nav_x) as f64;
        matrix[13] += (pres_y + pinned.1) as f64;
        // ArkUI matrices translate in pixels; core matrices use surface vp.
        let mut raw_values = [0.0f32; 16];
        for (i, v) in matrix.into_iter().enumerate() {
            raw_values[i] = (v * if matches!(i, 12..=14) {
                self.scale as f64
            } else if matches!(i, 3 | 7 | 11) {
                1.0 / self.scale as f64
            } else {
                1.0
            }) as f32;
        }
        if applied.transform != Some(raw_values) {
            native.set_number_attribute(
                Attribute::Transform,
                raw_values.into_iter().map(Number::Float).collect(),
            )?;
            applied.transform = Some(raw_values);
            self.invalidate_ink(applied.ink_outset)?;
        }
        self.backface_away
            .set(style.backface_hidden && m.0[10] < 0.0);
        let visibility = if !self.header_item_visible() {
            2
        } else if self
            .screen
            .borrow()
            .as_ref()
            .is_some_and(|screen| !screen.paint_visible())
        {
            1
        } else {
            i32::from(style.hidden || self.backface_away.get())
        };
        if applied.visibility != visibility {
            native.set_i32_attribute(Attribute::Visibility, visibility)?;
            applied.visibility = visibility;
        }
        let frame = self.frame.get();
        let ox = style
            .transform_origin
            .x
            .to_points(
                nsh_core::geometry::AbsoluteLength::Points(Points(frame.width)),
                nsh_core::geometry::REM_SIZE,
            )
            .0;
        let oy = style
            .transform_origin
            .y
            .to_points(
                nsh_core::geometry::AbsoluteLength::Points(Points(frame.height)),
                nsh_core::geometry::REM_SIZE,
            )
            .0;
        let oz = style.transform_origin.z.0;
        let center = [ox, oy, oz];
        if applied.transform_center != center {
            native.set_attribute(Attribute::TransformCenter, center.into())?;
            applied.transform_center = center;
        }
        self.applied.set(applied);
        let origin = (ox as f64, oy as f64, oz as f64);
        let mut before = Matrix::IDENTITY;
        before.0[12] = -origin.0;
        before.0[13] = -origin.1;
        before.0[14] = -origin.2;
        let mut after = Matrix::IDENTITY;
        after.0[12] = origin.0;
        after.0[13] = origin.1;
        after.0[14] = origin.2;
        self.input_transform
            .set(before.mul(&Matrix(matrix)).mul(&after));
        if self.scene_active() {
            self.invalidate_scene();
        }
        Ok(())
    }
    pub(crate) fn navigation_presentation(&self, alpha: f32, x: f32) -> Result<()> {
        let old = self.navigation_presentation.replace((alpha, x));
        if old != (alpha, x) {
            let style = self.style.borrow();
            if old.0 != alpha {
                self.apply_opacity(style.opacity)?;
            }
            if old.1 != x {
                self.transform(&style, |_| None)?;
            }
            drop(style);
            self.animator.apply_node(self)?;
        }
        Ok(())
    }
    pub(crate) fn text(
        self: &Rc<Self>,
        raster: &NativeText,
        content: Rect,
        layout: nsh_core::TextLayout,
    ) -> Result<bool> {
        let same_layout = self
            .raster
            .borrow()
            .as_ref()
            .is_some_and(|old| std::sync::Arc::ptr_eq(&old.0, &layout.0));
        let same_placement =
            self.content.get() == content && self.raster_origin_y.get() == raster.origin_y;
        if !same_layout {
            *self.text_drawing.borrow_mut() = raster.drawing.lock().expect("text drawing").take();
        }
        if self.text_drawing.borrow().is_some() {
            self.enable_drawing();
        }
        self.selection_clear()?;
        let has_links = !raster.links.is_empty();
        let links_changed = self.has_text_links.replace(has_links) != has_links || has_links;
        *self.raster.borrow_mut() = Some(layout);
        self.content.set(content);
        self.raster_origin_y.set(raster.origin_y);
        if raster.selection.is_some() || self.wrapper.borrow().is_some() {
            self.ensure_wrapper()?;
        }
        if !same_layout || !same_placement {
            self.native.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
        }
        Ok(links_changed)
    }
    pub(crate) fn text_listens(&self, event: EventKind) -> bool {
        self.raster
            .borrow()
            .as_ref()
            .and_then(|layout| layout.0.downcast_ref::<NativeText>())
            .is_some_and(|raster| {
                raster
                    .links
                    .iter()
                    .any(|(link, _)| link.events.contains(event))
            })
    }
    pub(crate) fn press(&self, event: EventKind, x: f32, y: f32) {
        if !self.touches.borrow().press_allowed(self.id) {
            return;
        }
        let target = {
            let raster = self.raster.borrow();
            raster
                .as_ref()
                .and_then(|layout| layout.0.downcast_ref::<NativeText>())
                .and_then(|raster| {
                    let content = self.content.get();
                    let x = x - content.x;
                    let y = y - content.y;
                    raster
                        .links
                        .iter()
                        .filter(|(link, rects)| {
                            link.events.contains(event)
                                && rects
                                    .iter()
                                    .any(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
                        })
                        .min_by_key(|(link, _)| link.end - link.start)
                        .map(|(link, _)| link.node)
                })
        };
        let payload = self.press_payload(x, y);
        if let Some(id) = target {
            if self.active.get() {
                self.control.dispatch_event(id, event, payload);
            }
        } else if self.interaction.mask().contains(event) {
            self.emit(event, payload);
        }
    }
    pub(crate) fn pointer_positions(
        &self,
        x: f32,
        y: f32,
    ) -> (nsh_core::Point<Points>, nsh_core::Point<Points>) {
        let position = nsh_core::Point::new(Points(x), Points(y));
        let mut page_position = position;
        if self.id != nsh_core::ROOT_ID
            && self.kind != ViewKind::Modal
            && let Ok(node_pos) = self.mounted().borrow().layout_position_in_window()
        {
            let mut top = self.parent_node.borrow().upgrade();
            let mut root_origin = None;
            while let Some(parent) = top {
                if parent.id == nsh_core::ROOT_ID || parent.kind == ViewKind::Modal {
                    root_origin = parent.mounted().borrow().layout_position_in_window().ok();
                    break;
                }
                top = parent.parent_node.borrow().upgrade();
            }
            if let Some(origin) = root_origin {
                page_position = nsh_core::Point::new(
                    Points((node_pos.x - origin.x) as f32 / self.scale + x),
                    Points((node_pos.y - origin.y) as f32 / self.scale + y),
                );
            }
        }
        (position, page_position)
    }
    pub(crate) fn press_payload(&self, x: f32, y: f32) -> EventPayload {
        let (position, page_position) = self.pointer_positions(x, y);
        EventPayload::Press(nsh_core::PressEvent {
            position,
            page_position,
            modifiers: self.host.modifiers(),
            clicks: self.touches.borrow().clicks(),
        })
    }
    pub(crate) fn mouse_payload(
        &self,
        x: f32,
        y: f32,
        button: Option<nsh_core::MouseButton>,
    ) -> EventPayload {
        let (position, page_position) = self.pointer_positions(x, y);
        EventPayload::Mouse(nsh_core::MouseEvent::new(
            position,
            page_position,
            self.host.modifiers(),
            button,
        ))
    }
    pub(crate) fn context_menu_payload(&self, x: f32, y: f32) -> EventPayload {
        let (position, page_position) = self.pointer_positions(x, y);
        EventPayload::ContextMenu(nsh_core::ContextMenuEvent {
            position,
            page_position,
            modifiers: self.host.modifiers(),
        })
    }
    pub(crate) fn press_feedback_allowed(&self) -> bool {
        if !self.touches.borrow().press_allowed(self.id) {
            return false;
        }
        if self.kind == ViewKind::Input {
            return true;
        }
        let mut policy = self
            .scroll
            .borrow()
            .as_ref()
            .map(|scroll| scroll.keyboard_taps());
        let mut parent = self.parent_node.borrow().upgrade();
        while policy.is_none() {
            let Some(node) = parent else { break };
            if node.kind == ViewKind::Input {
                return true;
            }
            policy = node
                .scroll
                .borrow()
                .as_ref()
                .map(|scroll| scroll.keyboard_taps());
            parent = node.parent_node.borrow().upgrade();
        }
        policy != Some(nsh_core::KeyboardTaps::Never)
            || self.platform.get().keyboard_height.0 <= 0.0
            || self
                .dispatcher
                .upgrade()
                .is_none_or(|route| route.keyboard.lock().unwrap().input.is_none())
    }
    fn measure(&self, event: &NodeCustomEvent) -> Result<()> {
        let mut frame = self.frame.get();
        if self.wrapper.borrow().is_none()
            && let Some(modal) = self.modal.borrow().as_ref()
        {
            let size = modal.measure(event)?;
            frame.width = size.width;
            frame.height = size.height;
            if self.frame.get() != frame
                && let Some(canvas) = self.canvas.borrow().as_ref()
            {
                canvas.invalidate()?;
            }
            self.frame.set(frame);
            self.raw_frame.set(frame);
        }
        // Rosen's `RSObjAbsGeometry::UpdateMatrix` returns early when both
        // width and height are `<= 0`, discarding the node's `(x, y)` layout
        // offset and transforms for all descendants, and `A::ResponseRegion`
        // percentages multiply by the measured size. Keep unclipped containers
        // at least 1px in each axis so their geometry and hit regions propagate.
        let mut width = (frame.width * self.scale).round() as i32;
        let mut height = (frame.height * self.scale).round() as i32;
        if !self.clips_children() && !self.children.borrow().is_empty() {
            width = width.max(1);
            height = height.max(1);
        }
        event.set_measured_size(width, height)?;
        if let Some(layer) = self.shadow_layer.borrow().as_ref() {
            layer.measure(event)?;
        }
        if let Some(input) = self.input.borrow().as_ref() {
            input.measure(event)?;
        } else if let Some(image) = self.image.borrow().as_ref() {
            crate::render::layout::ChildMeasure::new(event).child(&image.native.borrow())?;
        } else if let Some(scroll) = self.scroll.borrow().as_ref() {
            crate::render::layout::ChildMeasure::new(event).child(&scroll.mounted().borrow())?;
        } else {
            self.measure_children(event)?;
        }
        Ok(())
    }
    pub(crate) fn measure_children(&self, event: &NodeCustomEvent) -> Result<()> {
        let mut measure = crate::render::layout::ChildMeasure::new(event);
        if let Some(web) = self
            .web
            .borrow()
            .as_ref()
            .filter(|web| web.native().borrow().is_owned())
        {
            measure.child(&web.native().borrow())?;
        }
        if let Some(video) = self
            .video
            .borrow()
            .as_ref()
            .filter(|video| !video.is_fullscreen() && video.native().borrow().is_owned())
        {
            measure.child(&video.native().borrow())?;
        }
        if let Some(screen) = self.screen.borrow().as_ref() {
            screen.measure(event)?;
        }
        if let Some(component) = self.component.borrow().as_ref() {
            measure.child(&component.native.borrow())?;
        }
        if let Some(surface) = self.shaders.native(self.id) {
            measure.child(&surface.borrow())?;
        }
        if self.id == nsh_core::ROOT_ID
            && let Some(owner) = self.fullscreen_video.borrow().upgrade()
            && let Some(video) = owner.video.borrow().as_ref()
        {
            let frame = self.frame.get();
            video.fullscreen_frame(Size::new(frame.width, frame.height))?;
            measure.child(&video.native().borrow())?;
        }
        // Engine children have independent LayoutRect boundaries and native
        // Position. Their own dirty layout tasks apply engine-resolved geometry;
        // walking them here repeats native measurement of unaffected subtrees.
        Ok(())
    }
    fn layout(&self, event: &NodeCustomEvent) -> Result<()> {
        if let Some(layer) = self.shadow_layer.borrow().as_ref() {
            layer.layout(event)?;
        }
        let position = event.position_in_layout();
        event.set_layout_position(position.x, position.y)?;
        #[cfg(feature = "api-23")]
        self.accessibility_provider.bounds(self)?;
        if let Some(input) = self.input.borrow().as_ref() {
            input.layout(event, self.scale)?;
        } else if let Some(image) = self.image.borrow().as_ref() {
            event.layout_child(&image.native.borrow(), 0, 0)?;
        } else if let Some(scroll) = self.scroll.borrow().as_ref() {
            event.layout_child(&scroll.mounted().borrow(), 0, 0)?;
        } else {
            self.layout_children(event)?;
        }
        Ok(())
    }
    pub(crate) fn layout_children(&self, event: &NodeCustomEvent) -> Result<()> {
        if let Some(web) = self
            .web
            .borrow()
            .as_ref()
            .filter(|web| web.native().borrow().is_owned())
        {
            event.layout_child(&web.native().borrow(), 0, 0)?;
        }
        if let Some(video) = self
            .video
            .borrow()
            .as_ref()
            .filter(|video| !video.is_fullscreen() && video.native().borrow().is_owned())
        {
            event.layout_child(&video.native().borrow(), 0, 0)?;
        }
        if let Some(component) = self.component.borrow().as_ref() {
            event.layout_child(&component.native.borrow(), 0, 0)?;
        }
        if let Some(surface) = self.shaders.native(self.id) {
            event.layout_child(&surface.borrow(), 0, 0)?;
        }
        // Engine children place themselves through native Position; only
        // platform-owned children participate in this parent's layout pass.
        if let Some(screen) = self.screen.borrow().as_ref() {
            screen.layout(event)?;
        }
        if self.id == nsh_core::ROOT_ID
            && let Some(owner) = self.fullscreen_video.borrow().upgrade()
            && let Some(video) = owner.video.borrow().as_ref()
        {
            event.layout_child(&video.native().borrow(), 0, 0)?;
        }
        Ok(())
    }
    pub(crate) fn draw_outer_shadows(&self, canvas: &Canvas) {
        let frame = self.frame.get();
        self.decoration.shadows(
            canvas,
            &self.style.borrow(),
            Size::new(frame.width, frame.height),
            self.scale,
            false,
        );
    }
    fn draw(&self, event: &NodeCustomEvent) {
        let frame = self.frame.get();
        let has_area = frame.width > 0.0 && frame.height > 0.0;
        let has_effects = has_area
            && self
                .style
                .borrow()
                .effects
                .as_ref()
                .is_some_and(|e| !e.background_images.is_empty() || !e.box_shadows.is_empty());
        if !has_effects {
            self.decoration.clear_paint_cache();
            if !has_area
                || (self.canvas.borrow().is_none()
                    && self.text_drawing.borrow().is_none()
                    && self.selection.range.get().is_none())
            {
                return;
            }
        }
        if let Some(raw) = event.draw_context_in_draw().and_then(|ctx| ctx.canvas()) {
            // SAFETY: ArkUI owns this canvas for the duration of this draw callback.
            let canvas = unsafe { Canvas::from_raw_borrowed(raw) };
            if has_effects {
                self.decoration.background(
                    &canvas,
                    &self.style.borrow(),
                    Size::new(frame.width, frame.height),
                    self.scale,
                );
            }
            if let Some(drawing) = self.canvas.borrow().as_ref() {
                callback(|| drawing.draw(&canvas));
            }
            self.draw_selection(&canvas, false);
            if let Some(drawing) = self.text_drawing.borrow_mut().as_mut() {
                let content = self.content.get();
                let brush = self
                    .tint
                    .get()
                    .and_then(|color| {
                        ohos_drawing_binding::ColorFilter::blend(
                            argb(color),
                            ohos_drawing_binding::BlendMode::SrcIn,
                        )
                    })
                    .map(|color| {
                        let mut filter = ohos_drawing_binding::Filter::new();
                        filter.set_color_filter(Some(&color));
                        let mut brush = ohos_drawing_binding::Brush::new();
                        brush.set_filter(Some(&filter));
                        brush
                    });
                let raster_scale = self
                    .raster
                    .borrow()
                    .as_ref()
                    .and_then(|layout| layout.0.downcast_ref::<NativeText>())
                    .map_or(self.scale, |raster| raster.scale);
                let tx = content.x * self.scale;
                let ty = (content.y + self.raster_origin_y.get()) * self.scale;
                let scale_mismatch = (self.scale - raster_scale).abs() > f32::EPSILON;
                let needs_save = tx != 0.0 || ty != 0.0 || scale_mismatch;
                if needs_save {
                    canvas.save();
                    if tx != 0.0 || ty != 0.0 {
                        canvas.translate(tx, ty);
                    }
                    if scale_mismatch {
                        canvas.scale(self.scale / raster_scale, self.scale / raster_scale);
                    }
                }
                if let Some(brush) = &brush {
                    canvas.save_layer(None, Some(brush));
                }
                if let Err(error) = drawing.draw(&canvas) {
                    log::warn!("OHOS text draw: {error}");
                }
                if brush.is_some() {
                    canvas.restore();
                }
                if needs_save {
                    canvas.restore();
                }
            }
            self.draw_selection(&canvas, true);
        }
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.set_leaving(false);
        self.shadow_layer.borrow_mut().take();
        // Struct fields otherwise outlive this Drop body. Release controller
        // handles before recursively disposing their mounted native descendants.
        #[cfg(feature = "api-23")]
        self.accessibility_provider.clear();
        if let Some(input) = self.input.borrow_mut().take() {
            callback(|| input.close());
        }
        if let Some(web) = self.web.borrow_mut().take() {
            callback(|| web.close());
            let native = web.native().clone();
            drop(web);
            if native.borrow().is_owned() {
                callback(|| {
                    let index = self
                        .native
                        .borrow()
                        .children()
                        .iter()
                        .position(|child| Rc::ptr_eq(child, &native));
                    if let Some(index) = index {
                        if native.borrow().parent()?.is_some() {
                            self.native.borrow_mut().remove_child(index)?;
                        } else {
                            self.native.borrow_mut().children_mut().remove(index);
                        }
                    } else {
                        native.borrow().remove_from_parent()?;
                    }
                    if Rc::strong_count(&native) == 1 {
                        native.borrow_mut().dispose()?;
                    }
                    Ok(())
                });
            }
        }
        if let Some(video) = self.video.borrow_mut().take() {
            callback(|| video.close());
        }
        callback(|| self.interaction.dispose(self));
        if let Some(scroll) = self.scroll.borrow_mut().take() {
            callback(|| scroll.dispose());
            if let Some(plane) = scroll.overlay.borrow_mut().take() {
                callback(|| plane.close(self));
                // Its outer node remains in self.native; dropping Plane
                // releases its duplicate handle and its inner wrapper handle.
            }
            let wrapper = scroll.wrapper.clone();
            drop(scroll);
            // Texture export replaces the wrapper in the mounted tree. Once
            // the attachment releases it, this detached tree needs its own disposal.
            if wrapper.borrow().is_owned() && Rc::strong_count(&wrapper) == 1 {
                callback(|| {
                    wrapper.borrow().remove_from_parent()?;
                    wrapper.borrow_mut().dispose()?;
                    Ok(())
                });
            }
        }
        self.screen.borrow_mut().take();
        self.image.borrow_mut().take();
        if let Some(modal) = self.modal.borrow_mut().take() {
            callback(|| modal.close(false));
        }
        if let Some(component) = self.component.borrow_mut().take() {
            callback(|| {
                self.native.borrow_mut().remove_child(0)?;
                component.dispose()
            });
        }
        if let Some(wrapper) = self.wrapper.borrow_mut().take() {
            callback(|| wrapper.close(self));
            if wrapper.outer.borrow().is_owned() && Rc::strong_count(&wrapper.outer) == 1 {
                callback(|| {
                    wrapper.outer.borrow().remove_from_parent()?;
                    wrapper.outer.borrow_mut().dispose()?;
                    Ok(())
                });
            }
        }
        // A retiring Video retains this parent until its worker acknowledges
        // release. That UI owner disposes the parent after detaching its child.
        if Rc::strong_count(&self.native) == 1
            && let Err(error) = self.native.borrow_mut().dispose()
        {
            log::error!("OHOS dispose: {error}");
        }
    }
}
pub(crate) fn callback(f: impl FnOnce() -> Result<()>) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::error!("OHOS callback: {error}"),
        Err(_) => log::error!("panic in an OHOS native callback"),
    }
}

//! UI-thread controls and producer ownership survive worker release acknowledgement.
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use nsh_core::{ResizeMode, VideoProps};
use ohos_arkui_binding::{
    common::{attribute::ArkUINodeAttributeNumber as N, node_registry},
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::{Row, Slider, Stack, Text, XComponent},
    },
    types::{
        advanced::{LengthMetricUnit, NodeDirtyFlag},
        alignment::Alignment,
        attribute::ArkUINodeAttributeType as A,
        event::NodeEventType as E,
        node_type::ArkUINodeType,
    },
};
use ohos_native_window_binding::NativeWindow;
use ohos_xcomponent_binding::NativeXComponent;

use super::Operation;
use crate::{
    app::{
        runtime::{Dispatcher, Message},
        session::Services,
    },
    render::{
        geometry::Size,
        node::{NativeNode, Node, Result, callback},
    },
};

static NEXT: AtomicU64 = AtomicU64::new(1);
/// The height of the controls' bar and the side of the play / pause target.
const BAR: f32 = 48.0;
const CONTROL: f32 = 40.0;
thread_local! {
    static LIVE:RefCell<BTreeMap<u64,Weak<Ui>>>=RefCell::default();
    static RETIRING:RefCell<BTreeMap<u64,Rc<Ui>>>=RefCell::default();
}
#[derive(Clone, Copy, Default)]
pub(crate) struct UiState {
    pub(crate) playing: bool,
    pub(crate) time: i32,
    pub(crate) duration: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}
pub(super) struct Ui {
    pub(super) token: u64,
    pub(super) native: NativeNode,
    owner: Weak<Node>,
    parent: NativeNode,
    fullscreen_parent: RefCell<Option<NativeNode>>,
    home_index: Cell<usize>,
    // The owner's Weak cannot be upgraded from its Drop body.
    host: Arc<Services>,
    route: std::sync::Weak<Dispatcher>,
    surface: NativeNode,
    xcomponent: NativeXComponent,
    row: NativeNode,
    /// Play / pause: a drawn glyph ([`super::icon`]), as the system's.
    toggle: NativeNode,
    fullscreen_toggle: NativeNode,
    current: NativeNode,
    total: NativeNode,
    slider: NativeNode,
    /// What the glyph shows (read by its draw callback).
    playing: Rc<Cell<bool>>,
    fullscreen: Rc<Cell<bool>>,
    closed: Cell<bool>,
    controls: Cell<bool>,
    visibility: Cell<super::controls::Controls>,
    size: Cell<Size>,
    fullscreen_size: Cell<Size>,
    state: Cell<UiState>,
    mode: Cell<ResizeMode>,
    scale: f32,
    updating_slider: Rc<Cell<bool>>,
    /// The user holds the slider's thumb (Begin to End).
    scrubbing: Rc<Cell<bool>>,
}
#[derive(Default)]
struct Construction {
    nodes: Vec<NativeNode>,
    xcomponent: Option<NativeXComponent>,
    committed: bool,
}
impl Construction {
    fn own(&mut self, node: NativeNode) -> NativeNode {
        self.nodes.push(node.clone());
        node
    }
}
impl Drop for Construction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(xcomponent) = self.xcomponent.as_ref() {
            xcomponent.unregister_callbacks();
        }
        for node in &self.nodes {
            let result = (|| -> Result<()> {
                node.borrow().remove_event_receiver()?;
                node_registry::unregister(&node.borrow());
                node.borrow().remove_from_parent()?;
                Ok(())
            })();
            if let Err(error) = result {
                log::error!("Video construction cleanup failed; retaining owners: {error}");
                std::mem::forget(std::mem::take(&mut self.nodes));
                std::mem::forget(self.xcomponent.take());
                return;
            }
        }
        for node in &self.nodes {
            node.borrow_mut().children_mut().clear();
        }
        for node in self.nodes.iter().rev() {
            let owned = node.borrow().is_owned();
            if owned && let Err(error) = node.borrow_mut().dispose() {
                log::error!("Video construction node disposal: {error}");
            }
        }
    }
}
impl Ui {
    pub(super) fn new(owner: &Rc<Node>, queue: mpsc::SyncSender<Operation>) -> Result<Rc<Self>> {
        let token = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| napi_ohos::Error::from_reason("Video UI identity exhausted"))?;
        let mut construction = Construction::default();
        let native = construction.own(Stack::new()?.into_node());
        let surface = construction.own(XComponent::new()?.into_node());
        let row = construction.own(Row::new()?.into_node());
        let toggle = construction.own(Stack::new()?.into_node());
        let fullscreen_toggle = construction.own(Stack::new()?.into_node());
        let current = construction.own(Text::new()?.into_node());
        let total = construction.own(Text::new()?.into_node());
        let slider = construction.own(Slider::new()?.into_node());
        for node in [
            &native,
            &surface,
            &row,
            &toggle,
            &fullscreen_toggle,
            &current,
            &total,
            &slider,
        ] {
            node.borrow().set_length_metric_unit(LengthMetricUnit::Vp)?;
        }
        native
            .borrow()
            .set_i32_attribute(A::Alignment, Alignment::Center.into())?;
        native.borrow().set_bool_attribute(A::Clip, true)?;
        native
            .borrow()
            .set_u32_attribute(A::BackgroundColor, 0xff000000)?;
        surface
            .borrow()
            .set_string_attribute(A::XComponentId, format!("nsh-video-{token}"))?;
        // HarmonyOS's video controls, as ArkTS `Video` draws them: over the
        // picture's foot with no scrim, the ringed glyph, the elapsed time,
        // a thin track with a white knob, and the length. No label: what the
        // button does is its glyph.
        row.borrow().set_number_slice_attribute(
            A::Padding,
            &[N::Float(0.0), N::Float(12.0), N::Float(0.0), N::Float(2.0)],
        )?;
        row.borrow()
            // ArkUI_VerticalAlignment's Center.
            .set_i32_attribute(A::RowAlignItems, 1)?;
        for button in [&toggle, &fullscreen_toggle] {
            let button = button.borrow();
            button.width(CONTROL)?;
            button.height(CONTROL)?;
            button.set_u32_attribute(A::AccessibilityRole, ArkUINodeType::Button.into())?;
        }
        for text in [&current, &total] {
            let text = text.borrow();
            text.set_f32_attribute(A::FontSize, 14.0)?;
            text.set_u32_attribute(A::FontColor, 0xb3ff_ffff)?;
            // Tabular digits: the times keep their width as they count.
            text.set_string_attribute(A::FontFeature, "\"tnum\" on")?;
            text.set_string_attribute(A::TextContent, "00:00")?;
        }
        {
            let slider = slider.borrow();
            slider.set_u32_attribute(A::LayoutWeight, 1)?;
            // Top, right, bottom, left: ArkUI takes one value or four.
            slider.set_number_slice_attribute(
                A::Margin,
                &[N::Float(0.0), N::Float(8.0), N::Float(0.0), N::Float(8.0)],
            )?;
            slider.set_u32_attribute(A::SliderTrackColor, 0x33ff_ffff)?;
            slider.set_u32_attribute(A::SliderSelectedColor, 0xffff_ffff)?;
            slider.set_u32_attribute(A::SliderBlockColor, 0xffff_ffff)?;
            slider.set_f32_attribute(A::SliderTrackThickness, 3.0)?;
        }
        let playing = Rc::new(Cell::new(false));
        let glyph = playing.clone();
        toggle.borrow_mut().on_custom_draw(move |event| {
            let Some(context) = event.draw_context_in_draw() else {
                return;
            };
            let Some(raw) = context.canvas() else {
                return;
            };
            let size = context.size();
            // SAFETY: ArkUI owns this canvas for the duration of this draw callback.
            let canvas = unsafe { ohos_drawing_binding::Canvas::from_raw_borrowed(raw) };
            super::icon::draw(
                &canvas,
                (size.width as f32, size.height as f32),
                glyph.get(),
            );
        });
        let fullscreen = Rc::new(Cell::new(false));
        let glyph = fullscreen.clone();
        fullscreen_toggle.borrow_mut().on_custom_draw(move |event| {
            let Some(context) = event.draw_context_in_draw() else {
                return;
            };
            let Some(raw) = context.canvas() else {
                return;
            };
            let size = context.size();
            // SAFETY: ArkUI owns this canvas for the duration of this draw callback.
            let canvas = unsafe { ohos_drawing_binding::Canvas::from_raw_borrowed(raw) };
            super::icon::draw_fullscreen(
                &canvas,
                (size.width as f32, size.height as f32),
                glyph.get(),
            );
        });
        for child in [&toggle, &current, &slider, &total, &fullscreen_toggle] {
            row.borrow_mut().add_child((*child).clone())?;
        }
        native.borrow_mut().add_child(surface.clone())?;
        let xcomponent = surface
            .borrow()
            .native_xcomponent()
            .ok_or_else(|| napi_ohos::Error::from_reason("C Video XComponent is unavailable"))?;
        construction.xcomponent = Some(xcomponent.clone());
        let send = queue.clone();
        xcomponent.on_surface_created(move |_, window| {
            let raw = std::ptr::NonNull::new(window.0).ok_or_else(|| {
                napi_ohos::Error::from_reason("Video native surface window is null")
            })?;
            // SAFETY: this callback borrows a live SDK window; the UI owner is
            // retained until the worker releases its player and acknowledges.
            let window = unsafe { NativeWindow::retain_from_ptr(raw) }
                .map_err(|error| napi_ohos::Error::from_reason(error.to_string()))?;
            send.try_send(Operation::Window(Some(window)))
                .map_err(|_| napi_ohos::Error::from_reason("Video window queue is full or closed"))
        });
        let send = queue.clone();
        xcomponent.on_surface_destroyed(move |_, window| {
            send.try_send(Operation::WindowGone(window.0 as usize))
                .map_err(|_| napi_ohos::Error::from_reason("Video window queue is full or closed"))
        });
        xcomponent.register_callback()?;
        let send = queue.clone();
        toggle.borrow_mut().on_event_no_param(E::OnClick, move || {
            if let Err(error) = send.try_send(Operation::Toggle) {
                log::error!("Video control queue: {error}");
            }
        });
        let updating_slider = Rc::new(Cell::new(false));
        let setting = updating_slider.clone();
        let scrubbing = Rc::new(Cell::new(false));
        let scrub = scrubbing.clone();
        slider
            .borrow_mut()
            .on_event(E::SliderEventOnChange, move |event| {
                // The slider's value and change mode are its component event's
                // data[0].f32 and data[1].i32; the node event's number values are
                // empty for it.
                let mode = event.component_i32(1);
                // From Begin to End the thumb is the finger's: progress does not
                // move it meanwhile.
                if !setting.get() {
                    scrub.set(matches!(mode, Some(0 | 1)));
                }
                if let Some(seconds) = slider_seek(event.component_f32(0), mode, setting.get())
                    && let Err(error) = queue.try_send(Operation::Seek(seconds))
                {
                    log::error!("Video seek queue: {error}");
                }
            });
        let ui = Rc::new(Self {
            token,
            native,
            owner: Rc::downgrade(owner),
            parent: owner.native.clone(),
            fullscreen_parent: RefCell::default(),
            home_index: Cell::new(0),
            host: owner.host.clone(),
            route: owner.dispatcher.clone(),
            surface,
            xcomponent,
            row,
            toggle,
            fullscreen_toggle,
            current,
            total,
            slider,
            playing,
            fullscreen,
            closed: Cell::new(false),
            controls: Cell::new(false),
            visibility: Cell::new(super::controls::Controls::default()),
            size: Cell::new(Size::default()),
            fullscreen_size: Cell::new(Size::default()),
            state: Cell::new(UiState::default()),
            mode: Cell::new(ResizeMode::Contain),
            scale: owner.scale,
            updating_slider,
            scrubbing,
        });
        let weak = Rc::downgrade(&ui);
        ui.fullscreen_toggle
            .borrow_mut()
            .on_event_no_param(E::OnClick, move || {
                if let Some(ui) = weak.upgrade().filter(|ui| !ui.closed.get()) {
                    callback(|| ui.set_fullscreen(!ui.fullscreen.get()));
                }
            });
        let weak = Rc::downgrade(&ui);
        ui.surface.borrow_mut().on_click(move || {
            if let Some(ui) = weak
                .upgrade()
                .filter(|ui| !ui.closed.get() && ui.controls.get())
            {
                callback(|| {
                    let mut visibility = ui.visibility.get();
                    visibility.toggle(ui.state.get().playing, ui.now());
                    ui.visibility.set(visibility);
                    ui.apply_visibility()?;
                    ui.track_controls();
                    Ok(())
                });
            }
        });
        LIVE.with_borrow_mut(|live| {
            live.insert(token, Rc::downgrade(&ui));
        });
        construction.committed = true;
        Ok(ui)
    }
    pub(super) fn abort(self: &Rc<Self>) {
        self.closed.set(true);
        LIVE.with_borrow_mut(|live| {
            live.remove(&self.token);
        });
        if let Err(error) = self.complete_retirement() {
            log::error!("Video startup cleanup failed; retaining owners: {error}");
            RETIRING.with_borrow_mut(|retiring| {
                retiring.insert(self.token, self.clone());
            });
        }
    }
    pub(super) fn current_window(&self) -> Result<Option<NativeWindow>> {
        let Some(window) = self.xcomponent.native_window() else {
            return Ok(None);
        };
        let raw = NonNull::new(window.raw())
            .ok_or_else(|| napi_ohos::Error::from_reason("Video native window is null"))?;
        // SAFETY: the XComponent registry supplies this live UI-owned surface;
        // the retained UI owner survives worker use until close acknowledgement.
        unsafe { NativeWindow::retain_from_ptr(raw) }
            .map(Some)
            .map_err(|e| napi_ohos::Error::from_reason(e.to_string()).into())
    }
    pub(super) fn configure(&self, props: &VideoProps) -> Result<()> {
        if !props.controls || props.source.is_none() {
            self.set_fullscreen(false)?;
        }
        self.mode.set(props.resize_mode);
        if self.controls.get() != props.controls {
            if props.controls {
                self.native.borrow_mut().add_child(self.row.clone())?;
            } else {
                self.native.borrow_mut().remove_child(1)?;
            }
            self.controls.set(props.controls);
            if props.controls {
                let mut visibility = self.visibility.get();
                visibility.show(self.state.get().playing, self.now());
                self.visibility.set(visibility);
                self.track_controls();
            }
        }
        self.apply_visibility()?;
        self.layout()
    }
    pub(super) fn frame(&self, size: Size) -> Result<()> {
        self.size.set(size);
        if self.fullscreen.get() {
            return Ok(());
        }
        self.resize(size)
    }
    fn resize(&self, size: Size) -> Result<()> {
        self.native.borrow().width(size.width)?;
        self.native.borrow().height(size.height)?;
        self.layout()
    }
    /// The owner omits this Stack from its native layout while it is at the root.
    pub(super) fn is_fullscreen(&self) -> bool {
        self.fullscreen.get()
    }
    /// Root layout and window resize share the fullscreen dimensions.
    pub(super) fn fullscreen_frame(&self, size: Size) -> Result<()> {
        if self.fullscreen.get() && self.fullscreen_size.replace(size) != size {
            self.resize(size)?;
        }
        Ok(())
    }
    /// Moves the entire player UI beyond clipping ancestors on the UI thread.
    pub(super) fn set_fullscreen(&self, enabled: bool) -> Result<()> {
        if self.fullscreen.get() == enabled
            || (enabled && (self.closed.get() || !self.controls.get()))
        {
            return Ok(());
        }
        let bars = if enabled {
            let Some(owner) = self.owner.upgrade() else {
                return Ok(());
            };
            let mut root = owner.clone();
            while root.id != nsh_core::ROOT_ID {
                let parent = root.parent_node.borrow().upgrade();
                let Some(parent) = parent else {
                    return Ok(());
                };
                root = parent;
            }
            let previous = owner.fullscreen_video.borrow().upgrade();
            if let Some(previous) = previous
                && let Some(video) = previous.video.borrow().as_ref()
            {
                video.set_fullscreen(false)?;
            }
            let index = self
                .parent
                .borrow()
                .children()
                .iter()
                .position(|child| Rc::ptr_eq(child, &self.native))
                .ok_or_else(|| napi_ohos::Error::from_reason("Video native parent is missing"))?;
            self.detach(&self.parent)?;
            if let Err(error) = root.native.borrow_mut().add_child(self.native.clone()) {
                callback(|| {
                    self.parent
                        .borrow_mut()
                        .insert_child(self.native.clone(), index)?;
                    Ok(())
                });
                return Err(error.into());
            }
            self.home_index.set(index);
            *self.fullscreen_parent.borrow_mut() = Some(root.native.clone());
            self.fullscreen.set(true);
            *owner.fullscreen_video.borrow_mut() = Rc::downgrade(&owner);
            root.enable_layout();
            self.refresh_back();
            let frame = root.frame.get();
            let size = Size::new(frame.width, frame.height);
            self.fullscreen_size.set(size);
            self.resize(size)?;
            root.native
                .borrow()
                .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
            self.host.video_fullscreen(self.route.clone(), true)
        } else {
            // Restore bars even when the native move fails; retirement retries
            // the detach against the retained root before disposing the Stack.
            let bars = self.host.video_fullscreen(self.route.clone(), false);
            if let Some(parent) = self.fullscreen_parent.borrow().as_ref() {
                self.detach(parent)?;
                let index = self
                    .home_index
                    .get()
                    .min(self.parent.borrow().children().len());
                if let Err(error) = self
                    .parent
                    .borrow_mut()
                    .insert_child(self.native.clone(), index)
                {
                    callback(|| {
                        parent.borrow_mut().add_child(self.native.clone())?;
                        Ok(())
                    });
                    return Err(error.into());
                }
                parent.borrow().mark_dirty(NodeDirtyFlag::NeedMeasure)?;
            }
            self.fullscreen.set(false);
            if let Some(owner) = self.owner.upgrade() {
                *owner.fullscreen_video.borrow_mut() = Weak::new();
            }
            if self
                .fullscreen_parent
                .borrow()
                .as_ref()
                .is_some_and(|parent| Rc::strong_count(parent) > 1)
            {
                self.fullscreen_parent.borrow_mut().take();
            }
            self.refresh_back();
            self.resize(self.size.get())?;
            bars
        };
        // App siblings can have a positive z-index; append order breaks ties.
        self.native
            .borrow()
            .set_i32_attribute(A::ZIndex, if enabled { i32::MAX } else { 0 })?;
        self.fullscreen_toggle
            .borrow()
            .mark_dirty(NodeDirtyFlag::NeedRender)?;
        let mut visibility = self.visibility.get();
        visibility.show(self.state.get().playing, self.now());
        self.visibility.set(visibility);
        self.apply_visibility()?;
        self.track_controls();
        bars?;
        Ok(())
    }
    fn refresh_back(&self) {
        if let Some(route) = self.route.upgrade() {
            route.send(Message::UpdateBack);
        }
    }
    fn detach(&self, parent: &NativeNode) -> Result<()> {
        self.native.borrow().remove_from_parent()?;
        parent
            .borrow_mut()
            .children_mut()
            .retain(|child| !Rc::ptr_eq(child, &self.native));
        Ok(())
    }
    fn now(&self) -> f64 {
        self.owner.upgrade().map_or(0.0, |node| node.animator.now())
    }
    fn track_controls(&self) {
        if self.controls.get()
            && let Some(owner) = self.owner.upgrade()
        {
            owner.animator.track(&owner);
        }
    }
    fn apply_visibility(&self) -> Result<()> {
        self.row
            .borrow()
            .set_i32_attribute(A::Visibility, i32::from(!self.visibility.get().visible))?;
        Ok(())
    }
    pub(super) fn tick(&self, now: f64) -> Result<bool> {
        if !self.controls.get() {
            return Ok(false);
        }
        let mut visibility = self.visibility.get();
        let active = visibility.tick(now);
        self.visibility.set(visibility);
        self.apply_visibility()?;
        Ok(active)
    }
    fn layout(&self) -> Result<()> {
        let fullscreen = self.fullscreen.get();
        let size = if fullscreen {
            self.fullscreen_size.get()
        } else {
            self.size.get()
        };
        let state = self.state.get();
        let (width, height) = picture(
            size,
            (state.width, state.height),
            if fullscreen {
                ResizeMode::Contain
            } else {
                self.mode.get()
            },
            self.scale,
        );
        self.surface.borrow().width(width)?;
        self.surface.borrow().height(height)?;
        self.row.borrow().width(size.width)?;
        self.row.borrow().height(BAR)?;
        self.row
            .borrow()
            .set_position([0.0, (size.height - BAR).max(0.0)])?;
        if let Some(owner) = self.owner.upgrade() {
            let _ = owner
                .native
                .borrow()
                .mark_dirty(ohos_arkui_binding::types::advanced::NodeDirtyFlag::NeedMeasure);
        }
        Ok(())
    }
    pub(super) fn begin_retirement(self: &Rc<Self>) {
        self.closed.set(true);
        callback(|| self.set_fullscreen(false));
        if let Some(owner) = self.owner.upgrade() {
            let mut fullscreen = owner.fullscreen_video.borrow_mut();
            if fullscreen.ptr_eq(&self.owner) {
                *fullscreen = Weak::new();
            }
        }
        self.refresh_back();
        LIVE.with_borrow_mut(|live| {
            live.remove(&self.token);
        });
        if self.native.borrow().is_owned() {
            RETIRING.with_borrow_mut(|retiring| {
                retiring.insert(self.token, self.clone());
            });
        }
    }
    fn complete_retirement(&self) -> Result<()> {
        if self.fullscreen.replace(false) {
            callback(|| {
                self.host.video_fullscreen(self.route.clone(), false)?;
                Ok(())
            });
            self.refresh_back();
        }
        self.xcomponent.unregister_callbacks();
        for node in [
            &self.toggle,
            &self.fullscreen_toggle,
            &self.slider,
            &self.surface,
        ] {
            if node.borrow().is_owned() {
                node.borrow().remove_event_receiver()?;
                node_registry::unregister(&node.borrow());
            }
        }
        if self.native.borrow().is_owned() {
            if let Some(parent) = self.fullscreen_parent.borrow().as_ref() {
                self.detach(parent)?;
            }
            let index = self
                .parent
                .borrow()
                .children()
                .iter()
                .position(|child| Rc::ptr_eq(child, &self.native));
            if let Some(index) = index {
                if self.native.borrow().parent()?.is_some() {
                    self.parent.borrow_mut().remove_child(index)?;
                } else {
                    self.parent.borrow_mut().children_mut().remove(index);
                }
            } else {
                self.native.borrow().remove_from_parent()?;
            }
        }
        // The controls have independent owners even when hidden. Remove both
        // native and Rust child edges before disposing any of those owners.
        for (parent, node) in [
            (&self.row, &self.toggle),
            (&self.row, &self.fullscreen_toggle),
            (&self.row, &self.current),
            (&self.row, &self.total),
            (&self.row, &self.slider),
            (&self.native, &self.surface),
            (&self.native, &self.row),
        ] {
            if node.borrow().is_owned() {
                node.borrow().remove_from_parent()?;
                parent
                    .borrow_mut()
                    .children_mut()
                    .retain(|child| !Rc::ptr_eq(child, node));
            }
        }
        if self.native.borrow().is_owned() {
            self.native.borrow_mut().dispose()?;
        }
        // Independent Rc owners cover children already popped from a failed
        // parent disposal and controls currently detached from the native Stack.
        for node in [
            &self.surface,
            &self.row,
            &self.toggle,
            &self.fullscreen_toggle,
            &self.current,
            &self.total,
            &self.slider,
        ] {
            if node.borrow().is_owned() {
                node.borrow().remove_from_parent()?;
                node.borrow_mut().dispose()?;
            }
        }
        if self.owner.upgrade().is_none()
            && self.parent.borrow().is_owned()
            && Rc::strong_count(&self.parent) == 1
        {
            self.parent.borrow_mut().dispose()?;
        }
        if let Some(parent) = self.fullscreen_parent.borrow().as_ref()
            && parent.borrow().is_owned()
            && Rc::strong_count(parent) == 1
        {
            parent.borrow_mut().dispose()?;
        }
        self.fullscreen_parent.borrow_mut().take();
        Ok(())
    }
}
pub(crate) fn update(token: u64, state: UiState) {
    let ui = LIVE.with_borrow(|live| live.get(&token).and_then(Weak::upgrade));
    if let Some(ui) = ui.filter(|ui| !ui.closed.get()) {
        callback(|| {
            let was_playing = ui.state.replace(state).playing;
            if was_playing != state.playing {
                let mut visibility = ui.visibility.get();
                visibility.show(state.playing, ui.now());
                ui.visibility.set(visibility);
                ui.apply_visibility()?;
                ui.track_controls();
            }
            if ui.playing.replace(state.playing) != state.playing {
                ui.toggle.borrow().mark_dirty(NodeDirtyFlag::NeedRender)?;
            }
            ui.current
                .borrow()
                .set_string_attribute(A::TextContent, super::icon::time(state.time))?;
            ui.total
                .borrow()
                .set_string_attribute(A::TextContent, super::icon::time(state.duration))?;
            ui.slider
                .borrow()
                .set_f32_attribute(A::SliderMaxValue, state.duration.max(1) as f32)?;
            if !ui.scrubbing.get() {
                ui.updating_slider.set(true);
                let updated = ui
                    .slider
                    .borrow()
                    .set_f32_attribute(A::SliderValue, state.time.max(0) as f32);
                ui.updating_slider.set(false);
                updated?;
            }
            ui.layout()
        });
    }
}
pub(crate) fn retired(token: u64) {
    let ui = RETIRING
        .with_borrow_mut(|retiring| retiring.remove(&token))
        .or_else(|| LIVE.with_borrow(|live| live.get(&token).and_then(Weak::upgrade)));
    if let Some(ui) = ui
        && let Err(error) = ui.complete_retirement()
    {
        log::error!("Video UI release failed; retaining owner: {error}");
        RETIRING.with_borrow_mut(|retiring| {
            retiring.insert(token, ui);
        });
    }
}
// The native Slider event's state uses the SDK SliderChangeMode discriminants:
// End=2 and Click=3. Begin/Moving never commit playback seeks.
fn slider_seek(value: Option<f32>, mode: Option<i32>, programmatic: bool) -> Option<f64> {
    // A tap is Begin, Click, End and a drag Begin, Moving…, End (seen on a
    // phone): End alone seeks, once per gesture.
    if programmatic || mode != Some(2) {
        return None;
    }
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| f64::from(v) / 1000.0)
}
fn picture(size: Size, video: (i32, i32), mode: ResizeMode, scale: f32) -> (f32, f32) {
    if video.0 <= 0 || video.1 <= 0 || mode == ResizeMode::Stretch {
        return (size.width, size.height);
    }
    let (width, height) = (video.0 as f32, video.1 as f32);
    let x = size.width / width;
    let y = size.height / height;
    let mut factor = if mode == ResizeMode::Cover {
        x.max(y)
    } else {
        x.min(y)
    };
    if mode == ResizeMode::Center {
        factor = factor.min(1.0 / scale.max(f32::MIN_POSITIVE));
    }
    (width * factor, height * factor)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_committed_user_slider_changes_seek() {
        // Begin, Moving, Click and unknown modes never seek: a gesture seeks once.
        for mode in [0, 1, 3, 4, -1] {
            assert_eq!(slider_seek(Some(1000.0), Some(mode), false), None);
        }
        assert_eq!(slider_seek(Some(1250.0), Some(2), false), Some(1.25));
        assert_eq!(slider_seek(Some(1250.0), Some(2), true), None);
        for value in [f32::NAN, f32::INFINITY, -1.0] {
            assert_eq!(slider_seek(Some(value), Some(2), false), None);
        }
    }
    #[test]
    fn picture_fit_preserves_cover_contain_center_and_stretch() {
        let size = Size::new(200.0, 100.0);
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Contain, 2.0),
            (100.0, 100.0)
        );
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Cover, 2.0),
            (200.0, 200.0)
        );
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Stretch, 2.0),
            (200.0, 100.0)
        );
        assert_eq!(
            picture(size, (100, 100), ResizeMode::Center, 2.0),
            (50.0, 50.0)
        );
    }
}

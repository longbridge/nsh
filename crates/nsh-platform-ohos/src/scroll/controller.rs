//! ArkUI owns scrolling; core receives absolute offsets and correction counts
//! so its existing virtualized lists can keep their viewport and anchor.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use nsh_core::{EventKind, EventPayload, Point, Points, ScrollBehavior};
use ohos_arkui_binding::{
    common::attribute::ArkUINodeAttributeNumber as N,
    component::{
        attribute::{ArkUICommonAttribute, ArkUIEvent},
        built_in_component::{Custom, Scroll, Stack},
    },
    types::{
        advanced::{LengthMetricUnit, NodeDirtyFlag},
        attribute::ArkUINodeAttributeType as A,
        event::NodeEventType as E,
        scroll::{ScrollDirection, ScrollSource},
    },
};

use crate::{
    render::{
        geometry::Size,
        node::{NativeNode, Node, Result, callback},
    },
    scroll::motion::{ZoomFrame, ZoomMotion, focal_offset, resisted_scale},
};

pub(crate) struct Scroller {
    pub(crate) native: NativeNode,
    pub(crate) wrapper: NativeNode,
    pub(crate) overlay: RefCell<Option<Rc<crate::scroll::overlay::Plane>>>,
    cross: RefCell<Option<NativeNode>>,
    zoom_offset: Cell<Option<(f32, f32, bool)>>,
    pub(crate) content: NativeNode,
    owner: Weak<Node>,
    size: Cell<Size>,
    measured_size: Cell<Size>,
    layout_pending: Cell<bool>,
    viewport_size: Cell<Size>,
    native_width: Cell<f32>,
    offset: Cell<(f32, f32)>,
    reported: super::motion::OffsetReport,
    owed: Cell<(f32, f32)>,
    corrections: Cell<u32>,
    applying_offset: Cell<bool>,
    report_pending: Cell<bool>,
    phase: Cell<i32>,
    scroll_value: Cell<Option<u32>>,
    pull_value: Cell<Option<u32>>,
    pull: Cell<f32>,
    horizontal: Cell<bool>,
    indicator: Cell<bool>,
    mirrored: Cell<bool>,
    keyboard_dismiss: Cell<nsh_core::KeyboardDismissMode>,
    behavior: RefCell<ScrollBehavior>,
    sticky: RefCell<Vec<(nsh_core::StickyCell, Weak<Node>)>>,
    blocked: Cell<bool>,
    refresh_enabled: Cell<bool>,
    refreshing: Cell<bool>,
    refresh: RefCell<super::refresh::Refresh>,
    zoom: Cell<f32>,
    zoom_start: Cell<f32>,
    zoom_motion: Cell<Option<ZoomMotion>>,
    pinch_focal: Cell<(f32, f32)>,
    centering: Cell<(f32, f32)>,
    pinch_position: Cell<(f32, f32)>,
    snap_limit: Cell<Option<f32>>,
    pinch: RefCell<Option<ohos_arkui_binding::gesture::inner_gesture::Gesture>>,
    selection_progress: RefCell<crate::text::selection::scroll::Progress>,
}

impl Scroller {
    pub(crate) fn mounted(&self) -> NativeNode {
        self.overlay
            .borrow()
            .as_ref()
            .map_or_else(|| self.wrapper.clone(), |plane| plane.outer.clone())
    }
    pub(crate) fn prepare_scene(&self, owner: &Rc<Node>) -> Result<()> {
        if self.overlay.borrow().is_none() {
            let plane = crate::scroll::overlay::Plane::new(owner, &self.wrapper)?;
            let index = owner
                .native
                .borrow()
                .children()
                .iter()
                .position(|child| Rc::ptr_eq(child, &self.wrapper))
                .ok_or(crate::render::node::Error::InvalidNode(owner.id))?;
            owner.native.borrow_mut().remove_child(index)?;
            if let Err(error) = owner
                .native
                .borrow_mut()
                .insert_child(plane.outer.clone(), index)
            {
                owner
                    .native
                    .borrow_mut()
                    .insert_child(self.wrapper.clone(), index)?;
                return Err(error.into());
            }
            *self.overlay.borrow_mut() = Some(plane);
        }
        if let Some(plane) = self.overlay.borrow().as_ref() {
            plane.update(owner)?;
        }
        Ok(())
    }
    pub(crate) fn restore_scene(&self, owner: &Node) -> Result<()> {
        let plane = self.overlay.borrow().clone();
        if let Some(plane) = plane {
            let index = owner
                .native
                .borrow()
                .children()
                .iter()
                .position(|child| Rc::ptr_eq(child, &plane.outer))
                .ok_or(crate::render::node::Error::InvalidNode(owner.id))?;
            plane.restore(owner)?;
            owner.native.borrow_mut().remove_child(index)?;
            owner
                .native
                .borrow_mut()
                .insert_child(self.wrapper.clone(), index)?;
            self.overlay.borrow_mut().take();
        }
        Ok(())
    }
    pub(crate) fn new(owner: Weak<Node>) -> Result<Rc<Self>> {
        let native = Scroll::new()?.into_node();
        let content = Custom::new()?.into_node();
        let wrapper = Stack::new()?.into_node();
        wrapper
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        wrapper
            .borrow()
            .set_i32_attribute(A::StackAlignContent, 0)?;
        wrapper.borrow_mut().add_child(native.clone())?;
        native
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        native.borrow().set_i32_attribute(A::Alignment, 0)?;
        content
            .borrow()
            .set_length_metric_unit(LengthMetricUnit::Vp)?;
        content
            .borrow()
            .set_attribute(A::TransformCenter, vec![0.0; 6].into())?;
        native.borrow_mut().add_child(content.clone())?;
        let scroll = Rc::new(Self {
            native,
            wrapper,
            overlay: RefCell::default(),
            cross: RefCell::default(),
            zoom_offset: Cell::new(None),
            content,
            owner,
            size: Cell::new(Size::default()),
            measured_size: Cell::new(Size::default()),
            layout_pending: Cell::new(false),
            viewport_size: Cell::new(Size::default()),
            native_width: Cell::new(f32::NAN),
            offset: Cell::new((0.0, 0.0)),
            reported: super::motion::OffsetReport::default(),
            owed: Cell::new((0.0, 0.0)),
            corrections: Cell::new(0),
            applying_offset: Cell::new(false),
            report_pending: Cell::new(false),
            phase: Cell::new(0),
            scroll_value: Cell::new(None),
            pull_value: Cell::new(None),
            pull: Cell::new(0.0),
            horizontal: Cell::new(false),
            indicator: Cell::new(true),
            mirrored: Cell::new(false),
            keyboard_dismiss: Cell::new(nsh_core::KeyboardDismissMode::None),
            behavior: RefCell::default(),
            sticky: RefCell::default(),
            blocked: Cell::new(false),
            refresh_enabled: Cell::new(false),
            refreshing: Cell::new(false),
            refresh: RefCell::default(),
            zoom: Cell::new(1.0),
            zoom_start: Cell::new(1.0),
            zoom_motion: Cell::new(None),
            pinch_focal: Cell::new((0.0, 0.0)),
            centering: Cell::new((0.0, 0.0)),
            pinch_position: Cell::new((0.0, 0.0)),
            snap_limit: Cell::new(None),
            pinch: RefCell::default(),
            selection_progress: RefCell::default(),
        });
        let weak = Rc::downgrade(&scroll);
        scroll.content.borrow_mut().on_custom_measure(move |event| {
            callback(|| {
                if let Some(scroll) = weak.upgrade()
                    && let Some(owner) = scroll.owner.upgrade()
                {
                    let size = scroll.size.get();
                    event.set_measured_size(
                        (size.width * owner.scale * scroll.zoom.get()).round() as i32,
                        (size.height * owner.scale * scroll.zoom.get()).round() as i32,
                    )?;
                    scroll.measured_size.set(size);
                    owner.measure_children(event)?;
                }
                Ok(())
            })
        });
        let weak = Rc::downgrade(&scroll);
        scroll.content.borrow_mut().on_custom_layout(move |event| {
            callback(|| {
                if let Some(scroll) = weak.upgrade()
                    && let Some(owner) = scroll.owner.upgrade()
                {
                    let position = event.position_in_layout();
                    event.set_layout_position(position.x, position.y)?;
                    owner.layout_children(event)?;
                    if (scroll.layout_pending.get()
                        || scroll.zoom_offset.get().is_some()
                        || scroll.owed.get() != (0.0, 0.0))
                        && let Some(route) = owner.dispatcher.upgrade()
                    {
                        route.send(crate::app::runtime::Message::ScrollPosition {
                            id: owner.id,
                            x: 0.0,
                            y: 0.0,
                            animated: false,
                        });
                    }
                }
                Ok(())
            })
        });
        let weak = Rc::downgrade(&scroll);
        scroll
            .native
            .borrow_mut()
            .on_event(E::ScrollEventOnWillScroll, move |event| {
                callback(|| {
                    if let Some(scroll) = weak.upgrade() {
                        let phase = event.component_i32(2).unwrap_or(0);
                        if phase == 1 {
                            scroll.snap_limit.set(None);
                            scroll.zoom_motion.set(None);
                            scroll.owed.set((0.0, 0.0));
                        }
                        let behavior = scroll.behavior.borrow();
                        let source = event.component_i32(3).unwrap_or(-1);
                        if phase == 2
                            && (source == i32::from(ScrollSource::Fling)
                                || source == i32::from(ScrollSource::ScrollBarFling))
                            && behavior.one_step
                            && behavior.snaps()
                        {
                            let horizontal = scroll.horizontal.get();
                            let (dx, dy) = (
                                event.component_f32(0).unwrap_or(0.0),
                                event.component_f32(1).unwrap_or(0.0),
                            );
                            let delta = if horizontal { dx } else { dy };
                            let offset = if horizontal {
                                scroll.offset.get().0
                            } else {
                                scroll.offset.get().1
                            };
                            if scroll.snap_limit.get().is_none()
                                && let Some(owner) = scroll.owner.upgrade()
                            {
                                let frame = owner.frame.get();
                                let size = scroll.size.get();
                                scroll.snap_limit.set(
                                    behavior
                                        .snap_target(
                                            Points(offset),
                                            Points(offset),
                                            delta.signum(),
                                            Points(if horizontal {
                                                frame.width
                                            } else {
                                                frame.height
                                            }),
                                            Points(if horizontal {
                                                size.width
                                            } else {
                                                size.height
                                            }),
                                        )
                                        .map(|p| p.0),
                                );
                            }
                            if let Some(limit) = scroll.snap_limit.get() {
                                let distance = if delta > 0.0 {
                                    delta.min((limit - offset).max(0.0))
                                } else {
                                    delta.max((limit - offset).min(0.0))
                                };
                                event.set_return_f32_pair(
                                    if horizontal { distance } else { dx },
                                    if horizontal { dy } else { distance },
                                );
                            }
                        }
                    }
                    Ok(())
                });
            });
        let weak = Rc::downgrade(&scroll);
        scroll
            .native
            .borrow_mut()
            .on_event(E::ScrollEventOnScroll, move |_| {
                // Programmatic jumps acknowledge their position synchronously, as on
                // UIKit/Android. Gestures use OnDidScroll once layout settles:
                // ArkUI OnScroll also exposes intermediate animation steps within
                // one frame, which must not race a synchronized peer's commands.
                callback(|| match weak.upgrade() {
                    Some(scroll) if scroll.applying_offset.get() => scroll.report(),
                    Some(_) => Ok(()),
                    None => Ok(()),
                })
            });
        let weak = Rc::downgrade(&scroll);
        scroll
            .native
            .borrow_mut()
            .on_event(E::ScrollEventOnDidScroll, move |event| {
                callback(|| {
                    if let Some(scroll) = weak.upgrade() {
                        scroll.queue_report()?;
                        scroll.phase(event.component_i32(2).unwrap_or(0));
                    }
                    Ok(())
                })
            });
        let weak = Rc::downgrade(&scroll);
        scroll
            .native
            .borrow_mut()
            .on_event(E::ScrollEventOnScrollStop, move |_| {
                callback(|| {
                    if let Some(scroll) = weak.upgrade() {
                        scroll.queue_report()?;
                        scroll.phase(0);
                    }
                    Ok(())
                })
            });
        if let Some(owner) = scroll.owner.upgrade() {
            owner
                .native
                .borrow_mut()
                .add_child(scroll.wrapper.clone())?;
        }
        Ok(scroll)
    }

    pub(crate) fn viewport(&self, size: Size) -> Result<()> {
        let previous = self.viewport_size.get();
        if previous.width != size.width || previous.height != size.height {
            *self.selection_progress.borrow_mut() = Default::default();
            self.refresh.borrow().viewport(size)?;
        }
        if previous.width != size.width {
            self.wrapper.borrow().width(size.width)?;
            if let Some(cross) = self.cross.borrow().as_ref() {
                cross.borrow().width(size.width)?;
            }
        }
        if previous.height != size.height {
            self.wrapper.borrow().height(size.height)?;
            self.native.borrow().height(size.height)?;
            if let Some(cross) = self.cross.borrow().as_ref() {
                cross.borrow().height(size.height)?;
            }
        }
        let width = if self.behavior.borrow().zooms() {
            (self.size.get().width * self.zoom.get()).max(size.width)
        } else {
            size.width
        };
        if self.native_width.get() != width {
            self.native.borrow().width(width)?;
            self.native_width.set(width);
        }
        self.viewport_size.set(size);
        let center = if self.behavior.borrow().center_content {
            (
                (size.width - self.size.get().width * self.zoom.get()).max(0.0) * 0.5,
                (size.height - self.size.get().height * self.zoom.get()).max(0.0) * 0.5,
            )
        } else {
            (0.0, 0.0)
        };
        if self.centering.replace(center) != center {
            self.content
                .borrow()
                .set_attribute(A::Translate, [center.0, center.1, 0.0].into())?;
        }
        Ok(())
    }
    pub(crate) fn size(&self, size: Size) -> Result<()> {
        let previous = self.size.replace(size);
        if previous.width != size.width || previous.height != size.height {
            *self.selection_progress.borrow_mut() = Default::default();
            self.layout_pending.set(true);
        }
        if let Some(owner) = self.owner.upgrade() {
            let frame = owner.frame.get();
            self.viewport(Size::new(frame.width, frame.height))?;
        }
        self.content
            .borrow()
            .mark_dirty(NodeDirtyFlag::NeedMeasure)?;
        self.content
            .borrow()
            .mark_dirty(NodeDirtyFlag::NeedLayout)?;
        Ok(())
    }
    pub(crate) fn mirrored(&self) -> bool {
        self.mirrored.get()
    }
    pub(crate) fn config(
        &self,
        horizontal: bool,
        indicator: bool,
        value: Option<u32>,
        pull_value: Option<u32>,
        keyboard_dismiss: nsh_core::KeyboardDismissMode,
        mirrored: bool,
    ) -> Result<()> {
        self.scroll_value.set(value);
        self.pull_value.set(pull_value.filter(|_| !horizontal));
        self.horizontal.set(horizontal);
        self.indicator.set(indicator);
        self.keyboard_dismiss.set(keyboard_dismiss);
        let changed = self.mirrored.replace(horizontal && mirrored) != (horizontal && mirrored);
        self.native.borrow().set_attribute(
            A::Scale,
            [
                if horizontal && mirrored {
                    -1.0_f32
                } else {
                    1.0
                },
                1.0,
            ]
            .into(),
        )?;
        if changed && let Some(owner) = self.owner.upgrade() {
            for child in owner.children.borrow().iter().filter_map(Weak::upgrade) {
                child.restore_style()?;
            }
        }
        let native = self.native.borrow();
        native.set_i32_attribute(
            A::ScrollScrollDirection,
            i32::from(if horizontal {
                ScrollDirection::Horizontal
            } else {
                ScrollDirection::Vertical
            }),
        )?;
        native.set_i32_attribute(A::ScrollBarDisplayMode, if indicator { 1 } else { 0 })?;
        if let Some(cross) = self.cross.borrow().as_ref() {
            cross
                .borrow()
                .set_i32_attribute(A::ScrollBarDisplayMode, if indicator { 1 } else { 0 })?;
        }
        Ok(())
    }
    pub(crate) fn behavior(self: &Rc<Self>, behavior: &ScrollBehavior) -> Result<()> {
        let native = self.native.borrow();
        native.set_bool_attribute(
            A::ScrollEnableScrollInteraction,
            behavior.enabled && !self.blocked.get(),
        )?;
        native.set_bool_attribute(A::ScrollEnablePaging, behavior.paging && !behavior.snaps())?;
        if behavior.deceleration > 0.0 && behavior.deceleration < 1.0 {
            // ArkUI's fling projection is v/(4.2*friction); the shared rate
            // retains v*rate/(1-rate) milliseconds of travel.
            native.set_f32_attribute(
                A::ScrollFriction,
                (1000.0 * (1.0 - behavior.deceleration) / (4.2 * behavior.deceleration))
                    .clamp(0.01, 100.0),
            )?;
        } else {
            native.reset_attribute(A::ScrollFriction)?;
        }
        native.set_i32_attribute(
            A::ScrollEdgeEffect,
            if behavior.bounces && !self.refresh_enabled.get() {
                0
            } else {
                2
            },
        )?;
        native.set_attribute(
            A::Padding,
            vec![
                behavior.content_inset.top.0,
                behavior.content_inset.right.0,
                behavior.content_inset.bottom.0,
                behavior.content_inset.left.0,
            ]
            .into(),
        )?;
        if behavior.snaps() {
            let align = match behavior.snap_alignment {
                nsh_core::SnapAlignment::Start => 1,
                nsh_core::SnapAlignment::Center => 2,
                nsh_core::SnapAlignment::End => 3,
            };
            let mut snap = vec![N::Int(align), N::Int(1), N::Int(1)];
            if behavior.snap_interval.0 > 0.0 {
                snap.push(N::Float(behavior.snap_interval.0));
            } else {
                snap.extend(behavior.snap_offsets.iter().map(|p| N::Float(p.0)));
            }
            native.set_number_attribute(A::ScrollSnap, snap)?;
        } else {
            native.reset_attribute(A::ScrollSnap)?;
        }
        drop(native);
        *self.behavior.borrow_mut() = behavior.clone();
        self.snap_limit.set(None);
        self.configure_axes()?;
        self.configure_refresh()?;
        self.zoom_to(self.zoom.get(), None, false)?;
        self.configure_pinch()?;
        Ok(())
    }
    pub(crate) fn command_to(&self, x: f32, y: f32, animated: bool) -> Result<()> {
        self.corrections.set(self.corrections.get().wrapping_add(1));
        self.owed.set((0.0, 0.0));
        // Native ScrollOffset accepts overscroll positions. Commands, unlike
        // finger rubber-banding, stay within the content (as on UIKit/Android).
        let size = self.size.get();
        let viewport = self.viewport_size.get();
        let inset = self.behavior.borrow().content_inset;
        let x = x.clamp(
            -inset.left.0,
            (size.width * self.zoom.get() + inset.right.0 - viewport.width).max(-inset.left.0),
        );
        let y = y.clamp(
            -inset.top.0,
            (size.height * self.zoom.get() + inset.bottom.0 - viewport.height).max(-inset.top.0),
        );
        let animated = animated
            && self
                .owner
                .upgrade()
                .is_some_and(|owner| !owner.platform.get().reduce_motion);
        self.zoom_motion.set(None);
        let measured = self.measured_size.get();
        if self.layout_pending.get() || size.width > measured.width || size.height > measured.height
        {
            self.zoom_offset.set(Some((x, y, animated)));
        } else {
            self.zoom_offset.set(None);
        }
        // As ScrollView/UIScrollView do, let the native offset callback report
        // programmatic moves. Reporting here as well sends intermediate positions
        // back to synchronized peers before ArkUI finishes the current frame.
        self.to(x, y, animated && self.zoom_offset.get().is_none())?;

        Ok(())
    }
    pub(crate) fn flush_layout(&self) -> Result<()> {
        self.layout_pending.set(false);
        if let Some((x, y, animated)) = self.zoom_offset.take() {
            self.to(x, y, animated)?;
            self.report()?;
        }
        let (dx, dy) = self.owed.take();
        if dx != 0.0 || dy != 0.0 {
            self.pay_owed(dx, dy)?;
        }
        Ok(())
    }
    pub(crate) fn to(&self, x: f32, y: f32, animated: bool) -> Result<()> {
        let animated = animated
            && self
                .owner
                .upgrade()
                .is_some_and(|owner| !owner.platform.get().reduce_motion);
        let was_applying = self.applying_offset.replace(true);
        let result = (|| {
            if let Some(cross) = self.cross.borrow().as_ref()
                && self.behavior.borrow().zooms()
            {
                cross.borrow().set_number_slice_attribute(
                    A::ScrollOffset,
                    &[
                        N::Float(x),
                        N::Float(0.0),
                        N::Int(if animated { 250 } else { 0 }),
                    ],
                )?;
            }
            self.native.borrow().set_number_slice_attribute(
                A::ScrollOffset,
                &[
                    N::Float(if self.behavior.borrow().zooms() {
                        0.0
                    } else {
                        x
                    }),
                    N::Float(y),
                    N::Int(if animated { 250 } else { 0 }),
                ],
            )?;
            Ok(())
        })();
        self.applying_offset.set(was_applying);
        result
    }
    pub(crate) fn by(&self, dx: f32, dy: f32) -> Result<()> {
        self.corrections.set(self.corrections.get().wrapping_add(1));
        if let Some(mut motion) = self.zoom_motion.get() {
            motion.from.offset.0 += dx;
            motion.from.offset.1 += dy;
            motion.to.offset.0 += dx;
            motion.to.offset.1 += dy;
            self.zoom_motion.set(Some(motion));
            let current = self
                .zoom_offset
                .get()
                .map_or_else(|| self.offset.get(), |(x, y, _)| (x, y));
            return self.apply_zoom(ZoomFrame {
                scale: self.zoom.get(),
                offset: (current.0 + dx, current.1 + dy),
            });
        }
        if let Some((zx, zy, animated)) = self.zoom_offset.get() {
            self.zoom_offset.set(Some((zx + dx, zy + dy, animated)));
            self.to(zx + dx, zy + dy, false)?;
            return self.report();
        }
        let (ox, oy) = self.owed.take();
        self.pay_owed(dx + ox, dy + oy)
    }
    fn pay_owed(&self, dx: f32, dy: f32) -> Result<()> {
        let before = self.offset.get();
        self.native
            .borrow()
            .set_attribute(A::ScrollBy, [dx, dy].into())?;
        let size = self.size.get();
        let measured = self.measured_size.get();
        if (self.layout_pending.get()
            || size.width > measured.width
            || size.height > measured.height)
            && let Some(after) = self.read_native_offset()?
        {
            let rem_x = before.0 + dx - after.0;
            let rem_y = before.1 + dy - after.1;
            self.owed.set((
                if rem_x.abs() > 0.5 { rem_x } else { 0.0 },
                if rem_y.abs() > 0.5 { rem_y } else { 0.0 },
            ));
        }
        self.report()?;
        Ok(())
    }
    fn read_native_offset(&self) -> Result<Option<(f32, f32)>> {
        // Read without allocating; an offset ArkUI does not report is none.
        let Ok(mut offset) = self.native.borrow().f32_pair_attribute(A::ScrollOffset) else {
            return Ok(None);
        };
        if self.behavior.borrow().zooms()
            && let Some(cross) = self.cross.borrow().as_ref()
            && let Ok(x) = cross.borrow().f32_attribute(A::ScrollOffset, 0)
        {
            offset.0 = x;
        }
        Ok(Some(offset))
    }
    fn queue_report(&self) -> Result<()> {
        // Keep native bindings and phase payloads current; only the engine
        // notification waits until the current native UI update has finished.
        if let Some(offset) = self.read_native_offset()? {
            self.offset.set(offset);
            self.pin_sticky()?;
            if let Some(owner) = self.owner.upgrade() {
                owner.invalidate_scene();
                if let Some(value) = self.scroll_value.get() {
                    owner.animator.set(
                        value,
                        if self.horizontal.get() {
                            offset.0
                        } else {
                            offset.1
                        },
                    )?;
                }
                if !self.horizontal.get() && !self.refresh.borrow().is_mounted() {
                    self.pulled((-offset.1).max(0.0))?;
                }
            }
        }
        if !self.report_pending.replace(true)
            && let Some(owner) = self.owner.upgrade()
        {
            let mut pending = owner.scroll_reports.borrow_mut();
            let wake = pending.is_empty();
            pending.push(self.owner.clone());
            drop(pending);
            if !wake {
                return Ok(());
            }
            // OnDidScroll runs within the window's frame. Leave that native
            // update before reading offsets: it can advance several inertia
            // steps, and publishing each one feeds synchronized peers back into
            // one another. No second frame request is needed to settle the turn.
            if let Some(route) = owner.dispatcher.upgrade() {
                route.send(crate::app::runtime::Message::ScrollReports);
            }
        }
        Ok(())
    }
    pub(crate) fn flush_report(&self) -> Result<()> {
        self.report_pending.set(false);
        self.report()
    }
    fn report(&self) -> Result<()> {
        let Some(offset) = self.read_native_offset()? else {
            return Ok(());
        };
        self.offset.set(offset);
        let reported = if let Some((x, y, false)) = self.zoom_offset.get() {
            (x, y)
        } else {
            let (ox, oy) = self.owed.get();
            (offset.0 + ox, offset.1 + oy)
        };
        self.pin_sticky()?;
        // UIKit/Android report a position change once. ArkUI also reports
        // programmatic moves later in OnDidScroll and again in OnScrollStop;
        // replaying those offsets makes synchronized scroll views feed back.
        if !self.reported.update(reported, self.corrections.get()) {
            return Ok(());
        }

        if let Some(owner) = self.owner.upgrade() {
            owner.invalidate_scene();
            owner.control.scroll(
                owner.id,
                Point::new(Points(reported.0), Points(reported.1)),
                self.corrections.get(),
            );
            if let Some(value) = self.scroll_value.get() {
                owner.animator.set(
                    value,
                    if self.horizontal.get() {
                        reported.0
                    } else {
                        reported.1
                    },
                )?;
            }
            if !self.horizontal.get() && !self.refresh.borrow().is_mounted() {
                self.pulled((-reported.1).max(0.0))?;
            }
        }
        Ok(())
    }
    fn pulled(&self, distance: f32) -> Result<()> {
        let pull = distance.max(0.0);
        if self.pull.replace(pull) != pull
            && let Some(value) = self.pull_value.get()
            && let Some(owner) = self.owner.upgrade()
        {
            owner.animator.set(value, pull)?;
        }
        Ok(())
    }
    pub(crate) fn dispose(&self) -> Result<()> {
        self.zoom_motion.set(None);
        use ohos_arkui_binding::component::attribute::ArkUIGesture;
        if let Some(gesture) = self.pinch.borrow_mut().take()
            && self.wrapper.borrow().is_owned()
        {
            self.wrapper.borrow().remove_gesture(&gesture)?;
        }
        self.dispose_refresh()
    }
    fn dispose_refresh(&self) -> Result<()> {
        self.refresh_enabled.set(false);
        self.refreshing.set(false);
        // A failed reparent can leave an optional owner outside the mounted
        // tree. Restore its body before the normal owner-tree disposal.
        let detached = self
            .cross
            .borrow()
            .as_ref()
            .map(|cross| -> Result<_> {
                Ok(cross.borrow().is_owned() && cross.borrow().parent()?.is_none())
            })
            .transpose()?
            .unwrap_or(false);
        if detached {
            let is_body = self
                .cross
                .borrow()
                .as_ref()
                .is_some_and(|cross| Rc::ptr_eq(&self.body(), cross));
            if is_body {
                let cross = self.cross.borrow();
                self.refresh.borrow().wrap(
                    &self.wrapper,
                    &self.native,
                    cross.as_ref().expect("cross-scroll owner"),
                )?;
            } else {
                let cross = self.cross.borrow_mut().take().expect("cross-scroll owner");
                let disposed = cross.borrow_mut().dispose();
                if let Err(error) = disposed {
                    *self.cross.borrow_mut() = Some(cross);
                    return Err(error.into());
                }
            }
        }
        self.refresh
            .borrow_mut()
            .close(&self.wrapper, &self.body())?;
        Ok(())
    }
    pub(crate) fn scale(&self) -> f32 {
        self.zoom.get()
    }
    pub(crate) fn selection_scroll(&self, point: (f32, f32), elapsed: f64) -> Result<bool> {
        let behavior = self.behavior.borrow();
        if !behavior.enabled {
            return Ok(false);
        }
        let size = self.size.get();
        let viewport = self.viewport_size.get();
        let old = self.offset.get();
        let zoom = self.zoom.get();
        let horizontal = behavior.zooms() || self.horizontal.get();
        let vertical = behavior.zooms() || !self.horizontal.get();
        let inset = behavior.content_inset;
        let maximum = (
            if behavior.zooms() {
                self.native_width.get() - viewport.width
            } else {
                size.width * zoom + inset.left.0 + inset.right.0 - viewport.width
            },
            size.height * zoom + inset.top.0 + inset.bottom.0 - viewport.height,
        );
        let axis = |position, extent, current, maximum, enabled| {
            if enabled {
                crate::text::selection::scroll::offset(
                    current,
                    maximum,
                    crate::text::selection::scroll::velocity(position, extent),
                    elapsed,
                )
            } else {
                current
            }
        };
        let target = (
            axis(point.0, viewport.width, old.0, maximum.0, horizontal),
            axis(point.1, viewport.height, old.1, maximum.1, vertical),
        );
        drop(behavior);
        if target == old {
            *self.selection_progress.borrow_mut() = Default::default();
            return Ok(false);
        }
        if !self.selection_progress.borrow_mut().advance(old, target) {
            return Ok(false);
        }
        self.to(target.0, target.1, false)?;
        self.report()?;
        Ok(true)
    }
    pub(crate) fn centering(&self) -> (f32, f32) {
        self.centering.get()
    }
    pub(crate) fn keyboard_taps(&self) -> nsh_core::KeyboardTaps {
        self.behavior.borrow().keyboard_taps
    }
    pub(crate) fn refreshing(self: &Rc<Self>, value: bool) -> Result<()> {
        self.refreshing.set(value);
        self.configure_refresh()
    }
    pub(crate) fn refreshable(self: &Rc<Self>, enabled: bool) -> Result<()> {
        self.refresh_enabled.set(enabled);
        self.configure_refresh()
    }
    fn configure_refresh(self: &Rc<Self>) -> Result<()> {
        let enabled = self.refresh_enabled.get();
        if enabled {
            self.native
                .borrow()
                .set_attribute(A::ScrollNestedScroll, vec![1_i32, 1].into())?;
        } else {
            self.native
                .borrow()
                .reset_attribute(A::ScrollNestedScroll)?;
        }
        self.native.borrow().set_i32_attribute(
            A::ScrollEdgeEffect,
            if self.behavior.borrow().bounces && !enabled {
                0
            } else {
                2
            },
        )?;
        let body = self.body();
        let behavior = self.behavior.borrow();
        let own_indicator = behavior.refresh_indicator.map(|p| p.0);
        let threshold = behavior.refresh_threshold().0;
        drop(behavior);
        let weak = Rc::downgrade(self);
        let offset_weak = Rc::downgrade(self);
        self.refresh.borrow_mut().update(
            &self.wrapper,
            &body,
            super::refresh::Config {
                enabled,
                refreshing: self.refreshing.get(),
                viewport: self.viewport_size.get(),
                own_indicator,
                threshold,
            },
            move |_| {
                if let Some(scroll) = weak.upgrade()
                    && scroll.refresh_enabled.get()
                    && let Some(owner) = scroll.owner.upgrade()
                {
                    owner.emit(EventKind::Refresh, EventPayload::None);
                }
            },
            move |event| {
                callback(|| {
                    if let Some(scroll) = offset_weak.upgrade() {
                        let offset = event
                            .f32_value(0)
                            .or_else(|| event.component_f32(0))
                            .unwrap_or(0.0);
                        scroll.pulled(offset)?;
                    }
                    Ok(())
                });
            },
        )
    }
    fn body(&self) -> NativeNode {
        self.cross
            .borrow()
            .as_ref()
            .filter(|cross| {
                cross
                    .borrow()
                    .children()
                    .iter()
                    .any(|child| Rc::ptr_eq(child, &self.native))
            })
            .cloned()
            .unwrap_or_else(|| self.native.clone())
    }
    pub(crate) fn block(&self, blocked: bool) -> Result<()> {
        *self.selection_progress.borrow_mut() = Default::default();
        self.blocked.set(blocked);
        if let Some(cross) = self.cross.borrow().as_ref() {
            cross.borrow().set_bool_attribute(
                A::ScrollEnableScrollInteraction,
                !blocked && self.behavior.borrow().enabled,
            )?;
        }
        self.native.borrow().set_bool_attribute(
            A::ScrollEnableScrollInteraction,
            !blocked && self.behavior.borrow().enabled,
        )?;
        Ok(())
    }
    pub(crate) fn sticky(&self, cells: Vec<(nsh_core::StickyCell, Weak<Node>)>) -> Result<()> {
        for (_, node) in self.sticky.borrow().iter() {
            if let Some(node) = node.upgrade() {
                node.pin(0.0, 0.0)?;
            }
        }
        let animated = cells.iter().any(|(c, _)| c.offset.is_some());
        *self.sticky.borrow_mut() = cells;
        if let Some(owner) = self.owner.upgrade() {
            owner.animator.set_sticky(&owner, animated);
        }
        self.pin_sticky()
    }
    pub(crate) fn pin_sticky(&self) -> Result<()> {
        let horizontal = self.horizontal.get();
        let offset = self.offset.get();
        let owner = self.owner.upgrade();
        for (cell, node) in self.sticky.borrow().iter() {
            if let Some(node) = node.upgrade() {
                let extra = cell
                    .offset
                    .and_then(|id| owner.as_ref().map(|o| o.animator.value(id)))
                    .unwrap_or(0.0);
                let shift = ((if horizontal { offset.0 } else { offset.1 }) + extra - cell.start.0)
                    .clamp(0.0, (cell.end.0 - cell.start.0).max(0.0));
                node.pin(
                    if horizontal { shift } else { 0.0 },
                    if horizontal { 0.0 } else { shift },
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn zoom_to(
        &self,
        scale: f32,
        center: Option<(f32, f32)>,
        animated: bool,
    ) -> Result<()> {
        let behavior = self.behavior.borrow();
        let scale = scale.clamp(behavior.min_zoom, behavior.max_zoom.max(behavior.min_zoom));
        drop(behavior);
        let old = self.zoom.get();
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        let viewport = owner.frame.get();
        let offset = self.offset.get();
        let (x, y) = center.unwrap_or((
            (offset.0 + viewport.width * 0.5) / old,
            (offset.1 + viewport.height * 0.5) / old,
        ));
        let size = self.size.get();
        let to = ZoomFrame {
            scale,
            offset: (
                focal_offset(x, viewport.width * 0.5, scale, size.width, viewport.width),
                focal_offset(
                    y,
                    viewport.height * 0.5,
                    scale,
                    size.height,
                    viewport.height,
                ),
            ),
        };
        self.animate_zoom(to, animated)
    }
    fn animate_zoom(&self, to: ZoomFrame, animated: bool) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        self.zoom_motion.set(None);
        let offset = self.offset.get();
        if to.scale == self.zoom.get()
            && (to.offset.0 - offset.0).abs() < 0.001
            && (to.offset.1 - offset.1).abs() < 0.001
        {
            return Ok(());
        }
        if animated && !owner.platform.get().reduce_motion {
            self.zoom_motion.set(Some(ZoomMotion {
                from: ZoomFrame {
                    scale: self.zoom.get(),
                    offset: self.offset.get(),
                },
                to,
                start: owner.animator.now(),
            }));
            owner.animator.track_scroll(&owner);
            Ok(())
        } else {
            self.apply_zoom(to)
        }
    }
    pub(crate) fn tick(&self, now: f64) -> Result<bool> {
        let Some(motion) = self.zoom_motion.get() else {
            return Ok(false);
        };
        let (frame, running) = if self
            .owner
            .upgrade()
            .is_none_or(|owner| owner.platform.get().reduce_motion)
        {
            (motion.to, false)
        } else {
            motion.sample(now)
        };
        self.apply_zoom(frame)?;
        if !running {
            self.zoom_motion.set(None);
        }
        Ok(running)
    }
    fn apply_zoom(&self, frame: ZoomFrame) -> Result<()> {
        let Some(owner) = self.owner.upgrade() else {
            return Ok(());
        };
        let old = self.zoom.replace(frame.scale);
        if old == frame.scale {
            self.owed.set((0.0, 0.0));
            let size = self.size.get();
            let measured = self.measured_size.get();
            if self.layout_pending.get()
                || size.width > measured.width
                || size.height > measured.height
            {
                self.zoom_offset
                    .set(Some((frame.offset.0, frame.offset.1, false)));
            } else {
                self.zoom_offset.set(None);
            }
            self.to(frame.offset.0, frame.offset.1, false)?;
            self.report()?;
            return Ok(());
        }
        let content = self.content.borrow();
        content.set_attribute(A::Scale, vec![frame.scale, frame.scale].into())?;
        content.mark_dirty(NodeDirtyFlag::NeedMeasure)?;
        drop(content);
        let viewport = owner.frame.get();
        self.viewport(Size::new(viewport.width, viewport.height))?;
        self.zoom_offset
            .set(Some((frame.offset.0, frame.offset.1, false)));
        owner.control.zoom(owner.id, frame.scale);
        Ok(())
    }
    fn configure_axes(self: &Rc<Self>) -> Result<()> {
        use ohos_arkui_binding::component::attribute::ArkUIEvent;
        let zooms = self.behavior.borrow().zooms();
        if zooms && self.cross.borrow().is_none() {
            let cross = Scroll::new()?.into_node();
            let mounted = (|| -> Result<()> {
                cross
                    .borrow()
                    .set_length_metric_unit(LengthMetricUnit::Vp)?;
                cross.borrow().set_i32_attribute(A::Alignment, 0)?;
                let size = self.viewport_size.get();
                cross.borrow().width(size.width)?;
                cross.borrow().height(size.height)?;
                let weak = Rc::downgrade(self);
                cross
                    .borrow_mut()
                    .on_event(E::ScrollEventOnDidScroll, move |_| {
                        callback(|| {
                            if let Some(scroll) = weak.upgrade() {
                                scroll.queue_report()?;
                            }
                            Ok(())
                        });
                    });
                self.refresh
                    .borrow()
                    .wrap(&self.wrapper, &self.native, &cross)
            })();
            if let Err(error) = mounted {
                // Publish only a successful topology. If rollback or native
                // disposal also fails, retain the candidate for cleanup/retry;
                // body() still selects the node that actually contains Scroll.
                let has_body = !cross.borrow().children().is_empty();
                if has_body {
                    *self.cross.borrow_mut() = Some(cross);
                } else {
                    let cleanup = cross.borrow_mut().dispose();
                    if let Err(cleanup) = cleanup {
                        log::warn!("OHOS cross-scroll cleanup failed: {cleanup}");
                        *self.cross.borrow_mut() = Some(cross);
                    }
                }
                return Err(error);
            }
            *self.cross.borrow_mut() = Some(cross);
        }
        if let Some(cross) = self.cross.borrow().as_ref() {
            self.refresh
                .borrow()
                .wrap(&self.wrapper, &self.native, cross)?;
            cross.borrow().set_i32_attribute(
                A::ScrollBarDisplayMode,
                if self.indicator.get() { 1 } else { 0 },
            )?;
            cross.borrow().set_i32_attribute(
                A::ScrollScrollDirection,
                i32::from(if zooms {
                    ScrollDirection::Horizontal
                } else {
                    ScrollDirection::None
                }),
            )?;
            cross.borrow().set_bool_attribute(
                A::ScrollEnableScrollInteraction,
                !self.blocked.get() && self.behavior.borrow().enabled,
            )?;
        }
        self.native.borrow().set_i32_attribute(
            A::ScrollScrollDirection,
            i32::from(if !zooms && self.horizontal.get() {
                ScrollDirection::Horizontal
            } else {
                ScrollDirection::Vertical
            }),
        )?;
        if let Some(owner) = self.owner.upgrade() {
            let frame = owner.frame.get();
            self.viewport(Size::new(frame.width, frame.height))?;
        }
        Ok(())
    }
    fn configure_pinch(self: &Rc<Self>) -> Result<()> {
        use ohos_arkui_binding::{
            component::attribute::ArkUIGesture,
            gesture::{gesture_data::GestureData, inner_gesture::Gesture},
            types::gesture_event::GestureEventAction as G,
        };
        let wants = self.behavior.borrow().pinch
            && self.behavior.borrow().enabled
            && self.behavior.borrow().zooms();
        if self.pinch.borrow().is_some() == wants {
            return Ok(());
        }
        if let Some(gesture) = self.pinch.borrow_mut().take() {
            self.wrapper.borrow().remove_gesture(&gesture)?;
        }
        if wants {
            let distance = self
                .owner
                .upgrade()
                .map_or(5.0, |owner| 5.0 * owner.scale as f64);
            let gesture = Gesture::create_pinch_gesture(2, distance)?;
            let weak = Rc::downgrade(self);
            gesture.on_gesture(G::Accept | G::Update | G::End | G::Cancel, move |event| {
                callback(|| {
                    if let Some(scroll) = weak.upgrade()
                        && let GestureData::Pinch(pinch) = event.event_action_data
                    {
                        if scroll.blocked.get() {
                            return Ok(());
                        }
                        let Some(owner) = scroll.owner.upgrade() else {
                            return Ok(());
                        };
                        // Gesture centers remain physical pixels even when node input uses vp.
                        let (center_x, center_y) =
                            (pinch.center_x / owner.scale, pinch.center_y / owner.scale);
                        if event.event_action_type == G::Accept {
                            scroll.zoom_motion.set(None);
                            scroll.zoom_start.set(scroll.zoom.get());
                            let offset = scroll.offset.get();
                            let center = scroll.centering.get();
                            scroll.pinch_focal.set((
                                (offset.0 + center_x - center.0) / scroll.zoom.get(),
                                (offset.1 + center_y - center.1) / scroll.zoom.get(),
                            ));
                        }
                        if matches!(event.event_action_type, G::End | G::Cancel) {
                            let behavior = scroll.behavior.borrow();
                            let scale = scroll
                                .zoom
                                .get()
                                .clamp(behavior.min_zoom, behavior.max_zoom.max(behavior.min_zoom));
                            drop(behavior);
                            if scale == scroll.zoom.get() {
                                return Ok(());
                            }
                            let focal = scroll.pinch_focal.get();
                            let position = scroll.pinch_position.get();
                            let viewport = owner.frame.get();
                            let size = scroll.size.get();
                            scroll.animate_zoom(
                                ZoomFrame {
                                    scale,
                                    offset: (
                                        focal_offset(
                                            focal.0,
                                            position.0,
                                            scale,
                                            size.width,
                                            viewport.width,
                                        ),
                                        focal_offset(
                                            focal.1,
                                            position.1,
                                            scale,
                                            size.height,
                                            viewport.height,
                                        ),
                                    ),
                                },
                                true,
                            )?;
                        } else {
                            scroll.pinch_position.set((center_x, center_y));
                            let behavior = scroll.behavior.borrow();
                            let scale = resisted_scale(
                                scroll.zoom_start.get() * pinch.scale,
                                behavior.min_zoom,
                                behavior.max_zoom.max(behavior.min_zoom),
                                behavior.bounces_zoom,
                            );
                            drop(behavior);
                            let size = scroll.size.get();
                            let viewport = owner.frame.get();
                            let focal = scroll.pinch_focal.get();
                            scroll.apply_zoom(ZoomFrame {
                                scale,
                                offset: (
                                    focal_offset(
                                        focal.0,
                                        center_x,
                                        scale,
                                        size.width,
                                        viewport.width,
                                    ),
                                    focal_offset(
                                        focal.1,
                                        center_y,
                                        scale,
                                        size.height,
                                        viewport.height,
                                    ),
                                ),
                            })?;
                        }
                    }
                    Ok(())
                });
            })?;
            self.wrapper
                .borrow()
                .add_gesture_ref(&gesture, None, None)?;
            *self.pinch.borrow_mut() = Some(gesture);
        }
        Ok(())
    }
    fn phase(&self, phase: i32) {
        let previous = self.phase.replace(phase);
        if previous == phase {
            return;
        }
        if let Some(owner) = self.owner.upgrade() {
            let (x, y) = self.offset.get();
            let payload = EventPayload::Scroll(Point::new(Points(x), Points(y)));
            if previous == 1 {
                owner.emit(EventKind::ScrollDragEnd, payload.clone());
            }
            if previous == 2 {
                owner.emit(EventKind::MomentumEnd, payload.clone());
            }
            if phase == 1 {
                if self.keyboard_dismiss.get() != nsh_core::KeyboardDismissMode::None {
                    callback(|| owner.dismiss_keyboard());
                }
                owner.emit(EventKind::ScrollDragStart, payload.clone());
            }
            if phase == 2 {
                owner.emit(EventKind::MomentumStart, payload.clone());
            }
            if phase == 0 {
                owner.emit(EventKind::ScrollEnd, payload);
            }
        }
    }
}

//! Built-in scroll bars: a scroll view or list whose `scrollbar` is `true`
//! once the app draws every bar with a built-in one
//! ([`Core::set_builtin_scrollbars`]), or a theme (made by the UI set that
//! makes bars, [`Registry::scrollbars_from`](crate::element::Registry::scrollbars_from)),
//! shows that bar in place of the platform's indicator.
//!
//! The bar is a component the core makes as a child of the view, last
//! among its children so it draws above the content (what JS appends goes
//! below it), and leaves out of what counts as the view's children
//! (`first:`, `space-*`). It is laid out on its own as large as the view
//! and stays over its viewport as it scrolls: a sticky range across all
//! of the content, which the platforms pin on the main thread as they pin
//! sticky views, so scrolling never waits for the engine.

use std::sync::Arc;

use super::{Core, Key, Kind, list::StickyRange};
use crate::{NodeId, ScrollbarSetting, element::BuiltInScrollbar, text::TextEngine};

/// How far a bar's sticky range reaches: past any content.
const BAR_RANGE: f32 = 1.0e9;

impl<M: TextEngine> Core<M> {
    /// The app draws the bar of every view whose `scrollbar` is `true` with
    /// the built-in one the source makes of the theme, or (`None`) with the
    /// platform's indicator. The bars made for `true` are made again: the
    /// new theme may give each view another bar, or none.
    pub(super) fn set_builtin_scrollbars(
        &mut self,
        bars: Option<(crate::element::ScrollbarSource, crate::element::AnyValue)>,
    ) {
        self.builtin_scrollbars = bars;
        let views: Vec<Key> = (self.nodes.iter())
            .filter(|(_, node)| node.scroll.is_some())
            .map(|(key, _)| key as Key)
            .collect();
        for key in views {
            let scroll = self.nodes[key as usize].scroll.as_ref().expect("scroll");
            if scroll.bar.is_some() && scroll.scrollbar == ScrollbarSetting::Shown {
                self.release_scrollbar(key);
                self.mark(key, super::dirty::SCROLL);
            }
            self.scrollbars_pending.push(key);
        }
    }

    /// Makes the built-in bars of the views waiting for one and releases
    /// those no longer wanted (before the components render: the new ones
    /// render in the same commit).
    pub(super) fn sync_scrollbars(&mut self) {
        let mut pending = std::mem::take(&mut self.scrollbars_pending);
        pending.sort_unstable();
        pending.dedup();
        let themed = (self.scrollbar_source()).or(self.builtin_scrollbars.as_ref().map(|b| b.0));
        for key in pending {
            let Some(node) = self.nodes.get(key as usize) else {
                continue;
            };
            let Some(scroll) = node.scroll.as_ref() else {
                continue;
            };
            // A list in its parent's viewport scrolls with that view.
            let own = matches!(node.kind, Kind::Scroll | Kind::List)
                && node.list.as_ref().is_none_or(|l| !l.parent);
            let (wanted, source) = match &scroll.scrollbar {
                _ if !own => (None, None),
                ScrollbarSetting::Shown => {
                    (Some(None), self.builtin_scrollbars.as_ref().map(|b| b.0))
                }
                ScrollbarSetting::Themed(theme) => (Some(Some(theme.clone())), themed),
                ScrollbarSetting::Hidden => (None, None),
            };
            let wanted = wanted.filter(|_| source.is_some());
            let had = scroll.bar.as_ref().map(|(_, theme)| theme.clone());
            if had == wanted {
                continue;
            }
            if had.is_some() {
                self.release_scrollbar(key);
            }
            if let (Some(own), Some(source)) = (wanted, source) {
                self.make_scrollbar(key, own, source);
            }
            // The platform's indicator shows, or no longer does.
            self.mark(key, super::dirty::SCROLL);
        }
    }

    /// Makes the built-in bar of view `key` of theme `own`, the view's, or
    /// (`None`) the app's.
    fn make_scrollbar(
        &mut self,
        key: Key,
        own: Option<Arc<crate::JsonValue>>,
        source: crate::element::ScrollbarSource,
    ) {
        let node = &self.nodes[key as usize];
        let target = node.id;
        let horizontal = node.scroll.as_ref().is_some_and(|s| s.horizontal);
        let _entered = self.components.runtime.enter();
        let Some(el) = source(&BuiltInScrollbar {
            target,
            horizontal,
            theme: own.clone(),
            app_theme: self.builtin_scrollbars.as_ref().map(|b| b.1.clone()),
        }) else {
            return;
        };
        // A bar from its first render: never one of the view's children.
        let made = self.make_component(el, |core, id| {
            if let Ok(bar) = core.key(id) {
                core.nodes[bar as usize].decoration = true;
            }
        });
        let Some(id) = made else {
            return log::error!("the built-in scroll bar is not a component");
        };
        if let Err(e) = self.insert(target, id, None) {
            log::error!("the built-in scroll bar is not made: {e:?}");
            self.release_instance(id);
            let _ = self.release(id);
            return;
        }
        self.nodes[key as usize]
            .scroll
            .as_mut()
            .expect("scroll")
            .bar = Some((id, own));
        self.decorated.push(key);
        self.sticky_changed = true;
        self.needs_layout = true;
    }

    /// Releases the built-in bar of view `key`.
    pub(super) fn release_scrollbar(&mut self, key: Key) {
        let Some((id, _)) = self.nodes[key as usize]
            .scroll
            .as_mut()
            .and_then(|s| s.bar.take())
        else {
            return;
        };
        self.decorated.retain(|&k| k != key);
        self.sticky_changed = true;
        self.release_instance(id);
        let _ = self.release(id);
    }

    /// Lays the built-in bars out over their views (after the views' own
    /// layout and the lists': a view in a cell has its frame then).
    pub(super) fn layout_scrollbars(&mut self) {
        for i in 0..self.decorated.len() {
            let key = self.decorated[i];
            let node = &self.nodes[key as usize];
            let size = node.frame.size;
            let Some((id, _)) = node.scroll.as_ref().and_then(|s| s.bar.as_ref()) else {
                continue;
            };
            let Ok(bar) = self.key(*id) else { continue };
            let Some(layout) = self.nodes[bar as usize].layout else {
                continue;
            };
            if !node.created || size.width.0.is_nan() || size.height.0.is_nan() {
                continue;
            }
            let mut style = self.layout.style(layout).clone();
            let want = taffy::Size {
                width: taffy::Dimension::length(size.width.0),
                height: taffy::Dimension::length(size.height.0),
            };
            let restyled = style.size != want || style.position != taffy::Position::Relative;
            if restyled {
                style.size = want;
                style.position = taffy::Position::Relative;
                self.layout.set_style(layout, style);
            }
            if restyled || self.layout.dirty(layout) {
                self.layout_root(
                    layout,
                    taffy::Size {
                        width: taffy::AvailableSpace::Definite(size.width.0),
                        height: taffy::AvailableSpace::Definite(size.height.0),
                    },
                );
                self.emit_frames_in(bar, false, taffy::Point::ZERO);
            }
        }
    }

    /// The sticky ranges that keep the built-in bars over their views'
    /// viewports, by view.
    pub(super) fn scrollbar_ranges(&self) -> impl Iterator<Item = (Key, StickyRange)> + '_ {
        self.decorated.iter().filter_map(|&key| {
            let (cell, _): &(NodeId, _) = self.nodes[key as usize].scroll.as_ref()?.bar.as_ref()?;
            Some((
                key,
                StickyRange {
                    cell: *cell,
                    start: 0.0,
                    end: BAR_RANGE,
                    offset: None,
                },
            ))
        })
    }
}

# Scrolling and lists

- **Scrolling**: `pagingEnabled`, `snapToInterval` / `snapToOffsets` / `snapToAlignment`, `disableIntervalMomentum`,
  `decelerationRate`, `scrollable`, `bounces`, `contentInset`, `contentOffset`, `keyboardShouldPersistTaps`
  (`never` / `handled` / `always`), `onScroll` (+ `scrollEventThrottle`; at most once per engine turn), `onScrollDragStart` /
  `EndDrag`, `onMomentumStart` / `End` (a glide caught by a new drag ends with `onMomentumEnd`). Snapping is decided
  by `ScrollBehavior::snap_target` (core) on iOS and its Java port on Android, from the release offset, the projected glide and
  the velocity. Lists keep the visible rows in place when items are inserted above them (`splice`).
- **Input**: fingers (and pens) drag scroll views; on the desktop backends and the web the mouse never does (dragging
  the content is a touchscreen's way to scroll): it scrolls with the wheel, the touchpad, the keyboard and the
  indicator (a scroll bar), and its drags are the app's (pans, responders, text selection). On Linux, Windows and desktop Web, the indicator shows while scrolling or when the pointer reaches its draggable thumb;
  moving over content leaves its visibility and fade timer unchanged. macOS follows the system's scroll-bar
  visibility preference (`WhenScrolling` suppresses content-hover reveal). Horizontal
  views keep theirs with `scrollbar={false}` (the platforms' `scroll::config`; on the web `barShown`, in the
  desktop idiom): a mouse without a horizontal wheel has no other way to move them. With a
  mouse alone, pull to refresh needs a touchpad that rubber-bands (macOS): apps offer another way on the desktop.
- **Content container** (`<Scroll>`): `contentStyle` / `contentClass` are the renderer's
  (`dom.rs` `scroll_content`): the first one set makes a view the scroll view's children move into (also the ones
  already there; later inserts and removals go to it), and styles it. The core lays a scroll view's children out in
  the scroll view itself, so without it nothing changes (no extra view).
- **Zooming** (`<Scroll>`, not lists): `maximumZoomScale` above `minimumZoomScale` makes the content zoomable; `zoomScale`
  zooms when it changes, `bouncesZoom`, `zoomable`, `centerContent` (content smaller than the viewport is centered);
  ref `zoomTo({ scale, x, y, animated })` (content point `x`, `y` in the middle); `onScroll` reports `zoomScale` and the zoomed
  content size. iOS: the children live in a `NshZoomView` (`zoom.rs`) that UIKit scales (`viewForZoomingInScrollView:`), its
  bounds the unzoomed content size, so the core's frames stay valid; texts and canvases are drawn again at the zoomed
  resolution when a zoom ends. Android has no zooming scroll view: `ScrollBox` switches to its `ZScroll`, which scales the
  content around its top left and scrolls both ways itself (ScaleGestureDetector around the focal point, panning with any
  number of fingers, OverScroller flings, rubber bands past the edges and the zoom limits). Pinches start only after
  Android's minimum span (about 27 mm between the fingers). Zoom reports go to the core (`EngineControl::zoom`).
- **List** props: `each`, `estimatedItemSize`, `numColumns`, `inverted` (chat: item 0 at the bottom, starts at the end and keeps its
  distance from the end as content changes), `maintainVisibleContentPosition` (`autoscrollToBottomThreshold`: a list
  that is not inverted follows its end while within that distance of it, `ListState::anchor_end`; scrolled up, growth
  below the viewport moves nothing; Rust components: `ListLayout::follow_end`, on their own lists and the lists they place), `stickyHeaderIndices` (one column, not inverted), `header` /
  `footer` / `empty` / `separator` (a component or an element), `contentStyle`
  (padding and gaps, numbers) / `contentClass` (`p-*`, `gap-*`), `initialScrollIndex`, `onLoadMore` + `loadMoreThreshold`, `onViewableItemsChange` +
  `viewabilityConfig` (`itemVisiblePercentThreshold` / `viewAreaCoveragePercentThreshold` / `minimumViewTime`), `keyExtractor`,
  `refreshing` / `onRefresh`, `scrollValue`. Every other prop (`testID`, accessibility, `class`, `onLayout`, touch
  props, ...) goes to the scroll view as on a `<Scroll>` (`list.rs` `OWN`: the names are read once, when the list is
  made; their values follow their getters). Positions, columns, sticky travel and viewability are all computed in the core; sticky cells
  are pinned natively on every scroll (`MountItem::StickyCells`: translate by `clamp(offset - start, 0, end - start)`).
- **List scroll commands**: the core resolves a list's `scrollToEnd` / `scrollToIndex` to a `ScrollTo` with the sizes it
  knows, which are estimates for rows never rendered; the target stays pinned (`ListState::follow_pin`), and as the rows
  around it are measured the list follows it (a `ScrollBy`, or a retargeted `ScrollTo` while an animated scroll is in
  flight) until the rows in the viewport are all measured, the user scrolls away or the items change. Platforms only
  apply what they get.
- **List internals** (the window, binding cells, recycling): `crates/nsh-core/src/core/list.rs`.
- **Rust components**: the same props are `Scrollable`'s methods (`scroll()`, `virtual_list(..)`) and
  `VirtualList`'s (header, footer, empty view, separators, sticky rows, viewability); `docs/dev/components.md`.

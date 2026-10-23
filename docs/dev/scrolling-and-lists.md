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
  below the viewport moves nothing; Rust components: `ListLayout::follow_end`, on their own lists and the lists they place), `stickyHeaderIndices` (one column, not inverted) + `stickyHeaderOffset` (where they stick: the core subtracts it from the sticky ranges, so platforms pin as they do without one), `header` /
  `footer` / `empty` / `separator` (a component or an element), `contentStyle`
  (padding and gaps, numbers) / `contentClass` (`p-*`, `gap-*`), `initialScrollIndex`, `onLoadMore` + `loadMoreThreshold`, `onViewableItemsChange` +
  `viewabilityConfig` (`itemVisiblePercentThreshold` / `viewAreaCoveragePercentThreshold` / `minimumViewTime`), `keyExtractor`,
  `refreshing` / `onRefresh`, `scrollValue`. Every other prop (`testID`, accessibility, `class`, `onLayout`, touch
  props, ...) goes to the scroll view as on a `<Scroll>` (`list.rs` `OWN`: the names are read once, when the list is
  made; their values follow their getters). Positions, columns, sticky travel and viewability are all computed in the core; sticky cells
  are pinned natively on every scroll (`MountItem::StickyCells`: translate by `clamp(offset - start, 0, end - start)`);
  the web, whose scroll events trail the browser's scrolling by a frame, puts each in a track spanning that travel
  instead and lets the browser's `position: sticky` keep it pinned (`nsh.js` `op_sticky`).
- **List scroll commands**: the core resolves a list's `scrollToEnd` / `scrollToIndex` to a `ScrollTo` with the sizes it
  knows, which are estimates for rows never rendered; the target stays pinned (`ListState::follow_pin`), and as the rows
  around it are measured the list follows it (a `ScrollBy`, or a retargeted `ScrollTo` while an animated scroll is in
  flight) until the rows in the viewport are all measured, the user scrolls away or the items change. Platforms only
  apply what they get.
- **Lists in their parent's viewport** (`viewport="parent"`, `ListLayout::viewport(ListViewport::Parent)`; `own`,
  the default, is a scroll view of its own): fixed when the list's native view is made (a change in the batch that
  makes it counts; later ones warn once). The list mounts as a plain view clipping its cells, its Taffy node gets a
  layout-only child (the spacer) `content_len` long, so it lies in its parent's layout at its full length (padding,
  border and size limits as in a browser) and the scroll view around it covers it; a change of length lays the page
  out again in the same commit (`Core::commit`'s rounds). Its view is the nearest scroll view or own list around it
  on its axis, else the root view it is in (screen, modal, overlay, ...), found again when views move in a tree with
  such lists; the scroll view keeps its lists (`ScrollState::dependents`) and marks them for an update when it
  scrolls. The list's `ListState` gets the view's offset less where the list starts in its content (frames summed up
  to the view, plus the list's border and padding; negative while the list is below the viewport's start) and the
  view's viewport, so the window, `onLoadMore`, viewability and pins run unchanged in list coordinates; lists in
  the cells of other lists update after them. Anchoring is a browser's: the view moves only for rows measured or
  spliced wholly above its viewport's top while the list's start is scrolled past; `scrollToIndex`,
  `scrollToEnd` and `initialScrollIndex` scroll the view, clamped to its content. Those scrolls are summed per view
  and sent at the end of the commit (one `ScrollBy`, growing content first, shrinking after), counted with the
  view's own (`ScrollState::issued`; platforms count every `ScrollBy` and `ScrollTo` from the view's creation) and
  replayed over stale reports, as a list does its own; scroll commands for a list not placed in its view yet wait
  for the update that places it. The scroll view's props (`onScroll` and the other scroll events, `scrollValue`,
  `refreshing`, paging and snapping, `contentInset`, `scrollbar`, `scrollable`, `keyboardDismissMode`) and what
  needs a scroll view of the list's own (`inverted`, `maintainVisibleContentPosition`) are ignored there, each
  warned of once; a zoomable scroll view around it is not supported. Sticky rows stick to the view's viewport:
  their ranges are in the view's offsets, the last one stopping where its bottom meets the end of the list's
  content (CSS `position: sticky` in its container), and the view gets one `StickyCells` with all its lists' (and
  its own, when it is a list). The cells are not the view's children, so the core keeps them on top among their
  list's cells itself (it moves a cell that became sticky last with an `Insert`, and puts new cells below them);
  the platforms find them by id (the desktops' `with_cell` also looks below the view's children while a batch is
  applied), and the web hosts a cell's track in its list, offset by where the list is in the view's content
  (`trackOrigin`; views clip with `overflow: clip`, which lets `position: sticky` reach the scroll view). The web
  moves a smooth scroll of the core's in flight with a `ScrollBy` (`op_sb`), as the native backends move theirs:
  setting the offset would stop it. Lists of their own pay nothing:
  every step is reached through a list in parent mode, and a plain scroll view checks an empty list
  (`crates/nsh-core/src/core/parent_viewport.rs`).
- **List internals** (the window, binding cells, recycling): `crates/nsh-core/src/core/list.rs`.
- **Paced binding** (the runtime's lists): the window keeps a viewport of overscan on either side, and a fast scroll or
  a jump assigns a screen of cells at once, each bind rebuilding a cell's subtree. What shows binds in the commit that
  assigns it: the viewport, two scroll steps ahead of it, sticky items and cells whose item moved (a splice tells them
  their new index). The rest of the overscan waits in the core (`Slot::requested`: what JS was last asked to show, so
  nothing stale is queued and cells whose items went are never asked for) and the engine binds it as its per-turn
  budget allows (`PACED_BIND_OPS` ops or `PACED_BIND_TIME` of JS, at least one cell; nearest the leading edge first,
  one cell per list in turn), then on the next display frames (`MountSink::request_frame`). Cells assigned but not
  bound yet are moved out of sight like unassigned ones. Rust components' lists bind in the core, unpaced.
- **Rust components**: the same props are `Scrollable`'s methods (`scroll()`, `virtual_list(..)`) and
  `VirtualList`'s (header, footer, empty view, separators, sticky rows, viewability); `docs/dev/components.md`.

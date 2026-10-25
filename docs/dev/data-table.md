# DataTable

`DataTable` (`nsh/base`, `crates/nsh-base/src/data_table/`) is a table that scrolls both ways over any number of rows
and columns: only the cells in view (and a reach around them) exist as views. Its API follows gpui-component's
`DataTable` (columns, selection of rows, columns or cells, sorting, resizing, moving, fixed columns, loading more,
context menus, keyboard) and is adapted to NSH where that one is not: the rows are data the app hands over (`data`,
`dataStart`, `rowCount`), not a delegate the table calls; sorting is the app's (the table reports the next sort and
draws the one the app sets); paddings are the app's classes. As `Editor` and `DiffView` do, it has a default look
from the theme's tokens (an exception to "No styles in NSH", `AGENTS.md`), every part of which the app restyles. The
user-facing page is `website/elements/data-table.md`; how each module works inside is in its `//!` docs.

| File | What |
|---|---|
| `mod.rs` | The component: `DataTableProps`, `DataTableEvent`, `DataTableApi` (the ref's methods); the values: `DataTableAlign`, `DataTableSortDirection` (`PropEnum`s), `DataTableColumn`, `DataTableColumnGroup`, `DataTableSort`, `DataTableSelection`, `DataTableRange`, `DataTableCell`, ... (`JsObject`s) |
| `parts.rs` | The parts the app styles: one `parts!` list makes `DataTableClasses`, `DataTableStyles` and `DataTablePart` |
| `data.rs` | `DataTableRows`: the rows from JS, arrays or objects by the columns' keys, and each field as a `Cell` |
| `axis.rs` | `Axis`: one axis's real extent against the extent the scroll views get (see "Huge tables") |
| `columns.rs` | `Columns`: the columns' order, widths and prefix sums, the fixed ones, finding the column at an x, resizing, moving |
| `selection.rs` | `Selection`, `Selectable` (gpui-component's `rowSelectable`, `columnSelectable`, `cellSelectable`), and `Nav`: what a key moves the selection to |
| `controller.rs` | `Dt` and `State`: props and the ref's work, revisions bindings subscribe to, presses, keys, resizing, moving, fitting, copying |
| `view.rs` | The view, in layers (below) |

## Layers

The table is a hit layer, a body and a header (`view.rs`):

- the *hit layer* is a vertical scroll view of a horizontal one of an empty box of the content's size. The platforms
  scroll it as they scroll any scroll view (inertia, scroll bars, wheel, touch), and its box takes presses, hovers and
  the context menu;
- the *body* over it receives no pointer and draws the cells of the rows and columns in view, placed where the table
  last scrolled to. Its regions are the row headers, the fixed columns (which move with the rows only) and the scrolling
  columns (which move both ways). The cells a scroll needs come with the next commit, which moves the body with them;
- the *header* above it is interactive: group labels, titles, resize handles and the drop line of a column being moved.

Rows come from `RowWindow` (`window.rs`, `reaching(margin, ahead)`), columns from the same with a reach of 1 and 3, so
a scroll inside the window made commits a few mount items only.

The rows and the scrolling columns are *slots* (`Slots`, `view.rs`): views made once, each showing the item of the
window whose index leaves its slot number when divided by their count (the most the window has held). When the window
moves by `k` rows, `k` row slots take other rows: their texts and places change, and no view is made or dropped while
scrolling, however far. This is the retained tree's form of gpui-component's `uniform_list`, which draws the rows in
view each frame: the cost of a scroll is the cells that change, never the views that would have to be built.

## Huge tables

Platforms keep scroll offsets and content sizes in 32-bit floats, so ten million rows (a quarter of a billion points)
cannot be a scroll view's content. Past `MAX_EXTENT` (3 000 000 points) the scroll view gets that extent and its offset
is scaled to the content's (`Axis::ratio`); the table finds the rows from the scaled offset and places them relative to
the first row in view, so what is drawn is exact at any size and the scroll bar still spans the content. The columns
work the same way on the horizontal axis. Columns hold prefix sums, so finding the column at an x is a binary search.

## Columns' own classes, fitting, bounce

A column's `cellClass` and `headerClass` come after the part's classes (`column_parts`, `view.rs`). Two presses of a
resize handle that moved less than `MOVE_SLOP`, within `DOUBLE_PRESS` (400 ms), fit the column (`Dt::resize_end`).
`bounces` sets the bounce of both scroll views.

## Data

The app holds as many rows as it likes: `rowCount` is the table's length and `data` holds the rows from `dataStart`.
`onVisibleRowsChange` reports the rows in view, so the app serves the page around them; rows outside `data` show as
empty until it arrives. The demo in the gallery (`DataTableDemo.tsx`) serves a million rows of sixty columns in pages
of 400.

## Live data

Rises and falls take their colors natively: a column's `positiveClass` / `negativeClass` apply to the cells whose
number is above / below zero, or whose text starts with `+` / `-` (`Cell::sign`), with no call into JS per cell. The
gallery's live quotes (`DataTableQuotes.tsx`, as gpui-component's data table story: 5 000 or 100 000 stocks, 41
columns, a third of the rows in view moving every 33 ms) format and hand over only the rows in view and a reach around
them, so a tick costs what is on screen.

## Performance

Headless, debug build, the gallery's demo (`nsh test examples/gallery/App.tsx --trace`): a scroll frame inside the
window made commits 4 mount items in ~1 ms; a window shift (row slots taking other rows) ~350 items in ~3 ms, with no
views made (before the slots: ~2000 items in ~14 ms, every row of the window rebuilt); the first window ~4000 items. A tick of the live quotes commits ~44 items in a median 1.6 ms (at most 2.4 ms).

In a GTK window (`nsh test examples/gallery/App.tsx --target linux --fps`, release, 60 Hz): scrolling the million
rows and wheeling the horizontal bar hold 60 of 60 frames, ~2.8 ms a frame on average, none over budget; the live
quotes redraw at their tick (30 frames a second at 33 ms) and at 60 while scrolling during ticks, ~3 ms a frame.

## Keyboard focus

The table is one stop of Tab, as gpui-component's (a `tab_stop` focus handle): its hit layer's content box, whose
`onFocus` / `onBlur` set the table's focus (`Dt::set_focus`), which binds its keys; its rows, titles and sort buttons
are `focusable(false)`. While it has the keyboard Tab moves the selection, as gpui-component's, and Escape gives it
back. A press, `focus()` and `blur()` set the same flag. Tab stops at pressables on macOS and the web.

## Compared with gpui-component

The API is gpui-component's `DataTable` / `TableState` / `TableDelegate` as data: the delegate's methods are props
(`columns`, `data`, `rowCount`, `renderCell`, `renderHeaderCell`, `groups`, `empty`, `loadingView`), its callbacks
events (`onSortChange` for `perform_sort`, `onColumnMove` for `move_column`, `onLoadMore`, `onVisibleRowsChange`,
`onVisibleColumnsChange`, `onContextMenu` for `context_menu`, whose menu is the app's `Menu`), and `TableState`'s
methods the ref's. Deliberate differences:

- The sort cycle is unsorted, ascending, descending (gpui-component's starts descending): the order users expect.
- `onColumnResize` reports the column's key and width at every step of a drag (gpui-component sends every width,
  keyless, once at the end).
- `onLoadMore` fires once per number of rows and waits while `loading` (gpui-component's `load_more` comes again on
  every range change, for the delegate to guard).
- One `onSelectionChange` with a `{ row?, column? }` value replaces `SelectRow` / `SelectColumn` / `SelectCell` /
  `ClearSelection`; the arrow keys skip columns that do not select (gpui-component's land on them).
- No strings and no fixed look: the empty and loading slots are the app's, and every part (the sort button's chevrons
  among them) takes the app's classes over the theme's default look (gpui-component draws an inbox icon, a skeleton
  and a translated "Sort by" label).
- `onRowPress` is gpui-component's `render_tr` click handler, for the app's own selections of several rows;
  `rowClasses`, data beside `data`, is `render_tr`'s per-row look without a call into JS per row.
- A fit measures the cells' texts natively; `fitPadding` adds what the app's drawing adds to them (gpui-component
  asks its delegate to measure).


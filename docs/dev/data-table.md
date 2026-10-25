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
| `mod.rs` | The component: `DataTableProps`, `DataTableEvent`, `DataTableApi` (the ref's methods); the values: `DataTableAlign`, `DataTableSelectionMode`, `DataTableSortDirection` (`PropEnum`s), `DataTableColumn`, `DataTableColumnGroup`, `DataTableSort`, `DataTableSelection`, `DataTableRange`, `DataTableCell`, ... (`JsObject`s) |
| `parts.rs` | The parts the app styles: one `parts!` list makes `DataTableClasses`, `DataTableStyles` and `DataTablePart` |
| `data.rs` | `DataTableRows`: the rows from JS, arrays or objects by the columns' keys, and each field as a `Cell` |
| `axis.rs` | `Axis`: one axis's real extent against the extent the scroll views get (see "Huge tables") |
| `columns.rs` | `Columns`: the columns' order, widths and prefix sums, the fixed ones, finding the column at an x, resizing, moving |
| `selection.rs` | `Selection`, and `Nav`: what a key moves the selection to |
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

## Huge tables

Platforms keep scroll offsets and content sizes in 32-bit floats, so ten million rows (a quarter of a billion points)
cannot be a scroll view's content. Past `MAX_EXTENT` (3 000 000 points) the scroll view gets that extent and its offset
is scaled to the content's (`Axis::ratio`); the table finds the rows from the scaled offset and places them relative to
the first row in view, so what is drawn is exact at any size and the scroll bar still spans the content. The columns
work the same way on the horizontal axis. Columns hold prefix sums, so finding the column at an x is a binary search.

## Data

The app holds as many rows as it likes: `rowCount` is the table's length and `data` holds the rows from `dataStart`.
`onVisibleRowsChange` reports the rows in view, so the app serves the page around them; rows outside `data` show as
empty until it arrives. The demo in the gallery (`DataTableDemo.tsx`) serves a million rows of sixty columns in pages
of 400.

## Performance

Headless, debug build, the gallery's demo (`nsh test examples/gallery/App.tsx --trace`): a scroll frame inside the
window made commits 4 mount items in ~1.1 ms; a window shift ~700 items in ~10 ms; a page swap of the demo ~2400 items
in ~22 ms.

//! Layout in a terminal ([`crate::element::Environment::cell`]): the core's
//! own algorithm, in whole Cells, the way ratatui apps are laid out.
//!
//! A GUI layout in points does not survive being put on a grid: its
//! spacing, heights and borders, designed for a screen, cost rows and
//! columns a terminal cannot spare. So in a terminal the layout tree lays
//! out every node with [`compute`] instead of Taffy's algorithms: a box's
//! area is split among its children along one direction, as ratatui's
//! `Layout` splits a `Rect` by constraints, and every box is as big as its
//! content unless it is told to fill. Lengths are whole Cells inside (x in
//! Cell widths, y in rows); the layouts written back are in points (Cells ×
//! the Cell's size), so frames, hits, overlays and scroll views work as
//! anywhere else, and the leaves measure with the functions they always
//! do (text in rows of lines).
//!
//! The rule, per box:
//!
//! - **left out** ([`LeftOut`]): what a terminal cannot show at an icon's
//!   size (icon-sized images, videos, web views, shaders and canvases,
//!   [`ICON`]) is laid out as `display: none`, taking no Cells and no
//!   spacing; larger ones keep their box, blank (a canvas drawn in
//!   braille), so what is around them stays apart; and so are a view with nothing in it smaller than a row
//!   both ways (a status dot, a swatch) and a view whose content is all
//!   left out (an icon's tile), which would be blank Cells; a control
//!   whose content is all left out (an icon button) is its accessibility
//!   label in one row, left out without one;
//! - **block**: a border is one Cell on each side that has one, only on a
//!   card: a container (a box holding views with content that are not
//!   only texts: spacers and bars do not count) padded 12pt or more above
//!   or below, or around two rows of content or more, not a group of
//!   controls (a split button, a toolbar) ([`Insets`]; the
//!   core mounts the other sides without a border); controls, boxes of
//!   text and round boxes (a circle: corners of half its explicit side or
//!   more) never have one. Where the terminal shows fills as bands
//!   ([`Grid::surfaces`]), a filled container ([`Surface`]: a fill above
//!   3% alpha unlike the nearest opaque fill around it, or the page) is a
//!   surface instead: no border unless the app's stands out twice as far
//!   as its fill, a row of padding above and below from 12pt, two columns
//!   each side from 12pt and one from 4pt. Horizontal padding and margins are one
//!   Cell from 8pt, two from 24pt (at most one on a control or a box of
//!   text); vertical padding a row from 24pt (none on a control or a box
//!   of text);
//! - **split** along `flexDirection` (reversed for the `-reverse` ones):
//!   `flexGrow` is ratatui's `Fill(grow)`, every other child its content's
//!   size; a child's explicit width (points, or a percentage) is its
//!   `Length`, its minimum and maximum `Min` and `Max`; a `flexBasis` is
//!   a `Length` along the main axis either way. An explicit height
//!   counts only on boxes without content (charts, spacers) and on scroll
//!   views (as a maximum); a box with content is its content's height.
//!   When the children of a row do not fit, the gaps give first (down to
//!   a column), then icon controls shown as labels (down to three
//!   columns, then out), then those with `flexShrink` down to their
//!   texts' longest words, then all down to theirs, then the shrinking
//!   ones and all of them below, never below one Cell for one with
//!   content: a box never lays a child out across its border. A fill's
//!   text counts whole for the gaps and labels; a negative margin keeps a
//!   column between two in a row; a small box of an explicit width takes
//!   its content's longest words;
//! - **spacing**: a gap between children is one column from above 0, two
//!   from 24pt, in a row, margins added; in a column the gap and the
//!   margins between two children collapse into one blank row when any is
//!   16pt or more, and into none inside a bordered box (a card), whose
//!   border already sets its content apart;
//! - **flex**: `justifyContent` is ratatui's `Flex` (start, center, end,
//!   space between, around, evenly) over the space left; across,
//!   `alignItems` / `alignSelf` stretch (the default) or place the child at
//!   its content's size;
//! - **wrap** (`flexWrap`): lines of children, each split as above;
//! - **absolute** children take no room, placed by their insets (both
//!   of an axis without a size stretch them between); a box that holds
//!   only absolute children (a table's header, its rows) keeps its
//!   explicit height, as it has no rows of content to size it; a small
//!   label (one row, narrower than the sibling, no control in it) pinned
//!   on a row of a sibling's text that is not white space (a badge on an
//!   avatar's initials) is left out, as Cells cannot be seen through;
//!   controls, bars pinned over scroll views and boxes over blank Cells
//!   are placed;
//! - **scroll views** lay their content out unbounded along their axis;
//! - **dividers** the user drags ([`Splitter`]: a `col-resize` or
//!   `row-resize` cursor) are one Cell across their axis, never left out;
//! - **screens and modals** fill what the core sizes them to, their
//!   header's padding taken as given (the nearest row); a **native
//!   component** takes its explicit size, where the platform draws a
//!   terminal rendering or leaves blank Cells; a **text input** keeps a
//!   column after its text for its caret;
//! - **grids** (`display: grid`): columns from the template (`fr` fills,
//!   points are Cells, the rest their content's), rows their content's.

use taffy::{
    AlignContentKeyword as J, AlignItemsKeyword as A, AvailableSpace, CompactLength, Dimension,
    Display, FlexDirection, FlexWrap, GridTemplateComponent, Layout, LayoutInput, LayoutOutput,
    LengthPercentage, LengthPercentageAuto, Line, NodeId, Position, Rect, RepetitionCount,
    RequestedAxis, RunMode, Size, SizingMode, Style,
};

use crate::Color;

/// The Cell size of a terminal surface, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Grid {
    /// A Cell's width: the unit of the x axis.
    pub(crate) width: f32,
    /// A Cell's height: the unit of the y axis.
    pub(crate) height: f32,
    /// The terminal shows a filled box as a band of its own color
    /// (`Environment::cell_surfaces`): a filled container is a surface,
    /// with no border ([`Surface`]).
    pub(crate) surfaces: bool,
    /// The page's color, behind the root: black in a dark app, white in a
    /// light one.
    pub(crate) page: Color,
}

impl Grid {
    /// The grid of `cell`, if its sides are finite and above 0.
    pub(crate) fn new(cell: crate::Size<crate::Points>) -> Option<Grid> {
        let (width, height) = (cell.width.0, cell.height.0);
        (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0).then_some(Grid {
            width,
            height,
            surfaces: false,
            page: Color::BLACK,
        })
    }

    /// This grid, showing filled boxes as bands (`surfaces`) on a dark
    /// page or a light one.
    pub(crate) fn with_surfaces(self, surfaces: bool, dark: bool) -> Grid {
        Grid {
            surfaces,
            page: if dark {
                Color::BLACK
            } else {
                Color(0xffff_ffff)
            },
            ..self
        }
    }

    /// The "device pixels per point" frames are rounded to, per axis: one
    /// Cell is one pixel.
    pub(crate) fn ratio(self) -> Size<f32> {
        Size {
            width: 1.0 / self.width,
            height: 1.0 / self.height,
        }
    }

    /// `points` in the nearest whole columns (an exact half down).
    fn cols(self, points: f32) -> i32 {
        nearest(points / self.width)
    }

    /// `points` in the nearest whole rows (an exact half down).
    fn rows(self, points: f32) -> i32 {
        nearest(points / self.height)
    }

    /// The whole columns that fit in `points`.
    fn fit_cols(self, points: f32) -> i32 {
        ((points / self.width) + EPSILON).floor().max(0.0) as i32
    }

    /// The whole rows that fit in `points`.
    fn fit_rows(self, points: f32) -> i32 {
        ((points / self.height) + EPSILON).floor().max(0.0) as i32
    }

    /// The whole columns `points` takes, rounded up (a measured text).
    fn up_cols(self, points: f32) -> i32 {
        ((points / self.width) - EPSILON).ceil().max(0.0) as i32
    }

    /// The whole rows `points` takes, rounded up.
    fn up_rows(self, points: f32) -> i32 {
        ((points / self.height) - EPSILON).ceil().max(0.0) as i32
    }

    fn x(self, cols: i32) -> f32 {
        cols as f32 * self.width
    }

    fn y(self, rows: i32) -> f32 {
        rows as f32 * self.height
    }

    fn size(self, cells: Cells) -> Size<f32> {
        Size {
            width: self.x(cells.w),
            height: self.y(cells.h),
        }
    }

    fn sides(self, s: Sides) -> Rect<f32> {
        Rect {
            left: self.x(s.l),
            right: self.x(s.r),
            top: self.y(s.t),
            bottom: self.y(s.b),
        }
    }
}

/// What the core knows of a box that its style does not say
/// (`Core::grid_role`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Role {
    /// A control: a control's accessibility role, presses, a text input.
    pub(crate) control: bool,
    /// A control's accessibility label, in columns (0 without one): an
    /// icon button shows it in place of its icon.
    pub(crate) label: u16,
    /// A text or a text input: a box holding only such is a box of text.
    pub(crate) text: bool,
    /// A scroll view or a list: its content is laid out unbounded along
    /// its axis.
    pub(crate) scroll: bool,
    /// The root: its explicit size (the viewport's) is its size.
    pub(crate) root: bool,
    /// A screen or a modal: the core sizes it to its stack or window and
    /// pads it below its header, which it takes as given (whole rows, the
    /// nearest), as the root's.
    pub(crate) fills: bool,
    /// Its explicit size is its size, whatever it holds: a native
    /// component, which draws itself in it; an image, a video, a web view
    /// or a shader, which keeps its box blank; a list, whose cells are
    /// laid out apart, in the size it is given.
    pub(crate) sized: bool,
    /// A disclosure: a control whose `expanded` is set and that opens no
    /// popup (a collapsible's trigger, an accordion's, a tree's row). It
    /// keeps at least two columns before its content for the painter's
    /// `▸` / `▾`.
    pub(crate) disclosure: bool,
    /// A text that starts with a disclosure's mark (`▸`, `▾`) of its own.
    pub(crate) marked: bool,
    /// A text input: a column after its text keeps its caret in the field
    /// when the text fills a line.
    pub(crate) input: bool,
    /// A text with a character that is no white space: what an absolute
    /// box over it would hide.
    pub(crate) glyphs: bool,
    /// Its largest corner radius, in points: a box as round as it is
    /// small (a circle) has no border.
    pub(crate) radius: f32,
    /// Its background color (transparent without one).
    pub(crate) fill: Color,
    /// The color of its first bordered side.
    pub(crate) border_color: Color,
    /// A divider the user drags ([`Splitter`]).
    pub(crate) splitter: Splitter,
    /// Whether a terminal leaves it out.
    pub(crate) left_out: LeftOut,
    /// An overlay: one with nothing in it (a drop preview) keeps its
    /// border, which the painter draws over its outer Cells.
    pub(crate) overlay: bool,
}

/// Whether a terminal leaves a box out: what it cannot show is laid out as
/// `display: none` rather than as a placeholder.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum LeftOut {
    /// It is shown.
    #[default]
    No,
    /// An image, a video, a web view, a shader or a canvas: left out when
    /// it is icon-sized ([`ICON`]); larger, it keeps its box (a canvas
    /// drawn in braille, the others blank), so the labels around it do not
    /// close up over the place it takes.
    IfIcon,
}

/// The largest explicit width and height of an image, a video, a web view,
/// a shader or a canvas a terminal takes for an icon and leaves out, in
/// points.
const ICON: f32 = 48.0;

/// A tolerance for `f32` sums that stand for whole Cells or an exact half.
const EPSILON: f32 = 1.0e-4;

/// Whether a box styled `style` is left out ([`LeftOut`]).
pub(crate) fn left_out(style: &Style, left_out: LeftOut) -> bool {
    match left_out {
        LeftOut::No => false,
        LeftOut::IfIcon => {
            let icon = |d: Dimension| length(d.into_raw()).is_some_and(|v| v <= ICON);
            icon(style.size.width) && icon(style.size.height)
        }
    }
}

/// The tree [`compute`] lays out: the core's layout tree, its children
/// those laid out in a node's place (through holders).
pub(crate) trait Tree {
    fn style(&self, node: NodeId) -> &Style;
    fn role(&self, node: NodeId) -> Role;
    fn child_count(&self, node: NodeId) -> usize;
    fn child(&self, node: NodeId, index: usize) -> NodeId;
    /// Whether it is laid out as a root (it has no parent): the root, a
    /// list's cell.
    fn is_root(&self, node: NodeId) -> bool;
    /// Whether it is measured (a text, a field).
    fn measures(&self, node: NodeId) -> bool;
    /// Its content measured, a leaf's.
    fn measure(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput;
    /// Lays it out (cached; hidden when `inputs` say so).
    fn layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput;
    /// Records its layout, relative to its parent.
    fn place(&mut self, node: NodeId, layout: &Layout);
    /// Whether the nodes being laid out are inside a bordered box (a
    /// card): rows between them are spared there.
    fn framed(&self) -> bool;
    /// Sets [`Tree::framed`] while a bordered box's children are laid out.
    fn set_framed(&mut self, framed: bool);
    /// The border and padding its last full layout gave it.
    fn insets(&self, node: NodeId) -> Insets;
    /// Records [`Tree::insets`].
    fn set_insets(&mut self, node: NodeId, insets: Insets);
    /// The background behind `node`: the nearest opaque fill of a box
    /// around it ([`Role::fill`]), `None` for the page's.
    fn backdrop(&self, node: NodeId) -> Option<Color>;
    /// Where its last layout placed it, relative to its parent.
    fn placed(&self, node: NodeId) -> Layout;
}

/// A box's border and padding in Cells, as its last full layout gave it:
/// whether it keeps its border depends on its content's height, so it is
/// decided once and recorded where it is placed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Insets {
    border: Sides,
    padding: Sides,
}

/// `value` in whole units, the nearest, an exact half down; the sign kept.
fn nearest(value: f32) -> i32 {
    let n = value.abs();
    let whole = (n + EPSILON).floor();
    let units = if n - whole > 0.5 + EPSILON {
        whole + 1.0
    } else {
        whole
    };
    (units as i32) * if value < 0.0 { -1 } else { 1 }
}

/// Horizontal padding or margin of `points`: a Cell from 8pt, two from 24.
fn space_x(points: f32) -> i32 {
    if points >= 24.0 - EPSILON {
        2
    } else if points >= 8.0 - EPSILON {
        1
    } else {
        0
    }
}

/// Vertical padding of `points`: a row from 24pt.
fn space_y(points: f32) -> i32 {
    i32::from(points >= 24.0 - EPSILON)
}

/// Vertical spacing between siblings (a gap, a margin) of `points`: a
/// row from 16pt, none inside a bordered box (`framed`), where the border
/// already sets the content apart.
fn space_between(points: f32, framed: bool) -> i32 {
    i32::from(!framed && points >= 16.0 - EPSILON)
}

/// A gap of `points` between children in a row: a column from above 0,
/// two from 24pt.
fn gap_x(points: f32) -> i32 {
    if points >= 24.0 - EPSILON {
        2
    } else {
        i32::from(points > EPSILON)
    }
}

fn length(raw: CompactLength) -> Option<f32> {
    (raw.tag() == CompactLength::LENGTH_TAG).then(|| raw.value())
}

/// A length or a percentage of `basis`, in points.
fn resolve(raw: CompactLength, basis: Option<f32>) -> Option<f32> {
    match raw.tag() {
        CompactLength::LENGTH_TAG => Some(raw.value()),
        CompactLength::PERCENT_TAG => basis.map(|b| b * raw.value()),
        _ => None,
    }
}

fn lp(v: LengthPercentage, basis: Option<f32>) -> f32 {
    resolve(v.into_raw(), basis).unwrap_or(0.0)
}

fn lpa(v: LengthPercentageAuto, basis: Option<f32>) -> Option<f32> {
    resolve(v.into_raw(), basis)
}

fn dim(v: Dimension, basis: Option<f32>) -> Option<f32> {
    resolve(v.into_raw(), basis)
}

/// A size in Cells.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Cells {
    w: i32,
    h: i32,
}

/// Lengths of a box's four sides, in Cells.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Sides {
    l: i32,
    r: i32,
    t: i32,
    b: i32,
}

impl Sides {
    fn x(self) -> i32 {
        self.l + self.r
    }

    fn y(self) -> i32 {
        self.t + self.b
    }

    fn add(self, o: Sides) -> Sides {
        Sides {
            l: self.l + o.l,
            r: self.r + o.r,
            t: self.t + o.t,
            b: self.b + o.b,
        }
    }
}

/// Whether `node` is out of the layout: `display: none`, left out, or a
/// decoration.
fn hidden(tree: &impl Tree, node: NodeId) -> bool {
    let style = tree.style(node);
    style.display == Display::None
        || left_out(style, tree.role(node).left_out)
        || (tree.role(node).splitter == Splitter::No
            && (decoration(tree, node) || unlabelled_icon(tree, node) || icon_layer(tree, node)))
}

/// Whether a box is a divider the user drags (its cursor `col-resize` or
/// `row-resize`: a resizable panel's handle, a dock's split): it is one
/// Cell across its axis, never left out.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum Splitter {
    /// It is none.
    #[default]
    No,
    /// It moves along x (`col-resize`): one column wide.
    Columns,
    /// It moves along y (`row-resize`): one row high.
    Rows,
}

/// A view that is no control and does not grow whose content is all left
/// out (an icon's tile, circle or layer): it would be blank Cells, so it is
/// left out with it.
fn icon_layer(tree: &impl Tree, node: NodeId) -> bool {
    let role = tree.role(node);
    !role.control
        && !role.root
        && !role.scroll
        && tree.style(node).flex_grow <= 0.0
        && hollow(tree, node)
}

/// A control whose content is all left out (an icon button): it shows its
/// accessibility label in one row instead.
fn icon_control(tree: &impl Tree, node: NodeId) -> bool {
    let count = tree.child_count(node);
    tree.role(node).control && count > 0 && (0..count).all(|i| blank(tree, tree.child(node, i)))
}

/// Whether nothing of `node` would show: it is out of the layout, or a
/// view that is no control whose content is all blank (an icon's layer,
/// one that grows to center it too, as a menu trigger's chevron).
fn blank(tree: &impl Tree, node: NodeId) -> bool {
    if hidden(tree, node) {
        return true;
    }
    let role = tree.role(node);
    let count = tree.child_count(node);
    !role.control
        && !role.text
        && !role.scroll
        && !role.root
        && !tree.measures(node)
        && count > 0
        && (0..count).all(|i| blank(tree, tree.child(node, i)))
}

/// An icon control without a label: nothing of it would show, so it is left
/// out (the actions it opens stay reachable from the box it is in, its
/// context menu or the keyboard's).
fn unlabelled_icon(tree: &impl Tree, node: NodeId) -> bool {
    tree.role(node).label == 0 && icon_control(tree, node)
}

/// A view with nothing in it (no children, nothing measured, no canvas).
fn bare(tree: &impl Tree, node: NodeId) -> bool {
    tree.child_count(node) == 0 && !tree.measures(node) && tree.role(node).left_out == LeftOut::No
}

/// A view with children, all of them out of the layout (an icon's layer).
fn hollow(tree: &impl Tree, node: NodeId) -> bool {
    let count = tree.child_count(node);
    count > 0 && (0..count).all(|i| hidden(tree, tree.child(node, i)))
}

/// The largest circle a terminal leaves out when it holds nothing, in
/// points: an avatar's placeholder (three rows).
const SMALL_CIRCLE: f32 = 48.0;

/// A bare view smaller than a row both ways, not growing (a status dot, a
/// swatch, a tick): it draws nothing in a terminal, so it takes no Cells
/// and no spacing.
fn decoration(tree: &impl Tree, node: NodeId) -> bool {
    if !bare(tree, node) || tree.role(node).root {
        return false;
    }
    let style = tree.style(node);
    let small = |d: Dimension| length(d.into_raw()).is_some_and(|v| v < 16.0 - EPSILON);
    if style.flex_grow > 0.0 {
        return false;
    }
    // A small circle with nothing in it (a radio's ring, an avatar's
    // placeholder): a terminal draws no circle, and its blank Cells would
    // move what follows it by whether it holds a dot. A larger one is a
    // box like any (a band covers its corners).
    let (w, h) = (
        length(style.size.width.into_raw()),
        length(style.size.height.into_raw()),
    );
    let circle = match (w, h) {
        (Some(w), Some(h)) => {
            let role = tree.role(node);
            !role.control
                && w.min(h) <= SMALL_CIRCLE + EPSILON
                && w.max(h) <= w.min(h) * 1.5
                && role.radius * 2.0 >= w.min(h) - EPSILON
        }
        _ => false,
    };
    circle || (small(style.size.width) && small(style.size.height))
}

/// What a box is, as the algorithm sees it.
#[derive(Debug, Clone, Copy)]
struct Kind {
    /// It measures its content (a text, a field).
    measures: bool,
    /// An icon control shown as its label, one row.
    labelled: bool,
    /// Nothing in it and nothing measured: its explicit size is its size.
    empty: bool,
    /// It holds views that are not only texts, and is no control: it
    /// may have a border.
    container: bool,
    /// Its explicit height counts (empty, all its content absolute, an
    /// anchor of no width, or a scroll view's maximum).
    honors_height: bool,
}

fn kind(tree: &impl Tree, node: NodeId) -> Kind {
    let role = tree.role(node);
    let count = tree.child_count(node);
    let mut visible = false;
    let mut in_flow = false;
    let mut boxes = false;
    for i in 0..count {
        let child = tree.child(node, i);
        if hidden(tree, child) {
            continue;
        }
        visible = true;
        in_flow |= !tree.style(child).position.is_out_of_flow();
        // Texts, bare views (spacers, bars) and views whose content is all
        // left out (an icon's circle) make no box of it.
        boxes |= !tree.role(child).text && !bare(tree, child) && !hollow(tree, child);
    }
    let measures = count == 0 && tree.measures(node);
    let labelled = role.label > 0 && icon_control(tree, node);
    let empty = !visible && !measures && !labelled;
    // An absolutely placed line of no width with a height (a chart's
    // anchor down the room around a tick): its height is the room its
    // content is aligned in (centered on the tick), not rows of content.
    let style = tree.style(node);
    let anchor = style.position.is_out_of_flow()
        && length(style.size.width.into_raw()) == Some(0.0)
        && length(style.size.height.into_raw()).is_some();
    Kind {
        measures,
        labelled,
        empty,
        container: boxes && !role.control && !role.text,
        // A box whose content is all absolute (a table's header over its
        // cells, a row of them) has no rows of content to size it: its
        // explicit height is what its children are placed in.
        honors_height: empty
            || anchor
            || (visible && !in_flow)
            || role.scroll
            || role.root
            || role.fills
            || role.sized,
    }
}

/// A box's border and padding, in Cells, within a parent `parent_width`
/// points wide.
fn insets(
    tree: &impl Tree,
    node: NodeId,
    kind: Kind,
    parent_width: Option<f32>,
    grid: Grid,
    surface: Option<Surface>,
) -> (Sides, Sides) {
    let s = tree.style(node);
    let role = tree.role(node);
    if let Some(surface) = surface {
        // A band of its color is its edge: a row of padding above and
        // below from 12pt, two columns each side from 12pt, one from 4pt.
        let has = |v: LengthPercentage| i32::from(surface.border && lp(v, parent_width) > 0.0);
        let border = Sides {
            l: has(s.border.left),
            r: has(s.border.right),
            t: has(s.border.top),
            b: has(s.border.bottom),
        };
        let px = |v: LengthPercentage| {
            let p = lp(v, parent_width);
            if p >= 12.0 - EPSILON {
                2
            } else {
                i32::from(p >= 4.0 - EPSILON)
            }
        };
        let py = |v: LengthPercentage| i32::from(lp(v, parent_width) >= 12.0 - EPSILON);
        let padding = Sides {
            l: px(s.padding.left).max(disclosure_lead(tree, node)),
            r: px(s.padding.right) + i32::from(role.input),
            t: py(s.padding.top),
            b: py(s.padding.bottom),
        };
        return (border, padding);
    }
    // A bare overlay (a dock's drop preview) is its outline: its border is
    // sent for the painter to draw over its outer Cells, its size the
    // app's.
    let outline = role.overlay && kind.empty;
    // An empty box in flow of an explicit size two Cells or more each way
    // (a code cell waiting for its digit, a framed placeholder) is its
    // outline too: its size is its own, so the border adds no Cells. An
    // absolute one (a selection's or an indicator's box over its items)
    // would frame what it lies over, so it takes none.
    let framed_empty = kind.empty && !role.control && !s.position.is_out_of_flow() && {
        let cells = |v: Option<f32>, to: fn(Grid, f32) -> i32| v.is_some_and(|v| to(grid, v) >= 2);
        cells(dim(s.size.width, parent_width), Grid::cols)
            && cells(dim(s.size.height, None), Grid::rows)
    };
    let bordered = (kind.container || outline || framed_empty) && !round(s, role, parent_width);
    let has = |v: LengthPercentage| i32::from(bordered && lp(v, parent_width) > 0.0);
    let border = Sides {
        l: has(s.border.left),
        r: has(s.border.right),
        t: has(s.border.top),
        b: has(s.border.bottom),
    };
    let compact = !kind.container;
    let px = |v: LengthPercentage| {
        let c = space_x(lp(v, parent_width));
        if compact { c.min(1) } else { c }
    };
    let py = |v: LengthPercentage| {
        if role.fills {
            // The core's: a header's height.
            grid.rows(lp(v, parent_width)).max(0)
        } else if compact {
            0
        } else {
            space_y(lp(v, parent_width))
        }
    };
    let padding = Sides {
        l: px(s.padding.left).max(disclosure_lead(tree, node)),
        r: px(s.padding.right) + i32::from(role.input),
        t: py(s.padding.top),
        b: py(s.padding.bottom),
    };
    (border, padding)
}

/// The columns a disclosure (a collapsible's or an accordion's trigger,
/// a tree's row) keeps at least before its content for the painter's `▸`
/// / `▾` and a space: 2 (its padding counts), none when its first text
/// shows a mark of its own.
fn disclosure_lead(tree: &impl Tree, node: NodeId) -> i32 {
    if !tree.role(node).disclosure {
        return 0;
    }
    // A leading empty box two Cells or wider already keeps the columns
    // the mark is drawn in.
    let lead = (0..tree.child_count(node))
        .map(|i| tree.child(node, i))
        .find(|&c| !hidden(tree, c) && !tree.style(c).position.is_out_of_flow());
    if let Some(lead) = lead
        && tree.child_count(lead) == 0
        && !tree.role(lead).text
        && length(tree.style(lead).size.width.into_raw()).is_some_and(|w| w >= 16.0)
    {
        return 0;
    }
    match first_text(tree, node) {
        Some(text) if tree.role(text).marked => 0,
        _ => 2,
    }
}

/// The first text shown in `node`, in tree order (`node` itself if it is
/// one).
fn first_text(tree: &impl Tree, node: NodeId) -> Option<NodeId> {
    if hidden(tree, node) {
        return None;
    }
    let role = tree.role(node);
    if role.text && role.glyphs {
        return Some(node);
    }
    (0..tree.child_count(node)).find_map(|i| first_text(tree, tree.child(node, i)))
}

/// Whether a box styled `s` is as round as it is small (a circle, a
/// pill): its corners' radius at least half its explicit width or height.
/// A box of one or two Cells cannot be round, so it has no border.
fn round(s: &Style, role: Role, parent_width: Option<f32>) -> bool {
    let w = dim(s.size.width, parent_width);
    let h = dim(s.size.height, None);
    let side = match (w, h) {
        (Some(w), Some(h)) => w.min(h),
        (Some(v), None) | (None, Some(v)) => v,
        // A fully round box (`rounded-full`) is round at any size.
        (None, None) => return role.radius >= FULLY_ROUND,
    };
    role.radius > 0.0 && role.radius * 2.0 >= side - EPSILON
}

/// Below this many columns a box of an explicit width is small
/// ([`compute`]): it takes its content's words when they are wider.
const SMALL_BOX: i32 = 6;

/// A corner radius that rounds any box fully (`rounded-full` is 9999).
const FULLY_ROUND: f32 = 1000.0;

/// A filled box (a band of its own color in a terminal that shows them,
/// [`Grid::surfaces`]): it keeps its padding, in rows and columns, and
/// draws no border unless the app's border stands out more than the fill.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Surface {
    /// It keeps its border: its color is hued, or at least twice as far
    /// from the backdrop's lightness as the fill's.
    border: bool,
}

/// The alpha above which a fill counts (8 / 255).
const FILL_ALPHA: f32 = 0.03;

/// Whether a box of role `role` over `backdrop` is a surface.
fn surface(grid: Grid, role: Role, backdrop: Color) -> Option<Surface> {
    let (.., a) = role.fill.to_f64();
    if !grid.surfaces || a as f32 <= FILL_ALPHA {
        return None;
    }
    let shown = blend(role.fill, backdrop);
    let differs = (lightness(shown) - lightness(backdrop)).abs() >= 1.5
        // Its hue is the fill's own, a translucent accent wash's too.
        || (saturated(role.fill) && shown.0 | 0xff != backdrop.0 | 0xff);
    if !differs {
        return None;
    }
    let fill_d = (lightness(shown) - lightness(backdrop)).abs();
    // A hued border (an accent outline, a colored side) is a line of its
    // own color, however light it is.
    let border = !role.border_color.is_transparent()
        && ((lightness(blend(role.border_color, backdrop)) - lightness(backdrop)).abs()
            >= 2.0 * fill_d
            || saturated(role.border_color));
    Some(Surface { border })
}

/// `top` over the opaque `under`.
fn blend(top: Color, under: Color) -> Color {
    let (r, g, b, a) = top.to_f64();
    let (ur, ug, ub, _) = under.to_f64();
    let mix = |t: f64, u: f64| ((t * a + u * (1.0 - a)) * 255.0).round() as u8;
    Color::rgba(mix(r, ur), mix(g, ug), mix(b, ub), 0xff)
}

/// CIE L* of `c`'s color, 0 to 100.
fn lightness(c: Color) -> f32 {
    let (r, g, b, _) = c.to_f64();
    let lin = |v: f64| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let y = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
    let l = if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    };
    l as f32
}

/// Whether `c` is saturated: its channels 51 (a fifth) or more apart.
fn saturated(c: Color) -> bool {
    let v = c.0;
    let ch = [(v >> 24) & 0xff, (v >> 16) & 0xff, (v >> 8) & 0xff];
    ch.iter().max().unwrap_or(&0) - ch.iter().min().unwrap_or(&0) >= 51
}

/// A box's margins, in Cells (auto and negative margins are none),
/// inside a bordered box or not (`framed`).
fn margins(style: &Style, parent_width: Option<f32>, framed: bool) -> Sides {
    let x = |v| lpa(v, parent_width).map_or(0, space_x);
    let y = |v| lpa(v, parent_width).map_or(0, |p| space_between(p, framed));
    Sides {
        l: x(style.margin.left),
        r: x(style.margin.right),
        t: y(style.margin.top),
        b: y(style.margin.bottom),
    }
}

/// `value` within `min` and `max`.
fn clamp(value: i32, min: Option<i32>, max: Option<i32>) -> i32 {
    let v = max.map_or(value, |m| value.min(m));
    min.map_or(v, |m| v.max(m)).max(0)
}

fn definite(space: AvailableSpace) -> Option<f32> {
    match space {
        AvailableSpace::Definite(v) => Some(v),
        _ => None,
    }
}

fn input(
    run_mode: RunMode,
    known: (Option<i32>, Option<i32>),
    avail: (Option<i32>, Option<i32>),
    parent: (Option<i32>, Option<i32>),
    grid: Grid,
) -> LayoutInput {
    let space = |v: Option<f32>| v.map_or(AvailableSpace::MaxContent, AvailableSpace::Definite);
    LayoutInput {
        run_mode,
        sizing_mode: SizingMode::InherentSize,
        axis: RequestedAxis::Both,
        known_dimensions: Size {
            width: known.0.map(|c| grid.x(c)),
            height: known.1.map(|c| grid.y(c)),
        },
        known_dimensions_are_definite: Size {
            width: true,
            height: true,
        },
        parent_size: Size {
            width: parent.0.map(|c| grid.x(c)),
            height: parent.1.map(|c| grid.y(c)),
        },
        available_space: Size {
            width: space(avail.0.map(|c| grid.x(c))),
            height: space(avail.1.map(|c| grid.y(c))),
        },
        vertical_margins_are_collapsible: Line::FALSE,
    }
}

/// Lays `node` out in a terminal within `inputs` (in points, whole Cells
/// of `grid`): the module's docs.
pub(crate) fn compute(
    tree: &mut impl Tree,
    node: NodeId,
    inputs: LayoutInput,
    grid: Grid,
) -> LayoutOutput {
    let role = tree.role(node);
    let k = kind(tree, node);
    let parent_w = inputs.parent_size.width;
    let parent_h = inputs.parent_size.height;
    let surface = k
        .container
        .then(|| surface(grid, role, tree.backdrop(node).unwrap_or(grid.page)))
        .flatten();
    let (border, padding) = insets(tree, node, k, parent_w, grid, surface);
    let inset = border.add(padding);
    let s = tree.style(node);
    let ew = dim(s.size.width, parent_w).map(|v| grid.cols(v).max(0));
    let min_w = lpa(s.min_size.width, parent_w).map(|v| grid.cols(v));
    let max_w = lpa(s.max_size.width, parent_w).map(|v| grid.cols(v));
    let (eh, min_h) = if k.honors_height {
        (
            dim(s.size.height, parent_h).map(|v| grid.rows(v).max(0)),
            lpa(s.min_size.height, parent_h).map(|v| grid.rows(v)),
        )
    } else {
        (None, None)
    };
    // A maximum adds no rows, so it holds for any box: a popup bounded by
    // the room below its anchor.
    let max_h = lpa(s.max_size.height, parent_h).map(|v| grid.rows(v));
    let known_w = inputs.known_dimensions.width.map(|v| grid.fit_cols(v));
    let known_h = inputs.known_dimensions.height.map(|v| grid.fit_rows(v));
    let avail_w = definite(inputs.available_space.width).map(|v| grid.fit_cols(v));
    let avail_h = definite(inputs.available_space.height).map(|v| grid.fit_rows(v));
    // A root (a list's cell) takes the width it has, as Taffy's roots do.
    let root_w = tree.is_root(node).then_some(avail_w).flatten();
    // A small box of an explicit width (an avatar, a counter) takes its
    // content's words if they are wider: its initials stay on a row. So
    // does a control (a time slot with a radio's mark before it): its label
    // is never broken inside a word.
    let ew = match ew {
        Some(w)
            if known_w.is_none()
                && ((k.container && w < SMALL_BOX)
                    || (role.control && !k.labelled && !k.empty && !k.measures)) =>
        {
            Some(w.max(children_words(tree, grid, node) + inset.x()))
        }
        ew => ew,
    };
    let fixed_w = known_w.or(ew.map(|w| clamp(w, min_w, max_w))).or(root_w);
    let fixed_h = known_h.or(if role.scroll && !role.sized {
        None
    } else {
        eh.map(|h| clamp(h, min_h, max_h))
    });
    let inner = |v: Option<i32>, i: i32| v.map(|v| (v - i).max(0));
    if inputs.run_mode == RunMode::PerformLayout && (k.measures || k.empty || k.labelled) {
        tree.set_insets(node, Insets { border, padding });
    }
    if role.splitter != Splitter::No
        && (0..tree.child_count(node)).all(|i| hidden(tree, tree.child(node, i)))
    {
        // One Cell across its axis, as long as it is laid out along it;
        // what it holds (a grip) is the painter's line. One that holds a
        // line of its own (a `│`) is laid out as any box.
        let along = |known: Option<i32>, fixed: Option<i32>| known.or(fixed).unwrap_or(1);
        let size = match role.splitter {
            Splitter::Columns => Cells {
                w: 1,
                h: along(known_h, fixed_h),
            },
            _ => Cells {
                w: along(known_w, fixed_w),
                h: 1,
            },
        };
        if inputs.run_mode == RunMode::PerformLayout {
            tree.set_insets(node, Insets::default());
            for i in 0..tree.child_count(node) {
                let child = tree.child(node, i);
                tree.layout(child, LayoutInput::HIDDEN);
            }
        }
        return LayoutOutput::from_outer_size(grid.size(size));
    }
    if k.labelled {
        let size = Cells {
            // Its explicit width was its icon's.
            w: known_w.unwrap_or_else(|| clamp(i32::from(role.label) + inset.x(), None, max_w)),
            h: 1 + inset.y(),
        };
        if inputs.run_mode == RunMode::PerformLayout {
            for i in 0..tree.child_count(node) {
                let child = tree.child(node, i);
                tree.layout(child, LayoutInput::HIDDEN);
            }
        }
        return LayoutOutput::from_outer_size(grid.size(size));
    }
    if k.measures {
        let known = (inner(fixed_w, inset.x()), inner(fixed_h, inset.y()));
        let avail = (
            inner(fixed_w.or(avail_w), inset.x()),
            inner(fixed_h.or(avail_h), inset.y()),
        );
        let mut mi = input(inputs.run_mode, known, avail, (None, None), grid);
        mi.parent_size = inputs.parent_size;
        if fixed_w.is_none() && inputs.available_space.width == AvailableSpace::MinContent {
            mi.available_space.width = AvailableSpace::MinContent;
        }
        let out = tree.measure(node, mi);
        let content = Cells {
            w: grid.up_cols(out.size.width),
            h: grid.up_rows(out.size.height),
        };
        let size = Cells {
            w: fixed_w.unwrap_or_else(|| clamp(content.w + inset.x(), min_w, max_w)),
            h: fixed_h.unwrap_or(content.h + inset.y()),
        };
        return LayoutOutput::from_outer_size(grid.size(size));
    }
    if k.empty {
        let size = Cells {
            w: fixed_w.unwrap_or_else(|| clamp(inset.x(), min_w, max_w)),
            h: fixed_h
                .or(known_h)
                .unwrap_or_else(|| clamp(inset.y(), min_h, max_h)),
        };
        // Hidden children (left out) are laid out as hidden.
        if inputs.run_mode == RunMode::PerformLayout {
            for i in 0..tree.child_count(node) {
                let child = tree.child(node, i);
                tree.layout(child, LayoutInput::HIDDEN);
            }
        }
        return LayoutOutput::from_outer_size(grid.size(size));
    }

    // A container: its children split its content box, inside a bordered
    // box from here on if it has a border. A border costs two rows, so a
    // box keeps it only as a card: padded (12pt or more above or below in
    // its style) or around two rows of content or more. A pill, a badge, a
    // tab list, a group of buttons or a field's chrome around one row has
    // none, and its padding is a compact box's.
    let card = {
        let s = tree.style(node);
        let pad = |v: LengthPercentage| lp(v, parent_w) >= 12.0 - EPSILON;
        // A surface's band is its edge: it never trades its border for
        // rows.
        surface.is_some() || pad(s.padding.top) || pad(s.padding.bottom)
    };
    let perform = inputs.run_mode == RunMode::PerformLayout;
    let mut flow = Vec::new();
    let mut absolute = Vec::new();
    let mut gone = Vec::new();
    for i in 0..tree.child_count(node) {
        let child = tree.child(node, i);
        if hidden(tree, child) {
            gone.push(child);
        } else if tree.style(child).position.is_out_of_flow() {
            absolute.push((i as u32, child));
        } else {
            flow.push((i as u32, child));
        }
    }
    let outer_framed = tree.framed();
    let divided = absolute.iter().any(|&(_, c)| holds_splitter(tree, c));
    let (mut border, mut padding) = (border, padding);
    let mut inset = inset;
    // A frame around one frame and nothing else, with no padding of its
    // own (a box whose only content is a bordered list) draws one line,
    // the inner one: as ratatui's nested blocks are drawn once. With
    // padding the two frames are the app's, apart.
    let unpadded = {
        let p = tree.style(node).padding;
        [p.left, p.right, p.top, p.bottom]
            .iter()
            .all(|&v| lp(v, parent_w) <= EPSILON)
    };
    if border != Sides::default()
        && unpadded
        && surface.is_none()
        && !role.fills
        && !role.root
        && absolute.is_empty()
        && frames_one_frame(tree, node, grid, parent_w)
    {
        (border, padding) = (Sides::default(), Sides::default());
        inset = Sides::default();
    }
    // A band inside a frame (an inset well in a card) is set apart by its
    // color: its line would be a second frame around the same content.
    if surface.is_some_and(|s| s.border) && tree.framed() {
        border = Sides::default();
        inset = border.add(padding);
    }
    let mut attempt_kind = k;
    // A group of controls (a split button, a toolbar, a tab list, a
    // segmented control) is one row of labels: never a card.
    if surface.is_none() && !flow.is_empty() && flow.iter().all(|&(_, c)| tree.role(c).control) {
        attempt_kind.container = false;
        (border, padding) = insets(tree, node, attempt_kind, parent_w, grid, None);
        inset = border.add(padding);
    }
    let (content, mut size) = loop {
        let framed = outer_framed || border != Sides::default();
        tree.set_framed(framed);
        let s = tree.style(node);
        let row = matches!(
            s.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        ) && s.display != Display::Block;
        let reverse = matches!(
            s.flex_direction,
            FlexDirection::RowReverse | FlexDirection::ColumnReverse
        ) && s.display != Display::Block;
        let spec = Spec {
            row,
            reverse,
            wrap: s.flex_wrap != FlexWrap::NoWrap && s.display == Display::Flex,
            justify: s.justify_content.keyword,
            align: if s.display == Display::Block {
                A::Stretch
            } else {
                s.align_items.keyword
            },
            gap_main: if row {
                gap_x(lp(s.gap.width, parent_w))
            } else {
                // The gap a divider sits in (a resizable group's handle)
                // keeps its row, framed or not.
                let gap = lp(s.gap.height, parent_h);
                space_between(gap, framed).max(i32::from(divided && gap > EPSILON))
            },
            gap_cross: if row {
                space_between(lp(s.gap.height, parent_h), framed)
            } else {
                gap_x(lp(s.gap.width, parent_w))
            },
            grid: s.display == Display::Grid,
            banded: surface.is_some(),
            fills: role.fills,
        };
        let content_w = inner(fixed_w, inset.x());
        let content_h = inner(fixed_h, inset.y());
        let room_w = inner(fixed_w.or(avail_w), inset.x());
        let room_h = inner(fixed_h.or(avail_h), inset.y());
        let origin = (inset.l, inset.t);
        // A percentage of this box's height is of its explicit height
        // when the layout does not keep it (a histogram's bars in a 36pt
        // row): the same in every pass, measured or placed.
        let stated_h =
            dim(tree.style(node).size.height, parent_h).map(|v| (grid.rows(v) - inset.y()).max(0));
        let area = Area {
            known: (content_w, content_h),
            room: (room_w, room_h),
            stated_h,
            // A scroll view's content is unbounded along its axis.
            unbounded: role.scroll,
        };
        let content = if spec.grid {
            let s = tree.style(node);
            let tracks = tracks(s, grid, content_w);
            arrange_grid(tree, grid, &flow, &tracks, &spec, area, perform, origin)
        } else {
            arrange_flex(tree, grid, &flow, &spec, area, perform, origin)
        };
        let size = Cells {
            w: fixed_w.unwrap_or_else(|| clamp(content.w + inset.x(), min_w, max_w)),
            h: fixed_h.unwrap_or_else(|| {
                let h = content.h + inset.y();
                clamp(eh.map_or(h, |e| h.min(e)), min_h, max_h)
            }),
        };
        if !card && border != Sides::default() && content.h < 2 {
            attempt_kind.container = false;
            (border, padding) = insets(tree, node, attempt_kind, parent_w, grid, None);
            inset = border.add(padding);
            continue;
        }
        break (content, size);
    };
    if known_w.is_none() && fixed_w.is_none() {
        size.w = size.w.max(min_w.unwrap_or(0));
    }
    // Rows shorter than points leave a box less room than its absolute
    // children placed from its top were given (a step's label under its
    // dot): it holds them, up to its explicit height, so they never
    // cover what follows it.
    if known_h.is_none() && fixed_h.is_none() && !absolute.is_empty() {
        let explicit = dim(tree.style(node).size.height, parent_h).map(|v| grid.rows(v).max(0));
        let parent = (Some((size.w - border.x()).max(0)), None);
        let mut need = 0;
        for &(_, child) in &absolute {
            let cs = tree.style(child);
            let Some(top) = length(cs.inset.top.into_raw()) else {
                continue;
            };
            // Labels only: an overlay is placed by the core, a control (a
            // thumb) and a box of no text (a selection's frame, a canvas
            // larger than the box) are drawn over what is there.
            if !cs.inset.bottom.is_auto()
                || tree.role(child).overlay
                || holds_control(tree, child)
                || first_text(tree, child).is_none()
            {
                continue;
            }
            let top = grid.rows(top);
            if top < 0 {
                continue;
            }
            let (_, h) = measure(tree, grid, child, true, (None, None), parent, parent);
            need = need.max(top + h);
        }
        let need = need + border.y();
        if need > size.h {
            size.h = explicit.map_or(need, |e| need.min(e.max(size.h)));
            size.h = clamp(size.h, min_h, max_h);
        }
    }
    if perform {
        tree.set_insets(node, Insets { border, padding });
        for &child in &gone {
            tree.layout(child, LayoutInput::HIDDEN);
        }
        let pad_box = Cells {
            w: (size.w - border.x()).max(0),
            h: (size.h - border.y()).max(0),
        };
        for &(order, child) in &absolute {
            place_absolute(
                tree,
                grid,
                order,
                child,
                pad_box,
                (border.l, border.t),
                &flow,
            );
        }
    }
    tree.set_framed(outer_framed);
    let overflow = Rect {
        left: 0.0,
        top: 0.0,
        // From the padding box's origin to the children's ends, as
        // Taffy's (the core adds the trailing padding).
        right: grid.x(padding.l + content.w),
        bottom: grid.y(padding.t + content.h),
    };
    LayoutOutput::from_sizes(grid.size(size), overflow)
}

/// How a container splits its children.
#[derive(Debug, Clone, Copy)]
struct Spec {
    row: bool,
    reverse: bool,
    wrap: bool,
    justify: J,
    align: A,
    /// The spacing between children along the main axis, in Cells.
    gap_main: i32,
    /// The spacing between lines.
    gap_cross: i32,
    grid: bool,
    /// It paints a band of its own (a surface): controls side by side on
    /// it keep a column apart.
    banded: bool,
    /// It is a screen or a modal (`Role::fills`): a top margin of its
    /// children (a dialog's popup) is kept in rows, as given.
    fills: bool,
}

/// A container's content box: its size where known, the room it has
/// otherwise (a soft bound text wraps at), in Cells.
#[derive(Debug, Clone, Copy)]
struct Area {
    known: (Option<i32>, Option<i32>),
    room: (Option<i32>, Option<i32>),
    /// Its explicit height in rows, kept or not: what its children's
    /// percentage heights are of before the room left.
    stated_h: Option<i32>,
    unbounded: bool,
}

/// How a child is placed across.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Align {
    Start,
    Center,
    End,
    Stretch,
}

fn align(keyword: A) -> Align {
    match keyword {
        A::Start | A::FlexStart | A::SelfStart | A::Baseline => Align::Start,
        A::End | A::FlexEnd | A::SelfEnd => Align::End,
        A::Center => Align::Center,
        A::Stretch | A::Normal => Align::Stretch,
    }
}

/// A child of a flex split.
struct Item {
    order: u32,
    node: NodeId,
    grow: f32,
    shrink: f32,
    margin: Sides,
    align: Align,
    /// It stretches across: its cross size is not its own.
    stretches: bool,
    /// It has content: it is never shrunk below one Cell.
    content: bool,
    /// Its text's longest word, in columns, in a row that is too wide (0
    /// for a box that is no text): kept while anything else can give.
    floor: i32,
    /// The icon controls shown as their labels ([`Kind::labelled`]) it
    /// is, or a row of (a toolbar's group, [`labels_held`]); 0 for
    /// anything else. They give their columns before anything else but
    /// the gaps: down to three a label (cut short), then out.
    labels: i32,
    /// A negative margin pulls it over its sibling (stacked avatars): in a
    /// row it keeps a column from it instead.
    overlaps: bool,
    /// It is a control: two side by side in a row keep a column apart when
    /// they share a band (a split button's halves on its fill) or each
    /// paints one, as the bands would read as one.
    control: bool,
    /// It paints a band of its own (a surface).
    filled: bool,
    /// Its last column is a blank text's (a mark's trailing space).
    trails: bool,
    /// Which of its margins are `auto` (left, right, top, bottom): they
    /// take the space left, as CSS's.
    auto: [bool; 4],
    /// The blank columns at its start and end along a row: its border
    /// and padding, and a trailing blank text (a mark's space).
    edges: (i32, i32),
    /// Its `flexBasis` along the main axis, in Cells: its `Length`.
    basis: Option<i32>,
    /// Its minimum and maximum along the main axis, in Cells.
    min_main: Option<i32>,
    max_main: Option<i32>,
    main: i32,
    cross: i32,
}

impl Item {
    fn margin_main(&self, row: bool) -> (i32, i32) {
        if row {
            (self.margin.l, self.margin.r)
        } else {
            (self.margin.t, self.margin.b)
        }
    }

    fn margin_cross(&self, row: bool) -> (i32, i32) {
        if row {
            (self.margin.t, self.margin.b)
        } else {
            (self.margin.l, self.margin.r)
        }
    }

    /// Whether its margins along the main axis are `auto`.
    fn auto_main(&self, row: bool) -> (bool, bool) {
        let [l, r, t, b] = self.auto;
        if row { (l, r) } else { (t, b) }
    }

    /// Whether its margins across are `auto`.
    fn auto_cross(&self, row: bool) -> (bool, bool) {
        let [l, r, t, b] = self.auto;
        if row { (t, b) } else { (l, r) }
    }
}

/// (main, cross) as (width, height) in a row, the other way in a column.
fn wh<T>(row: bool, main: T, cross: T) -> (T, T) {
    if row { (main, cross) } else { (cross, main) }
}

/// A child's size: (main, cross) of what it gives back.
fn measure(
    tree: &mut impl Tree,
    grid: Grid,
    node: NodeId,
    row: bool,
    known: (Option<i32>, Option<i32>),
    room: (Option<i32>, Option<i32>),
    parent: (Option<i32>, Option<i32>),
) -> (i32, i32) {
    let k = wh(row, known.0, known.1);
    let r = wh(row, room.0, room.1);
    let p = wh(row, parent.0, parent.1);
    let out = tree.layout(node, input(RunMode::ComputeSize, k, r, p, grid));
    let (w, h) = (
        grid.fit_cols(out.size.width + EPSILON),
        grid.fit_rows(out.size.height + EPSILON),
    );
    if row { (w, h) } else { (h, w) }
}

/// The columns of the longest word of `node`'s texts: a text's, the
/// widest of a column's, a row's added up (0 for a box with an explicit
/// width, or without texts).
fn min_content_cols(
    tree: &mut impl Tree,
    grid: Grid,
    node: NodeId,
    parent: (Option<i32>, Option<i32>),
) -> i32 {
    if !tree.style(node).size.width.is_auto() || hidden(tree, node) {
        return 0;
    }
    if !tree.measures(node) {
        return children_words(tree, grid, node);
    }
    if !tree.role(node).text {
        return 0;
    }
    let mut i = input(
        RunMode::ComputeSize,
        (None, None),
        (None, None),
        parent,
        grid,
    );
    i.available_space.width = AvailableSpace::MinContent;
    let out = tree.layout(node, i);
    grid.fit_cols(out.size.width + EPSILON)
}

/// Gives `over` columns of a row back from its gaps (down to one column)
/// and then its icon controls shown as labels (down to three columns,
/// cut short, then out, the last first); returns what is still over.
fn give_gaps_and_labels(items: &mut [Item], before: &mut [i32], mut over: i32) -> i32 {
    while over > 0 {
        let pick = (1..before.len())
            .filter(|&i| before[i] > 1)
            .max_by_key(|&i| before[i]);
        let Some(pick) = pick else { break };
        before[pick] -= 1;
        over -= 1;
    }
    // Three columns a label, and a column between a group's.
    let least = |i: &Item| i.labels * 3 + (i.labels - 1).max(0);
    while over > 0 {
        let pick = items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.labels > 0 && i.main > least(i))
            .max_by_key(|(_, i)| i.main)
            .map(|(n, _)| n);
        let Some(pick) = pick else { break };
        items[pick].main -= 1;
        over -= 1;
    }
    // Three columns of a label do not make it: out, last first (a group
    // leaves out its own, last first, in what it is given).
    while over > 0 {
        let Some(pick) = items.iter().rposition(|i| i.labels > 0 && i.main > 0) else {
            break;
        };
        let take = match items[pick].labels {
            1 => items[pick].main,
            _ => over.min(items[pick].main),
        };
        over -= take;
        items[pick].main -= take;
    }
    over
}

/// How many icon controls shown as their labels `node` is (1), or holds
/// and nothing else, in a row or one wrapper deep (a toolbar's group, a
/// button in its tooltip's view): they give their columns as one. 0 for
/// anything else.
fn labels_held(tree: &impl Tree, node: NodeId) -> i32 {
    if kind(tree, node).labelled {
        return 1;
    }
    let role = tree.role(node);
    if role.control || role.text || role.scroll || tree.measures(node) {
        return 0;
    }
    let s = tree.style(node);
    let row = matches!(
        s.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    ) && s.display == Display::Flex;
    let mut n = 0;
    for i in 0..tree.child_count(node) {
        let child = tree.child(node, i);
        if hidden(tree, child) || tree.style(child).position.is_out_of_flow() {
            continue;
        }
        match labels_held(tree, child) {
            0 => return 0,
            c => n += c,
        }
    }
    if n > 1 && !row { 0 } else { n }
}

/// The longest words of `node`'s children ([`min_content_cols`]): the
/// widest in a column, added up in a row.
fn children_words(tree: &mut impl Tree, grid: Grid, node: NodeId) -> i32 {
    let s = tree.style(node);
    let row = matches!(
        s.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    ) && s.display == Display::Flex;
    let mut cols = 0;
    for i in 0..tree.child_count(node) {
        let child = tree.child(node, i);
        if tree.style(child).position.is_out_of_flow() {
            continue;
        }
        let c = min_content_cols(tree, grid, child, (None, None));
        cols = if row { cols + c } else { cols.max(c) };
    }
    cols
}

fn arrange_flex(
    tree: &mut impl Tree,
    grid: Grid,
    flow: &[(u32, NodeId)],
    spec: &Spec,
    area: Area,
    perform: bool,
    origin: (i32, i32),
) -> Cells {
    let row = spec.row;
    let (known_main, known_cross) = wh(row, area.known.0, area.known.1);
    let (room_main, room_cross) = wh(row, area.room.0, area.room.1);
    // What percentages are of: a scroll view's viewport along its axis
    // too, as CSS's (a carousel's `w-full` slide is one viewport wide),
    // though its content is unbounded along it. A height's is the box's
    // explicit height before the room left (a soft bound), which a pass
    // that measures may not have: a `50%` bar would be rows tall in one
    // pass and none in another.
    let parent = if row {
        (
            known_main.or(room_main),
            known_cross.or(area.stated_h).or(room_cross),
        )
    } else {
        (
            known_main.or(area.stated_h).or(room_main),
            known_cross.or(room_cross),
        )
    };
    let (known_main, room_main) = if area.unbounded {
        (None, None)
    } else {
        (known_main, room_main)
    };
    let parent_w = wh(row, parent.0, parent.1).0.map(|c| grid.x(c));
    let framed = tree.framed();
    // What a percentage along the main axis is of, in points, and points
    // along it in Cells.
    let parent_main_pt = parent.0.map(|c| if row { grid.x(c) } else { grid.y(c) });
    let on_main = |v: f32| if row { grid.cols(v) } else { grid.rows(v) };
    let mut items: Vec<Item> = flow
        .iter()
        .map(|&(order, node)| {
            let style = tree.style(node);
            let k = kind(tree, node);
            // An explicit height in points, kept or not (a badge's 26pt
            // is its row), never stretches: as CSS's, it is no `auto`.
            let cross_auto = if row {
                style.size.height.is_auto()
                    || (!k.honors_height && length(style.size.height.into_raw()).is_none())
            } else {
                style.size.width.is_auto()
            };
            let a = align(style.align_self.map_or(spec.align, |a| a.keyword));
            let edges = if row {
                blank_edges(tree, grid, node, k, parent_w)
            } else {
                (0, 0)
            };
            let style = tree.style(node);
            let m = style.margin;
            let auto = [m.left, m.right, m.top, m.bottom].map(|v| v.is_auto());
            let mut margin = margins(style, parent_w, framed);
            if spec.fills
                && let Some(t) = lpa(m.top, parent_w)
            {
                margin.t = grid.rows(t).max(0);
            }
            let cross_auto_margin = {
                let (a, b) = if row {
                    (auto[2], auto[3])
                } else {
                    (auto[0], auto[1])
                };
                a || b
            };
            Item {
                edges,
                auto,
                trails: row && trails(tree, node),
                order,
                node,
                grow: style.flex_grow,
                shrink: style.flex_shrink,
                margin,
                align: a,
                stretches: a == Align::Stretch && cross_auto && !cross_auto_margin,
                content: !k.empty || tree.role(node).splitter != Splitter::No,
                floor: 0,
                labels: labels_held(tree, node),
                control: tree.role(node).control,
                filled: surface(
                    grid,
                    tree.role(node),
                    tree.backdrop(node).unwrap_or(grid.page),
                )
                .is_some(),
                overlaps: row && {
                    let negative =
                        |v: LengthPercentageAuto| lpa(v, parent_w).is_some_and(|m| m < 0.0);
                    negative(style.margin.left) || negative(style.margin.right)
                },
                basis: dim(style.flex_basis, parent_main_pt).map(|v| on_main(v).max(0)),
                min_main: lpa(
                    if row {
                        style.min_size.width
                    } else {
                        style.min_size.height
                    },
                    parent_main_pt,
                )
                .map(on_main),
                max_main: lpa(
                    if row {
                        style.max_size.width
                    } else {
                        style.max_size.height
                    },
                    parent_main_pt,
                )
                .map(on_main),
                main: 0,
                cross: 0,
            }
        })
        .collect();
    // The children's own main sizes: their content's, or their explicit
    // ones (what they give back), a fill's later.
    for item in &mut items {
        let (ms, me) = item.margin_main(row);
        let (cs, ce) = item.margin_cross(row);
        let cross_known = if item.stretches {
            known_cross.map(|c| (c - cs - ce).max(0))
        } else {
            None
        };
        if item.grow > 0.0 && known_main.is_some() {
            continue;
        }
        // A basis is a `Length` (a resizable panel's size), whatever the
        // box holds.
        // A fill's 0 is its content's where there is nothing to fill.
        if let Some(basis) = item.basis.filter(|&b| b > 0 || item.grow <= 0.0) {
            item.main = clamp(basis, item.min_main, item.max_main);
            continue;
        }
        // A row of no width (a chart's point a label hangs from) holds its
        // content at the content's own width, overflowing it as CSS's: one
        // line, not a column of characters.
        let room_main = room_main.filter(|_| !(row && known_main == Some(0)));
        let room = (
            room_main.map(|m| (m - ms - me).max(0)),
            known_cross.or(room_cross).map(|c| (c - cs - ce).max(0)),
        );
        item.main = measure(
            tree,
            grid,
            item.node,
            row,
            (None, cross_known),
            room,
            parent,
        )
        .0;
    }
    // Lines.
    let mut lines: Vec<std::ops::Range<usize>> = Vec::new();
    let bound = known_main.or(room_main);
    if spec.wrap
        && let Some(bound) = bound
    {
        let mut start = 0;
        let mut used = 0;
        for (i, item) in items.iter().enumerate() {
            let (ms, me) = item.margin_main(row);
            let need = item.main + ms + me + if i > start { spec.gap_main } else { 0 };
            if i > start && used + need > bound {
                lines.push(start..i);
                start = i;
                used = item.main + ms + me;
            } else {
                used += need;
            }
        }
        lines.push(start..items.len());
    } else {
        lines.push(0..items.len());
    }
    let single = lines.len() == 1;
    let mut extent_main = 0;
    let mut cross_at = 0;
    for (n, line) in lines.iter().enumerate() {
        if n > 0 {
            cross_at += spec.gap_cross;
        }
        let line_known = if single { known_cross } else { None };
        let (main, cross) = split_line(
            tree,
            grid,
            &mut items[line.clone()],
            spec,
            known_main,
            room_main,
            line_known,
            room_cross,
            parent,
            perform,
            origin,
            cross_at,
        );
        extent_main = extent_main.max(main);
        cross_at += cross;
    }
    let (w, h) = wh(row, extent_main, cross_at);
    Cells { w, h }
}

/// Splits the main axis among one line's `items` and places them across;
/// returns the line's (main, cross) extent.
#[allow(clippy::too_many_arguments)]
fn split_line(
    tree: &mut impl Tree,
    grid: Grid,
    items: &mut [Item],
    spec: &Spec,
    known_main: Option<i32>,
    room_main: Option<i32>,
    known_cross: Option<i32>,
    room_cross: Option<i32>,
    parent: (Option<i32>, Option<i32>),
    perform: bool,
    origin: (i32, i32),
    cross_at: i32,
) -> (i32, i32) {
    let row = spec.row;
    let n = items.len() as i32;
    if n == 0 {
        return (0, 0);
    }
    // The space before each child and after the last: in a row its
    // margins and the gap add up; in a column they collapse into one
    // (the largest), at most a row, as a blank row is all a terminal
    // spends between blocks.
    let mut before: Vec<i32> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let (start, _) = item.margin_main(row);
        let space = match i {
            0 => start,
            _ => {
                let (_, end) = items[i - 1].margin_main(row);
                if row {
                    let pulled = item.overlaps
                        || items[i - 1].overlaps
                        || (item.control
                            && items[i - 1].control
                            && (spec.banded || (item.filled && items[i - 1].filled)));
                    let prev = &items[i - 1];
                    // A mark's own trailing space is the first column of
                    // the gap after it.
                    let trail = prev.edges.1.min(1).min(i32::from(prev.trails));
                    let base = (end + spec.gap_main + start - trail).max(0);
                    // Controls read as words (a checkbox and its label, an
                    // icon button shown as its label): two blank columns
                    // between their texts, theirs included (one when the
                    // row is short).
                    let words =
                        (item.control || item.labels > 0) && (prev.control || prev.labels > 0);
                    let need = 2 - prev.edges.1 - item.edges.0;
                    base.max(i32::from(pulled))
                        .max(if words { need } else { 0 })
                } else {
                    end.max(spec.gap_main).max(start).min(1)
                }
            }
        };
        before.push(space);
    }
    let trail = items.last().map_or(0, |i| i.margin_main(row).1);
    let used = |items: &[Item], before: &[i32]| {
        items.iter().map(|i| i.main).sum::<i32>() + before.iter().sum::<i32>() + trail
    };
    // Fills take what is left, by weight (ratatui's `Fill`).
    let weight: f32 = items.iter().filter(|i| i.grow > 0.0).map(|i| i.grow).sum();
    // A fill's text wraps only as the last resort: a row's gaps and
    // labels give first for what it holds.
    if row
        && let Some(bound) = known_main
        && weight > 0.0
    {
        let want: i32 = items
            .iter()
            .filter(|i| i.grow > 0.0)
            .map(|i| {
                // A fill's basis of 0 (`flex: 1`) wants its content.
                i.basis.filter(|&b| b > 0).unwrap_or_else(|| {
                    measure(
                        tree,
                        grid,
                        i.node,
                        row,
                        (None, None),
                        (None, known_cross),
                        parent,
                    )
                    .0
                })
            })
            .sum();
        let over = used(items, &before) + want - bound;
        // Only when that keeps the fills whole: a fill that wraps anyway
        // (a column of sections, its texts longer than any row) keeps the
        // gaps around it.
        if over > 0 {
            let (gaps, mains) = (
                before.clone(),
                items.iter().map(|i| i.main).collect::<Vec<_>>(),
            );
            if give_gaps_and_labels(items, &mut before, over) > 0 {
                before = gaps;
                for (item, main) in items.iter_mut().zip(mains) {
                    item.main = main;
                }
            }
        }
    }
    if let Some(bound) = known_main
        && weight > 0.0
    {
        let free = (bound - used(items, &before)).max(0);
        let mut given = 0;
        for item in items.iter_mut().filter(|i| i.grow > 0.0) {
            item.main = ((free as f32) * item.grow / weight).floor() as i32;
            given += item.main;
        }
        for item in items.iter_mut().filter(|i| i.grow > 0.0) {
            if given >= free {
                break;
            }
            item.main += 1;
            given += 1;
        }
        for item in items.iter_mut().filter(|i| i.grow > 0.0) {
            item.main = clamp(item.main, item.min_main, item.max_main);
        }
    }
    // Too much: a row's gaps give first, down to a column; then icon
    // controls shown as their labels, down to three columns (cut short)
    // or out; then the shrinking children down to their texts' longest
    // words, then all down to theirs; then the shrinking ones and all of
    // them below (a text wraps, the last resort), never below one Cell
    // for one with content: a box never lays a child out across its
    // border (`flexShrink` first, as ratatui's constraints give way).
    if let Some(bound) = known_main.or(room_main)
        && used(items, &before) > bound
    {
        let mut over = used(items, &before) - bound;
        if row {
            over = give_gaps_and_labels(items, &mut before, over);
            if over > 0 {
                for item in items.iter_mut() {
                    item.floor = min_content_cols(tree, grid, item.node, parent);
                }
            }
        }
        // (only `flexShrink`, keeping the longest word).
        const PHASES: [(bool, bool); 4] =
            [(true, true), (false, true), (true, false), (false, false)];
        for (shrinking, words) in PHASES {
            while over > 0 {
                let pick = items
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| {
                        let floor = if words {
                            i.floor.max(i32::from(i.content))
                        } else {
                            i32::from(i.content)
                        };
                        i.labels == 0 && i.main > floor && (!shrinking || i.shrink > 0.0)
                    })
                    .max_by(|a, b| {
                        let w = |i: &Item| i.main as f32 * if shrinking { i.shrink } else { 1.0 };
                        w(a.1).total_cmp(&w(b.1))
                    })
                    .map(|(n, _)| n);
                let Some(pick) = pick else { break };
                items[pick].main -= 1;
                over -= 1;
            }
        }
    }
    let total = used(items, &before);
    let mut free = known_main.map_or(0, |b| (b - total).max(0));
    // `auto` margins take the space left first, shared equally (a box
    // centered by `margin: auto`), as CSS's.
    let autos: i32 = items
        .iter()
        .map(|i| {
            let (a, b) = i.auto_main(row);
            i32::from(a) + i32::from(b)
        })
        .sum();
    let mut auto_share = (0, 0);
    if autos > 0 {
        auto_share = (free / autos, free % autos);
        free = 0;
    }
    let mut auto_given = 0;
    let mut auto_take = || {
        let extra = i32::from(auto_given < auto_share.1);
        auto_given += 1;
        auto_share.0 + extra
    };
    // Flex: where the space left goes.
    let (lead, between, between_extra) = match spec.justify {
        J::End | J::FlexEnd => (free, 0, 0),
        J::Center => (free / 2, 0, 0),
        J::SpaceBetween if n > 1 => (0, free / (n - 1), free % (n - 1)),
        J::SpaceAround => {
            let unit = free / n;
            (unit / 2, unit, 0)
        }
        J::SpaceEvenly => {
            let unit = free / (n + 1);
            (unit, unit, 0)
        }
        _ => (0, 0, 0),
    };
    let extent = known_main.unwrap_or(total);
    // Across: each child's own size at its main size, or the line's.
    let mut line_cross = 0;
    for item in items.iter_mut() {
        let (cs, ce) = item.margin_cross(row);
        let cross_known = if item.stretches {
            known_cross.map(|c| (c - cs - ce).max(0))
        } else {
            None
        };
        let room = (
            Some(item.main),
            known_cross.or(room_cross).map(|c| (c - cs - ce).max(0)),
        );
        let mut cross = measure(
            tree,
            grid,
            item.node,
            row,
            (Some(item.main), cross_known),
            room,
            parent,
        )
        .1;
        if let Some(c) = known_cross {
            cross = cross.min((c - cs - ce).max(0));
        }
        item.cross = cross;
        line_cross = line_cross.max(cross + cs + ce);
    }
    let line_cross = known_cross.unwrap_or(line_cross);
    let mut at = lead;
    for (i, item) in items.iter_mut().enumerate() {
        let (cs, ce) = item.margin_cross(row);
        if item.stretches {
            item.cross = (line_cross - cs - ce).max(0);
        }
        let (auto_start, auto_end) = item.auto_main(row);
        let main_at = at + before[i] + if auto_start { auto_take() } else { 0 };
        at = main_at + item.main + between + i32::from((i as i32) < between_extra);
        if auto_end {
            at += auto_take();
        }
        if !perform {
            continue;
        }
        let main_pos = if spec.reverse {
            extent - main_at - item.main
        } else {
            main_at
        };
        let align = match item.auto_cross(row) {
            (true, true) => Align::Center,
            (true, false) => Align::End,
            (false, true) => Align::Start,
            (false, false) => item.align,
        };
        let cross_pos = cross_at
            + match align {
                Align::Start | Align::Stretch => cs,
                Align::Center => (line_cross - item.cross - cs - ce) / 2 + cs,
                Align::End => line_cross - item.cross - ce,
            };
        let (x, y) = wh(row, main_pos, cross_pos);
        let (w, h) = wh(row, item.main, item.cross);
        let p = wh(row, parent.0, parent.1);
        place(
            tree,
            grid,
            item.order,
            item.node,
            (origin.0 + x, origin.1 + y),
            Cells { w, h },
            p,
        );
    }
    (total, line_cross)
}

/// Lays `node` out at `size` and records it at `at` (Cells, in its
/// parent), shifted by its relative insets.
fn place(
    tree: &mut impl Tree,
    grid: Grid,
    order: u32,
    node: NodeId,
    at: (i32, i32),
    size: Cells,
    parent: (Option<i32>, Option<i32>),
) {
    let out = tree.layout(
        node,
        input(
            RunMode::PerformLayout,
            (Some(size.w), Some(size.h)),
            (Some(size.w), Some(size.h)),
            parent,
            grid,
        ),
    );
    let style = tree.style(node);
    let parent_w = parent.0.map(|c| grid.x(c));
    let parent_h = parent.1.map(|c| grid.y(c));
    let (mut x, mut y) = at;
    if style.position == Position::Relative {
        let inset = style.inset;
        if let Some(l) = lpa(inset.left, parent_w) {
            x += grid.cols(l);
        } else if let Some(r) = lpa(inset.right, parent_w) {
            x -= grid.cols(r);
        }
        if let Some(t) = lpa(inset.top, parent_h) {
            y += grid.rows(t);
        } else if let Some(b) = lpa(inset.bottom, parent_h) {
            y -= grid.rows(b);
        }
    }
    let Insets { border, padding } = tree.insets(node);
    let margin = margins(tree.style(node), parent_w, tree.framed());
    let mut layout = Layout::with_order(order);
    layout.location = taffy::Point {
        x: grid.x(x),
        y: grid.y(y),
    };
    layout.size = grid.size(size);
    layout.scrollable_overflow_rect = out.scrollable_overflow_rect;
    layout.border = grid.sides(border);
    layout.padding = grid.sides(padding);
    layout.margin = grid.sides(margin);
    tree.place(node, &layout);
}

/// An absolutely positioned child of a box whose padding box is `area`,
/// at `origin` in it: placed by its insets, its size its own; left out
/// (laid out hidden) where it would cover the children in flow, `flow`
/// (x, y, width, height in the padding box): a badge pinned on an avatar's
/// corner, a layer over a box's content. Cells cannot be seen through, so
/// one of the two would be lost; the content is kept.
fn place_absolute(
    tree: &mut impl Tree,
    grid: Grid,
    order: u32,
    node: NodeId,
    area: Cells,
    origin: (i32, i32),
    flow: &[(u32, NodeId)],
) {
    let parent = (Some(area.w), Some(area.h));
    let style = tree.style(node);
    let (pw, ph) = (Some(grid.x(area.w)), Some(grid.y(area.h)));
    let inset = style.inset;
    // Both insets of an axis and no size along it: stretched between them
    // (a divider's band down a whole column), as CSS's.
    let between = |a: LengthPercentageAuto,
                   b: LengthPercentageAuto,
                   size: Dimension,
                   basis,
                   cells: i32,
                   unit: fn(Grid, f32) -> i32| {
        match (lpa(a, basis), lpa(b, basis)) {
            (Some(a), Some(b)) if size.is_auto() => {
                Some((cells - unit(grid, a) - unit(grid, b)).max(0))
            }
            _ => None,
        }
    };
    let known = (
        between(
            inset.left,
            inset.right,
            style.size.width,
            pw,
            area.w,
            Grid::cols,
        ),
        between(
            inset.top,
            inset.bottom,
            style.size.height,
            ph,
            area.h,
            Grid::rows,
        ),
    );
    let (w, h) = measure(tree, grid, node, true, known, parent, parent);
    let style = tree.style(node);
    let inset = style.inset;
    let x = lpa(inset.left, pw)
        .map(|l| grid.cols(l))
        .or_else(|| lpa(inset.right, pw).map(|r| area.w - w - grid.cols(r)))
        .unwrap_or(0);
    let y = lpa(inset.top, ph)
        .map(|t| grid.rows(t))
        .or_else(|| lpa(inset.bottom, ph).map(|b| area.h - h - grid.rows(b)))
        .unwrap_or(0);
    // A small label pinned over a row of a sibling's text (a badge on
    // an avatar's initials) would crowd or hide it, as Cells cannot be
    // seen through: it is left out and the text kept. Controls (a
    // slider's thumb), bars pinned over a scroll view and boxes over
    // blank Cells are placed.
    let at = (origin.0 + x, origin.1 + y, w, h);
    let hides = w > 0
        && h == 1
        && !holds_control(tree, node)
        && flow.iter().any(|&(_, sibling)| {
            let (sx, sy, sw, sh) = cells_of(grid, &tree.placed(sibling));
            w < sw
                && at.0 < sx + sw
                && sx < at.0 + w
                && at.1 < sy + sh
                && sy < at.1 + h
                && glyphs_on(tree, grid, sibling, sy, at.1)
        });
    if hides {
        tree.layout(node, LayoutInput::HIDDEN);
        return;
    }
    place(
        tree,
        grid,
        order,
        node,
        (origin.0 + x, origin.1 + y),
        Cells { w, h },
        parent,
    );
}

/// `layout`'s place and size, in Cells.
fn cells_of(grid: Grid, layout: &Layout) -> (i32, i32, i32, i32) {
    (
        nearest(layout.location.x / grid.width),
        nearest(layout.location.y / grid.height),
        nearest(layout.size.width / grid.width),
        nearest(layout.size.height / grid.height),
    )
}

/// Whether `node` is a divider ([`Splitter`]) or holds one.
fn holds_splitter(tree: &impl Tree, node: NodeId) -> bool {
    tree.role(node).splitter != Splitter::No
        || (0..tree.child_count(node)).any(|i| holds_splitter(tree, tree.child(node, i)))
}

/// Whether all `node` holds is one bordered box, alone in the flow and
/// through boxes of no border or padding of their own that hold only it.
fn frames_one_frame(tree: &impl Tree, node: NodeId, grid: Grid, parent_w: Option<f32>) -> bool {
    let mut at = node;
    loop {
        let mut only = None;
        for i in 0..tree.child_count(at) {
            let child = tree.child(at, i);
            if hidden(tree, child) {
                continue;
            }
            if only.is_some() || tree.style(child).position.is_out_of_flow() {
                return false;
            }
            only = Some(child);
        }
        let Some(child) = only else {
            return false;
        };
        let role = tree.role(child);
        let k = kind(tree, child);
        if !k.container || role.control || role.fills {
            return false;
        }
        let surface = surface(grid, role, tree.backdrop(child).unwrap_or(grid.page));
        let (border, padding) = insets(tree, child, k, parent_w, grid, surface);
        if border != Sides::default() {
            return true;
        }
        if padding != Sides::default() {
            return false;
        }
        at = child;
    }
}

/// The blank columns at the start and end of `node` in a row: its border
/// and padding, and a trailing blank text (a mark's space).
fn blank_edges(
    tree: &impl Tree,
    grid: Grid,
    node: NodeId,
    k: Kind,
    parent_w: Option<f32>,
) -> (i32, i32) {
    let role = tree.role(node);
    let surface = k
        .container
        .then(|| surface(grid, role, tree.backdrop(node).unwrap_or(grid.page)))
        .flatten();
    let (b, p) = insets(tree, node, k, parent_w, grid, surface);
    let (l, r) = (b.l + p.l, b.r + p.r + i32::from(trails(tree, node)));
    // An icon shown as its label in an explicit width wider than it (a
    // chevron in its square) has blank columns around it.
    let wide = k.labelled
        && dim(tree.style(node).size.width, parent_w)
            .is_some_and(|w| grid.cols(w) > i32::from(role.label) + l + r);
    if wide { (l.max(1), r.max(1)) } else { (l, r) }
}

/// Whether the last thing `node` shows along a row is a blank text (the
/// space after a checkbox's `[ ]`).
fn trails(tree: &impl Tree, node: NodeId) -> bool {
    let role = tree.role(node);
    if role.text {
        return !role.input && !role.glyphs && tree.measures(node);
    }
    let s = tree.style(node);
    if !matches!(s.flex_direction, FlexDirection::Row) || role.scroll {
        return false;
    }
    let last = (0..tree.child_count(node))
        .rev()
        .map(|i| tree.child(node, i))
        .find(|&c| !hidden(tree, c) && !tree.style(c).position.is_out_of_flow());
    last.is_some_and(|c| trails(tree, c))
}

/// Whether `node` is a control or holds one.
fn holds_control(tree: &impl Tree, node: NodeId) -> bool {
    tree.role(node).control
        || (0..tree.child_count(node)).any(|i| holds_control(tree, tree.child(node, i)))
}

/// Whether a text with characters that are no white space, in `node`
/// (placed at row `top`) and outside scroll views, is on row `row`.
fn glyphs_on(tree: &impl Tree, grid: Grid, node: NodeId, top: i32, row: i32) -> bool {
    let role = tree.role(node);
    if role.scroll {
        return false;
    }
    if role.text && tree.measures(node) {
        let (.., h) = cells_of(grid, &tree.placed(node));
        return role.glyphs && top <= row && row < top + h;
    }
    (0..tree.child_count(node)).any(|i| {
        let child = tree.child(node, i);
        let (_, y, ..) = cells_of(grid, &tree.placed(child));
        glyphs_on(tree, grid, child, top + y, row)
    })
}

/// A grid column.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Track {
    Fill(f32),
    Length(i32),
    Content,
}

/// The columns of a grid styled `style`, its content box `width` Cells
/// wide where known.
fn tracks(style: &Style, grid: Grid, width: Option<i32>) -> Vec<Track> {
    let basis = width.map(|w| grid.x(w));
    let track = |t: &taffy::TrackSizingFunction| {
        let raw = t.max_sizing_function().into_raw();
        match raw.tag() {
            CompactLength::FR_TAG => Track::Fill(raw.value()),
            _ => resolve(raw, basis).map_or(Track::Content, |v| Track::Length(grid.cols(v).max(0))),
        }
    };
    let mut out = Vec::new();
    for component in &style.grid_template_columns {
        match component {
            GridTemplateComponent::Single(t) => out.push(track(t)),
            GridTemplateComponent::Repeat(r) => {
                let times = match r.count {
                    RepetitionCount::Count(n) => usize::from(n),
                    _ => 1,
                };
                for _ in 0..times {
                    out.extend(r.tracks.iter().map(track));
                }
            }
        }
    }
    if out.is_empty() {
        out.push(Track::Fill(1.0));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn arrange_grid(
    tree: &mut impl Tree,
    grid: Grid,
    flow: &[(u32, NodeId)],
    tracks: &[Track],
    spec: &Spec,
    area: Area,
    perform: bool,
    origin: (i32, i32),
) -> Cells {
    let s_gap = (spec.gap_main, spec.gap_cross);
    // In a grid the main axis is the row's (x).
    let (col_gap, row_gap) = if spec.row { s_gap } else { (s_gap.1, s_gap.0) };
    let cols = tracks.len();
    let parent = (area.known.0.or(area.room.0), area.known.1.or(area.room.1));
    let mut widths: Vec<i32> = vec![0; cols];
    for (c, track) in tracks.iter().enumerate() {
        widths[c] = match *track {
            Track::Length(n) => n,
            _ => flow
                .iter()
                .skip(c)
                .step_by(cols)
                .map(|&(_, node)| {
                    measure(
                        tree,
                        grid,
                        node,
                        true,
                        (None, None),
                        (area.room.0, None),
                        parent,
                    )
                    .0
                })
                .max()
                .unwrap_or(0),
        };
    }
    let weight: f32 = tracks
        .iter()
        .filter_map(|t| {
            if let Track::Fill(f) = t {
                Some(*f)
            } else {
                None
            }
        })
        .sum();
    if let Some(bound) = area.known.0.or(area.room.0)
        && weight > 0.0
    {
        let fixed: i32 = tracks
            .iter()
            .zip(&widths)
            .filter(|(t, _)| !matches!(t, Track::Fill(_)))
            .map(|(_, w)| *w)
            .sum();
        let free = (bound - fixed - col_gap * (cols as i32 - 1)).max(0);
        let mut given = 0;
        for (t, w) in tracks.iter().zip(widths.iter_mut()) {
            if let Track::Fill(f) = t {
                *w = ((free as f32) * f / weight).floor() as i32;
                given += *w;
            }
        }
        for (t, w) in tracks.iter().zip(widths.iter_mut()) {
            if given >= free {
                break;
            }
            if matches!(t, Track::Fill(_)) {
                *w += 1;
                given += 1;
            }
        }
    }
    let total_w = widths.iter().sum::<i32>() + col_gap * (cols as i32 - 1).max(0);
    let mut heights: Vec<i32> = flow
        .chunks(cols)
        .map(|cells| {
            cells
                .iter()
                .enumerate()
                .map(|(c, &(_, node))| {
                    measure(
                        tree,
                        grid,
                        node,
                        true,
                        (Some(widths[c]), None),
                        (Some(widths[c]), None),
                        parent,
                    )
                    .1
                })
                .max()
                .unwrap_or(0)
        })
        .collect();
    // Rows stretch over a known height, as CSS's auto rows: a panel's one
    // cell (a dock's) fills it, whatever its content's own height (an
    // editor's layers, all absolute).
    if let Some(known) = area.known.1
        && !heights.is_empty()
    {
        let count = heights.len() as i32;
        let used = heights.iter().sum::<i32>() + row_gap * (count - 1);
        let free = known - used;
        if free > 0 {
            for (r, h) in heights.iter_mut().enumerate() {
                *h += free / count + i32::from((r as i32) < free % count);
            }
        }
    }
    let mut y = 0;
    for (r, cells) in flow.chunks(cols).enumerate() {
        if r > 0 {
            y += row_gap;
        }
        let height = heights[r];
        if perform {
            let mut x = 0;
            for (c, &(order, node)) in cells.iter().enumerate() {
                place(
                    tree,
                    grid,
                    order,
                    node,
                    (origin.0 + x, origin.1 + y),
                    Cells {
                        w: widths[c],
                        h: height,
                    },
                    parent,
                );
                x += widths[c] + col_gap;
            }
        }
        y += height;
    }
    Cells { w: total_w, h: y }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_round_down() {
        assert_eq!(nearest(1.5), 1);
        assert_eq!(nearest(1.51), 2);
        assert_eq!(nearest(-1.5), -1);
        assert_eq!(nearest(2.9999), 3);
    }

    #[test]
    fn spacing_is_coarse() {
        assert_eq!([4.0, 8.0, 16.0, 24.0, 32.0].map(space_x), [0, 1, 1, 2, 2]);
        assert_eq!([8.0, 12.0, 16.0, 24.0, 48.0].map(space_y), [0, 0, 0, 1, 1]);
        assert_eq!([0.0, 4.0, 24.0].map(gap_x), [0, 1, 2]);
    }
}

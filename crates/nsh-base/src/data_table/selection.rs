//! What is selected in a table and how the keyboard moves it.

use super::DataTableSelectionMode as Mode;

/// The selection: nothing, a row, a column (a definition index) or a cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Selection {
    #[default]
    None,
    Row(usize),
    Column(usize),
    Cell(usize, usize),
}

impl Selection {
    pub(crate) fn row(&self) -> Option<usize> {
        match *self {
            Selection::Row(r) | Selection::Cell(r, _) => Some(r),
            _ => None,
        }
    }

    pub(crate) fn column(&self) -> Option<usize> {
        match *self {
            Selection::Column(c) | Selection::Cell(_, c) => Some(c),
            _ => None,
        }
    }

    /// Whether the selection is of the kind `mode` selects (the cell mode
    /// also takes a row, which its row headers select).
    pub(crate) fn fits(&self, mode: Mode) -> bool {
        matches!(
            (self, mode),
            (Selection::None, _)
                | (Selection::Row(_), Mode::Row | Mode::Cell)
                | (Selection::Column(_), Mode::Column)
                | (Selection::Cell(..), Mode::Cell)
        )
    }
}

/// A key's move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Move {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    First,
    Last,
}

/// What a move needs to know of the table.
pub(crate) struct Nav<'a> {
    pub(crate) mode: Mode,
    pub(crate) rows: usize,
    /// The columns that can be selected, as definition indexes in display
    /// order.
    pub(crate) columns: &'a [usize],
    /// Rows in a page.
    pub(crate) page: usize,
    /// Moves past an end go around to the other.
    pub(crate) wrap: bool,
}

impl Nav<'_> {
    /// The selection after `movement` from `from`, `None` when nothing
    /// moves (no rows to select, or a move the mode has none of).
    pub(crate) fn step(&self, from: Selection, movement: Move) -> Option<Selection> {
        match self.mode {
            Mode::None => None,
            Mode::Row => self.step_row(from, movement),
            Mode::Column => self.step_column(from, movement),
            Mode::Cell => self.step_cell(from, movement),
        }
    }

    fn step_row(&self, from: Selection, movement: Move) -> Option<Selection> {
        if self.rows == 0 {
            return None;
        }
        let row = match from {
            Selection::Row(r) => Some(r.min(self.rows - 1)),
            _ => None,
        };
        Some(Selection::Row(self.vertical(row, movement)?))
    }

    fn step_column(&self, from: Selection, movement: Move) -> Option<Selection> {
        let at = match from {
            Selection::Column(c) => self.columns.iter().position(|&d| d == c),
            _ => None,
        };
        Some(Selection::Column(
            self.columns[self.horizontal(at, movement)?],
        ))
    }

    fn step_cell(&self, from: Selection, movement: Move) -> Option<Selection> {
        if self.rows == 0 || self.columns.is_empty() {
            return None;
        }
        let (row, col) = match from {
            Selection::Cell(row, col) => (row, col),
            // A row selected by its header: its first cell.
            Selection::Row(row) => (row, self.columns[0]),
            _ => return Some(Selection::Cell(0, self.columns[0])),
        };
        let row = row.min(self.rows - 1);
        let at = self.columns.iter().position(|&d| d == col);
        match movement {
            Move::Left | Move::Right | Move::First | Move::Last => {
                let at = at.or(Some(0));
                let to = self.horizontal(at, movement)?;
                Some(Selection::Cell(row, self.columns[to]))
            }
            _ => {
                let to = self.vertical(Some(row), movement)?;
                Some(Selection::Cell(to, col))
            }
        }
    }

    /// The row after a vertical `movement` (Home and End are the first and
    /// last row in the row and cell modes' vertical moves only for rows).
    fn vertical(&self, row: Option<usize>, movement: Move) -> Option<usize> {
        let last = self.rows - 1;
        let Some(row) = row else {
            // Nothing selected: the first row, or the last one for a move
            // backwards.
            return Some(match movement {
                Move::Last => last,
                _ => 0,
            });
        };
        let page = self.page.max(1);
        Some(match movement {
            Move::Up if row > 0 => row - 1,
            Move::Up if self.wrap => last,
            Move::Down if row < last => row + 1,
            Move::Down if self.wrap => 0,
            Move::Up | Move::Down => row,
            Move::PageUp => row.saturating_sub(page),
            Move::PageDown => (row + page).min(last),
            Move::First => 0,
            Move::Last => last,
            Move::Left | Move::Right => return None,
        })
    }

    /// The index into `columns` after a horizontal `movement`.
    fn horizontal(&self, at: Option<usize>, movement: Move) -> Option<usize> {
        if self.columns.is_empty() {
            return None;
        }
        let last = self.columns.len() - 1;
        let Some(at) = at else {
            return Some(match movement {
                Move::Last => last,
                _ => 0,
            });
        };
        Some(match movement {
            Move::Left if at > 0 => at - 1,
            Move::Left if self.wrap => last,
            Move::Right if at < last => at + 1,
            Move::Right if self.wrap => 0,
            Move::Left | Move::Right => at,
            Move::First => 0,
            Move::Last => last,
            Move::Up | Move::Down | Move::PageUp | Move::PageDown => return None,
        })
    }
}

/// `selection` kept inside a table of `rows` rows and `columns` columns, and
/// of the kind `mode` selects (anything else is cleared).
pub(crate) fn clamp(selection: Selection, mode: Mode, rows: usize, columns: usize) -> Selection {
    if !selection.fits(mode) {
        return Selection::None;
    }
    match selection {
        Selection::Row(r) if r >= rows => Selection::None,
        Selection::Column(c) if c >= columns => Selection::None,
        Selection::Cell(r, c) if r >= rows || c >= columns => Selection::None,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(mode: Mode, wrap: bool) -> Nav<'static> {
        Nav {
            mode,
            rows: 10,
            columns: &[2, 0, 1],
            page: 4,
            wrap,
        }
    }

    #[test]
    fn rows_move_and_wrap() {
        let n = nav(Mode::Row, false);
        assert_eq!(n.step(Selection::None, Move::Down), Some(Selection::Row(0)));
        assert_eq!(n.step(Selection::Row(3), Move::Up), Some(Selection::Row(2)));
        assert_eq!(n.step(Selection::Row(0), Move::Up), Some(Selection::Row(0)));
        assert_eq!(
            n.step(Selection::Row(9), Move::Down),
            Some(Selection::Row(9))
        );
        assert_eq!(
            n.step(Selection::Row(3), Move::PageDown),
            Some(Selection::Row(7))
        );
        assert_eq!(
            n.step(Selection::Row(8), Move::PageDown),
            Some(Selection::Row(9))
        );
        assert_eq!(
            n.step(Selection::Row(3), Move::PageUp),
            Some(Selection::Row(0))
        );
        assert_eq!(
            n.step(Selection::Row(5), Move::First),
            Some(Selection::Row(0))
        );
        assert_eq!(
            n.step(Selection::Row(5), Move::Last),
            Some(Selection::Row(9))
        );
        assert_eq!(n.step(Selection::Row(5), Move::Left), None);
        let w = nav(Mode::Row, true);
        assert_eq!(w.step(Selection::Row(0), Move::Up), Some(Selection::Row(9)));
        assert_eq!(
            w.step(Selection::Row(9), Move::Down),
            Some(Selection::Row(0))
        );
    }

    #[test]
    fn columns_move_in_display_order() {
        let n = nav(Mode::Column, false);
        assert_eq!(
            n.step(Selection::None, Move::Right),
            Some(Selection::Column(2))
        );
        assert_eq!(
            n.step(Selection::Column(2), Move::Right),
            Some(Selection::Column(0))
        );
        assert_eq!(
            n.step(Selection::Column(1), Move::Right),
            Some(Selection::Column(1))
        );
        assert_eq!(
            n.step(Selection::Column(0), Move::Left),
            Some(Selection::Column(2))
        );
        assert_eq!(
            n.step(Selection::Column(0), Move::Last),
            Some(Selection::Column(1))
        );
        assert_eq!(n.step(Selection::Column(0), Move::Down), None);
    }

    #[test]
    fn cells_move_in_both_directions() {
        let n = nav(Mode::Cell, false);
        assert_eq!(
            n.step(Selection::None, Move::Down),
            Some(Selection::Cell(0, 2))
        );
        assert_eq!(
            n.step(Selection::Cell(1, 0), Move::Down),
            Some(Selection::Cell(2, 0))
        );
        assert_eq!(
            n.step(Selection::Cell(1, 0), Move::Right),
            Some(Selection::Cell(1, 1))
        );
        assert_eq!(
            n.step(Selection::Cell(1, 0), Move::First),
            Some(Selection::Cell(1, 2))
        );
        assert_eq!(
            n.step(Selection::Cell(1, 0), Move::Last),
            Some(Selection::Cell(1, 1))
        );
        assert_eq!(
            n.step(Selection::Cell(8, 0), Move::PageDown),
            Some(Selection::Cell(9, 0))
        );
        let w = nav(Mode::Cell, true);
        assert_eq!(
            w.step(Selection::Cell(1, 1), Move::Right),
            Some(Selection::Cell(1, 2))
        );
    }

    #[test]
    fn nothing_moves_without_rows_or_a_mode() {
        let mut n = nav(Mode::Row, false);
        n.rows = 0;
        assert_eq!(n.step(Selection::None, Move::Down), None);
        assert_eq!(
            nav(Mode::None, false).step(Selection::None, Move::Down),
            None
        );
    }

    #[test]
    fn a_selection_is_kept_in_range_and_in_mode() {
        assert_eq!(clamp(Selection::Row(3), Mode::Row, 4, 2), Selection::Row(3));
        assert_eq!(clamp(Selection::Row(4), Mode::Row, 4, 2), Selection::None);
        assert_eq!(
            clamp(Selection::Row(1), Mode::Cell, 4, 2),
            Selection::Row(1)
        );
        assert_eq!(
            clamp(Selection::Column(1), Mode::Row, 4, 2),
            Selection::None
        );
        assert_eq!(
            clamp(Selection::Cell(1, 2), Mode::Cell, 4, 2),
            Selection::None
        );
    }
}

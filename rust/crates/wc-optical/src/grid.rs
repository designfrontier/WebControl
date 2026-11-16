//! Track A: the 31 x 15 grid of squares, the scan order and the error grid.

use crate::{Cell, OpticalSettings, Result, pending};

pub const COLUMNS: usize = 31;
pub const ROWS: usize = 15;
/// 3 inches between square centres.
pub const PITCH_MM: f64 = 76.2;
const HALF_COLUMNS: i32 = 15;
const HALF_ROWS: i32 = 7;

/// Where the square's centre is on a perfect machine, from the bed centre.
pub fn nominal_mm(cell: Cell) -> (f64, f64) {
    (f64::from(cell.i) * PITCH_MM, f64::from(cell.j) * PITCH_MM)
}

/// Python's storage index `[i + 15][7 - j]` as (column, row), or `None` off the grid.
pub fn index(cell: Cell) -> Option<(usize, usize)> {
    let column = usize::try_from(cell.i + HALF_COLUMNS).ok().filter(|c| *c < COLUMNS)?;
    let row = usize::try_from(HALF_ROWS - cell.j).ok().filter(|r| *r < ROWS)?;
    Some((column, row))
}

fn cell_at(column: usize, row: usize) -> Cell {
    Cell { i: column as i32 - HALF_COLUMNS, j: HALF_ROWS - row as i32 }
}

/// The settings' extents in serpentine order (`autoScanDirection` 0 by rows, 1 by columns).
pub fn scan_order(_settings: &OpticalSettings) -> Result<Vec<Cell>> {
    pending("grid::scan_order")
}

/// One axis of the error grid as the wire carries it: `[column][row]`, `None` where not measured.
pub type Axis = Vec<Vec<Option<f64>>>;

/// Each square's error in mm (x, y): the extra offset the machine needed to centre it.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorGrid {
    cells: Vec<Option<(f64, f64)>>,
}

impl Default for ErrorGrid {
    fn default() -> Self {
        ErrorGrid::new()
    }
}

impl ErrorGrid {
    pub fn new() -> ErrorGrid {
        ErrorGrid { cells: vec![None; COLUMNS * ROWS] }
    }

    pub fn get(&self, cell: Cell) -> Option<(f64, f64)> {
        index(cell).and_then(|(column, row)| self.cells[column * ROWS + row])
    }

    /// False, and nothing stored, for a cell off the grid.
    pub fn set(&mut self, cell: Cell, error: Option<(f64, f64)>) -> bool {
        index(cell).map(|(column, row)| self.cells[column * ROWS + row] = error).is_some()
    }

    /// The measured cells, column by column from the top left.
    pub fn measured(&self) -> Vec<(Cell, (f64, f64))> {
        let at = |n: usize| cell_at(n / ROWS, n % ROWS);
        self.cells.iter().enumerate().filter_map(|(n, error)| error.map(|e| (at(n), e))).collect()
    }

    /// The wire's `errorX` and `errorY`: `[column][row]`.
    pub fn columns(&self) -> (Axis, Axis) {
        let axis = |pick: fn((f64, f64)) -> f64| {
            self.cells.chunks(ROWS).map(|column| column.iter().map(|e| e.map(pick)).collect()).collect()
        };
        (axis(|e| e.0), axis(|e| e.1))
    }
}

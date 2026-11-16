//! Track A: the 31 x 15 grid of squares, the scan order and the error grid.

use crate::{Cell, OpticalError, OpticalSettings, Result};

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

/// The settings' extents in serpentine order (`autoScanDirection` 0 by rows, 1 by columns), from the top left.
pub fn scan_order(settings: &OpticalSettings) -> Result<Vec<Cell>> {
    let (left, top, right, bottom) = (settings.tl_x, settings.tl_y, settings.br_x, settings.br_y);
    let on_grid = |i, j| index(Cell { i, j }).is_some();
    if !(on_grid(left, top) && on_grid(right, bottom) && left <= right && bottom <= top) {
        return Err(OpticalError::Invalid(format!(
            "The scan extents ({left}, {top}) to ({right}, {bottom}) must run left to right and top to bottom \
             within columns -15..15 and rows 7..-7"
        )));
    }
    let snake = |n: usize, line: Vec<i32>| if n.is_multiple_of(2) { line } else { line.into_iter().rev().collect() };
    let columns = || (left..=right).collect::<Vec<_>>();
    let rows = || (bottom..=top).rev().collect::<Vec<_>>();
    match settings.auto_scan_direction {
        0 => Ok(rows()
            .into_iter()
            .enumerate()
            .flat_map(|(n, j)| snake(n, columns()).into_iter().map(move |i| Cell { i, j }))
            .collect()),
        1 => Ok(columns()
            .into_iter()
            .enumerate()
            .flat_map(|(n, i)| snake(n, rows()).into_iter().map(move |j| Cell { i, j }))
            .collect()),
        other => Err(OpticalError::Invalid(format!("autoScanDirection must be 0 or 1, got {other}"))),
    }
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

//! Track A: the measured grid as Python's `saveCalibrationToCSV` wrote it.

use crate::Result;
use crate::grid::{Axis, ErrorGrid, ROWS};

fn rows(axis: &Axis) -> String {
    (0..ROWS)
        .map(|row| axis.iter().map(|column| format!("{:.2},", column[row].unwrap_or(0.0))).collect::<String>() + "\n")
        .collect()
}

/// X grid, a blank line, then Y; rows top to bottom, 2 decimals, trailing commas; unmeasured cells are 0.00.
pub fn csv(grid: &ErrorGrid) -> Result<String> {
    let (x, y) = grid.columns();
    Ok(format!("{}\n{}", rows(&x), rows(&y)))
}

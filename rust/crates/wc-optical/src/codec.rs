//! Track A: the measured grid as Python's `saveCalibrationToCSV` wrote it.

use crate::grid::ErrorGrid;
use crate::{Result, pending};

/// X grid, a blank line, then Y; rows top to bottom, 2 decimals, trailing commas.
pub fn csv(_grid: &ErrorGrid) -> Result<String> {
    pending("codec::csv")
}

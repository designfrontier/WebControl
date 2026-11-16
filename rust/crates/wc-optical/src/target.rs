//! Track B: the printable target, tiled across pages with registration marks.

use crate::render::Target;
use crate::{Paper, Result, pending};

/// A PDF of black squares at true size, one tile per page, each page labelled with its rows and columns.
pub fn pdf(_target: &Target, _paper: Paper) -> Result<Vec<u8>> {
    pending("target::pdf")
}

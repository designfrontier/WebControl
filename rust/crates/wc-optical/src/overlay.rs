//! Track A: a frame annotated with the square and crosshairs, as PNG; the numbers travel as JSON.

use crate::detect::Square;
use crate::{Frame, PixelPoint, Result, pending};

pub fn annotate(_frame: &Frame, _square: Option<&Square>, _optical_centre: PixelPoint) -> Result<Vec<u8>> {
    pending("overlay::annotate")
}

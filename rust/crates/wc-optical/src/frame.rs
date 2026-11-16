//! Track A: a grayscale frame, converted from what cameras deliver.

use crate::{Result, pending};

/// 8-bit luma, row-major, `width * height` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub luma: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Yuyv,
    I420,
    Rgb24,
    Jpeg,
    Png,
}

impl Frame {
    /// `width`/`height` are needed for the raw formats and checked against the encoded ones.
    pub fn decode(_format: PixelFormat, _width: u32, _height: u32, _bytes: &[u8]) -> Result<Frame> {
        pending("Frame::decode")
    }

    pub fn png(&self) -> Result<Vec<u8>> {
        pending("Frame::png")
    }
}

//! Track A: a grayscale frame, converted from what cameras deliver.

use std::io::Cursor;

use image::{GrayImage, ImageFormat};

use crate::{OpticalError, Result};

/// 8-bit luma, row-major, `width * height` bytes; pixel centres sit at integer coordinates.
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

fn invalid<T>(why: String) -> Result<T> {
    Err(OpticalError::Invalid(why))
}

fn expect_len(format: PixelFormat, bytes: &[u8], want: usize) -> Result<()> {
    if bytes.len() == want {
        Ok(())
    } else {
        invalid(format!("A {format:?} frame needs {want} bytes, got {}", bytes.len()))
    }
}

/// BT.601 luma in 8-bit fixed point, as OpenCV's `cvtColor(BGR2GRAY)` computes it.
fn rgb_luma(rgb: &[u8]) -> u8 {
    let weighted = 77 * u32::from(rgb[0]) + 150 * u32::from(rgb[1]) + 29 * u32::from(rgb[2]);
    ((weighted + 128) >> 8) as u8
}

impl Frame {
    /// `width`/`height` are needed for the raw formats and checked against the encoded ones; 0 x 0 accepts any size.
    pub fn decode(format: PixelFormat, width: u32, height: u32, bytes: &[u8]) -> Result<Frame> {
        let pixels = width as usize * height as usize;
        let raw = |luma: Vec<u8>| Ok(Frame { width, height, luma });
        if pixels == 0 && matches!(format, PixelFormat::Yuyv | PixelFormat::I420 | PixelFormat::Rgb24) {
            return invalid(format!("A {format:?} frame needs a size, got {width}x{height}"));
        }
        match format {
            PixelFormat::Yuyv => {
                expect_len(format, bytes, pixels * 2)?;
                raw(bytes.iter().step_by(2).copied().collect())
            }
            PixelFormat::I420 => {
                let chroma = width.div_ceil(2) as usize * height.div_ceil(2) as usize;
                expect_len(format, bytes, pixels + 2 * chroma)?;
                raw(bytes[..pixels].to_vec())
            }
            PixelFormat::Rgb24 => {
                expect_len(format, bytes, pixels * 3)?;
                raw(bytes.chunks_exact(3).map(rgb_luma).collect())
            }
            PixelFormat::Jpeg | PixelFormat::Png => {
                let kind = if format == PixelFormat::Jpeg { ImageFormat::Jpeg } else { ImageFormat::Png };
                let decoded = image::load_from_memory_with_format(bytes, kind)
                    .map_err(|e| OpticalError::Invalid(format!("Bad {format:?} frame: {e}")))?
                    .into_luma8();
                let (w, h) = decoded.dimensions();
                if pixels != 0 && (w, h) != (width, height) {
                    return invalid(format!("Expected a {width}x{height} {format:?} frame, got {w}x{h}"));
                }
                Ok(Frame { width: w, height: h, luma: decoded.into_raw() })
            }
        }
    }

    /// The frame as an `image` buffer, refusing one whose `luma` is the wrong length.
    pub fn image(&self) -> Result<GrayImage> {
        let need = self.width as usize * self.height as usize;
        let bad = || OpticalError::Invalid(format!("A {}x{} frame needs {need} bytes", self.width, self.height));
        GrayImage::from_raw(self.width, self.height, self.luma.clone())
            .filter(|_| self.luma.len() == need)
            .ok_or_else(bad)
    }

    pub fn png(&self) -> Result<Vec<u8>> {
        encode_png(&image::DynamicImage::ImageLuma8(self.image()?))
    }
}

pub(crate) fn encode_png(picture: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    picture.write_to(&mut out, ImageFormat::Png).map_err(|e| OpticalError::Invalid(format!("PNG encode: {e}")))?;
    Ok(out.into_inner())
}

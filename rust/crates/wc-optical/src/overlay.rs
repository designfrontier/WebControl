//! Track A: a frame annotated with the square and crosshairs, as PNG; the numbers travel as JSON.

use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::draw_line_segment_mut;

use crate::detect::Square;
use crate::frame::encode_png;
use crate::{Frame, PixelPoint, Result};

pub const BOX: Rgb<u8> = Rgb([0, 200, 0]);
pub const CROSSHAIR: Rgb<u8> = Rgb([230, 0, 0]);
pub const SQUARE_CENTRE: Rgb<u8> = Rgb([0, 90, 255]);
const CENTRE_ARM_PX: f32 = 8.0;

fn point(p: PixelPoint) -> (f32, f32) {
    (p.x as f32, p.y as f32)
}

/// RGB PNG: full-frame crosshairs through the optical centre, the square's outline and a cross on its centre.
pub fn annotate(frame: &Frame, square: Option<&Square>, optical_centre: PixelPoint) -> Result<Vec<u8>> {
    let mut canvas: RgbImage = DynamicImage::ImageLuma8(frame.image()?).into_rgb8();
    let (w, h) = (frame.width as f32, frame.height as f32);
    let (cx, cy) = point(optical_centre);
    draw_line_segment_mut(&mut canvas, (0.0, cy), (w, cy), CROSSHAIR);
    draw_line_segment_mut(&mut canvas, (cx, 0.0), (cx, h), CROSSHAIR);
    square.iter().for_each(|square| {
        (0..4).for_each(|k| {
            draw_line_segment_mut(&mut canvas, point(square.corners[k]), point(square.corners[(k + 1) % 4]), BOX)
        });
        let (sx, sy) = point(square.centre);
        draw_line_segment_mut(&mut canvas, (sx - CENTRE_ARM_PX, sy), (sx + CENTRE_ARM_PX, sy), SQUARE_CENTRE);
        draw_line_segment_mut(&mut canvas, (sx, sy - CENTRE_ARM_PX), (sx, sy + CENTRE_ARM_PX), SQUARE_CENTRE);
    });
    encode_png(&DynamicImage::ImageRgb8(canvas))
}

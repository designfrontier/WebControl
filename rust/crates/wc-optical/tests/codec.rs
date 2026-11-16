//! Track A: the encodings — Python's CSV, and the frames cameras deliver.

use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};
use wc_optical::codec::csv;
use wc_optical::frame::PixelFormat;
use wc_optical::grid::ErrorGrid;
use wc_optical::{Cell, Frame, OpticalError};

#[test]
fn an_empty_grid_is_fifteen_rows_of_zeros_twice() {
    let text = csv(&ErrorGrid::new()).unwrap();
    let row = "0.00,".repeat(31) + "\n";
    assert_eq!(text, format!("{}\n{}", row.repeat(15), row.repeat(15)));
}

#[test]
fn values_land_by_row_from_the_top_with_pythons_rounding() {
    let mut grid = ErrorGrid::new();
    grid.set(Cell { i: -15, j: 7 }, Some((0.125, -0.001)));
    grid.set(Cell { i: 15, j: -7 }, Some((2.675, 1.005)));
    grid.set(Cell { i: 0, j: 0 }, Some((-12.3456, 0.375)));
    let text = csv(&grid).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 31);
    assert_eq!(lines[15], "");
    let field = |line: usize, column: usize| lines[line].split(',').nth(column).unwrap();
    assert_eq!((field(0, 0), field(16, 0)), ("0.12", "-0.00"));
    assert_eq!((field(14, 30), field(30, 30)), ("2.67", "1.00"));
    assert_eq!((field(7, 15), field(23, 15)), ("-12.35", "0.38"));
    assert!(lines.iter().filter(|l| !l.is_empty()).all(|l| l.ends_with(',') && l.matches(',').count() == 31));
}

fn encoded(picture: DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut out, format).unwrap();
    out.into_inner()
}

#[test]
fn raw_camera_formats_keep_only_luma() {
    let yuyv = [10, 128, 20, 128, 30, 128, 40, 128];
    assert_eq!(Frame::decode(PixelFormat::Yuyv, 2, 2, &yuyv).unwrap().luma, [10, 20, 30, 40]);
    let i420 = [1, 2, 3, 4, 5, 6, 7, 8, 9, 90, 91, 92, 93, 94, 95, 96, 97];
    let frame = Frame::decode(PixelFormat::I420, 3, 3, &i420).unwrap();
    assert_eq!((frame.width, frame.height, frame.luma), (3, 3, (1..=9).collect()));
    let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    assert_eq!(Frame::decode(PixelFormat::Rgb24, 4, 1, &rgb).unwrap().luma, [77, 149, 29, 255]);
}

#[test]
fn raw_formats_refuse_the_wrong_length_or_no_size() {
    let invalid = |r: Result<Frame, OpticalError>, text: &str| match r {
        Err(OpticalError::Invalid(why)) => assert_eq!(why, text),
        other => panic!("{other:?}"),
    };
    invalid(Frame::decode(PixelFormat::Yuyv, 2, 2, &[0; 7]), "A Yuyv frame needs 8 bytes, got 7");
    invalid(Frame::decode(PixelFormat::I420, 2, 2, &[0; 4]), "A I420 frame needs 6 bytes, got 4");
    invalid(Frame::decode(PixelFormat::Rgb24, 1, 1, &[0; 4]), "A Rgb24 frame needs 3 bytes, got 4");
    invalid(Frame::decode(PixelFormat::Yuyv, 0, 2, &[]), "A Yuyv frame needs a size, got 0x2");
}

#[test]
fn jpeg_and_png_decode_to_gray_and_check_their_size() {
    let gray = GrayImage::from_fn(8, 6, |x, y| image::Luma([(x * 30 + y) as u8]));
    let png = encoded(DynamicImage::ImageLuma8(gray.clone()), ImageFormat::Png);
    assert_eq!(Frame::decode(PixelFormat::Png, 8, 6, &png).unwrap().luma, gray.as_raw().as_slice());
    let any = Frame::decode(PixelFormat::Png, 0, 0, &png).unwrap();
    assert_eq!((any.width, any.height), (8, 6));
    let Err(OpticalError::Invalid(why)) = Frame::decode(PixelFormat::Png, 6, 8, &png) else { panic!() };
    assert_eq!(why, "Expected a 6x8 Png frame, got 8x6");
    let colour = RgbImage::from_pixel(16, 16, image::Rgb([200, 200, 200]));
    let jpeg = encoded(DynamicImage::ImageRgb8(colour), ImageFormat::Jpeg);
    let frame = Frame::decode(PixelFormat::Jpeg, 16, 16, &jpeg).unwrap();
    assert!(frame.luma.iter().all(|v| v.abs_diff(200) <= 2), "{:?}", frame.luma);
    let Err(OpticalError::Invalid(why)) = Frame::decode(PixelFormat::Jpeg, 16, 16, &png) else { panic!() };
    assert!(why.starts_with("Bad Jpeg frame: "), "{why}");
}

#[test]
fn a_frame_encodes_as_gray_png_unless_its_buffer_is_wrong() {
    let frame = Frame { width: 3, height: 2, luma: vec![0, 50, 100, 150, 200, 250] };
    let back = image::load_from_memory_with_format(&frame.png().unwrap(), ImageFormat::Png).unwrap();
    assert_eq!(back.as_luma8().unwrap().as_raw(), &frame.luma);
    let long = Frame { luma: vec![0; 7], ..frame.clone() };
    let Err(OpticalError::Invalid(why)) = long.png() else { panic!() };
    assert_eq!(why, "A 3x2 frame needs 6 bytes");
    assert!(matches!(Frame { luma: vec![0; 5], ..frame }.png(), Err(OpticalError::Invalid(_))));
}

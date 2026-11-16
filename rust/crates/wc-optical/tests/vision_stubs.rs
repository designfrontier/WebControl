//! Track A's frozen surface: the errors, the grid, the detection parameters, and its stubs.

mod support;

use support::{is_stub, settings};
use wc_optical::detect::{self, DetectParams, Mode};
use wc_optical::frame::PixelFormat;
use wc_optical::grid::{self, ErrorGrid};
use wc_optical::{Cell, Frame, OpticalError, PixelPoint, codec, measure, overlay};

#[test]
fn the_errors_read_as_sentences() {
    assert_eq!(OpticalError::NoFrame.to_string(), "The camera gave no frame");
    assert_eq!(OpticalError::NoSquare.to_string(), "No square found in the frame");
    assert_eq!(OpticalError::Camera("busy".into()).to_string(), "Camera: busy");
    assert_eq!(OpticalError::Invalid("Bad extents".into()).to_string(), "Bad extents");
}

#[test]
fn the_grid_indexes_cells_as_python_stored_them() {
    assert_eq!(grid::index(Cell { i: -15, j: 7 }), Some((0, 0)));
    assert_eq!(grid::index(Cell { i: 15, j: -7 }), Some((30, 14)));
    assert_eq!(grid::index(Cell { i: 16, j: 0 }), None);
    assert_eq!(grid::index(Cell { i: 0, j: 8 }), None);
    assert_eq!(grid::index(Cell { i: 0, j: -8 }), None);
    assert_eq!(grid::nominal_mm(Cell { i: 2, j: -1 }), (152.4, -76.2));
    let mut errors = ErrorGrid::default();
    assert!(errors.set(Cell { i: -14, j: 6 }, Some((0.5, -0.25))));
    assert!(!errors.set(Cell { i: 99, j: 0 }, Some((1.0, 1.0))));
    assert_eq!(errors.get(Cell { i: -14, j: 6 }), Some((0.5, -0.25)));
    assert_eq!(errors.get(Cell { i: 99, j: 0 }), None);
    assert_eq!(errors.measured(), [(Cell { i: -14, j: 6 }, (0.5, -0.25))]);
    let (x, y) = errors.columns();
    assert_eq!((x.len(), x[0].len(), x[1][1], y[1][1], x[0][0]), (31, 15, Some(0.5), Some(-0.25), None));
}

#[test]
fn the_settings_become_detection_parameters() {
    let params = DetectParams::from_settings(&settings(), Mode::FindCentre);
    assert_eq!((params.blur_kernel, params.canny_low, params.canny_high), (5, 50.0, 100.0));
    assert_eq!((params.marker_mm, params.min_area_px, params.mode), ((0.6 * 25.4, 12.7), 1000.0, Mode::FindCentre));
}

#[test]
fn the_vision_stubs_say_they_are_not_implemented() {
    let frame = Frame { width: 1, height: 1, luma: vec![0] };
    let params = DetectParams::from_settings(&settings(), Mode::Calibrate);
    let centre = PixelPoint { x: 0.0, y: 0.0 };
    is_stub(Frame::decode(PixelFormat::Yuyv, 1, 1, &[0, 0]));
    is_stub(frame.png());
    is_stub(detect::find_square(&frame, &params));
    is_stub(measure::measure(std::slice::from_ref(&frame), &settings(), None));
    is_stub(measure::find_centre(std::slice::from_ref(&frame), &settings()));
    is_stub(measure::orientation(centre, centre, centre, 5.0));
    is_stub(grid::scan_order(&settings()));
    is_stub(codec::csv(&ErrorGrid::new()));
    is_stub(overlay::annotate(&frame, None, centre));
}

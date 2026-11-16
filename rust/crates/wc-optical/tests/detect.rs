//! Track A: the detector against rendered ground truth (decision 6: within 0.3 px), its rejections and the overlay.

mod detect {
    pub mod synth;
}

use detect::synth::Shot;
use wc_optical::detect::{DetectParams, Mode, Square, find_square};
use wc_optical::frame::PixelFormat;
use wc_optical::overlay::{self, BOX, CROSSHAIR, SQUARE_CENTRE};
use wc_optical::{Frame, OpticalError, PixelPoint};

fn params(size_px: (f64, f64), mode: Mode) -> DetectParams {
    DetectParams {
        blur_kernel: 5,
        canny_low: 50.0,
        canny_high: 100.0,
        min_area_px: 1000.0,
        marker_mm: (size_px.0 / 6.0, size_px.1 / 6.0),
        mode,
    }
}

fn detect(shot: &Shot) -> Result<Square, OpticalError> {
    find_square(&shot.frame(), &params(shot.size_px, Mode::Calibrate))
}

fn centre_error(shot: &Shot, square: &Square) -> f64 {
    (square.centre.x - shot.centre.0).hypot(square.centre.y - shot.centre.1)
}

struct Condition {
    name: &'static str,
    apply: fn(Shot) -> Shot,
}

const CONDITIONS: [Condition; 7] = [
    Condition { name: "sharp", apply: |s| s },
    Condition { name: "blur 1.2", apply: |s| Shot { blur_sigma: 1.2, ..s } },
    Condition { name: "blur 2", apply: |s| Shot { blur_sigma: 2.0, ..s } },
    Condition { name: "noise 6", apply: |s| Shot { noise_sigma: 6.0, ..s } },
    Condition { name: "gradient 50%", apply: |s| Shot { gradient: 0.5, ..s } },
    Condition {
        name: "blur, noise, gradient",
        apply: |s| Shot { blur_sigma: 1.5, noise_sigma: 5.0, gradient: -0.4, ..s },
    },
    Condition {
        name: "corner occluded",
        apply: |s| {
            let (sin, cos) = s.angle_deg.to_radians().sin_cos();
            let corner = (s.size_px.0 / 2.0, s.size_px.1 / 2.0);
            let at = (s.centre.0 + corner.0 * cos + corner.1 * sin, s.centre.1 - corner.0 * sin + corner.1 * cos);
            Shot { occluder: Some((at, 0.25 * s.size_px.0)), ..s }
        },
    },
];

const ANGLES: [f64; 10] = [-40.0, -25.0, -10.0, -3.0, 0.0, 3.0, 7.5, 10.0, 25.0, 40.0];

/// Every condition at every angle, the centre moving by sub-pixel steps; describes each case off the bar.
fn misses(base: Shot, angles: &[f64]) -> Vec<String> {
    CONDITIONS
        .iter()
        .flat_map(|condition| {
            angles.iter().enumerate().map(move |(n, angle)| {
                let shot = (condition.apply)(Shot {
                    centre: (base.centre.0 + 3.37 * n as f64, base.centre.1 + 1.71 * n as f64),
                    angle_deg: *angle,
                    seed: n as u64 + 1,
                    ..base
                });
                (condition.name, shot)
            })
        })
        .filter_map(|(name, shot)| match detect(&shot) {
            Ok(square) => {
                let off = centre_error(&shot, &square);
                let tilt = (square.angle_deg - shot.angle_deg).abs();
                let scale = (square.px_per_mm.0 / 6.0 - 1.0).abs().max((square.px_per_mm.1 / 6.0 - 1.0).abs());
                (off > 0.3 || tilt > 0.5 || scale > 0.01).then(|| {
                    format!("{name} at {}°: off {off:.3} px, tilt {tilt:.3}°, scale {scale:.4}", shot.angle_deg)
                })
            }
            Err(e) => Some(format!("{name} at {}°: {e}", shot.angle_deg)),
        })
        .collect()
}

#[test]
fn seventy_rendered_squares_are_found_within_a_third_of_a_pixel() {
    let failures = misses(Shot { centre: (150.0, 112.0), size_px: (64.0, 58.0), ..Shot::default() }, &ANGLES);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn vga_squares_found_at_half_size_are_still_refined_within_a_third_of_a_pixel() {
    let vga = Shot { width: 640, height: 480, centre: (310.0, 230.0), size_px: (91.44, 76.2), ..Shot::default() };
    let failures = misses(vga, &[-35.0, -3.0, 0.0, 10.0, 40.0]);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_vga_square_matches_to_a_tenth_of_a_pixel_and_orders_its_corners() {
    let shot = Shot {
        width: 640,
        height: 480,
        centre: (342.25, 221.6),
        size_px: (91.44, 91.44),
        angle_deg: 12.0,
        ..Shot::default()
    };
    let square = detect(&shot).unwrap();
    assert!(centre_error(&shot, &square) < 0.1, "{square:?}");
    let [tl, tr, br, bl] = square.corners;
    assert!(tl.x < tr.x && tr.y < br.y && br.x > bl.x && bl.y > tl.y, "{square:?}");
    assert!((square.angle_deg - 12.0).abs() < 0.2);
}

#[test]
fn an_axis_aligned_square_reads_zero_degrees_not_ninety() {
    let square = detect(&Shot { centre: (180.3, 101.8), ..Shot::default() }).unwrap();
    assert!(square.angle_deg.abs() < 0.2, "{square:?}");
}

fn rejected(frame: &Frame, size_px: (f64, f64)) {
    assert_eq!(find_square(frame, &params(size_px, Mode::Calibrate)), Err(OpticalError::NoSquare));
    assert_eq!(find_square(frame, &params(size_px, Mode::FindCentre)), Err(OpticalError::NoSquare));
}

#[test]
fn frames_without_a_whole_square_are_rejected() {
    let blank = Shot { size_px: (0.0, 0.0), ..Shot::default() };
    rejected(&blank.frame(), (60.0, 60.0));
    let disc = Shot { size_px: (0.0, 0.0), occluder: None, ..Shot::default() };
    let mut circle = disc.frame();
    circle.luma.iter_mut().enumerate().for_each(|(n, v)| {
        let (x, y) = ((n % 320) as f64 - 160.0, (n / 320) as f64 - 120.0);
        *v = if x.hypot(y) < 35.0 { 30 } else { 225 };
    });
    rejected(&circle, (60.0, 60.0));
    rejected(&Shot { centre: (300.0, 120.0), ..Shot::default() }.frame(), (60.0, 60.0));
    rejected(&Shot { centre: (160.0, 10.0), ..Shot::default() }.frame(), (60.0, 60.0));
    rejected(&Shot { size_px: (25.0, 25.0), ..Shot::default() }.frame(), (25.0, 25.0));
    rejected(&Shot { size_px: (150.0, 30.0), ..Shot::default() }.frame(), (60.0, 60.0));
    rejected(&Frame { width: 1, height: 1, luma: vec![0] }, (60.0, 60.0));
}

#[test]
fn a_frame_of_the_wrong_length_is_invalid() {
    let frame = Frame { width: 4, height: 4, luma: vec![0; 15] };
    assert!(matches!(find_square(&frame, &params((60.0, 60.0), Mode::Calibrate)), Err(OpticalError::Invalid(_))));
}

#[test]
fn a_faint_square_falls_back_to_its_rectangle_only_when_finding_the_centre() {
    let shot = Shot { dark: 120.0, light: 140.0, centre: (161.0, 118.0), ..Shot::default() };
    let faint = |mode| DetectParams { canny_low: 5.0, canny_high: 10.0, ..params(shot.size_px, mode) };
    assert_eq!(find_square(&shot.frame(), &faint(Mode::Calibrate)), Err(OpticalError::NoSquare));
    let rough = find_square(&shot.frame(), &faint(Mode::FindCentre)).unwrap();
    assert!(centre_error(&shot, &rough) < 2.0, "{rough:?}");
}

#[test]
fn swapped_canny_thresholds_and_no_blur_still_detect() {
    let shot = Shot { centre: (158.2, 121.9), ..Shot::default() };
    let params =
        DetectParams { blur_kernel: 1, canny_low: 100.0, canny_high: 50.0, ..params(shot.size_px, Mode::Calibrate) };
    let square = find_square(&shot.frame(), &params).unwrap();
    assert!(centre_error(&shot, &square) < 0.3);
}

fn rgb_at(png: &[u8], x: u32, y: u32) -> [u8; 3] {
    image::load_from_memory(png).unwrap().into_rgb8().get_pixel(x, y).0
}

#[test]
fn the_overlay_draws_the_crosshairs_the_box_and_the_centre() {
    let shot = Shot { centre: (180.0, 100.0), ..Shot::default() };
    let frame = shot.frame();
    let square = detect(&shot).unwrap();
    let centre = PixelPoint { x: 159.5, y: 119.5 };
    let png = overlay::annotate(&frame, Some(&square), centre).unwrap();
    let decoded = image::load_from_memory(&png).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (320, 240));
    assert_eq!(rgb_at(&png, 10, 119), CROSSHAIR.0);
    assert_eq!(rgb_at(&png, 159, 230), CROSSHAIR.0);
    assert_eq!(rgb_at(&png, 180, 70), BOX.0);
    assert_eq!(rgb_at(&png, 175, 100), SQUARE_CENTRE.0);
    let bare = overlay::annotate(&frame, None, centre).unwrap();
    assert!(rgb_at(&bare, 180, 70).iter().all(|c| *c == rgb_at(&bare, 180, 70)[0]));
    assert_eq!(rgb_at(&bare, 10, 119), CROSSHAIR.0);
    let bad = Frame { width: 2, height: 2, luma: vec![0] };
    assert!(matches!(overlay::annotate(&bad, None, centre), Err(OpticalError::Invalid(_))));
}

#[test]
fn a_detected_png_round_trips_through_decode() {
    let shot = Shot::default();
    let png = shot.frame().png().unwrap();
    let back = Frame::decode(PixelFormat::Png, 320, 240, &png).unwrap();
    assert!(centre_error(&shot, &find_square(&back, &params(shot.size_px, Mode::Calibrate)).unwrap()) < 0.3);
}

/// The Pi 3B budget is 150 ms per 640x480 frame; run with `cargo test --release -- --ignored` on the target.
#[test]
#[ignore]
fn a_vga_frame_is_detected_within_the_pi_budget() {
    let shot = Shot { width: 640, height: 480, centre: (330.4, 251.7), size_px: (91.4, 91.4), ..Shot::default() };
    let frame = Shot { angle_deg: 4.0, blur_sigma: 1.0, noise_sigma: 4.0, ..shot }.frame();
    let params = params(shot.size_px, Mode::Calibrate);
    let started = std::time::Instant::now();
    (0..10).for_each(|_| assert!(find_square(&frame, &params).is_ok()));
    let each = started.elapsed() / 10;
    println!("{each:?} per frame");
    assert!(each < std::time::Duration::from_millis(150), "{each:?}");
}

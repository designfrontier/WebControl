//! Track A: offsets in machine mm over several frames, the camera's mounting, and the optical centre.

mod support;
#[path = "detect/synth.rs"]
mod synth;

use support::settings;
use synth::Shot;
use wc_optical::measure::{self, FRAMES};
use wc_optical::{Frame, OpticalError, OpticalSettings, Orientation, PixelPoint};

const PX_PER_MM: f64 = 4.0;
const CENTRE: (f64, f64) = (160.0, 120.0);

/// `support::settings()`'s marker is 0.6" x 0.5" with a 0.99 printer scale that only the pitch uses.
fn marker_px() -> (f64, f64) {
    (0.6 * 25.4 * PX_PER_MM, 0.5 * 25.4 * PX_PER_MM)
}

/// A square `m` mm from the camera, seen by a camera turned `rotation` degrees and maybe mirrored.
fn seen(m: (f64, f64), rotation: f64, mirrored: bool, seed: u64) -> Frame {
    let m = if mirrored { (-m.0, m.1) } else { m };
    let (sin, cos) = rotation.to_radians().sin_cos();
    let w = (PX_PER_MM * (m.0 * cos - m.1 * sin), PX_PER_MM * (m.0 * sin + m.1 * cos));
    Shot {
        centre: (CENTRE.0 + w.0, CENTRE.1 - w.1),
        size_px: marker_px(),
        angle_deg: rotation,
        noise_sigma: 3.0,
        seed,
        ..Shot::default()
    }
    .frame()
}

fn frames(m: (f64, f64), rotation: f64, mirrored: bool) -> Vec<Frame> {
    (0..FRAMES as u64).map(|seed| seen(m, rotation, mirrored, seed + 1)).collect()
}

fn close(got: (f64, f64), want: (f64, f64), tolerance: f64) {
    assert!((got.0 - want.0).abs() < tolerance && (got.1 - want.1).abs() < tolerance, "{got:?} != {want:?}");
}

/// Python under OpenCV 4.6 read an axis-aligned square as 90° and returned (2.54, -3.81) here (spec §1.9.1).
#[test]
fn an_offset_square_reads_in_machine_mm_with_y_up() {
    let found = measure::measure(&frames((3.81, 2.54), 0.0, false), &settings(), None).unwrap();
    close((found.dx_mm, found.dy_mm), (3.81, 2.54), 0.05);
    assert!(found.frames_used >= 8, "{found:?}");
    assert!(found.angle_deg.abs() < 0.3);
    close((found.centre_px.x, found.centre_px.y), (CENTRE.0 + 3.81 * 4.0, CENTRE.1 - 2.54 * 4.0), 0.3);
}

#[test]
fn a_measured_orientation_ignores_a_square_tilted_on_the_sheet() {
    let aligned = Orientation { rotation_deg: 0.0, mirrored: false, px_per_mm: PX_PER_MM };
    [-3.0, 3.0].iter().for_each(|tilt| {
        let tilted = (0..FRAMES as u64)
            .map(|seed| {
                Shot {
                    centre: (CENTRE.0 + 3.81 * PX_PER_MM, CENTRE.1 - 2.54 * PX_PER_MM),
                    size_px: marker_px(),
                    angle_deg: *tilt,
                    seed: seed + 1,
                    ..Shot::default()
                }
                .frame()
            })
            .collect::<Vec<_>>();
        let found = measure::measure(&tilted, &settings(), Some(&aligned)).unwrap();
        close((found.dx_mm, found.dy_mm), (3.81, 2.54), 0.05);
        let guessed = measure::measure(&tilted, &settings(), None).unwrap();
        assert!((guessed.dx_mm - 3.81).abs() > 0.05, "without an orientation the tilt is taken as the camera's");
    });
}

#[test]
fn a_turned_camera_is_undone_by_its_orientation_or_by_the_square_tilt() {
    let turned = frames((-4.0, 3.0), 12.0, false);
    let orientation = Orientation { rotation_deg: 12.0, mirrored: false, px_per_mm: PX_PER_MM };
    let found = measure::measure(&turned, &settings(), Some(&orientation)).unwrap();
    close((found.dx_mm, found.dy_mm), (-4.0, 3.0), 0.05);
    let guessed = measure::measure(&turned, &settings(), None).unwrap();
    close((guessed.dx_mm, guessed.dy_mm), (-4.0, 3.0), 0.05);
}

#[test]
fn a_mirrored_camera_needs_its_orientation() {
    let mirrored = frames((2.0, -5.0), -8.0, true);
    let orientation = Orientation { rotation_deg: -8.0, mirrored: true, px_per_mm: PX_PER_MM };
    let found = measure::measure(&mirrored, &settings(), Some(&orientation)).unwrap();
    close((found.dx_mm, found.dy_mm), (2.0, -5.0), 0.05);
}

#[test]
fn one_wild_frame_is_cut_at_two_sigma() {
    let mut shots = frames((1.0, 1.0), 0.0, false);
    shots[4] = seen((2.5, 1.0), 0.0, false, 99);
    let found = measure::measure(&shots, &settings(), None).unwrap();
    assert!((7..=9).contains(&found.frames_used), "{found:?}");
    close((found.dx_mm, found.dy_mm), (1.0, 1.0), 0.05);
}

#[test]
fn frames_without_the_square_count_against_a_majority() {
    let blank = Shot { size_px: (0.0, 0.0), ..Shot::default() }.frame();
    let mut shots = frames((0.5, -0.5), 0.0, false);
    shots[..4].fill(blank.clone());
    let found = measure::measure(&shots, &settings(), None).unwrap();
    assert!((4..=6).contains(&found.frames_used), "{found:?}");
    shots[..6].fill(blank);
    assert_eq!(measure::measure(&shots, &settings(), None), Err(OpticalError::NoSquare));
    assert_eq!(measure::find_centre(&shots, &settings()), Err(OpticalError::NoSquare));
    assert_eq!(measure::measure(&[], &settings(), None), Err(OpticalError::NoFrame));
    assert_eq!(measure::find_centre(&[], &settings()), Err(OpticalError::NoFrame));
}

#[test]
fn a_set_optical_centre_is_the_reference() {
    let shifted = OpticalSettings { optical_center_x: 150.0, optical_center_y: 130.0, ..settings() };
    let found = measure::measure(&frames((0.0, 0.0), 0.0, false), &shifted, None).unwrap();
    close((found.dx_mm, found.dy_mm), (10.0 / PX_PER_MM, 10.0 / PX_PER_MM), 0.05);
    assert_eq!(measure::optical_centre(&shifted, 320, 240), PixelPoint { x: 150.0, y: 130.0 });
    let half = OpticalSettings { optical_center_y: 0.0, ..shifted };
    assert_eq!(measure::optical_centre(&half, 640, 480), PixelPoint { x: 320.0, y: 240.0 });
}

#[test]
fn find_centre_averages_the_square_centre() {
    let found = measure::find_centre(&frames((2.0, 1.0), 0.0, false), &settings()).unwrap();
    close((found.x, found.y), (CENTRE.0 + 8.0, CENTRE.1 - 4.0), 0.3);
}

/// Where the square appears after the camera moves `m` mm, for a camera turned `rotation` and maybe mirrored.
fn after_move(start: PixelPoint, m: (f64, f64), rotation: f64, mirrored: bool) -> PixelPoint {
    let relative = if mirrored { (m.0, -m.1) } else { (-m.0, -m.1) };
    let (sin, cos) = rotation.to_radians().sin_cos();
    let w = (5.0 * (relative.0 * cos - relative.1 * sin), 5.0 * (relative.0 * sin + relative.1 * cos));
    PixelPoint { x: start.x + w.0, y: start.y - w.1 }
}

#[test]
fn orientation_reads_rotation_mirroring_and_scale_from_two_moves() {
    [(0.0, false), (14.0, false), (-170.0, false), (9.0, true), (180.0, true)].iter().for_each(
        |(rotation, mirrored)| {
            let start = PixelPoint { x: 300.0, y: 200.0 };
            let after_x = after_move(start, (5.0, 0.0), *rotation, *mirrored);
            let after_y = after_move(after_x, (0.0, 5.0), *rotation, *mirrored);
            let found = measure::orientation(start, after_x, after_y, 5.0).unwrap();
            let turn = (found.rotation_deg - rotation + 540.0).rem_euclid(360.0) - 180.0;
            assert!(turn.abs() < 1e-9 && found.mirrored == *mirrored, "{rotation} {mirrored}: {found:?}");
            assert!((found.px_per_mm - 5.0).abs() < 1e-9);
        },
    );
}

#[test]
fn orientation_and_measure_agree_on_a_mirrored_turned_camera() {
    let start = PixelPoint { x: 300.0, y: 200.0 };
    let after_x = after_move(start, (5.0, 0.0), 20.0, true);
    let orientation = measure::orientation(start, after_x, after_move(after_x, (0.0, 5.0), 20.0, true), 5.0).unwrap();
    let found = measure::measure(&frames((3.0, 2.0), 20.0, true), &settings(), Some(&orientation)).unwrap();
    close((found.dx_mm, found.dy_mm), (3.0, 2.0), 0.05);
}

#[test]
fn orientation_refuses_a_still_square_or_a_bad_step() {
    let p = PixelPoint { x: 10.0, y: 10.0 };
    assert!(matches!(measure::orientation(p, p, p, 5.0), Err(OpticalError::Invalid(_))));
    let moved = PixelPoint { x: 0.0, y: 10.0 };
    assert!(matches!(measure::orientation(p, moved, moved, 0.0), Err(OpticalError::Invalid(_))));
    assert!(matches!(measure::orientation(p, moved, moved, f64::NAN), Err(OpticalError::Invalid(_))));
}

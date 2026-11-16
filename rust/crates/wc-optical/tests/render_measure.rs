//! The renderer (track B) and the measurement (track A) share one pixel convention: a frame rendered with the camera
//! a known distance from a square measures that distance back.

mod support;

use support::settings;
use wc_optical::measure::{self, FRAMES};
use wc_optical::render::{Scene, Target, render};
use wc_optical::{Frame, OpticalSettings, Orientation, PixelPoint};

/// 0.3 px at the default scene's 6 px/mm.
const TOLERANCE_MM: f64 = 0.05;

fn shots(scene: &Scene, camera_mm: (f64, f64)) -> Vec<Frame> {
    let target = Target::from_settings(&settings());
    (0..FRAMES as u64).map(|seed| render(&target, &Scene { seed: seed + 1, ..*scene }, camera_mm).unwrap()).collect()
}

fn offset(
    scene: &Scene,
    settings: &OpticalSettings,
    camera_mm: (f64, f64),
    orientation: Option<&Orientation>,
) -> (f64, f64) {
    let found = measure::measure(&shots(scene, camera_mm), settings, orientation).unwrap();
    (found.dx_mm, found.dy_mm)
}

fn close(got: (f64, f64), want: (f64, f64)) {
    let off = (got.0 - want.0).hypot(got.1 - want.1);
    assert!(off < TOLERANCE_MM, "{got:?} != {want:?} ({off} mm)");
}

#[test]
fn the_image_centre_is_the_renderers_optical_centre() {
    let scene = Scene::default();
    assert_eq!(measure::image_centre(scene.width, scene.height), scene.optical_centre());
    assert_eq!(measure::optical_centre(&settings(), 640, 480), PixelPoint { x: 320.0, y: 240.0 });
}

#[test]
fn a_rendered_offset_measures_back_with_the_default_centre() {
    let scene = Scene { noise_sigma: 1.0, ..Scene::default() };
    [(0.0, 0.0), (1.5, -2.0), (-4.0, 3.25)].iter().for_each(|&camera| {
        close(offset(&scene, &settings(), camera, None), (-camera.0, -camera.1));
    });
}

#[test]
fn a_set_optical_centre_measures_back_against_an_offset_camera() {
    let scene = Scene { centre_offset_px: (12.5, -7.25), ..Scene::default() };
    let centre = scene.optical_centre();
    let settings = OpticalSettings { optical_center_x: centre.x, optical_center_y: centre.y, ..settings() };
    close(offset(&scene, &settings, (2.0, 1.0), None), (-2.0, -1.0));
}

/// The scene turns the image clockwise on screen, which the measured mounting reports as a negative rotation.
#[test]
fn the_measured_mounting_undoes_a_turned_or_mirrored_camera() {
    [(10.0, false), (-7.0, false), (0.0, true)].iter().for_each(|&(rotation_deg, mirrored)| {
        let scene = Scene { rotation_deg, mirrored, ..Scene::default() };
        let centre = |camera| measure::measure(&shots(&scene, camera), &settings(), None).unwrap().centre_px;
        let orientation =
            measure::orientation(centre((0.0, 0.0)), centre((5.0, 0.0)), centre((0.0, 5.0)), 5.0).unwrap();
        assert_eq!(orientation.mirrored, mirrored);
        assert!((orientation.rotation_deg + rotation_deg).abs() < 0.5, "{orientation:?}");
        assert!((orientation.px_per_mm - scene.px_per_mm).abs() < 0.05, "{orientation:?}");
        close(offset(&scene, &settings(), (1.0, -2.0), Some(&orientation)), (-1.0, 2.0));
    });
}

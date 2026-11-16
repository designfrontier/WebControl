//! Track B's frozen surface: the target from the settings, and its stubs.

mod support;

use std::sync::Arc;
use std::time::Instant;

use support::{is_stub, settings};
use wc_optical::camera::file::DirCamera;
use wc_optical::camera::synthetic::SyntheticCamera;
use wc_optical::camera::{Camera, CameraConfig};
use wc_optical::render::{self, Scene, Target};
use wc_optical::{Paper, target};

#[test]
fn the_settings_become_a_target() {
    let target = Target::from_settings(&settings());
    assert_eq!(target.marker_mm, (0.6 * 25.4, 12.7));
    assert!((target.pitch_mm.1 - 75.438).abs() < 1e-9);
}

#[test]
fn the_target_stubs_say_they_are_not_implemented() {
    let sheet = Target::from_settings(&settings());
    is_stub(render::render(&sheet, &Scene::default(), (0.0, 0.0)));
    is_stub(target::pdf(&sheet, Paper::A4));
    let config = CameraConfig { width: 640, height: 480, fps: 5.0 };
    let cameras: [Box<dyn Camera>; 2] = [
        Box::new(DirCamera::new(std::path::Path::new("frames"))),
        Box::new(SyntheticCamera::new(Arc::new(|| (0.0, 0.0)))),
    ];
    cameras.into_iter().for_each(|mut camera| {
        is_stub(camera.start(&config));
        is_stub(camera.next_frame(Instant::now()));
        camera.stop();
    });
}

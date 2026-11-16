//! Track C's frozen surface: the Camera Source setting, opening a source, and its stubs.

mod support;

use std::sync::Arc;

use support::is_stub;
use wc_optical::camera::rpicam::RpicamCamera;
use wc_optical::camera::{self, Camera, CameraConfig, SledPosition, Source};

#[test]
fn the_camera_source_setting_parses() {
    assert_eq!("auto".parse(), Ok(Source::Auto));
    assert_eq!(" ".parse(), Ok(Source::Auto));
    assert_eq!("rpicam".parse(), Ok(Source::Rpicam));
    assert_eq!("synthetic".parse(), Ok(Source::Synthetic));
    assert_eq!("/dev/video2".parse(), Ok(Source::Device("/dev/video2".into())));
    assert_eq!("dir:/tmp/frames".parse(), Ok(Source::Dir("/tmp/frames".into())));
    let wrong = "webcam".parse::<Source>().unwrap_err();
    assert!(wrong.to_string().starts_with("Camera Source \"webcam\" is not auto"), "{wrong}");
}

#[test]
fn opening_a_source_gives_its_camera() {
    let sled: SledPosition = Arc::new(|| (0.0, 0.0));
    [Source::Rpicam, Source::Dir("frames".into()), Source::Synthetic]
        .iter()
        .for_each(|source| assert!(camera::open(source, Some(sled.clone())).is_ok(), "{source:?}"));
    let refused = camera::open(&Source::Synthetic, None).err().unwrap();
    assert_eq!(refused.to_string(), "The synthetic camera needs webcontrol-rs --simulate");
    is_stub(camera::open(&Source::Auto, None).map(|_| ()));
    let device = camera::open(&Source::Device("/dev/video0".into()), None);
    if cfg!(target_os = "linux") {
        assert!(device.is_ok());
    } else {
        assert_eq!(device.err().unwrap().to_string(), "Camera: /dev/video0 needs Linux's V4L2");
    }
}

#[test]
fn the_camera_stubs_say_they_are_not_implemented() {
    let mut rpicam = RpicamCamera::default();
    assert_eq!(rpicam.program, std::path::Path::new("rpicam-vid"));
    is_stub(rpicam.start(&CameraConfig { width: 640, height: 480, fps: 5.0 }));
    is_stub(rpicam.next_frame(std::time::Instant::now()));
    rpicam.stop();
}

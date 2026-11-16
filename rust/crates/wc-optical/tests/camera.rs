//! The Camera Source setting, opening a source, the live preview, MJPEG splitting, and the Pi camera adapter
//! against a fake `rpicam-vid`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::ImageEncoder;
use wc_optical::camera::rpicam::RpicamCamera;
use wc_optical::camera::{self, Camera, CameraConfig, MjpegSplitter, PREVIEW_SIZE, SledPosition, Source};
use wc_optical::{Frame, OpticalError};

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
    let device = camera::open(&Source::Device("/dev/video0".into()), None);
    if cfg!(target_os = "linux") {
        assert!(device.is_ok());
    } else {
        assert_eq!(device.err().unwrap().to_string(), "Camera: /dev/video0 needs Linux's V4L2");
    }
    // Whatever this computer has; without a camera it says so instead of opening nothing.
    if let Err(error) = camera::open(&Source::Auto, None) {
        assert!(matches!(error, OpticalError::Camera(_)), "{error}");
    }
}

#[test]
fn auto_prefers_a_listed_pi_camera_then_the_first_usb_webcam() {
    let listed = "Available cameras\n-----------------\n0 : imx708 [4608x2592 10-bit RGGB] (/base/soc/i2c0mux)\n";
    let webcams = [PathBuf::from("/dev/video0"), PathBuf::from("/dev/video2")];
    assert_eq!(camera::pick_auto(Some(listed), &webcams), Ok(Source::Rpicam));
    assert_eq!(camera::pick_auto(Some("No cameras available!\n"), &webcams), Ok(Source::Device(webcams[0].clone())));
    assert_eq!(camera::pick_auto(None, &webcams[1..]), Ok(Source::Device(webcams[1].clone())));
    let none = camera::pick_auto(Some("Available cameras\n  Modes: 'SRGGB10_CSI2P' : 1536x864\n"), &[]);
    assert_eq!(none.unwrap_err().to_string(), "Camera: no Pi camera (rpicam-hello lists none) and no USB webcam");
}

#[test]
fn the_camera_settings_give_the_size_and_rate() {
    let config = |size, fps| CameraConfig::from_settings(size, fps);
    assert_eq!(config("1024x768", 2.0), CameraConfig { width: 1024, height: 768, fps: 2.0 });
    assert_eq!(config(" 320 x 240 ", 60.0), CameraConfig { width: 320, height: 240, fps: 30.0 });
    ["", "640", "0x480", "axb"].iter().for_each(|size| {
        assert_eq!(config(size, f64::NAN), CameraConfig { width: 640, height: 480, fps: 5.0 }, "{size:?}")
    });
    assert_eq!(config("640x480", -1.0).fps, 5.0);
}

fn gradient(width: u32, height: u32) -> Frame {
    let luma = (0..height).flat_map(|_| (0..width).map(move |x| (x * 255 / (width - 1)) as u8)).collect();
    Frame { width, height, luma }
}

#[test]
fn the_preview_is_a_small_png_of_the_frame() {
    let png = camera::preview(&gradient(640, 480)).unwrap();
    let small = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
    assert_eq!((small.width(), small.height()), PREVIEW_SIZE);
    let small = small.as_luma8().expect("8-bit gray").clone();
    assert!(small.get_pixel(2, 96)[0] < 10 && small.get_pixel(253, 96)[0] > 245, "the gradient survives");
    let short = Frame { width: 640, height: 480, luma: vec![0; 10] };
    assert_eq!(camera::preview(&short).unwrap_err().to_string(), "a 640x480 frame without its pixels");
}

#[test]
fn mjpeg_splits_on_soi_and_eoi_across_reads() {
    let one = [0xFF, 0xD8, 1, 0xFF, 0x00, 2, 0xFF, 0xD9];
    let two = [0xFF, 0xD8, 3, 0xFF, 0xD9];
    let stream: Vec<u8> = [&[9, 9][..], &one, &[7], &two, &[0xFF, 0xD8, 5]].concat();
    let mut splitter = MjpegSplitter::default();
    let mut got = Vec::new();
    stream.chunks(3).for_each(|chunk| got.extend(splitter.push(chunk)));
    assert_eq!(got, [one.to_vec(), two.to_vec()]);
    assert_eq!(splitter.push(&[0xFF]), Vec::<Vec<u8>>::new(), "a partial JPEG waits");
    assert_eq!(splitter.push(&[0xD9]), [vec![0xFF, 0xD8, 5, 0xFF, 0xD9]]);
    assert!(splitter.push(&[1, 2, 0xFF]).is_empty());
    assert_eq!(splitter.push(&[0xD8, 0xFF, 0xD9]), [vec![0xFF, 0xD8, 0xFF, 0xD9]], "an SOI split across reads");
}

fn jpeg(shade: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90)
        .write_image(&vec![shade; 32 * 24], 32, 24, image::ExtendedColorType::L8)
        .unwrap();
    bytes
}

/// A directory of its own under the system temp dir, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("wc-optical-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A fake `rpicam-vid` that records its arguments, then runs `body` with `$D` set to its directory.
#[cfg(unix)]
fn fake_rpicam(dir: &Path, body: &str) -> RpicamCamera {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("rpicam-vid");
    std::fs::write(&script, format!("#!/bin/sh\nD='{}'\necho \"$@\" > \"$D/args\"\n{body}\n", dir.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut rpicam = RpicamCamera::new();
    rpicam.program = script;
    rpicam
}

#[cfg(unix)]
#[test]
fn rpicam_streams_whole_jpegs_then_reports_that_it_stopped() {
    let scratch = Scratch::new("rpicam");
    let (first, second) = (jpeg(40), jpeg(200));
    std::fs::write(scratch.0.join("1.jpg"), &first).unwrap();
    std::fs::write(scratch.0.join("2.jpg"), &second).unwrap();
    let body = "sleep 0.3; cat \"$D/1.jpg\"; sleep 0.5; cat \"$D/2.jpg\"; printf '\\377\\330cut'; \
                echo 'camera unplugged' >&2";
    let mut rpicam = fake_rpicam(&scratch.0, body);
    rpicam.start(&CameraConfig { width: 320, height: 240, fps: 5.0 }).unwrap();
    let soon = || Instant::now() + Duration::from_secs(5);
    assert_eq!(rpicam.next_jpeg(soon()).unwrap(), first);
    assert_eq!(rpicam.next_jpeg(soon()).unwrap(), second);
    let stopped = rpicam.next_jpeg(soon()).unwrap_err().to_string();
    assert!(stopped.ends_with("rpicam-vid stopped: camera unplugged"), "{stopped}");
    let args = std::fs::read_to_string(scratch.0.join("args")).unwrap();
    assert_eq!(args.trim(), "--codec mjpeg --width 320 --height 240 --framerate 5 -t 0 --nopreview -o -");
    rpicam.stop();
}

#[cfg(unix)]
#[test]
fn rpicam_decodes_frames_waits_until_the_deadline_and_stops_its_process() {
    let scratch = Scratch::new("rpicam-wait");
    std::fs::write(scratch.0.join("1.jpg"), jpeg(90)).unwrap();
    let mut rpicam = fake_rpicam(&scratch.0, "sleep 0.3; cat \"$D/1.jpg\"; exec sleep 30");
    rpicam.start(&CameraConfig { width: 32, height: 24, fps: 5.0 }).unwrap();
    let frame = rpicam.next_frame(Instant::now() + Duration::from_secs(5)).unwrap();
    assert_eq!((frame.width, frame.height), (32, 24));
    let waited = Instant::now();
    assert_eq!(rpicam.next_jpeg(Instant::now() + Duration::from_millis(200)), Err(OpticalError::NoFrame));
    assert!(waited.elapsed() >= Duration::from_millis(200));
    let stopping = Instant::now();
    rpicam.stop();
    assert!(stopping.elapsed() < Duration::from_secs(5), "stop kills rpicam-vid instead of waiting for it");
}

#[test]
fn rpicam_without_its_program_fails_to_start() {
    let mut rpicam = RpicamCamera::default();
    rpicam.program = "/nonexistent/rpicam-vid".into();
    let error = rpicam.start(&CameraConfig { width: 640, height: 480, fps: 5.0 }).unwrap_err();
    assert!(error.to_string().starts_with("Camera: /nonexistent/rpicam-vid: "), "{error}");
}

//! The synthetic renderer against its own geometry, its pinned fixtures, and the cameras built on it.

mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::settings;
use wc_optical::camera::file::DirCamera;
use wc_optical::camera::synthetic::SyntheticCamera;
use wc_optical::camera::{Camera, CameraConfig};
use wc_optical::render::{Scene, Target, render};
use wc_optical::{Frame, OpticalError, PixelPoint};

const SHEET: Target = Target { marker_mm: (15.24, 15.24), pitch_mm: (76.2, 76.2) };
const PAPER: f64 = 235.0;

fn small() -> Scene {
    Scene { width: 320, height: 240, px_per_mm: 4.0, ..Scene::default() }
}

fn at(frame: &Frame, x: u32, y: u32) -> u8 {
    frame.luma[(y * frame.width + x) as usize]
}

/// The ink-weighted centroid within `radius` px of `near`, against a known paper level.
fn centroid(frame: &Frame, near: PixelPoint, radius: f64, paper: impl Fn(u32, u32) -> f64) -> PixelPoint {
    let points = (0..frame.height).flat_map(|y| (0..frame.width).map(move |x| (x, y)));
    let window =
        points.filter(|(x, y)| (f64::from(*x) - near.x).abs() <= radius && (f64::from(*y) - near.y).abs() <= radius);
    let (w, sx, sy) = window.fold((0.0, 0.0, 0.0), |(w, sx, sy), (x, y)| {
        let ink = (paper(x, y) - f64::from(at(frame, x, y))).max(0.0);
        (w + ink, sx + ink * f64::from(x), sy + ink * f64::from(y))
    });
    PixelPoint { x: sx / w, y: sy / w }
}

fn close(a: PixelPoint, b: PixelPoint, tolerance: f64) {
    assert!((a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance, "{a:?} vs {b:?} (±{tolerance})");
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3))
}

#[test]
fn the_settings_become_a_target() {
    let target = Target::from_settings(&settings());
    assert_eq!(target.marker_mm, (0.6 * 25.4, 12.7));
    assert!((target.pitch_mm.1 - 75.438).abs() < 1e-9);
    assert_eq!(target.nearest_square((40.0, -40.0)), (76.2, -75.438));
    assert_eq!(target.nearest_square((5000.0, -5000.0)), (15.0 * 76.2, -7.0 * 75.438));
}

#[test]
fn a_sharp_square_lands_where_the_geometry_says() {
    let cases = [
        (small(), (3.81, 2.54)),
        (small(), (-0.37, 1.11)),
        (Scene { centre_offset_px: (12.5, -7.25), ..small() }, (2.0, -4.0)),
        (Scene { rotation_deg: 7.0, px_per_mm: 5.3, ..small() }, (-6.1, 3.3)),
        (Scene { mirrored: true, rotation_deg: -12.0, ..small() }, (4.4, 4.4)),
    ];
    cases.iter().for_each(|(scene, offset)| {
        let frame = render(&SHEET, scene, (-offset.0, -offset.1)).unwrap();
        let truth = scene.to_pixel(*offset);
        let found = centroid(&frame, truth, 15.24 * scene.px_per_mm * 0.75, |_, _| PAPER);
        close(found, truth, 0.04);
    });
}

#[test]
fn the_optical_centre_is_the_middle_pixel_in_opencv_terms() {
    assert_eq!(Scene::default().optical_centre(), PixelPoint { x: 320.0, y: 240.0 });
    let frame = render(&SHEET, &small(), (0.0, 0.0)).unwrap();
    close(
        centroid(&frame, PixelPoint { x: 160.0, y: 120.0 }, 45.0, |_, _| PAPER),
        PixelPoint { x: 160.0, y: 120.0 },
        0.01,
    );
}

#[test]
fn the_axes_follow_the_camera_mounting() {
    let plain = small();
    let right = plain.to_pixel((10.0, 0.0));
    assert_eq!((right.x, right.y), (200.0, 120.0));
    assert_eq!(plain.to_pixel((0.0, 10.0)).y, 80.0, "machine +Y is image up");
    assert_eq!(Scene { mirrored: true, ..plain }.to_pixel((10.0, 0.0)).x, 120.0);
    let turned = Scene { rotation_deg: 90.0, ..plain }.to_pixel((10.0, 0.0));
    close(turned, PixelPoint { x: 160.0, y: 160.0 }, 1e-9);
}

#[test]
fn pixels_map_back_to_millimetres() {
    let scenes = [
        small(),
        Scene { mirrored: true, rotation_deg: 33.0, centre_offset_px: (3.0, -9.0), ..Scene::default() },
        Scene { radial_k1: 0.08, ..Scene::default() },
        Scene { radial_k1: -0.1, rotation_deg: -5.0, ..Scene::default() },
    ];
    scenes.iter().for_each(|scene| {
        [(0.0, 0.0), (12.0, -7.5), (-40.0, 30.0)].iter().for_each(|mm| {
            let back = scene.to_mm(scene.to_pixel(*mm));
            assert!((back.0 - mm.0).abs() < 1e-9 && (back.1 - mm.1).abs() < 1e-9, "{scene:?} {mm:?} -> {back:?}");
        })
    });
}

#[test]
fn radial_distortion_pushes_squares_outward() {
    let barrel = Scene { radial_k1: 0.1, ..small() };
    let offset = (12.0, -9.0);
    let truth = barrel.to_pixel(offset);
    let plain = small().to_pixel(offset);
    assert!(truth.x - 160.0 > plain.x - 160.0 && truth.y - 120.0 > plain.y - 120.0);
    let bent = render(&SHEET, &barrel, (-offset.0, -offset.1)).unwrap();
    let flat = render(&SHEET, &small(), (-offset.0, -offset.1)).unwrap();
    let (bent, flat) = (centroid(&bent, truth, 40.0, |_, _| PAPER), centroid(&flat, plain, 40.0, |_, _| PAPER));
    close(bent, truth, 1.0);
    assert!(bent.x - flat.x > (truth.x - plain.x) / 2.0 && bent.y - flat.y > (truth.y - plain.y) / 2.0);
}

#[test]
fn blur_softens_edges_but_keeps_the_centre() {
    let sharp = render(&SHEET, &small(), (-3.0, 2.0)).unwrap();
    let soft = render(&SHEET, &Scene { blur_sigma: 1.5, ..small() }, (-3.0, 2.0)).unwrap();
    let grey = |frame: &Frame| frame.luma.iter().filter(|v| (40..225).contains(*v)).count();
    assert!(grey(&soft) > 2 * grey(&sharp), "{} vs {}", grey(&soft), grey(&sharp));
    let truth = small().to_pixel((3.0, -2.0));
    close(centroid(&soft, truth, 50.0, |_, _| PAPER), truth, 0.02);
}

#[test]
fn noise_is_seeded() {
    let noisy = Scene { noise_sigma: 4.0, ..small() };
    let a = render(&SHEET, &noisy, (1300.0, 0.0)).unwrap();
    assert_eq!(a, render(&SHEET, &noisy, (1300.0, 0.0)).unwrap());
    assert_ne!(a, render(&SHEET, &Scene { seed: 2, ..noisy }, (1300.0, 0.0)).unwrap());
    let n = a.luma.len() as f64;
    let mean = a.luma.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let sd = (a.luma.iter().map(|v| (f64::from(*v) - mean).powi(2)).sum::<f64>() / n).sqrt();
    assert!((mean - PAPER).abs() < 0.1 && (sd - 4.0).abs() < 0.1, "{mean} {sd}");
}

#[test]
fn the_light_falls_off_to_the_right_and_the_corners() {
    let frame = render(&SHEET, &Scene { light_gradient: 0.4, ..small() }, (1300.0, 0.0)).unwrap();
    let (left, centre, right) = (at(&frame, 0, 120), at(&frame, 160, 120), at(&frame, 319, 120));
    assert!(left > right && centre > left && at(&frame, 160, 0) < centre, "{left} {centre} {right}");
    assert!((f64::from(centre) - PAPER * 0.9).abs() <= 0.5, "half the gradient at the centre: {centre}");
}

#[test]
fn jpeg_quality_adds_compression_artefacts() {
    let clean = render(&SHEET, &small(), (-2.0, 1.0)).unwrap();
    let lossy = render(&SHEET, &Scene { jpeg_quality: Some(60), ..small() }, (-2.0, 1.0)).unwrap();
    let differ = clean.luma.iter().zip(&lossy.luma).filter(|(a, b)| a != b).count();
    let worst = clean.luma.iter().zip(&lossy.luma).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(differ > 100 && worst < 80, "{differ} pixels differ, worst {worst}");
}

#[test]
fn off_the_grid_is_blank_paper() {
    let frame = render(&SHEET, &small(), (15.0 * 76.2 + 100.0, 0.0)).unwrap();
    assert!(frame.luma.iter().all(|v| f64::from(*v) == PAPER));
    let edge = render(&SHEET, &small(), (15.0 * 76.2 + 10.0, 0.0)).unwrap();
    assert!(edge.luma.iter().any(|v| *v < 100), "the last column is still drawn");
}

#[test]
fn impossible_scenes_are_refused() {
    let refused = |scene: Scene, target: Target| match render(&target, &scene, (0.0, 0.0)) {
        Err(OpticalError::Invalid(why)) => why,
        other => panic!("{scene:?} gave {other:?}"),
    };
    let base = small();
    [
        Scene { width: 0, ..base },
        Scene { px_per_mm: 0.0, ..base },
        Scene { rotation_deg: f64::NAN, ..base },
        Scene { centre_offset_px: (f64::INFINITY, 0.0), ..base },
        Scene { blur_sigma: -1.0, ..base },
        Scene { noise_sigma: -1.0, ..base },
        Scene { light_gradient: 1.5, ..base },
        Scene { radial_k1: -0.3, ..base },
    ]
    .into_iter()
    .for_each(|scene| assert!(refused(scene, SHEET).starts_with("Synthetic scene: ")));
    let flat = Target { marker_mm: (0.0, 15.0), ..SHEET };
    assert_eq!(refused(base, flat), "The target needs a positive square size and pitch");
}

#[test]
fn frames_are_pinned_by_hash() {
    let pins = [
        (small(), (-3.81, -2.54), 0x8319_08e1_3e21_bfab_u64),
        (
            Scene {
                rotation_deg: 4.0,
                blur_sigma: 0.8,
                noise_sigma: 2.0,
                light_gradient: 0.2,
                radial_k1: 0.05,
                ..Scene::default()
            },
            (5.5, -3.25),
            0x85ff_88c4_15e0_adab,
        ),
    ];
    let hashes: Vec<String> = pins
        .iter()
        .map(|(scene, camera, _)| format!("{:#x}", fnv(&render(&SHEET, scene, *camera).unwrap().luma)))
        .collect();
    let expected: Vec<String> = pins.iter().map(|(_, _, hash)| format!("{hash:#x}")).collect();
    assert_eq!(hashes, expected);
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The fixture frames, which `tools/optical/golden.sh` feeds to Python: (name, camera mm, scene).
fn cases() -> Vec<(&'static str, (f64, f64), Scene)> {
    let base = small();
    vec![
        ("sharp_offset", (-3.81, -2.54), base),
        ("tilted_plus_3", (-3.81, -2.54), Scene { rotation_deg: 3.0, ..base }),
        ("tilted_minus_3", (-3.81, -2.54), Scene { rotation_deg: -3.0, ..base }),
        ("tilted_plus_10", (2.2, -1.7), Scene { rotation_deg: 10.0, ..base }),
        ("centre_offset", (1.5, 3.0), Scene { centre_offset_px: (12.5, -7.25), ..base }),
        ("distorted", (-8.0, 6.0), Scene { radial_k1: 0.06, ..base }),
        ("blurred_noisy", (4.1, 0.9), Scene { blur_sigma: 1.2, noise_sigma: 2.0, seed: 7, ..base }),
        ("lit_jpeg", (-1.3, -4.4), Scene { light_gradient: 0.3, jpeg_quality: Some(85), ..base }),
        ("off_grid", (1300.0, 0.0), base),
    ]
}

fn case_json(name: &str, camera: (f64, f64), scene: &Scene) -> Value {
    let square = SHEET.nearest_square(camera);
    let offset = (square.0 - camera.0, square.1 - camera.1);
    let pixel = scene.to_pixel(offset);
    let visible = (0.0..f64::from(scene.width)).contains(&pixel.x) && (0.0..f64::from(scene.height)).contains(&pixel.y);
    let centre = scene.optical_centre();
    json!({
        "name": name, "png": format!("frames/{name}.png"), "cameraMm": [camera.0, camera.1],
        "target": {"markerMm": [SHEET.marker_mm.0, SHEET.marker_mm.1], "pitchMm": [SHEET.pitch_mm.0, SHEET.pitch_mm.1]},
        "scene": {"width": scene.width, "height": scene.height, "pxPerMm": scene.px_per_mm,
            "rotationDeg": scene.rotation_deg, "mirrored": scene.mirrored, "opticalCentrePx": [centre.x, centre.y],
            "radialK1": scene.radial_k1, "blurSigma": scene.blur_sigma, "noiseSigma": scene.noise_sigma,
            "lightGradient": scene.light_gradient, "jpegQuality": scene.jpeg_quality, "seed": scene.seed},
        "truth": visible.then(|| json!({"centrePx": [pixel.x, pixel.y], "offsetMm": [offset.0, offset.1]})),
    })
}

fn png(frame: &Frame) -> Vec<u8> {
    let image = image::GrayImage::from_raw(frame.width, frame.height, frame.luma.clone()).unwrap();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// Equal JSON, with numbers equal to 1e-9: serde_json's parse and another platform's `sin` may differ in the last bit.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-9,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// With `WC_OPTICAL_WRITE_FIXTURES=1` this rewrites the frames and `frames.json` instead of checking them.
#[test]
fn the_fixture_frames_are_what_the_renderer_draws() {
    let write = std::env::var_os("WC_OPTICAL_WRITE_FIXTURES").is_some();
    let manifest: Vec<Value> = cases().iter().map(|(name, camera, scene)| case_json(name, *camera, scene)).collect();
    let manifest_path = fixtures().join("frames.json");
    let text = serde_json::to_string_pretty(&json!({"cases": manifest})).unwrap() + "\n";
    if write {
        std::fs::create_dir_all(fixtures().join("frames")).unwrap();
        std::fs::write(&manifest_path, &text).unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert!(same(&stored["cases"], &Value::Array(manifest)), "frames.json drifted; regenerate it");
    cases().iter().for_each(|(name, camera, scene)| {
        let frame = render(&SHEET, scene, *camera).unwrap();
        let path = fixtures().join(format!("frames/{name}.png"));
        if write {
            std::fs::write(&path, png(&frame)).unwrap();
        }
        let image = image::open(&path).unwrap().to_luma8();
        assert_eq!(image.into_raw(), frame.luma, "{name} drifted; regenerate with tools/optical/golden.sh");
    });
}

#[test]
fn the_python_goldens_cover_every_fixture_frame() {
    let golden: Value =
        serde_json::from_str(&std::fs::read_to_string(fixtures().join("python_golden.json")).unwrap()).unwrap();
    assert_eq!(golden["source"]["commit"], "220aa0e");
    let names: Vec<&str> =
        golden["frames"].as_array().unwrap().iter().map(|case| case["name"].as_str().unwrap()).collect();
    assert_eq!(names, cases().iter().map(|(name, _, _)| *name).collect::<Vec<_>>());
    let sharp = &golden["frames"][0];
    let centre = &sharp["findCentre"]["centrePx"];
    let truth = &sharp["truth"]["centrePx"];
    assert!((centre[0].as_f64().unwrap() - truth[0].as_f64().unwrap()).abs() < 1.0, "{centre} vs {truth}");
    assert!(golden["frames"][8]["findCentre"].is_null(), "Python finds nothing off the grid");
    assert!(golden["csv"].as_array().unwrap().iter().all(|case| case["text"].as_str().unwrap().lines().count() == 31));
}

fn config(fps: f64) -> CameraConfig {
    CameraConfig { width: 320, height: 240, fps }
}

#[test]
fn the_synthetic_camera_looks_at_the_sled() {
    let sled = Arc::new(Mutex::new((0.0, 0.0)));
    let reader = sled.clone();
    let mut camera = SyntheticCamera::new(Arc::new(move || *reader.lock().unwrap()));
    camera.scene.px_per_mm = 4.0;
    assert_eq!(
        camera.next_frame(Instant::now()).unwrap_err(),
        OpticalError::Camera("the camera is not started".into())
    );
    camera.start(&config(1000.0)).unwrap();
    let soon = || Instant::now() + Duration::from_secs(1);
    let centred = camera.next_frame(soon()).unwrap();
    assert_eq!((centred.width, centred.height), (320, 240));
    *sled.lock().unwrap() = (-5.0, 0.0);
    let moved = camera.next_frame(soon()).unwrap();
    let found = centroid(&moved, PixelPoint { x: 180.0, y: 120.0 }, 45.0, |_, _| PAPER);
    close(found, PixelPoint { x: 180.0, y: 120.0 }, 0.02);
    camera.scene.noise_sigma = 3.0;
    assert_ne!(camera.next_frame(soon()).unwrap(), camera.next_frame(soon()).unwrap(), "fresh noise per frame");
    camera.stop();
    assert!(camera.next_frame(soon()).is_err());
}

#[test]
fn cameras_keep_to_their_frame_rate() {
    let mut camera = SyntheticCamera::new(Arc::new(|| (0.0, 0.0)));
    camera.start(&config(4.0)).unwrap();
    camera.next_frame(Instant::now() + Duration::from_secs(1)).unwrap();
    assert_eq!(camera.next_frame(Instant::now()).unwrap_err(), OpticalError::NoFrame, "the next frame is 250 ms away");
    let before = Instant::now();
    camera.next_frame(before + Duration::from_secs(1)).unwrap();
    assert!(before.elapsed() >= Duration::from_millis(100), "{:?}", before.elapsed());
    [config(0.0), config(f64::NAN), CameraConfig { width: 0, ..config(5.0) }].iter().for_each(|bad| {
        let why = camera.start(bad).unwrap_err().to_string();
        assert!(why.ends_with("fps are not usable"), "{why}");
    });
}

#[test]
fn the_directory_camera_loops_over_its_pngs_in_name_order() {
    let dir = std::env::temp_dir().join(format!("wc-optical-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let frames: Vec<Frame> = [2.0, -2.0].iter().map(|x| render(&SHEET, &small(), (*x, 0.0)).unwrap()).collect();
    std::fs::write(dir.join("b.PNG"), png(&frames[1])).unwrap();
    std::fs::write(dir.join("a.png"), png(&frames[0])).unwrap();
    std::fs::write(dir.join("notes.txt"), "not a frame").unwrap();
    let mut camera = DirCamera::new(&dir);
    assert!(camera.next_frame(Instant::now()).is_err(), "not started");
    camera.start(&config(1000.0)).unwrap();
    let soon = || Instant::now() + Duration::from_secs(1);
    let seen: Vec<Frame> = (0..3).map(|_| camera.next_frame(soon()).unwrap()).collect();
    assert_eq!(seen, vec![frames[0].clone(), frames[1].clone(), frames[0].clone()]);
    camera.stop();
    assert!(camera.next_frame(soon()).is_err());
    std::fs::write(dir.join("c.png"), "not a png").unwrap();
    let broken = camera.start(&config(5.0)).unwrap_err().to_string();
    assert!(broken.starts_with("Camera: ") && broken.contains("c.png"), "{broken}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_directory_camera_needs_frames() {
    let missing = DirCamera::new(Path::new("/no/such/frames")).start(&config(5.0)).unwrap_err();
    assert!(matches!(missing, OpticalError::Camera(_)), "{missing:?}");
    let empty = std::env::temp_dir().join(format!("wc-optical-empty-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let why = DirCamera::new(&empty).start(&config(5.0)).unwrap_err().to_string();
    assert!(why.ends_with("no PNG frames"), "{why}");
    std::fs::remove_dir_all(&empty).unwrap();
    let invalid = DirCamera::new(&fixtures().join("frames")).start(&config(0.0)).unwrap_err();
    assert!(matches!(invalid, OpticalError::Invalid(_)));
    let mut fixtures = DirCamera::new(&fixtures().join("frames"));
    fixtures.start(&config(1000.0)).unwrap();
    assert_eq!(fixtures.next_frame(Instant::now() + Duration::from_secs(1)).unwrap().width, 320);
}

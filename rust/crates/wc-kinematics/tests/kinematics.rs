//! Ported from `tests/test_holey_kinematics.py` and the kinematics half of `tests/test_holey_calibration_model.py`.
#![allow(clippy::approx_constant)]

use wc_kinematics::{CHAIN_DENSITY, ChainFeed, FIRMWARE_PI, Geometry, Kinematics, Mode, python_float, sprocket_radius};

fn triangular() -> Kinematics {
    Kinematics::new(Geometry { mode: Mode::Triangular, ..Geometry::default() })
}

/// `calibration_fixture_cases.simulator`: the default machine with a 10-tooth sprocket, fed from the bottom.
fn simulator(mode: Mode) -> Kinematics {
    Kinematics::new(Geometry {
        r: sprocket_radius(10.0 * 6.35),
        rotation_disk_radius: 139.1,
        chain_over_sprocket: ChainFeed::Bottom,
        mode,
        ..Geometry::default()
    })
}

fn near(actual: (f64, f64), expected: (f64, f64), tolerance: f64) -> bool {
    (actual.0 - expected.0).abs() <= tolerance && (actual.1 - expected.1).abs() <= tolerance
}

#[test]
fn the_firmware_constants() {
    assert_eq!(FIRMWARE_PI, 3.14159);
    assert_eq!(CHAIN_DENSITY, 0.14 * 9.8 / 1000.0);
    assert_eq!(sprocket_radius(63.5), 63.5 / (2.0 * 3.14159));
    assert_eq!(Geometry::default().chain_density, CHAIN_DENSITY);
}

#[test]
fn the_simulator_accepts_the_stored_string_values() {
    type Read = fn(&Geometry) -> f64;
    let cases: [(u32, &str, Read, f64); 13] = [
        (2, "2990.5", |g| g.d, 2990.5),
        (13, "63.5", |g| g.r, 63.5 / (2.0 * FIRMWARE_PI)),
        (13, "76.2", |g| g.r, 76.2 / (2.0 * FIRMWARE_PI)),
        (1, "1219.2", |g| g.machine_height, 1219.2),
        (0, "2438.4", |g| g.machine_width, 2438.4),
        (3, "470", |g| g.motor_offset_y, 470.0),
        (40, "0.25", |g| g.left_chain_tolerance, 0.25),
        (41, "-0.25", |g| g.right_chain_tolerance, -0.25),
        (45, "5.1685e-6", |g| g.chain_elasticity, 5.1685e-6),
        (46, "97.9", |g| g.sled_weight, 97.9),
        (8, "139.1", |g| g.rotation_disk_radius, 139.1),
        (37, "33", |g| g.chain_sag_correction, 33.0),
        (2, " 2_990.5 ", |g| g.d, 2990.5),
    ];
    cases.iter().for_each(|(key, value, read, expected)| {
        let mut kinematics = Kinematics::new(Geometry::default());
        assert!(kinematics.update_setting(*key, value), "${key}={value}");
        assert_eq!(read(kinematics.geometry()), *expected, "${key}={value}");
    });
}

#[test]
fn the_chain_feed_setting_truncates_like_int() {
    ["2", "2.0", "2.9"].iter().for_each(|value| {
        let mut kinematics = Kinematics::new(Geometry::default());
        assert!(kinematics.update_setting(38, value));
        assert_eq!(kinematics.geometry().chain_over_sprocket, ChainFeed::Bottom, "{value}");
    });
    let mut kinematics = Kinematics::new(Geometry { chain_over_sprocket: ChainFeed::Bottom, ..Geometry::default() });
    assert!(kinematics.update_setting(38, "1.5"));
    assert_eq!(kinematics.geometry().chain_over_sprocket, ChainFeed::Top);
}

#[test]
fn the_kinematics_type_accepts_strings() {
    let mut kinematics = Kinematics::new(Geometry::default());
    assert!(kinematics.update_setting(7, "2"));
    assert_eq!(kinematics.geometry().mode, Mode::Triangular);
    assert!(kinematics.update_setting(7, "1"));
    assert_eq!(kinematics.geometry().mode, Mode::Quadrilateral);
}

#[test]
fn the_simulator_ignores_a_value_or_key_it_cannot_use() {
    let mut kinematics = Kinematics::new(Geometry::default());
    let before = kinematics.geometry().clone();
    assert!(!kinematics.update_setting(13, "not a number"));
    assert!(!kinematics.update_setting(99, "1"));
    assert!(!kinematics.update_setting(38, "inf"));
    assert!(!kinematics.update_setting(7, "nan"));
    assert_eq!(kinematics.geometry(), &before);
}

#[test]
fn python_float_reads_what_python_float_reads() {
    assert_eq!(python_float(" 12.5\n"), Some(12.5));
    assert_eq!(python_float("1_000"), Some(1000.0));
    assert_eq!(python_float("-1e3"), Some(-1000.0));
    assert_eq!(python_float("Infinity"), Some(f64::INFINITY));
    assert!(python_float("nan").is_some_and(f64::is_nan));
    ["", "abc", "_1", "1_", "1__0", "1_.5"].iter().for_each(|text| assert_eq!(python_float(text), None, "{text}"));
}

#[test]
fn forward_inverts_inverse_inside_the_work_area() {
    let mut kinematics = triangular();
    let (left, right) = kinematics.inverse(-600.0, 300.0).expect("chains");
    assert!(near(kinematics.forward(left, right).expect("position"), (-600.0, 300.0), 1e-6));
}

#[test]
fn forward_returns_none_when_it_cannot_converge() {
    [(100.0, 100.0), (4000.0, 100.0)].iter().for_each(|(left, right)| {
        assert_eq!(triangular().forward(*left, *right), None);
    });
}

#[test]
fn forward_finds_points_on_the_edge_of_the_work_area() {
    [(-1219.2, 609.6), (1219.2, 0.0), (0.0, -609.6)].iter().for_each(|target| {
        let mut kinematics = triangular();
        let (left, right) = kinematics.inverse(target.0, target.1).expect("chains");
        assert!(near(kinematics.forward(left, right).expect("position"), *target, 1e-6), "{target:?}");
    });
}

#[test]
fn the_clamp_moves_each_axis_to_the_edge_like_the_firmware() {
    let mut kinematics = triangular();
    let g = kinematics.geometry().clone();
    assert_eq!(g.clamp(-5000.0, 10.0), (-1219.2, 10.0));
    assert_eq!(g.clamp(20.0, -5000.0), (20.0, -609.6));
    assert_eq!(g.clamp(-5000.0, 5000.0), (-1219.2, 609.6));
    assert_eq!(g.clamp(5000.0, -5000.0), (1219.2, -609.6));
    assert!(g.clamp(f64::NAN, 0.0).0.is_nan());
    assert_eq!(kinematics.inverse(5000.0, 10.0), kinematics.inverse(1219.2, 10.0));
    assert_eq!(kinematics.inverse(-5000.0, 5000.0), kinematics.inverse(-1219.2, 609.6));
    assert_ne!(kinematics.inverse_unclamped(-5000.0, 5000.0), kinematics.inverse(-5000.0, 5000.0));
}

#[test]
fn a_top_fed_machine_round_trips() {
    let mut kinematics = Kinematics::new(Geometry {
        chain_over_sprocket: ChainFeed::Top,
        ..simulator(Mode::Triangular).geometry().clone()
    });
    let (left, right) = kinematics.inverse(400.0, -250.0).expect("chains");
    assert!(near(kinematics.forward(left, right).expect("position"), (400.0, -250.0), 1e-6));
}

#[test]
fn a_target_on_a_sprocket_has_no_chain_lengths() {
    let mut kinematics = simulator(Mode::Triangular);
    let g = kinematics.geometry().clone();
    let motor_y = g.machine_height / 2.0 + g.motor_offset_y;
    assert_eq!(kinematics.inverse_unclamped(-g.d / 2.0, motor_y), None);
    let mut quadrilateral = simulator(Mode::Quadrilateral);
    assert_eq!(quadrilateral.inverse_unclamped(-g.d / 2.0, 0.0), None);
}

#[test]
fn quadrilateral_forward_still_converges() {
    let mut kinematics = Kinematics::new(Geometry::default());
    let (left, right) = kinematics.inverse(300.0, -200.0).expect("chains");
    assert!(near(kinematics.forward(left, right).expect("position"), (300.0, -200.0), 0.01));
}

#[test]
fn quadrilateral_forward_places_the_sled_on_the_centre_line() {
    let mut kinematics = simulator(Mode::Quadrilateral);
    (-24..=24).map(|n| f64::from(n) * 25.0).for_each(|y| {
        let (left, right) = kinematics.inverse(0.0, y).expect("chains");
        let (fx, fy) = kinematics.forward_from(left, right, (0.0, y)).expect("position");
        assert!(fx.hypot(fy - y) < 0.1, "{y}: ({fx}, {fy})");
    });
}

#[test]
fn quadrilateral_answers_depend_on_the_previous_tilt_like_python() {
    let fresh = simulator(Mode::Quadrilateral).inverse(600.0, 300.0);
    let mut used = simulator(Mode::Quadrilateral);
    used.inverse(-1200.0, -600.0);
    assert_ne!(used.inverse(600.0, 300.0), fresh);
    let geometry = used.geometry().clone();
    used.set_geometry(geometry);
    assert_ne!(used.inverse(600.0, 300.0), fresh, "recomputing keeps the tilt, as recomputeGeometry does");
}

#[test]
fn a_sled_narrower_than_a_millimetre_still_solves() {
    let mut kinematics = Kinematics::new(Geometry { l: 0.5, ..simulator(Mode::Quadrilateral).geometry().clone() });
    assert!(kinematics.inverse(0.0, 0.0).is_some());
}

#[test]
fn forward_refuses_chain_lengths_that_are_not_numbers() {
    assert_eq!(triangular().forward(f64::NAN, 1500.0), None);
    assert_eq!(triangular().forward(1500.0, f64::INFINITY), None);
}

#[test]
fn a_singular_quadrilateral_jacobian_gives_no_chains_and_no_position() {
    let pointlike = Geometry { l: 0.0, s: 0.0, h3: 0.0, mode: Mode::Quadrilateral, ..Geometry::default() };
    let mut kinematics = Kinematics::new(pointlike);
    assert_eq!(kinematics.inverse_unclamped(-989.2, 100.0), None);
    assert_eq!(kinematics.forward_from(1000.0, 2500.0, (-989.2, 100.0)), None);
    assert_eq!(triangular().inverse_unclamped(f64::NAN, 0.0), None);
}

#[test]
fn quadrilateral_forward_settles_for_its_last_guess_after_5001_steps_like_python() {
    let mut kinematics = simulator(Mode::Quadrilateral);
    let (left, right) = kinematics.inverse(0.0, 375.0).expect("chains");
    let found = kinematics.forward_from(left, right, (0.0, 375.0)).expect("position");
    assert!(near(found, (0.0035928456562487597, 375.006805620823), 1e-9), "{found:?}");
}

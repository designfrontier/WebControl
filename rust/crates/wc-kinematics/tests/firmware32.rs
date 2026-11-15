//! `tests/fixtures/firmware32.json`: the firmware's triangular inverse and the forward solve, computed in C with
//! float32 by `firmware32.c` (rebuild with `regenerate.sh`).

use serde_json::Value;
use wc_kinematics::{ChainFeed, Firmware32, Geometry, Kinematics, Mode};

fn fixture() -> Value {
    let path = format!("{}/tests/fixtures/firmware32.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn number(value: &Value) -> f64 {
    value.as_f64().expect("number")
}

fn geometry(machine: &Value) -> Geometry {
    Geometry {
        d: number(&machine["d"]),
        r: number(&machine["r"]),
        machine_width: number(&machine["machineWidth"]),
        machine_height: number(&machine["machineHeight"]),
        chain_sag_correction: number(&machine["chainSagCorrection"]),
        motor_offset_y: number(&machine["motorOffsetY"]),
        sled_weight: number(&machine["sledWeight"]),
        chain_elasticity: number(&machine["chainElasticity"]),
        left_chain_tolerance: number(&machine["leftChainTolerance"]),
        right_chain_tolerance: number(&machine["rightChainTolerance"]),
        rotation_disk_radius: number(&machine["rotationDiskRadius"]),
        chain_over_sprocket: ChainFeed::from_setting(machine["chainOverSprocket"].as_i64().expect("feed")),
        mode: Mode::Triangular,
        ..Geometry::default()
    }
}

/// How many float32 steps apart two values are.
fn ulps(a: f32, b: f32) -> u32 {
    (a.to_bits() as i64 - b.to_bits() as i64).unsigned_abs() as u32
}

fn f32_of(value: &Value) -> f32 {
    number(value) as f32
}

/// Two float32 steps allow for libm's last bit, which may differ from the Mac that generated the fixture.
#[test]
fn the_inverse_is_the_firmware_s_float32_inverse() {
    fixture()["machines"].as_array().expect("machines").iter().for_each(|case| {
        let model = Firmware32::from(&geometry(&case["machine"]));
        case["inverse"].as_array().expect("inverse").iter().for_each(|point| {
            let (left, right) = model.inverse(f32_of(&point["x"]), f32_of(&point["y"]));
            let expected = (f32_of(&point["left"]), f32_of(&point["right"]));
            assert!(ulps(left, expected.0) <= 2 && ulps(right, expected.1) <= 2, "{point}: ({left}, {right})");
        });
    });
}

fn near(actual: (f64, f64), expected: &Value, tolerance: f64) -> bool {
    (actual.0 - number(&expected[0])).abs() <= tolerance && (actual.1 - number(&expected[1])).abs() <= tolerance
}

/// A float32 position within a few of the solve's last steps of the C one, where the `f64` model is further off.
#[test]
fn the_computed_position_is_the_firmware_s_float32_forward() {
    fixture()["machines"].as_array().expect("machines").iter().for_each(|case| {
        let mut kinematics = Kinematics::new(geometry(&case["machine"]));
        case["forward"].as_array().expect("forward").iter().for_each(|point| {
            let (left, right) = (number(&point["left"]), number(&point["right"]));
            let guess = (number(&point["guess"][0]), number(&point["guess"][1]));
            let found = kinematics.forward_as_firmware(left, right, guess).expect("position");
            assert_eq!((found.0 as f32 as f64, found.1 as f32 as f64), found, "{point}");
            assert!(near(found, &point["position"], 3e-4), "{point}: {found:?}");
            let wide = kinematics.forward_from(left, right, guess).expect("f64 position");
            assert!(!near(wide, &point["position"], 5e-4), "{point}: the f64 model gives {wide:?} as well");
        });
    });
}

#[test]
fn unreachable_chains_have_no_float32_position() {
    let mut kinematics = Kinematics::new(geometry(&fixture()["machines"][0]["machine"]));
    assert_eq!(kinematics.forward_as_firmware(10.0, 9000.0, (0.0, 0.0)), None);
    assert_eq!(kinematics.forward_as_firmware(f64::NAN, 1650.0, (0.0, 0.0)), None);
}

/// Quadrilateral has no float32 port, so it keeps the `f64` model (whose solver carries state, hence two models).
#[test]
fn quadrilateral_keeps_the_f64_model() {
    let (left, right) = (1650.0, 1651.0);
    let expected = Kinematics::new(Geometry::default()).forward_from(left, right, (0.0, 100.0));
    assert!(expected.is_some());
    assert_eq!(Kinematics::new(Geometry::default()).forward_as_firmware(left, right, (0.0, 100.0)), expected);
}

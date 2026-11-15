//! M5's wire types: the triangular and holey answers against their closed schemas, and the forms' field checks
//! (ported from test_api_calibration).

use serde_json::{Value, json};
use wc_protocol::calibration::{
    HoleyCalculation, HoleyForm, HoleyParameters, TriangularCalculation, TriangularForm, TriangularParameters,
};
use wc_testkit::contract::Contract;

fn check(schema: &str, value: &impl serde::Serialize) -> Value {
    let value = serde_json::to_value(value).unwrap();
    Contract::shared().check(&format!("schemas/http/{schema}.json"), &value).unwrap();
    value
}

#[test]
fn triangular_answers_fit_their_closed_schemas() {
    let parameters = TriangularParameters { motor_yoffset: 463.0, rotation_radius: 139.1, chain_sag_correction: 33.0 };
    let value = check("get-api-calibration-triangular.200", &parameters);
    assert_eq!(value, json!({"motorYoffset": 463.0, "rotationRadius": 139.1, "chainSagCorrection": 33.0}));
    check("post-api-calibration-triangular-accept.200", &parameters);
    let calculation = TriangularCalculation { parameters, calibration_error: -0.13 };
    assert_eq!(check("post-api-calibration-triangular.200", &calculation)["calibrationError"], -0.13);
}

#[test]
fn holey_answers_fit_their_closed_schemas() {
    let parameters = HoleyParameters {
        motor_yoffset: 462.64616,
        distance_between_motors: 2978.54418,
        left_chain_tolerance: -0.03767,
        right_chain_tolerance: 0.04275,
    };
    let value = check("get-api-calibration-holey.200", &parameters);
    assert_eq!(value["distanceBetweenMotors"], 2978.54418);
    check("post-api-calibration-holey-accept.200", &parameters);
    let calculation = HoleyCalculation { parameters, calibration_error: 1.29 };
    assert_eq!(check("post-api-calibration-holey.200", &calculation)["calibrationError"], 1.29);
}

#[test]
fn the_triangular_form_takes_numbers_or_numeric_text() {
    let body = json!({"cut12": 1930, "cut34": "1930", "cut5": " 300 ", "bitDiameter": 6.35});
    Contract::shared().validate_request("POST", "/api/calibration/triangular", &body).unwrap();
    assert_eq!(
        TriangularForm::parse(&body),
        Ok(TriangularForm { cut12: 1930.0, cut34: 1930.0, cut5: 300.0, bit_diameter: 6.35 })
    );
}

#[test]
fn the_triangular_form_names_the_first_field_that_is_not_a_number() {
    let good = json!({"cut12": 1930, "cut34": 1930, "cut5": 300, "bitDiameter": 6.35});
    let with = |key: &str, value: Value| {
        let mut body = good.clone();
        body[key] = value;
        TriangularForm::parse(&body).unwrap_err()
    };
    assert_eq!(with("cut12", json!("abc")), "Please enter a number for the distance between cut 1 and cut 2.");
    assert_eq!(with("cut34", Value::Null), "Please enter a number for the distance between cut 3 and cut 4.");
    assert_eq!(
        with("cut5", json!(true)),
        "Please enter a number for the distance between the top of the work area and cut 5."
    );
    assert_eq!(with("bitDiameter", json!("nan")), "Please enter a number for the bit diameter.");
    assert_eq!(
        TriangularForm::parse(&json!([1, 2])).unwrap_err(),
        "Please enter a number for the distance between cut 1 and cut 2."
    );
}

#[test]
fn the_holey_form_needs_twelve_numbers() {
    let mut values: Vec<Value> = (1..=12).map(|n| json!(n as f64 * 100.0)).collect();
    values[0] = json!("100");
    let body = json!({ "measurements": values });
    Contract::shared().validate_request("POST", "/api/calibration/holey", &body).unwrap();
    let form = HoleyForm::parse(&body).unwrap();
    assert_eq!(form.measurements[0], 100.0);
    assert_eq!(form.measurements[11], 1200.0);
    let all = "Please enter all 12 measurements.";
    assert_eq!(HoleyForm::parse(&json!({})).unwrap_err(), all);
    assert_eq!(HoleyForm::parse(&json!({"measurements": vec![1; 11]})).unwrap_err(), all);
    assert_eq!(HoleyForm::parse(&json!({"measurements": "12"})).unwrap_err(), all);
    let mut bad = vec![json!(1); 11];
    bad.push(json!("x"));
    assert_eq!(
        HoleyForm::parse(&json!({ "measurements": bad })).unwrap_err(),
        "Please enter a number for the distance M12."
    );
}

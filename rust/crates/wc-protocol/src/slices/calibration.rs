//! The `calibration` slice: triangular and holey parameters, their calculations and the forms they read.
//! Optical calibration is out of scope.

use serde::Serialize;
use serde_json::Value;

use crate::float;

/// `GET /api/calibration/triangular` and the answer to its accept.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriangularParameters {
    pub motor_yoffset: f64,
    pub rotation_radius: f64,
    pub chain_sag_correction: f64,
}

/// `POST /api/calibration/triangular`: the calculated values, unsaved.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriangularCalculation {
    #[serde(flatten)]
    pub parameters: TriangularParameters,
    pub calibration_error: f64,
}

/// `GET /api/calibration/holey` and the answer to its accept.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoleyParameters {
    pub motor_yoffset: f64,
    pub distance_between_motors: f64,
    pub left_chain_tolerance: f64,
    pub right_chain_tolerance: f64,
}

/// `POST /api/calibration/holey`: the fit, unsaved.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoleyCalculation {
    #[serde(flatten)]
    pub parameters: HoleyParameters,
    pub calibration_error: f64,
}

/// A `POST /api/calibration/triangular` body, in mm; the ranges depend on the bed and are checked by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriangularForm {
    pub cut12: f64,
    pub cut34: f64,
    pub cut5: f64,
    pub bit_diameter: f64,
}

const TRIANGULAR_FIELDS: [(&str, &str); 4] = [
    ("cut12", "the distance between cut 1 and cut 2"),
    ("cut34", "the distance between cut 3 and cut 4"),
    ("cut5", "the distance between the top of the work area and cut 5"),
    ("bitDiameter", "the bit diameter"),
];

fn measurement(value: Option<&Value>, label: &str) -> Result<f64, String> {
    value.and_then(float).ok_or_else(|| format!("Please enter a number for {label}."))
}

impl TriangularForm {
    /// `api/calibration.py`'s field checks; a body that is not an object reads as `{}`.
    pub fn parse(body: &Value) -> Result<TriangularForm, String> {
        let values: Vec<f64> =
            TRIANGULAR_FIELDS.iter().map(|(key, label)| measurement(body.get(key), label)).collect::<Result<_, _>>()?;
        Ok(TriangularForm { cut12: values[0], cut34: values[1], cut5: values[2], bit_diameter: values[3] })
    }
}

/// A `POST /api/calibration/holey` body: M1 to M12 in mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoleyForm {
    pub measurements: [f64; HoleyForm::COUNT],
}

impl HoleyForm {
    pub const COUNT: usize = 12;

    pub fn parse(body: &Value) -> Result<HoleyForm, String> {
        let all = format!("Please enter all {} measurements.", HoleyForm::COUNT);
        let values = body.get("measurements").and_then(Value::as_array).filter(|v| v.len() == HoleyForm::COUNT);
        let values = values.ok_or(all)?;
        let mut measurements = [0.0; HoleyForm::COUNT];
        values.iter().zip(measurements.iter_mut()).enumerate().try_for_each(|(i, (value, slot))| {
            *slot = measurement(Some(value), &format!("the distance M{}", i + 1))?;
            Ok::<_, String>(())
        })?;
        Ok(HoleyForm { measurements })
    }
}

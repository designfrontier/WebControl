//! `Actions/triangularCalibration.py`: five marks cut near the corners and the top middle, three distances
//! measured, and an iteration for the motor height, rotation radius and chain sag that explains them.

use serde_json::Value;
use wc_config::python_float_str;
use wc_kinematics::{ChainFeed, FIRMWARE_PI, sprocket_radius};

use crate::accept::{Accept, SettingWrite};
use crate::{CalibrationError, MachineSettings, python};

const TOLERANCE: f64 = 0.0001;
const ITERATIONS: u32 = 5000;

/// The four form fields, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inputs {
    pub cut12: f64,
    pub cut34: f64,
    pub cut5: f64,
    pub bit_diameter: f64,
}

/// Motor height and rotation radius rounded to 0.1 mm and chain sag to 6 places, as Python rounds them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriangularResult {
    pub motor_y_offset: f64,
    pub rotation_radius: f64,
    pub chain_sag_correction: f64,
    pub cut34_y_offset: f64,
}

const FIELDS: [(&str, &str); 4] = [
    ("cut12", "the distance between cut 1 and cut 2"),
    ("cut34", "the distance between cut 3 and cut 4"),
    ("cut5", "the distance between the top of the work area and cut 5"),
    ("bitDiameter", "the bit diameter"),
];

/// Reads the form and applies `calculate`'s range checks, in Python's order.
pub fn parse_inputs(settings: &MachineSettings, form: &Value) -> Result<Inputs, CalibrationError> {
    let mut values = [0.0; 4];
    FIELDS.iter().zip(values.iter_mut()).try_for_each(|((key, label), value)| {
        *value = python::float(&form[*key]).ok_or_else(|| CalibrationError::NotANumber(label.to_string()))?;
        Ok(())
    })?;
    let [cut12, cut34, cut5, bit_diameter] = values;
    let (width, height) = (settings.bed_width, settings.bed_height);
    let checks = [
        (cut12, width / 2.0, width, "measurement between cut 1 and cut 2"),
        (cut34, width / 2.0, width, "measurement between cut 3 and cut 4"),
        (cut5, 0.0, height / 2.0, "measurement between the top edge of the work area and cut 5"),
        (bit_diameter, 0.0, 25.4, "bit diameter value"),
    ];
    match checks.iter().find(|(value, low, high, _)| value > high || value < low) {
        Some((value, _, _, what)) => Err(CalibrationError::OutOfRange { what, value: *value }),
        None => Ok(Inputs { cut12, cut34, cut5, bit_diameter }),
    }
}

/// One cut's chain: its angle below horizontal, the length wrapped on the sprocket and the straight run.
#[derive(Clone, Copy)]
struct Chain {
    angle: f64,
    around: f64,
    straight: f64,
}

struct Model {
    radius: f64,
    feed: ChainFeed,
}

impl Model {
    fn chain(&self, dx: f64, dy: f64) -> Chain {
        let r = self.radius;
        let distance = (dx * dx + dy * dy).sqrt();
        let (angle, around) = match self.feed {
            ChainFeed::Top => {
                let angle = (dy / distance).asin() + (r / distance).asin();
                (angle, r * angle)
            }
            ChainFeed::Bottom => {
                let angle = (dy / distance).asin() - (r / distance).asin();
                (angle, r * (FIRMWARE_PI - angle))
            }
        };
        Chain { angle, around, straight: (distance * distance - r * r).sqrt() }
    }

    /// Chain lengths for cuts 1 to 4, each pair sagging toward its partner, less the rotation radius.
    fn lengths(&self, offsets: [(f64, f64); 4], sag: f64, rotation_radius: f64) -> [f64; 4] {
        let chains = offsets.map(|(dx, dy)| self.chain(dx, dy));
        let partner = [1, 0, 3, 2];
        let mut lengths = [0.0; 4];
        lengths.iter_mut().enumerate().for_each(|(i, length)| {
            let (own, other) = (chains[i], chains[partner[i]]);
            let lean = other.angle.tan() * own.angle.cos() + own.angle.sin();
            let cos = own.angle.cos();
            let straight = own.straight
                * (1.0 + ((sag / 1000000000000.0) * (cos * cos) * (own.straight * own.straight) * (lean * lean)));
            *length = own.around + straight - rotation_radius;
        });
        lengths
    }
}

/// `calculate`: reads and checks the form, then iterates.
pub fn calculate(settings: &MachineSettings, form: &Value) -> Result<TriangularResult, CalibrationError> {
    calculate_inputs(settings, &parse_inputs(settings, form)?)
}

pub fn calculate_inputs(settings: &MachineSettings, inputs: &Inputs) -> Result<TriangularResult, CalibrationError> {
    let (width, height) = (settings.bed_width, settings.bed_height);
    let model = Model { radius: sprocket_radius(settings.dist_per_rot()), feed: settings.chain_over_sprocket };
    let motor_x = settings.motor_spacing_x / 2.0;
    let configured_y = (height / 2.0) + settings.motor_offset_y;
    let (mut rotation_radius, mut sag) = (settings.rotation_radius, settings.chain_sag_correction);
    let (cut_x, cut_y) = ((width / 2.0) - 254.0, (height / 2.0) - 254.0);
    let measured = model.lengths(
        [
            (motor_x - cut_x, configured_y - cut_y),
            (motor_x + cut_x, configured_y - cut_y),
            (motor_x - cut_x, configured_y + cut_y),
            (motor_x + cut_x, configured_y + cut_y),
        ],
        sag,
        rotation_radius,
    );

    let start_y = inputs.cut5 + (inputs.bit_diameter / 2.0) + 12.7;
    let mut motor_y = start_y;
    let mut cut34_offset = 0.0;
    let mut scales = [0.5, 0.5, 1.0, 0.5];
    let mut errors = [TOLERANCE; 4];
    let mut n = 0;
    while errors.iter().any(|e| e.abs() >= TOLERANCE) && n < ITERATIONS {
        n += 1;
        let lower_y = motor_y + (height - 508.0) - cut34_offset;
        let estimated = model.lengths(
            [
                (motor_x - (inputs.cut12 / 2.0), motor_y),
                (motor_x + (inputs.cut12 / 2.0), motor_y),
                (motor_x - (inputs.cut34 / 2.0), lower_y),
                (motor_x + (inputs.cut34 / 2.0), lower_y),
            ],
            sag,
            rotation_radius,
        );
        if !estimated.iter().all(|length| length.is_finite()) {
            return Err(CalibrationError::NoSolution);
        }
        errors = [0, 1, 2, 3].map(|i| estimated[i] - measured[i]);
        let [motor_scale, radius_scale, sag_scale, offset_scale] = scales;
        motor_y -= errors[0] * motor_scale;
        rotation_radius += errors[1] * radius_scale;
        if rotation_radius < 0.0 {
            rotation_radius = 0.0;
        }
        if errors[0].abs() < 1.0 && errors[1].abs() < 1.0 {
            sag -= errors[3] * sag_scale;
            cut34_offset += ((errors[2] + errors[3]) / 2.0) * offset_scale;
            if sag < 0.0 {
                sag = 0.0;
            }
        }
        if motor_y < -(height / 4.0) || motor_y > (2.0 * height) || rotation_radius > height {
            (motor_y, rotation_radius, sag, cut34_offset) = (start_y, 0.0, 0.0, 0.0);
            scales = scales.map(|scale| scale / 2.0);
        }
    }
    if n == ITERATIONS {
        return Err(CalibrationError::NotCalibrated);
    }
    Ok(TriangularResult {
        motor_y_offset: python::round(motor_y - inputs.cut5 - (inputs.bit_diameter / 2.0) - 12.7, 1),
        rotation_radius: python::round(rotation_radius, 1),
        chain_sag_correction: python::round(sag, 6),
        cut34_y_offset: cut34_offset,
    })
}

impl TriangularResult {
    /// `acceptTriangularCalibrationResults`: saves the three values, then returns to the centre in millimetres.
    pub fn accept(&self) -> Accept {
        let write = |section, key, value: f64| SettingWrite { section, key, value: python_float_str(value) };
        Accept {
            writes: vec![
                write("Maslow Settings", "motorOffsetY", self.motor_y_offset),
                write("Advanced Settings", "rotationRadius", self.rotation_radius),
                write("Advanced Settings", "chainSagCorrection", self.chain_sag_correction),
            ],
            gcode: ["G21 ", "G90 ", "G40 ", "G0 X0 Y0 "].map(String::from).to_vec(),
        }
    }
}

/// `cutTriangularCalibrationPattern`, in relative moves from the centre; the lines keep Python's trailing spaces.
pub fn test_pattern(settings: &MachineSettings, inches: bool) -> Vec<String> {
    let (width, height) = (settings.bed_width, settings.bed_height);
    let number = python_float_str;
    let lines = [
        "G21 ".to_string(),
        "G90 ".into(),
        "G40 ".into(),
        "G0 Z5 ".into(),
        "G0 X0 Y0 ".into(),
        "G17 ".into(),
        "G91 ".into(),
        "M3 ".into(),
        format!("G0 X-{} Y+{} ", number((width / 2.0) - 254.0), number((height / 2.0) - 241.3)),
        "G1 Z-7 F500 ".into(),
        "G1 Y-25.4 ".into(),
        "G1 Z7 ".into(),
        format!("G0 Y-{} ", number(height - 533.4)),
        "G1 Z-7 ".into(),
        "G1 Y-25.4 ".into(),
        "G1 Z7 ".into(),
        format!("G0 X{} Y{} ", number(width - 533.4), number(height - 482.6)),
        "G1 Z-7 ".into(),
        "G1 X25.4 ".into(),
        "G1 Y-25.4 ".into(),
        "G1 Z7 ".into(),
        format!("G0 Y-{} ", number(height - 533.4)),
        "G1 Z-7 ".into(),
        "G1 Y-25.4 ".into(),
        "G1 Z7 ".into(),
        "M5 ".into(),
        "G90 ".into(),
        "G0 X0 Y0 ".into(),
    ];
    lines.into_iter().chain(inches.then(|| "G20 ".to_string())).collect()
}

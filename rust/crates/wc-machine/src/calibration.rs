//! Triangular and holey calibration (`api/calibration.py` and the calibration actions): Calculate's checks and
//! answer, the one result Accept may save, and the test patterns.

use serde_json::Value;
use wc_calibration::MachineSettings;
use wc_calibration::holey::{self, HoleyResult};
use wc_calibration::triangular::{self, Inputs, TriangularResult};
use wc_config::{Settings, python_float_str};
use wc_protocol::calibration::{
    HoleyCalculation, HoleyForm, HoleyParameters, TriangularCalculation, TriangularForm, TriangularParameters,
};
use wc_protocol::main::Units;

use crate::edits::Response;
use crate::machine::{Machine, RunState};

const MASLOW: &str = "Maslow Settings";
const ADVANCED: &str = "Advanced Settings";
const NOT_CALIBRATED: &str = "The machine could not be calibrated. Check the work area dimensions and try again.";
const NO_FIT: &str = "The calibration could not be calculated from these measurements.";
const NOTHING_TO_ACCEPT: &str =
    "There is no calculated result to accept. Enter the measurements and press Calculate first.";
const JOB_RUNNING: &str = "Stop the job before accepting a calibration result.";
pub(crate) const COMMANDS: [&str; 4] = [
    "cutTriangularCalibrationPattern",
    "acceptTriangularCalibrationResults",
    "cutHoleyCalibrationPattern",
    "acceptHoleyCalibrationResults",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Triangular,
    Holey,
}

/// Why Calculate gave no result: the HTTP status and the message the UI shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Refused {
    pub status: u16,
    pub message: String,
}

impl Refused {
    fn new(status: u16, message: &str) -> Refused {
        Refused { status, message: message.into() }
    }
}

/// A Calculate, worked out off the machine task so a fit never holds up Stop; the machine then records it.
#[derive(Debug, Clone, PartialEq)]
pub enum Calculated {
    Triangular(Result<TriangularResult, Refused>),
    Holey(Result<HoleyResult, Refused>),
}

/// The last successful Calculate of each kind; Python's `hasResult`, withdrawn by Accept and any failed Calculate.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Results {
    triangular: Option<TriangularResult>,
    holey: Option<HoleyResult>,
}

fn machine_settings(settings: &Settings) -> Option<MachineSettings> {
    MachineSettings::from_lookup(|section, key| settings.text(section, key)).ok()
}

/// `_check_range`, with the value as Python prints a float.
fn in_range(value: f64, low: f64, high: f64, label: &str) -> Result<(), Refused> {
    let message = || {
        format!(
            "The {label} of {} mm seems wrong. Please check the number and enter it again.",
            python_float_str(value)
        )
    };
    if value < low || value > high { Err(Refused { status: 400, message: message() }) } else { Ok(()) }
}

fn triangular_inputs(settings: &Settings, body: &Value) -> Result<Inputs, Refused> {
    let form = TriangularForm::parse(body).map_err(|message| Refused { status: 400, message })?;
    let (width, height) = (settings.number(MASLOW, "bedWidth"), settings.number(MASLOW, "bedHeight"));
    in_range(form.cut12, width / 2.0, width, "measurement between cut 1 and cut 2")?;
    in_range(form.cut34, width / 2.0, width, "measurement between cut 3 and cut 4")?;
    in_range(form.cut5, 0.0, height / 2.0, "measurement between the top edge of the work area and cut 5")?;
    in_range(form.bit_diameter, 0.0, 25.4, "bit diameter")?;
    Ok(Inputs { cut12: form.cut12, cut34: form.cut34, cut5: form.cut5, bit_diameter: form.bit_diameter })
}

fn calculate_triangular(settings: &Settings, body: &Value) -> Result<TriangularResult, Refused> {
    let inputs = triangular_inputs(settings, body)?;
    let machine = machine_settings(settings).ok_or_else(|| Refused::new(422, NOT_CALIBRATED))?;
    triangular::calculate_inputs(&machine, &inputs).map_err(|_| Refused::new(422, NOT_CALIBRATED))
}

fn calculate_holey(settings: &Settings, body: &Value) -> Result<HoleyResult, Refused> {
    let form = HoleyForm::parse(body).map_err(|message| Refused { status: 400, message })?;
    let machine = machine_settings(settings).ok_or_else(|| Refused::new(422, NO_FIT))?;
    holey::calibrate_measurements(&machine, &form.measurements).map_err(|_| Refused::new(422, NO_FIT))
}

/// `POST /api/calibration/{kind}`'s checks and calculation, from the settings in force.
pub fn calculate(kind: Kind, settings: &Settings, body: &Value) -> Calculated {
    match kind {
        Kind::Triangular => Calculated::Triangular(calculate_triangular(settings, body)),
        Kind::Holey => Calculated::Holey(calculate_holey(settings, body)),
    }
}

pub fn triangular_parameters(settings: &Settings) -> TriangularParameters {
    TriangularParameters {
        motor_yoffset: settings.number(MASLOW, "motorOffsetY"),
        rotation_radius: settings.number(ADVANCED, "rotationRadius"),
        chain_sag_correction: settings.number(ADVANCED, "chainSagCorrection"),
    }
}

pub fn holey_parameters(settings: &Settings) -> HoleyParameters {
    HoleyParameters {
        motor_yoffset: settings.number(MASLOW, "motorOffsetY"),
        distance_between_motors: settings.number(MASLOW, "motorSpacingX"),
        left_chain_tolerance: settings.number(ADVANCED, "leftChainTolerance"),
        right_chain_tolerance: settings.number(ADVANCED, "rightChainTolerance"),
    }
}

fn answer<T>(result: &Result<T, Refused>, body: impl FnOnce(&T) -> Value) -> Response {
    match result {
        Ok(calculated) => Response { status: 200, body: body(calculated) },
        Err(refused) => Response::error(refused.status, &refused.message),
    }
}

fn json(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("protocol types always serialise")
}

impl Machine {
    pub(crate) fn calibrated(&mut self, calculated: Calculated) -> Response {
        match calculated {
            Calculated::Triangular(result) => {
                self.calibration.triangular = result.clone().ok();
                answer(&result, |r| {
                    json(TriangularCalculation {
                        parameters: TriangularParameters {
                            motor_yoffset: r.motor_y_offset,
                            rotation_radius: r.rotation_radius,
                            chain_sag_correction: r.chain_sag_correction,
                        },
                        calibration_error: r.cut34_y_offset,
                    })
                })
            }
            Calculated::Holey(result) => {
                self.calibration.holey = result.clone().ok();
                answer(&result, |r| {
                    json(HoleyCalculation {
                        parameters: HoleyParameters {
                            motor_yoffset: r.motor_y_offset,
                            distance_between_motors: r.distance_between_motors,
                            left_chain_tolerance: r.left_chain_tolerance,
                            right_chain_tolerance: r.right_chain_tolerance,
                        },
                        calibration_error: r.total_error,
                    })
                })
            }
        }
    }

    /// Saves the last result through `setValue`, so the controller gets its `$` lines, and forgets it.
    fn accept_result(&mut self, kind: Kind) -> bool {
        let accept = match kind {
            Kind::Triangular => self.calibration.triangular.take().map(|result| result.accept()),
            Kind::Holey => self.calibration.holey.take().map(|result| result.accept()),
        };
        let Some(accept) = accept else { return false };
        accept.writes.iter().for_each(|write| {
            self.write_setting(write.section, write.key, &Value::from(write.value.clone()));
        });
        self.queue(accept.gcode);
        true
    }

    /// `POST /api/calibration/{kind}/accept`. Python took it mid-job or mid-probe; here it waits, as the action gates do.
    pub(crate) fn accept_calibration(&mut self, kind: Kind) -> Response {
        if self.run != RunState::Idle {
            return Response::error(409, JOB_RUNNING);
        }
        if self.probing {
            return Response::error(409, crate::actions::PROBING);
        }
        if !self.accept_result(kind) {
            return Response::error(409, NOTHING_TO_ACCEPT);
        }
        match kind {
            Kind::Triangular => Response { status: 200, body: json(triangular_parameters(&self.settings)) },
            Kind::Holey => Response { status: 200, body: json(holey_parameters(&self.settings)) },
        }
    }

    pub(crate) fn calibration_action(&mut self, command: &str) -> Option<String> {
        let (kind, name) = match command {
            "cutTriangularCalibrationPattern" | "acceptTriangularCalibrationResults" => {
                (Kind::Triangular, "triangular")
            }
            _ => (Kind::Holey, "holey"),
        };
        if command.starts_with("accept") {
            return (!self.accept_result(kind)).then(|| format!("Error with accepting {name} calibration results."));
        }
        let Some(settings) = machine_settings(&self.settings) else {
            return Some(format!("Error with cutting {name} calibration pattern."));
        };
        let inches = self.units() == Units::Inches;
        let pattern = match kind {
            Kind::Triangular => triangular::test_pattern(&settings, inches),
            Kind::Holey => holey::test_pattern(&settings, inches),
        };
        self.queue(pattern);
        None
    }
}

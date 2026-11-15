use std::fmt;

use wc_kinematics::{ChainFeed, python_float};

/// The WebControl settings the calibrations read, parsed from their stored strings.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineSettings {
    pub bed_width: f64,
    pub bed_height: f64,
    pub motor_spacing_x: f64,
    pub motor_offset_y: f64,
    pub sled_weight: f64,
    pub rotation_radius: f64,
    pub chain_sag_correction: f64,
    pub left_chain_tolerance: f64,
    pub right_chain_tolerance: f64,
    pub chain_over_sprocket: ChainFeed,
    pub gear_teeth: f64,
    pub chain_pitch: f64,
    /// `None` when the setting is absent, and the model keeps its default.
    pub chain_elasticity: Option<f64>,
}

/// A setting the calibration needs is missing or is not a number, where Python's `float()` would raise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
    pub section: String,
    pub key: String,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "The setting {} in {} is not a number.", self.key, self.section)
    }
}

impl std::error::Error for SettingsError {}

const MASLOW: &str = "Maslow Settings";
const ADVANCED: &str = "Advanced Settings";

impl MachineSettings {
    /// Reads each setting through `lookup(section, key)`, which gives the stored string or `None`.
    pub fn from_lookup(lookup: impl Fn(&str, &str) -> Option<String>) -> Result<MachineSettings, SettingsError> {
        let number = |section: &str, key: &str| {
            lookup(section, key)
                .as_deref()
                .and_then(python_float)
                .ok_or_else(|| SettingsError { section: section.into(), key: key.into() })
        };
        let chain_elasticity = match lookup(ADVANCED, "chainElasticity") {
            None => None,
            Some(_) => Some(number(ADVANCED, "chainElasticity")?),
        };
        let top = lookup(ADVANCED, "chainOverSprocket").as_deref() == Some("Top");
        Ok(MachineSettings {
            bed_width: number(MASLOW, "bedWidth")?,
            bed_height: number(MASLOW, "bedHeight")?,
            motor_spacing_x: number(MASLOW, "motorSpacingX")?,
            motor_offset_y: number(MASLOW, "motorOffsetY")?,
            sled_weight: number(MASLOW, "sledWeight")?,
            rotation_radius: number(ADVANCED, "rotationRadius")?,
            chain_sag_correction: number(ADVANCED, "chainSagCorrection")?,
            left_chain_tolerance: number(ADVANCED, "leftChainTolerance")?,
            right_chain_tolerance: number(ADVANCED, "rightChainTolerance")?,
            chain_over_sprocket: if top { ChainFeed::Top } else { ChainFeed::Bottom },
            gear_teeth: number(ADVANCED, "gearTeeth")?,
            chain_pitch: number(ADVANCED, "chainPitch")?,
            chain_elasticity,
        })
    }

    /// `distPerRot`, which `computeSettings` keeps equal to gear teeth times chain pitch.
    pub fn dist_per_rot(&self) -> f64 {
        self.gear_teeth * self.chain_pitch
    }
}

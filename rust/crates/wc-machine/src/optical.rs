//! Experimental optical calibration (`spec/optical-calibration.md`); Track D builds the scan here.

use wc_config::Settings;

pub const ENABLED_SECTION: &str = "WebControl Settings";
pub const ENABLED_KEY: &str = "opticalCalibrationEnabled";
/// Its name in `GET /api/state`'s `experimental`.
pub const FEATURE: &str = "optical";
pub const OFF: &str = "Optical calibration is turned off. Turn on \"Optical calibration (experimental)\" in Settings > WebControl Settings.";

/// Read on every request, so the setting takes effect without a restart.
pub fn enabled(settings: &Settings) -> bool {
    matches!(settings.value(ENABLED_SECTION, ENABLED_KEY), Some(v) if v == 1 || v == true)
}

pub fn experimental(settings: &Settings) -> Vec<String> {
    enabled(settings).then(|| FEATURE.to_string()).into_iter().collect()
}

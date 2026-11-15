//! The `system` slice: version and capability info.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BundledFirmware {
    pub stock: Option<String>,
    pub holey: Option<String>,
    pub custom: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateStatus {
    pub available: bool,
    pub version: Option<String>,
}

/// `GET /api/system/info`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub version: String,
    pub platform: String,
    pub controller_firmware_version: f64,
    pub firmware: BundledFirmware,
    pub can_turn_off: bool,
    pub four_motor: bool,
    pub firmware_build: Option<String>,
    pub firmware_supports_zaxis_limit: bool,
    pub update: UpdateStatus,
}

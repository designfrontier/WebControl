//! The `system` slice: version and capability info.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::machine::PidKind;

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
    /// Only webcontrol-rs sends it; the UI shows the upgrade wizard when it is there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_upgrade: Option<FirmwareUpgrade>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeStage {
    Idle,
    Checking,
    Ready,
    Blocked,
    BackingUp,
    Flashing,
    Restarting,
    Migrating,
    SelfTest,
    Testing,
    Done,
    Failed,
    RollingBack,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Chains {
    pub left: f64,
    pub right: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MotorResult {
    pub motor: String,
    pub passed: Vec<bool>,
}

/// The holey 51.292 upgrade as `GET /api/system/info` reports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareUpgrade {
    pub target: String,
    /// The pinned image's SHA-256; `None` when this build carries no image.
    pub image_sha256: Option<String>,
    pub stage: UpgradeStage,
    pub message: String,
    /// The boot banner and B05 version pre-flight found.
    pub board: Option<String>,
    pub firmware: Option<String>,
    pub chains: Option<Chains>,
    pub chains_after: Option<Chains>,
    pub motors: Vec<MotorResult>,
    /// Where the `$$` dump, settings, chain lengths and flash read-back were saved.
    pub backup: Option<String>,
    pub can_rollback: bool,
    /// The version read off the board before the upgrade, which a rollback restores.
    pub rollback_firmware: Option<String>,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PidGains {
    pub kp: String,
    pub ki: String,
    pub kd: String,
}

/// `GET /api/system/pid`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PidSettings {
    pub velocity: PidGains,
    pub position: PidGains,
    pub four_motor: bool,
}

pub const PID_MOTORS: [&str; 4] = ["L", "R", "M", "N"];

/// A valid `POST /api/system/pid-test` body: each number as typed, since it is spliced into B13/B14.
#[derive(Debug, Clone, PartialEq)]
pub struct PidTest {
    pub kind: PidKind,
    pub motor: String,
    /// `(field, text)` in the order the old UI's action carried them.
    pub fields: Vec<(&'static str, String)>,
}

const VELOCITY_FIELDS: [&str; 7] = ["start", "stop", "steps", "version", "kp", "ki", "kd"];
const POSITION_FIELDS: [&str; 8] = ["start", "stop", "steps", "time", "version", "kp", "ki", "kd"];

/// `str(value).strip()` for what JSON carries; a float prints as Python's `repr` does for these magnitudes.
fn typed_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(n) if n.is_f64() => {
            let float = n.as_f64().unwrap_or_default();
            match float.abs() {
                magnitude if magnitude >= 1e16 || (magnitude < 1e-4 && magnitude > 0.0) => format!("{float:e}"),
                _ if float.fract() == 0.0 => format!("{float:.1}"),
                _ => float.to_string(),
            }
        }
        other => other.to_string(),
    }
}

/// `-?(\d+\.?\d*|\.\d+)`: the firmware's parser knows no exponents, `nan`, `inf` or separators.
fn plain_decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').map_or((digits, None), |(w, f)| (w, Some(f)));
    let all_digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());
    match fraction {
        None => !whole.is_empty() && all_digits(whole),
        Some(fraction) => all_digits(whole) && all_digits(fraction) && !(whole.is_empty() && fraction.is_empty()),
    }
}

impl PidTest {
    /// `api/system.py`'s `pid_test_action` checks, with its messages.
    pub fn parse(body: &Value) -> Result<PidTest, String> {
        let kind = match body.get("kind").and_then(Value::as_str) {
            Some("velocity") => PidKind::Velocity,
            Some("position") => PidKind::Position,
            _ => return Err("kind must be velocity or position".into()),
        };
        let motor = body
            .get("motor")
            .and_then(Value::as_str)
            .filter(|m| PID_MOTORS.contains(m))
            .ok_or_else(|| format!("motor must be one of {}", PID_MOTORS.join(", ")))?;
        let names: &[&'static str] = if kind == PidKind::Velocity { &VELOCITY_FIELDS } else { &POSITION_FIELDS };
        let text = |name: &str| body.get(name).filter(|v| v.is_string() || v.is_number()).map(typed_text);
        let bad: Vec<&str> = names.iter().copied().filter(|n| !text(n).is_some_and(|t| plain_decimal(&t))).collect();
        if !bad.is_empty() {
            return Err(format!("not numbers: {}", bad.join(", ")));
        }
        let fields = names.iter().map(|name| (*name, text(name).unwrap_or_default())).collect();
        Ok(PidTest { kind, motor: motor.into(), fields })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledBuild {
    pub version: String,
    pub commit: String,
}

/// A published GitHub release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestRelease {
    pub tag: String,
    pub name: String,
    pub published_at: Option<String>,
    pub url: String,
    pub notes: String,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateState {
    UpToDate,
    UpdateAvailable,
    DevBuild,
    Ahead,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateBackup {
    pub version: String,
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateAction {
    Update,
    Rollback,
}

/// Where `webcontrol-update --service` is; `Starting` is the server's, until the updater writes its first status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    Starting,
    Checking,
    Downloading,
    Verified,
    BackingUp,
    Installing,
    Restarting,
    HealthCheck,
    RollingBack,
    Finished,
}

/// `Interrupted` is the server's: the status file is unfinished but its unit is no longer running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateResult {
    Updated,
    UpToDate,
    RolledBack,
    RollbackFailed,
    Failed,
    Interrupted,
}

/// The last update or rollback started from the UI, from `/var/lib/webcontrol-rs/update.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub action: UpdateAction,
    pub phase: UpdatePhase,
    pub running: bool,
    pub from: Option<String>,
    pub to: Option<String>,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub result: Option<UpdateResult>,
    pub log: Vec<String>,
}

/// `GET /api/system/updates`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Updates {
    pub installed: InstalledBuild,
    pub repo: String,
    pub latest: Option<LatestRelease>,
    pub status: UpdateState,
    pub error: Option<String>,
    pub checked_at: Option<u64>,
    pub backup: Option<UpdateBackup>,
    pub can_update: bool,
    pub busy: Option<String>,
    pub progress: Option<UpdateProgress>,
}

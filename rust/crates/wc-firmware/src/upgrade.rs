//! The pure half of the one-click 51.292 or planner 51.30 upgrade (`spec/firmware.md` C): which boards and builds
//! may be upgraded, the avrdude command lines and how their output reads, and the settings the new firmware must be given.

use serde_json::Value;
use wc_config::Settings;

use crate::settings::{CUSTOM_FIRMWARE, Push, boot_push, setting_key, setting_line};

pub const TARGET_BUILD: &str = "51.292";
/// What B05 prints for every 51.29x build.
pub const TARGET_REPORTED: f64 = 51.29;
pub const PLANNER_BUILD: &str = "51.30";
pub const PLANNER_REPORTED: f64 = 51.3;

/// The build the guided upgrade flashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Holey,
    /// The experimental planner firmware; 51.292 is the way back.
    Planner,
}

impl Target {
    pub fn build(self) -> &'static str {
        match self {
            Target::Holey => TARGET_BUILD,
            Target::Planner => PLANNER_BUILD,
        }
    }

    pub fn check_version(self, version: f64, planner: bool) -> Result<(), String> {
        match self {
            Target::Holey => check_version(version, planner),
            Target::Planner => check_planner_version(version),
        }
    }

    /// What B05 must report once the build boots; the planner's capabilities are checked after it.
    pub fn reports(self, version: f64) -> bool {
        match self {
            Target::Holey => same_version(version, TARGET_REPORTED),
            Target::Planner => same_version(version, PLANNER_REPORTED),
        }
    }
}
pub const STOCK_Z_RPM: f64 = 12.6;
/// 51.292's EEPROM default for `$18`, ten times the stock Z motor's speed.
pub const FACTORY_Z_RPM_51_292: f64 = 126.0;
const Z_RPM_KEY: u32 = 18;
/// What `clearZ` writes: no Z limits, as on 51.27, which has none.
pub const CLEARED_Z_LIMITS: [(u32, f64); 2] = [(63, 9999.0), (64, -9999.0)];
/// 51.291 glues an `ok` onto its reply to these and misreads them.
const UNSAFE_ON_51_291: [u32; 2] = [60, 61];
/// L298 boards whose pins and straps read the same on 51.27 and 51.292.
const SAFE_BOARDS: [&str; 3] = ["1.0", "1.1", "1.2"];

/// A boot banner such as `PCB v1.6 TLE9201  Detected`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub pcb: String,
    pub drivers: Vec<String>,
}

pub fn parse_board(banner: &str) -> Option<Board> {
    let body = &banner[banner.find("PCB v")? + "PCB v".len()..];
    let body = body.trim_end().strip_suffix("Detected")?;
    let mut words = body.split_whitespace();
    let pcb = words.next().filter(|v| v.starts_with("1."))?.to_string();
    Some(Board { pcb, drivers: words.map(String::from).collect() })
}

/// Pre-flight's board check (C1): the board the upgrade is known to be safe on, or why it is blocked.
pub fn check_board(banner: Option<&str>) -> Result<Board, String> {
    let Some(banner) = banner else {
        return Err("No boot banner was seen since the controller connected, so the board is unknown. \
                    Power-cycle the controller, then check again."
            .into());
    };
    let board =
        parse_board(banner).ok_or_else(|| format!("The board banner '{banner}' is not one WebControl knows."))?;
    let has = |driver: &str| board.drivers.iter().any(|d| d == driver);
    if has("TLE9201") {
        return Err("51.29x reads the PCB v1.6 TLE9201 strap pattern as a TB6643 board and drives it with the wrong \
                    pins, and 51.292 swaps the TLE9201 left-encoder pins. The upgrade is blocked."
            .into());
    }
    if has("TLE5206") {
        return Err("One TLE5206 strap pattern hangs 51.29x at boot. The upgrade is blocked.".into());
    }
    match SAFE_BOARDS.contains(&board.pcb.as_str()) && board.drivers.is_empty() {
        true => Ok(board),
        false => Err(format!(
            "WebControl upgrades only the L298 boards known to be safe on 51.292 (PCB v1.0, v1.1, v1.2); this \
             controller reports '{banner}'."
        )),
    }
}

fn same_version(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Holey 51.27 and 51.28 share settings version 6; the upgrade starts from nothing else, bar a planner test build
/// (a 51.29 listing `planner`, or a later holey build such as 51.30) going back to 51.292.
pub fn check_version(version: f64, planner: bool) -> Result<(), String> {
    match version {
        v if [51.27, 51.28].iter().any(|from| same_version(v, *from)) => Ok(()),
        v if same_version(v, TARGET_REPORTED) && planner => Ok(()),
        v if v > TARGET_REPORTED && v < CUSTOM_FIRMWARE => Ok(()),
        v if same_version(v, TARGET_REPORTED) => Err("The controller already runs 51.29x.".into()),
        v if v <= 0.0 => Err("The controller never answered B05, so its firmware is unknown.".into()),
        v => Err(format!("Only holey 51.27 or 51.28 can be upgraded here; the controller reports {v}.")),
    }
}

pub fn reports_target(version: f64) -> bool {
    Target::Holey.reports(version)
}

/// 51.30 is reached from 51.27, 51.28 and any 51.29x, never from itself.
pub fn check_planner_version(version: f64) -> Result<(), String> {
    match version {
        v if [51.27, 51.28, TARGET_REPORTED].iter().any(|from| same_version(v, *from)) => Ok(()),
        v if same_version(v, PLANNER_REPORTED) => Err("The controller already runs 51.30.".into()),
        v if v <= 0.0 => Err("The controller never answered B05, so its firmware is unknown.".into()),
        v => Err(format!("Only holey 51.27, 51.28 or 51.29x can be upgraded to 51.30; the controller reports {v}.")),
    }
}

/// `Actions/firmware.py`'s `SERIAL_DEVICE`: avrdude never gets a port that could read as an option or a path.
pub fn is_serial_port(port: &str) -> bool {
    let name_char = |c: char| c.is_ascii_alphanumeric() || "._-".contains(c);
    let device = |rest: &str, extra: &str| !rest.is_empty() && rest.chars().all(|c| name_char(c) || extra.contains(c));
    if port.contains("..") {
        return false;
    }
    let tty = ["/dev/tty", "/dev/cu"].iter().any(|p| port.strip_prefix(p).is_some_and(|rest| device(rest, "")));
    let by = ["/dev/serial/by-id/", "/dev/serial/by-path/"]
        .iter()
        .any(|p| port.strip_prefix(p).is_some_and(|rest| device(rest, ":+")));
    let com =
        port.strip_prefix("COM").is_some_and(|n| (1..=3).contains(&n.len()) && n.chars().all(|c| c.is_ascii_digit()));
    tty || by || com
}

fn avrdude_args(port: &str, operation: String) -> Vec<String> {
    ["-patmega2560", "-cwiring", &format!("-P{port}"), "-b115200", "-D", &operation].map(String::from).to_vec()
}

/// Flash with the read-back verify avrdude does by default; `-D` keeps the bootloader and EEPROM.
pub fn write_args(port: &str, hex: &str) -> Vec<String> {
    avrdude_args(port, format!("-Uflash:w:{hex}:i"))
}

pub fn read_args(port: &str, hex: &str) -> Vec<String> {
    avrdude_args(port, format!("-Uflash:r:{hex}:i"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Avrdude {
    Done,
    /// The bootloader never answered; worth one retry (C4).
    SyncFailure,
    Failed,
}

pub fn avrdude_outcome(success: bool, output: &str) -> Avrdude {
    const SYNC: [&str; 4] = ["not in sync", "getsync", "timeout communicating", "programmer is not responding"];
    match success {
        true => Avrdude::Done,
        false if SYNC.iter().any(|marker| output.contains(marker)) => Avrdude::SyncFailure,
        false => Avrdude::Failed,
    }
}

/// What the new or restored firmware is given once it boots with factory settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Migration {
    /// To 51.292: `$18` never left at its factory 126, and Z limits cleared.
    Upgrade { preflight_z_rpm: Option<f64> },
    /// Back to the saved firmware, which reset its settings too (C8).
    Rollback,
}

fn z_rpm(settings: &Settings, preflight: Option<f64>) -> f64 {
    let usable = |v: f64| v > 0.0 && !same_version(v, FACTORY_Z_RPM_51_292);
    let stored = settings.value("Advanced Settings", "maxZRPM").and_then(wc_config::python_float);
    stored.filter(|v| usable(*v)).or(preflight.filter(|v| usable(*v))).unwrap_or(STOCK_Z_RPM)
}

/// C6: every setting the controller takes, blanks filled with their defaults, as after any EEPROM reset.
pub fn migration_push(settings: &Settings, firmware: f64, build: Option<&str>, migration: Migration) -> Push {
    let mut push = boot_push(settings, firmware, true);
    push.lines.retain(|line| {
        build == Some(TARGET_BUILD) || !setting_key(line).is_some_and(|k| UNSAFE_ON_51_291.contains(&k))
    });
    let Migration::Upgrade { preflight_z_rpm } = migration else {
        return push;
    };
    let rpm = z_rpm(settings, preflight_z_rpm);
    let line = setting_line(Z_RPM_KEY, &Value::from(rpm));
    match push.lines.iter_mut().find(|l| setting_key(l) == Some(Z_RPM_KEY)) {
        Some(existing) => *existing = line,
        None => push.lines.push(line),
    }
    push.adopted.retain(|(_, key, _)| key != "maxZRPM");
    push.adopted.push(("Advanced Settings".into(), "maxZRPM".into(), Value::from(wc_config::python_float_str(rpm))));
    push.lines.extend(CLEARED_Z_LIMITS.iter().map(|&(key, value)| setting_line(key, &Value::from(value))));
    push
}

/// One motor's B04 result: `Testing L motor:` then `Direction 1 - Pass` and `Direction 2 - Pass`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotorTest {
    pub motor: String,
    pub passed: Vec<bool>,
}

/// Reads one line of B04's report into the results so far; false if the line is not part of it.
pub fn read_motor_test(results: &mut Vec<MotorTest>, line: &str) -> bool {
    if let Some(motor) = line.strip_prefix("Testing ").and_then(|rest| rest.strip_suffix(" motor:")) {
        results.push(MotorTest { motor: motor.into(), passed: Vec::new() });
        return true;
    }
    let verdict = line.strip_prefix("Direction ").and_then(|rest| rest.split_once(" - ")).map(|(_, v)| v == "Pass");
    match (verdict, results.last_mut()) {
        (Some(pass), Some(test)) => {
            test.passed.push(pass);
            true
        }
        _ => line == "Tests complete.",
    }
}

/// C7: the motors of L, R and Z that did not pass both directions.
pub fn failed_motors(results: &[MotorTest]) -> Vec<&'static str> {
    let passed =
        |motor: &str| results.iter().any(|t| t.motor == motor && t.passed.len() == 2 && t.passed.iter().all(|p| *p));
    ["L", "R", "Z"].into_iter().filter(|motor| !passed(motor)).collect()
}

pub fn motor_test_passed(results: &[MotorTest]) -> bool {
    failed_motors(results).is_empty()
}

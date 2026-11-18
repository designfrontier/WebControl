//! Updating and rolling back from the Updates screen: `webcontrol-update@update.service` and `@rollback.service`,
//! started through the sudoers rule `install-pi.sh` adds, and the status file they write as root.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Deserialize;
use wc_machine::Snapshot;
use wc_protocol::main::PauseButton;
use wc_protocol::system::{UpdateAction, UpdatePhase, UpdateProgress, UpdateResult, UpgradeStage};

pub const STATUS_FILE: &str = "/var/lib/webcontrol-rs/update.json";
pub const UNIT_FILE: &str = "/etc/systemd/system/webcontrol-update@.service";
/// `install-pi.sh`'s sudoers rule allows exactly this, followed by one of [`unit`]'s names.
pub const START: [&str; 5] = ["sudo", "--non-interactive", "/usr/bin/systemctl", "start", "--no-block"];
pub const IS_ACTIVE: [&str; 2] = ["systemctl", "is-active"];
/// How long a start waits for the updater's first status before it counts as never started.
pub const START_GRACE: Duration = Duration::from_secs(30);
const LOG_LINES: usize = 200;

pub fn unit(action: UpdateAction) -> &'static str {
    match action {
        UpdateAction::Update => "webcontrol-update@update.service",
        UpdateAction::Rollback => "webcontrol-update@rollback.service",
    }
}

/// Starts the updater's units and asks whether one runs. Every call blocks, so each runs on a blocking thread.
pub trait Installer: Send + Sync + 'static {
    /// The units are installed, so the page can update.
    fn installed(&self) -> bool;
    fn running(&self) -> bool;
    fn start(&self, action: UpdateAction) -> Result<(), String>;
}

/// What a server built for tests has: no units.
pub struct Off;

impl Installer for Off {
    fn installed(&self) -> bool {
        false
    }

    fn running(&self) -> bool {
        false
    }

    fn start(&self, _: UpdateAction) -> Result<(), String> {
        Err("Updating is off in this server".into())
    }
}

/// systemd, through `sudo` for the start; tests point the commands elsewhere.
pub struct Systemd {
    pub unit_file: PathBuf,
    pub start: Vec<String>,
    pub is_active: Vec<String>,
}

impl Default for Systemd {
    fn default() -> Self {
        let strings = |words: &[&str]| words.iter().map(|word| word.to_string()).collect();
        Systemd { unit_file: UNIT_FILE.into(), start: strings(&START), is_active: strings(&IS_ACTIVE) }
    }
}

fn run(command: &[String], last: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(&command[0]).args(&command[1..]).args(last).output()
}

impl Installer for Systemd {
    fn installed(&self) -> bool {
        self.unit_file.is_file()
    }

    /// A oneshot unit is `activating` while it runs, which `is-active`'s exit status does not count.
    fn running(&self) -> bool {
        let units = [unit(UpdateAction::Update), unit(UpdateAction::Rollback)];
        run(&self.is_active, &units).is_ok_and(|output| {
            let states = String::from_utf8_lossy(&output.stdout);
            states.lines().any(|state| matches!(state.trim(), "active" | "activating" | "deactivating" | "reloading"))
        })
    }

    fn start(&self, action: UpdateAction) -> Result<(), String> {
        let output = run(&self.start, &[unit(action)]).map_err(|error| format!("Couldn't run sudo: {error}"))?;
        match output.status.success() {
            true => Ok(()),
            false => {
                Err(format!("Couldn't start {}: {}", unit(action), String::from_utf8_lossy(&output.stderr).trim()))
            }
        }
    }
}

/// Why updating now would cut something short, in `webcontrol-update`'s words, or `None` when the machine is idle.
pub fn busy_reason(snapshot: &Snapshot) -> Option<String> {
    use UpgradeStage::*;
    let reason = if snapshot.job_in_progress {
        "a job is running or paused"
    } else if snapshot.pause_button == PauseButton::Resume {
        "a job or tool change is held, waiting for Resume"
    } else if snapshot.manual_z_axis_adjust {
        "the controller is waiting for a tool change or a Z adjustment"
    } else if snapshot.probing {
        "a Z probe is in flight"
    } else if matches!(
        snapshot.firmware_upgrade.stage,
        BackingUp | Flashing | Restarting | Migrating | SelfTest | Testing | RollingBack
    ) {
        "a controller firmware upgrade is underway"
    } else if snapshot.optical.scan.is_some() {
        "an optical calibration scan is running"
    } else {
        return None;
    };
    Some(format!("Not now: {reason}. Finish or stop it first."))
}

/// The status file as `webcontrol-update --service` writes it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    action: UpdateAction,
    phase: UpdatePhase,
    from: Option<String>,
    to: Option<String>,
    started_at: Option<u64>,
    finished_at: Option<u64>,
    result: Option<UpdateResult>,
    #[serde(default)]
    log: Vec<String>,
}

/// A status file that is missing, half written or not one reads as no status.
pub fn read_status(path: &Path) -> Option<UpdateProgress> {
    let status: Status = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let skip = status.log.len().saturating_sub(LOG_LINES);
    Some(UpdateProgress {
        action: status.action,
        phase: status.phase,
        running: status.result.is_none(),
        from: status.from,
        to: status.to,
        started_at: status.started_at,
        finished_at: status.finished_at,
        result: status.result,
        log: status.log.into_iter().skip(skip).collect(),
    })
}

/// A start the updater has not answered yet.
#[derive(Debug, Clone, Copy)]
pub struct Pending {
    pub action: UpdateAction,
    pub at: Instant,
    pub at_ms: u64,
}

impl Pending {
    /// The updater's status counts from the second it started in, as it writes whole seconds.
    pub fn answered_by(&self, status: &UpdateProgress) -> bool {
        status.started_at.is_some_and(|started| started >= self.at_ms / 1000 * 1000)
    }

    fn progress(&self, phase: UpdatePhase, result: Option<UpdateResult>, log: Vec<String>) -> UpdateProgress {
        UpdateProgress {
            action: self.action,
            phase,
            running: result.is_none(),
            from: None,
            to: None,
            started_at: Some(self.at_ms),
            finished_at: None,
            result,
            log,
        }
    }
}

/// What the page shows, from the status file, an unanswered start and whether a unit runs.
pub fn progress(
    status: Option<UpdateProgress>,
    pending: Option<Pending>,
    unit_running: bool,
) -> Option<UpdateProgress> {
    if let Some(pending) = pending {
        if unit_running || pending.at.elapsed() < START_GRACE {
            return Some(pending.progress(UpdatePhase::Starting, None, Vec::new()));
        }
        let why = format!("The updater did not start; journalctl -u {} shows why.", unit(pending.action));
        return Some(pending.progress(UpdatePhase::Finished, Some(UpdateResult::Failed), vec![why]));
    }
    let status = status?;
    if status.result.is_some() || unit_running {
        return Some(status);
    }
    let mut log = status.log;
    log.push("The updater stopped before it finished; journalctl -u webcontrol-update@* shows why.".into());
    let result = Some(UpdateResult::Interrupted);
    Some(UpdateProgress { phase: UpdatePhase::Finished, running: false, result, log, ..status })
}

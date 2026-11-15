//! `upgradeHoleyFirmware`: the guided 51.27 -> 51.292 upgrade and its rollback (`spec/firmware.md` C1-C9). The
//! machine hands the port to avrdude through [`FirmwareJob`] effects and reads the results back as events.

use std::time::Duration;

use wc_firmware::image::{Image, rollback_image};
use wc_firmware::upgrade::{
    Avrdude, Migration, MotorTest, TARGET_BUILD, avrdude_outcome, check_board, check_version, failed_motors,
    is_serial_port, motor_test_passed, parse_board, read_motor_test, reports_target,
};
use wc_protocol::Action;
use wc_protocol::system::{Chains, FirmwareUpgrade, MotorResult, UpgradeStage as Stage};

use crate::machine::{Effect, Link, Machine, Queued, alert, broadcast};

pub(crate) const COMMAND: &str = "upgradeHoleyFirmware";
/// From the end of the flash to a B05 answer: a boot, the reset handshake and its 3 s fallback fit easily.
pub const POST_FLASH_WAIT: Duration = Duration::from_secs(30);
/// Chains measured further apart than this after the upgrade than before need setting again.
const CHAIN_TOLERANCE_MM: f64 = 1.0;
const LOG_LINES: usize = 40;
const NOT_CONNECTED: &str = "The controller is not connected.";

/// Work the shell runs off the machine task, in order, answering each with [`JobResult`].
#[derive(Debug, Clone, PartialEq)]
pub enum FirmwareJob {
    /// avrdude reads the controller's flash back as Intel HEX.
    ReadFlash {
        port: String,
    },
    /// Files for a new backup directory.
    Save {
        files: Vec<(String, String)>,
    },
    WriteFlash {
        port: String,
        image: Image,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobResult {
    /// The hex read, or avrdude's output when it failed.
    Read(Result<String, String>),
    /// The backup directory, or why it could not be written.
    Saved(Result<String, String>),
    Written {
        success: bool,
        output: String,
    },
}

/// A point in the queue the machine acts on once every line before it has been acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Checked,
    Migrated,
    Tested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Upgrade,
    Rollback,
}

#[derive(Debug, Clone)]
pub(crate) struct Upgrade {
    pub image: Option<Image>,
    stage: Stage,
    message: String,
    log: Vec<String>,
    board: Option<String>,
    firmware: Option<f64>,
    dump: Vec<String>,
    z_rpm: Option<f64>,
    chains: Vec<f64>,
    chains_after: Vec<f64>,
    motors: Vec<MotorTest>,
    backup: Option<String>,
    rollback: Option<Image>,
    port: String,
    attempt: u8,
    direction: Direction,
    deadline: Option<Duration>,
    banner_seen: bool,
    rewritten: Vec<String>,
}

impl Upgrade {
    pub fn new(image: Option<Image>) -> Upgrade {
        Upgrade {
            image,
            stage: Stage::Idle,
            message: String::new(),
            log: Vec::new(),
            board: None,
            firmware: None,
            dump: Vec::new(),
            z_rpm: None,
            chains: Vec::new(),
            chains_after: Vec::new(),
            motors: Vec::new(),
            backup: None,
            rollback: None,
            port: String::new(),
            attempt: 0,
            direction: Direction::Upgrade,
            deadline: None,
            banner_seen: false,
            rewritten: Vec::new(),
        }
    }

    /// avrdude or the reboot after it owns the controller; nothing else may reach it.
    pub fn holds_link(&self) -> bool {
        matches!(
            self.stage,
            Stage::BackingUp | Stage::Flashing | Stage::Restarting | Stage::Migrating | Stage::RollingBack
        )
    }

    fn busy(&self) -> bool {
        self.holds_link() || matches!(self.stage, Stage::Checking | Stage::Testing)
    }

    pub fn collects_chains(&self) -> bool {
        matches!(self.stage, Stage::Checking | Stage::Testing)
    }

    pub fn migrating(&self) -> Option<Migration> {
        match (self.stage, self.direction) {
            (Stage::Migrating, Direction::Upgrade) => Some(Migration::Upgrade { preflight_z_rpm: self.z_rpm }),
            (Stage::Migrating, Direction::Rollback) => Some(Migration::Rollback),
            _ => None,
        }
    }

    fn note(&mut self, text: &str) {
        self.log.push(text.to_string());
        let extra = self.log.len().saturating_sub(LOG_LINES);
        self.log.drain(..extra);
    }

    fn enter(&mut self, stage: Stage, message: &str) {
        self.stage = stage;
        self.message = message.to_string();
        self.note(message);
    }

    pub fn view(&self) -> FirmwareUpgrade {
        let chains = |c: &[f64]| match c {
            [left, right] => Some(Chains { left: *left, right: *right }),
            _ => None,
        };
        FirmwareUpgrade {
            target: TARGET_BUILD.into(),
            image_sha256: self.image.as_ref().map(|i| i.sha256.clone()),
            stage: self.stage,
            message: self.message.clone(),
            board: self.board.clone(),
            firmware: self.firmware.map(|v| format!("{v}")),
            chains: chains(&self.chains),
            chains_after: chains(&self.chains_after),
            motors: self
                .motors
                .iter()
                .map(|t| MotorResult { motor: t.motor.clone(), passed: t.passed.clone() })
                .collect(),
            backup: self.backup.clone(),
            can_rollback: self.rollback.is_some() && !self.busy(),
            log: self.log.clone(),
        }
    }
}

fn tail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join(" / ")
}

impl Machine {
    pub(crate) fn upgrade_action(
        &mut self,
        action: &Action,
        now: Duration,
        effects: &mut Vec<Effect>,
    ) -> Option<String> {
        let step = action.arg_text();
        if self.upgrade.busy() {
            return Some("Wait for the firmware upgrade step in progress to finish.".into());
        }
        match step.as_str() {
            "check" => self.preflight(),
            "upgrade" => self.start_upgrade(now, effects),
            "selfTest" => self.self_test(),
            "rollback" => self.start_rollback(now, effects),
            "reset" => {
                self.upgrade.enter(Stage::Idle, "Ready to check the controller.");
                None
            }
            _ => Some(
                "webcontrol-rs upgrades the holey firmware with the guided upgrade: open Actions, then \
                       Upgrade holey firmware."
                    .into(),
            ),
        }
    }

    /// Stock and custom firmware are only offered by the Python backend.
    pub(crate) fn other_firmware(&self, command: &str) -> String {
        let name = if command == "upgradeStockFirmware" { "stock" } else { "custom" };
        format!("webcontrol-rs only upgrades holey firmware. Switch to the Python backend to load the {name} firmware.")
    }

    /// C1: banner, B05, `$$` and both chain lengths, collected once everything queued before is answered.
    fn preflight(&mut self) -> Option<String> {
        let Link::Connected { port } = &self.link else {
            return Some(NOT_CONNECTED.into());
        };
        let image = self.upgrade.image.clone();
        let kept = (self.upgrade.rollback.take(), self.upgrade.backup.take(), std::mem::take(&mut self.upgrade.log));
        self.upgrade = Upgrade { port: port.clone(), ..Upgrade::new(image) };
        (self.upgrade.rollback, self.upgrade.backup, self.upgrade.log) = kept;
        self.upgrade.enter(Stage::Checking, "Checking the controller: B05, $$ and both chain lengths.");
        self.queue(["B05 ", "$$", "B10 L", "B10 R"].map(String::from));
        self.controller.queue.push_back(Queued::Mark(Mark::Checked));
        None
    }

    fn checked(&mut self) {
        let upgrade = &mut self.upgrade;
        upgrade.board = self.board.clone();
        upgrade.firmware = Some(self.firmware_version);
        upgrade.z_rpm = upgrade.dump.iter().find_map(|l| l.strip_prefix("$18=")).and_then(|v| v.trim().parse().ok());
        let verdict = (|| {
            check_board(self.board.as_deref())?;
            check_version(self.firmware_version)?;
            if upgrade.chains.len() != 2 {
                return Err("The controller did not report both chain lengths (B10).".to_string());
            }
            match &upgrade.image {
                Some(image) if image.intact() => Ok(()),
                _ => Err("This build of webcontrol-rs carries no firmware image, so it cannot upgrade. Install a \
                          build from CI, which embeds the pinned 51.292 image."
                    .to_string()),
            }
        })();
        match verdict {
            Ok(()) => upgrade.enter(
                Stage::Ready,
                &format!(
                    "Ready to upgrade {} to {TARGET_BUILD}. The flash resets the controller's settings; WebControl \
                     backs them up first and pushes them back afterwards.",
                    self.board.clone().unwrap_or_default()
                ),
            ),
            Err(reason) => upgrade.enter(Stage::Blocked, &reason),
        }
    }

    /// C2-C4: stop, hand the port to avrdude, read the flash back for rollback, save the backup, then flash.
    fn start_upgrade(&mut self, now: Duration, effects: &mut Vec<Effect>) -> Option<String> {
        if self.upgrade.stage != Stage::Ready {
            return Some("Check the controller before upgrading.".into());
        }
        if !matches!(self.link, Link::Connected { .. }) {
            return Some(NOT_CONNECTED.into());
        }
        if self.fixed_port.is_none() && !is_serial_port(&self.upgrade.port) {
            return Some(format!("Cannot upgrade firmware: '{}' is not a serial port.", self.upgrade.port));
        }
        self.upgrade.direction = Direction::Upgrade;
        self.upgrade.attempt = 1;
        self.upgrade.enter(Stage::BackingUp, "Reading the controller's flash back for rollback.");
        self.hand_off(now, effects);
        effects.push(Effect::Firmware(FirmwareJob::ReadFlash { port: self.upgrade.port.clone() }));
        None
    }

    fn hand_off(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        self.realtime("!", now, effects);
        self.drop_user_commands();
        self.controller.queue.clear();
        self.probing = false;
        self.link = Link::HandedOff;
        effects.push(Effect::ClosePort);
        effects.push(broadcast(&self.connection_status()));
    }

    fn reconnect(&mut self, now: Duration) {
        self.link = Link::Disconnected { retry_at: now };
    }

    fn fail(&mut self, reason: &str, now: Duration, effects: &mut Vec<Effect>) {
        self.upgrade.enter(Stage::Failed, reason);
        effects.push(broadcast(&wc_protocol::main::ActivateModal::titled_alert("Firmware upgrade failed", reason)));
        if self.link == Link::HandedOff {
            self.reconnect(now);
        }
    }

    pub(crate) fn firmware_done(&mut self, result: JobResult, now: Duration, effects: &mut Vec<Effect>) {
        match (self.upgrade.stage, result) {
            (Stage::BackingUp, JobResult::Read(read)) => self.read_back(read, now, effects),
            (Stage::BackingUp, JobResult::Saved(Ok(dir))) => {
                self.upgrade.note(&format!("Backup saved in {dir}."));
                self.upgrade.backup = Some(dir);
                self.upgrade.attempt = 1;
                self.flash_target(now, effects);
            }
            (Stage::BackingUp, JobResult::Saved(Err(error))) => {
                self.fail(&format!("Could not save the backup ({error}). Nothing was written."), now, effects)
            }
            (Stage::Flashing | Stage::RollingBack, JobResult::Written { success, output }) => {
                self.written(success, &output, now, effects)
            }
            (stage, result) => tracing::warn!(?stage, ?result, "a firmware job result nothing waits for"),
        }
    }

    fn read_back(&mut self, read: Result<String, String>, now: Duration, effects: &mut Vec<Effect>) {
        let hex = match read {
            Ok(hex) => hex,
            Err(output) if avrdude_outcome(false, &output) == Avrdude::SyncFailure && self.upgrade.attempt == 1 => {
                self.upgrade.attempt = 2;
                self.upgrade.note("avrdude lost sync reading the flash; trying once more.");
                return effects.push(Effect::Firmware(FirmwareJob::ReadFlash { port: self.upgrade.port.clone() }));
            }
            Err(output) => {
                let reason = format!("Could not read the controller's flash ({}). Nothing was written.", tail(&output));
                return self.fail(&reason, now, effects);
            }
        };
        let image = match rollback_image(&hex) {
            Ok(image) => image,
            Err(error) => {
                let reason = format!("The flash read back is not a usable image ({error}). Nothing was written.");
                return self.fail(&reason, now, effects);
            }
        };
        let upgrade = &mut self.upgrade;
        let chains = upgrade.chains.iter().map(|c| format!("{c}")).collect::<Vec<_>>().join("\n");
        let facts = format!(
            "board: {}\nfirmware: {}\nimage: {}\n",
            upgrade.board.clone().unwrap_or_default(),
            upgrade.firmware.unwrap_or_default(),
            upgrade.image.as_ref().map(|i| i.sha256.clone()).unwrap_or_default()
        );
        let files = vec![
            ("flash-read.hex".to_string(), hex),
            ("rollback.hex".to_string(), image.hex.to_string()),
            ("settings-dump.txt".to_string(), upgrade.dump.join("\n") + "\n"),
            ("chain-lengths.txt".to_string(), chains + "\n"),
            ("webcontrol.json".to_string(), self.settings.to_python_json()),
            ("preflight.txt".to_string(), facts),
        ];
        upgrade.rollback = Some(image);
        upgrade.note("Flash read back; saving the backup.");
        effects.push(Effect::Firmware(FirmwareJob::Save { files }));
    }

    fn flash_target(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let Some(image) = self.upgrade.image.clone().filter(Image::intact) else {
            return self.fail("The firmware image failed its SHA-256 check. Nothing was written.", now, effects);
        };
        self.upgrade.enter(Stage::Flashing, &format!("Flashing {TARGET_BUILD} (attempt {}).", self.upgrade.attempt));
        effects.push(Effect::Firmware(FirmwareJob::WriteFlash { port: self.upgrade.port.clone(), image }));
    }

    fn flash_rollback(&mut self, why: &str, effects: &mut Vec<Effect>) {
        let image = self.upgrade.rollback.clone().expect("a rollback image exists before anything is flashed");
        self.upgrade.direction = Direction::Rollback;
        let attempt = self.upgrade.attempt;
        self.upgrade.enter(Stage::RollingBack, &format!("{why} Restoring the saved firmware (attempt {attempt})."));
        effects.push(Effect::Firmware(FirmwareJob::WriteFlash { port: self.upgrade.port.clone(), image }));
    }

    fn written(&mut self, success: bool, output: &str, now: Duration, effects: &mut Vec<Effect>) {
        let outcome = avrdude_outcome(success, output);
        let rolling_back = self.upgrade.stage == Stage::RollingBack;
        match (outcome, self.upgrade.attempt) {
            (Avrdude::Done, _) => {
                let what = if rolling_back { "The saved firmware" } else { TARGET_BUILD };
                self.upgrade
                    .enter(Stage::Restarting, &format!("{what} is written; waiting for the controller to boot."));
                self.upgrade.deadline = Some(now + POST_FLASH_WAIT);
                self.upgrade.banner_seen = false;
                self.reconnect(now);
            }
            (Avrdude::SyncFailure, 1) => {
                self.upgrade.attempt = 2;
                self.upgrade.note("avrdude lost sync with the bootloader; trying once more.");
                match rolling_back {
                    true => self.flash_rollback("", effects),
                    false => self.flash_target(now, effects),
                }
            }
            _ if rolling_back => {
                let backup = self.upgrade.backup.clone().unwrap_or_default();
                let reason = format!(
                    "Could not restore the saved firmware ({}). Flash {backup}/rollback.hex with avrdude by hand; the \
                     bootloader was never written, so it still accepts it.",
                    tail(output)
                );
                self.fail(&reason, now, effects);
            }
            _ => {
                self.upgrade.attempt = 1;
                self.flash_rollback(&format!("The flash failed ({}).", tail(output)), effects);
            }
        }
    }

    /// C8: flash the image read back before the upgrade; settings are pushed again once it boots.
    fn start_rollback(&mut self, now: Duration, effects: &mut Vec<Effect>) -> Option<String> {
        if self.upgrade.rollback.is_none() {
            return Some("There is no saved firmware to roll back to in this session.".into());
        }
        if matches!(self.link, Link::Connected { .. }) {
            self.hand_off(now, effects);
        } else {
            self.link = Link::HandedOff;
        }
        self.upgrade.attempt = 1;
        self.flash_rollback("Rolling back.", effects);
        None
    }

    /// C5: after the flash the banner must match pre-flight and B05 must report the expected build.
    pub(crate) fn upgrade_banner(&mut self, line: &str, now: Duration, effects: &mut Vec<Effect>) {
        if self.upgrade.stage != Stage::Restarting {
            return;
        }
        let Some(banner) = wc_firmware::boot::board_banner(line) else {
            return;
        };
        self.upgrade.banner_seen = true;
        let expected = self.upgrade.board.as_deref().and_then(parse_board);
        if parse_board(&banner) != expected {
            let why = format!(
                "The controller now reports '{banner}', not '{}'.",
                self.upgrade.board.clone().unwrap_or_default()
            );
            self.post_flash_failed(&why, now, effects);
        }
    }

    pub(crate) fn upgrade_version(&mut self, version: f64, now: Duration, effects: &mut Vec<Effect>) {
        if self.upgrade.stage != Stage::Restarting {
            return;
        }
        let expected = match self.upgrade.direction {
            Direction::Upgrade => reports_target(version),
            Direction::Rollback => self.upgrade.firmware.is_some_and(|before| (before - version).abs() < 1e-9),
        };
        if !self.upgrade.banner_seen {
            return self.post_flash_failed("The controller booted without a board banner.", now, effects);
        }
        if !expected {
            return self.post_flash_failed(&format!("The controller reports firmware {version}."), now, effects);
        }
        let message = format!("Firmware {version} booted; pushing every setting back, then verifying with $$.");
        self.upgrade.enter(Stage::Migrating, &message);
        self.upgrade.deadline = None;
        self.upgrade.rewritten.clear();
        let units = if self.units() == wc_protocol::main::Units::Inches { "G20 " } else { "G21 " };
        self.queue([units.to_string(), "G90 ".to_string()]);
        self.controller.queue.push_back(Queued::Mark(Mark::Migrated));
    }

    fn post_flash_failed(&mut self, why: &str, now: Duration, effects: &mut Vec<Effect>) {
        self.upgrade.deadline = None;
        match self.upgrade.direction {
            Direction::Upgrade => {
                self.hand_off(now, effects);
                self.upgrade.attempt = 1;
                self.flash_rollback(why, effects);
            }
            Direction::Rollback => {
                self.fail(&format!("{why} The restored firmware did not come back as before."), now, effects)
            }
        }
    }

    pub(crate) fn upgrade_tick(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        if self.upgrade.deadline.is_some_and(|deadline| now >= deadline) {
            self.post_flash_failed("The controller did not answer B05 after the flash.", now, effects);
        }
    }

    /// A `$$` line during pre-flight or the migration's verify.
    pub(crate) fn upgrade_setting(&mut self, line: &str, rewrite: Option<&str>) {
        match self.upgrade.stage {
            Stage::Checking => self.upgrade.dump.push(line.to_string()),
            Stage::Migrating => self.upgrade.rewritten.extend(rewrite.map(String::from)),
            _ => {}
        }
    }

    pub(crate) fn upgrade_measured(&mut self, mm: f64) {
        match self.upgrade.stage {
            Stage::Checking => self.upgrade.chains.push(mm),
            _ => self.upgrade.chains_after.push(mm),
        }
    }

    /// B04's report while the self-test runs; false for any other line.
    pub(crate) fn upgrade_report(&mut self, line: &str) -> bool {
        self.upgrade.stage == Stage::Testing && read_motor_test(&mut self.upgrade.motors, line)
    }

    /// C7: with the chains slack, B04 turns each motor both ways; then both chains are measured again.
    fn self_test(&mut self) -> Option<String> {
        if !matches!(self.upgrade.stage, Stage::SelfTest | Stage::Done) {
            return Some("The self-test runs once the upgrade has finished.".into());
        }
        if !matches!(self.link, Link::Connected { .. }) {
            return Some(NOT_CONNECTED.into());
        }
        self.upgrade.motors.clear();
        self.upgrade.chains_after.clear();
        self.upgrade.enter(Stage::Testing, "Testing each motor in both directions (B04), then measuring the chains.");
        self.queue(["B04 ", "B10 L", "B10 R"].map(String::from));
        self.controller.queue.push_back(Queued::Mark(Mark::Tested));
        None
    }

    pub(crate) fn mark(&mut self, mark: Mark, effects: &mut Vec<Effect>) {
        match mark {
            Mark::Checked if self.upgrade.stage == Stage::Checking => self.checked(),
            Mark::Migrated if self.upgrade.stage == Stage::Migrating => self.migrated(effects),
            Mark::Tested if self.upgrade.stage == Stage::Testing => self.tested(),
            _ => tracing::debug!(?mark, "a stale upgrade mark"),
        }
    }

    fn migrated(&mut self, effects: &mut Vec<Effect>) {
        let rewritten = match self.upgrade.rewritten.as_slice() {
            [] => "every setting verified.".to_string(),
            lines => format!("re-sent what $$ showed differently: {}.", lines.join(", ")),
        };
        match self.upgrade.direction {
            Direction::Upgrade => {
                let message = format!(
                    "{TARGET_BUILD} is running and its settings are pushed; {rewritten} Next, with the chains slack, \
                     run the motor self-test."
                );
                self.upgrade.enter(Stage::SelfTest, &message);
            }
            Direction::Rollback => {
                let version = self.upgrade.firmware.map(|v| format!("{v}")).unwrap_or_default();
                let message = format!("Rolled back to {version}; {rewritten}");
                self.upgrade.enter(Stage::RolledBack, &message);
                effects.push(alert(&message));
            }
        }
    }

    fn tested(&mut self) {
        let upgrade = &mut self.upgrade;
        let moved = upgrade.chains.iter().zip(&upgrade.chains_after).any(|(a, b)| (a - b).abs() > CHAIN_TOLERANCE_MM);
        let chains = match (upgrade.chains_after.len(), moved) {
            (2, false) => "The chain lengths match pre-flight.",
            (2, true) => {
                "The chain lengths differ from pre-flight: set them again (B08 with the chains at their \
                          extend length, or re-measure with B02) before cutting."
            }
            _ => "The controller did not report both chain lengths.",
        };
        let message = match failed_motors(&upgrade.motors).as_slice() {
            [] => format!("Every motor passed both directions. {chains} B04 turned Z, so zero Z again before cutting."),
            failed => format!(
                "The motor test failed for {}. Check the wiring and run it again, or roll back.",
                failed.join(", ")
            ),
        };
        let stage = if motor_test_passed(&upgrade.motors) { Stage::Done } else { Stage::SelfTest };
        upgrade.enter(stage, &message);
    }
}

/// What still runs while avrdude or the reboot after it owns the controller.
pub(crate) fn allowed_while_flashing(command: &str) -> bool {
    matches!(command, "stopRun" | "stopZ" | "statusRequest" | "createDirectory" | "clearLogs")
}

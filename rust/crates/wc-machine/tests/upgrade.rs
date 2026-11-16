//! The M6 rehearsal: the guided 51.27 -> 51.292 upgrade and its rollback, run against `wc-sim` with a fake avrdude
//! on a fake clock (spec/firmware.md C1-C9): success, a sync failure and its retry, a wrong banner rolling back,
//! and the EEPROM reset answered by the settings migration.

use std::collections::VecDeque;
use std::time::Duration;

use serde_json::{Value, json};
use wc_config::Settings;
use wc_firmware::image::{Image, application_bytes};
use wc_machine::{Effect, Event, FirmwareJob, JobResult, Machine, POST_FLASH_WAIT};
use wc_protocol::system::UpgradeStage as Stage;
use wc_protocol::{Action, Namespace};
use wc_sim::{Build, Sim};
use wc_testkit::contract::Contract;

const OWNERS_BOARD: &str = "PCB v1.1 Detected";
const SYNC_LOST: &str = "avrdude: stk500v2_getsync(): timeout communicating with programmer";
const MARKER: &str = "WCSIM";

fn hex_record(kind: u8, address: u16, data: &[u8]) -> String {
    let header = [data.len() as u8, (address >> 8) as u8, address as u8, kind];
    let sum = header.iter().chain(data).fold(0u8, |s, b| s.wrapping_add(*b)).wrapping_neg();
    format!(":{}{sum:02X}\n", header.iter().chain(data).map(|b| format!("{b:02X}")).collect::<String>())
}

fn hex_of(text: &str) -> String {
    hex_record(0, 0, text.as_bytes()) + &hex_record(1, 0, &[])
}

/// What the fake avrdude reads back: the running build, written into flash as text.
fn read_back_of(build: &Build) -> String {
    let rewrite = u8::from(build.rewrite_notice);
    hex_of(&format!("{MARKER}|{}|{}|{rewrite}|{}", build.version, build.settings_version, build.banner))
}

fn build_in(image: &Image) -> Option<Build> {
    let text = String::from_utf8(application_bytes(&image.hex).ok()?).ok()?;
    let fields: Vec<&str> = text.split('|').collect();
    match fields.as_slice() {
        [MARKER, version, layout, rewrite, banner] => Some(Build {
            version: version.to_string(),
            settings_version: layout.parse().ok()?,
            rewrite_notice: *rewrite == "1",
            banner: banner.to_string(),
        }),
        _ => None,
    }
}

fn target() -> Image {
    Image::new(&hex_of("holey 51.292 stand-in"))
}

/// How the controller comes back from the flash of the target.
#[derive(Debug, Clone, Default, PartialEq)]
enum AfterUpgrade {
    #[default]
    Boots,
    OnBoard(String),
    OldFirmware,
    NoBanner,
    Dead,
}

/// What the simulated controller prints.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Board {
    Alive,
    NoBanner,
    Dead,
}

/// avrdude as the rehearsal scripts it: sync failures, a board that reads differently, a flash that never boots.
#[derive(Default)]
struct Avrdude {
    read_sync_failures: usize,
    read_fails: bool,
    write_sync_failures: usize,
    write_fails: usize,
    after_upgrade: AfterUpgrade,
    save_fails: bool,
    jobs: Vec<String>,
    saved: Vec<String>,
}

impl Avrdude {
    fn run(&mut self, job: FirmwareJob, sim: &mut Sim, board: &mut Board) -> JobResult {
        match job {
            FirmwareJob::ReadFlash { port } => {
                self.jobs.push(format!("read {port}"));
                match (self.read_fails, self.read_sync_failures) {
                    (true, _) => JobResult::Read(Err("avrdude: ser_open(): can't open device".into())),
                    (false, 0) => JobResult::Read(Ok(read_back_of(sim.build()))),
                    (false, _) => {
                        self.read_sync_failures -= 1;
                        JobResult::Read(Err(SYNC_LOST.into()))
                    }
                }
            }
            FirmwareJob::Save { files } => {
                self.jobs.push("save".into());
                self.saved = files.iter().map(|(name, _)| name.clone()).collect();
                match self.save_fails {
                    true => JobResult::Saved(Err("No space left on device".into())),
                    false => JobResult::Saved(Ok("/home/pi/.WebControl/firmware-backups/1".into())),
                }
            }
            FirmwareJob::WriteFlash { port, image } => {
                self.jobs
                    .push(format!("write {port} {}", if build_in(&image).is_some() { "rollback" } else { "target" }));
                if self.write_sync_failures > 0 {
                    self.write_sync_failures -= 1;
                    return JobResult::Written { success: false, output: SYNC_LOST.into() };
                }
                if self.write_fails > 0 {
                    self.write_fails -= 1;
                    return JobResult::Written { success: false, output: "avrdude: verification error".into() };
                }
                let banner = sim.build().banner.clone();
                *board = match (build_in(&image), &self.after_upgrade) {
                    (Some(build), _) => {
                        sim.flash(build);
                        Board::Alive
                    }
                    (None, AfterUpgrade::OldFirmware) => Board::Alive,
                    (None, after) => {
                        let banner = if let AfterUpgrade::OnBoard(other) = after { other.clone() } else { banner };
                        sim.flash(Build::holey_51_292().on_board(&banner));
                        match after {
                            AfterUpgrade::NoBanner => Board::NoBanner,
                            AfterUpgrade::Dead => Board::Dead,
                            _ => Board::Alive,
                        }
                    }
                };
                JobResult::Written { success: true, output: "avrdude: 1234 bytes of flash verified".into() }
            }
        }
    }
}

struct Bench {
    machine: Machine,
    sim: Sim,
    avrdude: Avrdude,
    now: Duration,
    open: bool,
    board: Board,
    /// Drops B10's replies, as a controller that never measured.
    no_measures: bool,
    pending: VecDeque<Event>,
    written: Vec<String>,
    messages: Vec<(String, Value)>,
}

fn settings() -> Settings {
    let mut settings = Settings::defaults();
    settings.set_value("Computed Settings", "units", json!("MM"));
    settings
}

impl Bench {
    fn on(build: Build) -> Bench {
        Bench::with(Machine::new("h", settings()).on_port("simulator").with_firmware(Some(target())), build)
    }

    fn with(machine: Machine, build: Build) -> Bench {
        let sim = Sim::new(build);
        let mut bench = Bench {
            machine,
            sim,
            avrdude: Avrdude::default(),
            now: Duration::ZERO,
            open: false,
            board: Board::Alive,
            no_measures: false,
            pending: VecDeque::new(),
            written: Vec::new(),
            messages: Vec::new(),
        };
        bench.run(8_000);
        bench
    }

    fn owners() -> Bench {
        Bench::on(Build::holey_51_27().on_board(OWNERS_BOARD))
    }

    fn ms(&self) -> u64 {
        self.now.as_millis() as u64
    }

    fn controller_says(&mut self, lines: Vec<String>) {
        if self.open && self.board != Board::Dead {
            let measured = |line: &String| line.starts_with("[Measure");
            let kept = lines.into_iter().filter(|l| !(self.no_measures && measured(l)));
            self.pending.extend(kept.map(Event::ControllerLine));
        }
    }

    fn apply(&mut self, event: Event) {
        let effects = self.machine.handle_inline(event, self.now);
        effects.into_iter().for_each(|effect| self.effect(effect));
    }

    fn effect(&mut self, effect: Effect) {
        match effect {
            Effect::Write(bytes) | Effect::Realtime(bytes) if self.open => {
                self.written.push(String::from_utf8_lossy(&bytes).trim_end().to_string());
                let replies = self.sim.receive(&bytes, self.ms());
                self.controller_says(replies);
            }
            Effect::OpenPort(port) => {
                self.open = true;
                self.pending.push_back(Event::PortOpened { port });
                let boot = self.sim.boot(self.ms());
                if self.board == Board::Alive {
                    self.controller_says(boot);
                }
            }
            Effect::ClosePort => {
                self.open = false;
                self.pending.retain(|e| !matches!(e, Event::ControllerLine(_)));
            }
            Effect::Firmware(job) => {
                let result = self.avrdude.run(job, &mut self.sim, &mut self.board);
                self.pending.push_back(Event::FirmwareDone(result));
            }
            Effect::Push(push) if push.ns == Namespace::MaslowCnc => {
                Contract::shared().validate_frame(&push.frame().encode()).unwrap();
                let data = push.data["data"].as_str().map(|t| serde_json::from_str(t).unwrap());
                let command = push.data["command"].as_str().unwrap().to_string();
                self.messages.push((command, data.unwrap_or(push.data["data"].clone())));
            }
            _ => {}
        }
    }

    fn step(&mut self) {
        self.now += Duration::from_millis(10);
        while let Some(event) = self.pending.pop_front() {
            self.apply(event);
        }
        if self.open {
            let lines = self.sim.tick(self.ms());
            self.controller_says(lines);
        }
        self.apply(Event::Tick);
    }

    fn run(&mut self, ms: u64) {
        (0..ms / 10).for_each(|_| self.step());
    }

    fn act(&mut self, step: &str) {
        self.apply(Event::Action(Action::new("upgradeHoleyFirmware", json!(step), json!(""))));
    }

    fn stage(&self) -> Stage {
        self.machine.snapshot().firmware_upgrade.stage
    }

    /// Runs until the upgrade leaves the stages it passes through on its way; panics after `limit_ms`.
    fn settle(&mut self, passing: &[Stage], limit_ms: u64) -> Stage {
        let deadline = self.now + Duration::from_millis(limit_ms);
        while passing.contains(&self.stage()) {
            assert!(self.now < deadline, "still {:?} after {limit_ms} ms: {:?}", self.stage(), self.view().log);
            self.step();
        }
        self.stage()
    }

    fn upgrade(&mut self) -> Stage {
        self.act("check");
        assert_eq!(
            self.settle(&[Stage::Checking], 10_000),
            Stage::Ready,
            "{:?} {:?}",
            self.view().message,
            self.alerts()
        );
        self.act("upgrade");
        let busy = [Stage::BackingUp, Stage::Flashing, Stage::Restarting, Stage::Migrating, Stage::RollingBack];
        self.settle(&busy, 120_000)
    }

    fn view(&self) -> wc_protocol::system::FirmwareUpgrade {
        self.machine.snapshot().firmware_upgrade
    }

    fn alerts(&self) -> Vec<String> {
        self.messages
            .iter()
            .filter(|(c, d)| c == "activateModal" && d["modalType"] == "alert")
            .map(|(_, d)| d["message"].as_str().unwrap().to_string())
            .collect()
    }

    fn take_written(&mut self) -> Vec<String> {
        std::mem::take(&mut self.written)
    }
}

#[test]
fn preflight_reads_the_board_version_settings_and_chains() {
    let mut bench = Bench::owners();
    bench.take_written();
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Ready);
    assert_eq!(bench.take_written(), ["B05", "$$", "B10 L", "B10 R"]);
    let view = bench.view();
    assert_eq!(view.board.as_deref(), Some(OWNERS_BOARD));
    assert_eq!(view.firmware.as_deref(), Some("51.27"));
    assert_eq!(view.chains.map(|c| (c.left, c.right)), Some((1650.0, 1651.0)));
    assert_eq!(view.image_sha256, Some(target().sha256));
    assert!(view.message.contains("Ready to upgrade PCB v1.1 Detected to 51.292"), "{}", view.message);
    assert!(!view.can_rollback);
    assert_eq!(view.rollback_firmware, None);
}

#[test]
fn the_upgrade_backs_up_flashes_migrates_and_rolls_back() {
    let mut bench = Bench::owners();
    assert_eq!(bench.upgrade(), Stage::SelfTest, "{:?}", bench.view().log);
    assert_eq!(bench.avrdude.jobs, ["read simulator", "save", "write simulator target"]);
    let files = ["flash-read.hex", "rollback.hex", "settings-dump.txt", "chain-lengths.txt", "webcontrol.json"];
    files.iter().for_each(|f| assert!(bench.avrdude.saved.contains(&f.to_string()), "{f}"));
    assert_eq!(bench.sim.build().version, "51.29");
    assert_eq!(bench.sim.setting(18), Some(f64::from(12.6_f32)), "never 51.292's factory 126");
    assert_eq!((bench.sim.setting(63), bench.sim.setting(64)), (Some(9999.0), Some(-9999.0)));
    let view = bench.view();
    assert_eq!(view.backup.as_deref(), Some("/home/pi/.WebControl/firmware-backups/1"));
    assert!(view.can_rollback);
    assert!(view.message.contains("every setting verified"), "{}", view.message);
    assert!(bench.machine.snapshot().firmware_build.as_deref() == Some("51.292"));

    bench.act("selfTest");
    assert_eq!(bench.settle(&[Stage::Testing], 20_000), Stage::Done, "{}", bench.view().message);
    let view = bench.view();
    assert_eq!(view.motors.len(), 3);
    assert!(view.motors.iter().all(|m| m.passed == [true, true]));
    assert_eq!(view.chains_after, view.chains);
    assert!(view.message.contains("match pre-flight") && view.message.contains("zero Z"), "{}", view.message);

    bench.take_written();
    bench.act("rollback");
    assert_eq!(bench.settle(&[Stage::RollingBack, Stage::Restarting, Stage::Migrating], 120_000), Stage::RolledBack);
    assert_eq!(bench.avrdude.jobs.last().map(String::as_str), Some("write simulator rollback"));
    assert_eq!(bench.sim.build().version, "51.27");
    assert_eq!(bench.sim.setting(18), Some(f64::from(12.6_f32)));
    let written = bench.take_written();
    assert_eq!(written.first().map(String::as_str), Some("!"), "Stop before the port is handed over");
    assert!(!written.iter().any(|l| l.starts_with("$63=") || l.starts_with("$64=")), "51.27 has no Z limits");
    assert!(bench.alerts().iter().any(|a| a.starts_with("Rolled back to 51.27")));
    assert_eq!(bench.machine.snapshot().controller_firmware_version, 51.27);
}

#[test]
fn an_eeprom_reset_after_the_flash_is_answered_by_the_settings_migration() {
    let mut bench = Bench::owners();
    bench.act("check");
    bench.settle(&[Stage::Checking], 10_000);
    let adopted = bench.machine.snapshot().settings.text_or_empty("Advanced Settings", "maxZRPM");
    assert_eq!(adopted, "12.6", "pre-flight's $$ adopted 51.27's Z speed");
    bench.act("upgrade");
    bench.settle(&[Stage::BackingUp, Stage::Flashing], 10_000);
    bench.take_written();
    bench.settle(&[Stage::Restarting, Stage::Migrating], 60_000);
    let written = bench.take_written();
    let push = written.iter().position(|l| l.starts_with("$0=")).expect("the full push");
    assert_eq!(written[..push], ["B05", "G21", "G90", "$$"]);
    let restarts = written.iter().filter(|l| l.starts_with("$38=")).count();
    assert_eq!(restarts, 1, "$38 goes once, then the restart; after it $$ shows it held: {written:?}");
    assert!(written.contains(&"$18=12.6".to_string()), "{written:?}");
    assert!(written.contains(&"$17=0".to_string()), "an explicit $17");
    let verify = written.iter().rposition(|l| l == "$$").unwrap();
    assert_eq!(written[verify - 2..verify], ["$63=9999".to_string(), "$64=-9999".to_string()]);
    assert_eq!(written[verify + 1..], ["G21", "G90"], "units and G90 again after the verify");
    assert!(!bench.alerts().iter().any(|a| a.contains("EEPROM read fail")), "the wizard explains the reset");
    assert_eq!(bench.sim.setting(18), Some(f64::from(12.6_f32)));
}

/// The migration rewrites every PID weight; text that reads above 1 would leave the motors without gains next boot.
#[test]
fn the_migrated_controller_still_has_every_pid_after_the_next_boot() {
    let mut bench = Bench::owners();
    bench.upgrade();
    let written = bench.take_written();
    ["$24=1", "$28=1", "$32=1", "$36=1"].iter().for_each(|l| assert!(written.contains(&l.to_string()), "{l}"));
    [24, 28, 32, 36].iter().for_each(|&key| assert_eq!(bench.sim.setting(key), Some(1.0), "${key}"));
    let boot = bench.sim.boot(bench.ms());
    assert!(bench.sim.main_axes_tuned() && bench.sim.z_axis_tuned());
    bench.controller_says(boot);
    bench.run(5_000);
    assert!(bench.sim.main_axes_tuned() && bench.sim.z_axis_tuned());
}

#[test]
fn a_sync_failure_is_retried_once() {
    let mut bench = Bench::owners();
    bench.avrdude.write_sync_failures = 1;
    bench.avrdude.read_sync_failures = 1;
    assert_eq!(bench.upgrade(), Stage::SelfTest);
    let jobs = ["read simulator", "read simulator", "save", "write simulator target", "write simulator target"];
    assert_eq!(bench.avrdude.jobs, jobs);
    assert!(bench.view().log.iter().any(|l| l.contains("lost sync with the bootloader; trying once more")));
    assert_eq!(bench.sim.build().version, "51.29");
}

#[test]
fn a_flash_that_keeps_failing_restores_the_saved_firmware() {
    let mut bench = Bench::owners();
    bench.avrdude.write_sync_failures = 2;
    assert_eq!(bench.upgrade(), Stage::RolledBack, "{:?}", bench.view().log);
    assert_eq!(
        bench.avrdude.jobs[2..],
        ["write simulator target", "write simulator target", "write simulator rollback"]
    );
    assert_eq!(bench.sim.build().version, "51.27");
}

#[test]
fn a_board_that_reads_differently_after_the_flash_is_rolled_back_before_any_setting_is_sent() {
    let mut bench = Bench::owners();
    bench.avrdude.after_upgrade = AfterUpgrade::OnBoard("PCB v1.5 TB6643  Detected".into());
    bench.act("check");
    bench.settle(&[Stage::Checking], 10_000);
    bench.act("upgrade");
    bench.settle(&[Stage::BackingUp, Stage::Flashing], 10_000);
    bench.take_written();
    assert_eq!(bench.settle(&[Stage::Restarting, Stage::RollingBack, Stage::Migrating], 120_000), Stage::RolledBack);
    let written = bench.take_written();
    let rollback = written.iter().position(|l| l == "!").expect("Stop before the rollback");
    assert!(written[..rollback].iter().all(|l| !l.starts_with('$')), "{written:?}");
    assert_eq!(bench.sim.build().banner, OWNERS_BOARD);
    assert_eq!(bench.sim.build().version, "51.27");
    assert!(bench.view().log.iter().any(|l| l.contains("now reports 'PCB v1.5 TB6643  Detected'")));
}

#[test]
fn a_flash_that_boots_the_old_version_or_nothing_is_rolled_back() {
    let mut bench = Bench::owners();
    bench.avrdude.after_upgrade = AfterUpgrade::OldFirmware;
    assert_eq!(bench.upgrade(), Stage::RolledBack);
    assert!(bench.view().log.iter().any(|l| l.contains("reports firmware 51.27")));

    let mut bench = Bench::owners();
    bench.avrdude.after_upgrade = AfterUpgrade::NoBanner;
    assert_eq!(bench.upgrade(), Stage::RolledBack);
    assert!(bench.view().log.iter().any(|l| l.contains("without a board banner")));

    let mut bench = Bench::owners();
    bench.avrdude.after_upgrade = AfterUpgrade::Dead;
    bench.act("check");
    bench.settle(&[Stage::Checking], 10_000);
    bench.act("upgrade");
    bench.settle(&[Stage::BackingUp, Stage::Flashing], 10_000);
    bench.run(POST_FLASH_WAIT.as_millis() as u64 - 1_000);
    assert_eq!(bench.stage(), Stage::Restarting);
    let stage = bench.settle(&[Stage::Restarting, Stage::RollingBack, Stage::Migrating], 120_000);
    assert_eq!(stage, Stage::RolledBack, "{:?}", bench.view().log);
    assert!(bench.view().log.iter().any(|l| l.contains("did not answer B05")));
}

#[test]
fn a_rollback_that_fails_says_how_to_restore_by_hand() {
    let mut bench = Bench::owners();
    bench.avrdude.write_fails = 2;
    assert_eq!(bench.upgrade(), Stage::Failed);
    let message = bench.view().message;
    assert!(message.contains("/home/pi/.WebControl/firmware-backups/1/rollback.hex"), "{message}");
    assert!(bench.alerts().iter().any(|a| a.contains("rollback.hex")));
}

#[test]
fn nothing_is_written_when_the_backup_cannot_be_made() {
    for (read_fails, save_fails, reason) in [(true, false, "can't open device"), (false, true, "No space left")] {
        let mut bench = Bench::owners();
        bench.avrdude.read_fails = read_fails;
        bench.avrdude.save_fails = save_fails;
        assert_eq!(bench.upgrade(), Stage::Failed);
        bench.run(500);
        assert!(bench.view().message.contains(reason) && bench.view().message.contains("Nothing was written"));
        assert!(!bench.avrdude.jobs.iter().any(|j| j.starts_with("write")));
        assert_eq!(bench.machine.snapshot().port, "simulator", "the link comes back");
        assert!(!bench.view().can_rollback || save_fails);
    }
}

#[test]
fn a_damaged_read_back_stops_the_upgrade() {
    let mut bench = Bench::owners();
    bench.act("check");
    bench.settle(&[Stage::Checking], 10_000);
    bench.act("upgrade");
    bench.pending.clear();
    bench.apply(Event::FirmwareDone(JobResult::Read(Ok("not hex".into()))));
    assert_eq!(bench.stage(), Stage::Failed);
    assert!(bench.view().message.contains("not a usable image"));
}

#[test]
fn hazardous_boards_and_imageless_builds_are_blocked() {
    let mut bench = Bench::on(Build::holey_51_27());
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Blocked);
    assert!(bench.view().message.contains("wrong pins"));
    bench.act("upgrade");
    assert!(bench.alerts().contains(&"Check the controller before upgrading.".to_string()));

    let machine = Machine::new("h", settings()).on_port("simulator").with_firmware(None);
    let mut bench = Bench::with(machine, Build::holey_51_27().on_board(OWNERS_BOARD));
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Blocked);
    assert!(bench.view().message.contains("carries no firmware image"));

    let mut bench = Bench::on(Build::holey_51_292().on_board(OWNERS_BOARD));
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Blocked);
    assert!(bench.view().message.contains("already runs 51.29x"));
}

#[test]
fn a_check_without_chain_lengths_is_blocked() {
    let mut bench = Bench::owners();
    bench.no_measures = true;
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Blocked);
    assert!(bench.view().message.contains("both chain lengths"));
}

#[test]
fn everything_but_stop_waits_while_avrdude_has_the_port() {
    let mut bench = Bench::owners();
    bench.act("check");
    bench.settle(&[Stage::Checking], 10_000);
    bench.act("upgrade");
    assert_eq!(bench.stage(), Stage::BackingUp);
    bench.apply(Event::Action(Action::new("home", json!(""), json!(""))));
    assert_eq!(bench.alerts().last().map(String::as_str), Some("Wait for the firmware upgrade to finish."));
    let before = bench.alerts().len();
    bench.apply(Event::Stop(Action::new("stopRun", json!(""), json!(""))));
    assert_eq!(bench.alerts().len(), before, "Stop is never refused");
    bench.act("check");
    assert_eq!(bench.alerts().last().map(String::as_str), Some("Wait for the firmware upgrade to finish."));
    assert_eq!(bench.machine.snapshot().port, "none", "the link is handed to avrdude");
    bench.pending.clear();
    bench.run(2_000);
    assert_eq!(bench.machine.snapshot().port, "none", "nothing reopens it during the flash");
}

#[test]
fn a_failed_motor_test_or_moved_chains_are_reported() {
    let mut bench = Bench::owners();
    bench.upgrade();
    bench.sim.stuck_motor = Some('Z');
    bench.sim.chains = (1660.0, 1651.0);
    bench.act("selfTest");
    assert_eq!(bench.settle(&[Stage::Testing], 20_000), Stage::SelfTest);
    assert!(bench.view().message.contains("failed for Z"), "{}", bench.view().message);
    bench.sim.stuck_motor = None;
    bench.act("selfTest");
    assert_eq!(bench.settle(&[Stage::Testing], 20_000), Stage::Done);
    assert!(bench.view().message.contains("differ from pre-flight"));
}

#[test]
fn steps_out_of_order_and_the_other_firmwares_alert() {
    let mut bench = Bench::owners();
    bench.act("selfTest");
    bench.act("rollback");
    bench.act("");
    bench.apply(Event::Action(Action::new("upgradeStockFirmware", json!(""), json!(""))));
    bench.apply(Event::Action(Action::new("upgradeCustomFirmware", json!(""), json!(""))));
    let alerts = bench.alerts();
    assert_eq!(alerts[0], "The self-test runs once the upgrade has finished.");
    assert_eq!(alerts[1], "There is no saved firmware to roll back to in this session.");
    assert!(alerts[2].contains("guided upgrade"));
    assert!(alerts[3].contains("load the stock firmware") && alerts[4].contains("load the custom firmware"));
    assert!(!alerts[3].contains("Python"), "the Python backend is gone: {}", alerts[3]);
    bench.act("reset");
    assert_eq!(bench.stage(), Stage::Idle);
}

#[test]
fn the_upgrade_needs_the_controller_and_a_serial_port() {
    let mut settings = settings();
    settings.set_value("Maslow Settings", "COMport", json!("notaport"));
    let mut machine = Machine::new("h", settings).with_firmware(Some(target()));
    let action = |step: &str| Event::Action(Action::new("upgradeHoleyFirmware", json!(step), json!("")));
    let alerts = |effects: Vec<Effect>| -> Vec<String> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Push(p) => p.data["data"].as_str().map(String::from),
                _ => None,
            })
            .collect()
    };
    assert!(alerts(machine.handle_inline(action("check"), Duration::ZERO))[0].contains("not connected"));
    let mut bench = Bench::with(machine, Build::holey_51_27().on_board(OWNERS_BOARD));
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Ready);
    bench.act("upgrade");
    assert!(bench.alerts().last().unwrap().contains("'notaport' is not a serial port"));
    assert!(bench.avrdude.jobs.is_empty());
}

#[test]
fn a_late_job_result_is_ignored() {
    let mut bench = Bench::owners();
    bench.apply(Event::FirmwareDone(JobResult::Written { success: true, output: String::new() }));
    assert_eq!(bench.stage(), Stage::Idle);
}

#[test]
fn a_link_lost_while_the_settings_go_back_restarts_the_post_flash_checks() {
    let mut bench = Bench::owners();
    bench.act("check");
    bench.settle(&[Stage::Checking], 10_000);
    bench.act("upgrade");
    bench.settle(&[Stage::BackingUp, Stage::Flashing, Stage::Restarting], 60_000);
    assert_eq!(bench.stage(), Stage::Migrating);
    bench.pending.clear();
    bench.apply(Event::PortClosed);
    let stage = bench.settle(&[Stage::Migrating, Stage::Restarting], 60_000);
    assert_eq!(stage, Stage::SelfTest, "{:?}", bench.view().log);
    assert_eq!(bench.sim.setting(18), Some(f64::from(12.6_f32)));
}

#[test]
fn a_controller_restart_during_the_check_or_self_test_frees_the_wizard() {
    let mut bench = Bench::owners();
    bench.act("check");
    bench.pending.clear();
    bench.apply(Event::PortClosed);
    assert_eq!(bench.settle(&[Stage::Checking], 20_000), Stage::Blocked, "{:?}", bench.view().log);
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Ready);

    let mut bench = Bench::owners();
    bench.upgrade();
    bench.act("selfTest");
    bench.pending.clear();
    bench.apply(Event::PortClosed);
    assert_eq!(bench.settle(&[Stage::Testing], 20_000), Stage::SelfTest, "{:?}", bench.view().log);
}

#[test]
fn a_check_after_the_upgrade_still_rolls_back_to_the_firmware_read_off_the_board() {
    let mut bench = Bench::owners();
    assert_eq!(bench.upgrade(), Stage::SelfTest);
    bench.act("check");
    assert_eq!(bench.settle(&[Stage::Checking], 10_000), Stage::Blocked);
    assert!(bench.view().can_rollback);
    assert_eq!(bench.view().firmware.as_deref(), Some("51.29"));
    assert_eq!(bench.view().rollback_firmware.as_deref(), Some("51.27"));
    bench.act("rollback");
    let stage = bench.settle(&[Stage::RollingBack, Stage::Restarting, Stage::Migrating], 120_000);
    assert_eq!(stage, Stage::RolledBack, "{:?}", bench.view().log);
    assert!(bench.alerts().iter().any(|a| a.starts_with("Rolled back to 51.27")));
}

/// The port opens off the machine task, so a rollback can start while an open is still on its way.
#[test]
fn a_port_that_opens_after_a_rollback_took_the_link_is_closed_again() {
    let mut bench = Bench::owners();
    assert_eq!(bench.upgrade(), Stage::SelfTest);
    bench.pending.clear();
    bench.apply(Event::PortClosed);
    while !bench.pending.iter().any(|e| matches!(e, Event::PortOpened { .. })) {
        bench.step();
    }
    let opened = bench.pending.pop_front().unwrap();
    bench.pending.clear();
    bench.act("rollback");
    bench.apply(opened);
    assert!(!bench.open, "the late port is closed, since avrdude has it");
    assert_eq!(bench.machine.snapshot().link, wc_protocol::main::LinkStatus::Disconnected);
    bench.apply(Event::PortFailed { port: "simulator".into(), error: "late".into() });
    let stage = bench.settle(&[Stage::RollingBack, Stage::Restarting, Stage::Migrating], 120_000);
    assert_eq!(stage, Stage::RolledBack, "{:?}", bench.view().log);
}

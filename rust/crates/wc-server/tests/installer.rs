//! Updating from the page: the start through sudo and systemd, the refusals, and the progress read back from the
//! status file `webcontrol-update --service` writes.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use wc_config::{Paths, Settings};
use wc_machine::{Event, Machine, NoHost, NoPort, Shared, spawn};
use wc_protocol::Action;
use wc_protocol::main::PauseButton;
use wc_protocol::system::{LatestRelease, UpdateAction, UpdatePhase, UpdateProgress, UpdateResult, UpgradeStage};
use wc_server::installer::{
    IS_ACTIVE, Installer, Off, Pending, START, START_GRACE, Systemd, busy_reason, progress, read_status, unit,
};
use wc_server::updates::{ReleaseSource, Timing, Updates};
use wc_server::{AppState, PingConfig, router};
use wc_testkit::contract::{Contract, ResponseBody};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn status(result: Option<UpdateResult>) -> UpdateProgress {
    UpdateProgress {
        action: UpdateAction::Update,
        phase: if result.is_some() { UpdatePhase::Finished } else { UpdatePhase::Downloading },
        running: result.is_none(),
        from: Some("0.3.0".into()),
        to: Some("0.4.0".into()),
        started_at: Some(1_800_000_000_000),
        finished_at: result.map(|_| 1_800_000_060_000),
        result,
        log: vec!["Downloading SHA256SUMS".into()],
    }
}

#[test]
fn the_status_file_reads_as_written_and_anything_else_as_none() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("update.json");
    assert_eq!(read_status(&file), None);
    std::fs::write(&file, "{\"action\":\"update\",\"phase\":\"downloa").unwrap();
    assert_eq!(read_status(&file), None, "half a file");
    std::fs::write(&file, r#"{"action":"update","phase":"warping","log":[]}"#).unwrap();
    assert_eq!(read_status(&file), None, "an unknown phase");
    let written = r#"{"action":"update","phase":"downloading","from":"0.3.0","to":"0.4.0","startedAt":1800000000000,
        "finishedAt":null,"result":null,"log":["Downloading SHA256SUMS"]}"#;
    std::fs::write(&file, written).unwrap();
    assert_eq!(read_status(&file), Some(status(None)));
    let lines: Vec<String> = (0..250).map(|n| format!("line {n}")).collect();
    let long = json!({"action": "rollback", "phase": "finished", "result": "rolledBack", "log": lines});
    std::fs::write(&file, long.to_string()).unwrap();
    let read = read_status(&file).unwrap();
    assert_eq!((read.log.len(), read.log[0].as_str(), read.running), (200, "line 50", false));
    assert_eq!((read.from, read.started_at), (None, None));
}

fn pending(age: Duration) -> Pending {
    let at = Instant::now().checked_sub(age).unwrap();
    Pending { action: UpdateAction::Rollback, at, at_ms: now_ms() - age.as_millis() as u64 }
}

#[test]
fn an_unanswered_start_shows_as_starting_until_its_grace_runs_out() {
    let young = progress(Some(status(Some(UpdateResult::Updated))), Some(pending(Duration::ZERO)), false).unwrap();
    assert_eq!(
        (young.action, young.phase, young.running, young.result),
        (UpdateAction::Rollback, UpdatePhase::Starting, true, None)
    );
    let queued = progress(None, Some(pending(START_GRACE * 2)), true).unwrap();
    assert_eq!(queued.phase, UpdatePhase::Starting, "the unit runs but has not written yet");
    let never = progress(None, Some(pending(START_GRACE * 2)), false).unwrap();
    assert_eq!((never.phase, never.running, never.result), (UpdatePhase::Finished, false, Some(UpdateResult::Failed)));
    assert_eq!(never.log, ["The updater did not start; journalctl -u webcontrol-update@rollback.service shows why."]);
}

#[test]
fn a_start_is_answered_by_a_status_from_its_second_or_later() {
    let start = Pending { action: UpdateAction::Update, at: Instant::now(), at_ms: 1_800_000_000_700 };
    let at = |ms| UpdateProgress { started_at: Some(ms), ..status(None) };
    assert!(start.answered_by(&at(1_800_000_000_000)));
    assert!(start.answered_by(&at(1_800_000_005_000)));
    assert!(!start.answered_by(&at(1_799_999_999_000)));
    assert!(!start.answered_by(&UpdateProgress { started_at: None, ..status(None) }));
}

#[test]
fn an_unfinished_status_whose_unit_stopped_is_interrupted() {
    assert_eq!(progress(None, None, false), None);
    assert_eq!(progress(Some(status(None)), None, true), Some(status(None)));
    let done = status(Some(UpdateResult::RolledBack));
    assert_eq!(progress(Some(done.clone()), None, false), Some(done));
    let cut = progress(Some(status(None)), None, false).unwrap();
    assert_eq!((cut.phase, cut.running, cut.result), (UpdatePhase::Finished, false, Some(UpdateResult::Interrupted)));
    assert_eq!(cut.log.len(), 2);
    assert!(cut.log[1].starts_with("The updater stopped before it finished"));
}

fn idle() -> wc_machine::Snapshot {
    Machine::new("h", Settings::defaults()).snapshot()
}

#[test]
fn the_machine_is_busy_during_a_job_a_hold_a_tool_change_a_probe_an_upgrade_or_a_scan() {
    assert_eq!(busy_reason(&idle()), None);
    let says = |snapshot: wc_machine::Snapshot, reason: &str| {
        assert_eq!(busy_reason(&snapshot), Some(format!("Not now: {reason}. Finish or stop it first.")));
    };
    says(wc_machine::Snapshot { job_in_progress: true, ..idle() }, "a job is running or paused");
    says(
        wc_machine::Snapshot { pause_button: PauseButton::Resume, ..idle() },
        "a job or tool change is held, waiting for Resume",
    );
    says(
        wc_machine::Snapshot { manual_z_axis_adjust: true, ..idle() },
        "the controller is waiting for a tool change or a Z adjustment",
    );
    says(wc_machine::Snapshot { probing: true, ..idle() }, "a Z probe is in flight");
    let mut upgrading = idle();
    upgrading.firmware_upgrade.stage = UpgradeStage::Flashing;
    says(upgrading, "a controller firmware upgrade is underway");
    let mut finished = idle();
    finished.firmware_upgrade.stage = UpgradeStage::Done;
    assert_eq!(busy_reason(&finished), None);
    let scanning = wc_machine::optical::View { scan: Some(3), ..Default::default() };
    says(
        wc_machine::Snapshot { optical: Shared(Arc::new(scanning)), ..idle() },
        "an optical calibration scan is running",
    );
}

fn script(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::process::Command::new("chmod").arg("+x").arg(&path).status().unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn systemd_starts_a_unit_through_sudo_and_reads_activating_as_running() {
    let dir = tempfile::tempdir().unwrap();
    let calls = dir.path().join("calls");
    let log = format!("echo \"$*\" >> {}", calls.display());
    let systemd = |start: &str, states: &str| Systemd {
        unit_file: dir.path().join("webcontrol-update@.service"),
        start: vec![script(dir.path(), "sudo", &format!("{log}\n{start}"))],
        is_active: vec![script(dir.path(), "systemctl", &format!("{log}\nprintf '{states}'\nexit 3"))],
    };
    let ok = systemd("exit 0", "inactive\\nactivating\\n");
    assert!(!ok.installed());
    std::fs::write(&ok.unit_file, "").unwrap();
    assert!(ok.installed());
    assert!(ok.running());
    assert_eq!(ok.start(UpdateAction::Rollback), Ok(()));
    let called = std::fs::read_to_string(&calls).unwrap();
    assert_eq!(
        called,
        "webcontrol-update@update.service webcontrol-update@rollback.service\nwebcontrol-update@rollback.service\n"
    );
    let refused = systemd("echo 'a password is required' >&2; exit 1", "inactive\\nfailed\\n");
    assert!(!refused.running());
    assert_eq!(
        refused.start(UpdateAction::Update),
        Err("Couldn't start webcontrol-update@update.service: a password is required".into())
    );
    let missing = Systemd { start: vec!["/nonexistent/sudo".into()], is_active: vec!["/nonexistent".into()], ..ok };
    assert!(missing.start(UpdateAction::Update).unwrap_err().starts_with("Couldn't run sudo: "));
    assert!(!missing.running());
    assert_eq!(Off.start(UpdateAction::Update), Err("Updating is off in this server".into()));
    assert!(!Off.installed() && !Off.running());
}

#[test]
fn the_sudoers_rule_and_the_unit_allow_exactly_what_the_server_runs() {
    let deploy = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy");
    let rule = std::fs::read_to_string(deploy.join("webcontrol-rs.sudoers")).unwrap();
    let rules: Vec<&str> = rule.lines().filter(|line| !line.starts_with('#')).collect();
    let start = |action| format!("pi ALL=(root) NOPASSWD: {} {}", START[2..].join(" "), unit(action));
    assert_eq!(
        rules,
        [
            "pi ALL=(root) NOPASSWD: /usr/bin/systemctl poweroff".to_string(),
            start(UpdateAction::Update),
            start(UpdateAction::Rollback)
        ]
    );
    assert_eq!(START[..2], ["sudo", "--non-interactive"]);
    assert_eq!(IS_ACTIVE, ["systemctl", "is-active"]);
    let unit_file = std::fs::read_to_string(deploy.join("webcontrol-update@.service")).unwrap();
    assert!(unit_file.lines().any(|line| line == "ExecStart=/usr/local/bin/webcontrol-update --service %i"));
    let default = Systemd::default();
    assert_eq!(default.unit_file, Path::new("/etc/systemd/system/webcontrol-update@.service"));
    assert_eq!(
        (default.start, default.is_active),
        (START.map(String::from).to_vec(), IS_ACTIVE.map(String::from).to_vec())
    );
}

/// Records the starts, and runs while `running` holds.
#[derive(Default)]
struct Fake {
    missing: bool,
    broken: bool,
    running: Arc<AtomicBool>,
    starts: Arc<Mutex<Vec<UpdateAction>>>,
}

impl Installer for Fake {
    fn installed(&self) -> bool {
        !self.missing
    }

    fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn start(&self, action: UpdateAction) -> Result<(), String> {
        if self.broken {
            return Err("Couldn't start it: sudo: a password is required".into());
        }
        self.starts.lock().unwrap().push(action);
        Ok(())
    }
}

struct Latest(&'static str);

impl ReleaseSource for Latest {
    fn latest(&self) -> Result<LatestRelease, String> {
        let url = "https://github.com/o/r/releases/tag/x".into();
        Ok(LatestRelease {
            tag: self.0.into(),
            name: self.0.into(),
            published_at: None,
            url,
            notes: String::new(),
            commit: None,
        })
    }
}

struct Page {
    app: Router,
    dir: tempfile::TempDir,
    running: Arc<AtomicBool>,
    starts: Arc<Mutex<Vec<UpdateAction>>>,
}

impl Page {
    fn new(latest: &'static str, installer: Fake, machine: Machine) -> Page {
        let dir = tempfile::tempdir().unwrap();
        let (running, starts) = (installer.running.clone(), installer.starts.clone());
        let timing = Timing { fresh: Duration::from_secs(600), ..Timing::default() };
        let updates = Updates::new(Latest(latest), "o/r", dir.path(), timing)
            .with_installer(installer, &dir.path().join("update.json"));
        let (machine, _task) = spawn(machine, Box::new(NoPort), Arc::new(NoHost));
        let paths = Paths::new(Path::new("/nonexistent-home"));
        let app = router(AppState::new(machine, PingConfig::default(), paths).with_updates(updates));
        Page { app, dir, running, starts }
    }

    fn idle(latest: &'static str) -> Page {
        Page::new(latest, Fake::default(), Machine::new("h", Settings::defaults()))
    }

    async fn call(&self, method: &str, path: &str) -> (StatusCode, Value) {
        let body = if method == "POST" {
            Contract::shared().validate_request(method, path, &json!({})).unwrap();
            Body::from("{}")
        } else {
            Body::empty()
        };
        let request = Request::builder().method(method).uri(path).header("content-type", "application/json");
        let response = tower::ServiceExt::oneshot(self.app.clone(), request.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let json: Value = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
        Contract::shared().validate_response(method, path, status.as_u16(), &ResponseBody::Json(json.clone())).unwrap();
        (status, json)
    }

    async fn post(&self, action: &str) -> (StatusCode, Value) {
        self.call("POST", &format!("/api/system/updates/{action}")).await
    }

    async fn updates(&self) -> Value {
        self.call("GET", "/api/system/updates").await.1
    }

    fn write_status(&self, status: Value) {
        std::fs::write(self.dir.path().join("update.json"), status.to_string()).unwrap();
    }

    fn back_up(&self) {
        std::fs::write(self.dir.path().join("MANIFEST"), "version=0.2.0\ndate=2026-11-16\n").unwrap();
    }

    fn starts(&self) -> Vec<UpdateAction> {
        self.starts.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn an_update_starts_its_unit_shows_starting_then_the_updaters_progress_and_result() {
    let page = Page::idle("v999.0.0");
    let before = page.updates().await;
    assert_eq!(
        (&before["canUpdate"], &before["busy"], &before["progress"]),
        (&json!(true), &Value::Null, &Value::Null)
    );

    let (status, body) = page.post("install").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(page.starts(), [UpdateAction::Update]);
    assert_eq!((&body["progress"]["phase"], &body["progress"]["running"]), (&json!("starting"), &json!(true)));
    let (again, refused) = page.post("install").await;
    assert_eq!((again, refused), (StatusCode::CONFLICT, json!({"error": "An update or rollback is already running."})));

    page.running.store(true, Ordering::SeqCst);
    let started = now_ms() / 1000 * 1000;
    page.write_status(json!({"action": "update", "phase": "downloading", "from": "0.3.0", "to": "999.0.0",
        "startedAt": started, "finishedAt": null, "result": null, "log": ["Downloading SHA256SUMS"]}));
    let running = page.updates().await["progress"].clone();
    assert_eq!(
        (&running["phase"], &running["running"], &running["to"]),
        (&json!("downloading"), &json!(true), &json!("999.0.0"))
    );
    assert_eq!(page.post("rollback").await.0, StatusCode::CONFLICT, "the unit still runs");

    page.running.store(false, Ordering::SeqCst);
    page.write_status(json!({"action": "update", "phase": "finished", "from": "0.3.0", "to": "999.0.0",
        "startedAt": started, "finishedAt": started + 60_000, "result": "updated", "log": ["Updated webcontrol-rs to 999.0.0."]}));
    let done = page.updates().await["progress"].clone();
    assert_eq!(
        (&done["result"], &done["running"], &done["log"][0]),
        (&json!("updated"), &json!(false), &json!("Updated webcontrol-rs to 999.0.0."))
    );
}

#[tokio::test]
async fn an_update_needs_a_newer_release_and_a_rollback_a_backup() {
    let page = Page::idle("v0.0.1");
    let refused = page.post("install").await;
    assert_eq!(refused, (StatusCode::CONFLICT, json!({"error": "There is no newer published release to update to."})));
    let refused = page.post("rollback").await;
    assert_eq!(refused, (StatusCode::CONFLICT, json!({"error": "There is no backup to roll back to."})));
    assert!(page.starts().is_empty());
    page.back_up();
    let (status, body) = page.post("rollback").await;
    assert_eq!((status, &body["progress"]["action"]), (StatusCode::ACCEPTED, &json!("rollback")));
    assert_eq!(page.starts(), [UpdateAction::Rollback]);
}

fn streaming() -> Machine {
    let mut machine = Machine::new("h", Settings::defaults());
    machine.handle_inline(Event::PortOpened { port: "/dev/ttyACM0".into() }, Duration::ZERO);
    machine.load_gcode("G0 X1\nG1 Z-1\n");
    machine.handle_inline(Event::Action(Action::new("startRun", json!(""), json!(""))), Duration::ZERO);
    machine
}

#[tokio::test]
async fn a_busy_machine_missing_units_or_a_failed_start_refuse() {
    let busy = Page::new("v999.0.0", Fake::default(), streaming());
    busy.back_up();
    let reason = "Not now: a job is running or paused. Finish or stop it first.";
    assert_eq!(busy.updates().await["busy"], reason);
    for action in ["install", "rollback"] {
        assert_eq!(busy.post(action).await, (StatusCode::CONFLICT, json!({"error": reason})));
    }
    assert!(busy.starts().is_empty());

    let missing =
        Page::new("v999.0.0", Fake { missing: true, ..Fake::default() }, Machine::new("h", Settings::defaults()));
    assert_eq!(missing.updates().await["canUpdate"], false);
    let (status, body) = missing.post("install").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("run webcontrol-update on the Pi once"));

    let broken =
        Page::new("v999.0.0", Fake { broken: true, ..Fake::default() }, Machine::new("h", Settings::defaults()));
    let failed = broken.post("install").await;
    assert_eq!(
        failed,
        (StatusCode::INTERNAL_SERVER_ERROR, json!({"error": "Couldn't start it: sudo: a password is required"}))
    );
    assert_eq!(broken.updates().await["progress"], Value::Null);
}

#[tokio::test]
async fn an_unfinished_status_with_no_unit_running_reads_as_interrupted() {
    let page = Page::idle("v999.0.0");
    page.write_status(json!({"action": "rollback", "phase": "restarting", "from": "1.0", "to": "0.9",
        "startedAt": 1_800_000_000_000_u64, "finishedAt": null, "result": null, "log": ["Restarting webcontrol-rs."]}));
    let progress = page.updates().await["progress"].clone();
    assert_eq!(
        (&progress["result"], &progress["phase"], &progress["running"]),
        (&json!("interrupted"), &json!("finished"), &json!(false))
    );
    assert_eq!(page.post("install").await.0, StatusCode::ACCEPTED, "an interrupted run does not block the next");
}

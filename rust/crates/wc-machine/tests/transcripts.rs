//! Replays the `contract/serial/` transcripts against `Machine` with a fake port and clock.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Map, Value};
use wc_config::{Settings, python_str};
use wc_machine::calibration::{Kind, calculate};
use wc_machine::optical::{Seen, Vision};
use wc_machine::{Effect, Event, Machine, Request};
use wc_protocol::optical::{Cell, Measurement, Orientation, PixelPoint, ScanRequest};
use wc_protocol::{Action, Namespace};
use wc_testkit::contract::Contract;
use wc_testkit::runner::{SerialBackend, replay};
use wc_testkit::transcript::{Transcript, UiAction};

struct MachineBackend {
    machine: Machine,
    now: Duration,
    unread: Vec<u8>,
    written: Vec<u8>,
    messages: Vec<(String, Value)>,
}

/// `str()` of a JSON number as the Python runner stores `setup.home`: whole numbers print without `.0`.
fn home_text(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 { format!("{}", value as i64) } else { python_str(&value.into()) }
}

impl MachineBackend {
    fn new() -> MachineBackend {
        let machine = Machine::new("127.0.0.1:5000", Settings::defaults());
        MachineBackend { machine, now: Duration::ZERO, unread: Vec::new(), written: Vec::new(), messages: Vec::new() }
    }

    fn settings(&self) -> Settings {
        (*self.machine.snapshot().settings).clone()
    }

    fn apply(&mut self, event: Event) {
        let effects = self.machine.handle_inline(event, self.now);
        self.collect(effects);
    }

    fn collect(&mut self, effects: Vec<Effect>) {
        effects.into_iter().for_each(|effect| match effect {
            Effect::Write(bytes) | Effect::Realtime(bytes) => self.written.extend(bytes),
            // The shell opens the port at once; the controller's banner then comes as the transcript's rx.
            Effect::OpenPort(port) => self.apply(Event::PortOpened { port }),
            Effect::Vision(request) => self.apply(Event::Seen(camera(&request))),
            Effect::Push(push) if push.ns == Namespace::MaslowCnc => {
                Contract::shared().validate_frame(&push.frame().encode()).unwrap();
                let data = push.data["data"].as_str().and_then(|t| serde_json::from_str(t).ok());
                let command = push.data["command"].as_str().unwrap_or_default().to_string();
                self.messages.push((command, data.unwrap_or_else(|| push.data["data"].clone())));
            }
            _ => {}
        });
    }
}

impl SerialBackend for MachineBackend {
    fn setup(&mut self, transcript: &Transcript) {
        let setup = &transcript.setup;
        let mut settings = Settings::defaults();
        setup.settings.iter().for_each(|(section, values)| {
            values.iter().for_each(|(key, value)| assert!(settings.set_value(section, key, value.clone()), "{key}"));
        });
        let units = setup.units.clone().unwrap_or_else(|| "MM".into());
        settings.set_value("Computed Settings", "units", units.clone().into());
        settings.set_value("Computed Settings", "unitsZ", setup.units_z.clone().unwrap_or(units).into());
        let (x, y) = setup.home.map_or((0.0, 0.0), |home| (home.x, home.y));
        settings.set_value("Advanced Settings", "homeX", home_text(x).into());
        settings.set_value("Advanced Settings", "homeY", home_text(y).into());
        self.machine = Machine::new("127.0.0.1:5000", settings);
        if let Some(lines) = transcript.gcode_lines() {
            self.machine.load_gcode(&(lines.join("\n") + "\n"));
        }
        self.apply(Event::PortOpened { port: "/dev/ttyACM0".into() });
    }

    fn receive(&mut self, bytes: &[u8]) {
        self.unread.extend_from_slice(bytes);
        while let Some(end) = self.unread.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.unread.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_string();
            self.apply(Event::ControllerLine(text));
        }
    }

    fn ui(&mut self, action: &UiAction) {
        let action = Action { command: action.command.clone(), arg: action.arg.clone(), arg1: action.arg1.clone() };
        let event = if action.is_realtime() { Event::Stop(action) } else { Event::Action(action) };
        self.apply(event);
    }

    /// The machine's HTTP writes, routed as wc-server routes them.
    fn http(&mut self, method: &str, path: &str, json: Option<&Value>, _: Option<&BTreeMap<String, String>>) -> u16 {
        let body = json.cloned().unwrap_or(Value::Null);
        let request = match (method, path) {
            ("PUT", _) if path.starts_with("/api/settings/") => {
                Request::UpdateSettings { section: path["/api/settings/".len()..].into(), body }
            }
            ("POST", "/api/machine/quick-configure") => Request::QuickConfigure(body),
            ("POST", "/api/system/pid-test") => Request::PidTest(body),
            ("POST", _) if path.starts_with("/api/calibration/") => match &path["/api/calibration/".len()..] {
                "triangular" => Request::Calibrated(calculate(Kind::Triangular, &self.settings(), &body)),
                "holey" => Request::Calibrated(calculate(Kind::Holey, &self.settings(), &body)),
                "triangular/accept" => Request::AcceptCalibration(Kind::Triangular),
                "holey/accept" => Request::AcceptCalibration(Kind::Holey),
                "optical/scan" => optical_scan(&body),
                _ => return 404,
            },
            _ => return 404,
        };
        let effects = self.machine.handle_inline(Event::Edit { id: 1, request }, self.now);
        let status = effects.iter().find_map(|effect| match effect {
            Effect::Respond { response, .. } => Some(response.status),
            _ => None,
        });
        self.collect(effects);
        status.expect("every request is answered")
    }

    fn advance(&mut self, ms: u64) {
        self.now += Duration::from_millis(ms);
        self.apply(Event::Tick);
    }

    fn take_written(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.written)
    }

    fn idle(&self) -> bool {
        true
    }

    fn take_messages(&mut self) -> Vec<(String, Value)> {
        std::mem::take(&mut self.messages)
    }

    fn state(&self) -> Map<String, Value> {
        self.machine.snapshot().state_fields()
    }

    fn setting(&self, section: &str, key: &str) -> Option<String> {
        self.machine.snapshot().settings.text(section, key)
    }
}

/// The optical transcripts' target lies this far right of and above its nominal place, in mm.
const TARGET_OFFSET: (f64, f64) = (0.8, -0.4);

/// A camera that sees the sled where it was sent: square-on, 10 px per mm, the square `TARGET_OFFSET` away.
fn camera(request: &Vision) -> Seen {
    match request {
        Vision::Measure { id, cell, target_mm, .. } => {
            let (x, y) = wc_optical::grid::nominal_mm(*cell);
            let (dx, dy) = (x + TARGET_OFFSET.0 - target_mm.0, y + TARGET_OFFSET.1 - target_mm.1);
            let centre_px = PixelPoint { x: 320.0 + 10.0 * dx, y: 240.0 - 10.0 * dy };
            let measurement = Measurement { dx_mm: dx, dy_mm: dy, centre_px, angle_deg: 0.0, frames_used: 10 };
            Seen::Measured { id: *id, result: Ok(measurement), png: None }
        }
        Vision::Orient { id, .. } => {
            Seen::Oriented { id: *id, result: Ok(Orientation { rotation_deg: 0.0, mirrored: false, px_per_mm: 10.0 }) }
        }
    }
}

/// The route's squares: the one `cell`, else the extents by rows (track A's `scan_order` once it lands).
fn optical_scan(body: &Value) -> Request {
    let scan: ScanRequest = serde_json::from_value(body.clone()).expect("a scan request");
    let s = &scan.settings;
    let rows = (s.br_y..=s.tl_y).rev().flat_map(|j| (s.tl_x..=s.br_x).map(move |i| Cell { i, j }));
    let order = wc_optical::grid::scan_order(s).unwrap_or_else(|_| rows.collect());
    Request::OpticalScan { cells: scan.cell.map_or(order, |cell| vec![cell]), settings: scan.settings }
}

fn corpus() -> Vec<Transcript> {
    Transcript::corpus().into_iter().map(|(_, t)| t).collect()
}

#[test]
fn every_transcript_passes() {
    let corpus = corpus();
    assert!(corpus.len() >= 39, "the whole corpus: {}", corpus.len());
    let failures: Vec<String> = corpus.iter().filter_map(|t| replay(&mut MachineBackend::new(), t).err()).collect();
    assert!(failures.is_empty(), "{} failing:\n{}", failures.len(), failures.join("\n"));
}

/// The milestone's hardware check, simulated: edit a setting, power-cycle the controller, and its `$$` shows the
/// edit held, so only `$13` goes out. Built from the owner's handshake transcript.
fn edit_then_reboot() -> Transcript {
    let path = wc_testkit::repo_root().join("contract/serial/transcripts/connect_boot_handshake.json");
    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    raw["meta"]["name"] = "settings_edit_then_reboot_writes_only_r".into();
    raw["setup"]["start"] = "connected".into();
    raw["setup"]["bootBanner"] = serde_json::json!(["PCB v1.1 Detected", "Grbl v1.00", "ready", "ok"]);
    let reboot: Vec<Value> = raw["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| match event["line"].as_str() {
            Some(line) if line.starts_with("$3=") => serde_json::json!({"type": "rx", "line": "$3=470.00000000"}),
            _ => event.clone(),
        })
        .collect();
    let edit = serde_json::json!([
        {"type": "http", "method": "PUT", "path": "/api/settings/maslow", "json": {"values": {"motorOffsetY": "470"}}, "status": 200},
        {"type": "tx", "bytes": "$3=470 \n"},
        {"type": "rx", "line": "ok"},
        {"type": "expect", "settings": [{"section": "Maslow Settings", "key": "motorOffsetY", "value": "470"}]},
    ]);
    raw["events"] = Value::Array([edit.as_array().unwrap().as_slice(), &reboot].concat());
    serde_json::from_value(raw).unwrap()
}

#[test]
fn an_edited_setting_the_controller_kept_is_not_written_again_after_a_reboot() {
    replay(&mut MachineBackend::new(), &edit_then_reboot()).unwrap();
}

#[test]
fn http_writes_the_machine_does_not_answer_are_404() {
    let mut backend = MachineBackend::new();
    assert_eq!(backend.http("GET", "/api/state", None, None), 404);
    let quick = serde_json::json!({});
    assert_eq!(backend.http("POST", "/api/machine/quick-configure", Some(&quick), None), 400);
    assert_eq!(backend.http("POST", "/api/system/pid-test", None, None), 400);
    assert_eq!(backend.http("POST", "/api/calibration/optical", None, None), 404);
    assert_eq!(backend.http("POST", "/api/calibration/triangular", None, None), 400);
    assert_eq!(backend.http("POST", "/api/calibration/triangular/accept", None, None), 409);
}

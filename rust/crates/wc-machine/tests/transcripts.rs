//! Replays `contract/serial/` transcripts (with the Rust overrides) against `Machine` with a fake port and clock.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Map, Value};
use wc_config::{Settings, python_str};
use wc_machine::{Effect, Event, Machine};
use wc_protocol::{Action, Namespace};
use wc_testkit::contract::Contract;
use wc_testkit::runner::{SerialBackend, replay};
use wc_testkit::transcript::{Transcript, UiAction};

/// Transcripts the Rust backend must already pass; each milestone adds the ones it makes pass.
const PASSING: &[&str] = &[
    "connect_boot_handshake",
    "connect_eeprom_read_fail_restores_settings",
    "connect_settings_sync_adopts_unknown_keys",
    "connect_without_a_reset_handshakes_after_3_s",
    "define_home_from_sled_then_home",
    "home_at_safe_height_mm",
    "jog_each_direction_inches",
    "jog_each_direction_mm",
    "move_z_in_inches_on_a_mm_machine",
    "move_z_raise_lower_and_zero_mm",
    "realtime_characters_stripped_from_lines",
    "return_to_center_inches",
    "stop_ignores_a_status_report_read_too_soon",
    "stop_mid_line_resets_the_index",
    "stop_with_a_stale_ok_in_flight",
    "stream_unbuffered_waits_for_a_slow_ok",
    "stream_unbuffered_with_home_shift",
    "touch_z_inches",
    "touch_z_mm",
    "touch_z_probe_fails_leaves_z_alone",
    "z_limits_on_51_292",
    "z_limits_refused_on_51_27",
];

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

    fn apply(&mut self, event: Event) {
        let effects = self.machine.handle(event, self.now);
        effects.into_iter().for_each(|effect| match effect {
            Effect::Write(bytes) | Effect::Realtime(bytes) => self.written.extend(bytes),
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

    fn http(&mut self, _: &str, _: &str, _: Option<&Value>, _: Option<&BTreeMap<String, String>>) -> u16 {
        404
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

fn corpus() -> Vec<Transcript> {
    Transcript::rust_corpus().into_iter().map(|(_, t)| t).collect()
}

#[test]
fn the_passing_list_names_real_transcripts() {
    let names: Vec<String> = corpus().into_iter().map(|t| t.meta.name).collect();
    PASSING.iter().for_each(|name| assert!(names.iter().any(|n| n == name), "{name} is not a transcript"));
}

#[test]
fn listed_transcripts_pass() {
    let failures: Vec<String> = corpus()
        .iter()
        .filter(|t| PASSING.contains(&t.meta.name.as_str()))
        .filter_map(|t| replay(&mut MachineBackend::new(), t).err())
        .collect();
    assert!(failures.is_empty(), "{} failing:\n{}", failures.len(), failures.join("\n"));
}

/// Python's handshake order is a wart the Rust backend must not copy (contract/serial README, wart 4).
#[test]
fn the_python_connect_transcripts_fail_on_the_rust_handshake() {
    let python = Transcript::corpus().into_iter().find(|(_, t)| t.meta.name == "connect_boot_handshake").unwrap().1;
    let err = replay(&mut MachineBackend::new(), &python).unwrap_err();
    assert!(err.contains("expected tx \"G21  \\n\", got \"B05  \\n\""), "{err}");
}

#[test]
#[ignore = "jobs land in M4; run with --ignored to see what is left"]
fn every_transcript_passes() {
    let failures: Vec<String> = corpus().iter().filter_map(|t| replay(&mut MachineBackend::new(), t).err()).collect();
    assert!(failures.is_empty(), "{} failing:\n{}", failures.len(), failures.join("\n"));
}

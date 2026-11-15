//! Replays `contract/serial/transcripts/` against `Machine` with a fake port and clock.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Map, Value};
use wc_machine::{Effect, Event, Machine};
use wc_protocol::{Action, Namespace};
use wc_testkit::runner::{SerialBackend, replay};
use wc_testkit::transcript::{Transcript, UiAction};

/// Transcripts the Rust backend must already pass; each milestone adds the ones it makes pass.
const PASSING: &[&str] = &[];

struct MachineBackend {
    machine: Machine,
    now: Duration,
    unread: Vec<u8>,
    written: Vec<u8>,
    messages: Vec<(String, Value)>,
}

impl MachineBackend {
    fn new() -> MachineBackend {
        let machine = Machine::new("127.0.0.1:5000");
        MachineBackend { machine, now: Duration::ZERO, unread: Vec::new(), written: Vec::new(), messages: Vec::new() }
    }

    fn apply(&mut self, event: Event) {
        let effects = self.machine.handle(event, self.now);
        effects.into_iter().for_each(|effect| match effect {
            Effect::Write(bytes) | Effect::Realtime(bytes) => self.written.extend(bytes),
            Effect::Push(push) if push.ns == Namespace::MaslowCnc => {
                let data = push.data["data"].as_str().and_then(|t| serde_json::from_str(t).ok());
                let command = push.data["command"].as_str().unwrap_or_default().to_string();
                self.messages.push((command, data.unwrap_or_else(|| push.data["data"].clone())));
            }
            Effect::Push(_) => {}
        });
    }
}

impl SerialBackend for MachineBackend {
    fn setup(&mut self, _: &Transcript) {}

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

    fn setting(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}

fn corpus() -> Vec<Transcript> {
    Transcript::corpus().into_iter().map(|(_, t)| t).collect()
}

#[test]
fn the_passing_list_names_real_transcripts() {
    let names: Vec<String> = corpus().into_iter().map(|t| t.meta.name).collect();
    PASSING.iter().for_each(|name| assert!(names.iter().any(|n| n == name), "{name} is not a transcript"));
}

#[test]
fn listed_transcripts_pass() {
    corpus()
        .iter()
        .filter(|t| PASSING.contains(&t.meta.name.as_str()))
        .for_each(|t| replay(&mut MachineBackend::new(), t).unwrap());
}

#[test]
fn the_runner_reaches_the_machine() {
    let transcript = corpus().into_iter().find(|t| t.meta.name == "connect_boot_handshake").unwrap();
    let err = replay(&mut MachineBackend::new(), &transcript).unwrap_err();
    assert!(err.contains("timed out waiting for tx \"G21  \\n\""), "{err}");
}

#[test]
#[ignore = "the serial link lands in M1; run with --ignored to see what is left"]
fn every_transcript_passes() {
    let failures: Vec<String> = corpus().iter().filter_map(|t| replay(&mut MachineBackend::new(), t).err()).collect();
    assert!(failures.is_empty(), "{} failing:\n{}", failures.len(), failures.join("\n"));
}

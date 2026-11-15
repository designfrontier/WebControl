//! Drives a `Machine` with a fake clock and records what it writes and pushes, checked against the contract.

#![allow(dead_code)]

pub mod sim;

use std::time::Duration;

use serde_json::{Value, json};
use wc_config::Settings;
use wc_machine::{Effect, Event, Machine};
use wc_protocol::{Action, Namespace};
use wc_testkit::contract::Contract;

pub struct Rig {
    pub machine: Machine,
    pub now: Duration,
    /// Every line written and not yet answered by `answer_until_quiet`.
    outbox: Vec<String>,
}

pub fn settings(changes: &[(&str, &str, Value)]) -> Settings {
    let mut settings = Settings::defaults();
    settings.set_value("Computed Settings", "units", json!("MM"));
    settings.set_value("Computed Settings", "unitsZ", json!("MM"));
    changes.iter().for_each(|(section, key, value)| assert!(settings.set_value(section, key, value.clone()), "{key}"));
    settings
}

/// Lines written to the port, realtime included, without their `\n`.
pub fn written(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Write(bytes) | Effect::Realtime(bytes) => {
                Some(String::from_utf8_lossy(bytes).trim_end().to_string())
            }
            _ => None,
        })
        .collect()
}

/// `/MaslowCNC` pushes as (command, decoded data), each validated against its closed schema.
pub fn messages(effects: &[Effect]) -> Vec<(String, Value)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Push(push) if push.ns == Namespace::MaslowCnc => {
                Contract::shared().validate_frame(&push.frame().encode()).unwrap();
                let json = push.data["dataFormat"] == "json";
                let data = push.data["data"].as_str().filter(|_| json).map(|t| serde_json::from_str(t).unwrap());
                Some((push.data["command"].as_str().unwrap().to_string(), data.unwrap_or(push.data["data"].clone())))
            }
            _ => None,
        })
        .collect()
}

pub fn commands(effects: &[Effect]) -> Vec<String> {
    messages(effects).into_iter().map(|(command, _)| command).collect()
}

pub fn alerts(effects: &[Effect]) -> Vec<String> {
    messages(effects)
        .into_iter()
        .filter(|(command, data)| command == "activateModal" && data["modalType"] == "alert")
        .map(|(_, data)| data["message"].as_str().unwrap().to_string())
        .collect()
}

pub fn pushed(effects: &[Effect], command: &str) -> Vec<Value> {
    messages(effects).into_iter().filter(|(c, _)| c == command).map(|(_, data)| data).collect()
}

pub fn action(command: &str, arg: Value, arg1: Value) -> Event {
    let action = Action::new(command, arg, arg1);
    if action.is_realtime() { Event::Stop(action) } else { Event::Action(action) }
}

impl Rig {
    pub fn with(settings: Settings) -> Rig {
        Rig { machine: Machine::new("10.0.0.5:5000", settings), now: Duration::ZERO, outbox: Vec::new() }
    }

    pub fn new() -> Rig {
        Rig::with(settings(&[]))
    }

    pub fn send(&mut self, event: Event) -> Vec<Effect> {
        let effects = self.machine.handle_inline(event, self.now);
        self.outbox.extend(written(&effects));
        effects
    }

    pub fn rx(&mut self, line: &str) -> Vec<Effect> {
        self.send(Event::ControllerLine(line.into()))
    }

    pub fn act(&mut self, command: &str) -> Vec<Effect> {
        self.send(action(command, json!(""), json!("")))
    }

    pub fn act_with(&mut self, command: &str, arg: Value, arg1: Value) -> Vec<Effect> {
        self.send(action(command, arg, arg1))
    }

    /// Ticks every 10 ms for `ms`, collecting the effects.
    pub fn advance(&mut self, ms: u64) -> Vec<Effect> {
        (0..ms.div_ceil(10))
            .flat_map(|_| {
                self.now += Duration::from_millis(10);
                self.send(Event::Tick)
            })
            .collect()
    }

    pub fn open(&mut self) -> Vec<Effect> {
        self.send(Event::PortOpened { port: "/dev/ttyACM0".into() })
    }

    /// Answers each written line as the transcript prelude does, until the machine writes nothing more.
    pub fn answer_until_quiet(&mut self, version: &str) -> Vec<String> {
        self.answer_with(version, &[])
    }

    /// `answer_until_quiet`, with `report` before the `ok` of every `$$`.
    pub fn answer_with(&mut self, version: &str, report: &[String]) -> Vec<String> {
        let mut all = Vec::new();
        loop {
            if self.outbox.is_empty() {
                self.advance(100);
            }
            let lines = std::mem::take(&mut self.outbox);
            if lines.is_empty() {
                return all;
            }
            for line in &lines {
                if line.trim_end() == "B05" {
                    self.rx(&format!("Firmware Version {version}"));
                }
                if line.trim_end() == "$$" {
                    report.iter().for_each(|reply| {
                        self.rx(reply);
                    });
                }
                self.rx("ok");
            }
            all.extend(lines);
        }
    }

    /// Port open, boot banner, the whole handshake answered.
    pub fn connected_with(settings: Settings) -> Rig {
        Rig::connected_on(settings, "51.27")
    }

    /// `connected_with` a controller that answers B05 with `version`.
    pub fn connected_on(settings: Settings, version: &str) -> Rig {
        let mut rig = Rig::with(settings);
        rig.boot_on(version);
        rig
    }

    /// The port opens, the board boots and the handshake is answered; returns what the handshake wrote.
    pub fn boot_on(&mut self, version: &str) -> Vec<String> {
        self.boot_reporting(version, &[])
    }

    /// `boot_on`, the controller answering `$$` with `report`.
    pub fn boot_reporting(&mut self, version: &str, report: &[String]) -> Vec<String> {
        self.open();
        ["PCB v1.6 TLE9201  Detected", "FAKE_SERVO off", "Grbl v1.00", "ready", "ok"].iter().for_each(|l| {
            self.rx(l);
        });
        let sent = self.answer_with(version, report);
        self.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
        self.advance(100);
        sent
    }

    /// The controller was restarted through its port (a `$38` write) and boots again.
    pub fn reboot(&mut self) -> Vec<String> {
        assert_eq!(self.machine.snapshot().link, wc_protocol::main::LinkStatus::Disconnected, "restarted");
        self.boot_on("51.27")
    }

    pub fn connected() -> Rig {
        Rig::connected_with(settings(&[]))
    }

    /// Forgets the lines written so far, so the next `run_to_quiet` returns only what follows.
    pub fn clear_written(&mut self) {
        self.outbox.clear();
    }

    /// Acks lines as they are written until the machine is quiet; returns what it wrote.
    pub fn run_to_quiet(&mut self) -> Vec<String> {
        self.answer_until_quiet("51.27")
    }
}

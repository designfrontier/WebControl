//! Replays a transcript against a backend with a fake clock, by the rules in `contract/serial/README.md`.

use std::collections::{BTreeMap, VecDeque};

use serde_json::{Map, Value};

use crate::transcript::{Alert, Emitted, Event, Expect, Start, Transcript, UiAction};

/// The clock moves in steps no longer than the Python serial loop's 10 ms pass.
pub const STEP_MS: u64 = 10;
const IDLE_LIMIT_MS: u64 = 10_000;
const PRELUDE_ROUNDS: usize = 1_000;
const PLANNER_CAPABILITIES: &str = "Capabilities: planner line-numbers accel-settings status-line";

/// A planner host's `N<n> <line>*<sum>` as its number and line.
fn unframed(line: &str) -> Option<(&str, &str)> {
    let (number, rest) = line.strip_prefix('N')?.split_once(' ')?;
    Some((number, rest.rsplit_once('*')?.0))
}

/// What a backend exposes to the runner: a fake serial port, a fake clock, the UI's inputs and its observable state.
pub trait SerialBackend {
    /// Applies `setup` (settings, units, home, gcode) the way the app loads them, before anything is received.
    fn setup(&mut self, transcript: &Transcript);
    /// Bytes from the controller.
    fn receive(&mut self, bytes: &[u8]);
    fn ui(&mut self, action: &UiAction);
    /// Makes the request and returns the status.
    fn http(&mut self, method: &str, path: &str, json: Option<&Value>, form: Option<&BTreeMap<String, String>>) -> u16;
    /// Lets `ms` of fake time pass.
    fn advance(&mut self, ms: u64);
    /// Every byte written to the port since the last call.
    fn take_written(&mut self) -> Vec<u8>;
    /// True once the backend has consumed every byte received.
    fn idle(&self) -> bool;
    /// `/MaslowCNC` message pushes since the last call, as (command, decoded data).
    fn take_messages(&mut self) -> Vec<(String, Value)>;
    /// `expect.state` names (`uploadFlag`, `gcodeIndex`, ...) with their current values.
    fn state(&self) -> Map<String, Value>;
    /// A stored setting as Python's `str()` prints it.
    fn setting(&self, section: &str, key: &str) -> Option<String>;
}

struct Replay<'a, B: SerialBackend> {
    backend: &'a mut B,
    transcript: &'a Transcript,
    now: u64,
    partial: Vec<u8>,
    lines: VecDeque<(String, u64)>,
}

fn ms(seconds: f64) -> u64 {
    (seconds * 1000.0).ceil() as u64
}

/// Objects match field by field, strings as substrings, lists element by element, numbers as f64.
pub fn matches(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(want), Value::Object(have)) => {
            want.iter().all(|(key, value)| have.get(key).is_some_and(|got| matches(value, got)))
        }
        (Value::String(want), Value::String(have)) => have.contains(want.as_str()),
        (Value::Array(want), Value::Array(have)) => {
            want.len() == have.len() && want.iter().zip(have).all(|(w, h)| matches(w, h))
        }
        (Value::Number(want), Value::Number(have)) => want.as_f64() == have.as_f64(),
        _ => expected == actual,
    }
}

fn state_matches(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::String(want), Value::String(have)) => want == have,
        (Value::Object(want), Value::Object(have)) => {
            want.iter().all(|(key, value)| have.get(key).is_some_and(|got| state_matches(value, got)))
        }
        _ => matches(expected, actual),
    }
}

fn is_alert(alert: &Alert, (command, data): &(String, Value)) -> bool {
    let field = |name: &str, want: &Option<String>| {
        want.as_ref().is_none_or(|w| data[name].as_str().is_some_and(|have| have.contains(w.as_str())))
    };
    command == "activateModal"
        && data["modalType"] == "alert"
        && field("title", &alert.title)
        && field("message", &alert.message)
}

fn is_emitted(want: &Emitted, (command, data): &(String, Value)) -> bool {
    *command == want.command && want.data.as_ref().is_none_or(|w| matches(w, data))
}

impl<B: SerialBackend> Replay<'_, B> {
    fn advance(&mut self, total: u64) {
        let mut left = total;
        while left > 0 {
            let step = left.min(STEP_MS);
            self.backend.advance(step);
            self.now += step;
            left -= step;
            self.collect();
        }
    }

    fn collect(&mut self) {
        self.partial.extend(self.backend.take_written());
        while let Some(end) = self.partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            self.lines.push_back((String::from_utf8_lossy(&line).into_owned(), self.now));
        }
    }

    fn wait_idle(&mut self) -> Result<(), String> {
        let start = self.now;
        while !self.backend.idle() {
            if self.now - start >= IDLE_LIMIT_MS {
                return Err("the backend never consumed the controller's lines".into());
            }
            self.advance(STEP_MS);
        }
        Ok(())
    }

    fn settle(&mut self, seconds: f64) -> Result<(), String> {
        self.wait_idle()?;
        self.advance(ms(seconds));
        self.collect();
        match self.lines.front() {
            Some((line, _)) => Err(format!("unexpected tx {line:?}")),
            None => Ok(()),
        }
    }

    fn receive(&mut self, line: &str) -> Result<(), String> {
        self.backend.receive(format!("{line}\r\n").as_bytes());
        self.wait_idle()
    }

    fn prelude_answer(&self, line: &str) -> Vec<String> {
        let firmware = &self.transcript.meta.firmware;
        let numbered = unframed(line.trim_end());
        let request = numbered.map_or(line.trim_end(), |(_, body)| body);
        let ok = numbered.map_or("ok".into(), |(number, _)| format!("ok N{number}"));
        let rewrites = firmware.build.as_deref() == Some("51.292") && request.starts_with('$') && request.contains('=');
        match request {
            "B05" => {
                let capabilities = (firmware.variant == "planner").then(|| PLANNER_CAPABILITIES.to_string());
                [format!("Firmware Version {}", firmware.version)].into_iter().chain(capabilities).chain([ok]).collect()
            }
            _ if rewrites => vec!["Change in setting, rewriting EEPROM".into(), ok],
            _ => vec![ok],
        }
    }

    fn prelude(&mut self) -> Result<(), String> {
        let setup = &self.transcript.setup;
        let default_banner = ["PCB v1.6 TLE9201  Detected", "FAKE_SERVO off", "Grbl v1.00", "ready", "ok"];
        let banner = setup.boot_banner.clone().unwrap_or_else(|| default_banner.map(String::from).to_vec());
        banner.iter().try_for_each(|line| self.receive(line))?;
        for _ in 0..PRELUDE_ROUNDS {
            self.advance(ms(self.transcript.timing.quiet));
            let Some((line, _)) = self.lines.pop_front() else {
                return self.finish_prelude();
            };
            self.prelude_answer(&line).iter().try_for_each(|answer| self.receive(answer))?;
        }
        Err("the backend kept writing during the prelude".into())
    }

    fn finish_prelude(&mut self) -> Result<(), String> {
        let setup = &self.transcript.setup;
        if let Some(sled) = setup.sled {
            let status = format!("<Idle,MPos:{:.2},{:.2},{:.2},WPos:0.000,0.000,0.000>", sled.x, sled.y, sled.z);
            self.receive(&status)?;
        }
        if setup.gcode_index != 0 {
            let goto = UiAction { command: "moveGcodeGoto".into(), arg: setup.gcode_index.into(), arg1: "".into() };
            self.backend.ui(&goto);
        }
        self.wait_idle()?;
        self.advance(ms(self.transcript.timing.quiet));
        Ok(())
    }

    fn tx(&mut self, bytes: &str, timeout: Option<f64>) -> Result<(), String> {
        let deadline = self.now + ms(timeout.unwrap_or(self.transcript.timing.tx_timeout));
        loop {
            self.collect();
            if let Some((line, at)) = self.lines.pop_front() {
                return match (line == bytes, at <= deadline) {
                    (false, _) => Err(format!("expected tx {bytes:?}, got {line:?}")),
                    (true, false) => Err(format!("tx {bytes:?} came {} ms late", at - deadline)),
                    (true, true) => Ok(()),
                };
            }
            if self.now >= deadline {
                return Err(format!("timed out waiting for tx {bytes:?}"));
            }
            self.advance(STEP_MS);
        }
    }

    fn expect(&mut self, expect: &Expect) -> Result<(), String> {
        let window = self.backend.take_messages();
        let failures: Vec<String> = [
            self.check_state(expect),
            self.check_settings(expect),
            check_present(&expect.emitted, &window, is_emitted, "not emitted"),
            check_absent(&expect.not_emitted, &window, is_emitted, "emitted"),
            check_present(&expect.alerts, &window, is_alert, "no alert"),
            check_absent(&expect.no_alerts, &window, is_alert, "alert"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if failures.is_empty() { Ok(()) } else { Err(failures.join("; ")) }
    }

    fn check_state(&self, expect: &Expect) -> Vec<String> {
        let actual = self.backend.state();
        expect
            .state
            .iter()
            .flatten()
            .filter(|(key, want)| !actual.get(*key).is_some_and(|have| state_matches(want, have)))
            .map(|(key, want)| format!("state {key}: expected {want}, got {:?}", actual.get(key)))
            .collect()
    }

    fn check_settings(&self, expect: &Expect) -> Vec<String> {
        expect
            .settings
            .iter()
            .flatten()
            .filter_map(|s| {
                let have = self.backend.setting(&s.section, &s.key);
                (have.as_deref() != Some(s.value.as_str()))
                    .then(|| format!("setting {}/{}: expected {:?}, got {have:?}", s.section, s.key, s.value))
            })
            .collect()
    }

    fn event(&mut self, event: &Event) -> Result<(), String> {
        if let Event::Tx { bytes, timeout, .. } = event {
            return self.tx(bytes, *timeout);
        }
        self.settle(self.transcript.timing.quiet)?;
        match event {
            Event::Rx { line, .. } => self.receive(line),
            Event::Ui { action, .. } => {
                self.backend.ui(action);
                Ok(())
            }
            Event::Http { method, path, json, form, status, .. } => {
                let got = self.backend.http(method, path, json.as_ref(), form.as_ref());
                match status {
                    Some(want) if *want != got => Err(format!("{method} {path}: expected status {want}, got {got}")),
                    _ => Ok(()),
                }
            }
            Event::Tick { seconds, .. } => {
                self.advance(ms(*seconds));
                Ok(())
            }
            Event::Expect(expect) => self.expect(expect),
            Event::Tx { .. } => unreachable!("handled above"),
        }
    }

    fn run(&mut self) -> Result<(), String> {
        self.backend.setup(self.transcript);
        if self.transcript.setup.start == Start::Connected {
            self.prelude().map_err(|e| format!("prelude: {e}"))?;
        }
        self.lines.clear();
        self.partial.clear();
        self.backend.take_messages();
        self.transcript
            .events
            .iter()
            .enumerate()
            .try_for_each(|(i, event)| self.event(event).map_err(|e| format!("event {i}: {e}")))?;
        let end = self.transcript.events.len();
        self.settle(self.transcript.timing.settle).map_err(|e| format!("after event {}: {e}", end))?;
        match self.partial.is_empty() {
            true => Ok(()),
            false => Err(format!("a line was left without its \\n: {:?}", String::from_utf8_lossy(&self.partial))),
        }
    }
}

fn check_present<T>(
    wanted: &Option<Vec<T>>,
    window: &[(String, Value)],
    hit: fn(&T, &(String, Value)) -> bool,
    verdict: &str,
) -> Vec<String>
where
    T: std::fmt::Debug,
{
    wanted
        .iter()
        .flatten()
        .filter(|want| !window.iter().any(|message| hit(want, message)))
        .map(|want| format!("{verdict}: {want:?} (window: {window:?})"))
        .collect()
}

fn check_absent<T>(
    unwanted: &Option<Vec<T>>,
    window: &[(String, Value)],
    hit: fn(&T, &(String, Value)) -> bool,
    verdict: &str,
) -> Vec<String>
where
    T: std::fmt::Debug,
{
    unwanted
        .iter()
        .flatten()
        .filter(|want| window.iter().any(|message| hit(want, message)))
        .map(|want| format!("{verdict}: {want:?}"))
        .collect()
}

/// Replays `transcript` against `backend`; the error names the event that failed and why.
pub fn replay<B: SerialBackend>(backend: &mut B, transcript: &Transcript) -> Result<(), String> {
    let mut replay = Replay { backend, transcript, now: 0, partial: Vec::new(), lines: VecDeque::new() };
    replay.run().map_err(|e| format!("{}: {e}", transcript.meta.name))
}

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use wc_testkit::runner::{SerialBackend, matches, replay};
use wc_testkit::transcript::{Event, Expect, Transcript, UiAction};

/// Plays the transcript's own tx back, each when the runner would expect it, and reports its expectations as met.
struct Oracle {
    events: Vec<Event>,
    quiet_ms: u64,
    armed: bool,
    cursor: usize,
    now: u64,
    scheduled: Vec<(u64, String)>,
    written: Vec<u8>,
    expects: Vec<Expect>,
    current: Option<Expect>,
    /// The transcript's `open` bauds, last first.
    opens: Vec<u32>,
}

fn is_input(event: &Event) -> bool {
    matches!(event, Event::Rx { .. } | Event::Ui { .. } | Event::Http { .. })
}

impl Oracle {
    fn new(transcript: &Transcript) -> Oracle {
        let expects = transcript
            .events
            .iter()
            .filter_map(|e| if let Event::Expect(x) = e { Some((**x).clone()) } else { None })
            .collect();
        Oracle {
            events: transcript.events.clone(),
            quiet_ms: (transcript.timing.quiet * 1000.0) as u64,
            armed: false,
            cursor: 0,
            now: 0,
            scheduled: Vec::new(),
            written: Vec::new(),
            expects,
            current: None,
            opens: transcript
                .events
                .iter()
                .rev()
                .filter_map(|e| if let Event::Open { baud, .. } = e { Some(*baud) } else { None })
                .collect(),
        }
    }

    /// Schedules the tx events up to the next input, delayed by the settles and ticks the runner does before them.
    fn schedule_from(&mut self, start: usize) {
        let mut delay = 0;
        for event in self.events[start..].iter().take_while(|e| !is_input(e)) {
            match event {
                Event::Tx { bytes, .. } => self.scheduled.push((self.now + delay.max(1), bytes.clone())),
                Event::Tick { seconds, .. } => delay += self.quiet_ms + (seconds * 1000.0) as u64 - 5,
                _ => delay += self.quiet_ms,
            }
        }
    }

    fn input(&mut self, matcher: impl Fn(&Event) -> bool) {
        if !self.armed {
            return;
        }
        let next = self.events[self.cursor..].iter().position(is_input).map(|i| i + self.cursor);
        if let Some(index) = next.filter(|&i| matcher(&self.events[i])) {
            self.cursor = index + 1;
            self.schedule_from(index + 1);
        }
    }
}

impl SerialBackend for Oracle {
    fn setup(&mut self, _: &Transcript) {}
    fn receive(&mut self, bytes: &[u8]) {
        let line = String::from_utf8_lossy(bytes).trim_end_matches("\r\n").to_string();
        self.input(|e| matches!(e, Event::Rx { line: l, .. } if *l == line));
    }
    fn ui(&mut self, action: &UiAction) {
        self.input(|e| matches!(e, Event::Ui { action: a, .. } if a == action));
    }
    fn http(&mut self, method: &str, path: &str, _: Option<&Value>, _: Option<&BTreeMap<String, String>>) -> u16 {
        let status = self.events[self.cursor..]
            .iter()
            .find_map(|e| match e {
                Event::Http { method: m, path: p, status, .. } if m == method && p == path => {
                    Some(status.unwrap_or(200))
                }
                _ => None,
            })
            .unwrap_or(200);
        self.input(|e| matches!(e, Event::Http { method: m, path: p, .. } if m == method && p == path));
        status
    }
    fn advance(&mut self, ms: u64) {
        self.now += ms;
        let now = self.now;
        let (due, later): (Vec<_>, Vec<_>) = self.scheduled.drain(..).partition(|(at, _)| *at <= now);
        self.scheduled = later;
        due.into_iter().for_each(|(_, bytes)| self.written.extend(bytes.into_bytes()));
    }
    fn take_written(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.written)
    }
    fn idle(&self) -> bool {
        true
    }
    fn take_messages(&mut self) -> Vec<(String, Value)> {
        if !self.armed {
            self.armed = true;
            self.schedule_from(0);
            return Vec::new();
        }
        self.current = (!self.expects.is_empty()).then(|| self.expects.remove(0));
        let expect = self.current.clone().unwrap_or_default();
        let emitted = expect.emitted.iter().flatten().map(|e| (e.command.clone(), e.data.clone().unwrap_or(json!({}))));
        let alerts = expect.alerts.iter().flatten().map(|a| {
            let data = json!({"title": a.title.clone().unwrap_or_default(), "message": a.message.clone().unwrap_or_default(), "modalType": "alert"});
            ("activateModal".to_string(), data)
        });
        emitted.chain(alerts).collect()
    }
    fn state(&self) -> Map<String, Value> {
        self.current.as_ref().and_then(|e| e.state.clone()).unwrap_or_default()
    }
    fn setting(&self, section: &str, key: &str) -> Option<String> {
        let settings = self.current.as_ref()?.settings.as_ref()?;
        settings.iter().find(|s| s.section == section && s.key == key).map(|s| s.value.clone())
    }
    fn take_opened(&mut self) -> Vec<u32> {
        self.opens.pop().into_iter().collect()
    }
}

#[test]
fn the_runner_replays_every_transcript_against_a_backend_that_writes_its_tx() {
    Transcript::corpus().iter().for_each(|(_, transcript)| {
        replay(&mut Oracle::new(transcript), transcript).unwrap();
    });
}

/// A backend that answers inputs with fixed writes and records what it was given.
#[derive(Default)]
struct Scripted {
    replies: Vec<(String, String, bool)>,
    delay_ms: u64,
    pending: Vec<(u64, String)>,
    now: u64,
    written: Vec<u8>,
    received: Vec<String>,
    actions: Vec<UiAction>,
    messages: Vec<(String, Value)>,
    state: Map<String, Value>,
    settings: BTreeMap<(String, String), String>,
    status: u16,
    busy: bool,
    prelude_done: bool,
}

impl Scripted {
    /// Writes `write` the first time `input` arrives.
    fn on(mut self, input: &str, write: &str) -> Self {
        self.replies.push((input.into(), write.into(), false));
        self
    }
    fn always(mut self, input: &str, write: &str) -> Self {
        self.replies.push((input.into(), write.into(), true));
        self
    }
    fn trigger(&mut self, input: &str) {
        let writes: Vec<String> = self.replies.iter().filter(|(i, ..)| i == input).map(|(_, w, _)| w.clone()).collect();
        self.replies.retain(|(i, _, repeat)| i != input || *repeat);
        writes.into_iter().for_each(|w| self.pending.push((self.now + self.delay_ms, w)));
    }
}

impl SerialBackend for Scripted {
    fn setup(&mut self, _: &Transcript) {
        self.trigger("setup");
    }
    fn receive(&mut self, bytes: &[u8]) {
        let line = String::from_utf8_lossy(bytes).to_string();
        self.received.push(line.clone());
        self.trigger(line.trim_end());
    }
    fn ui(&mut self, action: &UiAction) {
        self.actions.push(action.clone());
        self.trigger(&action.command);
    }
    fn http(&mut self, _: &str, path: &str, _: Option<&Value>, _: Option<&BTreeMap<String, String>>) -> u16 {
        self.trigger(path);
        self.status
    }
    fn advance(&mut self, ms: u64) {
        self.now += ms;
        let now = self.now;
        let (due, later): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|(at, _)| *at <= now);
        self.pending = later;
        due.into_iter().for_each(|(_, w)| self.written.extend(w.into_bytes()));
    }
    fn take_written(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.written)
    }
    fn idle(&self) -> bool {
        !self.busy
    }
    /// The first call is the runner forgetting the prelude; `messages` are for the events after it.
    fn take_messages(&mut self) -> Vec<(String, Value)> {
        match std::mem::replace(&mut self.prelude_done, true) {
            true => std::mem::take(&mut self.messages),
            false => Vec::new(),
        }
    }
    fn state(&self) -> Map<String, Value> {
        self.state.clone()
    }
    fn setting(&self, section: &str, key: &str) -> Option<String> {
        self.settings.get(&(section.to_string(), key.to_string())).cloned()
    }
}

fn transcript(setup: Value, events: Value) -> Transcript {
    let doc = json!({"formatVersion": 1, "meta": {"name": "t", "description": "d",
        "firmware": {"variant": "holey", "version": "51.27"}, "webcontrolCommit": "c"},
        "setup": setup, "events": events});
    Transcript::parse(&doc.to_string()).unwrap()
}

fn boot(events: Value) -> Transcript {
    transcript(json!({"start": "boot"}), events)
}

#[test]
fn a_matching_exchange_passes() {
    let t = boot(json!([{"type": "rx", "line": "ok"}, {"type": "tx", "bytes": "G21  \n"}]));
    replay(&mut Scripted::default().on("ok", "G21  \n"), &t).unwrap();
}

#[test]
fn writes_may_be_split_anywhere() {
    let t = boot(
        json!([{"type": "rx", "line": "ok"}, {"type": "tx", "bytes": "G21  \n"}, {"type": "tx", "bytes": "$$\n"}]),
    );
    replay(&mut Scripted::default().on("ok", "G2").on("ok", "1  \n$").on("ok", "$\n"), &t).unwrap();
}

#[test]
fn a_different_line_fails() {
    let t = boot(json!([{"type": "rx", "line": "ok"}, {"type": "tx", "bytes": "G21  \n"}]));
    let err = replay(&mut Scripted::default().on("ok", "G20  \n"), &t).unwrap_err();
    assert!(err.contains("event 1") && err.contains("G20"), "{err}");
}

#[test]
fn an_open_the_backend_did_not_make_fails() {
    let t = boot(json!([{"type": "open", "baud": 115200}]));
    let err = replay(&mut Scripted::default(), &t).unwrap_err();
    assert!(err.contains("event 0") && err.contains("115200"), "{err}");
}

#[test]
fn a_line_that_never_comes_times_out() {
    let t = boot(json!([{"type": "rx", "line": "ok"}, {"type": "tx", "bytes": "G21  \n", "timeout": 0.5}]));
    assert!(replay(&mut Scripted::default(), &t).unwrap_err().contains("timed out"));
}

#[test]
fn a_line_written_after_its_timeout_fails_even_if_right() {
    let t = boot(json!([{"type": "rx", "line": "ok"}, {"type": "tx", "bytes": "G21  \n", "timeout": 0.105}]));
    let mut slow = Scripted { delay_ms: 110, ..Scripted::default() }.on("ok", "G21  \n");
    assert!(replay(&mut slow, &t).unwrap_err().contains("ms late"));
}

#[test]
fn a_write_nobody_expected_fails_at_the_next_settle() {
    let t = boot(json!([{"type": "rx", "line": "ok"}, {"type": "rx", "line": "ok"}]));
    let err = replay(&mut Scripted::default().on("ok", "G21  \n"), &t).unwrap_err();
    assert!(err.contains("event 1: unexpected tx"), "{err}");
}

#[test]
fn a_write_after_the_last_event_fails() {
    let t = boot(json!([{"type": "rx", "line": "ok"}]));
    let err = replay(&mut Scripted::default().on("ok", "G21  \n"), &t).unwrap_err();
    assert!(err.contains("after event 1"), "{err}");
}

#[test]
fn a_line_left_without_its_newline_fails() {
    let t = boot(json!([{"type": "rx", "line": "ok"}]));
    let err = replay(&mut Scripted::default().on("ok", "G21"), &t).unwrap_err();
    assert!(err.contains("without its"), "{err}");
}

#[test]
fn a_backend_that_never_reads_fails() {
    let t = boot(json!([{"type": "rx", "line": "ok"}]));
    let err = replay(&mut Scripted { busy: true, ..Scripted::default() }, &t).unwrap_err();
    assert!(err.contains("never consumed"), "{err}");
}

#[test]
fn rx_lines_arrive_with_crlf_and_ui_actions_pass_through() {
    let t = boot(json!([{"type": "rx", "line": "Grbl v1.00"},
        {"type": "ui", "action": {"command": "move", "arg": "up", "arg1": 10}}]));
    let mut backend = Scripted::default();
    replay(&mut backend, &t).unwrap();
    assert_eq!(backend.received, ["Grbl v1.00\r\n"]);
    assert_eq!(backend.actions[0].command, "move");
    assert_eq!(backend.actions[0].arg1, json!(10));
}

#[test]
fn http_status_is_checked_when_given() {
    let t = boot(json!([{"type": "http", "method": "GET", "path": "/api/state", "status": 200}]));
    replay(&mut Scripted { status: 200, ..Scripted::default() }, &t).unwrap();
    let err = replay(&mut Scripted { status: 409, ..Scripted::default() }, &t).unwrap_err();
    assert!(err.contains("expected status 200, got 409"), "{err}");
    let unchecked = boot(json!([{"type": "http", "method": "POST", "path": "/x", "form": {"a": "b"}}]));
    replay(&mut Scripted { status: 500, ..Scripted::default() }, &unchecked).unwrap();
}

#[test]
fn writes_during_a_tick_are_matched_after_it() {
    let t = boot(
        json!([{"type": "rx", "line": "ok"}, {"type": "tick", "seconds": 0.5}, {"type": "tx", "bytes": "~\n", "realtime": true}]),
    );
    replay(&mut Scripted { delay_ms: 300, ..Scripted::default() }.on("ok", "~\n"), &t).unwrap();
}

fn expect(fields: Value) -> Transcript {
    let mut event = json!({"type": "expect"});
    event.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    boot(json!([event]))
}

fn backend_with(messages: Vec<(&str, Value)>) -> Scripted {
    Scripted { messages: messages.into_iter().map(|(c, d)| (c.to_string(), d)).collect(), ..Scripted::default() }
}

#[test]
fn state_compares_numbers_as_f64_and_strings_exactly() {
    let t = expect(json!({"state": {"uploadFlag": 0, "units": "MM", "home": {"x": 1}}}));
    let state = json!({"uploadFlag": 0.0, "units": "MM", "home": {"x": 1.0, "y": 2}});
    let mut backend = Scripted { state: state.as_object().unwrap().clone(), ..Scripted::default() };
    replay(&mut backend, &t).unwrap();
    backend.state.insert("units".into(), json!("MMX"));
    assert!(replay(&mut backend, &t).unwrap_err().contains("state units"));
    backend.state.remove("units");
    assert!(replay(&mut backend, &t).unwrap_err().contains("got None"));
}

#[test]
fn settings_compare_as_text() {
    let t = expect(json!({"settings": [{"section": "Maslow Settings", "key": "bufferOn", "value": "1"}]}));
    let mut backend = Scripted::default();
    assert!(replay(&mut backend, &t).unwrap_err().contains("bufferOn"));
    backend.settings.insert(("Maslow Settings".into(), "bufferOn".into()), "1".into());
    replay(&mut backend, &t).unwrap();
}

#[test]
fn emitted_messages_match_by_subset_and_substring() {
    let t = expect(json!({"emitted": [{"command": "controllerMessage", "data": {"data": "insert tool"}}]}));
    replay(&mut backend_with(vec![("controllerMessage", json!({"data": "Please insert tool 2"}))]), &t).unwrap();
    let err = replay(&mut backend_with(vec![("controllerMessage", json!({"data": "nope"}))]), &t).unwrap_err();
    assert!(err.contains("not emitted"), "{err}");
    let any_data = expect(json!({"emitted": [{"command": "clearAlarm"}]}));
    replay(&mut backend_with(vec![("clearAlarm", json!({}))]), &any_data).unwrap();
}

#[test]
fn not_emitted_fails_when_the_message_was_sent() {
    let t = expect(json!({"notEmitted": [{"command": "alarm"}]}));
    replay(&mut backend_with(vec![]), &t).unwrap();
    assert!(replay(&mut backend_with(vec![("alarm", json!({"message": "x"}))]), &t).unwrap_err().contains("emitted"));
}

#[test]
fn alerts_are_activate_modals_of_type_alert() {
    let t = expect(json!({"alerts": [{"message": "Cannot issue"}]}));
    let alert = ("activateModal", json!({"title": "Alert", "message": "Cannot issue command", "modalType": "alert"}));
    replay(&mut backend_with(vec![alert.clone()]), &t).unwrap();
    let note =
        ("activateModal", json!({"title": "Alert", "message": "Cannot issue command", "modalType": "notification"}));
    assert!(replay(&mut backend_with(vec![note]), &t).unwrap_err().contains("no alert"));
    let none = expect(json!({"noAlerts": [{"title": "Alert"}]}));
    replay(&mut backend_with(vec![]), &none).unwrap();
    assert!(replay(&mut backend_with(vec![alert]), &none).unwrap_err().contains("alert"));
}

#[test]
fn the_prelude_boots_the_backend_and_answers_its_writes() {
    let setup = json!({"sled": {"x": 1.234, "y": -2, "z": 0}, "gcodeIndex": 3});
    let t = transcript(setup, json!([]));
    let mut backend = Scripted::default().on("ok", "B05\n");
    replay(&mut backend, &t).unwrap();
    let received: Vec<&str> = backend.received.iter().map(|s| s.trim_end()).collect();
    assert_eq!(received[..5], ["PCB v1.6 TLE9201  Detected", "FAKE_SERVO off", "Grbl v1.00", "ready", "ok"]);
    assert!(received.contains(&"Firmware Version 51.27"));
    assert_eq!(received.last().unwrap(), &"<Idle,MPos:1.23,-2.00,0.00,WPos:0.000,0.000,0.000>");
    assert_eq!(backend.actions[0].command, "moveGcodeGoto");
    assert_eq!(backend.actions[0].arg, json!(3));
}

#[test]
fn the_prelude_answers_setting_writes_on_51_292_with_the_rewrite_notice() {
    let mut t = transcript(json!({"bootBanner": ["ok"]}), json!([]));
    t.meta.firmware.build = Some("51.292".into());
    let mut backend = Scripted::default().on("ok", "$13=63.5\n");
    replay(&mut backend, &t).unwrap();
    let received: Vec<&str> = backend.received.iter().map(|s| s.trim_end()).collect();
    assert_eq!(received[..3], ["ok", "Change in setting, rewriting EEPROM", "ok"]);
}

#[test]
fn a_backend_that_writes_forever_fails_the_prelude() {
    let t = transcript(json!({"bootBanner": ["ok"]}), json!([]));
    let err = replay(&mut Scripted::default().always("ok", "G21\n"), &t).unwrap_err();
    assert!(err.contains("prelude"), "{err}");
}

#[test]
fn the_matcher_follows_the_readme() {
    assert!(matches(&json!({"a": [1, "b"]}), &json!({"a": [1.0, "abc"], "z": 0})));
    assert!(!matches(&json!([1]), &json!([1, 2])));
    assert!(!matches(&json!(true), &json!(1)));
    assert!(matches(&json!(null), &json!(null)));
    assert!(!matches(&json!({"a": 1}), &json!({})));
}

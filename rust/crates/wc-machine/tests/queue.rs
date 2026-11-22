//! The job queue: each file starts when the one before it has finished, after Z rises; only a bit the files show to
//! differ waits at the tool-change prompt, unless the file's override skips it; Stop, an alarm, a lost link, a reset,
//! a refused or lost line, a failed probe, an unknown bit height or a failed load stop it.

mod rig;

use std::collections::VecDeque;
use std::path::PathBuf;

use rig::{Rig, alerts, pushed, settings};
use serde_json::{Value, json};
use wc_gcode::firmware_gcode::file_tools;
use wc_machine::files::{Gcode, Origin, Problem, Request as Ask};
use wc_machine::queue::{Edit, Item, Outbound};
use wc_machine::{Effect, Event, Machine, Request, Response};
use wc_protocol::queue::ContinueFrom;

const LIBRARY: &str = "/library";
const SAFE: &str = "G00 Z5.0";
const PROMPT: [&str; 2] = ["Tool Change: Please insert tool ", "Maslow Paused"];

/// A machine, the files the server would load for it, and a controller that pauses on a new tool.
struct Shop {
    rig: Rig,
    files: Vec<(&'static str, &'static str)>,
    outbox: VecDeque<String>,
    effects: Vec<Effect>,
    /// The tool the firmware holds; `None` makes it ack every M6 without a pause.
    firmware_tool: Option<i64>,
    paused: bool,
    /// The next load fails, as an unreadable file does.
    unreadable: bool,
    /// Loads wait in `held` instead of being answered.
    hold: bool,
    held: Vec<String>,
    /// The probe misses the plate instead of touching it without `z axis zeroed`.
    probe_misses: bool,
}

fn lines(text: &str) -> String {
    text.split('|').map(|line| format!("{line}\n")).collect()
}

impl Shop {
    fn new(files: &[(&'static str, &'static str)]) -> Shop {
        Shop::with(settings(&[]), files)
    }

    fn with(settings: wc_config::Settings, files: &[(&'static str, &'static str)]) -> Shop {
        let mut shop = Shop::unreported(settings, files);
        shop.rig.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
        shop.rig.advance(100);
        shop
    }

    /// Connected, but no status report has told the bit's height.
    fn unreported(settings: wc_config::Settings, files: &[(&'static str, &'static str)]) -> Shop {
        let mut rig = Rig::with(settings);
        rig.machine = rig.machine.clone().with_library(PathBuf::from(LIBRARY));
        rig.open();
        ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
            rig.rx(line);
        });
        rig.answer_until_quiet("51.27");
        let files = files.to_vec();
        Shop {
            rig,
            files,
            outbox: VecDeque::new(),
            effects: Vec::new(),
            firmware_tool: Some(0),
            paused: false,
            unreadable: false,
            hold: false,
            held: Vec::new(),
            probe_misses: false,
        }
    }

    fn text(&self, name: &str) -> String {
        lines(self.files.iter().find(|(file, _)| *file == name).expect("a known file").1)
    }

    fn gcode(&self, name: &str) -> Box<Gcode> {
        Box::new(Gcode::parse(&self.text(name), &self.rig.machine.snapshot().settings).unwrap())
    }

    fn take(&mut self, effects: Vec<Effect>) {
        self.outbox.extend(rig::written(&effects));
        let loads: Vec<String> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Queue(Outbound::Load(file)) => Some(file.clone()),
                _ => None,
            })
            .collect();
        self.effects.extend(effects);
        if self.hold {
            return self.held.extend(loads);
        }
        loads.into_iter().for_each(|file| {
            let request = match std::mem::take(&mut self.unreadable) {
                true => Ask::Problem(Problem::GcodeUnreadable),
                false => Ask::LoadGcode { gcode: self.gcode(&file), file: Some(path(&file)), origin: Origin::Queue },
            };
            self.event(Event::Request { id: 7, request });
        });
    }

    fn event(&mut self, event: Event) {
        let effects = self.rig.send(event);
        self.take(effects);
    }

    fn act(&mut self, command: &str) {
        self.event(rig::action(command, json!(""), json!("")));
    }

    fn rx(&mut self, line: &str) {
        self.event(Event::ControllerLine(line.into()));
    }

    /// Opens `name` as `/openGCode` does.
    fn open(&mut self, name: &str) {
        let request = Ask::LoadGcode { gcode: self.gcode(name), file: Some(path(name)), origin: Origin::Open };
        self.event(Event::Request { id: 3, request });
    }

    fn edit(&mut self, edit: Edit) -> Response {
        let effects = self.rig.send(Event::Edit { id: 5, request: Request::Queue(edit) });
        let response = effects.iter().find_map(|effect| match effect {
            Effect::Respond { response, .. } => Some(response.clone()),
            _ => None,
        });
        self.take(effects);
        response.expect("every edit is answered")
    }

    fn add(&mut self, name: &str) -> u64 {
        self.add_with(name, false)
    }

    fn add_with(&mut self, name: &str, skip_bit_changes: bool) -> u64 {
        let tools = file_tools(self.text(name).lines());
        let response = self.edit(Edit::Add(Item { file: name.into(), tools, skip_bit_changes }));
        assert_eq!(response.status, 200, "{response:?}");
        response.body["items"].as_array().unwrap().last().unwrap()["id"].as_u64().unwrap()
    }

    /// The controller's answers to one line: a tool change pauses, `~` releases it with the M6's `ok`.
    fn answer(&mut self, line: &str) {
        let m6_tool = line.strip_prefix('T').and_then(|rest| rest.strip_suffix(" M6")).and_then(|t| t.parse().ok());
        match (line, m6_tool) {
            (probe, _) if probe.contains("G38.2") && self.probe_misses => {
                ["error: probe did not connect", "program stopped", "z axis not set", ""]
                    .iter()
                    .for_each(|l| self.rx(l));
            }
            ("~", _) if self.paused => {
                self.paused = false;
                ["ok", "ok"].iter().for_each(|reply| self.rx(reply));
            }
            (_, Some(tool)) if self.firmware_tool.is_some_and(|held| held != tool) => {
                self.firmware_tool = Some(tool);
                self.paused = true;
                self.rx(&format!("{}{tool}", PROMPT[0]));
                self.rx(PROMPT[1]);
            }
            _ => self.rx("ok"),
        }
    }

    /// Answers every line until the machine writes nothing more; returns what it wrote.
    fn run(&mut self) -> Vec<String> {
        let mut sent = Vec::new();
        loop {
            while let Some(line) = self.outbox.pop_front() {
                sent.push(line.clone());
                self.answer(&line);
            }
            let effects = self.rig.advance(100);
            self.take(effects);
            if self.outbox.is_empty() {
                return sent;
            }
        }
    }

    fn queue(&self) -> Value {
        serde_json::to_value(&*self.rig.machine.snapshot().queue).unwrap()
    }

    fn files(&self) -> Vec<String> {
        self.queue()["items"].as_array().unwrap().iter().map(|item| item["file"].as_str().unwrap().into()).collect()
    }

    fn loads(&mut self) -> usize {
        let effects = std::mem::take(&mut self.effects);
        effects.iter().filter(|effect| matches!(effect, Effect::Queue(Outbound::Load(_)))).count()
    }

    /// A tool change typed before the job, so the spindle's tool is known.
    fn fit(&mut self, tool: i64) {
        self.event(Event::Request { id: 9, request: Ask::SendGcode(format!("T{tool} M6")) });
        self.run();
        self.act("resumeRun");
        self.run();
        self.effects.clear();
    }
}

fn path(name: &str) -> PathBuf {
    PathBuf::from(LIBRARY).join(name)
}

const A: (&str, &str) = ("a.nc", "G0 Z5|G1 X1 F100|G0 Z5");
const SAME: (&str, &str) = ("same.nc", "T1 M6|G1 X2");
const OTHER: (&str, &str) = ("other.nc", "T2 M6|G1 X3");
const UNNAMED: (&str, &str) = ("unnamed.nc", "G1 X4");

/// Tool 1 in the spindle, `a.nc` loaded and queued first, then `next`, then Play.
fn playing(next: (&'static str, &'static str)) -> Shop {
    let mut shop = Shop::new(&[A, next]);
    shop.fit(1);
    shop.open("a.nc");
    shop.add("a.nc");
    shop.add(next.0);
    shop.act("startRun");
    shop
}

#[test]
fn a_file_with_the_same_tool_starts_once_z_has_risen() {
    let mut shop = playing(SAME);
    assert_eq!(shop.run(), ["G0 Z5", "G1 X1 F100", "G0 Z5", SAFE, "T1 M6", "G1 X2"]);
    assert_eq!(shop.loads(), 1);
    assert_eq!(shop.rig.machine.snapshot().gcode_name.as_deref(), Some("same.nc"));
    assert_eq!(shop.queue()["state"], "idle", "{}", shop.queue());
    assert_eq!(shop.files(), Vec::<String>::new());
}

#[test]
fn a_file_with_another_tool_waits_at_the_tool_change_prompt_until_resume() {
    let mut shop = playing(OTHER);
    assert_eq!(shop.run(), ["G0 Z5", "G1 X1 F100", "G0 Z5", SAFE, "M5", "T2 M6"]);
    let snapshot = shop.rig.machine.snapshot();
    assert!(snapshot.manual_z_axis_adjust && !snapshot.job_in_progress);
    assert_eq!(shop.queue()["step"], "toolChange");
    assert_eq!(shop.files(), ["other.nc"]);
    shop.act("resumeRun");
    assert_eq!(shop.run(), ["~", SAFE, "T2 M6", "G1 X3"]);
    assert_eq!(shop.rig.machine.snapshot().current_tool, 2);
}

#[test]
fn a_file_starts_at_once_while_the_bit_in_the_spindle_is_unknown() {
    let mut shop = Shop::new(&[A, SAME]);
    shop.open("a.nc");
    shop.add("a.nc");
    shop.add("same.nc");
    assert_eq!(shop.queue()["items"][1]["between"], "unknownTool");
    shop.act("startRun");
    assert_eq!(shop.run(), ["G0 Z5", "G1 X1 F100", "G0 Z5", SAFE, "T1 M6"], "the file's own M6 pauses as in any job");
    assert_eq!((shop.queue()["state"].as_str(), shop.queue()["step"].as_str()), (Some("running"), Some("none")));
    assert!(shop.rig.machine.snapshot().job_in_progress);
}

#[test]
fn a_file_that_names_no_tool_starts_at_once() {
    let mut shop = playing(UNNAMED);
    assert_eq!(shop.queue()["items"][1]["between"], "noTool");
    assert_eq!(shop.run(), ["G0 Z5", "G1 X1 F100", "G0 Z5", SAFE, "G1 X4"]);
    assert!(!shop.rig.machine.snapshot().manual_z_axis_adjust);
}

#[test]
fn a_controller_that_takes_the_m6_without_pausing_still_gets_the_prompt() {
    let mut shop = playing(OTHER);
    shop.firmware_tool = None;
    assert_eq!(shop.run(), ["G0 Z5", "G1 X1 F100", "G0 Z5", SAFE, "M5", "T2 M6"]);
    let prompts = pushed(&shop.effects, "activateModal");
    assert!(prompts.iter().any(|modal| modal["message"] == "Please insert tool 2"), "{prompts:?}");
    assert!(shop.rig.machine.snapshot().manual_z_axis_adjust);
}

fn stopped_by(shop: &mut Shop, reason: &str) {
    shop.effects.clear();
    let queue = shop.queue();
    assert_eq!((queue["state"].as_str(), queue["step"].as_str()), (Some("stopped"), Some("none")), "{queue}");
    assert!(queue["reason"].as_str().unwrap().contains(reason), "{queue}");
    shop.run();
    assert_eq!(shop.loads(), 0, "nothing more is loaded");
}

#[test]
fn stop_mid_file_or_at_the_prompt_stops_the_queue() {
    let mut shop = playing(SAME);
    shop.rig.advance(100);
    shop.act("stopRun");
    stopped_by(&mut shop, "Stop");
    assert_eq!(shop.files(), ["a.nc", "same.nc"]);

    let mut shop = playing(OTHER);
    shop.run();
    shop.act("stopRun");
    stopped_by(&mut shop, "Stop");
    assert_eq!(shop.files(), ["other.nc"]);
}

#[test]
fn an_alarm_stops_the_queue_even_once_play_has_finished_the_file() {
    let mut shop = playing(SAME);
    shop.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    shop.rig.advance(300);
    shop.rx("<Idle,MPos:0.00,0.00,5.00,WPos:0.000,0.000,0.000>");
    assert_eq!(shop.queue()["state"], "stopped");
    shop.act("startRun");
    stopped_by(&mut shop, "alarm");
    assert!(!shop.rig.machine.snapshot().job_in_progress);
    assert_eq!(shop.files(), ["same.nc"], "the finished file leaves the queue");
}

#[test]
fn a_lost_link_or_a_controller_reset_stops_the_queue() {
    let mut shop = playing(SAME);
    shop.event(Event::PortClosed);
    stopped_by(&mut shop, "connection was lost");

    let mut shop = playing(SAME);
    shop.rx("PCB v1.6 TLE9201  Detected");
    stopped_by(&mut shop, "restarted");
}

const PARK: [&str; 4] = ["G21", "G90", "G00 Z5.0", "G00 X100.0 Y-50.0"];

fn parking(files: &[(&'static str, &'static str)]) -> Shop {
    let park = settings(&[("Maslow Settings", "parkX", json!("100")), ("Maslow Settings", "parkY", json!("-50"))]);
    let mut shop = Shop::with(park, files);
    shop.fit(1);
    shop.open(files[0].0);
    files.iter().for_each(|(name, _)| {
        shop.add(name);
    });
    shop
}

#[test]
fn the_queue_parks_after_its_last_file_and_never_between_files() {
    let mut shop = parking(&[A, SAME]);
    shop.act("startRun");
    let sent = shop.run();
    assert_eq!(sent[..6], ["G0 Z5", "G1 X1 F100", "G0 Z5", SAFE, "T1 M6", "G1 X2"], "no park between files");
    assert_eq!(sent[6..], PARK);
    let logged =
        |label: &str| shop.effects.iter().filter(|e| matches!(e, Effect::Log(t) if t.starts_with(label))).count();
    assert_eq!((logged("PARK START trigger=queue"), logged("PARK START trigger=file")), (1, 0));
}

#[test]
fn a_queue_stopped_at_the_prompt_or_by_a_refused_line_never_parks() {
    let mut shop = parking(&[A, OTHER]);
    shop.act("startRun");
    shop.run();
    stop(&mut shop);
    assert!(!shop.run().contains(&PARK[3].to_string()));

    let mut shop = parking(&[("probe.nc", "G1 X1|G38.4 Z-1|G1 X2")]);
    shop.act("startRun");
    assert_eq!(shop.run(), ["G1 X1"]);
}

#[test]
fn a_refused_line_stops_the_queue() {
    let mut shop = Shop::new(&[("probe.nc", "G1 X1|G38.4 Z-1|G1 X2"), SAME]);
    shop.fit(1);
    shop.open("probe.nc");
    shop.add("probe.nc");
    shop.add("same.nc");
    shop.act("startRun");
    assert_eq!(shop.run(), ["G1 X1"]);
    stopped_by(&mut shop, "Line 2 was not sent");
}

#[test]
fn a_file_that_fails_to_load_stops_the_queue_with_an_alert() {
    let mut shop = playing(SAME);
    shop.unreadable = true;
    shop.run();
    assert!(alerts(&shop.effects).iter().any(|alert| alert.contains("Could not load same.nc")));
    let queue = shop.queue();
    assert_eq!((queue["state"].as_str(), queue["reason"].as_str()), (Some("stopped"), Some("Could not load same.nc.")));
    assert_eq!(shop.files(), ["same.nc"], "the file stays queued");
}

#[test]
fn a_safe_height_of_zero_stops_the_queue_before_any_move() {
    let zero = settings(&[("Maslow Settings", "zAxisSafeHeight", json!(0))]);
    let mut shop = Shop::with(zero, &[A, SAME]);
    shop.open("a.nc");
    shop.add("a.nc");
    shop.add("same.nc");
    shop.act("startRun");
    assert_eq!(shop.run(), ["G0 Z5", "G1 X1 F100", "G0 Z5"]);
    assert!(shop.queue()["reason"].as_str().unwrap().contains("zAxisSafeHeight"));
}

#[test]
fn a_restored_queue_never_starts_on_its_own() {
    let items = vec![Item { file: "a.nc".into(), tools: Default::default(), skip_bit_changes: false }];
    let mut rig = Rig::with(settings(&[]));
    rig.machine = Machine::new("10.0.0.5:5000", settings(&[])).with_queue(items);
    rig.boot_on("51.27");
    let effects = rig.advance(2000);
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Queue(_) | Effect::Write(_))));
    let queue = serde_json::to_value(&*rig.machine.snapshot().queue).unwrap();
    assert_eq!((queue["state"].as_str(), queue["items"][0]["between"].as_str()), (Some("idle"), Some("noTool")));
}

#[test]
fn the_running_file_cannot_be_moved_or_removed_but_the_rest_can() {
    let mut shop = playing(SAME);
    let ids: Vec<u64> = shop.queue()["items"].as_array().unwrap().iter().map(|i| i["id"].as_u64().unwrap()).collect();
    let third = shop.add("a.nc");
    assert_eq!(shop.queue()["headLocked"], true);
    assert_eq!(shop.edit(Edit::Remove { id: ids[0] }).status, 409);
    assert_eq!(shop.edit(Edit::Move { id: ids[0], to: 2 }).status, 409);
    assert_eq!(shop.edit(Edit::Move { id: third, to: 0 }).status, 409);
    assert_eq!(shop.edit(Edit::Move { id: 999, to: 0 }).status, 404);
    assert_eq!(shop.edit(Edit::Move { id: third, to: 1 }).status, 200);
    assert_eq!(shop.files(), ["a.nc", "a.nc", "same.nc"]);
    assert_eq!(shop.edit(Edit::Remove { id: ids[1] }).status, 200);
    assert_eq!(shop.edit(Edit::Clear).status, 200);
    assert_eq!(shop.files(), ["a.nc"]);
    assert!(shop.effects.iter().any(
        |effect| matches!(effect, Effect::Queue(Outbound::Save(files)) if files == &[("a.nc".to_string(), false)])
    ));
}

/// Stop, then the status report that ends the controller's resync.
fn stop(shop: &mut Shop) {
    shop.act("stopRun");
    shop.run();
    shop.rig.advance(300);
    shop.rx("<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
}

const FROM_START: Edit = Edit::Continue(ContinueFrom::Start);
const FROM_STOP: Edit = Edit::Continue(ContinueFrom::Stopped);

#[test]
fn stop_at_the_prompt_keeps_the_bit_from_before_so_continue_asks_again() {
    let mut shop = playing(OTHER);
    shop.run();
    stop(&mut shop);
    assert_eq!(shop.queue()["items"][0]["between"], "toolChange");
    assert_eq!(shop.queue()["stoppedAt"], Value::Null, "stopped between files: no choice");
    assert_eq!(shop.edit(FROM_START).status, 200);
    assert_eq!(shop.queue()["state"], "running");
    // The firmware already took tool 2, so it does not pause; WebControl asks all the same.
    assert_eq!(shop.run(), [SAFE, "M5", "T2 M6"]);
    let prompts = pushed(&shop.effects, "activateModal");
    assert!(prompts.iter().any(|modal| modal["message"] == "Please insert tool 2"), "{prompts:?}");

    let mut shop = playing(SAME);
    shop.run();
    let response = shop.edit(FROM_START);
    assert_eq!((response.status, response.body["error"].as_str()), (409, Some("The queue is empty.")));
}

/// `a.nc` stopped with its second line written and not yet answered.
fn stopped_mid_file() -> Shop {
    let mut shop = playing(SAME);
    let first = shop.outbox.pop_front().unwrap();
    shop.answer(&first);
    let effects = shop.rig.advance(100);
    shop.take(effects);
    assert_eq!(shop.outbox.front().map(String::as_str), Some("G1 X1 F100"));
    shop.outbox.clear();
    stop(&mut shop);
    shop
}

#[test]
fn continue_after_a_stop_mid_file_starts_where_it_stopped_or_from_line_one() {
    let mut shop = stopped_mid_file();
    assert_eq!(shop.queue()["stoppedAt"], 1);
    shop.effects.clear();
    assert_eq!(shop.edit(FROM_STOP).status, 200);
    let sent = shop.run();
    assert!(sent.contains(&"G1 X1 F100".to_string()) && !sent.contains(&"G1 X1".to_string()), "{sent:?}");
    assert_eq!(sent[sent.len() - 4..], ["G0 Z5", SAFE, "T1 M6", "G1 X2"], "{sent:?}");
    assert_eq!((shop.loads(), shop.queue()["state"].as_str()), (1, Some("idle")));

    let mut shop = stopped_mid_file();
    assert_eq!(shop.edit(FROM_START).status, 200);
    assert_eq!(shop.queue()["stoppedAt"], Value::Null);
    assert_eq!(shop.run(), [SAFE, "G0 Z5", "G1 X1 F100", "G0 Z5", SAFE, "T1 M6", "G1 X2"]);
    let response = shop.edit(FROM_STOP);
    assert_eq!(response.status, 409, "{response:?}");
}

#[test]
fn play_and_other_loads_wait_while_the_queue_loads_its_next_file() {
    let mut shop = playing(SAME);
    shop.hold = true;
    shop.run();
    assert_eq!((shop.queue()["step"].as_str(), shop.held.len()), (Some("loading"), 1));
    let refused = shop.rig.act("startRun");
    assert!(alerts(&refused).iter().any(|alert| alert.contains("starting its next file")), "{:?}", alerts(&refused));
    let request = Ask::LoadGcode { gcode: shop.gcode("a.nc"), file: Some(path("a.nc")), origin: Origin::Open };
    let effects = shop.rig.send(Event::Request { id: 4, request });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Reply { reply: wc_machine::files::Reply::Refused(_), .. }))
    );
    shop.hold = false;
    let file = shop.held.remove(0);
    let request = Ask::LoadGcode { gcode: shop.gcode(&file), file: Some(path(&file)), origin: Origin::Queue };
    shop.event(Event::Request { id: 8, request });
    assert_eq!(shop.run(), [SAFE, "T1 M6", "G1 X2"]);
}

#[test]
fn a_finished_queue_logs_each_step() {
    let mut shop = playing(SAME);
    shop.run();
    let logged: Vec<String> = shop
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Log(line) if line.starts_with("QUEUE") => Some(line.clone()),
            _ => None,
        })
        .collect();
    let expected = ["DONE file=a.nc", "LOAD file=same.nc", "START file=same.nc", "DONE file=same.nc", "FINISHED"];
    assert_eq!(logged, expected.map(|line| format!("QUEUE {line}")));
}

#[test]
fn a_full_queue_refuses_more() {
    let mut shop = Shop::new(&[A]);
    (0..wc_machine::queue::MAX_ITEMS).for_each(|_| {
        shop.add("a.nc");
    });
    let tools = file_tools(shop.text("a.nc").lines());
    assert_eq!(shop.edit(Edit::Add(Item { file: "a.nc".into(), tools, skip_bit_changes: false })).status, 409);
}

#[test]
fn pause_and_resume_keep_the_queue_running() {
    let mut shop = playing(SAME);
    shop.act("pauseRun");
    shop.run();
    assert_eq!(shop.queue()["state"], "running");
    shop.act("resumeRun");
    let sent = shop.run();
    assert_eq!(sent[sent.len() - 3..], [SAFE, "T1 M6", "G1 X2"]);
    assert_eq!(shop.loads(), 1);
}

#[test]
fn a_line_lost_twice_stops_the_queue() {
    let mut shop = playing(SAME);
    let report = "<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>";
    (0..2).for_each(|_| {
        shop.outbox.clear();
        (0..4).for_each(|_| {
            shop.rig.advance(200);
            shop.rx(report);
            shop.rx("[PE:0.00,0.00,127,1590.41,1822.64]");
        });
    });
    stopped_by(&mut shop, "did not reach the controller");
}

#[test]
fn a_failed_or_unzeroing_touch_z_in_the_file_stops_the_queue() {
    [(false, "without zeroing"), (true, "Touch Z failed")].into_iter().for_each(|(misses, reason)| {
        let mut shop = Shop::new(&[("probe.nc", "G1 X1|G38.2 Z-10 F100|G1 X2"), SAME]);
        shop.probe_misses = misses;
        shop.fit(1);
        shop.open("probe.nc");
        shop.add("probe.nc");
        shop.add("same.nc");
        shop.act("startRun");
        stopped_after_run(&mut shop, reason);
    });
}

fn stopped_after_run(shop: &mut Shop, reason: &str) {
    shop.run();
    stopped_by(shop, reason);
}

#[test]
fn an_unknown_bit_height_stops_the_queue_before_it_moves() {
    let mut shop = Shop::unreported(settings(&[]), &[A]);
    shop.add("a.nc");
    assert_eq!(shop.edit(FROM_START).status, 200);
    let effects = shop.rig.advance(3500);
    shop.take(effects);
    assert!(rig::written(&shop.effects).is_empty(), "nothing moves");
    stopped_by(&mut shop, "height was unknown");
}

const SKIP: (&str, &str) = ("skip.nc", "T2 M6|G1 X5|T3 M6 G0 X6|G1 X7");
const THREE: (&str, &str) = ("three.nc", "T3 M6|G1 X8");

#[test]
fn the_override_skips_every_bit_change_and_still_follows_the_files_tools() {
    let mut shop = Shop::new(&[A, SKIP, THREE]);
    shop.fit(1);
    shop.open("a.nc");
    shop.add("a.nc");
    shop.add_with("skip.nc", true);
    shop.add("three.nc");
    let between: Vec<Value> = shop.queue()["items"].as_array().unwrap().iter().map(|i| i["between"].clone()).collect();
    assert_eq!(between[1..], [json!("toolChangeSkipped"), json!("sameTool")]);
    assert_eq!(shop.queue()["items"][1]["skipBitChanges"], true);
    shop.act("startRun");
    let sent = shop.run();
    let skipped = ["G1 X5", "G0 X6", "G1 X7"];
    // The next file names tool 3 as the last one did, so it starts at once; the firmware, never told, asks for it.
    assert_eq!(sent, [&["G0 Z5", "G1 X1 F100", "G0 Z5", SAFE][..], &skipped, &[SAFE, "T3 M6"]].concat());
    let logged: Vec<String> = shop
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Log(line) if line.contains("SKIPPED") => Some(line.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        logged,
        [
            "QUEUE BIT CHANGE SKIPPED between_files tool=2",
            "QUEUE BIT CHANGE SKIPPED file_line=1 tool=2",
            "QUEUE BIT CHANGE SKIPPED file_line=3 tool=3"
        ]
    );
    assert!(
        shop.effects
            .iter()
            .any(|e| matches!(e, Effect::Queue(Outbound::Save(f)) if f.get(1) == Some(&("skip.nc".into(), true))))
    );
}

#[test]
fn the_override_is_set_per_file_but_not_on_the_running_one_and_play_outside_the_queue_still_pauses() {
    let mut shop = Shop::new(&[A, SKIP]);
    shop.fit(1);
    shop.open("a.nc");
    let first = shop.add_with("a.nc", false);
    let second = shop.add_with("skip.nc", false);
    assert_eq!(shop.edit(Edit::Override { id: second, skip: true }).status, 200);
    assert_eq!(shop.edit(Edit::Override { id: 999, skip: true }).status, 404);
    shop.act("startRun");
    assert_eq!(shop.edit(Edit::Override { id: first, skip: true }).status, 409);
    stop(&mut shop);

    shop.open("skip.nc");
    shop.act("startRun");
    assert_eq!(shop.run(), ["T2 M6"]);
    assert!(shop.rig.machine.snapshot().manual_z_axis_adjust, "the file's own M6 pauses outside the queue");
}

/// `a.nc` with every line answered, the queue waiting on the next tick to see the controller idle.
fn finishing() -> Shop {
    let mut shop = playing(SAME);
    ["G0 Z5", "G1 X1 F100", "G0 Z5"].iter().for_each(|line| {
        assert_eq!(shop.outbox.pop_front().as_deref(), Some(*line));
        shop.answer(line);
        while shop.outbox.is_empty() && shop.queue()["step"] != "finishing" {
            let effects = shop.rig.advance(10);
            shop.take(effects);
        }
    });
    assert_eq!(shop.queue()["step"], "finishing", "{:?} {}", shop.outbox, shop.queue());
    shop
}

#[test]
fn pause_once_the_last_line_is_out_lets_the_file_end_but_starts_no_other() {
    let mut shop = finishing();
    shop.act("pauseRun");
    assert_eq!(shop.run(), Vec::<String>::new());
    assert_eq!(shop.files(), ["same.nc"], "the finished file leaves the queue");
    stopped_by(&mut shop, "Pause");

    let mut shop = playing(SAME);
    shop.hold = true;
    shop.run();
    shop.act("pauseRun");
    shop.hold = false;
    let file = shop.held.remove(0);
    let request = Ask::LoadGcode { gcode: shop.gcode(&file), file: Some(path(&file)), origin: Origin::Queue };
    shop.event(Event::Request { id: 8, request });
    stopped_by(&mut shop, "Pause");
    assert!(!shop.rig.machine.snapshot().job_in_progress);
}

#[test]
fn pause_while_z_rises_between_files_starts_no_file() {
    let mut shop = playing(SAME);
    ["G0 Z5", "G1 X1 F100", "G0 Z5"].iter().for_each(|line| {
        assert_eq!(shop.outbox.pop_front().as_deref(), Some(*line));
        shop.answer(line);
        let effects = shop.rig.advance(100);
        shop.take(effects);
    });
    assert_eq!(shop.outbox.front().map(String::as_str), Some(SAFE));
    shop.act("pauseRun");
    assert_eq!(shop.run(), [SAFE]);
    stopped_by(&mut shop, "Pause");
    assert!(!shop.rig.machine.snapshot().job_in_progress);
}

#[test]
fn stop_while_the_last_lines_drain_offers_the_line_it_stopped_at() {
    let buffered = settings(&[("Maslow Settings", "bufferOn", json!(1))]);
    let mut shop = Shop::with(buffered, &[A, SAME]);
    shop.fit(1);
    shop.open("a.nc");
    shop.add("a.nc");
    shop.add("same.nc");
    shop.act("startRun");
    let effects = shop.rig.advance(500);
    shop.take(effects);
    assert_eq!(shop.outbox.drain(..).skip(1).collect::<Vec<_>>(), ["G1 X1 F100", "G0 Z5"]);
    shop.rx("ok");
    assert_eq!(shop.queue()["step"], "finishing");
    stop(&mut shop);
    assert_eq!((shop.queue()["stoppedAt"].clone(), shop.files()), (json!(1), vec!["a.nc".into(), "same.nc".into()]));
    shop.open("same.nc");
    assert_eq!(shop.queue()["stoppedAt"], Value::Null, "another file is loaded");
}

/// `two.nc` changes to tool 2 in the file, though it named tool 1 when it was queued.
#[test]
fn the_bit_after_a_file_is_the_last_tool_of_the_file_as_it_ran() {
    let mut shop = Shop::new(&[("two.nc", "G1 X1|T2 M6|G1 X5"), SAME]);
    shop.fit(1);
    shop.open("two.nc");
    let stale = wc_gcode::firmware_gcode::FileTools { first: Some(1), last: Some(1) };
    assert_eq!(shop.edit(Edit::Add(Item { file: "two.nc".into(), tools: stale, skip_bit_changes: false })).status, 200);
    shop.add("same.nc");
    shop.act("startRun");
    assert_eq!(shop.run(), ["G1 X1", "T2 M6"]);
    assert_eq!(shop.queue()["items"][0]["tool"], 2, "the loaded file's tools are shown");
    shop.act("resumeRun");
    let sent = shop.run();
    assert_eq!(sent[sent.len() - 3..], [SAFE, "M5", "T1 M6"], "{sent:?}");
    assert!(shop.rig.machine.snapshot().manual_z_axis_adjust);
}

#[test]
fn continue_where_stop_left_a_file_with_the_override_restores_no_tool_change() {
    let mut shop = Shop::new(&[("skip.nc", "T2 M6|G1 X5|G1 X6|G1 X7")]);
    shop.fit(1);
    shop.add_with("skip.nc", true);
    assert_eq!(shop.edit(FROM_START).status, 200);
    let mut sent = Vec::new();
    while sent.last().map(String::as_str) != Some("G1 X5") {
        let line = shop.outbox.pop_front().expect("a line");
        shop.answer(&line);
        sent.push(line);
        let effects = shop.rig.advance(100);
        shop.take(effects);
    }
    shop.outbox.clear();
    stop(&mut shop);
    assert_eq!(shop.queue()["stoppedAt"], 2);
    assert_eq!(shop.edit(FROM_STOP).status, 200);
    let sent = shop.run();
    assert!(!sent.iter().any(|line| line.contains("M6")), "{sent:?}");
    assert!(sent.ends_with(&["G1 X6".into(), "G1 X7".into()]), "{sent:?}");
}

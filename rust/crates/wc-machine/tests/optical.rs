//! Optical calibration's scan against `wc-sim`, with a stand-in camera that sees the sled's true position against
//! a target whose squares sit off their nominal places by an error field (spec §3.4).

mod rig;

use rig::sim::SimRig;
use rig::{alerts, pushed, settings};
use serde_json::{Value, json};
use wc_config::Settings;
use wc_machine::files::{Reply, Request as FileRequest};
use wc_machine::optical::{self, ENABLED_KEY, ENABLED_SECTION, Seen, Vision};
use wc_machine::{Effect, Event, Request};
use wc_optical::grid::nominal_mm;
use wc_protocol::calibration::HoleyParameters;
use wc_protocol::optical::{Cell, GeometryFit, Measurement, OpticalSettings, Orientation, PixelPoint, SheetPose};

const PX_PER_MM: f64 = 10.0;
const UPRIGHT: Orientation = Orientation { rotation_deg: 0.0, mirrored: false, px_per_mm: PX_PER_MM };

fn with(value: Value) -> Settings {
    let mut settings = Settings::defaults();
    assert!(settings.store(ENABLED_SECTION, ENABLED_KEY, &value).is_some());
    settings
}

#[test]
fn the_setting_turns_optical_calibration_on() {
    let defaults = Settings::defaults();
    assert!(!optical::enabled(&defaults));
    assert!(optical::experimental(&defaults).is_empty());
    [json!(true), json!(1), json!("1")].into_iter().for_each(|on| {
        let settings = with(on.clone());
        assert!(optical::enabled(&settings), "{on}");
        assert_eq!(optical::experimental(&settings), ["optical"]);
    });
    [json!(false), json!(0)].into_iter().for_each(|off| assert!(!optical::enabled(&with(off.clone())), "{off}"));
}

/// What the stand-in camera reports for a square whose true centre is `square` with the sled at `sled`.
/// `n` counts the measurements; the first three are the orientation check's.
type Lens = fn(square: (f64, f64), sled: (f64, f64), n: usize) -> (f64, f64);

fn true_lens(square: (f64, f64), sled: (f64, f64), _: usize) -> (f64, f64) {
    (square.0 - sled.0, square.1 - sled.1)
}

/// The machine's error: squares sit this far from where the configured geometry puts them.
fn field((x, y): (f64, f64)) -> (f64, f64) {
    (0.8 + 0.002 * x, -0.4 + 0.001 * y)
}

struct World {
    rig: SimRig,
    field: fn((f64, f64)) -> (f64, f64),
    lens: Lens,
    orientation: Orientation,
    answers: bool,
    effects: Vec<Effect>,
    written: Vec<String>,
    requests: Vec<Vision>,
}

fn rig_with(changes: &[(&str, &str, Value)]) -> SimRig {
    let base = [("Maslow Settings", "zAxis", json!(1)), (ENABLED_SECTION, ENABLED_KEY, json!(1))];
    SimRig::with(settings(&[&base[..], changes].concat()))
}

impl World {
    fn new() -> World {
        World::on(rig_with(&[]))
    }

    fn on(rig: SimRig) -> World {
        World {
            rig,
            field,
            lens: true_lens,
            orientation: UPRIGHT,
            answers: true,
            effects: Vec::new(),
            written: Vec::new(),
            requests: Vec::new(),
        }
    }

    fn answer(&self, request: &Vision) -> Seen {
        match request {
            Vision::Measure { id, cell, .. } => {
                let (x, y) = nominal_mm(*cell);
                let (ex, ey) = (self.field)((x, y));
                let [sx, sy, _] = self.rig.sim.position();
                let n = self.requests.iter().filter(|r| matches!(r, Vision::Measure { .. })).count() - 1;
                let (dx, dy) = (self.lens)((x + ex, y + ey), (sx, sy), n);
                let centre_px = PixelPoint { x: 320.0 + dx * PX_PER_MM, y: 240.0 - dy * PX_PER_MM };
                let measurement = Measurement { dx_mm: dx, dy_mm: dy, centre_px, angle_deg: 0.0, frames_used: 10 };
                Seen::Measured { id: *id, result: Ok(measurement), png: Some("iVBORw0KGgo=".into()) }
            }
            Vision::Orient { id, .. } => Seen::Oriented { id: *id, result: Ok(self.orientation) },
        }
    }

    fn keep(&mut self) {
        let (effects, written) = self.rig.take();
        self.written.extend(written.iter().map(|line| line.trim_end().to_string()));
        let requests: Vec<Vision> = effects
            .iter()
            .filter_map(|e| match e {
                Effect::Vision(v) => Some(v.clone()),
                _ => None,
            })
            .collect();
        self.effects.extend(effects);
        self.requests.extend(requests.iter().cloned());
        if self.answers {
            requests.iter().for_each(|request| {
                let seen = self.answer(request);
                self.rig.send(Event::Seen(seen));
            });
        }
    }

    /// Lets `ms` pass, the camera answering each request as it comes.
    fn run(&mut self, ms: u64) {
        (0..ms / 10).for_each(|_| {
            self.rig.run(10);
            self.keep();
            self.keep();
        });
    }

    fn edit(&mut self, request: Request) -> (u16, Value) {
        self.rig.send(Event::Edit { id: 4, request });
        let response = self.rig.effects.iter().find_map(|effect| match effect {
            Effect::Respond { id: 4, response } => Some((response.status, response.body.clone())),
            _ => None,
        });
        self.keep();
        response.expect("answered")
    }

    fn scan(&mut self, cells: &[(i32, i32)]) -> (u16, Value) {
        self.scan_with(scan_settings(), cells)
    }

    fn scan_with(&mut self, settings: OpticalSettings, cells: &[(i32, i32)]) -> (u16, Value) {
        let cells = cells.iter().map(|&(i, j)| Cell { i, j }).collect();
        self.edit(Request::OpticalScan { settings, cells })
    }

    fn act(&mut self, command: &str) {
        self.rig.act(command);
        self.keep();
    }

    fn progress(&self) -> Vec<Value> {
        pushed(&self.effects, "opticalScanProgress")
    }

    fn last_state(&self) -> Value {
        self.progress().last().expect("progress")["state"].clone()
    }

    fn alerts(&self) -> Vec<String> {
        alerts(&self.effects)
    }

    fn view(&self) -> wc_machine::optical::View {
        (*self.rig.machine.snapshot().optical).clone()
    }

    fn moves(&self) -> Vec<String> {
        self.written.iter().filter(|line| line.starts_with("G1 ")).cloned().collect()
    }
}

fn scan_settings() -> OpticalSettings {
    OpticalSettings { settle_time: 0.2, ..optical::settings(&Settings::defaults()) }
}

#[test]
fn a_scan_moves_in_mm_finds_each_square_and_restores_the_modes() {
    let mut world = World::new();
    assert_eq!(world.scan(&[(0, 0), (1, 0)]), (200, json!({"total": 2, "scan": 1})));
    world.run(3000);
    assert_eq!(world.last_state(), "done");
    let grid = world.view().grid;
    [(0, 0), (1, 0)].iter().for_each(|&(i, j)| {
        let cell = Cell { i, j };
        let (ex, ey) = field(nominal_mm(cell));
        let (gx, gy) = grid.get(cell).expect("measured");
        assert!((gx - ex).abs() < 1e-9 && (gy - ey).abs() < 1e-9, "{cell:?}: {gx}, {gy}");
    });
    let start: Vec<&str> = world.written.iter().map(String::as_str).take(3).collect();
    assert_eq!(start, ["G21", "G90", "G0 Z5.0"]);
    assert_eq!(
        world.moves(),
        [
            "G1 X0.0 Y0.0 F500.0",
            "G1 X5.0 Y0.0 F500.0",
            "G1 X0.0 Y5.0 F500.0",
            "G1 X0.0 Y0.0 F500.0",
            "G1 X0.8 Y-0.4 F500.0",
            "G1 X77.0 Y-0.4 F500.0",
            "G1 X77.152 Y-0.4 F500.0",
        ]
    );
    assert_eq!(world.written.iter().filter(|l| *l == "B10 L").count(), 7, "one barrier per move");
    assert_eq!(world.written.last().unwrap(), "G90", "G90 restored, no G20 on a mm machine");
    assert_eq!(
        pushed(&world.effects, "opticalOrientation"),
        [json!({"rotationDeg": 0.0, "mirrored": false, "pxPerMm": 10.0})]
    );
    assert_eq!(pushed(&world.effects, "opticalImage").len(), 7);
    let view = world.view();
    assert_eq!(view.scan, None);
    let session = view.session.unwrap();
    assert_eq!(session.orientation, Some(UPRIGHT));
    assert!((session.error_x[16][7].unwrap() - field((76.2, 0.0)).0).abs() < 1e-9, "[i + 15][7 - j]");
    assert!(world.alerts().is_empty(), "{:?}", world.alerts());
}

#[test]
fn the_camera_hears_the_settings_the_orientation_and_the_scan_after_the_settle_time() {
    let mut world = World::new();
    world.answers = false;
    world.scan(&[(0, 0)]);
    world.run(1000);
    let measure = |request: &Vision| match request {
        Vision::Measure { scan, cell, target_mm, orientation, settings, .. } => {
            (*scan, *cell, *target_mm, *orientation, settings.clone())
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(world.requests.len(), 1);
    assert_eq!(measure(&world.requests[0]), (1, Cell { i: 0, j: 0 }, (0.0, 0.0), None, scan_settings()));
    assert_eq!(world.view().scan, Some(1));
    let states: Vec<Value> = world.progress().iter().map(|p| p["state"].clone()).collect();
    assert_eq!(states, ["orienting", "orienting", "orienting"]);
    // An answer to something it no longer waits for changes nothing.
    world.rig.send(Event::Seen(Seen::Oriented { id: 99, result: Ok(UPRIGHT) }));
    world.rig.send(Event::Seen(Seen::Measured { id: 99, result: Err("stale".into()), png: None }));
    world.keep();
    assert!(world.alerts().is_empty());
    world.answers = true;
    let seen = world.answer(&world.requests[0].clone());
    world.rig.send(Event::Seen(seen));
    world.run(3000);
    assert_eq!(world.last_state(), "done");
    let after = world.requests.iter().rev().find(|r| matches!(r, Vision::Measure { .. })).map(|r| measure(r).3);
    assert_eq!(after, Some(Some(UPRIGHT)), "squares are measured with the orientation the check found");
}

#[test]
fn an_inches_machine_runs_in_mm_and_goes_back_to_inches_even_after_stop() {
    let mut world = World::on(rig_with(&[("Computed Settings", "units", json!("INCHES"))]));
    world.scan(&[(0, 0), (1, 0)]);
    world.run(600);
    assert_eq!(world.rig.machine.snapshot().units(), wc_protocol::main::Units::Mm, "the G21 the scan sent");
    world.written.clear();
    world.act("stopRun");
    world.run(1000);
    assert_eq!(world.written, ["!", "G20", "G90"]);
    assert_eq!(world.last_state(), "stopped");
    assert_eq!(world.rig.machine.snapshot().units(), wc_protocol::main::Units::Inches);
    assert_eq!(world.view().scan, None);
    assert!(world.view().session.is_some(), "the partial grid stays for review");
}

#[test]
fn a_jump_trips_the_step_limit_and_skip_moves_on() {
    let mut world = World::new();
    world.field = |(x, _)| if x > 0.0 { (30.0, 0.0) } else { (0.0, 0.0) };
    world.scan(&[(0, 0), (1, 0), (2, 0)]);
    world.run(3000);
    assert_eq!(world.last_state(), "failed");
    assert!(world.alerts()[0].contains("more than one 10 mm step. Press Retry, Skip or Stop."), "{:?}", world.alerts());
    let moves = world.moves().len();
    world.act("opticalSkipCell");
    world.run(3000);
    assert_eq!(world.last_state(), "failed", "the next square jumps too");
    assert_eq!(world.moves().len(), moves + 1, "Skip went straight to the next square");
    world.act("opticalSkipCell");
    world.run(500);
    assert_eq!(world.last_state(), "done");
    assert_eq!(world.view().grid.measured().len(), 1, "skipped squares stay unmeasured");
}

#[test]
fn a_measurement_or_orientation_that_is_not_a_number_never_reaches_the_controller() {
    let mut world = World::new();
    world.lens = |square, sled, n| if n < 3 { true_lens(square, sled, n) } else { (f64::NAN, 0.0) };
    world.scan(&[(0, 0)]);
    world.run(3000);
    assert_eq!(world.alerts(), [format!("{} Press Retry, Skip or Stop.", optical::NOT_A_NUMBER)]);
    let mut turned = World::new();
    turned.orientation = Orientation { rotation_deg: f64::NAN, ..UPRIGHT };
    turned.scan(&[(0, 0)]);
    turned.run(3000);
    assert_eq!(turned.last_state(), "failed");
    let moves = [world.moves(), turned.moves()].concat();
    assert!(moves.iter().all(|line| !line.to_lowercase().contains("nan")), "{moves:?}");
}

#[test]
fn a_square_that_never_centres_fails_after_six_attempts_and_retry_starts_again() {
    let mut world = World::new();
    world.lens = |square, sled, n| if n < 3 { true_lens(square, sled, n) } else { (0.5, 0.0) };
    world.field = |_| (0.0, 0.0);
    world.scan(&[(0, 0)]);
    world.run(4000);
    assert_eq!(world.last_state(), "failed");
    assert!(world.alerts()[0].starts_with("The square was not centred within 6 attempts."));
    assert_eq!(world.moves().len() - 3, 6, "after the orientation check's three moves");
    world.act("opticalRetryCell");
    world.run(4000);
    assert_eq!(world.moves().len() - 3, 12, "Retry has six more attempts");
    assert_eq!(world.moves()[9], "G1 X0.0 Y0.0 F500.0", "from the square's first guess");
}

#[test]
fn corrections_that_add_up_past_the_limit_fail_the_square() {
    let mut world = World::new();
    world.lens = |square, sled, n| if n < 3 { true_lens(square, sled, n) } else { (6.0, 6.0) };
    world.field = |_| (0.0, 0.0);
    world.scan(&[(0, 0)]);
    world.run(4000);
    assert!(world.alerts()[0].contains("more than 25 mm"), "{:?}", world.alerts());
}

#[test]
fn the_orientation_check_refuses_a_mirrored_turned_or_mis_scaled_camera() {
    let cases: [(Orientation, Lens, &str); 3] = [
        (Orientation { mirrored: true, ..UPRIGHT }, true_lens, "mirrored"),
        (Orientation { rotation_deg: 180.0, ..UPRIGHT }, true_lens, "turned 180.0°"),
        (UPRIGHT, |square, sled, _| (0.9 * (square.0 - sled.0), 0.9 * (square.1 - sled.1)), "Check the marker size"),
    ];
    cases.into_iter().for_each(|(orientation, lens, expected)| {
        let mut world = World::new();
        (world.orientation, world.lens) = (orientation, lens);
        world.scan(&[(0, 0)]);
        world.run(3000);
        assert_eq!(world.last_state(), "failed", "{expected}");
        assert!(world.alerts()[0].contains(expected), "{:?}", world.alerts());
        assert_eq!(world.moves().len(), 3, "no square is approached");
        world.act("opticalSkipCell");
        assert!(world.alerts().last().unwrap().contains("cannot be skipped"));
        world.orientation = UPRIGHT;
        world.lens = true_lens;
        world.act("opticalRetryCell");
        world.run(3000);
        assert_eq!(world.last_state(), "done", "Retry checks the camera again");
    });
}

#[test]
fn an_alarm_or_a_lost_link_ends_the_scan() {
    let mut world = World::new();
    world.scan(&[(0, 0), (1, 0)]);
    world.run(400);
    let lines = world.rig.sim.alarm("Position Lost");
    world.rig.deliver(lines);
    world.run(1000);
    let last = world.progress().last().cloned().unwrap();
    assert_eq!(
        (last["state"].clone(), last["error"].clone()),
        (json!("stopped"), json!("Alarm: Position Lost.  Reset Chains."))
    );
    assert!(!world.written.iter().rev().take_while(|l| *l != "G90").any(|l| l.starts_with("G1")));
    let mut world = World::new();
    world.scan(&[(0, 0)]);
    world.run(300);
    world.rig.send(Event::PortClosed);
    world.keep();
    assert_eq!(world.progress().last().unwrap()["error"], "The controller's link was lost, so the scan stopped.");
}

#[test]
fn a_lost_barrier_reply_or_a_silent_camera_fails_the_square() {
    let mut world = World::new();
    world.scan(&[(0, 0)]);
    world.rig.sim.move_ms = 10_000_000;
    world.run(80_000);
    assert!(world.alerts()[0].starts_with("The controller did not report the move done."), "{:?}", world.alerts());
    // The controller finally finishes the move and is idle again.
    let mut sim = wc_sim::Sim::new(wc_sim::Build::holey_51_27());
    sim.boot(0);
    world.rig.sim = sim;
    world.rig.deliver(vec!["ok".into()]);
    world.written.clear();
    world.act("opticalRetryCell");
    world.run(3000);
    assert_eq!(world.written[..2], ["G0 Z5.0", "G1 X0.0 Y0.0 F500.0"], "the bit rises again before the travel");
    assert_eq!(world.last_state(), "done");
    let mut world = World::new();
    world.answers = false;
    world.scan(&[(0, 0)]);
    world.run(29_000);
    assert!(world.alerts().is_empty());
    world.run(2000);
    assert_eq!(world.alerts(), ["The camera did not answer. Press Retry, Skip or Stop."]);
}

#[test]
fn a_running_scan_refuses_everything_that_would_move_or_change_the_machine() {
    let mut world = World::new();
    world.answers = false;
    world.scan(&[(0, 0)]);
    world.run(500);
    ["move", "startRun", "home", "opticalClearSession"].iter().for_each(|command| world.act(command));
    assert_eq!(world.alerts(), vec![optical::RUNNING; 4]);
    world.rig.send(Event::Request { id: 2, request: FileRequest::SendGcode("G0 X100".into()) });
    let refused = world
        .rig
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Reply { reply: Reply::Refused(t), .. } if t == optical::RUNNING));
    assert!(refused, "/sendGCode");
    world.rig.send(Event::Request { id: 3, request: FileRequest::SetSettings(Vec::new()) });
    world.rig.send(Event::UpdateSetting { setting: "toInches".into(), value: json!("1") });
    let section =
        Request::UpdateSettings { section: "maslow".into(), body: json!({"values": {"motorSpacingX": "3000"}}) };
    assert_eq!(world.edit(section), (409, json!({"error": optical::RUNNING})));
    assert_eq!(world.scan(&[(0, 0)]).0, 409);
    assert_eq!(world.edit(Request::OpticalAccept), (409, json!({"error": optical::RUNNING})));
    assert_eq!(world.edit(Request::OpticalFitted(Box::new(fit()))).0, 409);
    assert_eq!(world.alerts().len(), 5, "the unit switch too");
    assert!(world.moves().iter().all(|m| !m.contains("X100")));
    world.act("opticalRetryCell");
    assert!(world.alerts().last().unwrap().starts_with("No square is waiting"));
    world.act("stopRun");
    world.act("opticalClearSession");
    assert_eq!(world.view().session, None);
}

#[test]
fn a_scan_needs_the_setting_a_connected_idle_machine_and_squares_on_the_bed() {
    let mut world = World::new();
    let bad = OpticalSettings { feed_rate: 0.0, ..scan_settings() };
    assert_eq!(world.scan_with(bad, &[(0, 0)]), (400, json!({"error": "feedRate must be a number above 0."})));
    let bad = OpticalSettings { settle_time: -1.0, ..scan_settings() };
    assert_eq!(world.scan_with(bad, &[(0, 0)]).0, 400);
    let forever = OpticalSettings { settle_time: 1e30, ..scan_settings() };
    let refused = json!({"error": format!("settleTime must be at most {} seconds.", optical::MAX_SETTLE_S)});
    assert_eq!(world.scan_with(forever, &[(0, 0)]), (400, refused), "a settle past Duration's range");
    assert_eq!(world.scan(&[(16, 0)]), (400, json!({"error": "Square (16, 0) is outside the work area."})));
    assert_eq!(world.scan(&[]), (400, json!({"error": "The extents hold no squares."})));
    let mut narrow = World::on(rig_with(&[("Maslow Settings", "bedWidth", json!(1000.0))]));
    assert_eq!(narrow.scan(&[(7, 0)]).1["error"], "Square (7, 0) is outside the work area.");
    ["high", "0", "-1"].into_iter().for_each(|height| {
        let mut unsafe_height = World::on(rig_with(&[("Maslow Settings", "zAxisSafeHeight", json!(height))]));
        let refused = format!(
            "Set the Z-axis safe height (Maslow Settings, zAxisSafeHeight) to a number above 0 before scanning, so \
             the bit rises before it moves. It holds \"{height}\"."
        );
        assert_eq!(unsafe_height.scan(&[(0, 0)]), (409, json!({"error": refused})));
    });
    let mut disconnected = World::new();
    disconnected.rig.send(Event::PortClosed);
    assert_eq!(disconnected.scan(&[(0, 0)]), (409, json!({"error": optical::NOT_CONNECTED})));
    let mut alarmed = World::new();
    let lines = alarmed.rig.sim.alarm("Position Lost");
    alarmed.rig.deliver(lines);
    assert_eq!(alarmed.scan(&[(0, 0)]).0, 409);
    let mut busy = World::new();
    busy.rig.machine.load_gcode("G21\nG1 X10 F100\nG1 X20\n");
    busy.rig.sim.move_ms = 60_000;
    busy.act("startRun");
    busy.run(300);
    assert_eq!(busy.scan(&[(0, 0)]), (409, json!({"error": optical::JOB_RUNNING})));
    let mut off = World::on(SimRig::new());
    off.act("opticalRetryCell");
    assert_eq!(off.alerts(), [optical::OFF]);
}

#[test]
fn one_square_keeps_the_rest_of_the_session() {
    let mut world = World::new();
    world.scan(&[(0, 0), (1, 0)]);
    world.run(3000);
    world.scan(&[(2, 0)]);
    world.run(3000);
    assert_eq!(world.view().grid.measured().len(), 3);
    world.scan(&[(0, 0), (1, 0)]);
    world.run(3000);
    assert_eq!(world.view().grid.measured().len(), 2, "a scan of the extents starts a new grid");
}

fn fit() -> GeometryFit {
    let current = HoleyParameters {
        motor_yoffset: 463.0,
        distance_between_motors: 2978.4,
        left_chain_tolerance: 0.0,
        right_chain_tolerance: 0.0,
    };
    let fitted =
        HoleyParameters { motor_yoffset: 458.0, distance_between_motors: 2981.4, left_chain_tolerance: 0.1, ..current };
    GeometryFit { current, fitted, sheet: SheetPose { x_mm: 0.0, y_mm: 0.0, rotation_deg: 0.0 }, rms_mm: 0.2, cells: 9 }
}

#[test]
fn accept_writes_the_fitted_geometry_through_the_exact_setting_lines_once() {
    let mut world = World::new();
    let nothing = (409, json!({"error": optical::NOTHING_TO_ACCEPT}));
    assert_eq!(world.edit(Request::OpticalAccept), nothing);
    let (status, answered) = world.edit(Request::OpticalFitted(Box::new(fit())));
    assert_eq!((status, answered["fitted"]["motorYoffset"].clone()), (200, json!(458.0)));
    world.run(200);
    assert!(world.written.is_empty(), "a fit writes nothing");
    assert_eq!(world.view().fit, Some(fit()));
    let (status, accepted) = world.edit(Request::OpticalAccept);
    assert_eq!((status, accepted["distanceBetweenMotors"].clone()), (200, json!(2981.4)));
    world.run(500);
    assert_eq!(world.written, ["$3=458", "$2=2981.3998", "$40=0.1"], "the f32 the firmware stores, exactly");
    assert_eq!(world.rig.machine.snapshot().settings.text("Maslow Settings", "motorOffsetY").unwrap(), "458.0");
    assert_eq!(world.edit(Request::OpticalAccept), nothing, "Accept forgets the fit");
    let current = fit().fitted;
    let fitted = HoleyParameters { motor_yoffset: f64::NAN, ..current.clone() };
    let not_finite = GeometryFit { current, fitted, ..fit() };
    world.edit(Request::OpticalFitted(Box::new(not_finite)));
    assert_eq!(world.edit(Request::OpticalAccept).0, 409);
}

#[test]
fn accept_refuses_a_fit_made_against_other_geometry() {
    let mut world = World::new();
    world.edit(Request::OpticalFitted(Box::new(fit())));
    let section =
        Request::UpdateSettings { section: "maslow".into(), body: json!({"values": {"motorOffsetY": "470"}}) };
    assert_eq!(world.edit(section).0, 200);
    world.run(500);
    world.written.clear();
    assert_eq!(world.edit(Request::OpticalAccept), (409, json!({"error": optical::GEOMETRY_CHANGED})));
    world.run(500);
    assert!(world.written.is_empty(), "{:?}", world.written);
}

#[test]
fn the_fit_needs_measured_squares() {
    let settings = Settings::defaults();
    assert_eq!(optical::fit(&settings, &[]), Err((409, optical::NOTHING_MEASURED.into())));
    let one = [(Cell { i: 0, j: 0 }, (0.1, 0.2))];
    assert!(matches!(optical::fit(&settings, &one), Err((409 | 422, _))), "one square fits nothing");
}

#[test]
fn settings_are_saved_as_numbers_and_refused_when_invalid_or_during_a_job() {
    let mut world = World::new();
    let saved = OpticalSettings { feed_rate: 750.0, tl_x: -3, calibration_extents: "Custom".into(), ..scan_settings() };
    let (status, body) = world.edit(Request::OpticalSettings(saved.clone()));
    assert_eq!((status, body["settings"]["feedRate"].clone()), (200, json!(750.0)));
    assert_eq!(optical::settings(&world.rig.machine.snapshot().settings), saved);
    let bad = OpticalSettings { marker_x: f64::INFINITY, ..saved.clone() };
    assert_eq!(world.edit(Request::OpticalSettings(bad)), (400, json!({"error": "markerX must be a number above 0."})));
    let bad = OpticalSettings { canny_low_value: f64::NAN, ..saved.clone() };
    assert_eq!(world.edit(Request::OpticalSettings(bad)).0, 400);
    let slow = OpticalSettings { gaussian_blur_value: 1_000_000, ..saved.clone() };
    let too_wide = json!({"error": format!("gaussianBlurValue must be 0 to {}.", optical::MAX_BLUR)});
    assert_eq!(world.edit(Request::OpticalSettings(slow)), (400, too_wide));
    world.rig.machine.load_gcode("G21\nG1 X10 F100\nG1 X20\n");
    world.rig.sim.move_ms = 60_000;
    world.act("startRun");
    world.run(300);
    assert_eq!(world.edit(Request::OpticalSettings(saved)).0, 409);
}

#[test]
fn an_idle_machine_keeps_one_view_of_optical_calibration() {
    let mut world = World::new();
    world.run(200);
    let before = world.rig.machine.snapshot().optical;
    world.run(500);
    assert!(std::sync::Arc::ptr_eq(&before.0, &world.rig.machine.snapshot().optical.0), "unchanged, not rebuilt");
    world.scan(&[(0, 0)]);
    assert_ne!(world.view().scan, None, "a change shows at once");
}

#[test]
fn stop_z_ends_the_scan_too() {
    let mut world = World::new();
    world.scan(&[(0, 0)]);
    world.run(300);
    world.act("stopZ");
    assert_eq!(world.last_state(), "stopped");
}

#[test]
fn on_a_machine_without_a_z_motor_resume_confirms_the_raise_and_the_scan_goes_on() {
    let mut world = World::on(SimRig::with(settings(&[(ENABLED_SECTION, ENABLED_KEY, json!(1))])));
    world.scan(&[(0, 0)]);
    world.run(1000);
    assert!(world.rig.machine.snapshot().manual_z_axis_adjust, "the raise waits for Z by hand");
    assert!(world.moves().is_empty());
    world.act("resumeRun");
    world.run(3000);
    assert_eq!(world.last_state(), "done");
}

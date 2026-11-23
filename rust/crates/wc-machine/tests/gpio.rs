//! GPIO, ported from test_gpio on a fake pin backend: the pins `GPIO Settings` names, debounced buttons that send the
//! UI's actions through the machine's gates, status LEDs that follow the job, and the status LED's blink patterns.

mod rig;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig::{Rig, alerts, settings, written};
use serde_json::json;
use wc_machine::gpio::{Button, Clock, Debounce, Gpio, Led, LedPin, Leds, NoPins, Pins, Plan, Status};
use wc_machine::{Event, Machine, Snapshot};

#[test]
fn the_plan_reads_each_pins_job_from_the_settings() {
    let plan = Plan::from_settings(&settings(&[
        ("GPIO Settings", "gpio17", json!("Stop")),
        ("GPIO Settings", "gpio27", json!("Return to Center")),
        ("GPIO Settings", "gpio22", json!("PlayLED")),
        ("GPIO Settings", "gpio5", json!("Spindle On")),
        ("GPIO Settings", "gpio6", json!("Bogus")),
        ("GPIO Settings", "gpio23", json!("StatusLED")),
        ("GPIO Settings", "gpio24", json!("StatusLED Active Low")),
        ("GPIO Settings", "gpio25", json!("statusled")),
    ]));
    assert_eq!(plan.buttons, [(17, Button::Stop), (27, Button::ReturnToCenter)]);
    assert_eq!(plan.leds, [(22, Led::Play)]);
    assert_eq!(plan.status, [(23, false), (24, true)]);
    let unsupported = [(5, "Spindle On"), (6, "Bogus"), (25, "statusled")].map(|(pin, text)| (pin, text.to_string()));
    assert_eq!(plan.unsupported, unsupported);
    assert_eq!(Plan::from_settings(&settings(&[])), Plan::default(), "every pin is blank by default");
}

#[test]
fn each_button_sends_the_action_the_ui_would() {
    let buttons = [
        ("Stop", "stopRun"),
        ("Pause", "pauseRun"),
        ("Play", "startRun"),
        ("Set Home", "defineHome"),
        ("Go Home", "home"),
        ("Return to Center", "returnToCenter"),
        ("Shutdown", "shutdown"),
    ];
    buttons.iter().for_each(|(setting, command)| {
        assert_eq!(Button::from_setting(setting).unwrap().command(false), *command, "{setting}");
    });
    assert_eq!(Button::Play.command(true), "resumeRun");
    assert_eq!(Button::from_setting("PlayLED"), None);
    assert_eq!(Led::from_setting("Stop"), None);
}

#[test]
fn lighting_one_status_led_turns_the_others_off() {
    let mut leds = Leds { play: true, pause: true, stop: false };
    leds.light(Led::Stop);
    assert_eq!(leds, Leds { play: false, pause: false, stop: true });
    leds.set(Led::Pause, true);
    assert!(leds.is_on(Led::Pause) && leds.is_on(Led::Stop) && !leds.is_on(Led::Play));
}

#[test]
fn a_press_counts_once_per_bounce_window() {
    let mut debounce = Debounce::default();
    let presses: Vec<bool> =
        [0, 40, 99, 100, 150, 400].iter().map(|&ms| debounce.press(Duration::from_millis(ms))).collect();
    assert_eq!(presses, [true, false, false, true, false, true]);
}

type Callbacks = Arc<Mutex<Vec<(u8, Box<dyn FnMut() + Send>)>>>;

#[derive(Default, Clone)]
struct FakePins {
    buttons: Callbacks,
    levels: Arc<Mutex<Vec<(u8, bool)>>>,
    broken: Vec<u8>,
}

struct FakeLed {
    pin: u8,
    levels: Arc<Mutex<Vec<(u8, bool)>>>,
}

impl LedPin for FakeLed {
    fn set(&mut self, on: bool) {
        self.levels.lock().unwrap().push((self.pin, on));
    }
}

impl Pins for FakePins {
    fn button(&mut self, pin: u8, pressed: Box<dyn FnMut() + Send>) -> Result<(), String> {
        if self.broken.contains(&pin) {
            return Err("busy".into());
        }
        self.buttons.lock().unwrap().push((pin, pressed));
        Ok(())
    }
    fn led(&mut self, pin: u8) -> Result<Box<dyn LedPin>, String> {
        if self.broken.contains(&pin) {
            return Err("busy".into());
        }
        Ok(Box::new(FakeLed { pin, levels: self.levels.clone() }))
    }
}

impl FakePins {
    fn press(&self, pin: u8) {
        let mut buttons = self.buttons.lock().unwrap();
        buttons.iter_mut().filter(|(p, _)| *p == pin).for_each(|(_, pressed)| pressed());
    }
}

fn clock() -> (Arc<AtomicU64>, Clock) {
    let millis = Arc::new(AtomicU64::new(1000));
    let reading = millis.clone();
    (millis, Arc::new(move || Duration::from_millis(reading.load(Ordering::SeqCst))))
}

#[test]
fn configured_buttons_report_debounced_presses_and_leds_show_the_machines_state() {
    let plan = Plan {
        buttons: vec![(17, Button::Stop), (18, Button::Play), (19, Button::Pause)],
        leds: vec![(22, Led::Play), (23, Led::Stop), (24, Led::Pause)],
        unsupported: vec![(5, "Spindle On".into())],
        ..Plan::default()
    };
    let pins = FakePins { broken: vec![19, 24], ..FakePins::default() };
    let pressed = Arc::new(Mutex::new(Vec::new()));
    let sink = pressed.clone();
    let (millis, clock) = clock();
    let mut gpio = Gpio::start(&plan, Box::new(pins.clone()), Arc::new(move |b| sink.lock().unwrap().push(b)), clock);
    pins.press(17);
    pins.press(17);
    millis.fetch_add(150, Ordering::SeqCst);
    pins.press(17);
    pins.press(18);
    assert_eq!(*pressed.lock().unwrap(), [Button::Stop, Button::Stop, Button::Play]);
    gpio.show(Leds { play: true, pause: false, stop: false });
    assert_eq!(*pins.levels.lock().unwrap(), [(22, true), (23, false)], "pin 24 would not open");
}

#[test]
fn without_gpio_every_pin_is_accepted_and_does_nothing() {
    let plan = Plan { buttons: vec![(17, Button::Stop)], leds: vec![(22, Led::Play)], ..Plan::default() };
    let (_, clock) = clock();
    let mut gpio = Gpio::start(&plan, Box::new(NoPins), Arc::new(|_| panic!("no presses without pins")), clock);
    gpio.show(Leds { play: true, ..Leds::default() });
}

#[test]
fn a_button_goes_through_the_same_gates_as_the_ui() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    assert_eq!(written(&rig.send(Event::Button(Button::Play))), ["G0 X1"]);
    let refused = rig.send(Event::Button(Button::GoHome));
    assert_eq!(alerts(&refused), ["Cannot issue command while sending gcode."]);
    rig.send(Event::Button(Button::Pause));
    assert_eq!(rig.machine.snapshot().upload_flag, 2);
    assert_eq!(written(&rig.send(Event::Button(Button::Play)))[0], "~", "Play resumes a paused job");
    assert_eq!(written(&rig.send(Event::Button(Button::Stop))), ["!"]);
    assert!(!rig.machine.snapshot().job_in_progress);
}

#[test]
fn set_home_takes_the_sled_and_play_resumes_a_waiting_tool_change() {
    let mut rig = Rig::connected();
    rig.rx("<Idle,MPos:12.30,4.50,0.00,WPos:0.000,0.000,0.000>");
    rig.send(Event::Button(Button::SetHome));
    assert_eq!(rig.machine.snapshot().settings.text("Advanced Settings", "homeX").as_deref(), Some("12.3"));
    rig.send(Event::Action(wc_protocol::Action::new("macro1", json!(""), json!(""))));
    rig.rx("Tool Change: Please insert tool 2");
    rig.rx("Maslow Paused");
    assert_eq!(written(&rig.send(Event::Button(Button::Play))), ["~"]);
    assert!(!rig.machine.snapshot().manual_z_axis_adjust);
}

/// The LED's level at each of `ms` into the state.
fn levels(status: Status, ms: &[u64]) -> Vec<bool> {
    ms.iter().map(|&ms| status.level(Duration::from_millis(ms)).0).collect()
}

#[test]
fn each_status_blinks_its_own_pattern() {
    assert_eq!(Status::Idle.level(Duration::from_secs(9)), (true, None), "idle is steady on");
    assert_eq!(levels(Status::Running, &[0, 999, 1000, 1999, 2000]), [true, true, false, false, true]);
    assert_eq!(levels(Status::Paused, &[0, 249, 250, 499, 500]), [true, true, false, false, true]);
    assert_eq!(levels(Status::Alarm, &[0, 49, 50, 99, 100]), [true, true, false, false, true]);
    let double = levels(Status::Disconnected, &[0, 150, 300, 450, 1499, 1500]);
    assert_eq!(double, [true, false, true, false, false, true], "two flashes, then a long gap");
    assert_eq!(Status::Disconnected.level(Duration::from_millis(1600)), (true, Some(Duration::from_millis(50))));
    assert_eq!(Status::Running.level(Duration::from_millis(1250)), (false, Some(Duration::from_millis(750))));
}

#[test]
fn the_status_follows_the_machine_most_urgent_first() {
    let rig = Rig::new();
    assert_eq!(rig.machine.snapshot().led_status, Status::Disconnected);
    let mut rig = Rig::connected();
    assert_eq!(rig.machine.snapshot().led_status, Status::Idle);
    rig.machine.load_gcode("G0 X1\nM0\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    assert_eq!(rig.machine.snapshot().led_status, Status::Running);
    rig.run_to_quiet();
    rig.rx("Maslow Paused");
    assert_eq!(rig.machine.snapshot().led_status, Status::Paused, "an M0");
    rig.act("resumeRun");
    assert_eq!(rig.machine.snapshot().led_status, Status::Running);
    rig.act("pauseRun");
    assert_eq!(rig.machine.snapshot().led_status, Status::Paused);
    rig.rx("ALARM: The sled is not keeping up with its expected position and has halted.");
    assert_eq!(rig.machine.snapshot().led_status, Status::Alarm, "an alarm outranks the pause");
    rig.act("stopRun");
    rig.rx("Tool Change: Please insert tool 2");
    assert_eq!(rig.machine.snapshot().led_status, Status::Paused, "a tool change");
    rig.send(Event::PortClosed);
    assert_eq!(rig.machine.snapshot().led_status, Status::Disconnected, "losing the link outranks the wait");
}

/// Each change of a pin's level, with the paused tokio clock's milliseconds.
type Timeline = Arc<Mutex<Vec<(u64, u8, bool)>>>;

struct TimedLed {
    pin: u8,
    start: tokio::time::Instant,
    timeline: Timeline,
}

impl LedPin for TimedLed {
    fn set(&mut self, high: bool) {
        let ms = u64::try_from(self.start.elapsed().as_millis()).unwrap();
        self.timeline.lock().unwrap().push((ms, self.pin, high));
    }
}

struct TimedPins(tokio::time::Instant, Timeline);

impl Pins for TimedPins {
    fn button(&mut self, _: u8, _: Box<dyn FnMut() + Send>) -> Result<(), String> {
        Ok(())
    }
    fn led(&mut self, pin: u8) -> Result<Box<dyn LedPin>, String> {
        if pin == 27 {
            return Err("busy".into());
        }
        Ok(Box::new(TimedLed { pin, start: self.0, timeline: self.1.clone() }))
    }
}

fn snapshot(status: Status) -> Snapshot {
    Snapshot { led_status: status, ..Machine::new("test", settings(&[])).snapshot() }
}

/// Pin 25 active high, 26 active low, and 27 that will not open.
fn status_leds(status: Status) -> (Gpio, Timeline, tokio::sync::watch::Sender<Snapshot>) {
    let timeline = Timeline::default();
    let pins = TimedPins(tokio::time::Instant::now(), timeline.clone());
    let plan = Plan { status: vec![(25, false), (26, true), (27, false)], ..Plan::default() };
    let (_, clock) = clock();
    let mut gpio = Gpio::start(&plan, Box::new(pins), Arc::new(|_| {}), clock);
    let (sender, snapshots) = tokio::sync::watch::channel(snapshot(status));
    gpio.follow(snapshots);
    (gpio, timeline, sender)
}

/// When the LED lit and went dark, checking the active-low pin always reads the opposite.
fn lit(timeline: &Timeline) -> Vec<(u64, bool)> {
    let changes = timeline.lock().unwrap().clone();
    let pin = |wanted: u8| changes.iter().filter(move |(_, pin, _)| *pin == wanted).map(|&(ms, _, high)| (ms, high));
    let inverted: Vec<_> = pin(26).map(|(ms, high)| (ms, !high)).collect();
    assert_eq!(pin(25).collect::<Vec<_>>(), inverted);
    inverted
}

#[tokio::test(start_paused = true)]
async fn one_task_blinks_every_status_led_and_restarts_the_pattern_on_a_new_status() {
    let (gpio, timeline, sender) = status_leds(Status::Paused);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(lit(&timeline), [(0, false), (0, true), (250, false), (500, true), (750, false), (1000, true)]);
    sender.send_modify(|snapshot| snapshot.sled = (1.0, 2.0));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(lit(&timeline)[6..], [(1250, false)], "an unchanged status keeps its rhythm");
    sender.send_modify(|snapshot| snapshot.led_status = Status::Idle);
    tokio::time::sleep(Duration::from_millis(5000)).await;
    assert_eq!(lit(&timeline)[7..], [(1300, true)]);
    sender.send_modify(|snapshot| snapshot.led_status = Status::Running);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(lit(&timeline)[8..], [(7300, false)], "running starts lit, which it already was");
    drop(gpio);
    assert_eq!(lit(&timeline)[9..], [], "already dark");
    sender.send_modify(|snapshot| snapshot.led_status = Status::Alarm);
    tokio::time::sleep(Duration::from_millis(5000)).await;
    assert_eq!(lit(&timeline).len(), 9, "and it stays dark once shut down");
}

#[tokio::test(start_paused = true)]
async fn shutting_down_turns_a_lit_status_led_off() {
    let (gpio, timeline, _sender) = status_leds(Status::Idle);
    tokio::time::sleep(Duration::from_millis(10)).await;
    drop(gpio);
    assert_eq!(lit(&timeline), [(0, false), (0, true), (10, false)]);
}

#[tokio::test(start_paused = true)]
async fn the_status_led_goes_dark_when_the_machine_ends() {
    let (_gpio, timeline, sender) = status_leds(Status::Idle);
    tokio::time::sleep(Duration::from_millis(10)).await;
    drop(sender);
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(lit(&timeline), [(0, false), (0, true), (10, false)]);
}

#[test]
fn without_a_status_led_nothing_blinks() {
    let (_, clock) = clock();
    let mut gpio = Gpio::start(&Plan::default(), Box::new(NoPins), Arc::new(|_| {}), clock);
    gpio.follow(tokio::sync::watch::channel(snapshot(Status::Alarm)).1);
}

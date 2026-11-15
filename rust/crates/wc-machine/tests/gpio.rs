//! GPIO, ported from test_gpio on a fake pin backend: the pins `GPIO Settings` names, debounced buttons that send the
//! UI's actions through the machine's gates, and status LEDs that follow the job.

mod rig;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig::{Rig, alerts, settings, written};
use serde_json::json;
use wc_machine::Event;
use wc_machine::gpio::{Button, Clock, Debounce, Gpio, Led, LedPin, Leds, NoPins, Pins, Plan};

#[test]
fn the_plan_reads_each_pins_job_from_the_settings() {
    let plan = Plan::from_settings(&settings(&[
        ("GPIO Settings", "gpio17", json!("Stop")),
        ("GPIO Settings", "gpio27", json!("Return to Center")),
        ("GPIO Settings", "gpio22", json!("PlayLED")),
        ("GPIO Settings", "gpio5", json!("Spindle On")),
        ("GPIO Settings", "gpio6", json!("Bogus")),
    ]));
    assert_eq!(plan.buttons, [(17, Button::Stop), (27, Button::ReturnToCenter)]);
    assert_eq!(plan.leds, [(22, Led::Play)]);
    assert_eq!(plan.unsupported, [(5, "Spindle On".to_string()), (6, "Bogus".to_string())]);
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
    let plan = Plan { buttons: vec![(17, Button::Stop)], leds: vec![(22, Led::Play)], unsupported: Vec::new() };
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

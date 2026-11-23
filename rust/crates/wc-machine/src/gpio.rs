//! GPIO buttons and status LEDs (`Actions/gpioActions.py`). `GPIO Settings` names each pin's job; a press becomes
//! the action the UI would send and goes through the machine's gates, and the LEDs follow Play, Pause and Stop. A
//! status LED blinks a pattern for the machine's state.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use wc_config::Settings;

use crate::job::Flow;
use crate::machine::{Link, Machine, Snapshot};

/// gpiozero's `bounce_time`: edges this soon after an accepted press are contact bounce.
pub const BOUNCE: Duration = Duration::from_millis(100);
const FIRST_PIN: u8 = 2;
const LAST_PIN: u8 = 27;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Stop,
    Pause,
    Play,
    SetHome,
    GoHome,
    ReturnToCenter,
    Shutdown,
}

impl Button {
    pub fn from_setting(text: &str) -> Option<Button> {
        let button = match text {
            "Stop" => Button::Stop,
            "Pause" => Button::Pause,
            "Play" => Button::Play,
            "Set Home" => Button::SetHome,
            "Go Home" => Button::GoHome,
            "Return to Center" => Button::ReturnToCenter,
            "Shutdown" => Button::Shutdown,
            _ => return None,
        };
        Some(button)
    }

    /// The UI action a press sends; Play on a paused job or a waiting tool change is Resume.
    pub fn command(self, play_resumes: bool) -> &'static str {
        match self {
            Button::Stop => "stopRun",
            Button::Pause => "pauseRun",
            Button::Play if play_resumes => "resumeRun",
            Button::Play => "startRun",
            Button::SetHome => "defineHome",
            Button::GoHome => "home",
            Button::ReturnToCenter => "returnToCenter",
            Button::Shutdown => "shutdown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Led {
    Play,
    Pause,
    Stop,
}

impl Led {
    pub fn from_setting(text: &str) -> Option<Led> {
        match text {
            "PlayLED" => Some(Led::Play),
            "PauseLED" => Some(Led::Pause),
            "StopLED" => Some(Led::Stop),
            _ => None,
        }
    }
}

/// The three status LEDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Leds {
    pub play: bool,
    pub pause: bool,
    pub stop: bool,
}

impl Leds {
    pub fn is_on(&self, led: Led) -> bool {
        match led {
            Led::Play => self.play,
            Led::Pause => self.pause,
            Led::Stop => self.stop,
        }
    }

    pub fn set(&mut self, led: Led, on: bool) {
        match led {
            Led::Play => self.play = on,
            Led::Pause => self.pause = on,
            Led::Stop => self.stop = on,
        }
    }

    /// `causeAction(led, "on")`: lighting one status LED turns the other two off.
    pub fn light(&mut self, led: Led) {
        *self = Leds::default();
        self.set(led, true);
    }
}

/// What the status LED shows, most urgent first: when several hold, the first wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// An alarm hold, including the firmware's position errors.
    Alarm,
    Disconnected,
    /// Pause, an M0, a tool change or a manual Z prompt.
    Paused,
    Running,
    Idle,
}

impl Status {
    /// Alternating on and off times in ms, from the moment the state began; none is steady on.
    fn blinks(self) -> &'static [u64] {
        match self {
            Status::Alarm => &[50, 50],
            Status::Disconnected => &[150, 150, 150, 1050],
            Status::Paused => &[250, 250],
            Status::Running => &[1000, 1000],
            Status::Idle => &[],
        }
    }

    /// Whether the LED is lit `elapsed` into this state, and for how much longer.
    pub fn level(self, elapsed: Duration) -> (bool, Option<Duration>) {
        let blinks = self.blinks();
        let period: u64 = blinks.iter().sum();
        if period == 0 {
            return (true, None);
        }
        let at = u64::try_from(elapsed.as_millis() % u128::from(period)).unwrap_or_default();
        let ends = blinks.iter().scan(0, |end, ms| {
            *end += ms;
            Some(*end)
        });
        let (index, end) = ends.enumerate().find(|&(_, end)| at < end).unwrap_or((0, period));
        (index % 2 == 0, Some(Duration::from_millis(end - at)))
    }

    /// `StatusLED` drives the pin high to light it, `StatusLED Active Low` low.
    pub fn active_low(text: &str) -> Option<bool> {
        match text {
            "StatusLED" => Some(false),
            "StatusLED Active Low" => Some(true),
            _ => None,
        }
    }
}

impl Machine {
    pub(crate) fn led_status(&self) -> Status {
        let job = &self.job;
        match () {
            _ if self.alarm_held() => Status::Alarm,
            _ if !matches!(self.link, Link::Connected { .. }) => Status::Disconnected,
            _ if job.firmware_paused || job.manual_z.is_some() || matches!(job.flow, Flow::Paused | Flow::Held) => {
                Status::Paused
            }
            _ if job.flow.in_job() || self.queue_busy() => Status::Running,
            _ => Status::Idle,
        }
    }
}

/// What `GPIO Settings` asks of each pin.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub buttons: Vec<(u8, Button)>,
    pub leds: Vec<(u8, Led)>,
    /// Status LED pins, each with whether it is active low.
    pub status: Vec<(u8, bool)>,
    /// Pins set to something that is neither, such as `Spindle On`, which Python leaves unconfigured too.
    pub unsupported: Vec<(u8, String)>,
}

impl Plan {
    pub fn from_settings(settings: &Settings) -> Plan {
        let mut plan = Plan::default();
        (FIRST_PIN..=LAST_PIN).for_each(|pin| {
            let value = settings.text_or_empty("GPIO Settings", &format!("gpio{pin}"));
            match (Button::from_setting(&value), Led::from_setting(&value), Status::active_low(&value)) {
                _ if value.is_empty() => {}
                (Some(button), _, _) => plan.buttons.push((pin, button)),
                (_, Some(led), _) => plan.leds.push((pin, led)),
                (_, _, Some(low)) => plan.status.push((pin, low)),
                _ => plan.unsupported.push((pin, value)),
            }
        });
        plan
    }
}

/// Accepts a press only once `BOUNCE` has passed since the last one it accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Debounce {
    last: Option<Duration>,
}

impl Debounce {
    pub fn press(&mut self, now: Duration) -> bool {
        let accepted = self.last.is_none_or(|last| now.saturating_sub(last) >= BOUNCE);
        if accepted {
            self.last = Some(now);
        }
        accepted
    }
}

pub trait LedPin: Send {
    fn set(&mut self, on: bool);
}

/// The Pi's pins, or a stand-in.
pub trait Pins: Send {
    /// Calls `pressed` on every falling edge of `pin`, an input pulled up and shorted to ground by the button.
    fn button(&mut self, pin: u8, pressed: Box<dyn FnMut() + Send>) -> Result<(), String>;
    fn led(&mut self, pin: u8) -> Result<Box<dyn LedPin>, String>;
}

/// No GPIO on this computer: every pin is accepted and does nothing.
pub struct NoPins;

struct NoLed;

impl LedPin for NoLed {
    fn set(&mut self, _: bool) {}
}

impl Pins for NoPins {
    fn button(&mut self, _: u8, _: Box<dyn FnMut() + Send>) -> Result<(), String> {
        Ok(())
    }
    fn led(&mut self, _: u8) -> Result<Box<dyn LedPin>, String> {
        Ok(Box::new(NoLed))
    }
}

/// The status LED pins; once stopped they stay dark.
#[derive(Default)]
struct StatusPins {
    pins: Vec<(Box<dyn LedPin>, bool)>,
    lit: Option<bool>,
    stopped: bool,
}

impl StatusPins {
    fn light(&mut self, on: bool) {
        if self.stopped || self.lit == Some(on) {
            return;
        }
        self.lit = Some(on);
        self.pins.iter_mut().for_each(|(pin, low)| pin.set(on != *low));
    }

    fn stop(&mut self) {
        self.light(false);
        self.stopped = true;
    }
}

/// Blinks the status LED for the machine's state until the machine ends; each new state starts its pattern afresh.
async fn blink(pins: Arc<Mutex<StatusPins>>, mut snapshots: watch::Receiver<Snapshot>) {
    let mut showing: Option<(Status, Instant)> = None;
    loop {
        let status = snapshots.borrow_and_update().led_status;
        let since = match showing {
            Some((shown, since)) if shown == status => since,
            _ => Instant::now(),
        };
        showing = Some((status, since));
        let (on, left) = status.level(since.elapsed());
        pins.lock().expect("status LED lock").light(on);
        let next = async {
            match left {
                Some(left) => tokio::time::sleep(left).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            changed = snapshots.changed() => if changed.is_err() { break },
            () = next => {}
        }
    }
    pins.lock().expect("status LED lock").stop();
}

/// The configured pins, kept open for as long as this lives.
pub struct Gpio {
    _pins: Box<dyn Pins>,
    leds: Vec<(Led, Box<dyn LedPin>)>,
    status: Arc<Mutex<StatusPins>>,
    blinker: Option<JoinHandle<()>>,
}

pub type Clock = Arc<dyn Fn() -> Duration + Send + Sync>;

impl Gpio {
    /// Sets up every pin the plan names; each debounced press calls `press`.
    pub fn start(plan: &Plan, mut pins: Box<dyn Pins>, press: Arc<dyn Fn(Button) + Send + Sync>, clock: Clock) -> Gpio {
        plan.unsupported.iter().for_each(|(pin, action)| {
            tracing::warn!(pin, action, "GPIO pin: this action is not supported, pin left unconfigured");
        });
        plan.buttons.iter().for_each(|&(pin, button)| {
            let (press, clock, debounce) = (press.clone(), clock.clone(), Mutex::new(Debounce::default()));
            let pressed = Box::new(move || {
                if debounce.lock().is_ok_and(|mut debounce| debounce.press(clock())) {
                    press(button);
                }
            });
            if let Err(error) = pins.button(pin, pressed) {
                tracing::warn!(pin, ?button, error, "could not set up the GPIO button");
            }
        });
        let leds = plan
            .leds
            .iter()
            .filter_map(|&(pin, led)| {
                let opened = pins.led(pin).inspect_err(|error| tracing::warn!(pin, ?led, error, "no GPIO LED"));
                opened.ok().map(|output| (led, output))
            })
            .collect();
        let status = plan
            .status
            .iter()
            .filter_map(|&(pin, low)| {
                let opened = pins.led(pin).inspect_err(|error| tracing::warn!(pin, error, "no GPIO status LED"));
                opened.ok().map(|output| (output, low))
            })
            .collect();
        let status = Arc::new(Mutex::new(StatusPins { pins: status, ..StatusPins::default() }));
        status.lock().expect("status LED lock").light(false);
        Gpio { _pins: pins, leds, status, blinker: None }
    }

    /// Starts blinking the status LED, if there is one, on the current tokio runtime.
    pub fn follow(&mut self, snapshots: watch::Receiver<Snapshot>) {
        if !self.status.lock().expect("status LED lock").pins.is_empty() {
            self.blinker = Some(tokio::spawn(blink(self.status.clone(), snapshots)));
        }
    }

    pub fn show(&mut self, leds: Leds) {
        self.leds.iter_mut().for_each(|(led, output)| output.set(leds.is_on(*led)));
    }
}

impl Drop for Gpio {
    fn drop(&mut self) {
        self.blinker.take().into_iter().for_each(|blinker| blinker.abort());
        self.status.lock().unwrap_or_else(std::sync::PoisonError::into_inner).stop();
    }
}

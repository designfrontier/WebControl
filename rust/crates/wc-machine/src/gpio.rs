//! GPIO buttons and status LEDs (`Actions/gpioActions.py`). `GPIO Settings` names each pin's job; a press becomes
//! the action the UI would send and goes through the machine's gates, and the LEDs follow Play, Pause and Stop.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use wc_config::Settings;

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

/// What `GPIO Settings` asks of each pin.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub buttons: Vec<(u8, Button)>,
    pub leds: Vec<(u8, Led)>,
    /// Pins set to something that is neither, such as `Spindle On`, which Python leaves unconfigured too.
    pub unsupported: Vec<(u8, String)>,
}

impl Plan {
    pub fn from_settings(settings: &Settings) -> Plan {
        let mut plan = Plan::default();
        (FIRST_PIN..=LAST_PIN).for_each(|pin| {
            let value = settings.text_or_empty("GPIO Settings", &format!("gpio{pin}"));
            match (Button::from_setting(&value), Led::from_setting(&value)) {
                _ if value.is_empty() => {}
                (Some(button), _) => plan.buttons.push((pin, button)),
                (_, Some(led)) => plan.leds.push((pin, led)),
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

/// The configured pins, kept open for as long as this lives.
pub struct Gpio {
    _pins: Box<dyn Pins>,
    leds: Vec<(Led, Box<dyn LedPin>)>,
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
        Gpio { _pins: pins, leds }
    }

    pub fn show(&mut self, leds: Leds) {
        self.leds.iter_mut().for_each(|(led, output)| output.set(leds.is_on(*led)));
    }
}

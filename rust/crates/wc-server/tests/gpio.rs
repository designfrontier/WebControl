//! The host's GPIO: the status LEDs reach the attached pins, and off the Pi every pin does nothing.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use wc_machine::Host;
use wc_machine::gpio::{Gpio, Led, LedPin, Leds, Pins, Plan};
use wc_server::host::SystemHost;

struct Lamp(Arc<Mutex<Vec<bool>>>);

impl LedPin for Lamp {
    fn set(&mut self, on: bool) {
        self.0.lock().unwrap().push(on);
    }
}

struct OneLamp(Arc<Mutex<Vec<bool>>>);

impl Pins for OneLamp {
    fn button(&mut self, _: u8, _: Box<dyn FnMut() + Send>) -> Result<(), String> {
        Ok(())
    }
    fn led(&mut self, _: u8) -> Result<Box<dyn LedPin>, String> {
        Ok(Box::new(Lamp(self.0.clone())))
    }
}

#[test]
fn the_status_leds_light_the_attached_gpio_until_it_is_let_go() {
    let dir = tempfile::tempdir().unwrap();
    let host = SystemHost::new(&dir.path().join(".WebControl"), Arc::new(Notify::new()));
    host.leds(Leds { play: true, ..Leds::default() });
    let lit = Arc::new(Mutex::new(Vec::new()));
    let plan = Plan { leds: vec![(22, Led::Play)], ..Plan::default() };
    let gpio = Gpio::start(&plan, Box::new(OneLamp(lit.clone())), Arc::new(|_| {}), Arc::new(|| Duration::ZERO));
    host.attach_gpio(Some(gpio));
    host.leds(Leds { play: true, ..Leds::default() });
    host.attach_gpio(None);
    host.leds(Leds::default());
    assert_eq!(*lit.lock().unwrap(), [true]);
}

#[test]
fn off_the_pi_every_pin_is_accepted_and_does_nothing() {
    let mut pins = wc_server::gpio::open();
    assert!(pins.led(22).is_ok());
    assert!(pins.button(17, Box::new(|| {})).is_ok());
}

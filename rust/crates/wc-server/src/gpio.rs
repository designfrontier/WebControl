//! The Pi's GPIO header through rppal. Anywhere else, or when rppal cannot open it, the pins do nothing.

use wc_machine::gpio::{NoPins, Pins};

pub fn open() -> Box<dyn Pins> {
    #[cfg(target_os = "linux")]
    match pi::RppalPins::new() {
        Ok(pins) => return Box::new(pins),
        Err(error) => tracing::info!(%error, "no GPIO here; GPIO buttons and LEDs do nothing"),
    }
    Box::new(NoPins)
}

#[cfg(target_os = "linux")]
mod pi {
    use rppal::gpio::{Gpio, InputPin, OutputPin, Trigger};
    use wc_machine::gpio::{LedPin, Pins};

    /// Inputs stay open here, since dropping one ends its interrupt thread.
    pub struct RppalPins {
        gpio: Gpio,
        inputs: Vec<InputPin>,
    }

    impl RppalPins {
        pub fn new() -> rppal::gpio::Result<RppalPins> {
            Ok(RppalPins { gpio: Gpio::new()?, inputs: Vec::new() })
        }
    }

    struct RppalLed(OutputPin);

    impl LedPin for RppalLed {
        fn set(&mut self, on: bool) {
            if on { self.0.set_high() } else { self.0.set_low() }
        }
    }

    impl Pins for RppalPins {
        fn button(&mut self, pin: u8, mut pressed: Box<dyn FnMut() + Send>) -> Result<(), String> {
            let mut input = self.gpio.get(pin).map_err(|e| e.to_string())?.into_input_pullup();
            input.set_async_interrupt(Trigger::FallingEdge, None, move |_| pressed()).map_err(|e| e.to_string())?;
            self.inputs.push(input);
            Ok(())
        }

        fn led(&mut self, pin: u8) -> Result<Box<dyn LedPin>, String> {
            let output = self.gpio.get(pin).map_err(|e| e.to_string())?.into_output_low();
            Ok(Box::new(RppalLed(output)))
        }
    }
}

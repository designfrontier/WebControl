//! Opening the controller's port: the real serial device (B1: 57600 8N1), or `wc-sim` behind `--simulate`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_serial::{DataBits, FlowControl, Parity, SerialPort, SerialPortBuilderExt, StopBits};
use wc_machine::{PortOpener, Serial};
use wc_sim::{Build, Sim};

pub const BAUD: u32 = 57600;
/// Long enough for the Mega's reset capacitor to see the edge (its RC is about 1 ms).
const RESET_PULSE: Duration = Duration::from_millis(20);

/// The real port. Dropping DTR and raising it again resets the Mega, so it prints its boot banner.
pub struct SerialOpener;

impl PortOpener for SerialOpener {
    fn open(&mut self, port: &str) -> std::io::Result<Serial> {
        let mut stream = tokio_serial::new(port, BAUD)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .open_native_async()?;
        let pulse = stream.write_data_terminal_ready(false).and_then(|_| {
            std::thread::sleep(RESET_PULSE);
            stream.write_data_terminal_ready(true)
        });
        if let Err(error) = pulse {
            tracing::debug!(%error, port, "no DTR on this port; the controller may not reset");
        }
        let (reader, writer) = tokio::io::split(stream);
        Ok((Box::new(reader), Box::new(writer)))
    }
}

/// The simulated controller: every open boots it, as a real port open resets the board.
pub struct SimOpener {
    sim: Arc<Mutex<Sim>>,
}

impl SimOpener {
    pub fn new(build: Build) -> SimOpener {
        SimOpener { sim: Arc::new(Mutex::new(Sim::new(build))) }
    }

    pub fn sim(&self) -> Arc<Mutex<Sim>> {
        self.sim.clone()
    }
}

impl PortOpener for SimOpener {
    fn open(&mut self, _port: &str) -> std::io::Result<Serial> {
        let (reader, writer) = tokio::io::split(wc_sim::spawn(self.sim.clone()));
        Ok((Box::new(reader), Box::new(writer)))
    }
}

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::time::{Instant, MissedTickBehavior};

use crate::Sim;

const TICK: Duration = Duration::from_millis(10);

async fn send(port: &mut DuplexStream, lines: Vec<String>) -> std::io::Result<()> {
    let text: String = lines.into_iter().map(|line| line + "\r\n").collect();
    port.write_all(text.as_bytes()).await
}

/// Opens the simulator's serial port: the board boots, then answers what the host writes until the host hangs up.
/// The simulator stays shared so a test can inject alarms or read its state.
pub fn spawn(sim: Arc<Mutex<Sim>>) -> DuplexStream {
    let (host, mut port) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let started = Instant::now();
        let now = || started.elapsed().as_millis() as u64;
        let boot = sim.lock().expect("simulator lock").boot(now());
        send(&mut port, boot).await?;
        let mut clock = tokio::time::interval(TICK);
        clock.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut buffer = [0u8; 256];
        loop {
            let replies = tokio::select! {
                read = port.read(&mut buffer) => match read? {
                    0 => return Ok::<(), std::io::Error>(()),
                    n => sim.lock().expect("simulator lock").receive(&buffer[..n], now()),
                },
                _ = clock.tick() => sim.lock().expect("simulator lock").tick(now()),
            };
            send(&mut port, replies).await?;
        }
    });
    host
}

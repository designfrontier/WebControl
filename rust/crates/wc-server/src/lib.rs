//! The `webcontrol-rs` server: `/api`, `/ws`, and the embedded React UI and images.

pub mod assets;
pub mod cli;
mod routes;
mod ws;

use std::sync::Arc;
use std::time::Duration;

use wc_config::Settings;
use wc_machine::MachineHandle;

pub use routes::router;

/// WebSocket liveness: ping every `interval`, close a connection that has not answered within `timeout`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingConfig {
    pub interval: Duration,
    pub timeout: Duration,
}

impl Default for PingConfig {
    fn default() -> Self {
        PingConfig { interval: Duration::from_secs(25), timeout: Duration::from_secs(20) }
    }
}

#[derive(Clone)]
pub struct AppState {
    machine: MachineHandle,
    settings: Arc<Settings>,
    ping: PingConfig,
}

impl AppState {
    pub fn new(machine: MachineHandle, settings: Settings, ping: PingConfig) -> AppState {
        AppState { machine, settings: Arc::new(settings), ping }
    }
}

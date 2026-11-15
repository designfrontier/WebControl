//! The `webcontrol-rs` server: `/api`, `/ws`, and the embedded React UI and images.

mod api;
pub mod assets;
pub mod cli;
pub mod host;
mod routes;
pub mod serial;
mod ws;

use std::time::Duration;

use std::sync::Arc;

use tokio::sync::watch;
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

/// What `GET /api/system/info` says about the computer the backend runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    pub name: String,
    /// Power-off works here: Linux, where `install-pi.sh` adds the sudoers rule.
    pub can_turn_off: bool,
}

impl Platform {
    pub fn current() -> Platform {
        let pi = cfg!(all(target_os = "linux", target_arch = "aarch64"));
        Platform {
            name: if pi { "RPI".into() } else { std::env::consts::OS.into() },
            can_turn_off: cfg!(target_os = "linux"),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    machine: MachineHandle,
    ping: PingConfig,
    platform: Platform,
    closing: Arc<watch::Sender<bool>>,
}

impl AppState {
    pub fn new(machine: MachineHandle, ping: PingConfig) -> AppState {
        let closing = Arc::new(watch::channel(false).0);
        AppState { machine, ping, platform: Platform::current(), closing }
    }

    /// Ends every WebSocket session, so a graceful shutdown is not left waiting on open sockets.
    pub fn close_sessions(&self) {
        self.closing.send_replace(true);
    }

    pub fn on(self, platform: Platform) -> AppState {
        AppState { platform, ..self }
    }
}

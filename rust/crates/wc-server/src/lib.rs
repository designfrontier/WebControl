//! The `webcontrol-rs` server: `/api`, `/ws`, and the embedded React UI and images.
// Routes refuse with the response itself; boxing every one would only hide that.
#![allow(clippy::result_large_err)]

mod api;
pub mod archive;
pub mod assets;
mod boards;
pub mod camera;
pub mod cli;
pub mod files;
pub mod firmware;
mod forms;
pub mod gpio;
mod guard;
pub mod help;
pub mod host;
pub mod installer;
mod optical;
pub mod queue;
mod routes;
pub mod serial;
pub mod updates;
mod ws;

use std::time::Duration;

use std::sync::Arc;

use tokio::sync::watch;
use wc_config::Paths;
use wc_machine::MachineHandle;

pub use routes::router;

/// The release version (`WC_VERSION` at build time, from the tag), else the crate's.
pub const VERSION: &str = env!("WC_VERSION");

/// The short git commit the binary was built from (`WC_GIT_SHA` at build time).
pub const COMMIT: &str = env!("WC_GIT_SHA");

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
    paths: Paths,
    ping: PingConfig,
    platform: Platform,
    closing: Arc<watch::Sender<bool>>,
    cameras: camera::Cameras,
    updates: updates::Updates,
    /// Gcode loads run one at a time, in the order they were asked for.
    loads: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    /// `paths` is the home whose `.WebControl` holds the libraries the file routes read and write.
    pub fn new(machine: MachineHandle, ping: PingConfig, paths: Paths) -> AppState {
        let closing = Arc::new(watch::channel(false).0);
        let cameras = camera::Cameras::default();
        let updates = updates::Updates::off();
        let loads = Arc::default();
        AppState { machine, paths, ping, platform: Platform::current(), closing, cameras, updates, loads }
    }

    pub fn with_updates(self, updates: updates::Updates) -> AppState {
        AppState { updates, ..self }
    }

    pub fn with_cameras(self, cameras: camera::Cameras) -> AppState {
        AppState { cameras, ..self }
    }

    pub fn cameras(&self) -> &camera::Cameras {
        &self.cameras
    }

    /// Ends every WebSocket session, so a graceful shutdown is not left waiting on open sockets.
    pub fn close_sessions(&self) {
        self.closing.send_replace(true);
    }

    pub fn on(self, platform: Platform) -> AppState {
        AppState { platform, ..self }
    }
}

//! The browser <-> backend wire as `contract/` describes it. Modules mirror `frontend/src/protocol/` slices.

#[path = "slices/calibration.rs"]
pub mod calibration;
mod envelope;
mod events;
mod frame;
#[path = "slices/machine.rs"]
pub mod machine;
#[path = "slices/main.rs"]
pub mod main;
#[path = "slices/settings.rs"]
pub mod settings;
#[path = "slices/system.rs"]
pub mod system;
#[path = "slices/viewer.rs"]
pub mod viewer;

pub use envelope::{DataFormat, Envelope, Message};
pub use events::{Action, ClientEvent, float};
pub use frame::{Frame, Namespace, Push};

/// `contract/VERSION` at build time.
pub const CONTRACT_VERSION: u32 = match u32::from_str_radix(env!("WC_CONTRACT_VERSION"), 10) {
    Ok(version) => version,
    Err(_) => panic!("WC_CONTRACT_VERSION is not a number"),
};

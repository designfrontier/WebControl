//! The browser <-> backend wire as `contract/` describes it. Modules mirror `frontend/src/protocol/` slices.

mod envelope;
mod events;
mod frame;
#[path = "slices/main.rs"]
pub mod main;

pub use envelope::{DataFormat, Envelope, Message};
pub use events::{Action, ClientEvent};
pub use frame::{Frame, Namespace, Push};

/// `contract/VERSION` at build time.
pub const CONTRACT_VERSION: u32 = match u32::from_str_radix(env!("WC_CONTRACT_VERSION"), 10) {
    Ok(version) => version,
    Err(_) => panic!("WC_CONTRACT_VERSION is not a number"),
};

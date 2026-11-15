//! The sans-IO machine core (`Machine::handle(event, now) -> effects`) and the tokio shell that feeds it.
//! The machine task is the only writer of machine state and the only writer to the port.

mod actions;
pub mod calibration;
pub mod commands;
mod edits;
pub mod files;
pub mod forms;
mod gates;
pub mod gpio;
mod job;
mod link;
mod machine;
mod pid;
mod setup;
mod shell;
mod ui;
mod upgrade;
mod work;

pub use edits::{Request, Response};
pub use gates::{Gate, gate};
pub use job::Flow;
pub use machine::{Effect, Event, Machine, Shared, Snapshot};
pub use shell::{Core, Host, MachineHandle, NoHost, NoPort, PortOpener, Serial, spawn};
pub use upgrade::{FirmwareJob, JobResult, POST_FLASH_WAIT};
pub use work::{Done, Work};

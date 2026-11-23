//! The sans-IO machine core (`Machine::handle(event, now) -> effects`) and the tokio shell that feeds it.
//! The machine task is the only writer of machine state and the only writer to the port.

mod actions;
mod alarm;
mod arcs;
pub mod calibration;
mod camera;
mod clearance;
pub mod commands;
pub mod controls;
mod edits;
pub mod files;
pub mod forms;
mod gates;
pub mod gpio;
mod job;
mod link;
mod lost;
mod machine;
pub mod optical;
pub mod pacing;
pub mod park;
mod pid;
pub mod queue;
mod setup;
mod shell;
mod sync;
mod ui;
mod upgrade;
mod work;
mod zlimits;

pub use camera::CameraReport;
pub use edits::{Request, Response};
pub use gates::{Gate, gate};
pub use job::Flow;
pub use machine::{Effect, Event, Machine, Shared, Snapshot};
pub use shell::{Core, Host, MachineHandle, NoHost, NoPort, PortOpener, Serial, spawn};
pub use upgrade::{FirmwareJob, JobResult, POST_FLASH_WAIT};
pub use work::{Done, Work};

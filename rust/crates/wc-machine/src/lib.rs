//! The sans-IO machine core (`Machine::handle(event, now) -> effects`) and the tokio shell that feeds it.
//! The machine task is the only writer of machine state and the only writer to the port.

mod actions;
pub mod commands;
pub mod files;
mod link;
mod machine;
mod shell;
mod ui;

pub use machine::{Effect, Event, Machine, Shared, Snapshot};
pub use shell::{Core, Host, MachineHandle, NoHost, NoPort, PortOpener, Serial, spawn};

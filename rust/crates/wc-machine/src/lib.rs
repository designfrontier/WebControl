//! The sans-IO machine core (`Machine::handle(event, now) -> effects`) and the tokio shell that feeds it.
//! The machine task is the only writer of machine state and the only writer to the port.

mod machine;
mod shell;

pub use machine::{Effect, Event, Machine, Snapshot};
pub use shell::{Core, MachineHandle, PortWriter, spawn};

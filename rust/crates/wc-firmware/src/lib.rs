//! The Maslow controller's serial protocol as `spec/firmware.md` describes it, with no IO: what a controller line
//! means, how many bytes its line buffer holds, what a boot looks like, and the `$` settings lines.

pub mod boot;
pub mod classify;
pub mod flow;
pub mod image;
pub mod number;
pub mod planner;
pub mod settings;
pub mod upgrade;

pub use classify::{Peaks, Planned, PositionError, Reply, RunMode, Status, classify, split_glued};
pub use flow::{Accounting, BUFFER_SIZE, EMPTY_RING, Sent};

//! The `queue` slice: the job queue's view and the bodies of its writes (Rust only).

use serde::{Deserialize, Serialize};

use crate::Message;

/// What happens before a queued file starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Between {
    SameTool,
    ToolChange,
    NoTool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueueItem {
    pub id: u64,
    pub file: String,
    pub tool: Option<i64>,
    pub between: Between,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QueueState {
    #[default]
    Idle,
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QueueStep {
    #[default]
    None,
    Finishing,
    Loading,
    Raising,
    ToolChange,
}

/// `GET /api/queue` and the `jobQueue` push.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobQueue {
    pub items: Vec<QueueItem>,
    pub state: QueueState,
    pub reason: Option<String>,
    pub head_locked: bool,
    pub step: QueueStep,
}

impl Message for JobQueue {
    const COMMAND: &'static str = "jobQueue";
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueAdd {
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueMove {
    pub id: u64,
    pub to: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueRemove {
    pub id: u64,
}

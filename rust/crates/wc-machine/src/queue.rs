//! The job queue (owner decision, 2026-11-22: "start at once; pause at tool changes"). When the queue's first file
//! has run and the controller has finished it, Z rises to the safe height and the next file starts; one that needs
//! another tool, or names none, first waits at the tool-change prompt for Resume.

use std::sync::Arc;
use std::time::Duration;

use wc_gcode::firmware_gcode::{FileTools, file_tools};
use wc_protocol::main::{ActivateModal, PauseButton};
use wc_protocol::queue::{Between, JobQueue, QueueItem, QueueState, QueueStep};

use crate::clearance::Raise;
use crate::edits::Response;
use crate::job::{Flow, pause_button};
use crate::machine::{Effect, Link, Machine, Queued, alert, broadcast};
use crate::pacing::announce;

pub const MAX_ITEMS: usize = 100;
const FULL: &str = "The queue holds 100 files at most.";
const NO_ITEM: &str = "That file is no longer in the queue.";
const IN_USE: &str = "The first file is loaded for the job or between files, so it stays first until it ends or Stop.";
const EMPTY: &str = "The queue is empty.";
const BUSY: &str = "A job, a tool change or the queue holds the machine. Wait for it to end, or press Stop.";
const NOT_CONNECTED: &str = "The controller is not connected.";
pub(crate) const STARTING: &str = "The queue is starting its next file. Wait for it, or press Stop.";

/// A file to queue, its tools read by the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub file: String,
    pub tools: FileTools,
}

/// A queue write from `/api/queue/*`.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    Add(Item),
    Move { id: u64, to: usize },
    Remove { id: u64 },
    Clear,
    Continue,
}

/// What the queue asks of the server: a file loaded the way `/openGCode` loads it, or the list saved.
#[derive(Debug, Clone, PartialEq)]
pub enum Outbound {
    Load(String),
    Save(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    id: u64,
    item: Item,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Change {
    None,
    Tool(i64),
    /// The file names no tool, so whatever is in the spindle must be checked.
    Check,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Step {
    #[default]
    None,
    Finishing,
    Loading,
    Raising(Change),
    ToolChange {
        prompted: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
enum Mode {
    #[default]
    Idle,
    Running,
    Stopped(String),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct State {
    entries: Vec<Entry>,
    next_id: u64,
    mode: Mode,
    step: Step,
    /// As last pushed.
    pub shown: Arc<JobQueue>,
    saved: Vec<String>,
}

impl State {
    fn push(&mut self, item: Item) {
        self.next_id += 1;
        self.entries.push(Entry { id: self.next_id, item });
    }

    fn names(&self) -> Vec<String> {
        self.entries.iter().map(|entry| entry.item.file.clone()).collect()
    }
}

impl Machine {
    /// The queue as saved before a restart; it waits for Play or Continue, never starting on its own.
    pub fn with_queue(mut self, items: Vec<Item>) -> Machine {
        items.into_iter().for_each(|item| self.queue.push(item));
        self.queue.saved = self.queue.names();
        self.queue.shown = Arc::new(self.queue_view());
        self
    }

    pub(crate) fn queue_busy(&self) -> bool {
        self.queue.step != Step::None
    }

    pub(crate) fn queue_loading(&self) -> bool {
        self.queue.step == Step::Loading
    }

    /// Play would race the queue between files; during its tool change Play is Resume.
    pub(crate) fn queue_refuses_play(&self) -> bool {
        self.queue_busy() && self.job.manual_z.is_none()
    }

    fn head_loaded(&self) -> bool {
        let name = self.gcode_name();
        self.queue.entries.first().is_some_and(|entry| name.as_deref() == Some(entry.item.file.as_str()))
    }

    fn head_locked(&self) -> bool {
        self.queue_busy() || (self.gcode_locked() && self.head_loaded())
    }

    /// Play on the queue's first file runs the queue.
    pub(crate) fn queue_played(&mut self) {
        if self.head_loaded() {
            self.queue.mode = Mode::Running;
        }
    }

    /// The last line went out; the file is done once the controller has finished it.
    pub(crate) fn queue_ran_out(&mut self) {
        if self.head_loaded() && !self.queue_busy() {
            self.queue.step = Step::Finishing;
        }
    }

    /// The queue starts nothing more until Continue or Play on its first file.
    pub(crate) fn queue_stop(&mut self, why: &str, effects: &mut Vec<Effect>) {
        if self.queue.mode != Mode::Running && !self.queue_busy() {
            return;
        }
        self.queue.mode = Mode::Stopped(why.into());
        self.queue.step = Step::None;
        announce(format!("QUEUE STOP reason={why}"), effects);
    }

    pub(crate) fn queue_load_failed(&mut self, effects: &mut Vec<Effect>) {
        if self.queue.step == Step::Loading {
            let file = self.queue.entries.first().map(|entry| entry.item.file.clone()).unwrap_or_default();
            let why = format!("Could not load {file}.");
            effects.push(alert(&format!("{why} The queue stopped; the files after it are still queued.")));
            self.queue_stop(&why, effects);
        }
    }

    /// Everything written has finished, and nothing holds the controller.
    fn controller_idle(&self) -> bool {
        let controller = &self.controller;
        matches!(self.link, Link::Connected { .. })
            && controller.accounting.is_empty()
            && controller.queue.is_empty()
            && controller.ready
            && controller.resync_after.is_none()
            && !self.alarm_held()
            && !self.probing
            && !self.job.firmware_paused
            && self.job.flow == Flow::Idle
            && self.job.manual_z.is_none()
    }

    /// Before each pump: the next step, once what the queue waits for has happened.
    pub(crate) fn queue_tick(&mut self, effects: &mut Vec<Effect>) {
        match self.queue.step {
            Step::Finishing if self.controller_idle() => self.queue_advance(effects),
            Step::ToolChange { prompted: false } if self.job.manual_z.is_some() => {
                self.queue.step = Step::ToolChange { prompted: true };
            }
            // The firmware already had the tool, so it did not pause; the bit still gets checked.
            Step::ToolChange { prompted: false } if self.controller_idle() => {
                self.queue_prompt(Some(self.job.tool), effects)
            }
            Step::ToolChange { prompted: true } if self.job.manual_z.is_none() => {
                self.queue_raise_then(Change::None, effects)
            }
            _ => {}
        }
    }

    fn queue_advance(&mut self, effects: &mut Vec<Effect>) {
        let done = self.queue.entries.remove(0);
        announce(format!("QUEUE DONE file={}", done.item.file), effects);
        self.queue.step = Step::None;
        if self.queue.mode != Mode::Running {
            return;
        }
        if self.queue.entries.is_empty() {
            self.queue.mode = Mode::Idle;
            return announce("QUEUE FINISHED".into(), effects);
        }
        self.queue_load(effects);
    }

    fn queue_load(&mut self, effects: &mut Vec<Effect>) {
        let file = self.queue.entries[0].item.file.clone();
        self.queue.step = Step::Loading;
        announce(format!("QUEUE LOAD file={file}"), effects);
        effects.push(Effect::Queue(Outbound::Load(file)));
    }

    /// The first file is loaded: raise, then start it or wait for the tool.
    pub(crate) fn queue_prepare(&mut self, effects: &mut Vec<Effect>) {
        let first = file_tools(self.gcode.iter().map(String::as_str)).first;
        let change = match first {
            Some(tool) if tool == self.job.tool => Change::None,
            Some(tool) => Change::Tool(tool),
            None => Change::Check,
        };
        self.index = 0;
        self.queue_raise_then(change, effects);
    }

    fn queue_raise_then(&mut self, change: Change, effects: &mut Vec<Effect>) {
        let Some(safe_height_mm) = self.safe_height_mm() else {
            let why = self.safe_height("starting the next queued file").err().unwrap_or_default();
            effects.push(alert(&why));
            return self.queue_stop(&why, effects);
        };
        self.queue_raise(Raise::Manual { units: self.units(), safe_height_mm });
        if change != Change::None {
            self.queue(["M5".to_string()]);
        }
        self.controller.queue.push_back(Queued::Advance);
        self.queue.step = Step::Raising(change);
    }

    /// `Queued::Advance`: every line before it has finished.
    pub(crate) fn queue_go(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let Step::Raising(change) = self.queue.step else { return };
        let file = self.queue.entries.first().map(|entry| entry.item.file.clone()).unwrap_or_default();
        match change {
            Change::None => {
                self.queue.step = Step::None;
                self.index = 0;
                announce(format!("QUEUE START file={file}"), effects);
                if let Some(refused) = self.start_run(now, effects) {
                    effects.push(alert(&refused));
                    self.queue_stop(&refused, effects);
                }
            }
            Change::Tool(tool) => {
                announce(format!("QUEUE TOOL CHANGE file={file} tool={tool}"), effects);
                self.queue([format!("T{tool} M6")]);
                self.queue.step = Step::ToolChange { prompted: false };
            }
            Change::Check => {
                announce(format!("QUEUE TOOL CHECK file={file}"), effects);
                self.queue_prompt(None, effects);
            }
        }
    }

    fn queue_prompt(&mut self, tool: Option<i64>, effects: &mut Vec<Effect>) {
        let file = self.queue.entries.first().map(|entry| entry.item.file.clone()).unwrap_or_default();
        let text = match tool {
            Some(tool) => format!("Please insert tool {tool}"),
            None => format!("{file} names no tool. Check the bit is the one it needs, then press Resume."),
        };
        self.job.manual_z = Some(true);
        self.job.paused_units = Some(self.units());
        effects.push(broadcast(&ActivateModal::notification(&text, true)));
        effects.push(pause_button(PauseButton::Resume));
        self.queue.step = Step::ToolChange { prompted: true };
    }

    pub(crate) fn queue_edit(&mut self, edit: Edit, effects: &mut Vec<Effect>) -> Response {
        let locked = self.head_locked();
        let find = |id: u64| self.queue.entries.iter().position(|entry| entry.id == id).ok_or((404, NO_ITEM.into()));
        let done: Result<(), (u16, String)> = match edit {
            Edit::Add(_) if self.queue.entries.len() >= MAX_ITEMS => Err((409, FULL.into())),
            Edit::Add(item) => {
                self.queue.push(item);
                Ok(())
            }
            Edit::Move { id, to } => find(id).and_then(|from| {
                let to = to.min(self.queue.entries.len() - 1);
                if locked && (from == 0 || to == 0) {
                    return Err((409, IN_USE.into()));
                }
                let entry = self.queue.entries.remove(from);
                self.queue.entries.insert(to, entry);
                Ok(())
            }),
            Edit::Remove { id } => find(id).and_then(|at| match locked && at == 0 {
                true => Err((409, IN_USE.into())),
                false => {
                    self.queue.entries.remove(at);
                    Ok(())
                }
            }),
            Edit::Clear => {
                self.queue.entries.truncate(usize::from(locked));
                Ok(())
            }
            Edit::Continue => self.queue_continue(effects),
        };
        if self.queue.entries.is_empty() && !self.queue_busy() {
            self.queue.mode = Mode::Idle;
        }
        match done {
            Ok(()) => Response::ok(200, &self.queue_view()),
            Err((status, text)) => Response::error(status, &text),
        }
    }

    fn queue_continue(&mut self, effects: &mut Vec<Effect>) -> Result<(), (u16, String)> {
        let refusal = match () {
            _ if self.queue.entries.is_empty() => Some(EMPTY.to_string()),
            _ if !matches!(self.link, Link::Connected { .. }) => Some(NOT_CONNECTED.into()),
            _ if self.gcode_locked() || self.loads > 0 || self.upgrade.holds_link() => Some(BUSY.into()),
            _ if self.alarm_held() => Some(crate::alarm::HELD.into()),
            _ if self.probing => Some(crate::actions::PROBING.into()),
            _ => self.restart_refusal().map(String::from).or_else(|| self.safe_height("starting the queue").err()),
        };
        if let Some(refusal) = refusal {
            return Err((409, refusal));
        }
        self.queue.mode = Mode::Running;
        match self.head_loaded() {
            true => self.queue_prepare(effects),
            false => self.queue_load(effects),
        }
        Ok(())
    }

    fn queue_view(&self) -> JobQueue {
        let items = self.queue.entries.iter().scan(self.job.tool, |spindle, Entry { id, item }| {
            let between = match item.tools.first {
                None => Between::NoTool,
                Some(tool) if tool == *spindle => Between::SameTool,
                Some(_) => Between::ToolChange,
            };
            *spindle = item.tools.after(*spindle);
            Some(QueueItem { id: *id, file: item.file.clone(), tool: item.tools.first, between })
        });
        let (state, reason) = match &self.queue.mode {
            Mode::Idle => (QueueState::Idle, None),
            Mode::Running => (QueueState::Running, None),
            Mode::Stopped(why) => (QueueState::Stopped, Some(why.clone())),
        };
        let step = match self.queue.step {
            Step::None => QueueStep::None,
            Step::Finishing => QueueStep::Finishing,
            Step::Loading => QueueStep::Loading,
            Step::Raising(_) => QueueStep::Raising,
            Step::ToolChange { .. } => QueueStep::ToolChange,
        };
        JobQueue { items: items.collect(), state, reason, head_locked: self.head_locked(), step }
    }

    /// After each event: every client hears a change, and a changed list is saved.
    pub(crate) fn queue_publish(&mut self, effects: &mut Vec<Effect>) {
        let view = self.queue_view();
        if view != *self.queue.shown {
            effects.push(broadcast(&view));
            self.queue.shown = Arc::new(view);
        }
        let names = self.queue.names();
        if names != self.queue.saved {
            effects.push(Effect::Queue(Outbound::Save(names.clone())));
            self.queue.saved = names;
        }
    }
}

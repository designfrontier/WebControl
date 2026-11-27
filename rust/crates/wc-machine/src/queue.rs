//! The job queue (owner decisions, 2026-11-22): when the queue's first file has run and the controller has finished
//! it, Z rises to the safe height and the next file starts. It waits at the tool-change prompt first only when the
//! files show another bit: the next file names a tool, the bit in the spindle is known, and they differ.

use std::sync::Arc;
use std::time::Duration;

use wc_gcode::firmware_gcode::{FileTools, file_tools, tool_change_line, without_tool_change};
use wc_protocol::main::{ActivateModal, PauseButton};
use wc_protocol::queue::{Between, ContinueFrom, JobQueue, QueueItem, QueueState, QueueStep};

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
const PAUSED: &str = "Pause was pressed between files.";
const NOT_STOPPED: &str = "Stop did not interrupt the first file part way, so continue it from the start.";
pub(crate) const STARTING: &str = "The queue is starting its next file. Wait for it, or press Stop.";

/// A file to queue, its tools read by the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub file: String,
    pub tools: FileTools,
    /// Queue runs of the file pause for no bit change, between files or in it.
    pub skip_bit_changes: bool,
}

/// A queue write from `/api/queue/*`.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    Add(Item),
    Move { id: u64, to: usize },
    Remove { id: u64 },
    Override { id: u64, skip: bool },
    Clear,
    Continue(ContinueFrom),
}

/// What the queue asks of the server: a file loaded the way `/openGCode` loads it, or the list saved.
#[derive(Debug, Clone, PartialEq)]
pub enum Outbound {
    Load(String),
    /// Each file and its override.
    Save(Vec<(String, bool)>),
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
    /// The bit the files and completed tool changes say is in the spindle; Stop at a prompt changes nothing.
    spindle: Option<i64>,
    /// The first file's entry and the line Stop interrupted it at.
    stopped_at: Option<(u64, usize)>,
    /// As last pushed.
    pub shown: Arc<JobQueue>,
    saved: Vec<(String, bool)>,
}

impl State {
    fn push(&mut self, item: Item) {
        self.next_id += 1;
        self.entries.push(Entry { id: self.next_id, item });
    }

    fn names(&self) -> Vec<(String, bool)> {
        self.entries.iter().map(|entry| (entry.item.file.clone(), entry.item.skip_bit_changes)).collect()
    }
}

impl Machine {
    /// The queue as saved before a restart; it waits for Play or Continue, never starting on its own.
    pub fn with_queue(mut self, items: Vec<Item>) -> Machine {
        items.into_iter().take(MAX_ITEMS).for_each(|item| self.queue.push(item));
        self.queue.saved = self.queue.names();
        self.queue.shown = Arc::new(self.queue_view());
        self
    }

    pub(crate) fn queue_busy(&self) -> bool {
        self.queue.step != Step::None
    }

    /// Between files, at the bit change the next file needs.
    pub(crate) fn queue_changing_tool(&self) -> bool {
        matches!(self.queue.step, Step::ToolChange { .. })
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

    /// A queue run of a file whose override skips its bit changes.
    pub(crate) fn skips_bit_changes(&self) -> bool {
        let skips = self.queue.entries.first().is_some_and(|entry| entry.item.skip_bit_changes);
        skips && self.queue.mode == Mode::Running && self.head_loaded()
    }

    /// A file line that would pause for a bit change goes out without its T and M6 under the override.
    pub(crate) fn queue_skip_bit_change(&mut self, line: String, effects: &mut Vec<Effect>) -> String {
        let change = tool_change_line(&line, self.job.next_tool, self.job.tool);
        if !change.pauses || !self.skips_bit_changes() {
            return line;
        }
        announce(format!("QUEUE BIT CHANGE SKIPPED file_line={} tool={}", self.index + 1, change.next_tool), effects);
        without_tool_change(&line)
    }

    /// Play on the queue's first file runs the queue.
    pub(crate) fn queue_played(&mut self) {
        if self.head_loaded() {
            self.queue.mode = Mode::Running;
            self.queue_rescan();
        }
    }

    /// The first file's tools as loaded, which may differ from the file scanned when it was queued.
    fn queue_rescan(&mut self) -> FileTools {
        let tools = file_tools(self.gcode.iter().map(String::as_str));
        if let Some(entry) = self.queue.entries.first_mut() {
            entry.item.tools = tools;
        }
        tools
    }

    /// The last line went out; the file is done once the controller has finished it. False for a file not queued.
    pub(crate) fn queue_ran_out(&mut self) -> bool {
        if self.head_loaded() && !self.queue_busy() {
            self.queue.step = Step::Finishing;
        }
        self.head_loaded()
    }

    /// A tool change was confirmed with Resume.
    pub(crate) fn queue_tool_fitted(&mut self) {
        self.queue.spindle = Some(self.job.tool).filter(|&tool| tool > 0).or(self.queue.spindle);
    }

    fn head_id(&self) -> Option<u64> {
        self.queue.entries.first().map(|entry| entry.id).filter(|_| self.head_loaded())
    }

    /// Where Stop left the first file part way, while it is still loaded.
    fn stopped_at(&self) -> Option<usize> {
        self.queue.stopped_at.filter(|&(id, _)| Some(id) == self.head_id()).map(|(_, line)| line)
    }

    /// The queue starts nothing more until Continue or Play on its first file.
    pub(crate) fn queue_stop(&mut self, why: &str, effects: &mut Vec<Effect>) {
        self.park_cancelled();
        self.park_tool_change_ended();
        let oldest = self.running_index();
        let part_way = self.job.flow.in_job() || (self.queue.step == Step::Finishing && oldest.is_some());
        if let Some(id) = self.head_id().filter(|_| part_way) {
            self.queue.stopped_at = Some((id, oldest.unwrap_or(self.index)));
        }
        if self.queue.mode != Mode::Running && !self.queue_busy() {
            return;
        }
        self.queue.mode = Mode::Stopped(why.into());
        self.queue.step = Step::None;
        announce(format!("QUEUE STOP reason={why}"), effects);
    }

    /// Pause once the file's last line is out: it finishes, and the next file waits for Continue.
    pub(crate) fn queue_paused(&mut self, effects: &mut Vec<Effect>) {
        match self.queue.step {
            Step::Finishing if self.queue.mode == Mode::Running => {
                self.queue.mode = Mode::Stopped(PAUSED.into());
                announce(format!("QUEUE STOP reason={PAUSED}"), effects);
            }
            Step::Loading | Step::Raising(_) => self.queue_stop(PAUSED, effects),
            _ => {}
        }
    }

    /// Another file, or another text of it, is loaded, so no line Stop left applies.
    pub(crate) fn queue_file_replaced(&mut self) {
        self.queue.stopped_at = None;
    }

    pub(crate) fn queue_load_failed(&mut self, effects: &mut Vec<Effect>) {
        let file = self.queue.entries.first().map(|entry| entry.item.file.clone()).unwrap_or_default();
        self.queue_load_refused(&format!("Could not load {file}."), effects);
    }

    pub(crate) fn queue_load_refused(&mut self, why: &str, effects: &mut Vec<Effect>) {
        if self.queue.step == Step::Loading {
            effects.push(alert(&format!("{why} The queue stopped; the files after it are still queued.")));
            self.queue_stop(why, effects);
        }
    }

    /// Everything written has finished, and nothing holds the controller.
    pub(crate) fn controller_idle(&self) -> bool {
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
    pub(crate) fn queue_tick(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        match self.queue.step {
            Step::Finishing if self.controller_idle() => self.queue_advance(effects),
            Step::ToolChange { prompted: false } if self.job.manual_z.is_some() => {
                self.queue.step = Step::ToolChange { prompted: true };
            }
            // The firmware already had the tool, so it did not pause; the bit still gets checked.
            Step::ToolChange { prompted: false } if self.controller_idle() => self.queue_prompt(now, effects),
            Step::ToolChange { prompted: true } if self.job.manual_z.is_none() => {
                self.queue_raise_then(Change::None, effects)
            }
            _ => {}
        }
    }

    fn queue_advance(&mut self, effects: &mut Vec<Effect>) {
        let done = self.queue.entries.remove(0);
        // The file as it ran, which may differ from the one scanned when it was queued.
        let ran = file_tools(self.gcode.iter().map(String::as_str));
        self.queue.spindle = ran.last.or(self.queue.spindle);
        announce(format!("QUEUE DONE file={}", done.item.file), effects);
        self.queue.step = Step::None;
        if self.queue.mode != Mode::Running {
            return;
        }
        if self.queue.entries.is_empty() {
            self.queue.mode = Mode::Idle;
            self.park_when_finished(crate::park::Trigger::Queue);
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
        let first = self.queue_rescan().first;
        let skip = self.queue.entries.first().is_some_and(|entry| entry.item.skip_bit_changes);
        let change = match (first, self.queue.spindle) {
            (Some(tool), Some(spindle)) if tool != spindle && !skip => Change::Tool(tool),
            (Some(tool), Some(spindle)) if tool != spindle => {
                announce(format!("QUEUE BIT CHANGE SKIPPED between_files tool={tool}"), effects);
                Change::None
            }
            _ => Change::None,
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
        if !self.queue_return_from_park(safe_height_mm) {
            self.queue_raise(Raise::Manual { units: self.units(), safe_height_mm });
        }
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
        }
    }

    fn queue_prompt(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let text = format!("Please insert tool {}", self.job.tool);
        self.job.manual_z = Some(true);
        self.job.paused_units = Some(self.units());
        effects.push(broadcast(&ActivateModal::tool_change(&text, u32::try_from(self.job.tool).ok())));
        effects.push(pause_button(PauseButton::Resume));
        self.queue.step = Step::ToolChange { prompted: true };
        self.park_tool_change_asked();
        self.park_for_tool_change(now, effects);
    }

    pub(crate) fn queue_edit(&mut self, edit: Edit, now: Duration, effects: &mut Vec<Effect>) -> Response {
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
            Edit::Override { id, skip } => find(id).and_then(|at| match locked && at == 0 {
                true => Err((409, IN_USE.into())),
                false => {
                    self.queue.entries[at].item.skip_bit_changes = skip;
                    Ok(())
                }
            }),
            Edit::Clear => {
                self.queue.entries.truncate(usize::from(locked));
                Ok(())
            }
            Edit::Continue(from) => self.queue_continue(from, now, effects),
        };
        if self.queue.entries.is_empty() && !self.queue_busy() {
            self.queue.mode = Mode::Idle;
        }
        match done {
            Ok(()) => Response::ok(200, &self.queue_view()),
            Err((status, text)) => Response::error(status, &text),
        }
    }

    fn queue_continue(
        &mut self,
        from: ContinueFrom,
        now: Duration,
        effects: &mut Vec<Effect>,
    ) -> Result<(), (u16, String)> {
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
        if from == ContinueFrom::Stopped {
            return self.queue_continue_where_stopped(now, effects);
        }
        self.queue.stopped_at = None;
        self.queue.mode = Mode::Running;
        match self.head_loaded() {
            true => self.queue_prepare(effects),
            false => self.queue_load(effects),
        }
        Ok(())
    }

    /// Play mid-file at the line Stop left, so the restart rebuilds the units, Z and the tool.
    fn queue_continue_where_stopped(&mut self, now: Duration, effects: &mut Vec<Effect>) -> Result<(), (u16, String)> {
        let line = self.stopped_at().ok_or((409, NOT_STOPPED.to_string()))?;
        self.index = line;
        // Running before the restart is built, so a file's override leaves its tool change out.
        let before = std::mem::replace(&mut self.queue.mode, Mode::Running);
        if let Some(refused) = self.start_run(now, effects) {
            self.queue.mode = before;
            return Err((409, refused));
        }
        self.queue.stopped_at = None;
        announce(format!("QUEUE CONTINUE file_line={}", line + 1), effects);
        Ok(())
    }

    fn queue_view(&self) -> JobQueue {
        let items = self.queue.entries.iter().scan(self.queue.spindle, |spindle, Entry { id, item }| {
            let between = match (item.tools.first, *spindle) {
                (None, _) => Between::NoTool,
                (Some(tool), Some(held)) if tool == held => Between::SameTool,
                (Some(_), Some(_)) if item.skip_bit_changes => Between::ToolChangeSkipped,
                (Some(_), Some(_)) => Between::ToolChange,
                (Some(_), None) => Between::UnknownTool,
            };
            *spindle = item.tools.last.or(*spindle);
            let skip_bit_changes = item.skip_bit_changes;
            Some(QueueItem { id: *id, file: item.file.clone(), tool: item.tools.first, between, skip_bit_changes })
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
        let head_locked = self.head_locked();
        JobQueue { items: items.collect(), state, reason, head_locked, step, stopped_at: self.stopped_at() }
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

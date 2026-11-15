//! The loaded gcode and the board (`File/gcodeFile.py`, `Boards/boardManager.py`): what the file routes load, the
//! preview the viewer draws, and the actions that move through the file or mark the board.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use wc_config::Settings;
use wc_gcode::board::{Board, BoardEdit, BoardTrim};
use wc_gcode::preview::{self, Move};
use wc_gcode::{GcodeError, load, position, py};
use wc_protocol::boards::BoardDataUpdate;
use wc_protocol::files::UpdateDirectories;
use wc_protocol::main::{ActivateModal, GcodePositionMessage, Units};
use wc_protocol::{Envelope, Push};

use crate::machine::{Effect, Machine, alert, broadcast};

pub const GCODE_LOCKED: &str = "Cannot load gcode while a job is running or paused. Stop the job first.";
pub const SETTINGS_LOCKED: &str = "Cannot change settings while a job is running or paused. Stop the job first.";
pub const SEND_LOCKED: &str = "Cannot send gcode while a job is running or paused. Stop the job first.";
const PROBING: &str = "Wait for Touch Z to finish or press Stop";

/// The 3D preview: the gzip `gcodeUpdateCompressed` sends, `None` until a file is first loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    pub compressed: Option<Vec<u8>>,
    pub moves: Vec<Move>,
    /// The units the file's last G20/G21 left, which `gcodePositionMessage` converts from.
    pub file_units: Units,
}

impl Preview {
    pub fn none(units: Units) -> Preview {
        Preview { compressed: None, moves: Vec::new(), file_units: units }
    }
}

/// A file parsed for the machine, off its task: a 100k-line file takes a while to filter and draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Gcode {
    pub lines: Vec<String>,
    pub z_moves: Vec<usize>,
    /// Set when the file's G20/G21 changed `Computed Settings.tolerance`.
    pub tolerance: Option<f64>,
    pub preview: Preview,
    pub truncate: Option<u32>,
    /// The alert Python raises when only the first 300,000 lines are drawn.
    pub too_large: Option<String>,
}

/// `Advanced Settings.truncate` is on when it equals 1; `digits` is the text Python splices into its pattern.
fn truncate_digits(settings: &Settings) -> Result<Option<u32>, GcodeError> {
    let on = settings.value("Advanced Settings", "truncate").is_some_and(|v| v == 1 || v == 1.0 || v == true);
    if !on {
        return Ok(None);
    }
    let digits = settings.value("Advanced Settings", "digits").and_then(Value::as_str);
    digits.and_then(|text| text.parse::<u32>().ok()).filter(|&d| d > 0).map(Some).ok_or(GcodeError::TruncateDigits)
}

impl Gcode {
    /// `loadUpdateFile`'s filter, Z moves and drawing.
    pub fn parse(raw: &str, settings: &Settings) -> Result<Gcode, GcodeError> {
        let truncate = truncate_digits(settings)?;
        let lines = load::filter(raw, truncate.map(|d| d as usize))?;
        let units = Units::from_setting(&settings.text_or_empty("Computed Settings", "units"));
        let tolerance = settings.number("Computed Settings", "tolerance");
        let moves = load::z_moves(&lines, tolerance);
        let drawn = preview::render(&lines, units);
        Ok(Gcode {
            z_moves: moves.indices,
            tolerance: (moves.tolerance != tolerance).then_some(moves.tolerance),
            preview: Preview {
                compressed: Some(preview::compress(&drawn.moves)),
                moves: drawn.moves,
                file_units: drawn.file_units,
            },
            truncate,
            too_large: drawn.too_large,
            lines,
        })
    }

    /// No gcode at all, as Clear GCode leaves it.
    pub fn empty(units: Units) -> Gcode {
        Gcode {
            lines: Vec::new(),
            z_moves: vec![0],
            tolerance: None,
            preview: Preview { compressed: Some(preview::compress(&[])), moves: Vec::new(), file_units: units },
            truncate: None,
            too_large: None,
        }
    }
}

/// How a file reached the machine, which decides the file name, the index and what is pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `/openGCode` and `/uploadGCode`: back to line 1; the client asks for the preview.
    Open,
    /// `/editGCode` with text: the index kept where it fits, the preview pushed.
    Edit,
    /// `/editGCode` with no text reads the file again from disk.
    Reload,
}

/// A file route's trouble the machine reports, as Python's alerts did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    GcodeUnreadable,
    GcodeUnsaved,
    BoardUnreadable,
    BoardUnsaved,
    ImportUnreadable,
}

/// The Edit Board form's fields, as posted.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoardForm {
    pub board_id: String,
    pub material: String,
    pub height: String,
    pub width: String,
    pub thickness: String,
    pub center_x: String,
    pub center_y: String,
    pub units: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrimForm {
    pub top: String,
    pub bottom: String,
    pub left: String,
    pub right: String,
    pub units: String,
}

/// What an HTTP route asks of the machine; the answer comes back as a `Reply`.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    SetSettings(Vec<(String, String, Value)>),
    /// A file route started building a load, which ends in `LoadGcode` or `Problem(GcodeUnreadable)`.
    LoadStarting,
    LoadGcode {
        gcode: Box<Gcode>,
        file: Option<PathBuf>,
        origin: Origin,
    },
    Problem(Problem),
    SendGcode(String),
    EditBoard(BoardForm),
    TrimBoard(TrimForm),
    LoadBoard(String),
    /// Names the board and answers its file's text.
    SaveBoard(String),
    ImportIni(String),
    ImportWebControl(Value),
    RestoreSettings(Box<Settings>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Done,
    Refused(String),
    Failed,
    Text(String),
}

/// Python's `int()` of an action argument.
fn int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)),
        Value::String(text) => py::int(text),
        Value::Bool(flag) => Some(i64::from(*flag)),
        _ => None,
    }
}

/// Python's `list[index]`, counting from the end when negative.
fn python_index<T: Copy>(list: &[T], index: i64) -> Option<T> {
    let resolved = if index < 0 { index + list.len() as i64 } else { index };
    usize::try_from(resolved).ok().and_then(|at| list.get(at).copied())
}

/// Python's `str.splitlines`, near enough: `/sendGCode` drops the blank lines it leaves.
fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split(['\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}'])
}

impl Machine {
    pub(crate) fn gcode_locked(&self) -> bool {
        self.job.flow.in_job() || self.job.manual_z.is_some()
    }

    pub(crate) fn push_gcode(&self, effects: &mut Vec<Effect>) {
        let compressed = self.preview.compressed.as_deref();
        if let Some(bytes) = compressed {
            effects.push(Effect::Push(Push::message(Envelope::int("showFPSpinner", bytes.len() as i64))));
        }
        effects.push(Effect::Push(Push::message(Envelope::base64("gcodeUpdateCompressed", compressed))));
    }

    pub(crate) fn push_board(&self, effects: &mut Vec<Effect>) {
        effects.push(broadcast(&BoardDataUpdate(self.workpiece.info())));
        effects.push(Effect::Push(Push::message(Envelope::int("showFPSpinner", 1))));
        let cut = self.workpiece.cut_payload();
        effects.push(Effect::Push(Push::message(Envelope::base64("boardCutDataUpdateCompressed", cut.as_deref()))));
    }

    /// `checkForGCodeUpdate`: the units, since a file can change them, then the preview.
    pub(crate) fn gcode_update_requested(&self, effects: &mut Vec<Effect>) {
        self.push_units(effects);
        self.push_gcode(effects);
    }

    pub(crate) fn directory_created(&self, name: String, effects: &mut Vec<Effect>) {
        effects.push(broadcast(&UpdateDirectories { directory: name }));
    }

    /// `sendGCodePositionUpdate(recalculate=True)`: false where Python's answer was falsy.
    pub(crate) fn push_gcode_position(&self, effects: &mut Vec<Effect>) -> bool {
        let Some(line) = self.gcode.get(self.index) else { return false };
        let Ok(at) = position::find_position_at(&self.gcode, self.index) else { return false };
        let scale = match (self.preview.file_units, self.units()) {
            (Units::Mm, Units::Inches) => 1.0 / 25.4,
            (Units::Inches, Units::Mm) => 25.4,
            _ => 1.0,
        };
        effects.push(broadcast(&GcodePositionMessage {
            xval: at.x * scale,
            yval: at.y * scale,
            zval: at.z * scale,
            gcode_line: line.clone(),
            gcode_line_index: self.index,
        }));
        true
    }

    fn install(&mut self, gcode: Gcode) {
        self.gcode = Arc::new(gcode.lines);
        self.z_moves = gcode.z_moves;
        self.preview = Arc::new(gcode.preview);
        self.truncate = gcode.truncate;
        if let Some(tolerance) = gcode.tolerance {
            self.set_setting("Computed Settings", "tolerance", Value::from(tolerance));
        }
    }

    fn load(&mut self, gcode: Gcode, file: Option<PathBuf>, origin: Origin, effects: &mut Vec<Effect>) -> Reply {
        self.loads = self.loads.saturating_sub(1);
        if self.gcode_locked() {
            return Reply::Refused(GCODE_LOCKED.into());
        }
        if let Some(text) = &gcode.too_large {
            effects.push(alert(text));
        }
        let index = self.index;
        self.install(gcode);
        self.index = match origin {
            Origin::Edit => index.min(self.gcode.len().saturating_sub(1)),
            Origin::Open | Origin::Reload => 0,
        };
        if origin != Origin::Edit {
            self.gcode_file = file;
        }
        self.push_gcode_position(effects);
        if origin != Origin::Open {
            self.push_gcode(effects);
        }
        Reply::Done
    }

    pub(crate) fn clear_gcode(&mut self, effects: &mut Vec<Effect>) -> Option<String> {
        if self.gcode_locked() {
            return Some(GCODE_LOCKED.into());
        }
        self.install(Gcode::empty(self.units()));
        self.index = 0;
        self.push_gcode(effects);
        None
    }

    /// `moveGcodeIndex`; `None` (no gcode) and a position Python could not work out both alert.
    fn move_gcode_index(&mut self, target: i64, effects: &mut Vec<Effect>) -> bool {
        let Some(last) = self.gcode.len().checked_sub(1) else { return false };
        self.index = usize::try_from(target).unwrap_or(0).min(last);
        self.push_gcode_position(effects)
    }

    pub(crate) fn move_gcode(&mut self, command: &str, arg: &Value, effects: &mut Vec<Effect>) -> Option<String> {
        let Some(arg) = int(arg) else {
            tracing::debug!(command, %arg, "not a whole number; Python raised here");
            return None;
        };
        let (target, failure) = match command {
            "moveGcodeGoto" => (Some(arg), "Error with moving to Z move"),
            "moveGcodeIndex" => (Some(self.index as i64 + arg), "Error with moving to index"),
            _ => (self.z_move_target(arg), "Error with moving to Z move"),
        };
        let moved = target.is_some_and(|target| self.move_gcode_index(target, effects));
        (!moved).then(|| failure.to_string())
    }

    /// `moveGcodeZ`: the index `moves` Z moves away, by Python's list indexing; `None` where it raised.
    fn z_move_target(&self, moves: i64) -> Option<i64> {
        let index = self.index as i64;
        let mut dist = 0;
        for (at, &z_move) in self.z_moves.iter().enumerate() {
            let (at, z_move) = (at as i64, z_move as i64);
            if moves > 0 && z_move > index {
                dist = python_index(&self.z_moves, at + moves - 1)? as i64 - index;
                break;
            }
            if moves < 0 && z_move < index {
                dist = python_index(&self.z_moves, at + moves + 1)? as i64 - index;
            }
        }
        Some(index + dist)
    }

    pub(crate) fn board_action(&mut self, command: &str, effects: &mut Vec<Effect>) {
        if command == "boardClearBoard" {
            Arc::make_mut(&mut self.workpiece).clear();
        } else {
            let home = |key: &str| py::round(self.settings.number("Advanced Settings", key), 4);
            let shift = (home("homeX"), home("homeY"));
            let units = self.units();
            let preview = self.preview.clone();
            if let Err(error) = Arc::make_mut(&mut self.workpiece).process_gcode(&preview.moves, shift, units) {
                return tracing::warn!(%error, "could not mark the board; Python raised here");
            }
        }
        self.push_board(effects);
    }

    fn problem(&mut self, problem: Problem, effects: &mut Vec<Effect>) {
        let text = match problem {
            Problem::GcodeUnreadable => "Cannot open gcode file.",
            Problem::GcodeUnsaved => "Cannot save gcode file.",
            Problem::BoardUnreadable => "Cannot open board.",
            Problem::BoardUnsaved => "Cannot save board.",
            Problem::ImportUnreadable => {
                return effects.push(broadcast(&ActivateModal::notification("Cannot open import file.", false)));
            }
        };
        if matches!(problem, Problem::GcodeUnreadable | Problem::GcodeUnsaved) {
            self.gcode_file = None;
        }
        effects.push(alert(text));
    }

    fn edit_board(&mut self, form: &BoardForm, effects: &mut Vec<Effect>) -> Reply {
        let edit = BoardEdit {
            board_id: &form.board_id,
            material: &form.material,
            height: &form.height,
            width: &form.width,
            thickness: &form.thickness,
            center_x: &form.center_x,
            center_y: &form.center_y,
            units: &form.units,
        };
        if !Arc::make_mut(&mut self.workpiece).update_info(&edit) {
            return Reply::Failed;
        }
        self.push_board(effects);
        Reply::Done
    }

    fn trim_board(&mut self, form: &TrimForm, effects: &mut Vec<Effect>) -> Reply {
        let trim = BoardTrim {
            top: &form.top,
            bottom: &form.bottom,
            left: &form.left,
            right: &form.right,
            units: &form.units,
        };
        let trimmed = Arc::make_mut(&mut self.workpiece).trim(&trim);
        self.push_board(effects);
        match trimmed {
            Ok(()) => Reply::Done,
            Err(error) => {
                tracing::warn!(%error, "could not trim the board");
                Reply::Failed
            }
        }
    }

    fn load_board(&mut self, text: &str, effects: &mut Vec<Effect>) -> Reply {
        if let Err(error) = Arc::make_mut(&mut self.workpiece).load(text) {
            tracing::warn!(%error, "could not open the board");
            self.problem(Problem::BoardUnreadable, effects);
            return Reply::Failed;
        }
        self.push_board(effects);
        Reply::Done
    }

    fn send_gcode(&mut self, text: String, effects: &mut Vec<Effect>) -> Reply {
        if self.probing {
            effects.push(alert(PROBING));
            return Reply::Failed;
        }
        self.queue(split_lines(&text).map(String::from).collect::<Vec<_>>());
        self.custom_gcode = Some(text);
        Reply::Done
    }

    /// Python asks the controller for its settings after an import or a restore, to compare them.
    fn settings_replaced(&mut self) -> Reply {
        self.settings_dirty = true;
        self.queue(["$$".to_string()]);
        Reply::Done
    }

    /// Decision 2: nothing but the job reaches the controller or the settings while the job holds the gcode.
    fn job_lock(&self, request: &Request) -> Option<&'static str> {
        let refused = match request {
            Request::SendGcode(_) => SEND_LOCKED,
            Request::ImportIni(_) | Request::ImportWebControl(_) | Request::RestoreSettings(_) => SETTINGS_LOCKED,
            _ => return None,
        };
        self.gcode_locked().then_some(refused)
    }

    pub(crate) fn request(&mut self, request: Request, effects: &mut Vec<Effect>) -> Reply {
        if let Some(refused) = self.job_lock(&request) {
            return Reply::Refused(refused.into());
        }
        match request {
            Request::SetSettings(changes) => {
                changes.into_iter().for_each(|(section, key, value)| self.set_setting(&section, &key, value));
                Reply::Done
            }
            Request::LoadGcode { gcode, file, origin } => self.load(*gcode, file, origin, effects),
            Request::LoadStarting => {
                self.loads += 1;
                effects.push(Effect::Push(Push::message(Envelope::int("showFPSpinner", 0))));
                Reply::Done
            }
            Request::Problem(problem) => {
                if problem == Problem::GcodeUnreadable {
                    // A failed load resends the current preview, so every client's spinner ends.
                    self.loads = self.loads.saturating_sub(1);
                    self.push_gcode(effects);
                }
                self.problem(problem, effects);
                Reply::Done
            }
            Request::SendGcode(text) => self.send_gcode(text, effects),
            Request::EditBoard(form) => self.edit_board(&form, effects),
            Request::TrimBoard(form) => self.trim_board(&form, effects),
            Request::LoadBoard(text) => self.load_board(&text, effects),
            Request::SaveBoard(name) => match Arc::make_mut(&mut self.workpiece).save(&name) {
                Ok(text) => Reply::Text(text),
                Err(_) => {
                    self.problem(Problem::BoardUnsaved, effects);
                    Reply::Failed
                }
            },
            Request::ImportIni(text) => match Arc::make_mut(&mut self.settings).import_ini(&text) {
                Some(()) => self.settings_replaced(),
                None => {
                    self.settings_dirty = true;
                    self.problem(Problem::ImportUnreadable, effects);
                    Reply::Failed
                }
            },
            Request::ImportWebControl(value) => match Arc::make_mut(&mut self.settings).import_webcontrol(&value) {
                Some(true) => self.settings_replaced(),
                Some(false) => Reply::Done,
                None => {
                    self.problem(Problem::ImportUnreadable, effects);
                    Reply::Failed
                }
            },
            Request::RestoreSettings(settings) => {
                self.settings = Arc::new(*settings);
                self.settings_replaced()
            }
        }
    }
}

/// The board a fresh start shows: the bed's size, nothing cut.
pub(crate) fn bed_board(settings: &Settings) -> Board {
    Board::for_bed(settings.number("Maslow Settings", "bedWidth"), settings.number("Maslow Settings", "bedHeight"))
}

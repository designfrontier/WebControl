//! The board (workpiece) model (`Boards/boards.py`, `Boards/boardManager.py`): its dimensions in inches, the 1-inch
//! grid of cells the gcode cut through, and the board file both backends read and write.

use std::fmt;
use std::io::Read;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};
use wc_protocol::main::Units;

use crate::load::universal_newlines;
use crate::preview::{Move, MoveKind, gzip};
use crate::py;

/// A dimension as the board file holds it: a number, or the text an old file or a form left there (wart 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dim {
    Number(Number),
    Text(String),
}

impl Dim {
    fn float(value: f64) -> Option<Dim> {
        Number::from_f64(value).map(Dim::Number)
    }

    /// The number Python's arithmetic sees; text raises a `TypeError` there.
    pub fn value(&self) -> Result<f64, BoardError> {
        match self {
            Dim::Number(number) => number.as_f64().ok_or(BoardError::NotANumber),
            Dim::Text(_) => Err(BoardError::NotANumber),
        }
    }

    fn python_json(&self) -> String {
        match self {
            Dim::Number(number) => py::json_value(&Value::Number(number.clone())),
            Dim::Text(text) => py::json_string(text),
        }
    }
}

/// One cell of `boardCutDataUpdateCompressed`: a flag per square inch, or a legacy convex-hull vertex (wart 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Cell {
    Cut(bool),
    Number(f64),
    Point([f64; 3]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardError {
    NotANumber,
    /// Cut data shorter than the board's grid, which Python reports as an `IndexError`.
    CutDataMismatch,
    /// No cut data to save, after a clear.
    NoCutData,
    BadFile(String),
    /// A grid with more cells than a board could need, which Python would exhaust memory building.
    TooLarge,
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoardError::NotANumber => f.write_str("a board dimension is not a number"),
            BoardError::CutDataMismatch => f.write_str("the cut data does not fit the board"),
            BoardError::NoCutData => f.write_str("the board has no cut data"),
            BoardError::BadFile(reason) => write!(f, "not a board file: {reason}"),
            BoardError::TooLarge => f.write_str("the board is too large to grid"),
        }
    }
}

impl std::error::Error for BoardError {}

/// The Edit Board form, as text in the form's units (`"mm"` or anything else for inches).
#[derive(Debug, Clone, PartialEq)]
pub struct BoardEdit<'a> {
    pub board_id: &'a str,
    pub material: &'a str,
    pub height: &'a str,
    pub width: &'a str,
    pub thickness: &'a str,
    pub center_x: &'a str,
    pub center_y: &'a str,
    pub units: &'a str,
}

/// The Trim Board form.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardTrim<'a> {
    pub top: &'a str,
    pub bottom: &'a str,
    pub left: &'a str,
    pub right: &'a str,
    pub units: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub width: Dim,
    pub height: Dim,
    pub thickness: Dim,
    pub center_x: Dim,
    pub center_y: Dim,
    pub board_id: String,
    pub material: String,
    pub file_name: String,
    pub cut_points: Vec<Cell>,
    /// Python's `compressedCutData` is `None` after a clear until the next cut or trim.
    has_cut_data: bool,
}

fn form_scale(units: &str) -> f64 {
    if units == "mm" { 25.4 } else { 1.0 }
}

fn inches(text: &str, scale: f64) -> Option<Dim> {
    py::float(text).and_then(|value| Dim::float(py::round(value / scale, 2)))
}

/// `math.ceil`, which raises on NaN and infinity.
fn ceil(value: f64) -> Result<i64, BoardError> {
    value.is_finite().then(|| value.ceil() as i64).ok_or(BoardError::NotANumber)
}

/// About a 340-foot square board in 1-inch cells.
const MAX_CELLS: i64 = 1 << 24;

/// Python's `range(columns * rows)`, empty when the product is negative.
fn cell_count(columns: i64, rows: i64) -> Result<usize, BoardError> {
    let cells = columns.checked_mul(rows).filter(|&cells| cells <= MAX_CELLS).ok_or(BoardError::TooLarge)?;
    Ok(usize::try_from(cells).unwrap_or(0))
}

/// Python's `list[index]`, negative indices counting from the end.
fn python_index(len: usize, index: i64) -> Result<usize, BoardError> {
    let resolved = if index < 0 { index + len as i64 } else { index };
    usize::try_from(resolved).ok().filter(|&at| at < len).ok_or(BoardError::CutDataMismatch)
}

impl Board {
    /// `initializeNewBoard`: the bed's size in inches, 0.75 inches thick, centred, nothing cut.
    pub fn for_bed(bed_width_mm: f64, bed_height_mm: f64) -> Board {
        let number = |value: f64| Dim::float(value).unwrap_or(Dim::Text(py::repr(value)));
        Board {
            width: number(py::round(bed_width_mm / 25.4, 2)),
            height: number(py::round(bed_height_mm / 25.4, 2)),
            thickness: number(0.75),
            center_x: number(0.0),
            center_y: number(0.0),
            board_id: "-".into(),
            material: "-".into(),
            file_name: String::new(),
            cut_points: Vec::new(),
            has_cut_data: true,
        }
    }

    /// `updateBoardInfo`: each field in turn, so a field that is not a number stops it there and answers false.
    pub fn update_info(&mut self, edit: &BoardEdit) -> bool {
        let scale = form_scale(edit.units);
        let fields = [
            (&mut self.width, edit.width),
            (&mut self.height, edit.height),
            (&mut self.thickness, edit.thickness),
            (&mut self.center_x, edit.center_x),
            (&mut self.center_y, edit.center_y),
        ];
        let all = fields.into_iter().all(|(field, text)| inches(text, scale).map(|value| *field = value).is_some());
        if all {
            self.board_id = edit.board_id.into();
            self.material = edit.material.into();
        }
        all
    }

    /// `boardDataUpdate`, as Python's `json.dumps` writes it.
    pub fn info_json(&self) -> String {
        let dims = [
            ("width", &self.width),
            ("height", &self.height),
            ("thickness", &self.thickness),
            ("centerX", &self.center_x),
            ("centerY", &self.center_y),
        ];
        let texts = [("boardID", &self.board_id), ("material", &self.material), ("fileName", &self.file_name)];
        let pairs = dims
            .iter()
            .map(|(key, dim)| format!("\"{key}\": {}", dim.python_json()))
            .chain(texts.iter().map(|(key, text)| format!("\"{key}\": {}", py::json_string(text))));
        format!("{{{}}}", pairs.collect::<Vec<_>>().join(", "))
    }

    pub fn info(&self) -> Value {
        serde_json::from_str(&self.info_json()).expect("info is JSON")
    }

    /// The gzipped JSON `boardCutDataUpdateCompressed` sends, or `None` after a clear.
    pub fn cut_payload(&self) -> Option<Vec<u8>> {
        self.has_cut_data.then(|| gzip(&serde_json::to_vec(&self.cut_points).expect("cells serialize")))
    }

    pub fn clear(&mut self) {
        self.cut_points.clear();
        self.has_cut_data = false;
    }

    /// `updateCutPoints`: the grid replaces empty cut data and is ORed into existing data.
    pub fn update_cut_points(&mut self, grid: Vec<bool>) -> Result<(), BoardError> {
        if self.cut_points.is_empty() {
            self.cut_points = grid.into_iter().map(Cell::Cut).collect();
        } else {
            self.cut_points
                .iter_mut()
                .zip(&grid)
                .filter(|(_, cut)| **cut)
                .for_each(|(cell, _)| *cell = Cell::Cut(true));
            if grid.len() < self.cut_points.len() {
                return Err(BoardError::CutDataMismatch);
            }
        }
        self.has_cut_data = true;
        Ok(())
    }

    /// `BoardManager.processGCode`: marks every cell the preview passes through below Z 0.
    pub fn process_gcode(&mut self, moves: &[Move], gcode_shift: (f64, f64), machine: Units) -> Result<(), BoardError> {
        let grid = CutGrid::new(self, gcode_shift, machine)?.mark(moves)?;
        self.update_cut_points(grid)
    }

    /// `trimBoard`: cuts the edges off the board and its grid.
    pub fn trim(&mut self, trim: &BoardTrim) -> Result<(), BoardError> {
        let scale = form_scale(trim.units);
        let edge = |text: &str| {
            py::float(text).map(|value| py::round(py::round(value, 3) / scale, 2)).ok_or(BoardError::NotANumber)
        };
        let (top, bottom, left, right) = (edge(trim.top)?, edge(trim.bottom)?, edge(trim.left)?, edge(trim.right)?);
        let (width, height) = (self.width.value()?, self.height.value()?);
        let old_columns = ceil(width)?;
        let columns = ceil(width - left - right)?;
        let rows = ceil(height - top - bottom)?;
        let mut cells = vec![Cell::Cut(false); cell_count(columns, rows)?];
        if !self.cut_points.is_empty() {
            let (skip_x, skip_y) = (left as i64, bottom as i64);
            for (x, y) in (0..columns).flat_map(|x| (0..rows).map(move |y| (x, y))) {
                let from = python_index(self.cut_points.len(), x + skip_x + (y + skip_y) * old_columns)?;
                let to = python_index(cells.len(), x + y * columns)?;
                cells[to] = self.cut_points[from].clone();
            }
        }
        self.cut_points = cells;
        self.height = Dim::float(height - top - bottom).ok_or(BoardError::NotANumber)?;
        self.width = Dim::float(width - left - right).ok_or(BoardError::NotANumber)?;
        self.has_cut_data = true;
        Ok(())
    }

    /// `saveBoard`'s file: the info line, then the cut data gzipped and base64 encoded. Names the board first, as
    /// Python does even when the save then fails.
    pub fn save(&mut self, file_name: &str) -> Result<String, BoardError> {
        self.file_name = file_name.into();
        let cut_data = self.cut_payload().ok_or(BoardError::NoCutData)?;
        Ok(format!("{}\n{}\n", self.info_json(), STANDARD.encode(cut_data)))
    }

    /// `loadBoard`: the info line, then the cut data if the file has a second line. Like Python, a bad cut data line
    /// leaves the new info in place.
    pub fn load(&mut self, text: &str) -> Result<(), BoardError> {
        let text = universal_newlines(text);
        let mut lines = text.split_inclusive('\n');
        let info: Value =
            serde_json::from_str(lines.next().unwrap_or_default()).map_err(|e| BoardError::BadFile(e.to_string()))?;
        self.load_info(&info)?;
        match lines.next() {
            Some(line) => {
                self.cut_points = decode_cut_data(line)?;
                self.has_cut_data = true;
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn load_info(&mut self, info: &Value) -> Result<(), BoardError> {
        let field = |key: &str| info.get(key).ok_or_else(|| BoardError::BadFile(format!("no {key}")));
        let dim = |key: &str| {
            field(key).and_then(|value| Dim::deserialize(value).map_err(|_| BoardError::BadFile(key.to_string())))
        };
        let text = |key: &str| {
            field(key).and_then(|value| value.as_str().map(str::to_string).ok_or(BoardError::BadFile(key.into())))
        };
        self.width = dim("width")?;
        self.height = dim("height")?;
        self.thickness = dim("thickness")?;
        self.center_x = dim("centerX")?;
        self.center_y = dim("centerY")?;
        self.board_id = text("boardID")?;
        self.material = text("material")?;
        self.file_name = text("fileName")?;
        Ok(())
    }
}

/// Python's lenient `base64.b64decode`, which skips characters outside the alphabet, then gunzip and JSON.
fn decode_cut_data(line: &str) -> Result<Vec<Cell>, BoardError> {
    let bad = |reason: String| BoardError::BadFile(reason);
    let alphabet: String = line.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')).collect();
    let gzipped = STANDARD.decode(alphabet).map_err(|e| bad(e.to_string()))?;
    let mut json = String::new();
    GzDecoder::new(&gzipped[..]).read_to_string(&mut json).map_err(|e| bad(e.to_string()))?;
    serde_json::from_str(&json).map_err(|e| bad(e.to_string()))
}

/// The board's grid in the preview's inches, with home applied.
struct CutGrid {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
    columns: i64,
    rows: i64,
    offset: (f64, f64),
    home: (f64, f64),
    cells: Vec<bool>,
}

impl CutGrid {
    fn new(board: &Board, gcode_shift: (f64, f64), machine: Units) -> Result<CutGrid, BoardError> {
        let (width, height) = (board.width.value()?, board.height.value()?);
        let (centre_x, centre_y) = (board.center_x.value()?, board.center_y.value()?);
        let (columns, rows) = (ceil(width)?, ceil(height)?);
        let scale = if machine == Units::Mm { 1.0 / 25.4 } else { 1.0 };
        Ok(CutGrid {
            left: centre_x - width / 2.0,
            right: centre_x + width / 2.0,
            top: centre_y + height / 2.0,
            bottom: centre_y - height / 2.0,
            columns,
            rows,
            offset: (columns as f64 / 2.0 - centre_x, rows as f64 / 2.0 - centre_y),
            home: (gcode_shift.0 * scale, gcode_shift.1 * scale),
            cells: vec![false; cell_count(columns, rows)?],
        })
    }

    fn mark(mut self, moves: &[Move]) -> Result<Vec<bool>, BoardError> {
        moves.iter().try_for_each(|line| match line.kind {
            MoveKind::Circle => self.mark_point(line.points[0]),
            MoveKind::Line => {
                line.points.iter().enumerate().try_for_each(|(index, &point)| match line.points.get(index + 1) {
                    Some(&next) => self.mark_segment(point, next),
                    None => self.mark_point(point),
                })
            }
        })?;
        Ok(self.cells)
    }

    fn inside(&self, x: f64, y: f64) -> bool {
        x >= self.left && x <= self.right && y >= self.bottom && y <= self.top
    }

    /// A preview point below Z 0 on the board, rounded to its cell as Python adds the offsets.
    fn mark_point(&mut self, [x, y, z]: [f64; 3]) -> Result<(), BoardError> {
        if z < 0.0 && self.inside(x + self.home.0, y + self.home.1) {
            self.set(x + self.offset.0 + self.home.0, y + self.offset.1 + self.home.1)?;
        }
        Ok(())
    }

    /// Samples a segment every quarter inch; a shorter one marks its ends.
    fn mark_segment(&mut self, from: [f64; 3], to: [f64; 3]) -> Result<(), BoardError> {
        let (x0, y0, z0) = (from[0] + self.home.0, from[1] + self.home.1, from[2]);
        let (x1, y1, z1) = (to[0] + self.home.0, to[1] + self.home.1, to[2]);
        let length = ((x0 - x1) * (x0 - x1) + (y0 - y1) * (y0 - y1) + (z0 - z1) * (z0 - z1)).sqrt();
        if length <= 0.25 {
            self.mark_point(from)?;
            return self.mark_point(to);
        }
        let samples = length * 4.0;
        // Python's int() raises here; a saturating cast would sample for ever.
        if !samples.is_finite() {
            return Err(BoardError::NotANumber);
        }
        (0..=samples as i64).try_for_each(|step| {
            let step = step as f64;
            let xa = x0 + (x1 - x0) / samples * step;
            let ya = y0 + (y1 - y0) / samples * step;
            let za = z0 + (z1 - z0) / samples * step;
            if za < 0.0 && self.inside(xa, ya) { self.set(xa + self.offset.0, ya + self.offset.1) } else { Ok(()) }
        })
    }

    fn set(&mut self, x: f64, y: f64) -> Result<(), BoardError> {
        let constrain = |value: i64, upper: i64| if value < 0 { 0 } else { value.min(upper - 1) };
        let column = constrain(py::round_int(x).ok_or(BoardError::NotANumber)?, self.columns);
        let row = constrain(py::round_int(y).ok_or(BoardError::NotANumber)?, self.rows);
        let at = python_index(self.cells.len(), column + row * self.columns)?;
        self.cells[at] = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(width: f64, height: f64, centre: (f64, f64)) -> Board {
        let mut board = Board::for_bed(0.0, 0.0);
        let (w, h, cx, cy) = (width.to_string(), height.to_string(), centre.0.to_string(), centre.1.to_string());
        let edit = BoardEdit {
            board_id: "-",
            material: "-",
            height: &h,
            width: &w,
            thickness: "0.75",
            center_x: &cx,
            center_y: &cy,
            units: "inches",
        };
        assert!(board.update_info(&edit));
        board
    }

    fn drawn(kind: MoveKind, points: &[[f64; 3]]) -> Vec<Move> {
        vec![Move { kind, points: points.to_vec(), dashed: false, command: None }]
    }

    fn cut_cells(board: &Board) -> Vec<(usize, usize)> {
        let width = board.width.value().unwrap().ceil() as usize;
        board
            .cut_points
            .iter()
            .enumerate()
            .filter(|(_, cell)| **cell == Cell::Cut(true))
            .map(|(i, _)| (i % width, i / width))
            .collect()
    }

    #[test]
    fn a_plunge_above_the_board_is_not_marked_as_cut() {
        let mut board = board(48.0, 24.0, (0.0, 0.0));
        board
            .process_gcode(&drawn(MoveKind::Circle, &[[0.0, 20.0, -1.0], [1.0, 0.0, -1.0]]), (0.0, 0.0), Units::Inches)
            .unwrap();
        assert!(cut_cells(&board).is_empty());
    }

    #[test]
    fn a_short_cut_is_checked_against_the_y_extent_not_home_x() {
        let mut board = board(48.0, 24.0, (0.0, 0.0));
        let moves = drawn(MoveKind::Line, &[[-10.0, 0.0, -0.1], [-10.0, 0.1, 0.05]]);
        board.process_gcode(&moves, (30.0, 0.0), Units::Inches).unwrap();
        assert_eq!(cut_cells(&board), [(44, 12)]);
    }

    #[test]
    fn a_long_cut_marks_every_cell_it_crosses_below_zero() {
        let mut board = board(4.0, 2.0, (0.0, 0.0));
        board
            .process_gcode(&drawn(MoveKind::Line, &[[-1.9, 0.0, -1.0], [1.9, 0.0, -1.0]]), (0.0, 0.0), Units::Inches)
            .unwrap();
        assert_eq!(cut_cells(&board), [(0, 1), (1, 1), (2, 1), (3, 1)]);
        board.process_gcode(&drawn(MoveKind::Line, &[[0.0, -25.4, -1.0]]), (0.0, 25.4), Units::Mm).unwrap();
        assert_eq!(cut_cells(&board).len(), 4);
    }

    #[test]
    fn text_dimensions_cannot_be_cut_or_trimmed() {
        let mut board = board(4.0, 2.0, (0.0, 0.0));
        board.width = Dim::Text("4".into());
        assert_eq!(board.process_gcode(&[], (0.0, 0.0), Units::Inches), Err(BoardError::NotANumber));
        let trim = BoardTrim { top: "0", bottom: "0", left: "0", right: "0", units: "inches" };
        assert_eq!(board.trim(&trim), Err(BoardError::NotANumber));
        assert_eq!(board.clone().trim(&BoardTrim { top: "x", ..trim }), Err(BoardError::NotANumber));
    }

    #[test]
    fn cut_data_shorter_than_the_grid_is_ored_then_refused() {
        let mut board = board(2.0, 1.0, (0.0, 0.0));
        board.cut_points = vec![Cell::Point([1.0, 2.0, 0.0]), Cell::Cut(false), Cell::Cut(false)];
        assert_eq!(board.update_cut_points(vec![true, false]), Err(BoardError::CutDataMismatch));
        assert_eq!(board.cut_points[0], Cell::Cut(true));
    }

    #[test]
    fn a_trim_keeps_the_cells_that_remain() {
        let mut board = board(3.0, 2.0, (0.0, 0.0));
        board.cut_points = [true, false, true, false, true, false].map(Cell::Cut).to_vec();
        board.trim(&BoardTrim { top: "0", bottom: "1", left: "1", right: "0", units: "inches" }).unwrap();
        assert_eq!(board.cut_points, [true, false].map(Cell::Cut));
        assert_eq!((board.width.value().unwrap(), board.height.value().unwrap()), (2.0, 1.0));
    }

    #[test]
    fn a_trim_past_the_old_grid_is_refused() {
        let mut board = board(2.0, 1.0, (0.0, 0.0));
        board.cut_points = vec![Cell::Cut(true)];
        let trim = BoardTrim { top: "0", bottom: "0", left: "0", right: "0", units: "inches" };
        assert_eq!(board.trim(&trim), Err(BoardError::CutDataMismatch));
    }

    #[test]
    fn clearing_removes_the_payload_until_the_next_cut() {
        let mut board = board(2.0, 1.0, (0.0, 0.0));
        assert!(board.cut_payload().is_some());
        board.clear();
        assert_eq!(board.cut_payload(), None);
        assert_eq!(board.save("a.brd"), Err(BoardError::NoCutData));
        assert_eq!(board.file_name, "a.brd");
        board.update_cut_points(vec![false, true]).unwrap();
        assert!(board.cut_payload().is_some());
    }

    #[test]
    fn a_saved_board_loads_back() {
        let mut saved = board(2.0, 1.0, (0.5, -0.5));
        saved.update_cut_points(vec![false, true]).unwrap();
        let text = saved.save("b.brd").unwrap();
        assert!(
            text.starts_with(r#"{"width": 2.0, "height": 1.0, "thickness": 0.75, "centerX": 0.5, "centerY": -0.5,"#)
        );
        let mut loaded = Board::for_bed(100.0, 100.0);
        loaded.load(&text.replace('\n', "\r\n")).unwrap();
        assert_eq!(loaded, saved);
    }

    #[test]
    fn bad_board_files_are_refused() {
        let mut board = Board::for_bed(100.0, 100.0);
        assert!(matches!(board.load(""), Err(BoardError::BadFile(_))));
        assert!(matches!(board.load("{}"), Err(BoardError::BadFile(_))));
        let info = r#"{"width": 1, "height": 1, "thickness": 1, "centerX": 0, "centerY": 0, "boardID": "a", "material": "b", "fileName": "c"}"#;
        assert!(matches!(board.load(&format!("{info}\n!!!\n")), Err(BoardError::BadFile(_))));
        assert_eq!(board.board_id, "a");
        assert!(matches!(board.load(&format!("{info}\nAAAA\n")), Err(BoardError::BadFile(_))));
        let not_cells = STANDARD.encode(gzip(b"{}"));
        assert!(matches!(board.load(&format!("{info}\n{not_cells}\n")), Err(BoardError::BadFile(_))));
        assert!(matches!(board.load(&info.replace("\"a\"", "5")), Err(BoardError::BadFile(_))));
        assert!(matches!(board.load(&info.replace("1,", "true,")), Err(BoardError::BadFile(_))));
    }

    #[test]
    fn non_finite_values_are_not_numbers() {
        assert_eq!(ceil(f64::NAN), Err(BoardError::NotANumber));
        assert_eq!(python_index(3, -1), Ok(2));
        assert_eq!(python_index(3, -4), Err(BoardError::CutDataMismatch));
        assert!(!Board::for_bed(1.0, 1.0).update_info(&BoardEdit {
            board_id: "",
            material: "",
            height: "1",
            width: "nan",
            thickness: "1",
            center_x: "0",
            center_y: "0",
            units: "mm"
        }));
    }

    #[test]
    fn a_cut_to_a_coordinate_past_float_range_is_refused_not_sampled_forever() {
        let mut board = board(48.0, 24.0, (0.0, 0.0));
        let moves = drawn(MoveKind::Line, &[[0.0, 0.0, -1.0], [f64::INFINITY, 0.0, -1.0]]);
        assert_eq!(board.process_gcode(&moves, (0.0, 0.0), Units::Inches), Err(BoardError::NotANumber));
    }

    #[test]
    fn a_grid_too_large_to_hold_is_refused() {
        let mut huge = board(1e10, 1e10, (0.0, 0.0));
        assert_eq!(huge.process_gcode(&[], (0.0, 0.0), Units::Inches), Err(BoardError::TooLarge));
        let trim = BoardTrim { top: "0", bottom: "0", left: "-1e10", right: "0", units: "inches" };
        assert_eq!(board(4.0, 1e10, (0.0, 0.0)).trim(&trim), Err(BoardError::TooLarge));
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(BoardError::NotANumber.to_string(), "a board dimension is not a number");
        assert_eq!(BoardError::CutDataMismatch.to_string(), "the cut data does not fit the board");
        assert_eq!(BoardError::NoCutData.to_string(), "the board has no cut data");
        assert_eq!(BoardError::BadFile("x".into()).to_string(), "not a board file: x");
        assert_eq!(BoardError::TooLarge.to_string(), "the board is too large to grid");
    }
}

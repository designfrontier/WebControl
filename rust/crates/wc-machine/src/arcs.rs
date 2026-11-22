//! No firmware runs an R arc, a G91 arc or a G90.1 arc as the file means it, and every one truncates G90.1 and G91.1
//! to G90 and G91 (spec/firmware.md §9, B19), so each arc goes out with I and J offsets and absolute X and Y, worked
//! out from where the sled is when the line reaches the controller, and G90.1 and G91.1 never go out.

use wc_gcode::Units;
use wc_gcode::arc::{ArcCentres, Interpreter};
use wc_gcode::sanitise::LineProblem;

/// Half a status report's last digit: MPos has two decimals in the controller's units.
const REPORT_HALF_STEP: f64 = 0.005;

/// Where the sled is: the XY WebControl commanded, or else the last status report's.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sled {
    commanded: Interpreter,
    /// MPos x and y read since the last line that moved the sled (or changed units) was acknowledged, in mm, and how
    /// far the sled may be from it on each axis.
    reported_mm: Option<([f64; 2], f64)>,
    moving: bool,
}

impl Sled {
    pub fn new() -> Sled {
        Sled { commanded: Interpreter::new(), reported_mm: None, moving: false }
    }

    fn placed(&self) -> Interpreter {
        let mut commanded = self.commanded.clone();
        if let (None, Some((xy, spread))) = (commanded.xy_mm(), self.reported_mm) {
            commanded.set_xy_mm(Some(xy), spread);
        }
        commanded
    }

    pub fn written(&mut self, line: &str) {
        self.commanded = self.placed();
        if self.commanded.written(line) {
            self.reported_mm = None;
            self.moving = true;
        }
    }

    /// The line as the file or the user had it, for the G90.1 and G91.1 its written form leaves out.
    pub fn sent(&mut self, line: &str) {
        self.commanded.follow_arc_centres(line);
    }

    /// A job run from its first line starts with I and J as offsets and in G17, whatever an earlier file or command
    /// set.
    pub fn file_modes_reset(&mut self) {
        self.commanded.set_arc_centres(ArcCentres::Offsets);
        self.commanded.set_plane(17);
    }

    /// The file's plane at a restart, which no line the restart writes carries.
    pub fn set_plane(&mut self, plane: u32) {
        self.commanded.set_plane(plane);
    }

    pub fn settled(&mut self) {
        self.moving = false;
    }

    pub fn reported(&mut self, x: f64, y: f64) {
        let Some(units) = self.commanded.units().filter(|_| !self.moving) else { return };
        let mm_per_unit = if units == Units::Inches { 25.4 } else { 1.0 };
        let xy = [x * mm_per_unit, y * mm_per_unit];
        self.reported_mm = (x.is_finite() && y.is_finite()).then_some((xy, REPORT_HALF_STEP * mm_per_unit));
    }

    /// The last status report's X and Y in mm, read while nothing that moves was unacknowledged.
    pub fn reported_mm(&self) -> Option<[f64; 2]> {
        self.reported_mm.map(|(xy, _)| xy)
    }

    /// A Stop or an alarm halted the sled somewhere a status report must tell.
    pub fn lost(&mut self) {
        self.commanded.set_xy_mm(None, 0.0);
        self.reported_mm = None;
        self.moving = false;
    }

    /// The line as it goes to the controller, on every firmware version: 51.29x reads R but drops its sign. `home`
    /// is the shift a file line's X and Y got, which its absolute arc centres need too.
    pub fn convert(&self, line: &str, home: [f64; 2]) -> Result<String, LineProblem> {
        self.placed().convert(line, home).map_err(LineProblem::Arc)
    }
}

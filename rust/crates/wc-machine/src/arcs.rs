//! No firmware runs an R arc or a G91 arc as the file means it (spec/firmware.md §9, B18), so each goes out with I
//! and J and absolute X and Y, worked out from where the sled is when the line reaches the controller.

use wc_gcode::Units;
use wc_gcode::arc::Interpreter;
use wc_gcode::sanitise::LineProblem;

/// Where the sled is: the XY WebControl commanded, or else the last status report's.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sled {
    commanded: Interpreter,
    /// MPos x and y read since the last line that moved the sled (or changed units) was acknowledged, in mm.
    reported_mm: Option<[f64; 2]>,
    moving: bool,
}

impl Sled {
    pub fn new() -> Sled {
        Sled { commanded: Interpreter::new(), reported_mm: None, moving: false }
    }

    pub fn written(&mut self, line: &str) {
        if self.commanded.xy_mm().is_none() {
            self.commanded.set_xy_mm(self.reported_mm);
        }
        if self.commanded.written(line) {
            self.reported_mm = None;
            self.moving = true;
        }
    }

    pub fn settled(&mut self) {
        self.moving = false;
    }

    pub fn reported(&mut self, x: f64, y: f64) {
        let Some(units) = self.commanded.units().filter(|_| !self.moving) else { return };
        let mm_per_unit = if units == Units::Inches { 25.4 } else { 1.0 };
        self.reported_mm = (x.is_finite() && y.is_finite()).then_some([x * mm_per_unit, y * mm_per_unit]);
    }

    /// A Stop or an alarm halted the sled somewhere a status report must tell.
    pub fn lost(&mut self) {
        self.commanded.set_xy_mm(None);
        self.reported_mm = None;
        self.moving = false;
    }

    /// The line as it goes to the controller, on every firmware version: 51.29x reads R but drops its sign.
    pub fn convert(&self, line: &str) -> Result<String, LineProblem> {
        let mut commanded = self.commanded.clone();
        if commanded.xy_mm().is_none() {
            commanded.set_xy_mm(self.reported_mm);
        }
        commanded.convert(line).map_err(LineProblem::Arc)
    }
}

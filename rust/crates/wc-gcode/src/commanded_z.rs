//! The controller's Z as WebControl last commanded it, followed through every line written to it.

use crate::{Positioning, Units, words};

/// The G codes whose Z word this follows; a Z beside any other leaves Z unknown.
const FOLLOWED: [f64; 13] = [0.0, 1.0, 2.0, 3.0, 10.0, 17.0, 20.0, 21.0, 40.0, 90.0, 91.0, 90.1, 91.1];

#[derive(Debug, Clone, PartialEq)]
pub struct CommandedZ {
    /// Unknown until a G20 or G21 is written: a controller that did not reset keeps its own.
    units: Option<Units>,
    positioning: Positioning,
    z_mm: Option<f64>,
}

impl Default for CommandedZ {
    fn default() -> CommandedZ {
        CommandedZ::new()
    }
}

impl CommandedZ {
    /// A controller just connected: units and Z unknown, absolute.
    pub fn new() -> CommandedZ {
        CommandedZ { units: None, positioning: Positioning::Absolute, z_mm: None }
    }

    pub fn z_mm(&self) -> Option<f64> {
        self.z_mm
    }

    pub fn set_z_mm(&mut self, z_mm: Option<f64>) {
        self.z_mm = z_mm;
    }

    /// The units the controller is in, which its status reports use.
    pub fn units(&self) -> Option<Units> {
        self.units
    }

    /// Whether `line` moves Z, redefines it or changes units, which a status report read before it no longer
    /// describes.
    pub fn written(&mut self, line: &str) -> bool {
        let line = words::blank_comments(line);
        let codes = words::numbers('G', &line);
        let last_of = |pair: [f64; 2]| codes.iter().rev().find(|code| pair.contains(code)).copied();
        let units = match last_of([20.0, 21.0]) {
            Some(20.0) => Some(Units::Inches),
            Some(_) => Some(Units::Mm),
            None => self.units,
        };
        let units_changed = units != self.units;
        self.units = units;
        self.positioning = match last_of([90.0, 91.0]) {
            Some(90.0) => Positioning::Absolute,
            Some(_) => Positioning::Relative,
            None => self.positioning,
        };
        let probe = codes.contains(&38.2);
        let z = match words::value(&line, 'Z') {
            Ok(None) if !probe => return units_changed,
            Ok(z) => z,
            Err(()) => None,
        };
        let Some(units) = self.units else {
            self.z_mm = None;
            return true;
        };
        let scale = if units == Units::Inches { 25.4 } else { 1.0 };
        let followed = !probe && codes.iter().all(|code| FOLLOWED.contains(code));
        // G10 sets where the bit already is, in either positioning mode.
        let relative = self.positioning == Positioning::Relative && !codes.contains(&10.0);
        self.z_mm = z
            .filter(|_| followed)
            .and_then(|z| if relative { self.z_mm.map(|from| from + z * scale) } else { Some(z * scale) });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(lines: &[&str]) -> CommandedZ {
        let mut z = CommandedZ::default();
        z.written("G21 ");
        lines.iter().for_each(|line| {
            z.written(line);
        });
        z
    }

    #[test]
    fn z_is_unknown_until_a_line_sets_it() {
        assert_eq!(after(&[]).z_mm(), None);
        assert_eq!(after(&["G0 X1 Y2 "]).z_mm(), None);
        assert_eq!(after(&["G91 ", "G00 Z10.0 "]).z_mm(), None);
    }

    #[test]
    fn absolute_and_relative_moves_follow_the_modal_positioning() {
        assert_eq!(after(&["G0 Z5 ", "G1 X1 Z-2.5 F100 "]).z_mm(), Some(-2.5));
        assert_eq!(after(&["G0 Z5 ", "G21 ", "G91 ", "G00 Z-1.5 ", "G90 "]).z_mm(), Some(3.5));
        assert_eq!(after(&["G0 Z5 ", "G91 G0 Z1 ", "Z1 "]).z_mm(), Some(7.0));
        assert_eq!(after(&["G2 X1 Y1 I1 J0 Z-3 "]).z_mm(), Some(-3.0));
        assert_eq!(after(&["G91.1 ", "G0 Z5 "]).z_mm(), Some(5.0));
        assert_eq!(after(&["G0 Z5 ", "G1 Z-1 (Z9) "]).z_mm(), Some(-1.0));
    }

    #[test]
    fn z_is_kept_in_mm_whatever_the_units() {
        let z = after(&["G20 ", "G0 Z0.5 "]);
        assert_eq!((z.z_mm(), z.units()), (Some(12.7), Some(Units::Inches)));
        assert_eq!(after(&["G20 G0 Z1 ", "G21 ", "G91 ", "G0 Z-2.7 "]).z_mm(), Some(22.7));
    }

    #[test]
    fn g10_redefines_z_where_the_bit_is() {
        let mut z = after(&["G0 Z5 "]);
        assert!(z.written("G10 Z0 "));
        assert_eq!(z.z_mm(), Some(0.0));
        assert!(!z.written("G0 X3 "));
    }

    #[test]
    fn a_probe_or_a_line_it_cannot_follow_leaves_z_unknown() {
        ["G38.2 Z-10 ", "G90 G38.2 Z-25.4", "G28 Z0 ", "G1 Z. ", "G43 Z1 "].iter().for_each(|line| {
            let mut z = after(&["G0 Z5 "]);
            assert!(z.written(line), "{line}");
            assert_eq!(z.z_mm(), None, "{line}");
        });
    }

    #[test]
    fn it_can_be_told_where_z_is() {
        let mut z = after(&[]);
        z.set_z_mm(Some(0.0));
        z.written("G91 G0 Z2 ");
        assert_eq!(z.z_mm(), Some(2.0));
    }

    #[test]
    fn a_units_change_is_reported_and_z_is_unknown_until_the_units_are() {
        let mut z = CommandedZ::new();
        assert!(z.written("G0 Z5 "));
        assert_eq!((z.z_mm(), z.units()), (None, None));
        assert!(z.written("G20 "));
        assert!(!z.written("G20 "));
        assert!(z.written("G21 "));
        assert!(!z.written("B05 "));
    }

    #[test]
    fn g10_in_relative_mode_still_sets_z_where_the_bit_is() {
        assert_eq!(after(&["G0 Z-10 ", "G91 ", "G10 Z0 ", "G0 Z10 "]).z_mm(), Some(10.0));
    }
}

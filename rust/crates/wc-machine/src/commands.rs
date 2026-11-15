//! The lines each manual command queues, byte for byte as `Actions/actions.py` builds them.

use wc_config::python_float_str;
use wc_protocol::main::Units;

const MM_PER_INCH: f64 = 25.4;

/// Python's `round(x, 4)`; both round the exact binary value half to even.
pub fn round4(value: f64) -> f64 {
    format!("{value:.4}").parse().unwrap_or(value)
}

fn scale(from: Units, to: Units) -> f64 {
    match (from, to) {
        (a, b) if a == b => 1.0,
        (Units::Inches, _) => MM_PER_INCH,
        _ => 1.0 / MM_PER_INCH,
    }
}

/// A jog: G91, one rapid, G90. `diagonal` keeps a diagonal jog's length at `distance`.
pub fn jog(direction: &str, distance: f64, diagonal: bool) -> Option<Vec<String>> {
    let diag = if diagonal { round4((distance * distance / 2.0).sqrt()) } else { distance };
    let (x, y) = match direction {
        "upLeft" => (Some(-diag), Some(diag)),
        "up" => (None, Some(distance)),
        "upRight" => (Some(diag), Some(diag)),
        "left" => (Some(-distance), None),
        "right" => (Some(distance), None),
        "downLeft" => (Some(-diag), Some(-diag)),
        "down" => (None, Some(-distance)),
        "downRight" => (Some(diag), Some(-diag)),
        _ => return None,
    };
    let words: Vec<String> = [("X", x), ("Y", y)]
        .into_iter()
        .filter_map(|(axis, value)| value.map(|v| format!("{axis}{}", python_float_str(v))))
        .collect();
    Some(vec!["G91 ".into(), format!("G00 {} ", words.join(" ")), "G90 ".into()])
}

/// The Z panel keeps its own units; the move goes out converted, so WebControl's units never change.
pub fn move_z(direction: &str, distance: f64, units: Units, units_z: Units) -> Vec<String> {
    let sign = match direction {
        "raise" => Some(1.0),
        "lower" => Some(-1.0),
        _ => None,
    };
    let units_line = if units == Units::Inches { "G20 " } else { "G21 " };
    let moved = distance * scale(units_z, units);
    let mut lines = vec![units_line.to_string()];
    if let Some(sign) = sign {
        lines.extend(["G91 ".into(), format!("G00 Z{} ", python_float_str(round4(sign * moved))), "G90 ".into()]);
    }
    lines
}

fn safe_height(units: Units, safe_height_mm: f64) -> String {
    match units {
        Units::Inches => format!("G00 Z{:.3}", safe_height_mm / MM_PER_INCH),
        Units::Mm => format!("G00 Z{}", python_float_str(safe_height_mm)),
    }
}

/// Raise to the safe height, rapid home, lower to Z0; `home_x`/`home_y` are the stored text.
pub fn home(units: Units, safe_height_mm: f64, home_x: &str, home_y: &str) -> Vec<String> {
    vec!["G90  ".into(), safe_height(units, safe_height_mm), format!("G00 X{home_x} Y{home_y} "), "G00 Z0 ".into()]
}

pub fn return_to_center(units: Units, safe_height_mm: f64) -> Vec<String> {
    vec!["G90  ".into(), safe_height(units, safe_height_mm), "G00 X0.0 Y0.0 ".into()]
}

/// B09 single-axis moves leave the firmware not knowing where it is, so B08 tells it.
pub fn move_to_default(chain_length: &str) -> Vec<String> {
    vec!["G90 ".into(), format!("B09 R{chain_length} L{chain_length} "), "G91 ".into(), "B08 ".into()]
}

/// `moveTo` takes inches from the context menu and refuses anything off the bed.
pub fn move_to(x: f64, y: f64, units: Units, bed_width_mm: f64, bed_height_mm: f64) -> Option<String> {
    let (width, height) = (bed_width_mm / MM_PER_INCH, bed_height_mm / MM_PER_INCH);
    let on_bed = x.abs() <= width / 2.0 && y.abs() <= height / 2.0;
    let factor = if units == Units::Inches { 1.0 } else { MM_PER_INCH };
    on_bed.then(|| {
        format!("G90 G00 X{} Y{} ", python_float_str(round4(x * factor)), python_float_str(round4(y * factor)))
    })
}

/// G38.2 down to the plunge limit in the machine's units; the firmware probes at a fixed feed.
pub fn touch_z(plunge_mm: f64, units: Units) -> String {
    let depth = format!("{:.4}", plunge_mm * scale(Units::Mm, units));
    format!("G90 G38.2 Z-{}", depth.trim_end_matches('0').trim_end_matches('.'))
}

/// Define Home with coordinates takes inches and stores machine units, rounded to 4 places.
pub fn home_from(x: f64, y: f64, units: Units) -> (f64, f64) {
    let factor = if units == Units::Mm { MM_PER_INCH } else { 1.0 };
    (round4(x * factor), round4(y * factor))
}

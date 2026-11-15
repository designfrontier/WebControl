//! Starting or resuming mid-file (`Actions.gcodeStateAt`, `queueRestartSequence`): the modal state the file has
//! built up by a line, and the lines that put the sled, bit and controller there.

use std::sync::LazyLock;

use regex::Regex;
use wc_protocol::main::Units;

use crate::firmware_gcode::{changes_tool, tool_named_by};
use crate::{GcodeError, Positioning, py, words};

/// A restart lowers the bit at this feed when the file has not set a Z feed.
pub const DEFAULT_PLUNGE_FEED_MM_PER_MIN: f64 = 12.8;

static LINE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\s\x1c-\x1f]*N[0-9]+").expect("pattern"));
static DWELL_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^G0*4").expect("pattern"));

/// The state in effect before a line, all in the units in effect there; X and Y include home.
#[derive(Debug, Clone, PartialEq)]
pub struct GcodeState {
    pub units: Units,
    pub positioning: Positioning,
    pub x: f64,
    pub y: f64,
    /// `None` until the file sets Z for the current tool; a restart must not lower the bit to a guess.
    pub z: Option<f64>,
    /// `None` until the file sets F; a restart must not replace the firmware's modal feed with a guess.
    pub feed_rate: Option<f64>,
    pub z_feed_rate: f64,
    /// The last of G0–G3.
    pub motion: Option<u8>,
    pub tool: Option<i64>,
    pub spindle: Option<&'static str>,
    /// The last S word, which 51.29x needs on a line of its own after M3 or M4.
    pub spindle_speed: Option<f64>,
    pub laser: Option<&'static str>,
    pub dwell: Option<String>,
}

pub fn units_word(units: Units) -> &'static str {
    match units {
        Units::Mm => "G21 ",
        Units::Inches => "G20 ",
    }
}

/// `gcodeStateAt(index)`: parses `lines[..index]`. Home is in machine units, so a file without G20/G21 runs in them.
pub fn state_at(
    lines: &[String],
    index: usize,
    home_x: f64,
    home_y: f64,
    machine: Units,
) -> Result<GcodeState, GcodeError> {
    let before = lines.get(..index).ok_or(GcodeError::IndexOutOfRange { index, len: lines.len() })?;
    let mut parser = StateParser::new(home_x, home_y, machine);
    before.iter().enumerate().try_for_each(|(line_index, line)| {
        parser.line(line).map_err(|()| GcodeError::Unreadable { index: line_index })
    })?;
    Ok(parser.state)
}

struct StateParser {
    state: GcodeState,
    home: (f64, f64),
    next_tool: Option<i64>,
}

impl StateParser {
    fn new(home_x: f64, home_y: f64, machine: Units) -> StateParser {
        let feed_scale = if machine == Units::Inches { 1.0 / 25.4 } else { 1.0 };
        let state = GcodeState {
            units: machine,
            positioning: Positioning::Absolute,
            x: home_x,
            y: home_y,
            z: None,
            feed_rate: None,
            z_feed_rate: DEFAULT_PLUNGE_FEED_MM_PER_MIN * feed_scale,
            motion: None,
            tool: None,
            spindle: None,
            spindle_speed: None,
            laser: None,
            dwell: None,
        };
        StateParser { state, home: (home_x, home_y), next_tool: None }
    }

    fn line(&mut self, raw: &str) -> Result<(), ()> {
        let line = words::blank_comments(raw);
        if py::isspace(&line) {
            return Ok(());
        }
        self.next_tool = tool_named_by(&line).or(self.next_tool);
        if changes_tool(&line) && self.next_tool.is_some() {
            self.state.tool = self.next_tool;
        }
        let unnumbered = LINE_NUMBER.replace(&line, "");
        if let Ok(Some(speed)) = words::value(&unnumbered, 'S') {
            self.state.spindle_speed = Some(speed);
        }
        group_words(&unnumbered).iter().try_for_each(|words| self.words(words))
    }

    fn words(&mut self, line: &str) -> Result<(), ()> {
        if line.starts_with(['G', 'X', 'Y', 'Z', 'F']) {
            self.motion_words(line)?;
        }
        if line.starts_with('M') {
            let codes = words::numbers('M', line);
            let has = |code: f64| codes.contains(&code);
            self.state.spindle = [(3.0, "M3 "), (4.0, "M4 "), (5.0, "M5 ")]
                .into_iter()
                .find_map(|(code, word)| has(code).then_some(word))
                .or(self.state.spindle);
            if has(106.0) {
                self.state.laser = Some("M106 ");
            }
            if has(107.0) || has(16.0) {
                self.state.laser = Some("M107 ");
            }
            if has(6.0) {
                self.state.z = None;
            }
        }
        Ok(())
    }

    fn motion_words(&mut self, line: &str) -> Result<(), ()> {
        let codes = words::numbers('G', line);
        let has = |code: f64| codes.contains(&code);
        let state = &mut self.state;
        state.motion = (0..4u8).find(|&code| has(f64::from(code))).or(state.motion);
        if has(90.0) {
            state.positioning = Positioning::Absolute;
        }
        if has(91.0) {
            state.positioning = Positioning::Relative;
        }
        let units = if has(21.0) {
            Units::Mm
        } else if has(20.0) {
            Units::Inches
        } else {
            state.units
        };
        if units != state.units {
            let scale = if units == Units::Inches { 1.0 / 25.4 } else { 25.4 };
            self.home = (self.home.0 * scale, self.home.1 * scale);
            state.x *= scale;
            state.y *= scale;
            state.z = state.z.map(|z| z * scale);
            state.feed_rate = state.feed_rate.map(|feed| feed * scale);
            state.z_feed_rate *= scale;
            state.units = units;
        }
        let relative = state.positioning == Positioning::Relative;
        if line.contains('X') {
            let x = word(line, 'X')?;
            state.x = if relative { state.x + x } else { x + self.home.0 };
        }
        if line.contains('Y') {
            let y = word(line, 'Y')?;
            state.y = if relative { state.y + y } else { y + self.home.1 };
        }
        if has(38.2) {
            // A touching probe zeroes Z; a miss holds the job at the probe line.
            state.z = Some(0.0);
        } else if line.contains('Z') {
            let z = word(line, 'Z')?;
            state.z = if relative { state.z.map(|current| current + z) } else { Some(z) };
        }
        if has(4.0) {
            state.dwell = Some(py::strip(&DWELL_CODE.replace(line, "")).to_string());
        }
        if line.contains('F') {
            let feed = word(line, 'F')?;
            state.feed_rate = Some(feed);
            // A rapid's F is not a plunge feed, though the firmware keeps it as the modal F.
            if line.contains('Z') && !has(0.0) {
                state.z_feed_rate = feed;
            }
        }
        Ok(())
    }
}

/// A word Python reads with `float(re.search(...).groups()[0])`, which raises when it cannot.
fn word(line: &str, letter: char) -> Result<f64, ()> {
    words::value(line, letter)?.ok_or(())
}

/// Splits the line at spaces and regroups it so each part starts with an M, G or T word.
fn group_words(line: &str) -> Vec<String> {
    line.split(' ').filter(|part| !part.is_empty()).fold(Vec::new(), |mut groups: Vec<String>, part| {
        match groups.last_mut() {
            Some(last) if !part.starts_with(['M', 'G', 'T']) => {
                last.push(' ');
                last.push_str(part);
            }
            _ => groups.push(part.to_string()),
        }
        groups
    })
}

/// The lines a restart queues, and whether it stopped after an M6 the firmware will pause on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restart {
    pub lines: Vec<String>,
    pub stopped_at_tool_change: bool,
}

/// What a restart needs beyond the file's state.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Where to lower the bit to; the state's Z by default.
    pub z_target: Option<f64>,
    /// Stop after an M6 that changes from `current_tool`.
    pub change_tool: bool,
    pub current_tool: i64,
    pub safe_height_mm: f64,
    /// The bit's height before the restart, in mm, when known.
    pub current_z_mm: Option<f64>,
    /// 51.29x leaves the spindle's PWM at 0 after M3 or M4 until an S line.
    pub speed_after_spindle: bool,
}

/// `value` rounded up to `digits` places, ignoring float noise below them.
pub fn round_up(value: f64, digits: i32) -> f64 {
    let scale = 10f64.powi(digits);
    ((value * scale) - 1e-6).ceil() / scale
}

/// The height to travel at, in `units`: the safe height, or the bit's own height (rounded up) when that is higher.
pub fn clearance(units: Units, safe_height_mm: f64, current_z_mm: Option<f64>) -> f64 {
    let to_units = |mm: f64| if units == Units::Inches { mm / 25.4 } else { mm };
    let safe_height = to_units(safe_height_mm);
    current_z_mm.map(to_units).filter(|&z| z > safe_height).map_or(safe_height, |z| round_up(z, 4))
}

/// `queueRestartSequence`: rise to the travel height, move to the state's XY, restore tool, spindle, laser and dwell,
/// lower the bit to its Z target at the plunge feed, then restore the file's modal G0/G1, feed and positioning.
pub fn sequence(state: &GcodeState, options: &Options) -> Restart {
    let number = |value: f64| py::repr(py::round(value, 4));
    let travel_height = clearance(state.units, options.safe_height_mm, options.current_z_mm);
    let mut lines = vec![
        units_word(state.units).to_string(),
        "G90 ".to_string(),
        // G0 runs at the firmware's maximum and ignores F, but would still set the modal F.
        format!("G0 Z{} ", number(travel_height)),
        format!("G0 X{} Y{} ", number(state.x), number(state.y)),
    ];
    if let Some(tool) = state.tool.filter(|_| options.change_tool) {
        lines.push(format!("T{tool} M6 "));
        // Anything queued behind a pausing M6 would run, plunge included, as soon as a jog releases the firmware.
        if tool != options.current_tool {
            return Restart { lines, stopped_at_tool_change: true };
        }
    }
    lines.extend(state.spindle.map(str::to_string));
    let spindle_on = matches!(state.spindle, Some("M3 " | "M4 "));
    let speed = state.spindle_speed.filter(|_| spindle_on && options.speed_after_spindle);
    lines.extend(speed.map(|speed| format!("S{} ", number(speed))));
    lines.extend(state.laser.map(str::to_string));
    lines.extend(state.dwell.iter().map(|dwell| format!("G4 {dwell}")));
    let z = options.z_target.or(state.z);
    lines.extend(z.map(|z| format!("G1 Z{} F{} ", number(z), number(state.z_feed_rate))));
    // Bare axis words in the file continue its G0 or G1, not the restart's last move; an arc needs its own words.
    let motion = state.motion.filter(|&code| code < 2).map_or(String::new(), |code| format!("G{code} "));
    let last_move = if z.is_some() { "G1 " } else { "G0 " };
    match state.feed_rate {
        Some(feed) => lines.push(format!("{motion}F{} ", number(feed))),
        None if !motion.is_empty() && motion != last_move => lines.push(motion),
        None => {}
    }
    lines.push(positioning_word(state.positioning).to_string());
    Restart { lines, stopped_at_tool_change: false }
}

pub fn positioning_word(positioning: Positioning) -> &'static str {
    match positioning {
        Positioning::Absolute => "G90 ",
        Positioning::Relative => "G91 ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAFE_Z_MM: &str = "G0 Z5.0 ";

    fn file(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| format!("{line} ")).collect()
    }

    fn restart_with(lines: &[&str], index: usize, home: (f64, f64), units: Units, current_tool: i64) -> Vec<String> {
        let state = state_at(&file(lines), index, home.0, home.1, units).unwrap();
        sequence(&state, &Options { current_tool, ..options() }).lines
    }

    fn options() -> Options {
        Options {
            z_target: None,
            change_tool: true,
            current_tool: 0,
            safe_height_mm: 5.0,
            current_z_mm: None,
            speed_after_spindle: false,
        }
    }

    fn resumed_from(lines: &[&str], index: usize, options: Options) -> Vec<String> {
        sequence(&state_at(&file(lines), index, 0.0, 0.0, Units::Mm).unwrap(), &options).lines
    }

    #[test]
    fn the_travel_height_never_lowers_the_bit() {
        let at = |current_z_mm| resumed_from(&METRIC_FILE, 3, Options { current_z_mm, ..options() })[2].clone();
        assert_eq!(at(Some(10.0)), "G0 Z10.0 ");
        assert_eq!(at(Some(5.0)), SAFE_Z_MM);
        assert_eq!(at(Some(-3.0)), SAFE_Z_MM);
        assert_eq!(at(None), SAFE_Z_MM);
        assert_eq!(at(Some(10.00001)), "G0 Z10.0001 ");
    }

    #[test]
    fn the_travel_height_is_in_the_files_units() {
        let lines = ["G20", "G90", "G0 X1 Y2", "G1 Z-0.1", "G1 X3"];
        let raised = resumed_from(&lines, 4, Options { current_z_mm: Some(12.7), ..options() });
        assert_eq!(raised[..4], ["G20 ", "G90 ", "G0 Z0.5 ", "G0 X1.0 Y2.0 "]);
        assert_eq!(clearance(Units::Inches, 5.0, Some(1.0)), 5.0 / 25.4);
        assert_eq!(clearance(Units::Inches, 5.0, Some(5.08)), 0.2);
    }

    #[test]
    fn rounding_up_ignores_float_noise() {
        assert_eq!(round_up(23.14, 4), 23.14);
        assert_eq!(round_up(0.19685, 4), 0.1969);
        assert_eq!(round_up(-1.00001, 4), -1.0);
    }

    #[test]
    fn on_51_29x_the_spindle_gets_the_files_last_speed_on_its_own_line() {
        let on_51_29 = Options { speed_after_spindle: true, ..options() };
        let cases: [(&[&str], &[&str]); 5] = [
            (&["G90", "M3 S12000", "G1 X1"], &["M3 ", "S12000.0 "]),
            (&["G90", "S8000", "M4", "G1 X1"], &["M4 ", "S8000.0 "]),
            (&["G90", "M3 S1000", "S2000.5", "G1 X1"], &["M3 ", "S2000.5 "]),
            (&["G90", "M3 S1000", "M5", "G1 X1"], &["M5 "]),
            (&["G90", "M3", "G1 X1"], &["M3 "]),
        ];
        cases.iter().for_each(|(lines, restored)| {
            let queued = resumed_from(lines, lines.len() - 1, on_51_29.clone());
            let spindle = queued.iter().filter(|line| line.starts_with(['M', 'S'])).collect::<Vec<_>>();
            assert_eq!(spindle, *restored, "{lines:?}");
        });
        let elsewhere = resumed_from(&["G90", "M3 S12000", "G1 X1"], 2, options());
        assert!(elsewhere.iter().all(|line| !line.starts_with('S')), "{elsewhere:?}");
    }

    fn restart(lines: &[&str], index: usize) -> Vec<String> {
        restart_with(lines, index, (0.0, 0.0), Units::Mm, 0)
    }

    const METRIC_FILE: [&str; 4] = ["G90", "G0 X10 Y20", "G1 Z-3", "G1 X30 Y40"];

    #[test]
    fn restart_mid_file_keeps_machine_mm_when_file_has_no_units() {
        assert_eq!(restart(&METRIC_FILE, 3), ["G21 ", "G90 ", SAFE_Z_MM, "G0 X10.0 Y20.0 ", "G1 Z-3.0 F12.8 ", "G90 "]);
    }

    #[test]
    fn restart_mid_file_keeps_machine_inches_when_file_has_no_units() {
        assert_eq!(restart_with(&METRIC_FILE, 3, (0.0, 0.0), Units::Inches, 0)[..2], ["G20 ", "G90 "]);
    }

    #[test]
    fn restart_mid_file_uses_the_files_units_over_the_machines() {
        let lines = ["G20", "G90", "G0 X10 Y20", "G1 Z-3", "G1 X30 Y40"];
        assert_eq!(restart(&lines, 4), ["G20 ", "G90 ", "G0 Z0.1969 ", "G0 X10.0 Y20.0 ", "G1 Z-3.0 F0.5039 ", "G90 "]);
    }

    #[test]
    fn restart_converts_home_not_the_current_position_when_the_file_switches_units() {
        let lines = ["G21", "G0 X10 Y10", "G20", "G0 X1 Y1", "G1 Z-0.1"];
        assert_eq!(restart_with(&lines, 5, (25.4, 50.8), Units::Mm, 0)[3], "G0 X2.0 Y3.0 ");
    }

    #[test]
    fn a_metric_file_restarted_on_an_inch_machine_switches_it_with_g21() {
        let lines = ["G21", "G0 X25.4 Y25.4", "G1 Z-1"];
        assert_eq!(
            restart_with(&lines, 2, (2.0, 3.0), Units::Inches, 0)[..4],
            ["G21 ", "G90 ", SAFE_Z_MM, "G0 X76.2 Y101.6 "]
        );
    }

    #[test]
    fn restart_plunges_at_the_files_z_feed_then_restores_its_modal_feed() {
        let cases = [
            ("G1 Y10 F300", "G1 Z-1.0 F12.8 ", "G1 F300.0 "),
            ("G1 Z-2 F100", "G1 Z-2.0 F100.0 ", "G1 F100.0 "),
            ("G1 X10 Z-2 F250", "G1 Z-2.0 F250.0 ", "G1 F250.0 "),
            ("G0 Z-2 F1000", "G1 Z-2.0 F12.8 ", "G0 F1000.0 "),
        ];
        cases.iter().for_each(|&(line, plunge, feed)| {
            assert_eq!(restart(&["G90", "G1 Z-1", line, "G1 X1"], 3)[4..], [plunge, feed, "G90 "], "{line}");
        });
    }

    #[test]
    fn a_probe_leaves_z_at_zero_not_at_its_depth() {
        let cases: [&[&str]; 2] = [&["G90", "G0 Z5", "G38.2 Z-10 F100", "G0 X1"], &["G91", "G38.2 Z-10", "G0 X1"]];
        cases.iter().for_each(|lines| {
            let queued = restart(lines, lines.len() - 1);
            assert!(queued.iter().any(|line| line.starts_with("G1 Z0.0 ")), "{queued:?}");
        });
    }

    #[test]
    fn restart_sets_no_feed_when_the_file_has_set_none() {
        let cases: [(&[&str], &[&str]); 4] = [
            (&["G90", "G1 Z-1", "G1 X10", "G1 X1"], &["G1 Z-1.0 F12.8 ", "G90 "]),
            (&["G90", "G1 Z-1", "G0 X10", "X1"], &["G1 Z-1.0 F12.8 ", "G0 ", "G90 "]),
            (&["G90", "G1 X10 Y20", "X1"], &["G0 X10.0 Y20.0 ", "G1 ", "G90 "]),
            (&["G90", "G0 X10 Y20", "X1"], &["G0 X10.0 Y20.0 ", "G90 "]),
        ];
        cases.iter().for_each(|(lines, restored)| {
            let queued = restart(lines, lines.len() - 1);
            assert_eq!(queued[queued.len() - restored.len()..], **restored);
            assert!(queued.iter().filter(|line| !line.starts_with("G1 Z")).all(|line| !line.contains('F')));
        });
    }

    #[test]
    fn restart_before_the_file_sets_z_restores_the_feed_without_lowering() {
        let lines = ["G90", "G1 X10 Y20 F300", "G1 X1"];
        assert_eq!(restart(&lines, 2), ["G21 ", "G90 ", SAFE_Z_MM, "G0 X10.0 Y20.0 ", "G1 F300.0 ", "G90 "]);
    }

    #[test]
    fn restart_does_not_mistake_other_g4x_words_for_a_dwell() {
        ["G40", "G41 D1", "G43 H1"].iter().for_each(|word| {
            assert!(restart(&["G90", word, "G1 X1"], 2).iter().all(|line| !line.starts_with("G4")), "{word}");
        });
    }

    #[test]
    fn restart_repeats_the_last_dwell() {
        ["G4 P2", "G04 P2"]
            .iter()
            .for_each(|word| assert!(restart(&["G90", word, "G1 X1"], 2).contains(&"G4 P2".into())));
    }

    #[test]
    fn restart_does_not_mistake_longer_m_codes_for_spindle_on() {
        ["M30", "M300"]
            .iter()
            .for_each(|word| assert!(!restart(&["G90", "G0 X1", word, "G1 X2"], 3).contains(&"M3 ".into())));
    }

    #[test]
    fn restart_restores_the_spindle_direction_and_laser() {
        [("M04", "M4 "), ("M4", "M4 "), ("M05", "M5 "), ("M03 S1000", "M3 "), ("M106", "M106 "), ("M16", "M107 ")]
            .iter()
            .for_each(|&(word, restored)| {
                assert!(restart(&["G90", word, "G1 X2"], 2).contains(&restored.into()), "{word}")
            });
    }

    #[test]
    fn restart_does_not_mistake_g91_1_arc_centres_for_relative_moves() {
        let queued = restart(&["G90", "G91.1", "G0 X10 Y20", "G0 X30 Y40", "G1 Z-1"], 5);
        assert_eq!(queued[3], "G0 X30.0 Y40.0 ");
        assert_eq!(queued.last().unwrap(), "G90 ");
    }

    #[test]
    fn restart_converts_the_default_plunge_feed_to_inches() {
        let queued = restart_with(&["G90", "G0 X1 Y1", "G1 Z-0.1", "G1 X2"], 3, (0.0, 0.0), Units::Inches, 0);
        assert_eq!(queued[2..], ["G0 Z0.1969 ", "G0 X1.0 Y1.0 ", "G1 Z-0.1 F0.5039 ", "G90 "]);
    }

    #[test]
    fn restart_converts_z_and_feeds_when_the_file_switches_units() {
        let lines = ["G21", "G1 Z-2.54 F254", "G1 X25.4 F508", "G20", "G1 X2"];
        assert_eq!(
            restart(&lines, 4),
            ["G20 ", "G90 ", "G0 Z0.1969 ", "G0 X1.0 Y0.0 ", "G1 Z-0.1 F10.0 ", "G1 F20.0 ", "G90 "]
        );
    }

    #[test]
    fn restart_follows_modal_and_numbered_moves_without_a_g_word() {
        let modal = ["G90", "G1 X10 Y20 F300", "X30 Y40", "Y50", "G1 Z-1"];
        assert_eq!(restart(&modal, 4)[3..], ["G0 X30.0 Y50.0 ", "G1 F300.0 ", "G90 "]);
        let numbered = ["N1 G90", "N2 G1 X10 Y20 F300", "N3 X30 Y40", "N4 G1 Z-1"];
        assert_eq!(restart(&numbered, 3)[3..], ["G0 X30.0 Y40.0 ", "G1 F300.0 ", "G90 "]);
        assert_eq!(
            restart(&["G90", "G1 F450", "G1 X10 Y20", "G1 X30"], 3)[3..],
            ["G0 X10.0 Y20.0 ", "G1 F450.0 ", "G90 "]
        );
    }

    #[test]
    fn restart_restores_the_files_modal_g0_or_g1_for_bare_axis_lines() {
        let cases: [(&[&str], &str); 4] = [
            (&["G90", "G1 Z-1 F100", "G0 X10 Y20", "X30"], "G0 F100.0 "),
            (&["G90", "G0 X10 Y20", "G1 Z-1 F100", "X30"], "G1 F100.0 "),
            (&["G90", "G1 Z-1 F100", "G2 X10 Y20 I5 J0", "X30"], "F100.0 "),
            (&["G90", "F100", "G4 P1", "G90"], "F100.0 "),
        ];
        cases.iter().for_each(|(lines, modal)| {
            let queued = restart(lines, 3);
            assert_eq!(queued[queued.len() - 2..], [modal.to_string(), "G90 ".to_string()]);
        });
    }

    #[test]
    fn a_resume_lowers_to_its_own_z_target_and_never_changes_tool() {
        let lines = file(&["T2 M6", "G90", "G0 X10 Y20", "G1 X30"]);
        let state = state_at(&lines, 3, 0.0, 0.0, Units::Mm).unwrap();
        let resumed = sequence(&state, &Options { z_target: Some(1.5), change_tool: false, ..options() });
        assert_eq!(resumed.lines[4], "G1 Z1.5 F12.8 ");
        assert!(resumed.lines.iter().all(|line| !line.contains("M6")));
        assert_eq!(state_at(&file(&["G91", "G0 X10 Y20", "G1 Z-3", "G1 X5"]), 3, 0.0, 0.0, Units::Mm).unwrap().z, None);
    }

    const TOOL_CHANGE_FILE: [&str; 13] = [
        "G21",
        "G90",
        "G0 Z5",
        "G0 X10 Y10",
        "G1 Z-3",
        "G1 X20",
        "G0 Z5",
        "G0 X0 Y0",
        "T2 M6",
        "G0 X40 Y40",
        "G0 Z5",
        "G1 Z-2",
        "G1 X50",
    ];

    fn lowered_to(queued: &[String]) -> Vec<&String> {
        queued
            .iter()
            .filter(|line| (line.starts_with("G0 Z") || line.starts_with("G1 Z")) && *line != SAFE_Z_MM)
            .collect()
    }

    #[test]
    fn a_restart_before_the_file_sets_z_stays_at_safe_height() {
        assert!(lowered_to(&restart(&["G21", "G90", "G0 X10 Y10", "G1 X20 Y20"], 3)).is_empty());
    }

    #[test]
    fn a_restart_after_a_tool_change_stops_at_the_m6_and_stays_at_safe_height() {
        let state = state_at(&file(&TOOL_CHANGE_FILE), 9, 0.0, 0.0, Units::Mm).unwrap();
        let restart = sequence(&state, &options());
        assert!(restart.stopped_at_tool_change);
        assert_eq!(restart.lines.last().unwrap(), "T2 M6 ");
        assert!(lowered_to(&restart.lines).is_empty());
        assert!(lowered_to(&sequence(&state, &Options { change_tool: false, ..options() }).lines).is_empty());
    }

    #[test]
    fn a_restart_once_the_new_tool_has_a_z_lowers_to_it() {
        let queued = restart_with(&TOOL_CHANGE_FILE, 12, (0.0, 0.0), Units::Mm, 2);
        assert_eq!(lowered_to(&queued), ["G1 Z-2.0 F12.8 "]);
        assert!(queued.contains(&"T2 M6 ".into()));
    }

    #[test]
    fn the_restart_changes_to_the_tool_the_last_m6_changed_to() {
        let tool_of = |lines: &[&str]| state_at(&file(lines), lines.len(), 0.0, 0.0, Units::Mm).unwrap().tool;
        assert_eq!(tool_of(&["G21", "T2", "M6", "G0 X1"]), Some(2));
        assert_eq!(tool_of(&["G21", "M6 T3", "G0 X1"]), Some(3));
        assert_eq!(tool_of(&["G21", "T2", "M6", "T5", "G0 X1"]), Some(2));
        assert_eq!(tool_of(&["G21", "G0 X1 T4", "M6", "G0 X1"]), Some(4));
        assert_eq!(tool_of(&["G21", "M6"]), None);
    }

    #[test]
    fn unreadable_words_and_indices_past_the_end_are_errors() {
        assert_eq!(state_at(&file(&["G1 X. Y2"]), 1, 0.0, 0.0, Units::Mm), Err(GcodeError::Unreadable { index: 0 }));
        assert_eq!(
            state_at(&file(&["G1 X1"]), 2, 0.0, 0.0, Units::Mm),
            Err(GcodeError::IndexOutOfRange { index: 2, len: 1 })
        );
        assert!(state_at(&file(&["(c)", "G1 F"]), 2, 0.0, 0.0, Units::Mm).is_err());
    }
}

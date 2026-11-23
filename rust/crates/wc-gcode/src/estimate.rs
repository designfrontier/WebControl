//! How long a file's moves take at the feeds the firmware allows (`spec/firmware.md` §5), plus a fixed cost per move,
//! for the job timer's estimate. The timer scales this by the job's real progress.

use std::f64::consts::TAU;

use crate::arc::{ArcCentres, RADIUS_TOLERANCE_MM, centre_from_radius};
use crate::{Units, words};

/// Seconds each move costs on top of its feed time, as the firmware stops at the end of every line; fitted to the
/// owner's jobs on 51.292.
pub const MOVE_S: f64 = 0.015;

/// In mm/min: G0's speed and every feed's cap (`$15`), and the cap on Z's share of a move (`$18 × |$19|`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub max_feed: f64,
    pub max_z_feed: f64,
}

/// Seconds from the start through the end of each line, from the origin in `units`; a G1 before any F runs at `$15`.
pub fn cumulative_seconds(lines: &[String], units: Units, rates: Rates) -> Vec<f32> {
    let max_feed = rates.max_feed.max(1.0);
    let mut modal = Modal {
        position: [0.0; 3],
        scale: if units == Units::Inches { 25.4 } else { 1.0 },
        relative: false,
        centres: ArcCentres::Offsets,
        motion: 0,
        feed: max_feed,
        rates: Rates { max_feed, ..rates },
    };
    lines
        .iter()
        .scan(0.0, |total, line| {
            *total += modal.seconds(line);
            Some(*total as f32)
        })
        .collect()
}

struct Modal {
    position: [f64; 3],
    scale: f64,
    relative: bool,
    centres: ArcCentres,
    motion: u8,
    feed: f64,
    rates: Rates,
}

impl Modal {
    fn seconds(&mut self, raw: &str) -> f64 {
        let code = words::code(raw);
        let mut dwell = false;
        words::numbers('G', &code).into_iter().for_each(|number| match number {
            0.0 | 1.0 | 2.0 | 3.0 => self.motion = number as u8,
            4.0 => dwell = true,
            20.0 => self.scale = 25.4,
            21.0 => self.scale = 1.0,
            90.0 => self.relative = false,
            91.0 => self.relative = true,
            _ => self.centres = ArcCentres::set_by(number).unwrap_or(self.centres),
        });
        let value = |letter| words::value(&code, letter).ok().flatten();
        if dwell {
            // The firmware's G4 P is milliseconds; S counts only without P.
            let ms = value('P').map_or(0.0, f64::abs);
            return if ms > 0.0 { ms / 1000.0 } else { value('S').map_or(0.0, f64::abs) };
        }
        if let Some(feed) = value('F') {
            self.feed = (feed * self.scale).clamp(1.0, self.rates.max_feed);
        }
        let axes = [value('X'), value('Y'), value('Z')];
        let centred = self.motion >= 2 && ['I', 'J', 'R'].into_iter().any(|letter| value(letter).is_some());
        if axes.iter().all(Option::is_none) && !centred {
            return 0.0;
        }
        let start = self.position;
        let end: [f64; 3] = std::array::from_fn(|axis| match axes[axis] {
            Some(v) if self.relative => start[axis] + v * self.scale,
            Some(v) => v * self.scale,
            None => start[axis],
        });
        self.position = end;
        let length = match self.motion {
            2 | 3 => self.arc_mm(&code, start, end),
            _ => ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2) + (end[2] - start[2]).powi(2)).sqrt(),
        };
        let feed = if self.motion == 0 { self.rates.max_feed } else { self.feed };
        let z_minutes =
            if self.rates.max_z_feed > 0.0 { (end[2] - start[2]).abs() / self.rates.max_z_feed } else { 0.0 };
        (length / feed).max(z_minutes) * 60.0 + MOVE_S
    }

    /// The arc's length in XY, as the firmware sweeps it: a full turn when it ends where it starts.
    fn arc_mm(&self, code: &str, start: [f64; 3], end: [f64; 3]) -> f64 {
        let value = |letter| words::value(code, letter).ok().flatten().map(|v| v * self.scale);
        let clockwise = self.motion == 2;
        let (from, to) = ([start[0], start[1]], [end[0], end[1]]);
        let chord = (to[0] - from[0]).hypot(to[1] - from[1]);
        let centre = match (value('I'), value('J'), value('R')) {
            (None, None, Some(radius)) => centre_from_radius(from, to, radius, clockwise, RADIUS_TOLERANCE_MM).ok(),
            (None, None, None) => None,
            (i, j, _) if self.centres == ArcCentres::Absolute => Some([i.unwrap_or(0.0), j.unwrap_or(0.0)]),
            (i, j, _) => Some([from[0] + i.unwrap_or(0.0), from[1] + j.unwrap_or(0.0)]),
        };
        let Some(centre) = centre else { return chord };
        let radius = (from[0] - centre[0]).hypot(from[1] - centre[1]);
        let theta = (to[1] - centre[1]).atan2(to[0] - centre[0]) - (from[1] - centre[1]).atan2(from[0] - centre[0]);
        let sweep = match clockwise {
            true if theta >= 0.0 => theta - TAU,
            false if theta <= 0.0 => theta + TAU,
            _ => theta,
        };
        sweep.abs() * radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: Rates = Rates { max_feed: 1000.0, max_z_feed: 60.0 };

    /// The file's seconds without the cost of its `moves` moves.
    fn total(text: &str, moves: usize) -> f64 {
        total_in(text, Units::Mm, moves)
    }

    fn total_in(text: &str, units: Units, moves: usize) -> f64 {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        let seconds = cumulative_seconds(&lines, units, RATES).last().copied().map_or(0.0, f64::from);
        seconds - moves as f64 * MOVE_S
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 0.01, "{actual} is not {expected}");
    }

    #[test]
    fn lines_run_at_their_feed_and_rapids_at_the_max_feed() {
        close(total("G1 X100 F600", 1), 10.0);
        close(total("G0 X100", 1), 6.0);
        close(total("G0 X100 F10", 1), 6.0);
        close(total("G1 X30 Y40 F300", 1), 10.0);
    }

    #[test]
    fn the_feed_is_modal_capped_and_set_by_any_motion() {
        close(total("G1 X10 F60\nX20\nG1 X30", 3), 30.0);
        close(total("G1 X1000 F5000", 1), 60.0);
        close(total("G0 X0 F60\nG1 X10", 2), 10.0);
        close(total("F60\nG1 X10", 1), 10.0);
        close(total("G1 X100", 1), 6.0);
        close(total("G1 X1 F0", 1), 60.0);
    }

    #[test]
    fn each_line_adds_to_the_running_total() {
        let lines: Vec<String> = ["G1 X10 F60", "(comment)", "G1 X20", "M3"].map(String::from).to_vec();
        assert_eq!(cumulative_seconds(&lines, Units::Mm, RATES), vec![10.015, 10.015, 20.03, 20.03]);
    }

    #[test]
    fn inches_scale_coordinates_and_feeds() {
        close(total("G20\nG1 X1 F1", 1), 60.0);
        close(total_in("G1 X1 F1", Units::Inches, 1), 60.0);
        close(total_in("G21\nG1 X25.4 F25.4", Units::Inches, 1), 60.0);
        close(total("G20 G1 X10 F10\nG21 G1 X508", 2), 120.0);
    }

    #[test]
    fn relative_moves_add_to_the_position() {
        close(total("G91\nG1 X10 F60\nX10\nG90\nX20", 3), 20.0);
    }

    #[test]
    fn z_is_capped_at_its_own_feed() {
        close(total("G1 Z-6 F600", 1), 6.0);
        close(total("G0 Z6", 1), 6.0);
        close(total("G1 X100 Z-1 F600", 1), 10.0);
        let no_cap = Rates { max_z_feed: 0.0, ..RATES };
        close(f64::from(cumulative_seconds(&["G1 Z-6 F600".into()], Units::Mm, no_cap)[0]), 0.6 + MOVE_S);
    }

    #[test]
    fn arcs_take_their_length() {
        let half = std::f64::consts::PI * 10.0;
        close(total("G0 X10\nG3 X-10 Y0 I-10 J0 F60", 2) - 0.6, half);
        close(total("G0 X10\nG2 X-10 Y0 I-10 J0 F60", 2) - 0.6, half);
        close(total("G0 X10\nG3 X0 Y10 I-10 J0 F60", 2) - 0.6, half / 2.0);
        close(total("G0 X10\nG2 X0 Y10 I-10 J0 F60", 2) - 0.6, half * 1.5);
    }

    #[test]
    fn an_arc_back_to_its_start_is_a_full_circle() {
        let full = std::f64::consts::TAU * 10.0;
        close(total("G0 X10\nG2 X10 Y0 I-10 J0 F60", 2) - 0.6, full);
        close(total("G0 X10\nG3 I-10 F60", 2) - 0.6, full);
    }

    #[test]
    fn arc_centres_follow_g90_1_and_radius() {
        let half = std::f64::consts::PI * 10.0;
        close(total("G90.1\nG0 X20\nG3 X0 Y0 I10 J0 F60", 2) - 1.2, half);
        close(total("G0 X10\nG3 X-10 Y0 R10 F60", 2) - 0.6, half);
        close(total("G0 X10\nG3 X0 Y10 R-10 F60", 2) - 0.6, half * 1.5);
        close(total("G0 X10\nG3 X-10 Y0 R1 F60", 2) - 0.6, 20.0);
        close(total("G0 X10\nG3 X-10 Y0 F60", 2) - 0.6, 20.0);
    }

    #[test]
    fn helical_arcs_wait_for_z() {
        close(total("G0 X10\nG3 X-10 Y0 I-10 J0 Z-60 F6000", 2) - 0.6, 60.0);
    }

    #[test]
    fn dwell_is_milliseconds_or_seconds() {
        close(total("G4 P1500", 0), 1.5);
        close(total("G4 S2", 0), 2.0);
        close(total("G4 P0 S2", 0), 2.0);
        close(total("G4 X10", 0), 0.0);
    }

    #[test]
    fn other_lines_take_no_time() {
        close(total("M6 T2\nM3 S12000\n(G1 X100 F1)\n; G1 X100\n\nG17", 0), 0.0);
    }

    /// `moves` zigzags at F600 that take `feed_s` at their feed alone.
    fn job(moves: usize, feed_s: f64) -> Vec<String> {
        let length = feed_s / moves as f64 * 10.0;
        let zigzag = (0..moves).map(|n| format!("G1 X{} F600", if n % 2 == 0 { length } else { 0.0 }));
        zigzag.collect()
    }

    #[test]
    fn the_owners_jobs_come_within_15_percent() {
        let owner = Rates { max_feed: 800.0, max_z_feed: 12.6 * 24.0 };
        // Each mancala job's moves and their feed time without MOVE_S, against how long it really took on the Pi.
        [(45_232, 3559.0, 4188.5), (120_387, 8299.0, 10_344.5), (122, 389.0, 404.8)].into_iter().for_each(
            |(moves, feed_s, real_s)| {
                let estimate = f64::from(*cumulative_seconds(&job(moves, feed_s), Units::Mm, owner).last().unwrap());
                assert!((estimate / real_s - 1.0).abs() < 0.15, "{moves} moves: {estimate} s against {real_s} s");
            },
        );
    }
}

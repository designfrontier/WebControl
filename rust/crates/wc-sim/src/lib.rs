//! A Maslow controller as `spec/firmware.md` describes its serial side: the 127-byte ring, `ok` when a line
//! finishes, `!`/`~`, M0/M1 and M6 pauses, `$$` and `$n=` per build, B05, boot banners, status reports and alarms.
//! [`Sim`] is sans IO; [`spawn`] puts it behind an in-memory serial port for the shell, e2e and `--simulate`.

mod port;

pub use port::spawn;

use std::collections::{BTreeMap, VecDeque};

use wc_firmware::number::{in_firmware_range, read_float};

/// The firmware's `INCBUFFERLENGTH 128` ring holds 127 bytes.
pub const RING_BYTES: usize = 127;
const STATUS_INTERVAL_MS: u64 = 200;
/// Kp, Ki, Kd and the proportional weight of the main position, main velocity, Z position and Z velocity PIDs.
const TUNINGS: [[u32; 4]; 4] = [[21, 22, 23, 24], [25, 26, 27, 28], [29, 30, 31, 32], [33, 34, 35, 36]];

/// Which firmware the simulator pretends to be.
#[derive(Debug, Clone, PartialEq)]
pub struct Build {
    /// What B05 reports.
    pub version: String,
    /// 51.292 prints a notice after every `$n=` write.
    pub rewrite_notice: bool,
    pub banner: String,
    /// The EEPROM layout; a flash to another one boots with `EEPROM read fail` and factory settings.
    pub settings_version: u8,
}

impl Build {
    pub fn holey_51_27() -> Build {
        Build {
            version: "51.27".into(),
            rewrite_notice: false,
            banner: "PCB v1.6 TLE9201  Detected".into(),
            settings_version: 6,
        }
    }

    pub fn holey_51_292() -> Build {
        Build { version: "51.29".into(), rewrite_notice: true, settings_version: 7, ..Build::holey_51_27() }
    }

    /// The same firmware on another board, which prints its own banner.
    pub fn on_board(self, banner: &str) -> Build {
        Build { banner: banner.into(), ..self }
    }
}

/// A line the firmware holds until `~`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hold {
    Pause,
    ToolChange,
}

#[derive(Debug, Clone)]
pub struct Sim {
    build: Build,
    settings: BTreeMap<u32, f64>,
    eeprom_reset: bool,
    partial: Vec<u8>,
    overflowing: bool,
    queue: VecDeque<String>,
    finishes_at: Option<u64>,
    held: Option<Hold>,
    after_realtime: bool,
    mm: bool,
    absolute: bool,
    position: [f64; 3],
    tool: u32,
    next_status: u64,
    max_unread: usize,
    overflows: usize,
    /// How long a move takes.
    pub move_ms: u64,
    /// Whether a G38.2 probe touches the plate.
    pub probe_touches: bool,
    /// `$` setting zAxisAttached: without it G38.2 acks without probing.
    pub z_axis_attached: bool,
    /// Whether it prints a status report every 200 ms.
    pub reports_status: bool,
    /// What B10 measures for the left and right chains.
    pub chains: (f64, f64),
    /// A motor whose encoder B04 finds not moving.
    pub stuck_motor: Option<char>,
    /// `$38` was written since the boot: `setupAxes()` ran on the live board and the motors no longer follow.
    axes_reinitialised: bool,
    /// Which of `TUNINGS` holds gains: `setup()` zeroes them and `SetTunings` replaces them only with a tuning it accepts.
    tuned: [bool; 4],
}

/// The 51.27 defaults a fresh EEPROM boots with.
fn default_settings(build: &Build) -> BTreeMap<u32, f64> {
    let z_rpm = if build.rewrite_notice { 126.0 } else { 12.6 };
    [
        (0, 2438.4),
        (1, 1219.2),
        (2, 2978.4),
        (3, 463.0),
        (4, 310.0),
        (5, 139.0),
        (6, 79.0),
        (7, 1.0),
        (8, 139.1),
        (9, 2000.0),
        (10, 3360.0),
        (11, 1651.0),
        (12, 8113.73),
        (13, 63.5),
        (15, 700.0),
        (16, 1.0),
        (17, 0.0),
        (18, z_rpm),
        (19, 3.17),
        (20, 7560.0),
        (45, 0.0000051685),
        (46, 97.9),
    ]
    .into_iter()
    .collect()
}

fn word(line: &str, letter: char) -> Option<f64> {
    let at = line.find(letter)? + 1;
    let rest = &line[at..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn g_codes(line: &str) -> Vec<String> {
    line.match_indices('G')
        .map(|(at, _)| line[at + 1..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<String>())
        .filter(|code| !code.is_empty())
        .map(|code| code.trim_start_matches('0').to_string())
        .map(|code| if code.is_empty() || code.starts_with('.') { format!("0{code}") } else { code })
        .collect()
}

/// The tool the firmware reads: a T leading a line that also has a G or M word.
fn leading_tool(line: &str) -> Option<u32> {
    let text = line.trim();
    let rest = text.strip_prefix('T')?;
    let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
    text.contains(['G', 'M']).then(|| number.parse().ok()).flatten()
}

/// The number after each `M`, as the firmware reads it: `M06` is 6, `M6.1` is not.
fn m_codes(text: &str) -> impl Iterator<Item = f64> + '_ {
    text.match_indices('M').filter_map(|(at, _)| {
        let rest = &text[at + 1..];
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        rest[..end].parse().ok()
    })
}

impl Sim {
    pub fn new(build: Build) -> Sim {
        Sim {
            settings: default_settings(&build),
            build,
            eeprom_reset: false,
            partial: Vec::new(),
            overflowing: false,
            queue: VecDeque::new(),
            finishes_at: None,
            held: None,
            after_realtime: false,
            mm: true,
            absolute: true,
            position: [0.0; 3],
            tool: 0,
            next_status: STATUS_INTERVAL_MS,
            max_unread: 0,
            overflows: 0,
            move_ms: 0,
            probe_touches: true,
            z_axis_attached: true,
            reports_status: true,
            chains: (1650.0, 1651.0),
            stuck_motor: None,
            axes_reinitialised: false,
            tuned: [true; 4],
        }
    }

    /// A value its EEPROM already holds, as `$$` reports it.
    pub fn with_setting(mut self, key: u32, value: f64) -> Sim {
        self.settings.insert(key, value);
        self
    }

    /// The next boot reports `EEPROM read fail`, as after a flash to another settings version.
    pub fn with_eeprom_reset(mut self) -> Sim {
        self.eeprom_reset = true;
        self
    }

    /// What the board prints when the port opens and resets it.
    pub fn boot(&mut self, now: u64) -> Vec<String> {
        let eeprom = std::mem::take(&mut self.eeprom_reset);
        let fresh = Sim::new(self.build.clone());
        *self = Sim {
            settings: std::mem::take(&mut self.settings),
            move_ms: self.move_ms,
            probe_touches: self.probe_touches,
            z_axis_attached: self.z_axis_attached,
            reports_status: self.reports_status,
            chains: self.chains,
            stuck_motor: self.stuck_motor,
            next_status: now + STATUS_INTERVAL_MS,
            ..fresh
        };
        let mut lines = vec![self.build.banner.clone(), "FAKE_SERVO off".into()];
        if eeprom {
            self.settings = default_settings(&self.build);
            lines.push("error: EEPROM read fail. Using default settings.".into());
        }
        self.setup_axes();
        lines.extend(["Grbl v1.00", "ready", "ok"].map(String::from));
        lines
    }

    /// Whether `PID::SetTunings` takes the stored tuning: no negative gain, a weight within [0, 1].
    fn accepts(&self, tuning: &[u32; 4]) -> bool {
        tuning.iter().all(|key| self.settings.get(key).is_none_or(|value| in_firmware_range(*key, *value)))
    }

    /// `setupAxes()`: every PID starts at zero gains, then takes the stored tuning if it accepts it.
    fn setup_axes(&mut self) {
        self.tuned = TUNINGS.map(|tuning| self.accepts(&tuning));
    }

    /// `setPIDValues()` after a write of `$21`-`$36`: a refused tuning leaves the gains the PID had.
    fn set_pid_values(&mut self, key: u32) {
        let accepted = TUNINGS.map(|tuning| self.accepts(&tuning));
        let group = if key <= 28 { 0..2 } else { 2..4 };
        group.for_each(|i| self.tuned[i] |= accepted[i]);
    }

    fn unread(&self) -> usize {
        self.partial.len() + self.queue.iter().map(|l| l.len() + 1).sum::<usize>()
    }

    /// Bytes from the host; returns the lines the firmware prints in reply.
    pub fn receive(&mut self, bytes: &[u8], now: u64) -> Vec<String> {
        let mut out = Vec::new();
        bytes.iter().for_each(|&byte| self.byte(byte, now, &mut out));
        out.extend(self.run(now));
        out
    }

    fn byte(&mut self, byte: u8, now: u64, out: &mut Vec<String>) {
        let after_realtime = std::mem::take(&mut self.after_realtime);
        match byte {
            b'!' => {
                self.after_realtime = true;
                self.stop();
                out.push("ok".into());
            }
            b'~' => {
                self.after_realtime = true;
                out.push("ok".into());
                if self.held.take().is_some() {
                    self.finishes_at = Some(now);
                }
            }
            b'\n' if after_realtime => {}
            b'\r' => {}
            b'\n' => {
                let line = String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned();
                match std::mem::take(&mut self.overflowing) {
                    // The firmware runs the unread tail of an over-long line as a command of its own.
                    true => self.queue.push_front(line),
                    false => self.queue.push_back(line),
                }
            }
            _ if self.unread() + 1 > RING_BYTES => {
                self.overflows += 1;
                self.stop();
                self.overflowing = true;
                self.partial.push(byte);
                out.push("Buffer overflow!".into());
            }
            _ => self.partial.push(byte),
        }
        self.max_unread = self.max_unread.max(self.unread());
    }

    fn stop(&mut self) {
        self.queue.clear();
        self.partial.clear();
        self.finishes_at = None;
        self.held = None;
    }

    /// Lets time pass: finishes lines and prints status reports.
    pub fn tick(&mut self, now: u64) -> Vec<String> {
        let mut out = self.run(now);
        if self.reports_status && now >= self.next_status {
            self.next_status = now + STATUS_INTERVAL_MS;
            out.push(self.status());
        }
        out
    }

    pub fn status(&self) -> String {
        let scale = if self.mm { 1.0 } else { 25.4 };
        let [x, y, z] = self.position.map(|v| v / scale);
        let mode = if self.held.is_some() { "Pause" } else { "Idle" };
        format!("<{mode},MPos:{x:.2},{y:.2},{z:.2},WPos:0.000,0.000,0.000>")
    }

    /// Finishes the running line when its time is up, then starts the next.
    fn run(&mut self, now: u64) -> Vec<String> {
        let mut out = Vec::new();
        loop {
            match self.finishes_at {
                Some(at) if at <= now && self.held.is_none() => {
                    self.finishes_at = None;
                    out.push("ok".into());
                }
                Some(_) => return out,
                None => {}
            }
            if self.held.is_some() {
                return out;
            }
            let Some(line) = self.queue.pop_front() else {
                return out;
            };
            if let Some(duration) = self.execute(&line, &mut out) {
                self.finishes_at = Some(now + duration);
            }
        }
    }

    /// Runs one line; `None` when it never acks.
    fn execute(&mut self, line: &str, out: &mut Vec<String>) -> Option<u64> {
        let text = line.trim();
        if text == "$$" {
            out.extend(self.settings.iter().map(|(key, value)| format!("${key}={value:.8}")));
            return Some(0);
        }
        if let Some((key, value)) = text.strip_prefix('$').and_then(|t| t.split_once('=')) {
            if key.starts_with("RST") {
                return None;
            }
            match (key.parse::<u32>(), read_float(value)) {
                (Ok(key), Some(value)) => {
                    self.settings.insert(key, f64::from(value));
                    if key == 38 {
                        self.axes_reinitialised = true;
                        self.setup_axes();
                    }
                    if (21..=36).contains(&key) {
                        self.set_pid_values(key);
                    }
                    if self.build.rewrite_notice {
                        out.push("Change in setting, rewriting EEPROM".into());
                    }
                }
                (Ok(_), None) => out.push("error: Bad number format".into()),
                _ => out.push("error: Invalid statement".into()),
            }
            return Some(0);
        }
        if text.starts_with("B05") {
            out.push(format!("Firmware Version {}", self.build.version));
            return Some(0);
        }
        if text.starts_with("B10") {
            let chain = if text.contains('L') { self.chains.0 } else { self.chains.1 };
            out.push(format!("[Measure: {chain:.2}]"));
            return Some(0);
        }
        if text.starts_with("B04") {
            out.extend(self.motor_test());
            return Some(0);
        }
        if text.starts_with('B') {
            return Some(0);
        }
        self.gcode(text, out)
    }

    fn gcode(&mut self, text: &str, out: &mut Vec<String>) -> Option<u64> {
        let codes = g_codes(text);
        let has = |code: &str| codes.iter().any(|c| c == code);
        let scale = if self.mm { 1.0 } else { 25.4 };
        if has("20") || has("21") {
            self.mm = has("21");
        }
        if has("90") || has("91") {
            self.absolute = has("90");
        }
        if has("38.2") {
            return self.probe(out);
        }
        if has("10") {
            self.position[2] = 0.0;
            return Some(0);
        }
        if let Some(tool) = leading_tool(text) {
            self.tool_change_to(tool, text, out)?;
        }
        if m_codes(text).any(|m| m == 0.0 || m == 1.0) {
            out.push("Maslow Paused".into());
            self.held = Some(Hold::Pause);
            return Some(0);
        }
        let axes = ['X', 'Y', 'Z'].map(|axis| word(text, axis));
        let moved = axes.iter().any(Option::is_some);
        let untuned = !(self.tuned[0] && self.tuned[1]);
        if moved && (self.axes_reinitialised || untuned) {
            // The setpoint runs ahead of encoders that never move (the owner's machine, 2026-11-15).
            self.stop();
            out.push("ALARM: The sled is not keeping up with its expected position and has halted.".into());
            return None;
        }
        // Only L and R raise the keeping-up alarm (Report.cpp); a Z with zero gains just stays where it is.
        let z_follows = self.tuned[2] && self.tuned[3];
        axes.iter().enumerate().filter(|(i, _)| *i < 2 || z_follows).for_each(|(i, value)| {
            if let Some(value) = value {
                self.position[i] = if self.absolute { value * scale } else { self.position[i] + value * scale };
            }
        });
        // `$16=0` (no Z motor): the firmware asks for each Z move by hand and pauses until `~`.
        if axes[2].is_some() && self.settings.get(&16) == Some(&0.0) {
            out.push(format!("Message: Please adjust Z-Axis to a depth of {:.2} mm", self.position[2]));
            out.push("Maslow Paused".into());
            self.held = Some(Hold::Pause);
        }
        codes
            .iter()
            .filter(|c| !matches!(c.as_str(), "0" | "1" | "2" | "3" | "4" | "20" | "21" | "90" | "91"))
            .for_each(|c| out.push(format!("Command G{c} unsupported and ignored.")));
        Some(if moved { self.move_ms } else { 0 })
    }

    fn tool_change_to(&mut self, tool: u32, text: &str, out: &mut Vec<String>) -> Option<()> {
        if !m_codes(text).any(|m| m == 6.0) || tool == self.tool {
            return Some(());
        }
        self.tool = tool;
        out.push(format!("Tool Change: Please insert tool {tool}"));
        out.push("Maslow Paused".into());
        self.held = Some(Hold::ToolChange);
        Some(())
    }

    fn probe(&mut self, out: &mut Vec<String>) -> Option<u64> {
        if !self.z_axis_attached {
            out.push("G38.2 gcode only valid with z-axis attached".into());
            return Some(0);
        }
        out.extend(["probing for z axis zero", "max depth -10.00 mm.", "feedrate 39.94 mm per min."].map(String::from));
        if self.probe_touches {
            self.position[2] = 0.0;
            out.push("z axis zeroed".into());
            return Some(0);
        }
        out.extend(["error: probe did not connect", "program stopped", "z axis not set", ""].map(String::from));
        // `sys.stop`: the firmware drops what it held, as for `!`.
        self.stop();
        None
    }

    /// `Axis::test` for L, R and Z: each status report is printed with no newline, so it glues onto the next line.
    fn motor_test(&self) -> Vec<String> {
        const REPORT: &str = "<Idle,MPos:0,0,0,WPos:0.000,0.000,0.000>";
        let mut glue = "";
        let mut lines: Vec<String> = ['L', 'R', 'Z']
            .iter()
            .flat_map(|&motor| {
                let verdict = if self.stuck_motor == Some(motor) { "Fail" } else { "Pass" };
                let first = format!("{glue}Testing {motor} motor:");
                glue = REPORT;
                [first, format!("{REPORT}Direction 1 - {verdict}"), format!("{REPORT}Direction 2 - {verdict}")]
            })
            .collect();
        lines.push(format!("{REPORT}Tests complete."));
        lines
    }

    /// Loads another build, as avrdude does; the EEPROM is untouched, so a new layout reads as a failure at boot.
    pub fn flash(&mut self, build: Build) {
        self.eeprom_reset = self.eeprom_reset || build.settings_version != self.build.settings_version;
        self.build = build;
    }

    /// The firmware stops itself on an alarm and never acks the lines it held.
    pub fn alarm(&mut self, text: &str) -> Vec<String> {
        self.stop();
        vec![format!("ALARM: {text}")]
    }

    /// The machine position in mm.
    pub fn position(&self) -> [f64; 3] {
        self.position
    }

    pub fn axes_reinitialised(&self) -> bool {
        self.axes_reinitialised
    }

    /// Whether the main (L and R) position and velocity PIDs hold gains, so the sled follows moves.
    pub fn main_axes_tuned(&self) -> bool {
        self.tuned[0] && self.tuned[1]
    }

    /// Whether the Z position and velocity PIDs hold gains, so Z follows moves.
    pub fn z_axis_tuned(&self) -> bool {
        self.tuned[2] && self.tuned[3]
    }

    pub fn setting(&self, key: u32) -> Option<f64> {
        self.settings.get(&key).copied()
    }

    /// Times the ring overflowed; a correct host never makes it overflow.
    pub fn overflows(&self) -> usize {
        self.overflows
    }

    /// The most bytes the ring ever held unread.
    pub fn max_unread(&self) -> usize {
        self.max_unread
    }

    pub fn is_paused(&self) -> bool {
        self.held.is_some()
    }

    pub fn build(&self) -> &Build {
        &self.build
    }
}

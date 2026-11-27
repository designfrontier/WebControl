//! What one controller line means, in the order `Background/messageProcessor.py` tests for it.

use crate::boot::parse_firmware_version;
use crate::settings::parse_setting_reply;

/// The firmware's state word in a status report; it never says Run (it says Idle while moving).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Idle,
    Paused,
    Stopped,
    Unknown,
}

impl RunMode {
    /// The `positionMessage.state` text.
    pub fn label(self) -> &'static str {
        match self {
            RunMode::Idle => "Idle",
            RunMode::Paused => "Paused",
            RunMode::Stopped => "Stopped",
            RunMode::Unknown => "",
        }
    }
}

/// `<Idle,MPos:x,y,z,WPos:...>`; MPos has two decimals in the current units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Status {
    pub mode: RunMode,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// `[PE:lErr,rErr,space]` (stock), `[PE:lErr,rErr,space,lChain,rChain]` (holey) or, from a planner build,
/// `[PE:lErr,rErr,space,lChain,rChain,line,queued]`, with `,lAhead,lBehind,rAhead,rBehind` from one listing `peak-error`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionError {
    pub left: f64,
    pub right: f64,
    pub space: i64,
    pub chains: Option<(f64, f64)>,
    pub planner: Option<Planned>,
    pub peaks: Option<Peaks>,
}

/// The largest error since the last report each way along the setpoint's travel, in mm, both non-negative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peaks {
    pub left_ahead: f64,
    pub left_behind: f64,
    pub right_ahead: f64,
    pub right_behind: f64,
}

impl Peaks {
    pub fn ahead(&self) -> f64 {
        self.left_ahead.max(self.right_ahead)
    }

    pub fn max(&self) -> f64 {
        self.ahead().max(self.left_behind).max(self.right_behind)
    }
}

/// The planner's half of a `[PE:]` report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Planned {
    /// The `N` of the line running now, 0 for none.
    pub line: u64,
    /// Moves in the planner, 0 to 15.
    pub queued: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok,
    /// A planner build's `ok N<n>`: every line up to `n` is queued.
    OkLine(u64),
    /// A planner build's `rs N<n>`: send again from line `n`.
    Resend(u64),
    /// `error: ...`, the line's ack all the same.
    Error(String),
    Status(Status),
    /// A status report that would not parse.
    Misread(String),
    PositionError(PositionError),
    /// `$n=value` from a `$$` dump; `value` is NaN for `nan`, `inf`, `ovf` or text.
    Setting {
        key: u32,
        value: f64,
    },
    /// `[Measure: <mm>]`, B10's chain length.
    Measured(f64),
    FakeServo(bool),
    MaslowPaused,
    ToolChange {
        tool: Option<u32>,
    },
    Message(String),
    Alarm(String),
    BufferOverflow,
    FirmwareVersion(Option<f64>),
    /// The words after `Capabilities:`, which a planner test build prints after its version.
    Capabilities(Vec<String>),
    Other(String),
}

fn parse_status(line: &str) -> Option<Status> {
    let mode = if line.contains("Stop") {
        RunMode::Stopped
    } else if line.contains("Pause") {
        RunMode::Paused
    } else if line.contains("Idle") {
        RunMode::Idle
    } else {
        RunMode::Unknown
    };
    let body = &line[line.find("MPos:")? + "MPos:".len()..];
    // Without WPos Python's find() gives -1, which slices off the last character, normally the closing `>`.
    let last_char = body.char_indices().next_back().map_or(0, |(at, _)| at);
    let mut numbers = body[..body.find("WPos:").unwrap_or(last_char)].split(',').map(|n| n.trim().parse::<f64>());
    let mut next = || numbers.next().and_then(Result::ok);
    Some(Status { mode, x: next()?, y: next()?, z: next()? })
}

fn parse_position_error(line: &str) -> Option<PositionError> {
    let body = &line[line.find(':')? + 1..line.find(']').unwrap_or(line.len())];
    let fields: Vec<&str> = body.split(',').map(str::trim).collect();
    let number = |i: usize| fields.get(i).and_then(|f| f.parse::<f64>().ok());
    let chains = number(3).zip(number(4)).filter(|_| fields.len() >= 5);
    let integer = |i: usize| fields.get(i).and_then(|f| f.parse::<u64>().ok());
    let planner = integer(5).zip(integer(6)).map(|(line, queued)| Planned { line, queued: queued as u32 });
    let peaks = number(7).zip(number(8)).zip(number(9).zip(number(10))).map(
        |((left_ahead, left_behind), (right_ahead, right_behind))| Peaks {
            left_ahead,
            left_behind,
            right_ahead,
            right_behind,
        },
    );
    let space = fields.get(2)?.parse().ok()?;
    Some(PositionError { left: number(0)?, right: number(1)?, space, chains, planner, peaks })
}

/// Python reads `message[9:-3]` of the line with its `\r\n`, which drops the closing bracket.
fn measured(line: &str) -> Option<f64> {
    let end = line.char_indices().next_back()?.0;
    line.get(9..end)?.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn tool_in(line: &str) -> Option<u32> {
    let at = line.find("tool")? + "tool".len();
    let rest = &line[at..];
    let digits = rest.trim_start();
    if digits.len() == rest.len() {
        return None;
    }
    digits.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

/// One controller line, without its `\r\n`.
pub fn classify(line: &str) -> Reply {
    match line {
        "ok" => Reply::Ok,
        _ if let Some(number) = line.strip_prefix("ok N").and_then(|n| n.parse().ok()) => Reply::OkLine(number),
        _ if let Some(number) = line.strip_prefix("rs N").and_then(|n| n.parse().ok()) => Reply::Resend(number),
        _ if line.starts_with("error:") => Reply::Error(line["error:".len()..].trim().to_string()),
        _ if line.starts_with('<') => parse_status(line).map_or_else(|| Reply::Misread(line.into()), Reply::Status),
        _ if line.starts_with('$') => match parse_setting_reply(line) {
            Some((key, value)) => Reply::Setting { key, value },
            None => Reply::Other(line.into()),
        },
        _ if line.starts_with("[PE:") => {
            parse_position_error(line).map_or_else(|| Reply::Misread(line.into()), Reply::PositionError)
        }
        _ if line.starts_with("[Measure") => measured(line).map_or_else(|| Reply::Other(line.into()), Reply::Measured),
        _ if line.starts_with("FAKE_SERVO off") => Reply::FakeServo(false),
        _ if line.starts_with("FAKE_SERVO on") => Reply::FakeServo(true),
        _ if line.starts_with("Maslow Paused") => Reply::MaslowPaused,
        _ if line.starts_with("Tool Change:") => Reply::ToolChange { tool: tool_in(line) },
        _ if line.starts_with("Message:") => Reply::Message(line.into()),
        _ if line.starts_with("ALARM:") => Reply::Alarm(line.into()),
        _ if line.starts_with("Buffer overflow!") => Reply::BufferOverflow,
        _ if line.starts_with("Firmware") => Reply::FirmwareVersion(parse_firmware_version(line)),
        _ if line.starts_with("Capabilities:") => {
            Reply::Capabilities(line["Capabilities:".len()..].split_whitespace().map(String::from).collect())
        }
        _ => Reply::Other(line.into()),
    }
}

/// B04 prints a status report with no newline, and 51.291 glues an `ok` onto a line; both are split off.
pub fn split_glued(line: &str) -> Vec<String> {
    if line.starts_with('<')
        && let Some(end) = line.find('>').filter(|end| end + 1 < line.len())
    {
        let mut parts = vec![line[..=end].to_string()];
        parts.extend(split_glued(&line[end + 1..]));
        return parts;
    }
    match line.strip_suffix("ok") {
        Some(head) if !head.is_empty() && !head.ends_with(|c: char| c.is_alphabetic()) => {
            vec![head.to_string(), "ok".to_string()]
        }
        _ => vec![line.to_string()],
    }
}

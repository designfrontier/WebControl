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

/// `[PE:lErr,rErr,space]` (stock) or `[PE:lErr,rErr,space,lChain,rChain]` (holey).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionError {
    pub left: f64,
    pub right: f64,
    pub space: i64,
    pub chains: Option<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok,
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
    FakeServo(bool),
    MaslowPaused,
    ToolChange {
        tool: Option<u32>,
    },
    Message(String),
    Alarm(String),
    BufferOverflow,
    FirmwareVersion(Option<f64>),
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
    let start = line.find("MPos:")? + "MPos:".len();
    // Without WPos Python's find() gives -1, which slices off the closing `>`.
    let end = line[start..].find("WPos:").map_or(line.len().saturating_sub(1).max(start), |at| start + at);
    let mut numbers = line[start..end].split(',').map(|n| n.trim().parse::<f64>());
    let mut next = || numbers.next().and_then(Result::ok);
    Some(Status { mode, x: next()?, y: next()?, z: next()? })
}

fn parse_position_error(line: &str) -> Option<PositionError> {
    let body = &line[line.find(':')? + 1..line.find(']').unwrap_or(line.len())];
    let fields: Vec<&str> = body.split(',').map(str::trim).collect();
    let number = |i: usize| fields.get(i).and_then(|f| f.parse::<f64>().ok());
    let chains = number(3).zip(number(4)).filter(|_| fields.len() >= 5);
    Some(PositionError { left: number(0)?, right: number(1)?, space: fields.get(2)?.parse().ok()?, chains })
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
        _ if line.starts_with("error:") => Reply::Error(line["error:".len()..].trim().to_string()),
        _ if line.starts_with('<') => parse_status(line).map_or_else(|| Reply::Misread(line.into()), Reply::Status),
        _ if line.starts_with('$') => match parse_setting_reply(line) {
            Some((key, value)) => Reply::Setting { key, value },
            None => Reply::Other(line.into()),
        },
        _ if line.starts_with("[PE:") => {
            parse_position_error(line).map_or_else(|| Reply::Misread(line.into()), Reply::PositionError)
        }
        _ if line.starts_with("FAKE_SERVO off") => Reply::FakeServo(false),
        _ if line.starts_with("FAKE_SERVO on") => Reply::FakeServo(true),
        _ if line.starts_with("Maslow Paused") => Reply::MaslowPaused,
        _ if line.starts_with("Tool Change:") => Reply::ToolChange { tool: tool_in(line) },
        _ if line.starts_with("Message:") => Reply::Message(line.into()),
        _ if line.starts_with("ALARM:") => Reply::Alarm(line.into()),
        _ if line.starts_with("Buffer overflow!") => Reply::BufferOverflow,
        _ if line.starts_with("Firmware") => Reply::FirmwareVersion(parse_firmware_version(line)),
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

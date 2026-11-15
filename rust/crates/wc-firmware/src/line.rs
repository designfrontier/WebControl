//! What may go out on the wire (B5): comments stripped, realtime characters removed, unsafe lines refused.

use crate::flow::BUFFER_SIZE;

/// Mach3 `( )` comments, each up to its first `)`, then a `;` comment to the end of the line.
pub fn strip_comments(line: &str) -> String {
    let mut text = line.to_string();
    let mut from = 0;
    while let Some(open) = text[from..].find('(').map(|at| from + at) {
        match text[open..].find(')') {
            Some(close) => text.replace_range(open..=open + close, ""),
            None => from = open + 1,
        }
    }
    text.find(';').map_or(text.clone(), |at| text[..at].to_string())
}

fn is_removed(c: char) -> bool {
    matches!(c, '!' | '~' | '`' | '?') || c.is_ascii_control()
}

/// The firmware acts on `!`, `~`, backtick and `?` anywhere in a line, so they never go out inside one.
pub fn sanitise(line: &str) -> String {
    strip_comments(line).chars().filter(|c| !is_removed(*c)).collect()
}

/// Why the controller can't safely take a line: a title, the reason and the advice, as Python alerts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub title: &'static str,
    pub reason: String,
    pub advice: &'static str,
}

/// `line` exactly as it will be written, without its `\n`.
pub fn problem_with(line: &str) -> Option<Problem> {
    if line.contains('(') {
        // 51.27 loops forever on a comment.
        return Some(Problem {
            title: "Unclosed Comment",
            reason: "has an unclosed comment, which hangs the controller.".into(),
            advice: "Resume stops here again: stop the job, fix the file and load it again.",
        });
    }
    let written = line.len() as i64 + 1;
    // The ring overflows and the firmware then runs the line's unread tail as a command.
    (written > BUFFER_SIZE).then(|| Problem {
        title: "Too Long",
        reason: format!("is {written} characters; the controller accepts at most {BUFFER_SIZE}."),
        advice: "Resume stops here again: stop the job, fix the file (e.g. enable Truncate Floating Point Numbers) \
                 and load it again.",
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Positioning {
    Absolute,
    Relative,
}

impl Positioning {
    /// `statusMessage.positioningMode`: 0 for G90, 1 for G91.
    pub fn code(self) -> u8 {
        match self {
            Positioning::Absolute => 0,
            Positioning::Relative => 1,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Positioning::Absolute => "G90",
            Positioning::Relative => "G91",
        }
    }
}

/// The mode the line's last G90 or G91 switches to, if it has either.
pub fn positioning_set_by(line: &str) -> Option<Positioning> {
    match (line.rfind("G90"), line.rfind("G91")) {
        (None, None) => None,
        (absolute, relative) if absolute > relative => Some(Positioning::Absolute),
        _ => Some(Positioning::Relative),
    }
}

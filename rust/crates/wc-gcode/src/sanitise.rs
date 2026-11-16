//! What a streamed line goes through before it is written (`SerialPortThread._sanitise` and `_problemWith`): its
//! comments and realtime characters removed, and a refusal for a line the controller cannot safely take.

use crate::arc::ArcProblem;
use crate::shift::{HomeShift, shift_to_home};
use crate::{Positioning, py, words};

/// The controller's receive buffer, counting the trailing newline.
pub const BUFFER_SIZE: usize = 126;

/// `_stripComments`: `( )` and `;` comments removed.
pub fn strip_comments(line: &str) -> String {
    words::replace_semicolons(&words::replace_parens(line, ""), "")
}

/// `_sanitise`: comments removed, then every character the firmware would act on as a realtime command anywhere in
/// a line (`!`, `~`, backtick, `?`), and every control character.
pub fn sanitise(line: &str) -> String {
    strip_comments(line).chars().filter(|&c| !matches!(c, '!' | '~' | '`' | '?' | '\x00'..='\x1f' | '\x7f')).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineProblem {
    /// A `(` survived comment stripping; 51.27 loops forever on a comment.
    UnclosedComment,
    /// The ring overflows and the firmware then runs the line's unread tail as a command.
    TooLong { written: usize },
    /// An arc that cannot be rewritten as I and J with absolute X and Y.
    Arc(ArcProblem),
}

impl LineProblem {
    pub fn title(&self) -> &'static str {
        match self {
            LineProblem::UnclosedComment => "Unclosed Comment",
            LineProblem::TooLong { .. } => "Too Long",
            LineProblem::Arc(problem) => problem.title(),
        }
    }

    /// What is wrong, as the end of a sentence about the line or command.
    pub fn reason(&self) -> String {
        match self {
            LineProblem::UnclosedComment => "has an unclosed comment, which hangs the controller.".to_string(),
            LineProblem::TooLong { written } => {
                format!("is {written} characters; the controller accepts at most {BUFFER_SIZE}.")
            }
            LineProblem::Arc(problem) => problem.reason().to_string(),
        }
    }

    /// The alert's title and text for the 0-based `index` of the refused line.
    pub fn alert(&self, index: usize) -> (String, String) {
        let advice = match self {
            LineProblem::UnclosedComment => "Resume stops here again: stop the job, fix the file and load it again.",
            LineProblem::TooLong { .. } => {
                "Resume stops here again: stop the job, fix the file (e.g. enable Truncate Floating Point Numbers) \
                 and load it again."
            }
            LineProblem::Arc(problem) => problem.advice(),
        };
        (format!("Line {}", self.title()), format!("Line {} {} {advice}", index + 1, self.reason()))
    }
}

/// `_problemWith`, for the line exactly as it will be written without its newline. Length counts bytes, as the
/// firmware does; Python counts characters, which only differ for non-ASCII text.
pub fn problem_with(line: &str) -> Option<LineProblem> {
    let written = line.len() + 1;
    if line.contains('(') {
        Some(LineProblem::UnclosedComment)
    } else if written > BUFFER_SIZE {
        Some(LineProblem::TooLong { written })
    } else {
        None
    }
}

/// `_streamProblem`: a blank line is never refused; any other is checked as it will be written, home shift included.
pub fn stream_problem(line: &str, current: Positioning, shift: &HomeShift) -> Option<LineProblem> {
    if py::isspace(line) { None } else { problem_with(&shift_to_home(line, current, shift)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_SHIFT: HomeShift = HomeShift { x: 0.0, y: 0.0, truncate: None };

    #[test]
    fn comments_and_realtime_characters_are_removed() {
        assert_eq!(strip_comments("G1 X1 (a) ; b"), "G1 X1  ");
        assert_eq!(sanitise("G1 X1 ! ~ ` ? (c)\t\x01\x7f"), "G1 X1     ");
        assert_eq!(sanitise("G1 X1 (open"), "G1 X1 (open");
    }

    #[test]
    fn an_unclosed_comment_is_refused() {
        assert_eq!(problem_with("G1 X1 (open"), Some(LineProblem::UnclosedComment));
    }

    #[test]
    fn a_line_over_the_buffer_is_refused_and_one_that_fits_is_not() {
        assert_eq!(problem_with(&"X".repeat(125)), None);
        assert_eq!(problem_with(&"X".repeat(126)), Some(LineProblem::TooLong { written: 127 }));
    }

    #[test]
    fn the_home_shift_can_push_a_line_over() {
        let line = format!("G1 X1 {}", " ".repeat(117));
        let far = HomeShift { x: 1000.5, y: 0.0, truncate: None };
        assert_eq!(stream_problem(&line, Positioning::Absolute, &NO_SHIFT), None);
        assert_eq!(stream_problem(&line, Positioning::Absolute, &far), Some(LineProblem::TooLong { written: 130 }));
        assert_eq!(stream_problem(&line, Positioning::Relative, &far), None);
        assert_eq!(stream_problem("   ", Positioning::Absolute, &NO_SHIFT), None);
    }

    #[test]
    fn a_blank_line_is_never_refused_however_long() {
        assert_eq!(stream_problem(&" ".repeat(200), Positioning::Absolute, &NO_SHIFT), None);
        assert_eq!(stream_problem(&"\x1c".repeat(200), Positioning::Absolute, &NO_SHIFT), None);
        assert_eq!(stream_problem("", Positioning::Absolute, &NO_SHIFT), None);
    }

    #[test]
    fn the_alerts_name_the_line_and_the_fix() {
        assert_eq!(
            LineProblem::UnclosedComment.alert(4),
            (
                "Line Unclosed Comment".to_string(),
                "Line 5 has an unclosed comment, which hangs the controller. Resume stops here again: stop the job, \
                 fix the file and load it again."
                    .to_string()
            )
        );
        assert_eq!(
            LineProblem::TooLong { written: 127 }.alert(0).1,
            "Line 1 is 127 characters; the controller accepts at most 126. Resume stops here again: stop the job, \
             fix the file (e.g. enable Truncate Floating Point Numbers) and load it again."
        );
        assert_eq!(
            LineProblem::Arc(ArcProblem::RadiusTooSmall).alert(2),
            (
                "Line Impossible Arc".to_string(),
                "Line 3 is an arc whose radius (R) is shorter than half the distance to its end. Resume stops here \
                 again: stop the job, fix the file and load it again."
                    .to_string()
            )
        );
    }
}

//! How the Maslow firmware reads the words that pause it (`Connection/firmwareGcode.py`, spec/firmware.md §7): M0/M1,
//! and M6 with the tool chosen by T, which the firmware reads only where it leads a line with a G or M word.

use std::sync::LazyLock;

use regex::Regex;

use crate::py;

static TOOL_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)T[\s\x1c-\x1f]*([0-9]+(?:\.[0-9]*)?)").expect("pattern"));
static FIRMWARE_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)").expect("pattern"));
static ONLY_TOOL_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[\s\x1c-\x1f]*(N[0-9]+[\s\x1c-\x1f]*)?T[\s\x1c-\x1f]*[0-9]+(\.[0-9]*)?[\s\x1c-\x1f]*$")
        .expect("pattern")
});

/// Python's `M0*[<last>](?![0-9.])` searched anywhere in the line, in either case as the firmware upper-cases it.
fn has_m_word(line: &str, last: &[u8]) -> bool {
    let bytes = line.as_bytes();
    let continues = |at: Option<&u8>| at.is_some_and(|&b| b.is_ascii_digit() || b == b'.');
    bytes.iter().enumerate().filter(|&(_, &b)| b.eq_ignore_ascii_case(&b'M')).any(|(index, _)| {
        let rest = &bytes[index + 1..];
        let zeros = rest.iter().take_while(|&&b| b == b'0').count();
        (0..=zeros).any(|k| rest.get(k).is_some_and(|b| last.contains(b)) && !continues(rest.get(k + 1)))
    })
}

/// An M0 or M1 word, after which the firmware waits for `~`.
pub fn pauses_firmware(line: &str) -> bool {
    has_m_word(line, b"01")
}

/// An M6 word, which changes to the tool the firmware has pending.
pub fn changes_tool(line: &str) -> bool {
    has_m_word(line, b"6")
}

fn is_machine_command(line: &str) -> bool {
    py::lstrip(line).starts_with(['B', 'b', '$'])
}

/// A line with nothing but a T word, which the firmware runs as an empty modal move.
pub fn only_selects_tool(line: &str) -> bool {
    ONLY_TOOL_WORD.is_match(line)
}

fn whole_tool(number: &str) -> i64 {
    number.parse::<f64>().map_or(0, |value| value as i64)
}

/// The tool the line selects as the file means it: its T word, wherever it stands.
pub fn tool_named_by(line: &str) -> Option<i64> {
    if is_machine_command(line) {
        return None;
    }
    TOOL_WORD.captures(line).map(|found| whole_tool(&found[1]))
}

/// The tool the firmware reads from the line: a T leading a line that also has a G or M word.
pub fn tool_the_firmware_reads(line: &str) -> Option<i64> {
    let text = py::upper(py::strip(line));
    let first_g_or_m = [text.find('G'), text.find('M')].into_iter().flatten().min();
    let segment = first_g_or_m.filter(|&index| index > 0).map_or("", |index| &text[..index]);
    if is_machine_command(&text) || segment.chars().count() < 2 || !segment.starts_with('T') {
        return None;
    }
    Some(FIRMWARE_NUMBER.find(&segment[1..]).map_or(0, |number| whole_tool(number.as_str())))
}

/// Rewrites an M6 line so the firmware reads `tool` from it, by leading the line with `T<tool>`.
pub fn with_tool_first(line: &str, tool: i64) -> String {
    if is_machine_command(line) || !changes_tool(line) || tool_the_firmware_reads(line) == Some(tool) {
        return line.to_string();
    }
    let trailing = &line[line.trim_end_matches(' ').len()..];
    let without_tool = TOOL_WORD.replace_all(line, "");
    let rest = py::split(&without_tool).collect::<Vec<_>>().join(" ");
    format!("T{tool} {rest}{trailing}")
}

/// The two bytes after each G38 word's `G38`, which the firmware's `G38()` probes on only when they are `.2`; on
/// any other it moves nothing, zeroes nothing and acks `ok` (spec/firmware.md §4).
fn g38_suffixes(line: &str) -> impl Iterator<Item = Option<&[u8]>> {
    let text = line.as_bytes();
    let g_number = move |at: usize| {
        let digits = text[at + 1..].iter().take_while(|b| b.is_ascii_digit() || b.is_ascii_punctuation()).count();
        let number = String::from_utf8_lossy(&text[at + 1..at + 1 + digits]);
        FIRMWARE_NUMBER.find(&number).and_then(|found| found.as_str().parse::<f64>().ok()).map(|value| value as i64)
    };
    let gcode = !is_machine_command(line);
    (0..text.len())
        .filter(move |&at| gcode && text[at].eq_ignore_ascii_case(&b'G') && g_number(at) == Some(38))
        .map(move |at| text.get(at + 3..at + 5))
}

/// A G38 word other than G38.2, which the firmware acks without probing.
pub fn probe_the_firmware_ignores(line: &str) -> bool {
    g38_suffixes(line).any(|suffix| suffix != Some(b".2"))
}

/// A G38 word the firmware probes on: `G38.2`, but not `G038.2` or `G 38.2`.
pub fn firmware_probes(line: &str) -> bool {
    g38_suffixes(line).any(|suffix| suffix == Some(b".2"))
}

/// `toolChangeLine`: what to send for a line, the tool the firmware will have pending after it, and whether its M6
/// changes tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolChange {
    pub line: String,
    pub next_tool: i64,
    pub pauses: bool,
}

pub fn tool_change_line(line: &str, next_tool: i64, current_tool: i64) -> ToolChange {
    let next_tool = tool_named_by(line).unwrap_or(next_tool);
    // Even a same-tool M6 names the tool, so the firmware cannot pause on a T the host saw differently.
    ToolChange {
        line: with_tool_first(line, next_tool),
        next_tool,
        pauses: changes_tool(line) && next_tool != current_tool,
    }
}

/// The tools a file uses, as the job queue compares them between files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileTools {
    /// The first tool its T words name; tool 0 names none.
    pub first: Option<i64>,
    /// The tool its last M6 changes to.
    pub last: Option<i64>,
}

impl FileTools {
    /// The tool in the spindle once the file has run, from the one before it; the queue changes to `first` itself.
    pub fn after(self, before: i64) -> i64 {
        self.last.or(self.first).unwrap_or(before)
    }
}

pub fn file_tools<'a>(lines: impl IntoIterator<Item = &'a str>) -> FileTools {
    let (tools, _) = lines.into_iter().fold((FileTools::default(), None), |(tools, pending), line| {
        let line = crate::sanitise::sanitise(line);
        let named = tool_named_by(&line);
        let pending = named.or(pending);
        let last = if changes_tool(&line) { pending.or(tools.last) } else { tools.last };
        (FileTools { first: tools.first.or(named.filter(|&tool| tool > 0)), last }, pending)
    });
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_tools_are_its_first_t_word_and_its_last_m6() {
        let tools = |lines: &[&str]| file_tools(lines.iter().copied());
        assert_eq!(tools(&["(T9 in a comment)", "G0 X1", "T2 M6", "G1 X2", "T3", "M6", "T4"]).first, Some(2));
        assert_eq!(tools(&["T2 M6", "T3", "M6", "T4"]).last, Some(3));
        assert_eq!(tools(&["G0 X1", "M5"]), FileTools::default());
        assert_eq!(tools(&["T0 M6", "T1"]), FileTools { first: Some(1), last: Some(0) });
        assert_eq!(tools(&["T2", "G1 X1"]).after(5), 2);
        assert_eq!(tools(&["G1 X1"]).after(5), 5);
        assert_eq!(tools(&["T2 M6", "T3 M6"]).after(5), 3);
    }

    #[test]
    fn every_g38_but_g38_2_is_a_probe_the_firmware_ignores() {
        ["G38.3 Z-10 ", "G90 G38.4 Z-1", "g38.5 z-2", "G38 Z-1", "G38.1", "G038.2 Z-1", "G38.35", "G38G1"]
            .iter()
            .for_each(|line| assert!(probe_the_firmware_ignores(line), "{line}"));
        ["G38.2 Z-10 ", "G90 G38.2 Z-1", "g38.2", "G38.25", "G1 X38.3", "G3 X1", "G 38.3", "B38.3", "$38=1", "G3.8"]
            .iter()
            .for_each(|line| assert!(!probe_the_firmware_ignores(line), "{line}"));
    }

    #[test]
    fn the_firmware_probes_only_where_its_g38_reads_dot_2() {
        ["G38.2 Z-10 ", "G90 G38.2 Z-1", "g38.2", "G38.25", "G38.20 Z-1", "X1G38.2"]
            .iter()
            .for_each(|line| assert!(firmware_probes(line), "{line}"));
        ["G038.2 Z-1", "G 38.2 Z-1", "G38.3 Z-1", "G38", "B38.2", "$38.2", "G1 X38.2"]
            .iter()
            .for_each(|line| assert!(!firmware_probes(line), "{line}"));
    }

    #[test]
    fn the_firmware_reads_t_only_where_its_parser_does() {
        let cases = [
            ("T2 M6 ", Some(2)),
            ("T2.0 M6 ", Some(2)),
            ("T 2 M6 ", Some(0)),
            ("t3 g0 x1 ", Some(3)),
            ("T2 ", None),
            ("M6 T2 ", None),
            ("N10 T2 M6 ", None),
            ("G0 X1 T2 ", None),
            ("B14 T2 M6 ", None),
            ("TM6", None),
        ];
        cases.iter().for_each(|(line, tool)| assert_eq!(tool_the_firmware_reads(line), *tool, "{line}"));
    }

    #[test]
    fn pause_words_are_m0_and_m1_but_not_longer_codes() {
        ["M0 ", "M00", "M01 ", "M1", "G1 X1 M0", "M2 M1 ", "M001. M1"]
            .iter()
            .for_each(|l| assert!(pauses_firmware(l), "{l}"));
        ["M10 ", "M0.5 ", "M001. ", "M30", "m30 ", "G1 X1"].iter().for_each(|l| assert!(!pauses_firmware(l), "{l}"));
        ["m0 ", "g1 x1 m1"].iter().for_each(|l| assert!(pauses_firmware(l), "{l}"));
    }

    #[test]
    fn m6_and_m06_change_tool_but_m60_does_not() {
        ["M6", "M06 ", "M006 ", "T2 M6", "m6", "t2 m06"].iter().for_each(|line| assert!(changes_tool(line), "{line}"));
        ["M60 ", "M6.1 ", "M16 ", "G6"].iter().for_each(|line| assert!(!changes_tool(line), "{line}"));
    }

    #[test]
    fn a_line_that_only_selects_a_tool() {
        ["T2 ", "T 3 ", "N10 T2 ", "T2.0 ", " T2\n", "t2 ", "n10 t2 "]
            .iter()
            .for_each(|line| assert!(only_selects_tool(line), "{line}"));
        ["T2 M6 ", "G0 T2", "T", "TX"].iter().for_each(|line| assert!(!only_selects_tool(line), "{line}"));
    }

    #[test]
    fn the_file_names_a_tool_with_its_t_word_wherever_it_stands() {
        assert_eq!(tool_named_by("G0 X1 T2 "), Some(2));
        assert_eq!(tool_named_by("M6 T3.7"), Some(3));
        assert_eq!(tool_named_by("B14 L1 T2"), None);
        assert_eq!(tool_named_by("$13=T2"), None);
        assert_eq!(tool_named_by("G1 X1"), None);
        assert_eq!(tool_named_by("m6 t4"), Some(4));
        assert_eq!(tool_named_by("b14 l1 t2"), None);
    }

    #[test]
    fn an_m6_is_led_by_the_tool_it_changes_to() {
        assert_eq!(with_tool_first("M6 T2 ", 2), "T2 M6 ");
        assert_eq!(with_tool_first("N10 T2 M6 ", 2), "T2 N10 M6 ");
        assert_eq!(with_tool_first("M6  ", 3), "T3 M6  ");
        assert_eq!(with_tool_first("T2 M6 ", 2), "T2 M6 ");
        assert_eq!(with_tool_first("G1 X1 ", 2), "G1 X1 ");
        assert_eq!(with_tool_first("B14 M6", 2), "B14 M6");
        assert_eq!(with_tool_first("m6 t2 ", 2), "T2 m6 ");
    }

    #[test]
    fn tool_change_line_tracks_the_pending_tool() {
        assert_eq!(tool_change_line("T2 ", 0, 0), ToolChange { line: "T2 ".into(), next_tool: 2, pauses: false });
        assert_eq!(tool_change_line("M6 ", 2, 0), ToolChange { line: "T2 M6 ".into(), next_tool: 2, pauses: true });
        assert_eq!(tool_change_line("M6 ", 2, 2), ToolChange { line: "T2 M6 ".into(), next_tool: 2, pauses: false });
        assert_eq!(tool_change_line("G1 X1 ", 3, 1).next_tool, 3);
    }
}

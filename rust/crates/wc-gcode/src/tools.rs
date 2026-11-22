//! The tools a file names: its tool changes, and what its CAM comments say each tool is (Fusion 360's tool lines,
//! Carbide Create's `TOOL/MILL` and `M0 ;T102`, Estlcam's `Change tool:`, Vectric's tools list, plain `(T1 ...)`).

use std::sync::LazyLock;

use regex::Regex;
use wc_protocol::tools::{FileTool, ToolUnits};

use crate::firmware_gcode::{changes_tool, only_selects_tool, pauses_firmware, tool_named_by};

fn pattern(text: &str) -> Regex {
    Regex::new(text).expect("pattern")
}

static FUSION: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)^T(\d+)\s+D=([0-9.]+)(?:\s+CR=[0-9.]+)?(?:\s+TAPER=([0-9.]+)DEG)?(?:\s+-\s+ZMIN=\S+)?\s+-\s+(.*\S)$")
});
static CARBIDE: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^TOOL/MILL,\s*([0-9.]+),\s*([0-9.]+),\s*[0-9.]+,\s*([0-9.]+)"));
static ESTLCAM: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)^change\s+tool\s*:?\s*(.*\S)$"));
static TOOLS_USED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)^tools\s+used"));
static LISTED: LazyLock<Regex> = LazyLock::new(|| pattern(r"^(\d+)\s*=\s*(.*\S)$"));
static NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^(?:T\s*|tool\s*(?:#|no\.?|number)?\s*)(\d+)\b\s*(?:[:=-]\s*|\s+)(.*\S)$"));
static BARE: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)^T\s*(\d+)$"));
static ONLY_WORDS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)^([A-Z]\s*[-+]?[0-9.]+\s*)+$"));
static UNITS: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)G\s*0*2([01])(?:[^0-9.]|$)"));
static FLUTES: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)\b(\d{1,2}|single|one|two|three|four)[\s-]*flutes?\b"));

/// A described tool waits this many lines for the tool change that names its number.
const PENDING_LINES: usize = 6;
const FRACTIONS: [u32; 6] = [2, 4, 8, 16, 32, 64];
const MM_PER_INCH: f64 = 25.4;

/// Where the comment opened at the start of `text` closes; Vectric nests parentheses in tool names.
fn comment_end(text: &str) -> usize {
    let mut depth = 0;
    text.char_indices()
        .find_map(|(at, c)| {
            depth += match c {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            (depth == 0).then_some(at)
        })
        .unwrap_or(text.len())
}

/// The line's code, and the text of each `( )` and `;` comment in it.
fn split(line: &str) -> (String, Vec<&str>) {
    let mut code = String::new();
    let mut comments = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find(['(', ';']) {
        code.push_str(&rest[..at]);
        if rest[at..].starts_with(';') {
            comments.push(rest[at + 1..].trim());
            return (code, comments);
        }
        let end = at + comment_end(&rest[at..]);
        comments.push(rest[at + 1..end].trim());
        rest = rest.get(end + 1..).unwrap_or_default();
    }
    code.push_str(rest);
    (code, comments)
}

fn units_of(raw: &str) -> Option<ToolUnits> {
    raw.lines().find_map(|line| {
        let (code, _) = split(line);
        let found = UNITS.captures(&code)?;
        Some(if &found[1] == "0" { ToolUnits::In } else { ToolUnits::Mm })
    })
}

fn trimmed(number: f64) -> String {
    let text = format!("{number:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `1/4"` when `inches` is a common fraction or a whole number of inches.
fn fraction(inches: f64) -> Option<String> {
    let whole = inches.round();
    if whole >= 1.0 && (inches - whole).abs() < 5e-4 {
        return Some(format!("{whole}\""));
    }
    FRACTIONS.iter().find_map(|&denominator| {
        let top = (inches * f64::from(denominator)).round();
        let exact = top >= 1.0 && (inches * f64::from(denominator) - top).abs() < 5e-4 && top % 2.0 == 1.0;
        exact.then(|| format!("{top}/{denominator}\""))
    })
}

fn size(diameter: f64, units: Option<ToolUnits>) -> String {
    match units {
        Some(ToolUnits::Mm) => match fraction(diameter / MM_PER_INCH) {
            Some(inches) => format!("{} mm ({inches})", trimmed(diameter)),
            None => format!("{} mm", trimmed(diameter)),
        },
        Some(ToolUnits::In) => fraction(diameter).unwrap_or_else(|| format!("{} in", trimmed(diameter))),
        None => format!("{} diameter", trimmed(diameter)),
    }
}

fn flutes_in(text: &str) -> Option<u32> {
    let word = FLUTES.captures(text)?[1].to_lowercase();
    match word.as_str() {
        "single" | "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        digits => digits.parse().ok().filter(|&n| n > 0),
    }
}

fn number(text: &str) -> Option<u32> {
    text.parse().ok().filter(|&n| n > 0)
}

fn described(number: u32, description: String) -> FileTool {
    FileTool { number, flutes: flutes_in(&description), description: Some(description), diameter: None, units: None }
}

fn sized(number: u32, description: String, diameter: Option<f64>, units: Option<ToolUnits>) -> FileTool {
    let diameter = diameter.filter(|d| d.is_finite() && *d > 0.0);
    FileTool { units: units.filter(|_| diameter.is_some()), diameter, ..described(number, description) }
}

fn fusion(comment: &str, units: Option<ToolUnits>) -> Option<FileTool> {
    let found = FUSION.captures(comment)?;
    let diameter = found[2].parse::<f64>().ok();
    let taper = found.get(3).and_then(|angle| angle.as_str().parse::<f64>().ok()).filter(|&angle| angle > 0.0);
    let kind = found[4].trim().to_lowercase();
    let mut description = diameter.map_or(kind.clone(), |d| format!("{} {kind}", size(d, units)));
    if let Some(angle) = taper {
        description.push_str(&format!(", {}° taper", trimmed(angle)));
    }
    Some(sized(number(&found[1])?, description, diameter, units))
}

/// Carbide Create's APT line: diameter, corner radius, length and taper angle.
fn carbide(comment: &str, units: Option<ToolUnits>) -> Option<(String, Option<f64>)> {
    let found = CARBIDE.captures(comment)?;
    let [diameter, radius, angle] = [1, 2, 3].map(|at| found[at].parse::<f64>().unwrap_or(0.0));
    let kind = match () {
        _ if angle > 0.0 => format!("V-bit, {}° taper", trimmed(angle)),
        _ if diameter > 0.0 && (radius * 2.0 - diameter).abs() < 0.02 => "ball end mill".into(),
        _ => "end mill".into(),
    };
    Some((format!("{} {kind}", size(diameter, units)), Some(diameter)))
}

fn numbered(comment: &str, listing: bool) -> Option<FileTool> {
    let found = NUMBERED.captures(comment).or_else(|| listing.then(|| LISTED.captures(comment)).flatten())?;
    let text = found[2].trim();
    let real = text.chars().any(char::is_alphabetic) && !ONLY_WORDS.is_match(text) && !changes_tool(comment);
    let number = number(&found[1]).filter(|_| real)?;
    Some(described(number, text.to_string()))
}

/// Every tool the file changes to, selects or describes, in the order it first names each.
pub fn file_tools(raw: &str) -> Vec<FileTool> {
    let units = units_of(raw);
    let mut tools: Vec<FileTool> = Vec::new();
    let mut add = |tool: FileTool| match tools.iter_mut().find(|known| known.number == tool.number) {
        Some(known) if known.description.is_none() => *known = tool,
        Some(_) => {}
        None => tools.push(tool),
    };
    let mut pending: Option<(String, Option<f64>, usize)> = None;
    let mut listing = false;
    raw.lines().enumerate().for_each(|(at, line)| {
        let (code, comments) = split(line);
        let code_only = !code.trim().is_empty();
        listing &= !code_only;
        let mut named = None;
        if changes_tool(&code) || only_selects_tool(&code) {
            named = tool_named_by(&code).and_then(|tool| u32::try_from(tool).ok()).filter(|&tool| tool > 0);
        }
        comments.iter().for_each(|comment| {
            if let Some(tool) = fusion(comment, units).or_else(|| numbered(comment, listing)) {
                add(tool);
            } else if let Some((description, diameter)) = carbide(comment, units) {
                pending = Some((description, diameter, at));
            } else if let Some(found) = ESTLCAM.captures(comment) {
                pending = Some((found[1].to_string(), None, at));
            } else if let Some(found) = BARE.captures(comment).filter(|_| pauses_firmware(&code) || changes_tool(&code))
            {
                named = named.or(number(&found[1]));
            }
            listing |= TOOLS_USED.is_match(comment);
        });
        pending = pending.take().filter(|&(_, _, since)| at - since <= PENDING_LINES);
        if let Some(tool) = named {
            add(match pending.take() {
                Some((description, diameter, _)) => sized(tool, description, diameter, units),
                None => FileTool { number: tool, description: None, diameter: None, units: None, flutes: None },
            });
        }
    });
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(number: u32, description: &str) -> (u32, Option<String>) {
        (number, Some(description.to_string()))
    }

    fn summary(raw: &str) -> Vec<(u32, Option<String>)> {
        file_tools(raw).into_iter().map(|tool| (tool.number, tool.description)).collect()
    }

    #[test]
    fn splits_code_from_paren_and_semicolon_comments() {
        assert_eq!(split("G0 X1 (a) Y2 ; b (c)"), ("G0 X1  Y2 ".to_string(), vec!["a", "b (c)"]));
        assert_eq!(split("(open"), (String::new(), vec!["open"]));
        assert_eq!(split("(1 = End Mill (6 mm)) G1"), (" G1".to_string(), vec!["1 = End Mill (6 mm)"]));
        assert_eq!(split("G1"), ("G1".to_string(), vec![]));
    }

    #[test]
    fn reads_the_owners_fusion_360_files() {
        let raw = "(2.CLEARING.25.ENDMILL.NULLCASE)\n(T6  D=6.35 CR=0 - ZMIN=-10.995 - FLAT END MILL)\n\
                   G90 G94\nG17\nG21\n(ADAPTIVE2)\nT6 M6\nS10000 M3\n";
        let tools = file_tools(raw);
        assert_eq!(summary(raw), [tool(6, "6.35 mm (1/4\") flat end mill")]);
        assert_eq!((tools[0].diameter, tools[0].units), (Some(6.35), Some(ToolUnits::Mm)));
        let tapered = "(T3 D=0.456 CR=0.25 TAPER=5.3DEG - ZMIN=-1 - TAPERED MILL)\nG21\nT3 M6\n";
        assert_eq!(summary(tapered), [tool(3, "0.456 mm tapered mill, 5.3° taper")]);
        let chamfer = "(T10  D=19.05 CR=0 TAPER=45DEG - ZMIN=-3.5 - CHAMFER MILL)\nG21\n";
        assert_eq!(summary(chamfer), [tool(10, "19.05 mm (3/4\") chamfer mill, 45° taper")]);
        let face = "(T8  D=50.8 CR=0 - ZMIN=-1 - FACE MILL)\nG21\n";
        assert_eq!(summary(face), [tool(8, "50.8 mm (2\") face mill")]);
        let lower = "(T1 D=6.35 CR=0. - ZMIN=-3. - flat end mill)\n(T2 D=3 CR=1.5 - ball end mill)\nG21\n";
        assert_eq!(summary(lower), [tool(1, "6.35 mm (1/4\") flat end mill"), tool(2, "3 mm ball end mill")]);
    }

    #[test]
    fn fusion_diameters_follow_the_files_units() {
        assert_eq!(summary("(T1 D=0.25 CR=0 - ZMIN=-1 - flat end mill)\nG20\n"), [tool(1, "1/4\" flat end mill")]);
        assert_eq!(summary("(T1 D=0.3 CR=0 - ZMIN=-1 - flat end mill)\nG20\n"), [tool(1, "0.3 in flat end mill")]);
        let unknown = file_tools("(T1 D=6 CR=0 - ZMIN=-1 - flat end mill)\n");
        assert_eq!(unknown[0].description.as_deref(), Some("6 diameter flat end mill"));
        assert_eq!(unknown[0].units, None);
    }

    #[test]
    fn reads_carbide_creates_tool_lines_and_its_m0_tool_comments() {
        let owner = "G90\nG21\n(Pocket.To...)\nM05\nM0 ;T10\nM03S10000\nG0X78.064Y94.064Z5.000\n";
        assert_eq!(summary(owner), [(10, None)]);
        let described = "G21\n(Pocket 1)\n(TOOL/MILL,3.18,0.00,0.00,0.00)\nM6T102\n\
                         (TOOL/MILL,6.35,3.17,0.00,0.00)\nM0 ;T251\n(TOOL/MILL,0.03,0.00,0.00,45.00)\nM6T301\n";
        let tools = file_tools(described);
        assert_eq!(
            summary(described),
            [
                tool(102, "3.18 mm end mill"),
                tool(251, "6.35 mm (1/4\") ball end mill"),
                tool(301, "0.03 mm V-bit, 45° taper")
            ]
        );
        assert_eq!(tools[0].diameter, Some(3.18));
    }

    #[test]
    fn a_described_tool_waits_only_a_few_lines_for_its_number() {
        let late = "(TOOL/MILL,3.18,0.00,0.00,0.00)\nG1\nG1\nG1\nG1\nG1\nG1\nG1\nM6T102\n";
        assert_eq!(summary(late), [(102, None)]);
    }

    #[test]
    fn reads_estlcam_change_tool_comments() {
        let raw = "G21\nM5\n;Change tool: 1/16\" ball\nM6 T2\nM0 (Change tool: 3mm 2 flute endmill) ;T4\n";
        let tools = file_tools(raw);
        assert_eq!(summary(raw), [tool(2, "1/16\" ball"), tool(4, "3mm 2 flute endmill")]);
        assert_eq!(tools[1].flutes, Some(2));
    }

    #[test]
    fn reads_vectrics_tools_list_and_tool_change_notes() {
        let raw = "(Tools used in this file: )\n(1 = End Mill (6 mm))\n(2 = V-Bit (60 deg 12.7 mm))\nT1\nG17\n\
                   (3 = not a tool once code starts)\nT2 M6\n";
        assert_eq!(summary(raw), [tool(1, "End Mill (6 mm)"), tool(2, "V-Bit (60 deg 12.7 mm)")]);
    }

    #[test]
    fn reads_generic_numbered_notes_and_skips_gcode_in_comments() {
        let raw = "; T1 1/4\" 2 flute endmill\n(Tool 2: single flute upcut)\n(T3 M6)\n(T4 S1000)\n(Tool #5 = 60 deg vee)\n\
                   (T6)\nT3 M6\nG0 T7\n";
        let tools = file_tools(raw);
        assert_eq!(
            summary(raw),
            [tool(1, "1/4\" 2 flute endmill"), tool(2, "single flute upcut"), tool(5, "60 deg vee"), (3, None)]
        );
        assert_eq!((tools[0].flutes, tools[1].flutes), (Some(2), Some(1)));
    }

    #[test]
    fn keeps_the_first_description_and_fills_in_a_bare_number() {
        let raw = "T2 M6\n(T2 D=3.175 CR=0 - ZMIN=-1 - flat end mill)\n(T2 D=9 CR=0 - ZMIN=-1 - other)\nG21\n";
        assert_eq!(summary(raw), [tool(2, "3.175 mm (1/8\") flat end mill")]);
        assert!(file_tools("T0 M6\nG0 X1\n").is_empty());
    }

    #[test]
    fn counts_flutes_in_words_and_digits() {
        assert_eq!(flutes_in("2-flute"), Some(2));
        assert_eq!(flutes_in("three flutes"), Some(3));
        assert_eq!(flutes_in("four flute"), Some(4));
        assert_eq!(flutes_in("one flute"), Some(1));
        assert_eq!(flutes_in("0 flute"), None);
        assert_eq!(flutes_in("fluted"), None);
    }
}

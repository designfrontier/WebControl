//! A planner test build's line protocol (`spec/firmware.md` D): numbered, checksummed lines and its settings.

/// Acceleration (mm/s²) and junction deviation (mm), which only a planner build takes.
pub const PLANNER_KEYS: [u32; 2] = [47, 48];
/// The feed-forward gain, which only a build listing the `feed-forward` capability takes.
pub const FEED_FORWARD_KEY: u32 = 49;
/// The moves the planner holds.
pub const PLANNER_DEPTH: u32 = 15;
/// Its `ok` means queued, so this one's means every move before it has finished.
pub const DRAIN: &str = "G4 P0";

/// The firmware runs the line only once every queued move has finished (`PROTOCOL.md`'s drain points).
pub fn waits_for_moves(line: &str) -> bool {
    let line = line.trim().to_ascii_uppercase();
    let mut g_codes = line.split('G').skip(1).filter_map(|rest| {
        rest.split(|c: char| !c.is_ascii_digit()).next().and_then(|digits| digits.parse::<u32>().ok())
    });
    line.starts_with(['S', 'B', '$']) || line.contains('M') || g_codes.any(|g| matches!(g, 4 | 10 | 38))
}

/// `N<n> <line>*<sum>`, the sum the XOR of every byte before `*`, over the line trimmed and upper-cased.
pub fn numbered(number: u64, line: &str) -> String {
    let body = format!("N{number} {}", line.trim().to_ascii_uppercase());
    let sum = body.bytes().fold(0u8, |sum, byte| sum ^ byte);
    format!("{body}*{sum}")
}

/// The line sets the planner's next number: `M110 N<n>`, sent without a number of its own.
pub fn renumber(next: u64) -> String {
    format!("M110 N{next}")
}

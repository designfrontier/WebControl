//! The words the firmware pauses on (`Connection/firmwareGcode.py`), without look-around regexes.

/// Every number written right after `letter`: the maximal run of digits and dots.
fn runs_after(line: &str, letter: char) -> impl Iterator<Item = &str> {
    line.match_indices(letter).map(move |(at, _)| {
        let rest = &line[at + 1..];
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        &rest[..end]
    })
}

/// `0*` then `last`, and nothing after it.
fn is_zero_padded(run: &str, last: &[char]) -> bool {
    run.strip_suffix(last).is_some_and(|zeros| zeros.chars().all(|c| c == '0'))
}

/// M0 or M1 (also M00, M01), which pause the firmware until `~`.
pub fn pauses_firmware(line: &str) -> bool {
    runs_after(line, 'M').any(|run| is_zero_padded(run, &['0', '1']))
}

/// M6 (also M06), a tool change.
pub fn changes_tool(line: &str) -> bool {
    runs_after(line, 'M').any(|run| is_zero_padded(run, &['6']))
}

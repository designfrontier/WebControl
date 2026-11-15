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

fn skip_digits(text: &str) -> (&str, usize) {
    let count = text.chars().take_while(char::is_ascii_digit).count();
    (&text[count..], count)
}

/// A line with nothing but a T word (`^\s*(N[0-9]+\s*)?T\s*[0-9]+(\.[0-9]*)?\s*$`), which the firmware runs as
/// an empty modal move.
pub fn only_selects_tool(line: &str) -> bool {
    let mut rest = line.trim_start();
    if let Some(numbered) = rest.strip_prefix('N') {
        let (after, count) = skip_digits(numbered);
        if count == 0 {
            return false;
        }
        rest = after.trim_start();
    }
    let Some(tool) = rest.strip_prefix('T') else {
        return false;
    };
    let (after, count) = skip_digits(tool.trim_start());
    let after = after.strip_prefix('.').map_or(after, |fraction| skip_digits(fraction).0);
    count > 0 && after.trim().is_empty()
}

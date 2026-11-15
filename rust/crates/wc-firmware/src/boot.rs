//! What a boot looks like and what the firmware says about itself (`Connection/firmwareFacts.py`).

/// Printed on every `$` write by 51.292 but not 51.291; B05 reports both as 51.29.
pub const EEPROM_REWRITE_NOTICE: &str = "Change in setting, rewriting EEPROM";
pub const EEPROM_READ_FAIL: &str = "EEPROM read fail";
const BOARD_BANNER: &str = "PCB v1.";
/// A banner this long after the last one is a new boot, for a boot whose `ready` line was lost.
pub const BOOT_BANNER_WINDOW_MS: u64 = 10_000;

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Python's `\bready\b`.
fn has_ready_word(line: &str) -> bool {
    line.match_indices("ready").any(|(at, word)| {
        let before = line[..at].chars().next_back().is_none_or(|c| !is_word(c));
        let after = line[at + word.len()..].chars().next().is_none_or(|c| !is_word(c));
        before && after
    })
}

/// 51.29x prints its prescaler message with no newline, so a banner can arrive glued to other text.
pub fn is_boot_banner(line: &str) -> bool {
    line.contains(BOARD_BANNER) || line.contains("Grbl v1.00") || has_ready_word(line)
}

pub fn ends_boot(line: &str) -> bool {
    has_ready_word(line)
}

/// The PCB and driver banner, e.g. `PCB v1.6 TLE9201  Detected`.
pub fn board_banner(line: &str) -> Option<String> {
    line.find(BOARD_BANNER).map(|at| line[at..].trim().to_string())
}

/// `^Firmware Version\s+([\d.]+)`, as Python's `float()` reads the number.
pub fn parse_firmware_version(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("Firmware Version")?;
    let digits = rest.trim_start();
    if digits.len() == rest.len() {
        return None;
    }
    let number: String = digits.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    number.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The reported version as Python's `format(v, "g")` prints a two-decimal B05 reply.
fn version_text(version: f64) -> String {
    let text = format!("{version}");
    text.strip_suffix(".0").map(String::from).unwrap_or(text)
}

/// The version, with 51.29 narrowed to 51.292 when its rewrite notice was seen, else `51.29x`; `None` if unknown.
pub fn firmware_build(version: f64, rewrite_notice_seen: bool) -> Option<String> {
    if version <= 0.0 {
        return None;
    }
    if (version - 51.29).abs() < 1e-9 {
        return Some(if rewrite_notice_seen { "51.292" } else { "51.29x" }.to_string());
    }
    Some(version_text(version))
}

/// Only holey 51.292 and later store Z limits (`$63`/`$64`); B17-B19 do nothing or corrupt `$43`/`$44` elsewhere.
pub fn supports_z_limits(build: Option<&str>) -> bool {
    match build {
        None | Some("51.29x") => false,
        Some(text) => text.parse::<f64>().is_ok_and(|v| (51.292..100.0).contains(&v)),
    }
}

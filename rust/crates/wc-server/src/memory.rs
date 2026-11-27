//! Whether a gcode file fits in the memory the host has free.

/// What a loaded line costs webcontrol-rs: resident memory with a file loaded, less idle, was 350 bytes a line for
/// 73,631 lines and 305 for 589,040 (macOS, release); the Pi measured 225 to 325.
pub const BYTES_PER_LINE: u64 = 350;
const WARN_PERCENT: u64 = 40;
const REFUSE_PERCENT: u64 = 90;
const MB: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fit {
    Fine,
    Warn(String),
    Refuse(String),
}

/// `MemAvailable` from `/proc/meminfo`, in bytes.
pub fn mem_available(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find_map(|line| line.strip_prefix("MemAvailable:"))?;
    line.trim().strip_suffix("kB")?.trim().parse::<u64>().ok().map(|kb| kb * 1024)
}

/// The host's free memory; `None` where `/proc/meminfo` cannot be read, as on a Mac.
pub fn host_available() -> Option<u64> {
    std::fs::read_to_string("/proc/meminfo").ok().as_deref().and_then(mem_available)
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let first = digits.len() % 3;
    let groups = (first..digits.len()).step_by(3).map(|at| &digits[at..at + 3]);
    std::iter::once(&digits[..first]).filter(|head| !head.is_empty()).chain(groups).collect::<Vec<_>>().join(",")
}

pub fn fit(lines: usize, available: Option<u64>) -> Fit {
    let Some(available) = available.filter(|&bytes| bytes > 0) else { return Fit::Fine };
    let needed = lines as u64 * BYTES_PER_LINE;
    let percent = needed * 100 / available;
    let size = format!(
        "This file has {} lines and needs about {} MB of the {} MB this computer has free.",
        grouped(lines as u64),
        needed.div_ceil(MB),
        available / MB
    );
    let split = "into smaller files and running them one after another with the job queue.";
    match percent {
        p if p >= REFUSE_PERCENT => Fit::Refuse(format!("{size} It was not opened. Try splitting it {split}")),
        p if p >= WARN_PERCENT => Fit::Warn(format!("{size} Consider splitting it {split}")),
        _ => Fit::Fine,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_available_reads_the_kilobytes_line() {
        let meminfo = "MemTotal:         926732 kB\nMemFree:          301284 kB\nMemAvailable:     727104 kB\n";
        assert_eq!(mem_available(meminfo), Some(727_104 * 1024));
        assert_eq!(mem_available("MemTotal: 1 kB\n"), None);
        assert_eq!(mem_available("MemAvailable: lots\n"), None);
    }

    #[test]
    fn lines_are_grouped_in_thousands() {
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(73_631), "73,631");
        assert_eq!(grouped(1_200_000), "1,200,000");
    }

    #[test]
    fn a_file_warns_past_40_percent_of_free_memory_and_is_refused_past_90() {
        let free = 700 * MB;
        let lines = |percent: u64| (free * percent / 100 / BYTES_PER_LINE) as usize;
        assert_eq!(fit(lines(39), Some(free)), Fit::Fine);
        assert!(matches!(fit(lines(41), Some(free)), Fit::Warn(_)));
        assert!(matches!(fit(lines(91), Some(free)), Fit::Refuse(_)));
        assert_eq!(fit(10_000_000, None), Fit::Fine, "no meminfo, no check");
        assert_eq!(fit(10, Some(0)), Fit::Fine);
        let Fit::Warn(note) = fit(1_200_000, Some(free)) else { panic!() };
        assert_eq!(
            note,
            "This file has 1,200,000 lines and needs about 401 MB of the 700 MB this computer has free. Consider \
             splitting it into smaller files and running them one after another with the job queue."
        );
    }
}

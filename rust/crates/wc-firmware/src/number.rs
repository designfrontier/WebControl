//! How the firmware reads the value of a `$n=value` line, which settings it refuses to use out of range, and the
//! text that makes it store the float32 nearest the value WebControl means (`spec/firmware.md` §4).

/// Proportional weights (`pOn`) for main position, main velocity, Z position and Z velocity.
pub const PROPORTIONAL_WEIGHT_KEYS: [u32; 4] = [24, 28, 32, 36];
/// Every PID setting: `$21`-`$36`, the gains and the weights above.
pub const PID_KEYS: std::ops::RangeInclusive<u32> = 21..=36;
/// Nine significant digits pin down any float32.
const SIGNIFICANT_DIGITS: i32 = 9;

/// The parse in progress, field for field the locals of `readFloat`.
#[derive(Clone, Copy)]
struct Read {
    negative: bool,
    fraction: bool,
    value: i32,
    scale: f32,
    digits: u8,
}

impl Read {
    fn take(self, byte: u8) -> Read {
        match byte {
            b'-' => Read { negative: true, ..self },
            b'.' => Read { fraction: true, ..self },
            b'0'..=b'9' => Read {
                value: self.value.wrapping_mul(10).wrapping_add(i32::from(byte - b'0')),
                scale: if self.fraction { self.scale * 0.1 } else { self.scale },
                digits: self.digits.wrapping_add(1),
                ..self
            },
            _ => self,
        }
    }

    fn continues(&self, byte: u8) -> bool {
        byte.is_ascii_digit() || (byte == b'.' && !self.fraction)
    }
}

/// `readFloat` (NutsAndBolts.cpp, the same in 1.26 and every holey build): digits accumulate in a 32-bit `long`,
/// each fraction digit multiplies a float32 by 0.1, and the result is `value * fraction` in float32, so
/// `1.000000` reads as 1.00000012. `None` where the firmware answers `Bad number format`.
pub fn read_float(text: &str) -> Option<f32> {
    let (&first, tail) = text.trim_start_matches(' ').as_bytes().split_first()?;
    let start = Read { negative: false, fraction: false, value: 0, scale: 1.0, digits: 0 }.take(first);
    let read = tail
        .iter()
        .try_fold(start, |read, &byte| if read.continues(byte) { Ok(read.take(byte)) } else { Err(read) })
        .unwrap_or_else(|read| read);
    if read.digits == 0 {
        return None;
    }
    let value = if read.negative { read.value.wrapping_neg() } else { read.value };
    Some(if read.fraction { value as f32 * read.scale } else { value as f32 })
}

/// The values `PID::SetTunings` accepts: it ignores a whole tuning with a gain below 0 or a weight outside [0, 1],
/// which leaves an axis at the zero gains `setup()` gave it, and that axis never moves.
pub fn firmware_range(key: u32) -> Option<(f32, f32)> {
    match key {
        k if PROPORTIONAL_WEIGHT_KEYS.contains(&k) => Some((0.0, 1.0)),
        k if PID_KEYS.contains(&k) => Some((0.0, f32::MAX)),
        _ => None,
    }
}

/// Whether the firmware uses a stored value as it is; a NaN never is, for a key with a range.
pub fn in_firmware_range(key: u32, value: f64) -> bool {
    firmware_range(key).is_none_or(|(low, high)| (f64::from(low)..=f64::from(high)).contains(&value))
}

/// `n` with `decimals` digits after the point.
fn render(negative: bool, n: u32, decimals: usize) -> String {
    let digits = format!("{n:0>width$}", width = decimals + 1);
    let (whole, fraction) = digits.split_at(digits.len() - decimals);
    let sign = if negative && n != 0 { "-" } else { "" };
    if decimals == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{fraction}") }
}

/// The float32 `readFloat` multiplies by after `decimals` fraction digits.
fn scale(decimals: usize) -> f32 {
    (0..decimals).fold(1.0, |scale: f32, _| scale * 0.1)
}

/// Texts with `decimals` fraction digits around `target` as written and as `readFloat`'s drifting scale sees it.
fn candidates(target: f32, decimals: usize) -> impl Iterator<Item = String> {
    let magnitude = f64::from(target.abs());
    let written = (magnitude * 10f64.powi(decimals as i32)).round();
    let scaled = (magnitude / f64::from(scale(decimals))).round();
    let negative = target.is_sign_negative();
    let mut numbers: Vec<f64> =
        [written, scaled].iter().flat_map(|n| (-3..=3).map(move |s| n + f64::from(s))).collect();
    numbers.sort_by(f64::total_cmp);
    numbers.dedup();
    numbers
        .into_iter()
        .filter(|n| (0.0..=f64::from(i32::MAX)).contains(n))
        .map(move |n| render(negative, n as u32, decimals))
}

fn most_decimals(target: f32) -> usize {
    let whole_digits = if target == 0.0 { 1 } else { f64::from(target.abs()).log10().floor() as i32 + 1 };
    (SIGNIFICANT_DIGITS - whole_digits).clamp(0, 45) as usize
}

/// The shortest text `readFloat` turns into the float32 nearest `intended` (kept within the key's range); when
/// none does, whichever of those texts and `fallback` reads closest to it without leaving the range.
pub fn firmware_text(key: u32, intended: f64, fallback: &str) -> String {
    let (low, high) = firmware_range(key).unwrap_or((f32::MIN, f32::MAX));
    let nearest = intended as f32;
    if !nearest.is_finite() {
        return fallback.to_string();
    }
    let target = nearest.clamp(low, high);
    let distance = |text: &String| text.parse::<f64>().map_or(f64::INFINITY, |v| (v - intended).abs());
    let by_decimals = (0..=most_decimals(target)).map(|decimals| candidates(target, decimals).collect::<Vec<_>>());
    let exact = by_decimals.clone().find_map(|texts| {
        let mut exact: Vec<String> = texts.into_iter().filter(|t| read_float(t) == Some(target)).collect();
        exact.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        exact.into_iter().next()
    });
    exact.unwrap_or_else(|| {
        let in_range = |text: &String| read_float(text).is_some_and(|v| (low..=high).contains(&v));
        let off = |text: &String| (f64::from(read_float(text).unwrap_or(f32::NAN)) - f64::from(target)).abs();
        let texts = by_decimals.flatten().chain([fallback.trim().to_string()]).filter(in_range);
        texts.min_by(|a, b| off(a).total_cmp(&off(b)).then(a.len().cmp(&b.len()))).unwrap_or(fallback.to_string())
    })
}

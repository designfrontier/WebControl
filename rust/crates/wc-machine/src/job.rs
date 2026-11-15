//! Just enough of a loaded file to stream it and Stop it: the loader's normalisation and the home shift
//! (`File/gcodeFile.py`). wc-gcode replaces this in M4, with truncate, tool changes and restarts.

/// A shifted coordinate keeps at most this many decimals.
const MAX_SHIFT_DECIMALS: u32 = 6;

/// `re.sub(pattern, ...)` passes over the text once, left to right, never overlapping.
fn collapse_blank_lines(text: &str) -> String {
    text.replace("\n\n", "\n")
}

/// `([0-9])([GXYZIJFTM]) *` -> `\1 \2`: a word glued to the number before it gets a space.
fn space_words(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        out.push(chars[i]);
        if chars[i].is_ascii_digit() && chars.get(i + 1).is_some_and(|c| "GXYZIJFTM".contains(*c)) {
            out.push(' ');
            out.push(chars[i + 1]);
            i += 2;
            while chars.get(i) == Some(&' ') {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    out
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    text.chars().for_each(|c| {
        if !(c == ' ' && out.ends_with(' ')) {
            out.push(c);
        }
    });
    out
}

/// The file's lines as `/openGCode` stores them: each ends in a space, words spaced, runs of spaces collapsed.
pub fn load(text: &str) -> Vec<String> {
    let text = collapse_spaces(&space_words(&collapse_blank_lines(text)));
    text.split('\n')
        .map(|line| {
            let line = format!("{} ", line.trim_start());
            ["X", "Y", "Z", "I", "J", "F"].iter().fold(line, |line, axis| line.replace(&format!("{axis} "), axis))
        })
        .collect()
}

/// A decimal as `mantissa * 10^-scale`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Decimal {
    mantissa: i128,
    scale: i32,
}

impl Decimal {
    /// Python's `Decimal(text)` for `[+-]digits[.digits][e[+-]digits]`.
    fn parse(text: &str) -> Option<Decimal> {
        let (number, exponent) = match text.find(['e', 'E']) {
            Some(at) => (&text[..at], text[at + 1..].parse::<i32>().ok()?),
            None => (text, 0),
        };
        let (negative, unsigned) = match number.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, number.strip_prefix('+').unwrap_or(number)),
        };
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let digits = format!("{whole}{fraction}");
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) || digits.len() > 30 {
            return None;
        }
        let magnitude: i128 = digits.parse().ok()?;
        let mantissa = if negative { -magnitude } else { magnitude };
        Some(Decimal { mantissa, scale: fraction.len() as i32 - exponent })
    }

    /// `Decimal.normalize()`: no trailing zeros in the mantissa.
    fn normalized(self) -> Decimal {
        match self.mantissa {
            0 => Decimal { mantissa: 0, scale: 0 },
            m if m % 10 == 0 => Decimal { mantissa: m / 10, scale: self.scale - 1 }.normalized(),
            _ => self,
        }
    }

    fn rescaled(self, scale: i32) -> Option<i128> {
        self.mantissa.checked_mul(10i128.checked_pow(u32::try_from(scale - self.scale).ok()?)?)
    }
}

/// Rounds half to even to `places` decimals and prints it; only an exact zero loses its sign.
fn format_decimal(mantissa: i128, scale: u32, places: u32) -> String {
    let drop = 10i128.pow(scale - places);
    let (quotient, remainder) = (mantissa / drop, (mantissa % drop).abs());
    let rounds_up = remainder * 2 > drop || (remainder * 2 == drop && quotient % 2 != 0);
    let rounded = if rounds_up { quotient + mantissa.signum() } else { quotient };
    let digits = format!("{:0>width$}", rounded.abs(), width = places as usize + 1);
    let (whole, fraction) = digits.split_at(digits.len() - places as usize);
    let sign = if mantissa < 0 { "-" } else { "" };
    if places == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{fraction}") }
}

/// The first `axis` word's number plus `shift`, keeping the longer operand's decimals.
fn shift_axis(line: &str, axis: char, shift: f64) -> Option<String> {
    let Some(at) = line.find(axis).filter(|at| at + 1 < line.len()) else {
        return Some(line.to_string());
    };
    let rest = &line[at + 1..];
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    let after_spaces = &rest[spaces..];
    let sign = usize::from(after_spaces.starts_with(['+', '-']));
    let whole = after_spaces[sign..].chars().take_while(char::is_ascii_digit).count();
    let tail = &after_spaces[sign + whole..];
    let fraction = match tail.strip_prefix('.') {
        Some(digits) => match digits.chars().take_while(char::is_ascii_digit).count() {
            0 => 0,
            n => n + 1,
        },
        None => 0,
    };
    let end = spaces + sign + whole + fraction;
    let original = Decimal::parse(rest[..end].trim())?;
    let offset = Decimal::parse(&wc_config::python_float_str(shift))?.normalized();
    let scale = original.scale.max(offset.scale).max(0);
    let sum = original.rescaled(scale)?.checked_add(offset.rescaled(scale)?)?;
    let places = (scale as u32).min(MAX_SHIFT_DECIMALS);
    let text = format_decimal(sum, scale as u32, places);
    Some(format!("{}{text}{}", &line[..=at], &rest[end..]))
}

/// `gcodeFile.moveLine`: shifts the first X and Y of an absolute move by the home position; a comment after
/// the line is kept as it is. A line that will not parse goes out unshifted.
pub fn move_line(line: &str, home: (f64, f64)) -> String {
    if line.starts_with(['(', ';']) {
        return line.to_string();
    }
    let (open, semicolon) = (line.find('('), line.find(';'));
    // Python compares with find()'s -1, so a `(` counts only when a `;` follows it.
    let split = match (open, semicolon) {
        (Some(a), Some(b)) if a < b => Some(a),
        _ => semicolon,
    };
    let (code, comment) = split.map_or((line, ""), |at| line.split_at(at));
    let upper = format!("{} ", code.to_uppercase());
    shift_axis(&upper, 'X', home.0)
        .and_then(|shifted| shift_axis(&shifted, 'Y', home.1))
        .map_or_else(|| line.to_string(), |shifted| shifted + comment)
}

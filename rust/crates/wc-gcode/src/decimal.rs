//! Python's `Decimal` sum for the home shift: a coordinate as written plus the repr of a float shift, under the
//! default 28-digit context (ties to even), printed at fixed places.

use std::cmp::Ordering;

use rust_decimal::{Decimal, RoundingStrategy};

const PRECISION: usize = 28;

/// A decimal as written: `±digits × 10^exponent`, digits most significant first, leading zeros kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    negative: bool,
    digits: Vec<u8>,
    exponent: i32,
}

impl Written {
    /// Python's `Decimal(text)` for `[+-]?digits[.digits]` with at least one digit.
    pub fn parse(text: &str) -> Option<Written> {
        let (negative, unsigned) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        let digits: Vec<u8> = whole.bytes().chain(fraction.bytes()).map(|b| b.wrapping_sub(b'0')).collect();
        if digits.is_empty() || digits.iter().any(|&d| d > 9) {
            return None;
        }
        Some(Written { negative, digits, exponent: -(fraction.len() as i32) })
    }

    /// A float's `repr`, which may use an exponent; `None` for `nan` and `inf`.
    pub fn from_repr(text: &str) -> Option<Written> {
        let (mantissa, exponent) = text.split_once('e').unwrap_or((text, "0"));
        let written = Written::parse(mantissa)?;
        Some(Written { exponent: written.exponent + exponent.parse::<i32>().ok()?, ..written })
    }

    /// The decimal places as written, which is what Python's exponent gives.
    pub fn places(&self) -> u32 {
        (-self.exponent).max(0) as u32
    }

    /// The places after Python's `normalize()`, which drops trailing zeros.
    pub fn normalized_places(&self) -> u32 {
        let zeros = self.digits.iter().rev().take_while(|&&d| d == 0).count();
        if zeros == self.digits.len() { 0 } else { (-(self.exponent + zeros as i32)).max(0) as u32 }
    }

    /// One past the power of ten of the most significant non-zero digit.
    fn top(&self) -> i32 {
        let significant = self.digits.iter().skip_while(|&&d| d == 0).count();
        self.exponent + significant as i32
    }

    fn to_decimal(&self) -> Option<Decimal> {
        let mantissa = self.digits.iter().try_fold(0i128, |acc, &d| acc.checked_mul(10)?.checked_add(i128::from(d)))?;
        let scaled = mantissa.checked_mul(10i128.checked_pow(self.exponent.max(0) as u32)?)?;
        let mut value = Decimal::try_from_i128_with_scale(scaled, (-self.exponent).max(0) as u32).ok()?;
        value.set_sign_negative(self.negative);
        Some(value)
    }
}

/// `f"{(value if value else abs(value)):.{places}f}"` for `value = original + offset`.
pub fn sum_text(original: &Written, offset: &Written, places: u32) -> String {
    let lowest = original.exponent.min(offset.exponent);
    let span = original.top().max(offset.top()) - lowest + 1;
    let fits = span <= PRECISION as i32 && -lowest <= PRECISION as i32;
    let exact = fits.then(|| original.to_decimal()?.checked_add(offset.to_decimal()?)).flatten();
    exact.map_or_else(|| long_sum_text(original, offset, places), |sum| decimal_text(sum, places))
}

/// The common case: both numbers and their exact sum fit a `rust_decimal`, so the 28-digit context never rounds.
fn decimal_text(value: Decimal, places: u32) -> String {
    let negative = value.is_sign_negative() && !value.is_zero();
    let mut rounded = value.abs().round_dp_with_strategy(places, RoundingStrategy::MidpointNearestEven);
    rounded.rescale(places);
    if negative { format!("-{rounded}") } else { rounded.to_string() }
}

/// Digits least significant first, so carries and padding happen at the front.
type Magnitude = Vec<u8>;

fn aligned(written: &Written, exponent: i32) -> Magnitude {
    let padding = std::iter::repeat_n(0, (written.exponent - exponent) as usize);
    padding.chain(written.digits.iter().rev().copied()).collect()
}

fn trimmed(mut digits: Magnitude) -> Magnitude {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    digits
}

fn compare(a: &[u8], b: &[u8]) -> Ordering {
    let (a, b) = (trimmed(a.to_vec()), trimmed(b.to_vec()));
    a.len().cmp(&b.len()).then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

fn add(a: &[u8], b: &[u8]) -> Magnitude {
    let mut carry = 0;
    let mut sum: Magnitude = (0..a.len().max(b.len()))
        .map(|i| {
            let total = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
            carry = total / 10;
            total % 10
        })
        .collect();
    if carry > 0 {
        sum.push(carry);
    }
    sum
}

/// `a - b` for `a >= b`.
fn subtract(a: &[u8], b: &[u8]) -> Magnitude {
    let mut borrow = 0;
    a.iter()
        .enumerate()
        .map(|(i, &digit)| {
            let taken = b.get(i).copied().unwrap_or(0) + borrow;
            borrow = u8::from(digit < taken);
            digit + 10 * borrow - taken
        })
        .collect()
}

/// Drops the `count` lowest digits, ties to even.
fn round_off(mut digits: Magnitude, count: usize) -> Magnitude {
    digits.resize(digits.len().max(count + 1), 0);
    let (dropped, kept) = digits.split_at(count);
    let first = dropped.last().copied().unwrap_or(0);
    let beyond = dropped.iter().rev().skip(1).any(|&d| d != 0);
    let up = first > 5 || (first == 5 && (beyond || kept[0] % 2 == 1));
    if up { add(kept, &[1]) } else { kept.to_vec() }
}

fn long_sum_text(original: &Written, offset: &Written, places: u32) -> String {
    let lowest = original.exponent.min(offset.exponent);
    let (a, b) = (aligned(original, lowest), aligned(offset, lowest));
    let (negative, sum) = if original.negative == offset.negative {
        (original.negative, add(&a, &b))
    } else {
        match compare(&a, &b) {
            Ordering::Less => (offset.negative, subtract(&b, &a)),
            _ => (original.negative, subtract(&a, &b)),
        }
    };
    let (mut sum, mut exponent) = (trimmed(sum), lowest);
    while sum.len() > PRECISION {
        let excess = sum.len() - PRECISION;
        sum = trimmed(round_off(sum, excess));
        exponent += excess as i32;
    }
    let zero = sum.iter().all(|&d| d == 0);
    let target = -(places as i32);
    let digits = if exponent >= target {
        aligned(&Written { negative: false, digits: sum.iter().rev().copied().collect(), exponent }, target)
    } else {
        round_off(sum, (target - exponent) as usize)
    };
    let mut text: Vec<u8> = digits.iter().rev().map(|d| d + b'0').collect();
    let width = places as usize + 1;
    if text.len() < width {
        text.splice(0..0, std::iter::repeat_n(b'0', width - text.len()));
    }
    let point = text.len() - places as usize;
    let mut body = String::from_utf8(text[..point].to_vec()).expect("ascii digits");
    let whole = body.trim_start_matches('0');
    body = if whole.is_empty() { "0".into() } else { whole.into() };
    if places > 0 {
        body.push('.');
        body.push_str(std::str::from_utf8(&text[point..]).expect("ascii digits"));
    }
    if negative && !zero { format!("-{body}") } else { body }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(original: &str, offset: &str, places: u32) -> String {
        sum_text(&Written::parse(original).unwrap(), &Written::from_repr(offset).unwrap(), places)
    }

    #[test]
    fn written_numbers_keep_their_places() {
        assert_eq!(Written::parse("10.50").unwrap().places(), 2);
        assert_eq!(Written::parse("5.").unwrap().places(), 0);
        assert_eq!(Written::parse("+.5").unwrap().places(), 1);
        assert_eq!(Written::parse(""), None);
        assert_eq!(Written::parse("."), None);
        assert_eq!(Written::parse("1a"), None);
        assert_eq!(Written::from_repr("nan"), None);
        assert_eq!(Written::from_repr("1e-05").unwrap().normalized_places(), 5);
        assert_eq!(Written::from_repr("100.0").unwrap().normalized_places(), 0);
        assert_eq!(Written::from_repr("-0.0").unwrap().normalized_places(), 0);
        assert_eq!(Written::from_repr("1.50").unwrap().normalized_places(), 1);
    }

    #[test]
    fn short_sums_are_exact() {
        assert_eq!(sum("95.8967", "0.0001", 4), "95.8968");
        assert_eq!(sum("-41.6763", "41.6763", 4), "0.0000");
        assert_eq!(sum("-0.0000001", "0.0", 6), "-0.000000");
    }

    #[test]
    fn long_sums_round_to_28_digits_then_to_the_places() {
        assert_eq!(sum("1", "1e+30", 0), "1000000000000000000000000000000");
        assert_eq!(sum("123456789012345678901234567890", "0.0", 0), "123456789012345678901234567900");
        assert_eq!(sum("0.99999999999999999999999999999", "0.0", 6), "1.000000");
        assert_eq!(sum("32.123456789012345678901234567", "0.0", 6), "32.123457");
        assert_eq!(sum("-1.0000000000000000000000000000001", "1.0", 6), "-0.000000");
        assert_eq!(sum("1.0000000000000000000000000000001", "-1.0", 6), "0.000000");
        assert_eq!(sum("-5", "1e+30", 0), "1000000000000000000000000000000");
        assert_eq!(sum("1", "-1e+30", 2), "-1000000000000000000000000000000.00");
        assert_eq!(sum("0.0000000000000000000000000000025", "0.0", 30), "0.000000000000000000000000000002");
        assert_eq!(sum("0.0000000000000000000000000000035", "0.0", 30), "0.000000000000000000000000000004");
        assert_eq!(sum("0.00000000000000000000000000000251", "0.0", 30), "0.000000000000000000000000000003");
    }

    #[test]
    fn ties_go_to_even_in_both_paths() {
        assert_eq!(sum("0.125", "0.0", 2), "0.12");
        assert_eq!(sum("0.135", "0.0", 2), "0.14");
        assert_eq!(sum("2.5000000000000000000000000000000", "0.0", 0), "2");
        assert_eq!(sum("3.5000000000000000000000000000000", "0.0", 0), "4");
    }
}

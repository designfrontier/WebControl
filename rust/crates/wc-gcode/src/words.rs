//! Hand-written scanners for the Python regexes that use look-around, which the `regex` crate lacks.

use std::sync::LazyLock;

use regex::Regex;

use crate::py;

/// One match of Python's `L(?=.)(([ ]*)?[+-]?([0-9]*)(\.([0-9]+))?)`: the letter at `start`, the number after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word<'a> {
    pub start: usize,
    pub end: usize,
    pub text: &'a str,
}

/// The first `letter` followed by any character but a newline, and the number text after it (possibly empty).
pub fn find(line: &str, letter: char) -> Option<Word<'_>> {
    let bytes = line.as_bytes();
    let start = line
        .char_indices()
        .find(|&(index, c)| c == letter && bytes.get(index + 1).is_some_and(|&next| next != b'\n'))?
        .0;
    let after = start + 1;
    let mut end = after + count_while(&bytes[after..], |b| b == b' ');
    end += usize::from(matches!(bytes.get(end), Some(b'+' | b'-')));
    end += count_while(&bytes[end..], |b| b.is_ascii_digit());
    let fraction = count_while(bytes.get(end + 1..).unwrap_or_default(), |b| b.is_ascii_digit());
    if bytes.get(end) == Some(&b'.') && fraction > 0 {
        end += 1 + fraction;
    }
    Some(Word { start, end, text: &line[after..end] })
}

fn count_while(bytes: &[u8], keep: impl Fn(u8) -> bool) -> usize {
    bytes.iter().take_while(|&&b| keep(b)).count()
}

/// The word's number as Python's `float()` reads it: `Ok(None)` without the word, `Err` when it cannot be read.
pub fn value(line: &str, letter: char) -> Result<Option<f64>, ()> {
    find(line, letter).map_or(Ok(None), |word| py::float(word.text).map(Some).ok_or(()))
}

static G_NUMBERS: LazyLock<Regex> = LazyLock::new(|| number_words('G'));
static M_NUMBERS: LazyLock<Regex> = LazyLock::new(|| number_words('M'));

fn number_words(letter: char) -> Regex {
    Regex::new(&format!(r"{letter}[\s\x1c-\x1f]*([0-9]+(?:\.[0-9]+)?)")).expect("valid pattern")
}

/// `gcodeNumbers`: the numbers of the line's G or M words, so M3 and M03 give 3, M30 gives 30 and G91.1 gives 91.1.
pub fn numbers(letter: char, line: &str) -> Vec<f64> {
    let pattern = if letter == 'G' { &G_NUMBERS } else { &M_NUMBERS };
    pattern.captures_iter(line).filter_map(|found| found[1].parse().ok()).collect()
}

/// Python's `re.sub(r'\(([^)]*)\)', replacement, line)`.
pub fn replace_parens(line: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else { break };
        out.push_str(&rest[..open]);
        out.push_str(replacement);
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// Python's `re.sub(r';([^\n]*)?', replacement, line)`.
pub fn replace_semicolons(line: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find(';') {
        out.push_str(&rest[..start]);
        out.push_str(replacement);
        rest = &rest[start..];
        rest = &rest[rest.find('\n').unwrap_or(rest.len())..];
    }
    out.push_str(rest);
    out
}

/// Comments replaced by newlines, the way the Python parsers blank them before `isspace()`.
pub fn blank_comments(line: &str) -> String {
    replace_semicolons(&replace_parens(line, ""), "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &str, letter: char) -> Option<&str> {
        find(line, letter).map(|word| word.text)
    }

    #[test]
    fn a_word_is_the_first_letter_with_something_after_it() {
        assert_eq!(text("G1 X10.5 Y2 ", 'X'), Some("10.5"));
        assert_eq!(text("G1 X 10.5", 'X'), Some(" 10.5"));
        assert_eq!(text("G1 X-.5", 'X'), Some("-.5"));
        assert_eq!(text("G1 X5. ", 'X'), Some("5"));
        assert_eq!(text("G1 X. Y2", 'X'), Some(""));
        assert_eq!(text("G1 XY", 'X'), Some(""));
        assert_eq!(text("G1 X", 'X'), None);
        assert_eq!(text("X\nX1", 'X'), Some("1"));
        assert_eq!(text("ü X+3 ", 'X'), Some("+3"));
        assert_eq!(find("G1 X12 Y", 'X'), Some(Word { start: 3, end: 6, text: "12" }));
    }

    #[test]
    fn a_value_is_read_with_python_float() {
        assert_eq!(value("G1 Z-1.25 ", 'Z'), Ok(Some(-1.25)));
        assert_eq!(value("G1 X1 ", 'Z'), Ok(None));
        assert_eq!(value("G1 Z`3 ", 'Z'), Err(()));
        assert_eq!(value("G1 Z - ", 'Z'), Err(()));
    }

    #[test]
    fn numbers_read_every_word_of_a_letter() {
        assert_eq!(numbers('G', "G91.1 G0 G 21"), [91.1, 0.0, 21.0]);
        assert_eq!(numbers('M', "M03 S1000 M8"), [3.0, 8.0]);
        assert!(numbers('M', "G1 X1").is_empty());
    }

    #[test]
    fn comments_are_replaced_like_python_re_sub() {
        assert_eq!(replace_parens("G1 (a) X1 (b", ""), "G1  X1 (b");
        assert_eq!(replace_parens("((a) b)", "#"), "# b)");
        assert_eq!(replace_semicolons("G1 ; a ; b", ""), "G1 ");
        assert_eq!(replace_semicolons("a;b\nc;d", "\n"), "a\n\nc\n");
        assert_eq!(blank_comments("(c) G1 ; x"), " G1 \n");
    }
}

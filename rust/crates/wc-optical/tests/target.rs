//! The printable target, read back from its PDF: page sizes, cross-references and true-size squares.

use wc_optical::render::Target;
use wc_optical::target::{paper_mm, pdf};
use wc_optical::{OpticalError, Paper};

const PT_PER_MM: f64 = 72.0 / 25.4;
const NOMINAL: Target = Target { marker_mm: (15.24, 15.24), pitch_mm: (76.2, 76.2) };

/// One page's filled rectangles in mm (x, y, w, h), and the strings it shows.
struct Page {
    squares: Vec<[f64; 4]>,
    words: Vec<String>,
    media_box: Vec<f64>,
}

fn object(text: &str, number: usize) -> &str {
    let head = format!("\n{number} 0 obj\n");
    let start = text.find(&head).unwrap_or_else(|| panic!("no object {number}")) + head.len();
    &text[start..start + text[start..].find("\nendobj\n").unwrap()]
}

fn numbers(text: &str) -> Vec<f64> {
    text.split_whitespace().filter_map(|word| word.trim_matches(['[', ']']).parse().ok()).collect()
}

fn pages(bytes: &[u8]) -> Vec<Page> {
    let text = std::str::from_utf8(bytes).unwrap();
    let kids = object(text, 2);
    let refs: Vec<usize> = kids[kids.find('[').unwrap() + 1..kids.find(']').unwrap()]
        .split(" 0 R")
        .filter_map(|n| n.trim().parse().ok())
        .collect();
    refs.iter()
        .map(|page| {
            let dict = object(text, *page);
            let media = &dict[dict.find("/MediaBox").unwrap()..];
            let contents: usize = numbers(&dict[dict.find("/Contents").unwrap() + 9..])[0] as usize;
            let stream = object(text, contents);
            let length = numbers(&stream[..stream.find(">>").unwrap()])[0] as usize;
            let body = &stream[stream.find("stream\n").unwrap() + 7..];
            assert_eq!(&body[length..], "endstream", "/Length is exact");
            Page {
                squares: body[..length]
                    .lines()
                    .filter(|line| line.ends_with(" re f"))
                    .map(|line| {
                        let n = numbers(line);
                        [n[0], n[1], n[2], n[3]].map(|pt| pt / PT_PER_MM)
                    })
                    .collect(),
                words: body
                    .lines()
                    .filter_map(|line| Some(line.split_once('(')?.1.rsplit_once(')')?.0.to_string()))
                    .collect(),
                media_box: numbers(&media[..media.find(']').unwrap()]),
            }
        })
        .collect()
}

fn check_cross_references(bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(text.starts_with("%PDF-1.4\n") && text.ends_with("%%EOF\n"));
    let xref: usize = text[text.rfind("startxref\n").unwrap() + 10..].lines().next().unwrap().parse().unwrap();
    assert!(text[xref..].starts_with("xref\n0 "));
    let entries: Vec<&str> = text[xref..].lines().skip(3).take_while(|line| !line.starts_with("trailer")).collect();
    entries.iter().enumerate().for_each(|(n, entry)| {
        assert_eq!(entry.len() + 1, 20, "{entry:?}");
        let offset: usize = entry[..10].parse().unwrap();
        assert!(text[offset..].starts_with(&format!("{} 0 obj\n", n + 1)), "object {} at {offset}", n + 1);
    });
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.002, "{a} vs {b}");
}

#[test]
fn every_square_prints_at_true_size_on_both_papers() {
    [(Paper::Letter, 612.0, 792.0), (Paper::A4, 595.28, 841.89)].iter().for_each(|(paper, w, h)| {
        let bytes = pdf(&NOMINAL, *paper).unwrap();
        check_cross_references(&bytes);
        let pages = pages(&bytes);
        assert_eq!(pages.len(), 16 * 5, "2 x 3 squares a page");
        assert_eq!(pages.iter().map(|page| page.squares.len()).sum::<usize>(), 31 * 15);
        pages.iter().for_each(|page| {
            assert_eq!(page.media_box, vec![0.0, 0.0, *w, *h]);
            page.squares.iter().for_each(|square| {
                close(square[2], 15.24);
                close(square[3], 15.24);
            });
            let (paper_w, paper_h) = paper_mm(*paper);
            assert!(
                page.squares.iter().all(|s| s[0] > 10.0
                    && s[0] + s[2] < paper_w - 10.0
                    && s[1] > 18.0
                    && s[1] + s[3] < paper_h - 18.0)
            );
        });
        let full = &pages[0].squares;
        assert_eq!(full.len(), 6);
        close(full[1][0] - full[0][0], 76.2);
        close(full[0][1] - full[2][1], 76.2);
    });
}

#[test]
fn pages_say_which_squares_they_hold() {
    let pages = pages(&pdf(&NOMINAL, Paper::Letter).unwrap());
    assert!(pages[0].words[0].starts_with("WebControl optical target, page 1 of 80: columns -15 to -14, rows 7 to 5;"));
    assert!(pages[15].words[0].contains("page 16 of 80: columns 15 to 15, rows 7 to 5"), "{}", pages[15].words[0]);
    assert!(pages[79].words[0].contains("columns 15 to 15, rows -5 to -7"));
    assert_eq!(pages[79].squares.len(), 3);
    assert!(pages[0].words.iter().any(|w| w == "100 mm: scaleX = measured / 100"));
    assert!(pages[0].words.iter().any(|w| w == "100 mm: scaleY = measured / 100"));
    assert!(pages[0].words[1].starts_with("Print at 100% \\(no fit to page\\)."), "parentheses are escaped");
}

#[test]
fn a_bigger_target_takes_fewer_squares_a_page() {
    let wide = Target { marker_mm: (25.4, 20.0), pitch_mm: (101.6, 101.6) };
    let pages = pages(&pdf(&wide, Paper::A4).unwrap());
    assert_eq!(pages.len(), 31 * 8, "1 x 2 squares a page");
    close(pages[0].squares[0][2], 25.4);
    close(pages[0].squares[0][3], 20.0);
}

#[test]
fn a_target_that_cannot_print_is_refused() {
    let invalid = |target: Target| match pdf(&target, Paper::Letter) {
        Err(OpticalError::Invalid(why)) => why,
        other => panic!("{other:?}"),
    };
    assert!(invalid(Target { pitch_mm: (300.0, 76.2), ..NOMINAL }).ends_with("does not fit on Letter paper"));
    invalid(Target { pitch_mm: (76.2, 0.0), ..NOMINAL });
    invalid(Target { marker_mm: (80.0, 15.24), ..NOMINAL });
    invalid(Target { marker_mm: (15.24, 0.0), ..NOMINAL });
}

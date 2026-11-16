//! Track B: the printable target, tiled across pages with trim lines and scale bars.

use crate::grid::{COLUMNS, ROWS};
use crate::render::Target;
use crate::{OpticalError, Paper, Result};

const PT_PER_MM: f64 = 72.0 / 25.4;
const SIDE_MARGIN_MM: f64 = 10.0;
/// Room above the squares for the label and below them for the scale bar.
const END_MARGIN_MM: f64 = 18.0;
const BAR_MM: f64 = 100.0;

/// Portrait page size in mm.
pub fn paper_mm(paper: Paper) -> (f64, f64) {
    match paper {
        Paper::Letter => (215.9, 279.4),
        Paper::A4 => (210.0, 297.0),
    }
}

/// A PDF of black squares at true size, one tile per page, each page labelled with its rows and columns.
/// Pass the nominal target: `scaleX`/`scaleY` describe a printer's error, which the PDF should not copy.
pub fn pdf(target: &Target, paper: Paper) -> Result<Vec<u8>> {
    let (width, height) = paper_mm(paper);
    let (pitch_x, pitch_y) = target.pitch_mm;
    let fits = |pitch: f64, room: f64| if pitch > 0.0 { (room / pitch).floor() as usize } else { 0 };
    let per_page = (fits(pitch_x, width - 2.0 * SIDE_MARGIN_MM), fits(pitch_y, height - 2.0 * END_MARGIN_MM));
    let (mx, my) = target.marker_mm;
    if per_page.0 == 0 || per_page.1 == 0 || !(mx > 0.0 && my > 0.0 && mx <= pitch_x && my <= pitch_y) {
        return Err(OpticalError::Invalid(format!(
            "A {mx} x {my} mm square at {pitch_x} x {pitch_y} mm pitch does not fit on {paper:?} paper"
        )));
    }
    let columns: Vec<_> = chunks(COLUMNS, per_page.0, 15, 1);
    let rows: Vec<_> = chunks(ROWS, per_page.1, 7, -1);
    let total = columns.len() * rows.len();
    let tiles = rows.iter().flat_map(|row| columns.iter().map(move |column| (*column, *row)));
    let pages = tiles
        .enumerate()
        .map(|(n, (column, row))| Tile { target, per_page, paper: (width, height), column, row, page: n + 1, total })
        .map(|tile| tile.content())
        .collect();
    Ok(document(pages, (width * PT_PER_MM, height * PT_PER_MM)))
}

/// Grid indices split into pages: (first index, count), walking from `-half * step` by `step`.
fn chunks(cells: usize, per_page: usize, half: i32, step: i32) -> Vec<(i32, usize)> {
    (0..cells).step_by(per_page).map(|start| (step * (start as i32 - half), per_page.min(cells - start))).collect()
}

struct Tile<'a> {
    target: &'a Target,
    per_page: (usize, usize),
    paper: (f64, f64),
    column: (i32, usize),
    row: (i32, usize),
    page: usize,
    total: usize,
}

impl Tile<'_> {
    fn content(&self) -> String {
        let (width, height) = self.paper;
        let (pitch_x, pitch_y) = self.target.pitch_mm;
        let (mx, my) = self.target.marker_mm;
        let left = (width - self.per_page.0 as f64 * pitch_x) / 2.0;
        let top = (height + self.per_page.1 as f64 * pitch_y) / 2.0;
        let (block_w, block_h) = (self.column.1 as f64 * pitch_x, self.row.1 as f64 * pitch_y);
        let squares: String = (0..self.column.1 * self.row.1)
            .map(|n| {
                let (cx, cy) = (
                    left + ((n % self.column.1) as f64 + 0.5) * pitch_x,
                    top - ((n / self.column.1) as f64 + 0.5) * pitch_y,
                );
                format!("{} {} {} {} re f\n", pt(cx - mx / 2.0), pt(cy - my / 2.0), pt(mx), pt(my))
            })
            .collect();
        let trim =
            format!("q 0.3 w [3 3] 0 d {} {} {} {} re S Q\n", pt(left), pt(top - block_h), pt(block_w), pt(block_h));
        let corners: String =
            [(left, top), (left + block_w, top), (left, top - block_h), (left + block_w, top - block_h)]
                .iter()
                .map(|(x, y)| cross(*x, *y))
                .collect();
        let last = |(first, count): (i32, usize), step: i32| first + step * (count as i32 - 1);
        let label = format!(
            "WebControl optical target, page {} of {}: columns {} to {}, rows {} to {}; pitch {:.2} x {:.2} mm, square {:.2} x {:.2} mm",
            self.page,
            self.total,
            self.column.0,
            last(self.column, 1),
            self.row.0,
            last(self.row, -1),
            pitch_x,
            pitch_y,
            mx,
            my
        );
        let note = "Print at 100% (no fit to page). Trim on the dashed line and butt the pages edge to edge.";
        let bar_y = top - block_h - 12.0;
        let bar_y_up = top - block_h / 2.0 - BAR_MM / 2.0;
        format!(
            "0 g\n{squares}{trim}{corners}{}{}{}{}{}{}",
            text((left, top + 10.0), false, &label),
            text((left, top + 5.0), false, note),
            bar((left, bar_y), (BAR_MM, 0.0)),
            text((left + BAR_MM + 3.0, bar_y - 1.0), false, "100 mm: scaleX = measured / 100"),
            bar((width - 6.0, bar_y_up), (0.0, BAR_MM)),
            text((width - 9.0, bar_y_up), true, "100 mm: scaleY = measured / 100"),
        )
    }
}

fn pt(mm: f64) -> String {
    format!("{:.3}", mm * PT_PER_MM)
}

fn line((x0, y0): (f64, f64), (x1, y1): (f64, f64)) -> String {
    format!("{} {} m {} {} l S\n", pt(x0), pt(y0), pt(x1), pt(y1))
}

fn cross(x: f64, y: f64) -> String {
    format!("q 0.3 w {}{}Q\n", line((x - 4.0, y), (x + 4.0, y)), line((x, y - 4.0), (x, y + 4.0)))
}

/// A ruler `length` long from `start`, ticked every 10 mm; the vertical one is for `scaleY`.
fn bar(start: (f64, f64), length: (f64, f64)) -> String {
    let end = (start.0 + length.0, start.1 + length.1);
    let ticks: String = (0..=10)
        .map(|n| {
            let at = (start.0 + length.0 * f64::from(n) / 10.0, start.1 + length.1 * f64::from(n) / 10.0);
            let size = if n % 5 == 0 { 2.5 } else { 1.5 };
            let (dx, dy) = if length.1 == 0.0 { (0.0, size) } else { (size, 0.0) };
            line((at.0 - dx, at.1 - dy), (at.0 + dx, at.1 + dy))
        })
        .collect();
    format!("q 0.4 w {}{ticks}Q\n", line(start, end))
}

/// 7 pt Helvetica at `at`, turned to read upwards when `turned`.
fn text(at: (f64, f64), turned: bool, words: &str) -> String {
    let escaped: String =
        words.chars().flat_map(|c| if "()\\".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
    let matrix = if turned { "0 1 -1 0" } else { "1 0 0 1" };
    format!("BT /F1 7 Tf {matrix} {} {} Tm ({escaped}) Tj ET\n", pt(at.0), pt(at.1))
}

/// An uncompressed PDF 1.4: catalog, page tree, Helvetica, then a page and its content stream per tile.
fn document(pages: Vec<String>, (width, height): (f64, f64)) -> Vec<u8> {
    let kids: Vec<String> = (0..pages.len()).map(|n| format!("{} 0 R", 4 + 2 * n)).collect();
    let head = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let page_objects = pages.iter().enumerate().flat_map(|(n, content)| {
        [
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.2} {height:.2}] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                5 + 2 * n
            ),
            format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        ]
    });
    let objects: Vec<String> = head.into_iter().chain(page_objects).collect();
    let mut out = String::from("%PDF-1.4\n");
    let offsets: Vec<usize> = objects
        .iter()
        .enumerate()
        .map(|(n, body)| {
            let at = out.len();
            out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", n + 1));
            at
        })
        .collect();
    let xref = out.len();
    let entries: String = offsets.iter().map(|at| format!("{at:010} 00000 n \n")).collect();
    out.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n{entries}trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1,
        objects.len() + 1
    ));
    out.into_bytes()
}

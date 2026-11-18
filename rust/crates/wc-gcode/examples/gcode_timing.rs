//! Times each step of opening a gcode file, for the load time in spec/rust-checklists/files-boards.md:
//! `gcode_timing <file.nc> [home_x home_y]` (home in mm).

use std::time::Instant;

use wc_gcode::board::Board;
use wc_gcode::shift::{HomeShift, move_line};
use wc_gcode::{Units, load, preview, restart};

fn timed<T>(label: &str, step: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = step();
    println!("{label:<24} {:>9.1} ms", start.elapsed().as_secs_f64() * 1000.0);
    result
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: gcode_timing <file.nc> [home_x home_y]");
        std::process::exit(2);
    };
    let home = |index: usize| args.get(index).and_then(|text| text.parse().ok()).unwrap_or(0.0);
    let total = Instant::now();
    let raw = timed("read", || std::fs::read_to_string(path).expect("a readable UTF-8 file"));
    let lines = timed("load filter", || load::filter(&load::universal_newlines(&raw), None).expect("loads"));
    let z_moves = timed("zMoves", || load::z_moves(&lines, 0.5));
    let drawn = timed("3D preview", || preview::render(&lines, Units::Mm));
    let payload = timed("gzip payload", || preview::compress(&drawn.moves));
    let shift = HomeShift::from_home(home(2), home(3), None);
    timed("home shift, every line", || lines.iter().map(|line| move_line(line, &shift).len()).sum::<usize>());
    timed("restart state at end", || restart::state_at(&lines, lines.len(), home(2), home(3), Units::Mm).is_ok());
    let mut board = Board::for_bed(2438.4, 1219.2);
    timed("board cut grid", || board.process_gcode(&drawn.moves, (shift.x, shift.y), Units::Mm)).expect("a cut grid");
    let cut = board.cut_points.iter().filter(|cell| **cell == wc_gcode::board::Cell::Cut(true)).count();
    println!("{:<24} {:>9.1} ms", "total", total.elapsed().as_secs_f64() * 1000.0);
    println!(
        "{} lines, {} Z moves, {} preview moves, {} byte payload, {cut} board cells cut",
        lines.len(),
        z_moves.indices.len(),
        drawn.moves.len(),
        payload.len()
    );
}

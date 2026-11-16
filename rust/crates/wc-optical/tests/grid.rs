//! Track A: the scan order Python's `on_AutoHome` walked, and the extents it refuses.

mod support;

use support::settings;
use wc_optical::grid::{self, COLUMNS, ROWS};
use wc_optical::{Cell, OpticalError, OpticalSettings};

fn extents(tl: (i32, i32), br: (i32, i32), direction: i32) -> OpticalSettings {
    OpticalSettings { tl_x: tl.0, tl_y: tl.1, br_x: br.0, br_y: br.1, auto_scan_direction: direction, ..settings() }
}

fn cells(pairs: &[(i32, i32)]) -> Vec<Cell> {
    pairs.iter().map(|(i, j)| Cell { i: *i, j: *j }).collect()
}

#[test]
fn a_horizontal_scan_snakes_by_rows_from_the_top_left() {
    let order = grid::scan_order(&extents((-1, 1), (1, 0), 0)).unwrap();
    assert_eq!(order, cells(&[(-1, 1), (0, 1), (1, 1), (1, 0), (0, 0), (-1, 0)]));
}

#[test]
fn a_vertical_scan_snakes_by_columns_from_the_top_left() {
    let order = grid::scan_order(&extents((-1, 1), (0, -1), 1)).unwrap();
    assert_eq!(order, cells(&[(-1, 1), (-1, 0), (-1, -1), (0, -1), (0, 0), (0, 1)]));
}

#[test]
fn the_full_area_visits_every_cell_once_stepping_one_cell_at_a_time() {
    [0, 1].iter().for_each(|direction| {
        let order = grid::scan_order(&OpticalSettings { auto_scan_direction: *direction, ..settings() }).unwrap();
        assert_eq!(order.len(), COLUMNS * ROWS);
        assert_eq!((order[0], order[order.len() - 1]), (Cell { i: -15, j: 7 }, Cell { i: 15, j: -7 }));
        assert!(order.windows(2).all(|w| (w[0].i - w[1].i).abs() + (w[0].j - w[1].j).abs() == 1));
        let mut unique = order.iter().map(|c| (c.i, c.j)).collect::<Vec<_>>();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), COLUMNS * ROWS);
    });
}

#[test]
fn a_single_cell_is_its_own_scan() {
    assert_eq!(grid::scan_order(&extents((3, -2), (3, -2), 1)).unwrap(), cells(&[(3, -2)]));
}

#[test]
fn extents_off_the_grid_backwards_or_an_unknown_direction_are_refused() {
    let refused = |s: OpticalSettings| assert!(matches!(grid::scan_order(&s), Err(OpticalError::Invalid(_))), "{s:?}");
    refused(extents((-16, 7), (15, -7), 0));
    refused(extents((-15, 8), (15, -7), 0));
    refused(extents((-15, 7), (16, -7), 0));
    refused(extents((-15, 7), (15, -8), 0));
    refused(extents((2, 0), (1, 0), 0));
    refused(extents((0, -1), (0, 0), 0));
    let Err(OpticalError::Invalid(why)) = grid::scan_order(&extents((0, 0), (0, 0), 2)) else { panic!() };
    assert_eq!(why, "autoScanDirection must be 0 or 1, got 2");
}

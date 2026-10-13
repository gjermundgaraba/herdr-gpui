use super::*;

/// A unit's row, 20 high.
fn unit(unit: usize) -> PreviewRow {
    PreviewRow {
        unit: Some(unit),
        marker: false,
        height: 20.,
    }
}

/// A header or divider, 10 high.
const MARKER: PreviewRow = PreviewRow {
    unit: None,
    marker: true,
    height: 10.,
};

/// A row the drag leaves alone, such as the parent of the children dragged.
const OTHER: PreviewRow = PreviewRow {
    unit: None,
    marker: false,
    height: 20.,
};

#[test]
fn a_flat_list_shifts_as_the_shared_unit_preview_does() {
    let rows = [unit(0), unit(1), unit(2), unit(3)];
    let heights = [20.; 4];
    for dragged in 0..4 {
        for slot in 0..=4 {
            assert_eq!(
                row_shifts(&rows, dragged, slot),
                crate::reorder::preview(&heights, dragged, slot),
                "{dragged} -> {slot}"
            );
        }
    }
}

#[test]
fn a_unit_of_several_rows_moves_as_one_block() {
    // Unit 0 is a parent with a child; rows differ in height.
    let mut rows = [unit(0), unit(0), unit(1), unit(2)];
    rows[1].height = 16.;
    rows[2].height = 30.;
    assert_eq!(row_shifts(&rows, 0, 2), [30., 30., -36., 0.]);
    assert_eq!(row_shifts(&rows, 0, 3), [50., 50., -36., -36.]);
    assert_eq!(row_shifts(&rows, 2, 0), [20., 20., 20., -66.]);
}

#[test]
fn the_gaps_around_the_carried_unit_and_unknown_units_move_nothing() {
    let rows = [unit(0), MARKER, unit(1), unit(2)];
    for slot in [1, 2] {
        assert_eq!(row_shifts(&rows, 1, slot), [0.; 4], "{slot}");
    }
    // A unit with no row, as a collapsed group hides it, carries nothing.
    assert_eq!(row_shifts(&rows, 7, 0), [0.; 4]);
    assert!(row_shifts(&[], 0, 1).is_empty());
}

#[test]
fn going_down_the_headers_passed_slide_up_with_their_rows() {
    // [a0] | header B | [b1] [b2]
    let rows = [unit(0), MARKER, unit(1), unit(2)];
    // Past b1: the header and b1 close a0's place, and a0 takes theirs.
    assert_eq!(row_shifts(&rows, 0, 2), [30., -20., -20., 0.]);
    assert_eq!(row_shifts(&rows, 0, 3), [50., -20., -20., -20.]);
}

#[test]
fn going_down_stops_above_the_next_run_s_header() {
    // header A | [a0] [a1] | header B | [b2]
    let rows = [MARKER, unit(0), unit(1), MARKER, unit(2)];
    // To the end of its own run: only a1 moves, header B stays put.
    assert_eq!(row_shifts(&rows, 0, 2), [0., 20., -20., 0., 0.]);
}

#[test]
fn going_up_into_another_run_lands_above_that_run_s_header() {
    // header A | [a0] [a1] | header B | [b2]
    let rows = [MARKER, unit(0), unit(1), MARKER, unit(2)];
    // Before a1: inside run A, so header A stays and header B slides down.
    assert_eq!(row_shifts(&rows, 2, 1), [0., 0., 20., 20., -30.]);
    // Before a0: above header A, which slides down with its run.
    assert_eq!(row_shifts(&rows, 2, 0), [20., 20., 20., 20., -60.]);
}

#[test]
fn going_up_within_its_own_run_stays_below_the_header() {
    // header A | [a0] | divider | [u1] [u2]
    let rows = [MARKER, unit(0), MARKER, unit(1), unit(2)];
    assert_eq!(row_shifts(&rows, 2, 1), [0., 0., 0., 20., -20.]);
}

#[test]
fn hidden_units_are_passed_over() {
    // Unit 1 is hidden by a collapsed group, whose header still shows.
    let rows = [unit(0), MARKER, unit(2), unit(3)];
    assert_eq!(row_shifts(&rows, 0, 3), [30., -20., -20., 0.]);
    // Nothing visible lies between the unit and the gap before unit 2.
    assert_eq!(row_shifts(&rows, 0, 2), [0.; 4]);
    assert_eq!(row_shifts(&rows, 3, 1), [0., 0., 20., -20.]);
}

#[test]
fn a_child_drag_moves_only_its_siblings() {
    // header | parent | [c0] [c1] [c2] | other
    let rows = [MARKER, OTHER, unit(0), unit(1), unit(2), OTHER];
    assert_eq!(row_shifts(&rows, 0, 3), [0., 0., 40., -20., -20., 0.]);
    assert_eq!(row_shifts(&rows, 2, 0), [0., 0., 20., 20., -40., 0.]);
}

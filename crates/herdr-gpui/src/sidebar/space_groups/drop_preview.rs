//! How far each row of one host's spaces list moves to preview a workspace
//! drop. The list is more than the units a drag reorders: group headers and
//! dividers sit between them, and they have to slide with the rows around
//! them or a shifted row would paint over a header that stayed put.
//!
//! Everything the carried unit passes on its way closes the place it left,
//! and it takes the room they held. Going up into another run it lands above
//! that run's header, as the daemon then puts it in the run above or gives it
//! a run of its own; within its own run it stays below the header.

use super::SidebarRow;
use crate::{
    HerdrWindow,
    config::SpaceLayout,
    sidebar::{line_height, reorder::Plan},
};

impl HerdrWindow {
    /// The configured gap above a row that starts a repository, as upstream's
    /// spaces loop adds it; none without a custom spaces layout.
    pub(super) fn space_row_gap(&self) -> f32 {
        // Mirrors `spaces_custom` in `render.rs`'s `render_sidebar` and the
        // `row_gap * line_height` gap its spaces loop paints: change both.
        let spaces = &self.config.sidebar_layout.spaces;
        if self.config.usage.inline && *spaces != SpaceLayout::default() {
            f32::from(spaces.row_gap) * line_height(&self.config.sidebar)
        } else {
            0.
        }
    }

    /// Each of one host's `rows`, markers included, shifted to preview the
    /// drop `plan` describes into `slot`, from the heights the last frame
    /// laid out: shifting moves rows, never resizes them. `base` is the
    /// child position of the host's first row in the spaces list. Upstream
    /// sums each unit's height here instead, which leaves markers behind.
    pub(in crate::sidebar) fn preview_shifts(
        &self,
        rows: &[SidebarRow<'_>],
        plan: &Plan,
        base: usize,
        slot: usize,
    ) -> Vec<f32> {
        let row_gap = self.space_row_gap();
        let measured: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(position, row)| {
                // A unit keeps the gap above its rows wherever it lands, the
                // host's first row included.
                let gapped = match position.checked_sub(1) {
                    None => matches!(row, SidebarRow::Workspace(_)),
                    Some(previous) => row.gap_after(&rows[previous]),
                };
                PreviewRow {
                    unit: match row {
                        SidebarRow::Workspace(entry) => plan.unit_of(entry.index),
                        SidebarRow::Marker(_) => None,
                    },
                    marker: matches!(row, SidebarRow::Marker(_)),
                    height: self.sidebar_scroll[0]
                        .bounds_for_item(base + position)
                        .map_or(0., |bounds| f32::from(bounds.size.height))
                        + if gapped { row_gap } else { 0. },
                }
            })
            .collect();
        row_shifts(&measured, plan.dragged(), slot)
    }
}

/// One row of a host's spaces list, as the last frame laid it out.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PreviewRow {
    /// The drag's unit this row belongs to; `None` for a row the drag does
    /// not reorder, such as a marker.
    unit: Option<usize>,
    /// A group header or a divider.
    marker: bool,
    /// The row's height with the gap above it.
    height: f32,
}

/// Each row's shift to preview dropping unit `dragged` into `slot`, the gap
/// before unit `slot`. A gap that moves nothing moves no row, and units
/// without a row (hidden ones) are passed over.
fn row_shifts(rows: &[PreviewRow], dragged: usize, slot: usize) -> Vec<f32> {
    let mut shifts = vec![0.; rows.len()];
    let in_units = |position: usize, units: std::ops::Range<usize>| {
        rows[position]
            .unit
            .is_some_and(|unit| units.contains(&unit))
    };
    let carried = |position: usize| rows[position].unit == Some(dragged);
    let (Some(first), Some(last)) = (
        (0..rows.len()).find(|&position| carried(position)),
        (0..rows.len()).rfind(|&position| carried(position)),
    ) else {
        return shifts;
    };
    let hole: f32 = rows[first..=last].iter().map(|row| row.height).sum();
    // The rows the carried unit passes, and which way they close its place.
    let (passed, direction) = if slot > dragged + 1 {
        let Some(end) =
            (last + 1..rows.len()).rfind(|&position| in_units(position, dragged + 1..slot))
        else {
            return shifts;
        };
        (last + 1..end + 1, -1.)
    } else if slot < dragged {
        let Some(mut start) = (0..first).find(|&position| in_units(position, slot..dragged)) else {
            return shifts;
        };
        // Leaving its run, the unit lands above the header of the run it
        // enters, not between that header and the run's first row.
        let leaves_run = rows[start..first].iter().any(|row| row.marker);
        if leaves_run && start > 0 && rows[start - 1].marker {
            start -= 1;
        }
        (start..first, 1.)
    } else {
        return shifts;
    };
    let room: f32 = rows[passed.clone()].iter().map(|row| row.height).sum();
    shifts[passed].fill(direction * hole);
    shifts[first..=last].fill(-direction * room);
    shifts
}

#[cfg(test)]
mod tests;

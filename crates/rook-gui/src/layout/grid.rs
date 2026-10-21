//! UIGridLayout: equal cells in rows or columns.

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use super::{Arranged, Arrangement, alignment, constraints, resolve};

/// Measured: as many cells fit along `FillDirection` as the area holds
/// (at most `FillDirectionMaxCells` when set, at least one), lines follow
/// across; the block of cells is aligned as a whole and `StartCorner`
/// mirrors the order inside it. Every child takes the cell's size, which
/// size constraints under the grid hold.
pub(super) fn arrange(dom: &Dom, layout: InstanceId, count: usize, inner: Vec2) -> Arrangement {
    let udim2 = |name| dom.udim2_property(layout, name).unwrap_or_default();
    let cell = constraints::apply(dom, layout, resolve(udim2("CellSize"), inner), inner);
    let gap = resolve(udim2("CellPadding"), inner);
    let horizontal = dom.enum_item_property(layout, "FillDirection") != Some("Vertical");
    let major = |v: Vec2| if horizontal { v.x } else { v.y };
    let fit = ((major(inner) + major(gap)) / (major(cell) + major(gap))).floor();
    let fit = if fit.is_finite() {
        (fit as usize).max(1)
    } else {
        1
    };
    let max = dom
        .number_property(layout, "FillDirectionMaxCells")
        .unwrap_or(0.0) as usize;
    let per_line = if max > 0 { max.min(fit) } else { fit };
    let lines = count.div_ceil(per_line);
    let along = count.min(per_line);
    let (columns, rows) = if horizontal {
        (along, lines)
    } else {
        (lines, along)
    };
    let span = |n: usize, cell: f32, gap: f32| {
        if n == 0 {
            0.0
        } else {
            n as f32 * cell + (n - 1) as f32 * gap
        }
    };
    let content = Vec2::new(span(columns, cell.x, gap.x), span(rows, cell.y, gap.y));
    let origin = (inner - content) * alignment(dom, layout);
    let corner = dom
        .enum_item_property(layout, "StartCorner")
        .unwrap_or("TopLeft");
    let (flip_x, flip_y) = (corner.ends_with("Right"), corner.starts_with("Bottom"));
    let slots = (0..count)
        .map(|i| {
            let (a, b) = (i % per_line, i / per_line);
            let (column, row) = if horizontal { (a, b) } else { (b, a) };
            let mut at = Vec2::new(column as f32, row as f32) * (cell + gap);
            if flip_x {
                at.x = content.x - at.x - cell.x;
            }
            if flip_y {
                at.y = content.y - at.y - cell.y;
            }
            origin + at
        })
        .collect();
    Arrangement {
        slots,
        arranged: Arranged {
            content,
            cells: Some((cell, Vec2::new(columns as f32, rows as f32))),
        },
        cell: Some(cell),
    }
}

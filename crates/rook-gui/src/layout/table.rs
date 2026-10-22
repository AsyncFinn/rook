//! UITableLayout: its children are rows (or columns) and theirs the cells
//! (docs/behavior/gui/modern.md#uitablelayout).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use super::{Arranged, Arrangement, Kid, alignment, gui_children, list, resolve, visible};

/// A cell the table places in its row: its top left in the row and size.
pub(crate) struct Cell {
    pub(crate) id: InstanceId,
    pub(crate) at: Vec2,
    pub(crate) size: Vec2,
}

/// Measured: each row's cells go across in order; a column is as wide as
/// its widest cell and a row as tall as its tallest, and every cell takes
/// its column's width and its row's height; `Padding`'s X is between
/// columns and Y between rows; `FillEmptySpaceColumns` and
/// `FillEmptySpaceRows` stretch the columns or rows in proportion to fill
/// the area; the table is aligned as a whole; a row is as wide as the
/// table. `MajorAxis` ColumnMajor, or `FillDirection` Horizontal, makes
/// the children columns instead. Cells are sized by `cell_size` (against
/// the table's area, measured).
pub(super) fn arrange(
    dom: &Dom,
    layout: InstanceId,
    rows: &[Kid],
    inner: Vec2,
    mut cell_size: impl FnMut(InstanceId) -> Vec2,
) -> (Arrangement, Vec<Cell>) {
    let columns_major = dom.enum_item_property(layout, "MajorAxis") == Some("ColumnMajor");
    let horizontal = dom.enum_item_property(layout, "FillDirection") == Some("Horizontal");
    // Children as rows: row y, cell x. Otherwise swapped.
    let flip = columns_major != horizontal;
    let pick = |v: Vec2| if flip { Vec2::new(v.y, v.x) } else { v };
    let cells: Vec<Vec<(InstanceId, Vec2)>> = (rows.iter())
        .map(|r| {
            let kids: Vec<Kid> = (gui_children(dom, r.id).into_iter())
                .filter(|&c| visible(dom, c))
                .map(|c| Kid {
                    id: c,
                    size: pick(cell_size(c)),
                })
                .collect();
            // Inference: cells sort as rows do.
            (list::sorted(dom, layout, &kids).into_iter())
                .map(|i| (kids[i].id, kids[i].size))
                .collect()
        })
        .collect();
    let gap = pick(resolve(
        dom.udim2_property(layout, "Padding").unwrap_or_default(),
        inner,
    ));
    let area = pick(inner);
    let count = cells.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0.0f32; count];
    for row in &cells {
        for (n, (_, size)) in row.iter().enumerate() {
            widths[n] = widths[n].max(size.x);
        }
    }
    let mut heights: Vec<f32> = (cells.iter())
        .map(|row| row.iter().map(|(_, s)| s.y).fold(0.0, f32::max))
        .collect();
    let span = |sizes: &[f32], gap: f32| {
        sizes.iter().sum::<f32>() + gap * sizes.len().saturating_sub(1) as f32
    };
    let fill = |sizes: &mut [f32], gap: f32, room: f32| {
        let total: f32 = sizes.iter().sum();
        let spare = room - span(sizes, gap);
        if spare > 0.0 && total > 0.0 {
            for s in sizes.iter_mut() {
                *s += spare * *s / total;
            }
        }
    };
    let (fill_columns, fill_rows) = (
        dom.bool_property(layout, "FillEmptySpaceColumns") == Some(true),
        dom.bool_property(layout, "FillEmptySpaceRows") == Some(true),
    );
    if if flip { fill_rows } else { fill_columns } {
        fill(&mut widths, gap.x, area.x);
    }
    if if flip { fill_columns } else { fill_rows } {
        fill(&mut heights, gap.y, area.y);
    }
    let content = Vec2::new(span(&widths, gap.x), span(&heights, gap.y));
    let origin = (inner - pick(content)) * alignment(dom, layout);
    let mut placed = Vec::new();
    let mut slots = Vec::with_capacity(rows.len());
    let mut sizes = Vec::with_capacity(rows.len());
    let mut y = 0.0;
    for (row, &height) in cells.iter().zip(&heights) {
        slots.push(origin + pick(Vec2::new(0.0, y)));
        sizes.push(pick(Vec2::new(content.x, height)));
        let mut x = 0.0;
        for ((id, _), &width) in row.iter().zip(&widths) {
            placed.push(Cell {
                id: *id,
                at: pick(Vec2::new(x, 0.0)),
                size: pick(Vec2::new(width, height)),
            });
            x += width + gap.x;
        }
        y += height + gap.y;
    }
    let arrangement = Arrangement {
        slots,
        sizes,
        arranged: Arranged {
            content: pick(content),
            cells: None,
        },
    };
    (arrangement, placed)
}

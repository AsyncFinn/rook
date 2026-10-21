//! Cells: the legacy name for voxels. Old scripts address terrain by cell
//! (`SetCell`, `GetCell`, `WorldToCell`, ...), and modern Roblox still
//! honors them on smooth voxels (observed in Studio 0.742): a cell is a
//! voxel, `SetCell` fills it with the smooth material standing in for the
//! legacy one, and `GetCell` names the legacy material for the voxel's.

use glam::{IVec3, Vec3};

use crate::TerrainMaterial;
use crate::grid::{Grid, VOXEL_STUDS};

/// The cell holding `p` (`WorldToCell`).
pub fn world_to_cell(p: Vec3) -> IVec3 {
    (p / VOXEL_STUDS).floor().as_ivec3()
}

/// A cell's center (`CellCenterToWorld`); fractional cells round half away
/// from zero (observed: 1.5 is cell 2, -1.5 cell -2, 0.4 cell 0).
pub fn cell_center(cell: Vec3) -> Vec3 {
    (cell.round() + 0.5) * VOXEL_STUDS
}

/// A cell's lowest corner (`CellCornerToWorld`).
pub fn cell_corner(cell: Vec3) -> Vec3 {
    cell.round() * VOXEL_STUDS
}

/// `WorldToCellPreferSolid` and `WorldToCellPreferEmpty`: the cell holding
/// `p`, but for a point on a cell's face, the cell on the side that is
/// solid (or empty) when the other isn't (observed: on the face between a
/// Rock voxel and an air voxel, each picks its side). Solid is a voxel of
/// a material other than Air and Water (Inference).
pub fn world_to_cell_prefer(grid: &Grid, p: Vec3, solid: bool) -> IVec3 {
    let cell = world_to_cell(p);
    let wants = |c: IVec3| grid.voxel(c).is_solid() == solid;
    if wants(cell) {
        return cell;
    }
    let scaled = p / VOXEL_STUDS;
    let on_face = scaled.cmpeq(scaled.floor());
    // The cells sharing the point: one step down on each axis it sits on
    // a face of, in every combination.
    (1..8)
        .map(|bits: i32| IVec3::new(bits & 1, (bits >> 1) & 1, (bits >> 2) & 1))
        .filter(|step| (0..3).all(|axis| step[axis] == 0 || on_face.test(axis)))
        .map(|step| cell - step)
        .find(|c| wants(*c))
        .unwrap_or(cell)
}

/// The smooth material `SetCell` writes for each `Enum.CellMaterial`, by
/// name (observed: Granite is Slate, the metals and CinderBlock Concrete,
/// Gravel Asphalt, MossyStone Pavement, RedPlastic Sandstone, BluePlastic
/// Ice).
pub fn from_cell_material(name: &str) -> Option<TerrainMaterial> {
    use TerrainMaterial::*;
    Some(match name {
        "Empty" => Air,
        "Grass" => Grass,
        "Sand" => Sand,
        "Brick" => Brick,
        "Granite" => Slate,
        "Asphalt" | "Gravel" => Asphalt,
        "Iron" | "Aluminum" | "Gold" | "CinderBlock" | "Cement" => Concrete,
        "WoodPlank" | "WoodLog" => WoodPlanks,
        "MossyStone" => Pavement,
        "RedPlastic" => Sandstone,
        "BluePlastic" => Ice,
        "Water" => Water,
        _ => return None,
    })
}

/// The `Enum.CellMaterial` `GetCell` reports for a voxel's material, by
/// name: the one `SetCell` maps to it, and Grass for materials no legacy
/// one maps to (observed: Rock, Snow and Basalt are Grass).
pub fn to_cell_material(material: TerrainMaterial) -> &'static str {
    use TerrainMaterial::*;
    match material {
        Air => "Empty",
        Water => "Water",
        Sand => "Sand",
        Brick => "Brick",
        Slate => "Granite",
        Asphalt => "Asphalt",
        Concrete => "Cement",
        WoodPlanks => "WoodPlank",
        Pavement => "MossyStone",
        Sandstone => "RedPlastic",
        Ice => "BluePlastic",
        _ => "Grass",
    }
}

/// The `Enum.CellBlock` `GetCell` reports for how full a voxel is, by
/// name (observed, every few bytes from 0 to 255: the wedges in bands
/// around the occupancy `SetCell` gives each, and Solid for nearly full
/// and nearly empty voxels).
pub fn to_cell_block(occupancy: u8) -> &'static str {
    match occupancy {
        0..=42 | 213..=255 => "Solid",
        43..=84 => "CornerWedge",
        85..=127 => "VerticalWedge",
        128..=212 => "InverseCornerWedge",
    }
}

/// How full `SetCell` makes a voxel for each `Enum.CellBlock`, by name
/// (observed; the orientation changes nothing).
pub fn cell_block_occupancy(name: &str) -> u8 {
    match name {
        "VerticalWedge" | "HorizontalWedge" => 127,
        "CornerWedge" => 85,
        "InverseCornerWedge" => 170,
        _ => 255,
    }
}

//! The Terrain API: fills, voxel reads and writes, region copies, legacy
//! cells and material colors, on the voxels `rook_terrain` edits
//! (`docs/behavior/terrain/api.md`). Workspace's Terrain is
//! edited in the physics world's copy of its voxels, which goes back into
//! `SmoothGrid` once a frame; any other Terrain is decoded and written
//! back on each call.

use glam::{Affine3A, IVec3, Vec3};
use rbx_types::{Region3int16, Vector3int16};
use rook_terrain::grid::cells;
use rook_terrain::grid::edit::Shape;
use rook_terrain::grid::edit::{Bounds, REGION_MAX_VOXELS, region_voxels};
use rook_terrain::{Grid, TerrainMaterial, Voxel};
use rook_types::{Material, MaterialSet, TerrainMaterials};

use super::*;
use crate::types::{LColor3, LRegion3, LRegion3int16, LVector3int16, enums};

mod colors;
mod fills;
mod legacy;
mod regions;
mod voxels;

pub(super) use colors::{get_material_color, set_material_color};
pub(super) use fills::{
    fill_ball, fill_block, fill_cylinder, fill_region, fill_wedge, replace_material,
};
pub(super) use legacy::{
    autowedge_cell, autowedge_cells, cell_center_to_world, cell_corner_to_world, get_cell,
    get_water_cell, set_cell, set_cells_region, set_water_cell, world_to_cell,
    world_to_cell_prefer_empty, world_to_cell_prefer_solid,
};
pub(crate) use regions::region_voxels_of;
pub(super) use regions::{copy_region, paste_region};
pub(super) use voxels::{count_cells, read_voxels, terrain_clear, write_voxels};

fn enum_id(name: &str) -> rook_reflection::EnumId {
    database()
        .enum_by_name(name)
        .expect("enum in the database")
        .id
}

/// An enum argument's item name.
fn enum_name(lua: &Lua, a: &Args, i: usize, enum_name: &str) -> ApiResult<&'static str> {
    let id = enum_id(enum_name);
    let value = enums::coerce(lua, a.get(i), id).map_err(ScriptError)?;
    Ok(database()
        .enum_info(id)
        .items
        .iter()
        .find(|(_, v)| *v == value)
        .map_or("", |(n, _)| n))
}

/// The item of `enum_name` called `name`.
fn push_named(lua: &Lua, enum_name: &str, name: &str) -> ApiResult<LuaValue> {
    let id = enum_id(enum_name);
    let value = rook_reflection::enum_item_value(id, name).unwrap_or(0);
    Ok(enums::push_item(lua, id, value)?)
}

/// A `Material` argument.
fn material_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<Material> {
    let value = enums::coerce(lua, a.get(i), enum_id("Material")).map_err(ScriptError)?;
    Ok(Material::of_part(Some(value)))
}

/// The voxel material a fill or write of `m` makes: materials terrain
/// doesn't have fill as Grass (observed for Plastic, Wood, Neon, Glass,
/// ForceField and the rest).
fn voxel_material(m: Material) -> TerrainMaterial {
    TerrainMaterial::from_material(m).unwrap_or(TerrainMaterial::Grass)
}

fn pose(cf: &LCFrame) -> Affine3A {
    Affine3A::from_mat3_translation(cf.0.rotation, cf.0.position)
}

fn glam_vec(v: rook_types::Vector3) -> Vec3 {
    Vec3::new(v.x(), v.y(), v.z())
}

/// Read the voxels of `terrain`.
fn read<R>(lua: &Lua, terrain: InstanceId, f: impl FnOnce(&Grid) -> R) -> R {
    with_world(lua, |w| {
        if let Some(grid) = w.physics.terrain_grid(&mut w.dom, terrain) {
            return f(grid);
        }
        f(&decoded(&w.dom, terrain))
    })
}

/// Edit the voxels of `terrain`; `f` returns the voxels it may have
/// changed.
fn edit<R>(lua: &Lua, terrain: InstanceId, f: impl FnOnce(&mut Grid) -> (R, Option<Bounds>)) -> R {
    with_world(lua, |w| {
        if w.physics.terrain_grid(&mut w.dom, terrain).is_some() {
            return w
                .physics
                .edit_terrain(&mut w.dom, terrain, f)
                .expect("Workspace's Terrain");
        }
        let mut grid = decoded(&w.dom, terrain);
        let (out, changed) = f(&mut grid);
        if changed.is_some() {
            let bytes = Value::BinaryString(grid.encode().into());
            rook_dom::ignore_stale(w.dom.set_property_by_name(terrain, "SmoothGrid", bytes));
        }
        out
    })
}

/// An instance's `SmoothGrid` voxels; none when it has none or they don't
/// decode.
fn decoded(dom: &rook_dom::Dom, id: InstanceId) -> Grid {
    match dom.binary_property(id, "SmoothGrid") {
        Some(bytes) => Grid::decode(&bytes).unwrap_or_default(),
        None => Grid::new(),
    }
}

/// A region argument and its resolution, which must be 4 (observed:
/// "Resolution has to be 4"): the first voxel and the voxel count.
fn region_arg(a: &Args, i: usize) -> ApiResult<(IVec3, IVec3)> {
    let region = a.userdata::<LRegion3>(i, "Region3")?.0;
    if a.number(i + 1)? != 4.0 {
        return Err(ScriptError("Resolution has to be 4".into()));
    }
    let v = |v: rbx_types::Vector3| Vec3::new(v.x, v.y, v.z);
    Ok(region_voxels(v(region.min), v(region.max)))
}

/// A region read or write's size, within the limit.
fn checked_size(size: IVec3) -> ApiResult<IVec3> {
    let count = i64::from(size.x) * i64::from(size.y) * i64::from(size.z);
    if count > REGION_MAX_VOXELS {
        return Err(ScriptError("Region is too large".into()));
    }
    Ok(size)
}

fn region16_arg(a: &Args, i: usize) -> ApiResult<Region3int16> {
    Ok(a.userdata::<LRegion3int16>(i, "Region3int16")?.0)
}

fn v16(v: Vector3int16) -> IVec3 {
    IVec3::new(v.x.into(), v.y.into(), v.z.into())
}

/// A Region3int16's cells, both corners included.
fn region16_bounds(r: Region3int16) -> Option<Bounds> {
    let (min, max) = (v16(r.min), v16(r.max));
    min.cmple(max).all().then_some(Bounds { min, max })
}

//! The legacy cell API (`GetCell`, `SetCell`, `WorldToCell` and the
//! rest), on the same voxels.

use super::*;

fn cell_args(a: &Args, i: usize) -> ApiResult<IVec3> {
    Ok(IVec3::new(
        a.number(i)? as i32,
        a.number(i + 1)? as i32,
        a.number(i + 2)? as i32,
    ))
}

fn vec_out(lua: &Lua, v: Vec3) -> ApiResult<Outcome> {
    Outcome::ret(lua, vector3_to_lua(rook_types::Vector3::new(v.x, v.y, v.z)))
}

pub(in crate::api::methods) fn world_to_cell(
    lua: &Lua,
    _: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    vec_out(lua, cells::world_to_cell(glam_vec(a.vector3(0)?)).as_vec3())
}

fn world_to_cell_prefer(lua: &Lua, this: InstanceId, a: &Args, solid: bool) -> ApiResult<Outcome> {
    let p = glam_vec(a.vector3(0)?);
    let cell = read(lua, this, |g| cells::world_to_cell_prefer(g, p, solid));
    vec_out(lua, cell.as_vec3())
}

pub(in crate::api::methods) fn world_to_cell_prefer_solid(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    world_to_cell_prefer(lua, this, a, true)
}

pub(in crate::api::methods) fn world_to_cell_prefer_empty(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    world_to_cell_prefer(lua, this, a, false)
}

fn cell_coords(a: &Args) -> ApiResult<Vec3> {
    Ok(Vec3::new(
        a.number(0)? as f32,
        a.number(1)? as f32,
        a.number(2)? as f32,
    ))
}

pub(in crate::api::methods) fn cell_center_to_world(
    lua: &Lua,
    _: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    vec_out(lua, cells::cell_center(cell_coords(a)?))
}

pub(in crate::api::methods) fn cell_corner_to_world(
    lua: &Lua,
    _: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    vec_out(lua, cells::cell_corner(cell_coords(a)?))
}

/// The cell's legacy material, a block shape for how full it is, and
/// `NegZ` whatever was set (observed).
pub(in crate::api::methods) fn get_cell(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let cell = cell_args(a, 0)?;
    let voxel = read(lua, this, |g| g.voxel(cell));
    Outcome::ret(
        lua,
        (
            push_named(lua, "CellMaterial", cells::to_cell_material(voxel.material))?,
            push_named(lua, "CellBlock", cells::to_cell_block(voxel.occupancy))?,
            push_named(lua, "CellOrientation", "NegZ")?,
        ),
    )
}

/// Cells to set from a material and block shape; Empty clears them.
fn set_cells(lua: &Lua, this: InstanceId, bounds: Bounds, a: &Args, i: usize) -> ApiResult<()> {
    let material = cells::from_cell_material(enum_name(lua, a, i, "CellMaterial")?)
        .unwrap_or(TerrainMaterial::Air);
    let occupancy = cells::cell_block_occupancy(enum_name(lua, a, i + 1, "CellBlock")?);
    enum_name(lua, a, i + 2, "CellOrientation")?;
    let voxel = Voxel::new(material, occupancy);
    let voxels = vec![voxel; bounds_len(bounds)];
    edit(lua, this, |g| {
        (g.write_block(bounds, &voxels), Some(bounds))
    });
    Ok(())
}

fn bounds_len(b: Bounds) -> usize {
    let s = b.max - b.min + IVec3::ONE;
    (s.x * s.y * s.z) as usize
}

pub(in crate::api::methods) fn set_cell(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let cell = cell_args(a, 0)?;
    set_cells(
        lua,
        this,
        Bounds {
            min: cell,
            max: cell,
        },
        a,
        3,
    )?;
    Outcome::ret(lua, ())
}

pub(in crate::api::methods) fn set_cells_region(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    if let Some(bounds) = region16_bounds(region16_arg(a, 0)?) {
        set_cells(lua, this, bounds, a, 1)?;
    }
    Outcome::ret(lua, ())
}

/// Smooth terrain has no wedges to fit (observed: returns false).
pub(in crate::api::methods) fn autowedge_cell(
    lua: &Lua,
    _: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    cell_args(a, 0)?;
    Outcome::ret(lua, false)
}

pub(in crate::api::methods) fn autowedge_cells(
    lua: &Lua,
    _: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    region16_arg(a, 0)?;
    Outcome::ret(lua, ())
}

/// Fills the cell with water, over whatever was there (observed: over
/// full Grass, the voxel reads back as full Water).
pub(in crate::api::methods) fn set_water_cell(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let cell = cell_args(a, 0)?;
    enum_name(lua, a, 3, "WaterForce")?;
    enum_name(lua, a, 4, "WaterDirection")?;
    let bounds = Bounds {
        min: cell,
        max: cell,
    };
    let water = [Voxel::new(TerrainMaterial::Water, 255)];
    edit(lua, this, |g| (g.write_block(bounds, &water), Some(bounds)));
    Outcome::ret(lua, ())
}

/// Whether the cell is water, then `None` and `NegX` (observed).
pub(in crate::api::methods) fn get_water_cell(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let cell = cell_args(a, 0)?;
    let water = read(lua, this, |g| {
        g.voxel(cell).material == TerrainMaterial::Water
    });
    Outcome::ret(
        lua,
        (
            water,
            push_named(lua, "WaterForce", "None")?,
            push_named(lua, "WaterDirection", "NegX")?,
        ),
    )
}

// Regions

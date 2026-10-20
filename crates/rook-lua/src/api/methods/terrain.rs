//! The Terrain API: fills, voxel reads and writes, region copies, legacy
//! cells and material colours, on the voxels `rook_terrain` edits
//! (`docs/behavior/terrain.md`, "The Terrain API"). Workspace's Terrain is
//! edited in the physics world's copy of its voxels, which goes back into
//! `SmoothGrid` once a frame; any other Terrain is decoded and written
//! back on each call.

use glam::{Affine3A, IVec3, Vec3};
use rbx_types::{Region3int16, Vector3int16};
use rook_terrain::grid::cells;
use rook_terrain::grid::edit::shape::Shape;
use rook_terrain::grid::edit::{Bounds, REGION_MAX_VOXELS, region_voxels};
use rook_terrain::{Grid, TerrainMaterial, Voxel};
use rook_types::{Material, MaterialSet, TerrainMaterials};

use super::*;
use crate::types::{LColor3, LRegion3, LRegion3int16, LVector3int16, enums};

mod legacy;

pub(super) use legacy::{
    autowedge_cell, autowedge_cells, cell_center_to_world, cell_corner_to_world, get_cell,
    get_water_cell, set_cell, set_cells_region, set_water_cell, world_to_cell,
    world_to_cell_prefer_empty, world_to_cell_prefer_solid,
};

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

fn empty_extents() -> ScriptError {
    ScriptError("Extents cannot be empty".into())
}

fn fill(lua: &Lua, this: InstanceId, shape: Shape, m: Material) -> ApiResult<Outcome> {
    if shape.is_empty() {
        return Err(empty_extents());
    }
    let material = voxel_material(m);
    edit(lua, this, |g| ((), g.fill(&shape, material)));
    Outcome::ret(lua, ())
}

pub(super) fn fill_block(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pose = pose(&a.userdata::<LCFrame>(0, "CFrame")?);
    let size = glam_vec(a.vector3(1)?);
    fill(
        lua,
        this,
        Shape::Block { pose, size },
        material_arg(lua, a, 2)?,
    )
}

pub(super) fn fill_ball(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let center = glam_vec(a.vector3(0)?);
    let radius = a.number(1)? as f32;
    fill(
        lua,
        this,
        Shape::Ball { center, radius },
        material_arg(lua, a, 2)?,
    )
}

pub(super) fn fill_cylinder(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pose = pose(&a.userdata::<LCFrame>(0, "CFrame")?);
    let (height, radius) = (a.number(1)? as f32, a.number(2)? as f32);
    let shape = Shape::Cylinder {
        pose,
        height,
        radius,
    };
    fill(lua, this, shape, material_arg(lua, a, 3)?)
}

pub(super) fn fill_wedge(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pose = pose(&a.userdata::<LCFrame>(0, "CFrame")?);
    let size = glam_vec(a.vector3(1)?);
    fill(
        lua,
        this,
        Shape::Wedge { pose, size },
        material_arg(lua, a, 2)?,
    )
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

pub(super) fn fill_region(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (min, size) = region_arg(a, 0)?;
    let material = voxel_material(material_arg(lua, a, 2)?);
    if let Some(bounds) = Bounds::of_block(min, size) {
        edit(lua, this, |g| {
            (g.fill_block(bounds, material), Some(bounds))
        });
    }
    Outcome::ret(lua, ())
}

pub(super) fn replace_material(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (min, size) = region_arg(a, 0)?;
    let from = voxel_material(material_arg(lua, a, 2)?);
    let to = voxel_material(material_arg(lua, a, 3)?);
    if let Some(bounds) = Bounds::of_block(min, size) {
        edit(lua, this, |g| {
            ((), g.replace(bounds, from, to).then_some(bounds))
        });
    }
    Outcome::ret(lua, ())
}

/// `ReadVoxels`: materials and occupancies as `[x][y][z]` arrays with a
/// `Size` field; an air voxel's occupancy is 0, any other's
/// `(byte + 1) / 256` (see `rook_terrain::Voxel`).
pub(super) fn read_voxels(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (min, size) = region_arg(a, 0)?;
    let size = checked_size(size)?;
    let mut voxels = Vec::new();
    if size.cmpgt(IVec3::ZERO).all() {
        read(lua, this, |g| g.read_block(min, size, &mut voxels));
    }
    let material_enum = enum_id("Material");
    let materials = lua.create_table()?;
    let occupancies = lua.create_table()?;
    for x in 0..size.x {
        let (mx, ox) = (lua.create_table()?, lua.create_table()?);
        for y in 0..size.y {
            let (my, oy) = (lua.create_table()?, lua.create_table()?);
            for z in 0..size.z {
                let v = voxels[((z * size.y + y) * size.x + x) as usize];
                let item = enums::push_item(lua, material_enum, v.material.material().value())?;
                my.raw_set(z + 1, item)?;
                let fill = if v.material == TerrainMaterial::Air {
                    0.0
                } else {
                    v.fill()
                };
                oy.raw_set(z + 1, fill)?;
            }
            mx.raw_set(y + 1, my)?;
            ox.raw_set(y + 1, oy)?;
        }
        materials.raw_set(x + 1, mx)?;
        occupancies.raw_set(x + 1, ox)?;
    }
    let size_value = crate::ctx::convert::vector3_to_lua(rook_types::Vector3::new(
        size.x as f32,
        size.y as f32,
        size.z as f32,
    ));
    materials.raw_set("Size", size_value.clone())?;
    occupancies.raw_set("Size", size_value)?;
    Outcome::ret(lua, (materials, occupancies))
}

/// `t[x][y][z]` of a `size` array argument, with Studio's errors.
fn voxel_array<T>(
    a: &Args,
    i: usize,
    name: &str,
    size: IVec3,
    mut each: impl FnMut(&LuaValue) -> Option<T>,
    expected: &str,
) -> ApiResult<Vec<T>> {
    let shape_error = || {
        ScriptError(format!(
            "Bad argument {name} to 'WriteVoxels' ({}x{}x{} array expected)",
            size.x, size.y, size.z
        ))
    };
    let LuaValue::Table(t) = a.get(i) else {
        return Err(shape_error());
    };
    let mut out = Vec::with_capacity((size.x * size.y * size.z).max(0) as usize);
    let mut rows = vec![None; (size.x * size.y).max(0) as usize];
    if t.raw_len() != size.x as usize {
        return Err(shape_error());
    }
    for x in 0..size.x {
        let LuaValue::Table(tx) = t.raw_get::<LuaValue>(x + 1)? else {
            return Err(shape_error());
        };
        if tx.raw_len() != size.y as usize {
            return Err(shape_error());
        }
        for y in 0..size.y {
            let LuaValue::Table(ty) = tx.raw_get::<LuaValue>(y + 1)? else {
                return Err(shape_error());
            };
            if ty.raw_len() != size.z as usize {
                return Err(shape_error());
            }
            rows[(y * size.x + x) as usize] = Some(ty);
        }
    }
    // Out in the grid's order: X fastest, then Y, then Z.
    for z in 0..size.z {
        for y in 0..size.y {
            for x in 0..size.x {
                let row = rows[(y * size.x + x) as usize].as_ref().expect("checked");
                let v: LuaValue = row.raw_get(z + 1)?;
                let value = each(&v).ok_or_else(|| {
                    ScriptError(format!(
                        "Bad argument {name}[{}][{}][{}] to 'WriteVoxels' ({expected} expected, got {})",
                        x + 1,
                        y + 1,
                        z + 1,
                        crate::ctx::args::type_name(&v)
                    ))
                })?;
                out.push(value);
            }
        }
    }
    Ok(out)
}

/// `WriteVoxels`: occupancies are clamped to 0..1; 0, or Air, leaves air
/// (observed).
pub(super) fn write_voxels(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (min, size) = region_arg(a, 0)?;
    let size = checked_size(size)?;
    let material_enum = enum_id("Material");
    let materials = voxel_array(
        a,
        2,
        "materials",
        size,
        |v| match v {
            LuaValue::UserData(u) => u
                .borrow::<enums::LEnumItem>()
                .ok()
                .filter(|i| i.enum_id == material_enum)
                .map(|i| voxel_material(Material::of_part(Some(i.value)))),
            _ => None,
        },
        "Enum.Material",
    )?;
    let occupancies = voxel_array(
        a,
        3,
        "occupancies",
        size,
        crate::ctx::args::number_of,
        "number",
    )?;
    let Some(bounds) = Bounds::of_block(min, size) else {
        return Outcome::ret(lua, ());
    };
    let voxels: Vec<Voxel> = materials
        .into_iter()
        .zip(occupancies)
        .map(|(m, o)| {
            let byte = (o.clamp(0.0, 1.0) * 256.0).round() - 1.0;
            if byte < 0.0 {
                Voxel::AIR
            } else {
                Voxel::new(m, byte as u8)
            }
        })
        .collect();
    edit(lua, this, |g| {
        (g.write_block(bounds, &voxels), Some(bounds))
    });
    Outcome::ret(lua, ())
}

pub(super) fn terrain_clear(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    edit(lua, this, |g| {
        let chunks: Vec<IVec3> = g.chunks().collect();
        let bounds = chunks.iter().fold(None, |b: Option<Bounds>, c| {
            let lo = *c * rook_terrain::CHUNK_VOXELS;
            let hi = lo + IVec3::splat(rook_terrain::CHUNK_VOXELS - 1);
            Some(match b {
                None => Bounds { min: lo, max: hi },
                Some(b) => Bounds {
                    min: b.min.min(lo),
                    max: b.max.max(hi),
                },
            })
        });
        *g = Grid::new();
        ((), bounds)
    });
    Outcome::ret(lua, ())
}

/// The number of voxels that aren't air. Studio counts differently for
/// small terrain (observed: one voxel, or two side by side, count 2; a
/// full chunk counts its 32,768).
pub(super) fn count_cells(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let n = read(lua, this, Grid::count_cells);
    Outcome::ret(lua, n as f64)
}

// Cells

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

/// A TerrainRegion holding the region's voxels, in its `SmoothGrid` from
/// voxel 0, with its size in `ExtentsMax` (observed: the corners are both
/// included; an inverted region errors).
pub(super) fn copy_region(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let bounds = region16_bounds(region16_arg(a, 0)?)
        .ok_or_else(|| ScriptError("The region can not be empty or inverted.".into()))?;
    let copy = read(lua, this, |g| g.copy(bounds));
    let size = bounds.max - bounds.min + IVec3::ONE;
    let id = with_world(lua, |w| -> Result<InstanceId, DomError> {
        let id = w.dom.create_by_name("TerrainRegion")?;
        let v =
            |v: IVec3| Value::Vector3int16(Vector3int16::new(v.x as i16, v.y as i16, v.z as i16));
        w.dom
            .set_property_by_name(id, "ExtentsMin", v(IVec3::ZERO))?;
        w.dom.set_property_by_name(id, "ExtentsMax", v(size))?;
        w.dom
            .set_property_by_name(id, "SmoothGrid", Value::BinaryString(copy.encode().into()))?;
        Ok(id)
    })?;
    ret_instance(lua, Some(id))
}

/// A TerrainRegion's voxels and size.
#[expect(
    clippy::disallowed_methods,
    reason = "Vector3int16 has no typed reader; this is its one read"
)]
pub(crate) fn region_voxels_of(dom: &rook_dom::Dom, region: InstanceId) -> (Grid, IVec3) {
    let extent = |name| match dom.property_by_name(region, name) {
        Some(Value::Vector3int16(v)) => v16(v),
        _ => IVec3::ZERO,
    };
    (
        decoded(dom, region),
        extent("ExtentsMax") - extent("ExtentsMin"),
    )
}

pub(super) fn paste_region(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let region = a.instance(0)?;
    let corner = v16(a.userdata::<LVector3int16>(1, "Vector3int16")?.0);
    let paste_empty = a.truthy(2);
    let (copy, size) = with_world(lua, |w| region_voxels_of(&w.dom, region));
    if let Some(source) = Bounds::of_block(corner, size) {
        edit(lua, this, |g| {
            (g.paste(&copy, size, corner, paste_empty), Some(source))
        });
    }
    Outcome::ret(lua, ())
}

// Material colours

/// The `MaterialColors` key of a material argument, with Studio's errors.
fn color_key(lua: &Lua, a: &Args) -> ApiResult<(TerrainMaterial, TerrainMaterials)> {
    let m = material_arg(lua, a, 0)?;
    let t = TerrainMaterial::from_material(m)
        .ok_or_else(|| ScriptError("Invalid terrain material".into()))?;
    let key = t
        .name()
        .parse()
        .map_err(|_| ScriptError("Unsupported terrain material".into()))?;
    Ok((t, key))
}

fn material_colors(dom: &rook_dom::Dom, id: InstanceId) -> Option<rook_types::MaterialColors> {
    dom.material_colors_property(id, "MaterialColors")
}

pub(super) fn get_material_color(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (t, _) = color_key(lua, a)?;
    let colors = with_world(lua, |w| material_colors(&w.dom, this));
    let c = t
        .color(colors.as_ref(), MaterialSet::Materials2022)
        .unwrap_or(rook_types::Color3uint8::new(0, 0, 0));
    Outcome::ret(lua, LColor3(rook_types::Color3::from(c)))
}

pub(super) fn set_material_color(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (_, key) = color_key(lua, a)?;
    let color = a.userdata::<LColor3>(1, "Color3")?.0;
    mutate(lua, |w| {
        let mut colors = material_colors(&w.dom, this).unwrap_or_default();
        colors.set_color(key, color.into());
        w.dom
            .set_property_by_name(this, "MaterialColors", Value::MaterialColors(colors))
    })?;
    Outcome::ret(lua, ())
}

//! Voxel reads and writes: `ReadVoxels`, `WriteVoxels`, `Clear` and
//! `CountCells`.

use super::*;

/// `ReadVoxels`: materials and occupancies as `[x][y][z]` arrays with a
/// `Size` field; an air voxel's occupancy is 0, any other's
/// `(byte + 1) / 256` (see `rook_terrain::Voxel`).
pub(in crate::api::methods) fn read_voxels(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
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
pub(in crate::api::methods) fn write_voxels(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
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

pub(in crate::api::methods) fn terrain_clear(
    lua: &Lua,
    this: InstanceId,
    _: &Args,
) -> ApiResult<Outcome> {
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
pub(in crate::api::methods) fn count_cells(
    lua: &Lua,
    this: InstanceId,
    _: &Args,
) -> ApiResult<Outcome> {
    let n = read(lua, this, Grid::count_cells);
    Outcome::ret(lua, n as f64)
}

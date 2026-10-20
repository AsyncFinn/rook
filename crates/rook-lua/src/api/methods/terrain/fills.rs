//! The fills: `FillBlock`, `FillBall`, `FillCylinder`, `FillWedge`,
//! `FillRegion` and `ReplaceMaterial`.

use super::*;

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

pub(in crate::api::methods) fn fill_block(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let pose = pose(&a.userdata::<LCFrame>(0, "CFrame")?);
    let size = glam_vec(a.vector3(1)?);
    fill(
        lua,
        this,
        Shape::Block { pose, size },
        material_arg(lua, a, 2)?,
    )
}

pub(in crate::api::methods) fn fill_ball(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let center = glam_vec(a.vector3(0)?);
    let radius = a.number(1)? as f32;
    fill(
        lua,
        this,
        Shape::Ball { center, radius },
        material_arg(lua, a, 2)?,
    )
}

pub(in crate::api::methods) fn fill_cylinder(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let pose = pose(&a.userdata::<LCFrame>(0, "CFrame")?);
    let (height, radius) = (a.number(1)? as f32, a.number(2)? as f32);
    let shape = Shape::Cylinder {
        pose,
        height,
        radius,
    };
    fill(lua, this, shape, material_arg(lua, a, 3)?)
}

pub(in crate::api::methods) fn fill_wedge(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let pose = pose(&a.userdata::<LCFrame>(0, "CFrame")?);
    let size = glam_vec(a.vector3(1)?);
    fill(
        lua,
        this,
        Shape::Wedge { pose, size },
        material_arg(lua, a, 2)?,
    )
}

pub(in crate::api::methods) fn fill_region(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let (min, size) = region_arg(a, 0)?;
    let material = voxel_material(material_arg(lua, a, 2)?);
    if let Some(bounds) = Bounds::of_block(min, size) {
        edit(lua, this, |g| {
            (g.fill_block(bounds, material), Some(bounds))
        });
    }
    Outcome::ret(lua, ())
}

pub(in crate::api::methods) fn replace_material(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
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

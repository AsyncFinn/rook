//! Region copies: `CopyRegion` into a TerrainRegion and `PasteRegion`
//! back.

use super::*;

/// A TerrainRegion holding the region's voxels, in its `SmoothGrid` from
/// voxel 0, with its size in `ExtentsMax` (observed: the corners are both
/// included; an inverted region errors).
pub(in crate::api::methods) fn copy_region(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
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

pub(in crate::api::methods) fn paste_region(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
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

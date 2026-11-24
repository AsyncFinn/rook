use mlua::Lua;
use rook_dom::{DomError, InstanceId};
use rook_physics::assembly::{self};
use rook_types::{CFrame, NumberRange, NumberSequence, NumberSequenceKeypoint, Value, Vector3};

use crate::api::instance::{settle_joints, settle_joints_within};
use crate::api::methods::{RawMethod, mutate};
use crate::api::properties::{cframe, part_cframe, part_ids, set_cframe};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::convert::vector3_to_lua;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::world::World;
use crate::types::LCFrame;

pub(super) fn parts_of(w: &World, model: InstanceId) -> Vec<InstanceId> {
    w.dom
        .descendants(model)
        .into_iter()
        .filter(|d| w.dom.is_a_class(*d, part_ids().class))
        .collect()
}

fn primary_part(w: &World, model: InstanceId) -> Option<InstanceId> {
    match w.dom.ref_property(model, "PrimaryPart") {
        Some(id) => w.dom.get(id).is_some().then_some(id),
        None => None,
    }
}

/// `PVInstance:GetPivot` for a part is `CFrame * PivotOffset`; a model
/// with a PrimaryPart pivots on that part, whatever its stored pivot says
/// (measured in Studio 0.744: `WorldPivot` still reads the stored one).
pub(crate) fn pivot_of(w: &World, id: InstanceId) -> CFrame {
    if w.dom.is_a(id, "BasePart") {
        let offset = w
            .dom
            .cframe_property(id, "PivotOffset")
            .unwrap_or(CFrame::IDENTITY);
        return cframe(w, id) * offset;
    }
    if let Some(primary) = primary_part(w, id) {
        return pivot_of(w, primary);
    }
    match w.dom.optional_cframe_property(id, "WorldPivotData") {
        Some(c) => c,
        None => bounding_box(w, id).0,
    }
}

/// Move every part of a model rigidly by `delta` (applied in world space),
/// taking along whatever they're joined to.
fn transform_model(w: &mut World, model: InstanceId, delta: CFrame) -> Result<(), DomError> {
    let delta = rigid(delta);
    let moves: Vec<_> = parts_of(w, model)
        .into_iter()
        .map(|part| (part, delta * part_cframe(w, part)))
        .collect();
    assembly::move_parts(&mut w.dom, moves)?;
    if let Some(p) = w.dom.optional_cframe_property(model, "WorldPivotData") {
        let pid = w
            .dom
            .property_id(model, "WorldPivotData")
            .ok_or(DomError::Stale)?;
        w.dom
            .set_property(model, pid, Value::OptionalCFrame(Some(delta * p)))?;
    }
    Ok(())
}

/// A move made from a place's slightly skewed CFrames (`target *
/// current.inverse()`, where the inverse is a transpose) is skewed too;
/// left that way, moving a model again and again scales its pivot further
/// each time until it's infinite. Moves are rigid.
fn rigid(delta: CFrame) -> CFrame {
    delta.orthonormalize()
}

pub(super) fn get_pivot(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    settle_joints(lua, this);
    settle_joints_within(lua, this);
    Outcome::ret(lua, LCFrame(with_world(lua, |w| pivot_of(w, this))))
}

/// Deprecated; where the part is drawn, which is its `CFrame` (Observed:
/// Studio 0.747 gives the same CFrame for still, falling and character
/// parts, every frame). Rook draws parts where they are, without lag.
pub(super) fn get_render_cframe(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    settle_joints(lua, this);
    Outcome::ret(lua, LCFrame(with_world(lua, |w| cframe(w, this))))
}

pub(super) fn pivot_to(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let target: LCFrame = a.userdata(0, "CFrame")?;
    mutate(lua, |w| {
        let current = pivot_of(w, this);
        let delta = target.0 * current.inverse();
        if w.dom.is_a(this, "BasePart") {
            let c = cframe(w, this);
            set_cframe(w, this, rigid(delta) * c)
        } else {
            transform_model(w, this, delta)
        }
    })?;
    Outcome::ret(lua, ())
}

fn no_primary_part(method: &str) -> ScriptError {
    ScriptError(format!(
        "Model:{method}() failed because no PrimaryPart has been set, or the PrimaryPart no longer exists. Please set Model.PrimaryPart before using this."
    ))
}

pub(super) fn get_primary_part_cframe(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    settle_joints_within(lua, this);
    let c = with_world(lua, |w| primary_part(w, this).map(|p| cframe(w, p)))
        .ok_or_else(|| no_primary_part("GetPrimaryPartCFrame"))?;
    Outcome::ret(lua, LCFrame(c))
}

pub(super) fn set_primary_part_cframe(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let target: LCFrame = a.userdata(0, "CFrame")?;
    let primary = with_world(lua, |w| primary_part(w, this))
        .ok_or_else(|| no_primary_part("SetPrimaryPartCFrame"))?;
    mutate(lua, |w| {
        let delta = target.0 * cframe(w, primary).inverse();
        transform_model(w, this, delta)
    })?;
    Outcome::ret(lua, ())
}

/// Deprecated; the same CFrame as `GetBoundingBox`, not the primary part's
/// (Confirmed in Studio: door scripts rotate about it, so the hinge side
/// often carries an invisible part to pull the box center there).
pub(super) fn get_model_cframe(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let c = with_world(lua, |w| bounding_box(w, this).0);
    Outcome::ret(lua, LCFrame(c))
}

pub(super) fn translate_by(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let v = a.vector3(0)?;
    mutate(lua, |w| {
        transform_model(w, this, CFrame::from_position(v.0))
    })?;
    Outcome::ret(lua, ())
}

/// Moves the model so its primary part (or bounding box center) is at the
/// target, then up until nothing it collides with overlaps it. A model
/// outside Workspace, or an empty one, stays where it is (measured:
/// docs/behavior/physics/queries.md#modelmoveto).
pub(super) fn model_move_to(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let target = a.vector3(0)?;
    mutate(lua, |w| {
        let in_workspace =
            (w.dom.find_service("Workspace")).is_some_and(|ws| w.dom.is_ancestor_of(ws, this));
        let parts = parts_of(w, this);
        if !in_workspace || parts.is_empty() {
            return Ok(());
        }
        let from = match primary_part(w, this) {
            Some(p) => cframe(w, p).position,
            None => bounding_box(w, this).0.position,
        };
        let shift = target.0 - from;
        let moved: Vec<_> = parts
            .into_iter()
            .map(|p| (p, CFrame::from_position(shift) * cframe(w, p)))
            .collect();
        let w = &mut *w;
        let rise = w.physics.rise_out_of_parts(&mut w.dom, &moved);
        transform_model(w, this, CFrame::from_position(shift + glam::Vec3::Y * rise))
    })?;
    Outcome::ret(lua, ())
}

/// Bounds of a model's parts as (center, size), in a box turned to the
/// primary part's orientation, else to the saved pivot's, else to the world
/// axes (Confirmed: creator-docs and Studio; the pivot's observed in Studio
/// 0.742 on an archived spinning light, whose script turns it about
/// `GetModelCFrame()`'s X axis).
fn bounding_box(w: &World, model: InstanceId) -> (CFrame, Vector3) {
    let turn = match primary_part(w, model) {
        Some(p) => Some(cframe(w, p)),
        None => w.dom.optional_cframe_property(model, "WorldPivotData"),
    };
    let basis = match turn {
        Some(c) => CFrame {
            position: glam::Vec3::ZERO,
            ..c
        },
        None => CFrame::IDENTITY,
    };
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for part in parts_of(w, model) {
        let c = basis.to_object_space(part_cframe(w, part));
        let size = (w.dom)
            .read_property(part, part_ids().size, Value::as_vector3)
            .unwrap_or(glam::Vec3::ONE);
        let extent = c.box_reach(size);
        min = min.min(c.position - extent);
        max = max.max(c.position + extent);
    }
    if min.x > max.x {
        return (CFrame::IDENTITY, Vector3::ZERO);
    }
    let center = basis.to_world_space(CFrame::from_position((min + max) * 0.5));
    (center, Vector3(max - min))
}

pub(super) fn get_extents_size(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    settle_joints_within(lua, this);
    let size = with_world(lua, |w| bounding_box(w, this).1);
    Outcome::ret(lua, vector3_to_lua(size))
}

pub(super) fn get_bounding_box(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    settle_joints_within(lua, this);
    let (c, s) = with_world(lua, |w| bounding_box(w, this));
    Outcome::ret(lua, (LCFrame(c), vector3_to_lua(s)))
}

pub(super) fn get_mass(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let mass = with_world(lua, |w| w.physics.part_mass(&w.dom, this));
    Outcome::ret(lua, f64::from(mass))
}

/// The model's scale factor, its `Scale` property (saved as `ScaleFactor`),
/// 1 until `ScaleTo` changes it.
fn scale_of(w: &World, model: InstanceId) -> f32 {
    w.dom.float_property(model, "Scale").unwrap_or(1.0)
}

pub(super) fn get_scale(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, f64::from(with_world(lua, |w| scale_of(w, this))))
}

/// Scales the model about its pivot by `new / current` (creator-docs
/// Model.ScaleTo; measured in Studio 0.748): parts' sizes, pivot offsets
/// and offsets from the pivot, the offsets of attachments, bones, joints
/// and weld constraints, meshes' offsets and file meshes' scales (a
/// primitive mesh already scales with its part), models inside, whose
/// pivots move and whose `Scale` scales too, and [`SCALED_LENGTHS`].
pub(super) fn scale_to(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let new = a.number(0)? as f32;
    // Measured in Studio 0.748: NaN changes nothing; zero and below raise.
    // Inference: infinity is ignored like NaN.
    if new.is_nan() || new == f32::INFINITY {
        return Outcome::ret(lua, ());
    }
    if new <= 0.0 {
        return Err(ScriptError(
            "Only positive non-zero scale values are allowed.".into(),
        ));
    }
    settle_joints_within(lua, this);
    mutate(lua, |w| {
        // A scale that isn't positive can't be divided by; Inference: it
        // counts as 1 (`ScaleTo` never stores one).
        let old = Some(scale_of(w, this)).filter(|s| *s > 0.0).unwrap_or(1.0);
        let r = new / old;
        let pivot = pivot_of(w, this);
        let scale_offset = |c: CFrame| CFrame {
            position: c.position * r,
            ..c
        };
        let mut moves = Vec::new();
        for d in w.dom.descendants(this) {
            if w.dom.is_a(d, "BasePart") {
                let size = (w.dom)
                    .read_property(d, part_ids().size, Value::as_vector3)
                    .unwrap_or(glam::Vec3::ONE);
                w.dom
                    .set_property(d, part_ids().size, Value::Vector3(Vector3(size * r)))?;
                if let Some(c) = w.dom.cframe_property(d, "PivotOffset") {
                    w.dom
                        .set_property_by_name(d, "PivotOffset", Value::CFrame(scale_offset(c)))?;
                }
                let c = part_cframe(w, d);
                let local = scale_offset(pivot.to_object_space(c));
                moves.push((d, pivot.to_world_space(local)));
            } else if w.dom.is_a(d, "Model") {
                if let Some(c) = w.dom.optional_cframe_property(d, "WorldPivotData") {
                    let moved = pivot.to_world_space(scale_offset(pivot.to_object_space(c)));
                    w.dom.set_property_by_name(
                        d,
                        "WorldPivotData",
                        Value::OptionalCFrame(Some(moved)),
                    )?;
                }
                let inner = scale_of(w, d);
                w.dom
                    .set_property_by_name(d, "Scale", Value::Float32(inner * r))?;
            } else if w.dom.is_a(d, "WeldConstraint") {
                // Part1's pose against Part0, which the move below holds
                // the parts to.
                if let Some(c) = w.dom.cframe_property(d, "CFrame0") {
                    w.dom
                        .set_property_by_name(d, "CFrame0", Value::CFrame(scale_offset(c)))?;
                }
            } else if w.dom.is_a(d, "Attachment") {
                if let Some(c) = w.dom.cframe_property(d, "CFrame") {
                    w.dom
                        .set_property_by_name(d, "CFrame", Value::CFrame(scale_offset(c)))?;
                }
            } else if w.dom.is_a(d, "JointInstance") {
                for joint in ["C0", "C1"] {
                    if let Some(c) = w.dom.cframe_property(d, joint) {
                        w.dom
                            .set_property_by_name(d, joint, Value::CFrame(scale_offset(c)))?;
                    }
                }
            } else if w.dom.is_a(d, "DataModelMesh") {
                let file = w.dom.class_name(d) == "FileMesh"
                    || w.dom.enum_item_property(d, "MeshType") == Some("FileMesh");
                let props: &[&str] = if file {
                    &["Scale", "Offset"]
                } else {
                    &["Offset"]
                };
                for &prop in props {
                    if let Some(v) = w.dom.vector3_property(d, prop) {
                        w.dom
                            .set_property_by_name(d, prop, Value::Vector3(Vector3(v * r)))?;
                    }
                }
            }
            for &(class, props) in SCALED_LENGTHS {
                if w.dom.is_a(d, class) {
                    for &prop in props {
                        scale_property(w, d, prop, r)?;
                    }
                }
            }
            if w.dom.is_a(d, "Humanoid") {
                scale_property(w, d, "JumpPower", r.sqrt())?;
            }
        }
        assembly::move_parts(&mut w.dom, moves)?;
        w.dom
            .set_property_by_name(this, "Scale", Value::Float32(new))
    })?;
    Outcome::ret(lua, ())
}

/// Lengths, distances and speeds in a model that `ScaleTo` scales by the
/// factor, as measured in Studio 0.748 with `ScaleTo(2)`; a Humanoid's
/// `JumpPower` scales by its square root (50 read 70.71), so the height it
/// reaches scales by the factor. A BillboardGui's `Size` and a Light's
/// `Range` stay as they are. Inference: CylindricalConstraint's limits
/// scale as PrismaticConstraint's (both are SlidingBallConstraints), a
/// Beam's `CurveSize1` as its `CurveSize0`, and a ParticleEmitter's `Size`
/// envelopes with its sizes.
const SCALED_LENGTHS: &[(&str, &[&str])] = &[
    ("RopeConstraint", &["Length", "Thickness"]),
    ("RodConstraint", &["Length", "Thickness"]),
    (
        "SpringConstraint",
        &["FreeLength", "Radius", "MinLength", "MaxLength"],
    ),
    ("SlidingBallConstraint", &["UpperLimit", "LowerLimit"]),
    ("ParticleEmitter", &["Size", "Speed", "Acceleration"]),
    ("Fire", &["Size"]),
    ("Beam", &["Width0", "Width1", "CurveSize0", "CurveSize1"]),
    ("Trail", &["MaxLength"]),
    ("Texture", &["StudsPerTileU", "StudsPerTileV"]),
    ("Sound", &["RollOffMinDistance", "RollOffMaxDistance"]),
    ("BillboardGui", &["StudsOffset", "MaxDistance"]),
    ("Humanoid", &["HipHeight", "WalkSpeed", "JumpHeight"]),
];

/// Multiplies `d`'s number, Vector3, NumberRange or NumberSequence `prop`
/// by `r`, keeping its type.
fn scale_property(w: &mut World, d: InstanceId, prop: &str, r: f32) -> Result<(), DomError> {
    let scaled = w.dom.read_by_name(d, prop, |v| {
        Some(match v {
            Value::Float32(x) => Value::Float32(x * r),
            Value::Float64(x) => Value::Float64(x * f64::from(r)),
            Value::Vector3(v) => Value::Vector3(Vector3(v.0 * r)),
            Value::NumberRange(n) => Value::NumberRange(NumberRange {
                min: n.min * r,
                max: n.max * r,
            }),
            Value::NumberSequence(s) => Value::NumberSequence(NumberSequence {
                keypoints: (s.keypoints.iter())
                    .map(|k| NumberSequenceKeypoint {
                        value: k.value * r,
                        envelope: k.envelope * r,
                        ..*k
                    })
                    .collect(),
            }),
            _ => return None,
        })
    });
    match scaled {
        Some(v) => w.dom.set_property_by_name(d, prop, v),
        None => Ok(()),
    }
}

/// `WorldRoot:BulkMoveTo(parts, cframes, eventMode)`: each part moves as
/// setting its CFrame would, taking its assembly along, in one change set,
/// with the errors measured in Studio 0.748 (entries that aren't Instances
/// first, then the lengths, the parts and the CFrames). Approximation:
/// `eventMode` is checked but changes are signalled as any move's.
pub(super) fn bulk_move_to(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let parts = a.instances(0)?;
    if !a.get(2).is_nil() {
        a.enum_value(lua, 2, "BulkMoveMode")?;
    }
    let mlua::Value::Table(list) = a.get(1) else {
        return Err(a.wrong(1, "table"));
    };
    let values = list
        .sequence_values::<mlua::Value>()
        .collect::<mlua::Result<Vec<_>>>()?;
    if parts.len() != values.len() {
        return Err(ScriptError(format!(
            "Length of part list ({}) and length of CFrame list ({}) do not match.",
            parts.len(),
            values.len()
        )));
    }
    let not_part = with_world(lua, |w| {
        (parts.iter().enumerate())
            .find(|(_, p)| !w.dom.is_a(**p, "BasePart"))
            .map(|(i, p)| (i, w.dom.class_name(*p).to_owned()))
    });
    if let Some((i, class)) = not_part {
        return Err(ScriptError(format!(
            "Can only call BulkMoveTo on BaseParts, partList[{}] is a {class}.",
            i + 1
        )));
    }
    let mut cframes = Vec::with_capacity(values.len());
    for (i, v) in values.iter().enumerate() {
        match v {
            mlua::Value::UserData(u) if u.borrow::<LCFrame>().is_ok() => {
                cframes.push(u.borrow::<LCFrame>()?.0);
            }
            v => {
                return Err(ScriptError(format!(
                    "CFrame list must only contain CFrames, cframeList[{}] is a {}.",
                    i + 1,
                    crate::runtime::ctx::args::type_name(v)
                )));
            }
        }
    }
    mutate(lua, |w| {
        assembly::move_parts(&mut w.dom, parts.into_iter().zip(cframes))
    })?;
    Outcome::ret(lua, ())
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("BasePart", "GetRenderCFrame") => get_render_cframe,
        ("PVInstance", "GetPivot") => get_pivot,
        ("PVInstance", "PivotTo") => pivot_to,
        ("Model", "GetPrimaryPartCFrame") => get_primary_part_cframe,
        ("Model", "SetPrimaryPartCFrame") => set_primary_part_cframe,
        ("Model", "GetModelCFrame") => get_model_cframe,
        ("Model", "TranslateBy") => translate_by,
        ("Model", "MoveTo" | "move" | "moveTo") => model_move_to,
        ("Model", "GetExtentsSize" | "GetModelSize") => get_extents_size,
        ("Model", "GetBoundingBox") => get_bounding_box,
        ("WorldRoot", "BulkMoveTo") => bulk_move_to,
        ("Model", "GetScale") => get_scale,
        ("Model", "ScaleTo") => scale_to,
        ("BasePart", "GetMass" | "getMass") => get_mass,
        _ => return None,
    })
}

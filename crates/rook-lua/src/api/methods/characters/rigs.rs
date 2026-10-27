//! Characters' bodies: StarterPlayer's StarterCharacter, Roblox's R15 rig
//! file, or the R6 rig built here from Studio's measurements
//! (docs/behavior/characters/rigs.md).

use std::sync::Arc;

use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{CFrame, Color3, Value, Vector3};

use super::appearance::{self, Avatar};
use crate::api::properties::content_value;
use crate::world::World;

/// `Enum.HumanoidRigType`'s two rigs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RigType {
    R6,
    R15,
}

/// Builds the default body of a rig type, detached in `dom` (the engine's,
/// from Roblox's `avatar/characterR15DynamicHeadV2.rbxm`), or `None` when its file
/// can't be had.
/// The builder itself.
pub type BuildRig = dyn Fn(&mut Dom, RigType) -> Option<InstanceId> + Send + Sync;

#[derive(Clone)]
pub struct RigSource(pub Arc<BuildRig>);

impl std::fmt::Debug for RigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RigSource")
    }
}

/// A new, detached body for `player`, named after it, with its root part at
/// the origin: StarterPlayer's StarterCharacter as it is (creator-docs
/// StarterPlayer), else the rig the game's avatar setting picks (the
/// player's own with PlayerChoice) dressed in the player's look.
pub(super) fn body(w: &mut World, player: InstanceId) -> Result<InstanceId, DomError> {
    let starter = w
        .dom
        .find_service("StarterPlayer")
        .and_then(|sp| w.dom.find_first_child(sp, "StarterCharacter", false))
        .filter(|&c| w.dom.is_a(c, "Model"));
    let avatar = w.avatars.get(&player).cloned().unwrap_or_default();
    let model = match starter.and_then(|c| w.dom.clone_instance(c)) {
        Some(model) => model,
        None => {
            let rig = rig_type(&w.dom, &avatar);
            let built = match (rig, w.rigs.clone()) {
                (RigType::R15, Some(source)) => (source.0)(&mut w.dom, RigType::R15),
                _ => None,
            };
            let model = match built {
                Some(model) => model,
                // Without the R15 file the body is R6, which needs no file.
                None => r6(&mut w.dom)?,
            };
            appearance::dress(&mut w.dom, model, &avatar)?;
            model
        }
    };
    w.dom.set_name(model, w.dom.name(player).to_owned())?;
    if let Some(root) = w.dom.find_first_child(model, "HumanoidRootPart", false) {
        place_at(&mut w.dom, model, root, CFrame::IDENTITY)?;
        w.dom
            .set_property_by_name(model, "PrimaryPart", Value::Ref(Some(root.to_ref())))?;
    }
    Ok(model)
}

/// StarterPlayer's avatar setting (`GameSettingsAvatar`, from the game's
/// settings on Roblox): R6, R15, or the player's own with PlayerChoice.
fn rig_type(dom: &Dom, avatar: &Avatar) -> RigType {
    let setting = dom
        .find_service("StarterPlayer")
        .and_then(|sp| dom.enum_item_property(sp, "GameSettingsAvatar"));
    match setting {
        Some("R6") => RigType::R6,
        Some("PlayerChoice") => avatar.rig,
        _ => RigType::R15,
    }
}

/// Move `model`'s parts, keeping their places around `root`, so `root`
/// is at `at`.
pub(super) fn place_at(
    dom: &mut Dom,
    model: InstanceId,
    root: InstanceId,
    at: CFrame,
) -> Result<(), DomError> {
    let Some(from) = dom.cframe_property(root, "CFrame") else {
        return Ok(());
    };
    let to_new = at * from.inverse();
    for part in dom.descendants(model) {
        if let Some(cf) = dom.cframe_property(part, "CFrame")
            && dom.is_a(part, "BasePart")
        {
            dom.set_property_by_name(
                part,
                "CFrame",
                Value::CFrame((to_new * cf).orthonormalize()),
            )?;
        }
    }
    Ok(())
}

/// One R6 part: name, size, place relative to the root part, CanCollide.
const R6_PARTS: [(&str, [f32; 3], [f32; 3], bool); 6] = [
    ("Head", [2.0, 1.0, 1.0], [0.0, 1.5, 0.0], true),
    ("Torso", [2.0, 2.0, 1.0], [0.0, 0.0, 0.0], true),
    ("Left Arm", [1.0, 2.0, 1.0], [-1.5, 0.0, 0.0], false),
    ("Right Arm", [1.0, 2.0, 1.0], [1.5, 0.0, 0.0], false),
    ("Left Leg", [1.0, 2.0, 1.0], [-0.5, -2.0, 0.0], false),
    ("Right Leg", [1.0, 2.0, 1.0], [0.5, -2.0, 0.0], false),
];

/// Rotations of R6's joint frames (rows of the matrix).
const FACE_RIGHT: [f32; 9] = [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0];
const FACE_LEFT: [f32; 9] = [0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0];
const UPRIGHT: [f32; 9] = [-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0];

/// An R6 Motor6D: name, holder and Part0, Part1, C0 and C1 positions,
/// and the rotation both frames share.
type R6Joint = (
    &'static str,
    &'static str,
    &'static str,
    [f32; 3],
    [f32; 3],
    [f32; 9],
);

const R6_JOINTS: [R6Joint; 6] = [
    (
        "Right Shoulder",
        "Torso",
        "Right Arm",
        [1.0, 0.5, 0.0],
        [-0.5, 0.5, 0.0],
        FACE_RIGHT,
    ),
    (
        "Left Shoulder",
        "Torso",
        "Left Arm",
        [-1.0, 0.5, 0.0],
        [0.5, 0.5, 0.0],
        FACE_LEFT,
    ),
    (
        "Right Hip",
        "Torso",
        "Right Leg",
        [1.0, -1.0, 0.0],
        [0.5, 1.0, 0.0],
        FACE_RIGHT,
    ),
    (
        "Left Hip",
        "Torso",
        "Left Leg",
        [-1.0, -1.0, 0.0],
        [-0.5, 1.0, 0.0],
        FACE_LEFT,
    ),
    (
        "Neck",
        "Torso",
        "Head",
        [0.0, 1.0, 0.0],
        [0.0, -0.5, 0.0],
        UPRIGHT,
    ),
    (
        "RootJoint",
        "HumanoidRootPart",
        "Torso",
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        UPRIGHT,
    ),
];

/// R6's attachments: part, name, position (all unrotated).
const R6_ATTACHMENTS: [(&str, &str, [f32; 3]); 18] = [
    ("Head", "HairAttachment", [0.0, 0.6, 0.0]),
    ("Head", "HatAttachment", [0.0, 0.6, 0.0]),
    ("Head", "FaceFrontAttachment", [0.0, 0.0, -0.6]),
    ("Head", "FaceCenterAttachment", [0.0, 0.0, 0.0]),
    ("Torso", "NeckAttachment", [0.0, 1.0, 0.0]),
    ("Torso", "BodyFrontAttachment", [0.0, 0.0, -0.5]),
    ("Torso", "BodyBackAttachment", [0.0, 0.0, 0.5]),
    ("Torso", "LeftCollarAttachment", [-1.0, 1.0, 0.0]),
    ("Torso", "RightCollarAttachment", [1.0, 1.0, 0.0]),
    ("Torso", "WaistFrontAttachment", [0.0, -1.0, -0.5]),
    ("Torso", "WaistCenterAttachment", [0.0, -1.0, 0.0]),
    ("Torso", "WaistBackAttachment", [0.0, -1.0, 0.5]),
    ("Left Arm", "LeftShoulderAttachment", [0.0, 1.0, 0.0]),
    ("Left Arm", "LeftGripAttachment", [0.0, -1.0, 0.0]),
    ("Right Arm", "RightShoulderAttachment", [0.0, 1.0, 0.0]),
    ("Right Arm", "RightGripAttachment", [0.0, -1.0, 0.0]),
    ("Left Leg", "LeftFootAttachment", [0.0, -1.0, 0.0]),
    ("Right Leg", "RightFootAttachment", [0.0, -1.0, 0.0]),
];

fn frame(position: [f32; 3], rotation: [f32; 9]) -> CFrame {
    let [x, y, z] = position;
    let [a, b, c, d, e, f, g, h, i] = rotation;
    CFrame::from_components([x, y, z, a, b, c, d, e, f, g, h, i])
}

fn vector(v: [f32; 3]) -> Value {
    Value::Vector3(Vector3(glam::Vec3::from(v)))
}

/// A new instance of `class` named `name` under `parent`, with `props`.
fn add(
    dom: &mut Dom,
    class: &str,
    name: &str,
    parent: InstanceId,
    props: &[(&str, Value)],
) -> Result<InstanceId, DomError> {
    let id = dom.create_by_name(class)?;
    dom.set_name(id, name.to_owned())?;
    for (prop, value) in props {
        dom.set_property_by_name(id, prop, value.clone())?;
    }
    dom.set_parent(id, Some(parent))?;
    Ok(id)
}

/// The R6 rig, as Studio 0.744 builds it for a HumanoidDescription
/// (`Players:CreateHumanoidModelFromDescription`), in its child order.
fn r6(dom: &mut Dom) -> Result<InstanceId, DomError> {
    let model = dom.create_by_name("Model")?;
    let part = |dom: &mut Dom, name: &str, size: [f32; 3], at: [f32; 3], collide: bool| {
        add(
            dom,
            "Part",
            name,
            model,
            &[
                ("Size", vector(size)),
                ("CFrame", Value::CFrame(CFrame::from_position(at.into()))),
                ("CanCollide", Value::Bool(collide)),
                // Bodies have smooth sides (Studio's rig shows no studs).
                ("TopSurface", Value::Enum(0)),
                ("BottomSurface", Value::Enum(0)),
            ],
        )
    };
    let mut parts = Vec::new();
    for (name, size, at, collide) in R6_PARTS {
        parts.push((name, part(dom, name, size, at, collide)?));
    }
    add(dom, "Humanoid", "Humanoid", model, &[])?;
    let root = part(dom, "HumanoidRootPart", [2.0, 2.0, 1.0], [0.0; 3], false)?;
    dom.set_property_by_name(root, "Transparency", Value::Float32(1.0))?;
    dom.set_property_by_name(
        root,
        "Color",
        Value::Color3(Color3::from_rgb(163, 162, 165)),
    )?;
    parts.push(("HumanoidRootPart", root));
    let find = |name: &str| parts.iter().find(|(n, _)| *n == name).map(|(_, id)| *id);
    let head = find("Head").ok_or(DomError::Stale)?;
    add(
        dom,
        "SpecialMesh",
        "Mesh",
        head,
        &[
            ("MeshType", Value::Enum(0)),
            ("Scale", vector([1.25, 1.25, 1.25])),
        ],
    )?;
    add(
        dom,
        "Decal",
        "face",
        head,
        &[(
            "TextureContent",
            content_value("rbxasset://textures/face.png"),
        )],
    )?;
    let torso = find("Torso").ok_or(DomError::Stale)?;
    add(dom, "Decal", "roblox", torso, &[])?;
    for (holder, name, at) in R6_ATTACHMENTS {
        let holder = find(holder).ok_or(DomError::Stale)?;
        add(
            dom,
            "Attachment",
            name,
            holder,
            &[("CFrame", Value::CFrame(CFrame::from_position(at.into())))],
        )?;
    }
    for (name, holder, part1, c0, c1, rotation) in R6_JOINTS {
        let (Some(holder), Some(part1)) = (find(holder), find(part1)) else {
            continue;
        };
        add(
            dom,
            "Motor6D",
            name,
            holder,
            &[
                ("Part0", Value::Ref(Some(holder.to_ref()))),
                ("Part1", Value::Ref(Some(part1.to_ref()))),
                ("C0", Value::CFrame(frame(c0, rotation))),
                ("C1", Value::CFrame(frame(c1, rotation))),
            ],
        )?;
    }
    Ok(model)
}

//! How characters look: a player's avatar from the `avatar` web domain
//! ([`super::super::web`]) when it has an endpoint, else Roblox's classic
//! default look, and HumanoidDescriptions as far as body colors and
//! scales. Body part meshes, clothes and accessories are M11's.

use mlua::{Lua, MultiValue};
use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{CFrame, Color3, Value, Vector3};
use serde_json::Value as Json;

use super::super::web::{self, WebRequest};
use super::rigs::RigType;
use crate::api::instance::push_instance;
use crate::api::methods::{mutate, ret_instance};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};

/// A player's look.
#[derive(Debug, Clone, PartialEq)]
pub struct Avatar {
    /// The rig the player picked, used when the game lets players choose.
    pub rig: RigType,
    /// Head, torso, left arm, right arm, left leg, right leg.
    pub colors: [Color3; 6],
    pub scales: Scales,
}

/// HumanoidDescription's body scales.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scales {
    pub height: f32,
    pub width: f32,
    pub depth: f32,
    pub head: f32,
    /// How far parts blend from their own sizes toward Rthro's (`BodyTypeScale`).
    pub body_type: f32,
    /// How far that Rthro shape is slender rather than wide (`ProportionScale`).
    pub proportion: f32,
}

impl Scales {
    /// The rig file's own sizes.
    const UNSCALED: Self = Self {
        height: 1.0,
        width: 1.0,
        depth: 1.0,
        head: 1.0,
        body_type: 0.0,
        proportion: 0.0,
    };
}

impl Default for Scales {
    /// A new HumanoidDescription's.
    fn default() -> Self {
        Self {
            body_type: 0.3,
            proportion: 1.0,
            ..Self::UNSCALED
        }
    }
}

/// Each R15 part's size factor at `BodyTypeScale` 1, by its name without
/// Left or Right: with `ProportionScale` 0 (wide), then 1 (slender).
/// Factors between are bilinear in the two (Studio 0.744, measured to
/// within 1e-5).
const RTHRO: [(&str, [f32; 3], [f32; 3]); 9] = [
    ("Head", [0.942, 0.942, 0.942], [0.896289, 0.942, 0.896289]),
    (
        "UpperTorso",
        [1.033, 1.309, 1.14],
        [0.905346, 1.204232, 1.013333],
    ),
    (
        "LowerTorso",
        [1.033, 1.309, 1.14],
        [0.985687, 1.004605, 1.013333],
    ),
    (
        "UpperArm",
        [1.129, 1.342, 1.132],
        [1.003556, 1.207921, 1.006222],
    ),
    (
        "LowerArm",
        [1.129, 1.342, 1.132],
        [1.003556, 1.207921, 1.006222],
    ),
    ("Hand", [1.066, 1.174, 1.231], [0.947556, 1.174, 1.094222]),
    (
        "UpperLeg",
        [1.023, 1.506, 1.023],
        [0.976145, 1.40093, 0.909333],
    ),
    (
        "LowerLeg",
        [1.023, 1.506, 1.023],
        [0.976145, 1.300518, 0.909333],
    ),
    ("Foot", [1.079, 1.267, 1.129], [1.02958, 1.133274, 1.003556]),
];

/// BrickColor numbers of Roblox's classic default look: Bright yellow
/// head and arms, Bright blue torso, Br. yellowish green legs.
const DEFAULT_COLORS: [u16; 6] = [24, 23, 24, 24, 37, 37];

fn brick(number: u16) -> Color3 {
    rbx_types::BrickColor::from_number(number)
        .map(Color3::from)
        .unwrap_or_default()
}

impl Default for Avatar {
    fn default() -> Self {
        Self {
            rig: RigType::R15,
            colors: DEFAULT_COLORS.map(brick),
            scales: Scales::default(),
        }
    }
}

/// HumanoidDescription's and BodyColors' color properties, in
/// [`Avatar::colors`] order.
const DESCRIPTION_COLORS: [&str; 6] = [
    "HeadColor",
    "TorsoColor",
    "LeftArmColor",
    "RightArmColor",
    "LeftLegColor",
    "RightLegColor",
];

/// The parts each color paints, R6's names and R15's.
const PAINTED: [&[&str]; 6] = [
    &["Head"],
    &["Torso", "UpperTorso", "LowerTorso"],
    &["Left Arm", "LeftUpperArm", "LeftLowerArm", "LeftHand"],
    &["Right Arm", "RightUpperArm", "RightLowerArm", "RightHand"],
    &["Left Leg", "LeftUpperLeg", "LeftLowerLeg", "LeftFoot"],
    &["Right Leg", "RightUpperLeg", "RightLowerLeg", "RightFoot"],
];

/// Read `avatar.roblox.com/v1/users/{id}/avatar`'s answer.
fn from_json(json: &Json) -> Avatar {
    let colors = &json["bodyColors"];
    let id = |key: &str, default: u16| {
        colors[key]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .unwrap_or(default)
    };
    let keys = [
        "headColorId",
        "torsoColorId",
        "leftArmColorId",
        "rightArmColorId",
        "leftLegColorId",
        "rightLegColorId",
    ];
    let scale =
        |key: &str, default: f32| json["scales"][key].as_f64().map_or(default, |v| v as f32);
    let defaults = Scales::default();
    Avatar {
        rig: match json["playerAvatarType"].as_str() {
            Some("R6") => RigType::R6,
            _ => RigType::R15,
        },
        colors: std::array::from_fn(|i| brick(id(keys[i], DEFAULT_COLORS[i]))),
        scales: Scales {
            height: scale("height", defaults.height),
            width: scale("width", defaults.width),
            depth: scale("depth", defaults.depth),
            head: scale("head", defaults.head),
            body_type: scale("bodyType", defaults.body_type),
            proportion: scale("proportion", defaults.proportion),
        },
    }
}

/// Ask the `avatar` domain for `player`'s look, kept for its spawns; with
/// no endpoint it has the default look at once. Returns whether an answer
/// is on its way.
pub(crate) fn request(lua: &Lua, player: InstanceId) -> bool {
    let user = with_world(lua, |w| w.dom.number_property(player, "UserId")).unwrap_or(0.0) as i64;
    let Some(url) = web::url(lua, "avatar", &format!("/v1/users/{user}/avatar")) else {
        return false;
    };
    with_world(lua, |w| w.avatars_pending.insert(player));
    web::send(
        lua,
        WebRequest::get(url),
        Box::new(move |lua, answer| {
            let avatar = web::json(answer).map(|j| from_json(&j));
            with_world(lua, |w| {
                w.avatars_pending.remove(&player);
                match avatar {
                    Ok(avatar) => {
                        w.avatars.insert(player, avatar);
                    }
                    Err(e) => tracing::warn!("avatar of user {user}: {e}"),
                }
            });
            Ok(MultiValue::new())
        }),
    );
    true
}

/// Paint a new body, scale it and give its Humanoid a HumanoidDescription
/// of `avatar`, as Roblox's own spawning does (observed: Studio's
/// characters carry Body Colors and the description they were built from).
pub(super) fn dress(dom: &mut Dom, model: InstanceId, avatar: &Avatar) -> Result<(), DomError> {
    paint(dom, model, &avatar.colors)?;
    let colors = dom.create_by_name("BodyColors")?;
    dom.set_name(colors, "Body Colors".into())?;
    for (i, c) in avatar.colors.iter().enumerate() {
        dom.set_property_by_name(
            colors,
            &format!("{}3", DESCRIPTION_COLORS[i]),
            Value::Color3(*c),
        )?;
    }
    dom.set_parent(colors, Some(model))?;
    let Some(humanoid) = dom.find_first_child_of_class(model, "Humanoid") else {
        return Ok(());
    };
    scale(dom, model, Scales::UNSCALED, avatar.scales)?;
    let description = dom.create_by_name("HumanoidDescription")?;
    write_description(dom, description, avatar)?;
    dom.set_parent(description, Some(humanoid))
}

fn paint(dom: &mut Dom, model: InstanceId, colors: &[Color3; 6]) -> Result<(), DomError> {
    for (names, color) in PAINTED.iter().zip(colors) {
        for name in *names {
            if let Some(part) = dom.find_first_child(model, name, false) {
                dom.set_property_by_name(part, "Color", Value::Color3(*color))?;
            }
        }
    }
    Ok(())
}

fn write_description(dom: &mut Dom, d: InstanceId, avatar: &Avatar) -> Result<(), DomError> {
    for (name, c) in DESCRIPTION_COLORS.iter().zip(&avatar.colors) {
        dom.set_property_by_name(d, name, Value::Color3(*c))?;
    }
    let s = avatar.scales;
    for (name, v) in [
        ("HeightScale", s.height),
        ("WidthScale", s.width),
        ("DepthScale", s.depth),
        ("HeadScale", s.head),
        ("BodyTypeScale", s.body_type),
        ("ProportionScale", s.proportion),
    ] {
        dom.set_property_by_name(d, name, Value::Float32(v))?;
    }
    Ok(())
}

fn read_description(dom: &Dom, d: InstanceId) -> Avatar {
    let mut avatar = Avatar::default();
    for (i, name) in DESCRIPTION_COLORS.iter().enumerate() {
        if let Some(c) = dom.color3_property(d, name) {
            avatar.colors[i] = c;
        }
    }
    let s = avatar.scales;
    let scale = |name, default| dom.float_property(d, name).unwrap_or(default);
    avatar.scales = Scales {
        height: scale("HeightScale", s.height),
        width: scale("WidthScale", s.width),
        depth: scale("DepthScale", s.depth),
        head: scale("HeadScale", s.head),
        body_type: scale("BodyTypeScale", s.body_type),
        proportion: scale("ProportionScale", s.proportion),
    };
    avatar
}

/// A part's size factor at `scales`: its Rthro blend times the classic
/// scales (the head's own, the rest's width, height and depth), which
/// multiply in Studio 0.744.
fn part_factor(name: &str, scales: Scales) -> glam::Vec3 {
    let side = name
        .strip_prefix("Left")
        .or_else(|| name.strip_prefix("Right"))
        .unwrap_or(name);
    let rthro =
        RTHRO
            .iter()
            .find(|(n, ..)| *n == side)
            .map_or(glam::Vec3::ONE, |&(_, wide, slender)| {
                let shape = glam::Vec3::from(wide).lerp(slender.into(), scales.proportion);
                glam::Vec3::ONE.lerp(shape, scales.body_type)
            });
    let classic = if name == "Head" {
        glam::Vec3::splat(scales.head)
    } else {
        glam::Vec3::new(scales.width, scales.height, scales.depth)
    };
    rthro * classic
}

/// Resize an R15 body from scales `from` to `to`, when its Humanoid
/// scales automatically: each part, its attachments and its ends of
/// joints by [`part_factor`], then its HipHeight to the legs' new
/// length. Matches Studio for bodies of Classic parts (the rig file's);
/// other `AvatarPartScaleType`s are M11's.
fn scale(dom: &mut Dom, model: InstanceId, from: Scales, to: Scales) -> Result<(), DomError> {
    let Some(humanoid) = dom.find_first_child_of_class(model, "Humanoid") else {
        return Ok(());
    };
    if from == to
        || dom.enum_item_property(humanoid, "RigType") != Some("R15")
        || dom.bool_property(humanoid, "AutomaticScalingEnabled") == Some(false)
    {
        return Ok(());
    }
    let factor = |part: InstanceId, dom: &Dom| {
        let name = dom.name(part);
        part_factor(name, to) / part_factor(name, from)
    };
    let parts: Vec<InstanceId> = dom
        .descendants(model)
        .into_iter()
        .filter(|&p| dom.is_a(p, "BasePart"))
        .collect();
    for &part in &parts {
        let f = factor(part, dom);
        if let Some(size) = dom.vector3_property(part, "Size") {
            dom.set_property_by_name(part, "Size", Value::Vector3(Vector3(size * f)))?;
        }
        for child in dom.children(part).to_vec() {
            if dom.is_a(child, "Attachment")
                && let Some(mut cf) = dom.cframe_property(child, "CFrame")
            {
                cf.position *= f;
                dom.set_property_by_name(child, "CFrame", Value::CFrame(cf))?;
            }
        }
    }
    for joint in dom.descendants(model) {
        if !dom.is_a(joint, "JointInstance") {
            continue;
        }
        for (end, frame) in [("Part0", "C0"), ("Part1", "C1")] {
            if let (Some(part), Some(mut cf)) = (
                dom.ref_property(joint, end),
                dom.cframe_property(joint, frame),
            ) {
                cf.position *= factor(part, dom);
                dom.set_property_by_name(joint, frame, Value::CFrame(cf))?;
            }
        }
    }
    snap(dom, model)?;
    if let Some(hip) = hip_height(dom, model) {
        dom.set_property_by_name(humanoid, "HipHeight", Value::Float32(hip))?;
    }
    Ok(())
}

/// How far the legs reach below the root part's bottom, which an
/// automatically scaled R15 Humanoid's HipHeight follows (Studio 0.744:
/// 2 for the rig file as it is, 2.1924 for a new description's body).
fn hip_height(dom: &Dom, model: InstanceId) -> Option<f32> {
    let root = super::root_part(dom, model)?;
    let to_root = dom.cframe_property(root, "CFrame")?.inverse();
    let root_half = dom.vector3_property(root, "Size")?.y / 2.0;
    let lowest = dom
        .children(model)
        .iter()
        .filter(|&&p| {
            let name = dom.name(p);
            dom.is_a(p, "BasePart") && (name.contains("Leg") || name.contains("Foot"))
        })
        .filter_map(|&p| {
            let at = to_root * dom.cframe_property(p, "CFrame")?;
            let size = dom.vector3_property(p, "Size")?;
            Some(at.position.y - at.box_reach(size).y)
        })
        .reduce(f32::min)?;
    Some(-lowest - root_half)
}

/// Put each part where its joints hold it, from the root part out.
fn snap(dom: &mut Dom, model: InstanceId) -> Result<(), DomError> {
    let Some(root) = super::root_part(dom, model) else {
        return Ok(());
    };
    let joints: Vec<(InstanceId, InstanceId, CFrame, CFrame)> = dom
        .descendants(model)
        .into_iter()
        .filter(|&j| dom.is_a(j, "JointInstance"))
        .filter_map(|j| {
            Some((
                dom.ref_property(j, "Part0")?,
                dom.ref_property(j, "Part1")?,
                dom.cframe_property(j, "C0")?,
                dom.cframe_property(j, "C1")?,
            ))
        })
        .collect();
    let mut placed = vec![root];
    let mut i = 0;
    while i < placed.len() {
        let from = placed[i];
        i += 1;
        let Some(at) = dom.cframe_property(from, "CFrame") else {
            continue;
        };
        for &(p0, p1, c0, c1) in &joints {
            let (other, cf) = if p0 == from {
                (p1, at * c0 * c1.inverse())
            } else if p1 == from {
                (p0, at * c1 * c0.inverse())
            } else {
                continue;
            };
            if !placed.contains(&other) {
                dom.set_property_by_name(other, "CFrame", Value::CFrame(cf))?;
                placed.push(other);
            }
        }
    }
    Ok(())
}

/// `Humanoid:ApplyDescription(description)`: the body takes its colors
/// and scales (creator-docs; the rest of a description is M11's), and the
/// Humanoid keeps a copy as its applied description.
pub(super) fn apply_description(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let description = a.instance(0)?;
    mutate(lua, |w| {
        let dom = &mut w.dom;
        let Some(model) = dom.parent(this) else {
            return Ok(());
        };
        let to = read_description(dom, description);
        let current = dom.find_first_child_of_class(this, "HumanoidDescription");
        let from = current.map_or_else(Scales::default, |d| read_description(dom, d).scales);
        paint(dom, model, &to.colors)?;
        if let Some(colors) = dom.find_first_child_of_class(model, "BodyColors") {
            for (i, c) in to.colors.iter().enumerate() {
                dom.set_property_by_name(
                    colors,
                    &format!("{}3", DESCRIPTION_COLORS[i]),
                    Value::Color3(*c),
                )?;
            }
        }
        scale(dom, model, from, to.scales)?;
        if let Some(old) = current {
            dom.destroy(old)?;
        }
        if let Some(copy) = dom.clone_instance(description) {
            dom.set_parent(copy, Some(this))?;
        }
        Ok(())
    })?;
    Outcome::ret(lua, ())
}

/// `Humanoid:GetAppliedDescription()`: a copy of the description the body
/// was last dressed in.
pub(super) fn get_applied_description(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let copy = mutate(lua, |w| {
        let dom = &mut w.dom;
        let copy = match dom.find_first_child_of_class(this, "HumanoidDescription") {
            Some(d) => dom.clone_instance(d),
            None => None,
        };
        match copy {
            Some(copy) => Ok(copy),
            None => dom.create_by_name("HumanoidDescription"),
        }
    })?;
    ret_instance(lua, Some(copy))
}

/// `Players:GetHumanoidDescriptionFromUserId(id)`: `GET
/// avatar/v1/users/{id}/avatar` read into a new HumanoidDescription; with
/// no endpoint, the default look's.
pub(super) fn description_from_user_id(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let user = a.number(0)? as i64;
    let make = |lua: &Lua, avatar: &Avatar| -> Result<MultiValue, String> {
        let d = with_world(lua, |w| {
            let d = w.dom.create_by_name("HumanoidDescription")?;
            write_description(&mut w.dom, d, avatar)?;
            Ok::<_, DomError>(d)
        })
        .map_err(|e| e.to_string())?;
        let ud = push_instance(lua, d).map_err(|e| e.to_string())?;
        Ok(MultiValue::from_vec(vec![mlua::Value::UserData(ud)]))
    };
    let Some(url) = web::url(lua, "avatar", &format!("/v1/users/{user}/avatar")) else {
        let values = make(lua, &Avatar::default()).map_err(crate::ctx::ScriptError)?;
        return Ok(Outcome::Return(values));
    };
    web::call(
        lua,
        WebRequest::get(url),
        Box::new(move |lua, answer| make(lua, &from_json(&web::json(answer)?))),
    )
}

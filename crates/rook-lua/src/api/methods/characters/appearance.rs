//! How characters look: a player's avatar from the `avatar` web domain
//! ([`super::super::web`]) when it has an endpoint, else Roblox's classic
//! default look, and HumanoidDescriptions as far as body colors and
//! scale. Body part meshes, clothes and accessories are M11's.

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

/// HumanoidDescription's classic body scales.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scales {
    pub height: f32,
    pub width: f32,
    pub depth: f32,
    pub head: f32,
}

impl Default for Scales {
    fn default() -> Self {
        Self {
            height: 1.0,
            width: 1.0,
            depth: 1.0,
            head: 1.0,
        }
    }
}

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
    let scale = |key: &str| json["scales"][key].as_f64().unwrap_or(1.0) as f32;
    Avatar {
        rig: match json["playerAvatarType"].as_str() {
            Some("R6") => RigType::R6,
            _ => RigType::R15,
        },
        colors: std::array::from_fn(|i| brick(id(keys[i], DEFAULT_COLORS[i]))),
        scales: Scales {
            height: scale("height"),
            width: scale("width"),
            depth: scale("depth"),
            head: scale("head"),
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
    scale(dom, model, Scales::default(), avatar.scales)?;
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
    let scale = |name| dom.float_property(d, name).unwrap_or(1.0);
    avatar.scales = Scales {
        height: scale("HeightScale"),
        width: scale("WidthScale"),
        depth: scale("DepthScale"),
        head: scale("HeadScale"),
    };
    avatar
}

/// Resize an R15 body from scales `from` to `to` the classic way: each
/// part, its attachments and its ends of joints by width, height and
/// depth (the head by its own scale), and the Humanoid's HipHeight by
/// height, when the Humanoid scales automatically. Approximation: Roblox
/// also blends toward its Rthro proportions (`BodyTypeScale`,
/// `ProportionScale`) and keeps parts' original sizes to scale from.
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
        if dom.name(part) == "Head" {
            glam::Vec3::splat(to.head / from.head)
        } else {
            glam::Vec3::new(
                to.width / from.width,
                to.height / from.height,
                to.depth / from.depth,
            )
        }
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
    if let Some(hip) = dom.float_property(humanoid, "HipHeight") {
        let hip = hip * to.height / from.height;
        dom.set_property_by_name(humanoid, "HipHeight", Value::Float32(hip))?;
    }
    snap(dom, model)
}

/// Put each part where its joints hold it, from the root part out.
fn snap(dom: &mut Dom, model: InstanceId) -> Result<(), DomError> {
    let Some(root) = dom.find_first_child(model, "HumanoidRootPart", false) else {
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

//! Derived properties: values computed from other properties rather than
//! stored. `get` and `set` return `None` for properties stored normally.

use std::sync::LazyLock;

use rbx_types::VariantType;
use rook_dom::{DomError, InstanceId};
use rook_physics::assembly::{self, Assembly};
use rook_physics::joints;
use rook_reflection::{PropertyId, PropertyKind, PropertyType, Serialization};
use rook_types::{CFrame, Color3, Value, Vector3};

use crate::api::methods::characters::spawning;
use crate::types::color::closest_brick_color;

mod attachment;
mod text;
use crate::world::World;

/// A part's CFrame.
pub(crate) fn cframe(w: &World, id: InstanceId) -> CFrame {
    w.dom
        .cframe_property(id, "CFrame")
        .unwrap_or(CFrame::IDENTITY)
}

/// Set a part's CFrame as a script does: its assembly moves with it.
pub(crate) fn set_cframe(w: &mut World, id: InstanceId, c: CFrame) -> Result<(), DomError> {
    assembly::move_part(&mut w.dom, id, c)
}

/// Move one part on its own, as setting Position, Orientation or Rotation
/// does: its joints are rewritten to hold it where it now is (observed).
fn set_cframe_alone(w: &mut World, id: InstanceId, c: CFrame) -> Result<(), DomError> {
    let pid = w.dom.property_id(id, "CFrame").ok_or(DomError::Stale)?;
    w.dom
        .set_property(id, pid, Value::CFrame(c.orthonormalize()))?;
    joints::reseat(&mut w.dom, id)
}

fn degrees(v: (f32, f32, f32)) -> Vector3 {
    Vector3::new(v.0.to_degrees(), v.1.to_degrees(), v.2.to_degrees())
}

/// `Orientation`: degrees, applied in Y, X, Z order (Confirmed: creator-docs
/// BasePart.Orientation).
fn orientation(c: &CFrame) -> Vector3 {
    degrees(c.to_orientation())
}

/// `Rotation`: degrees, the XYZ Euler angles (Confirmed: creator-docs
/// BasePart.Rotation).
fn rotation(c: &CFrame) -> Vector3 {
    degrees(c.to_euler_angles_xyz())
}

/// Unserialized properties that read and write another property of the
/// same object, found by comparing the database's serialization data.
const SAME_AS: &[(&str, &str, &str)] = &[
    ("Humanoid", "Health", "Health_XML"),
    ("IntConstrainedValue", "Value", "ConstrainedValue"),
    ("DoubleConstrainedValue", "Value", "ConstrainedValue"),
    ("Sound", "Pitch", "PlaybackSpeed"),
    // Observed: the deprecated MaxDistance reads and sets
    // RollOffMaxDistance (EmitterSize is RollOffMinDistance's alias in the
    // database already).
    ("Sound", "MaxDistance", "RollOffMaxDistance"),
    ("GuiObject", "Transparency", "BackgroundTransparency"),
    ("GuiBase2d", "Localize", "AutoLocalize"),
    (
        "BallSocketConstraint",
        "MaxFrictionTorque",
        "MaxFrictionTorqueXml",
    ),
    // Read-only for scripts. Observed in Studio 0.742: a MeshPart saved
    // with InitialSize reads it as MeshSize, and one saved without reads
    // (0, 0, 0) even after its mesh loads.
    ("MeshPart", "MeshSize", "InitialSize"),
    // Unions too (observed: a small place's unions read their InitialSize).
    ("PartOperation", "MeshSize", "InitialSize"),
    // Files save a truss's Style as `style`, a property of its own in the
    // database (a large 2016 place's NoSupports trusses read back the default).
    ("TrussPart", "Style", "style"),
];

/// BrickColor views of Color3 properties: reads give the nearest
/// BrickColor, writes store the BrickColor's color.
const BRICK_COLOR_OF: &[(&str, &str, &str)] = &[
    ("BasePart", "BrickColor", "Color"),
    ("BodyColors", "HeadColor", "HeadColor3"),
    ("BodyColors", "LeftArmColor", "LeftArmColor3"),
    ("BodyColors", "LeftLegColor", "LeftLegColor3"),
    ("BodyColors", "RightArmColor", "RightArmColor3"),
    ("BodyColors", "RightLegColor", "RightLegColor3"),
    ("BodyColors", "TorsoColor", "TorsoColor3"),
    ("GuiObject", "BackgroundColor", "BackgroundColor3"),
    ("GuiObject", "BorderColor", "BorderColor3"),
    ("GuiBase3d", "Color", "Color3"),
];

/// The `Content` property a legacy string content property is a view of:
/// `MeshId` of `MeshContent`, `Texture` of `TextureContent`, `SoundId` of
/// `AudioContent` and so on. Files store only the `Content` one (the
/// database marks each legacy property as migrating to it), and in Studio
/// both read the same value. A table by property id, so the check costs
/// one lookup per read.
pub(crate) fn content_view(property: PropertyId) -> Option<PropertyId> {
    static VIEWS: LazyLock<Vec<Option<PropertyId>>> = LazyLock::new(|| {
        let db = rook_reflection::database();
        (0..db.property_count())
            .map(|i| {
                let p = db.property(PropertyId(i as u32));
                let PropertyKind::Canonical(Serialization::Migrate { to }) = &p.kind else {
                    return None;
                };
                let [target] = to.as_slice() else {
                    return None;
                };
                let target = db.find_canonical_property(p.owner, target)?;
                let content = PropertyType::Value(VariantType::Content);
                (p.ty == PropertyType::Value(VariantType::ContentId) && target.ty == content)
                    .then_some(target.id)
            })
            .collect()
    });
    VIEWS.get(property.0 as usize).copied().flatten()
}

/// What setting a legacy content property stores: backslashes in an
/// `rbxasset://` path become slashes (observed in Studio 0.742 for every
/// content property tried: `rbxasset://Textures\Face.PNG` reads back as
/// `rbxasset://Textures/Face.PNG`), and an empty string is no content.
pub(crate) fn content_value(url: &str) -> Value {
    if url.is_empty() {
        return Value::Content(rook_types::Content::none());
    }
    let url = match url.strip_prefix("rbxasset://") {
        Some(path) => format!("rbxasset://{}", path.replace('\\', "/")),
        None => url.to_owned(),
    };
    Value::Content(rook_types::Content::from_uri(url))
}

/// A TextLabel, TextButton or TextBox.
fn is_text_object(w: &World, id: InstanceId) -> bool {
    matches!(w.dom.class_name(id), "TextLabel" | "TextButton" | "TextBox")
}

fn view<'a>(
    w: &World,
    id: InstanceId,
    name: &str,
    table: &'a [(&str, &str, &str)],
) -> Option<&'a str> {
    table
        .iter()
        .find(|(class, prop, _)| *prop == name && w.dom.is_a(id, class))
        .map(|(_, _, target)| *target)
}

/// The stored property whose changes a property's changed signal hears: a
/// view's own name for the ones that read another property.
pub(crate) fn stored_name<'a>(w: &World, id: InstanceId, name: &'a str) -> &'a str {
    view(w, id, name, SAME_AS).unwrap_or(name)
}

/// The name `Changed` gives a stored property scripts can't see
/// (`Health_XML` changes as `Health`).
pub(crate) fn changed_name(w: &World, id: InstanceId, stored: &'static str) -> &'static str {
    SAME_AS
        .iter()
        .find(|(class, _, target)| *target == stored && w.dom.is_a(id, class))
        .filter(|_| {
            w.dom.property_id(id, stored).is_some_and(|p| {
                w.dom.database().property(p).scriptability == rook_reflection::Scriptability::None
            })
        })
        .map_or(stored, |(_, view, _)| *view)
}

fn set_named(w: &mut World, id: InstanceId, name: &str, value: Value) -> Result<(), DomError> {
    w.dom.set_property_by_name(id, name, value)
}

pub(crate) fn set_lighting_hours(
    w: &mut World,
    id: InstanceId,
    hours: f64,
) -> Result<(), DomError> {
    set_named(
        w,
        id,
        "TimeOfDay",
        Value::String(rook_types::time_of_day(hours)),
    )
}

#[expect(
    clippy::disallowed_methods,
    reason = "a property view returns another property's Value as it is"
)]
pub(crate) fn get(w: &World, id: InstanceId, name: &str) -> Option<Value> {
    let dom = &w.dom;
    if dom.is_a(id, "Attachment")
        && let Some(v) = attachment::get(w, id, name)
    {
        return Some(v);
    }
    if let Some(target) = view(w, id, name, SAME_AS) {
        return dom.property_by_name(id, target);
    }
    if is_text_object(w, id)
        && let Some(v) = text::get(w, id, name)
    {
        return Some(v);
    }
    if let Some(target) = view(w, id, name, BRICK_COLOR_OF) {
        return match dom.property_by_name(id, target) {
            Some(Value::Color3(c)) => Some(Value::BrickColor(closest_brick_color(c))),
            _ => None,
        };
    }
    match name {
        "ClockTime" if dom.is_a(id, "Lighting") => Some(Value::Float32(dom.clock_time(id) as f32)),
        // The top bar's free space, between Roblox's buttons (creator-docs
        // GuiService.TopbarInset). Approximation: all of it, until Rook
        // measures where the CoreScripts' buttons end; a server has none.
        "TopbarInset" if dom.class_name(id) == "GuiService" => {
            let height = if w.is_client {
                rook_gui::GUI_INSET
            } else {
                0.0
            };
            let width = dom
                .find_service("Workspace")
                .and_then(|ws| dom.ref_property(ws, "CurrentCamera"))
                .and_then(|c| dom.vector2_property(c, "ViewportSize"))
                .map_or(0.0, |v| v.0.x);
            Some(Value::Rect(rbx_types::Rect::new(
                rbx_types::Vector2::new(0.0, 0.0),
                rbx_types::Vector2::new(width, height),
            )))
        }
        // Measured: a view of ScreenInsets, true for all but
        // CoreUISafeInsets (files migrate it to ScreenInsets, and only
        // that replicates).
        "IgnoreGuiInset" if dom.class_name(id) == "ScreenGui" => Some(Value::Bool(
            dom.enum_item_property(id, "ScreenInsets") != Some("CoreUISafeInsets"),
        )),
        "Workspace" if id == dom.root() => Some(Value::Ref(
            dom.find_service("Workspace").map(InstanceId::to_ref),
        )),
        // The deprecated `game.lighting` has no capitalized twin.
        "lighting" if id == dom.root() => Some(Value::Ref(
            dom.find_service("Lighting").map(InstanceId::to_ref),
        )),
        // Observed: a copy's size, and smooth whatever was copied.
        "SizeInCells" if dom.is_a(id, "TerrainRegion") => {
            let (_, size) = super::methods::terrain_region_voxels(dom, id);
            Some(Value::Vector3(rook_types::Vector3::new(
                size.x as f32,
                size.y as f32,
                size.z as f32,
            )))
        }
        "IsSmooth" if dom.is_a(id, "TerrainRegion") => Some(Value::Bool(true)),
        "Terrain" if dom.is_a(id, "Workspace") => Some(Value::Ref(
            dom.find_first_child_of_class(id, "Terrain")
                .map(InstanceId::to_ref),
        )),
        // Observed: a part outside Workspace has no root, and an anchored
        // assembly's mass is infinite.
        // Observed in Studio 0.744: fixed, and not where rays start (see
        // `methods::camera`). The default camera's occlusion reads it.
        "NearPlaneZ" if dom.class_name(id) == "Camera" => Some(Value::Float32(-0.1)),
        "RootPart" | "Torso" if dom.class_name(id) == "Humanoid" => Some(Value::Ref(
            dom.parent(id)
                .and_then(|model| super::methods::characters::root_part(dom, model))
                .map(InstanceId::to_ref),
        )),
        "AssemblyRootPart" if dom.is_a(id, "BasePart") => {
            Some(Value::Ref(Assembly::of(dom, id).map(|a| a.root().to_ref())))
        }
        "AssemblyMass" if dom.is_a(id, "BasePart") => Some(Value::Float32(
            Assembly::of(dom, id).map_or_else(|| rook_physics::part_mass(dom, id), |a| a.mass(dom)),
        )),
        "Velocity" if dom.is_a(id, "BasePart") => Some(Value::Vector3(Vector3(
            rook_physics::assembly::part_velocity(dom, id),
        ))),
        "RotVelocity" if dom.is_a(id, "BasePart") => {
            dom.property_by_name(id, "AssemblyAngularVelocity")
        }
        "Position" | "Orientation" | "Rotation" if dom.is_a(id, "BasePart") => {
            let c = cframe(w, id);
            Some(Value::Vector3(match name {
                "Position" => Vector3(c.position),
                "Orientation" => orientation(&c),
                _ => rotation(&c),
            }))
        }
        // Measured: a view of the four corners' radii (files migrate it to
        // them), reading the top left's.
        "CornerRadius" if dom.class_name(id) == "UICorner" => {
            dom.property_by_name(id, "TopLeftRadius")
        }
        // Observed: IsPlaying reads Playing, IsPaused the opposite.
        "IsPlaying" | "IsPaused" if dom.is_a(id, "Sound") => {
            let playing = dom.bool_property(id, "Playing") == Some(true);
            Some(Value::Bool(playing == (name == "IsPlaying")))
        }
        // Observed: whether the wire carries sound now (crate::audio_api).
        "Connected" if dom.is_a(id, "Wire") => {
            let all = crate::audio_api::all_wires(dom);
            let connected = crate::audio_api::connections(dom, &all);
            Some(Value::Bool(connected.iter().any(|c| c.wire == id)))
        }
        "RequestQueueSize" if dom.is_a(id, "ContentProvider") => {
            Some(Value::Int32(w.content.queue_size() as i32))
        }
        "Enabled" if dom.is_a(id, "BaseScript") => {
            let disabled = dom.bool_property(id, "Disabled") == Some(true);
            Some(Value::Bool(!disabled))
        }
        "WorldPivot" if dom.is_a(id, "Model") => {
            if let Some(c) = dom.optional_cframe_property(id, "WorldPivotData") {
                return Some(Value::CFrame(c));
            }
            // Without stored pivot data, the pivot is the PrimaryPart's CFrame
            // (Confirmed: creator-docs PVInstance pivots). The bounding-box
            // fallback needs part geometry and arrives with physics.
            match dom.ref_property(id, "PrimaryPart") {
                Some(part) => Some(Value::CFrame(cframe(w, part))),
                None => Some(Value::CFrame(CFrame::IDENTITY)),
            }
        }
        _ => None,
    }
}

/// Reference properties that take one class only, with that class. Rook
/// checks just these: the database doesn't say which class the others
/// take.
const REF_CLASSES: &[(&str, &str, &str)] = &[("Sound", "SoundGroup", "SoundGroup")];

/// Studio's error for an instance of the wrong class given to one of
/// [`REF_CLASSES`] (observed for `Sound.SoundGroup`: "Expected SoundGroup
/// got Part for Sound.SoundGroup.").
pub(crate) fn wrong_ref_class(
    w: &World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<String> {
    let Value::Ref(Some(r)) = value else {
        return None;
    };
    let target = InstanceId::from_ref(*r);
    let &(owner, _, class) = REF_CLASSES
        .iter()
        .find(|(owner, p, _)| *p == name && w.dom.is_a(id, owner))?;
    (!w.dom.is_a(target, class)).then(|| {
        format!(
            "Expected {class} got {} for {owner}.{name}.",
            w.dom.class_name(target)
        )
    })
}

pub(crate) fn set(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    if w.dom.is_a(id, "Sound") {
        return set_sound(w, id, name, value);
    }
    if w.dom.is_a(id, "Attachment")
        && let Some(done) = attachment::set(w, id, name, value)
    {
        return Some(done);
    }
    if let Some(done) = crate::audio_api::clamp(w, id, name, value) {
        return Some(done);
    }
    if is_text_object(w, id)
        && let Some(done) = text::set(w, id, name, value)
    {
        return Some(done);
    }
    if let Some(target) = view(w, id, name, SAME_AS) {
        return Some(set_named(w, id, target, value.clone()));
    }
    if let (Some(target), Value::BrickColor(b)) = (view(w, id, name, BRICK_COLOR_OF), value) {
        return Some(set_named(w, id, target, Value::Color3(Color3::from(*b))));
    }
    match (name, value) {
        // Measured: sets all four corners.
        ("CornerRadius", v) if w.dom.class_name(id) == "UICorner" => Some(
            [
                "TopLeftRadius",
                "TopRightRadius",
                "BottomRightRadius",
                "BottomLeftRadius",
            ]
            .into_iter()
            .try_for_each(|corner| set_named(w, id, corner, v.clone())),
        ),
        // Observed in Studio 0.742: kept within 0 to 10, as Sound.Volume.
        ("Volume", v) if w.dom.is_a(id, "SoundGroup") => {
            let v = v.as_number()?;
            Some(set_named(
                w,
                id,
                "Volume",
                Value::Float32(v.clamp(0.0, 10.0) as f32),
            ))
        }
        ("ClockTime", Value::Float32(h)) if w.dom.is_a(id, "Lighting") => {
            Some(set_lighting_hours(w, id, f64::from(*h)))
        }
        // Measured: true sets ScreenInsets to DeviceSafeInsets, false to
        // CoreUISafeInsets.
        ("IgnoreGuiInset", Value::Bool(ignore)) if w.dom.class_name(id) == "ScreenGui" => {
            let item = if *ignore {
                "DeviceSafeInsets"
            } else {
                "CoreUISafeInsets"
            };
            let e = rook_reflection::database().enum_by_name("ScreenInsets")?;
            let value = rook_reflection::enum_item_value(e.id, item)?;
            Some(set_named(w, id, "ScreenInsets", Value::Enum(value)))
        }
        ("CFrame", Value::CFrame(c)) if w.dom.is_a(id, "BasePart") => Some(set_cframe(w, id, *c)),
        // Measured: kept within the canvas as laid out now.
        ("CanvasPosition", Value::Vector2(v)) if w.dom.class_name(id) == "ScrollingFrame" => {
            Some(w.gui.set_canvas_position(&mut w.dom, id, v.0))
        }
        // A truss keeps at most one axis other than 2 (observed; conformance
        // case `content/truss`).
        ("Size", Value::Vector3(v)) if w.dom.class_name(id) == "TrussPart" => {
            let pid = w.dom.property_id(id, "Size")?;
            let size = rook_types::truss_size(v.0);
            Some(w.dom.set_property(id, pid, Value::Vector3(Vector3(size))))
        }
        // Assembly velocities belong to the whole assembly; the deprecated
        // Velocity and RotVelocity set them too (observed). Files save
        // those two; rook_physics::adopt_saved_velocities moves them.
        (
            "AssemblyLinearVelocity" | "AssemblyAngularVelocity" | "Velocity" | "RotVelocity",
            Value::Vector3(v),
        ) if w.dom.is_a(id, "BasePart") => {
            let target = match name {
                "Velocity" | "AssemblyLinearVelocity" => "AssemblyLinearVelocity",
                _ => "AssemblyAngularVelocity",
            };
            Some(rook_physics::assembly::set_velocity(
                &mut w.dom, id, target, v.0,
            ))
        }
        ("Position", Value::Vector3(p)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            c.position = p.0;
            Some(set_cframe_alone(w, id, c))
        }
        ("Orientation", Value::Vector3(v)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            let r = |d: f32| d.to_radians();
            c.rotation = CFrame::from_orientation(r(v.x()), r(v.y()), r(v.z())).rotation;
            Some(set_cframe_alone(w, id, c))
        }
        ("Rotation", Value::Vector3(v)) if w.dom.is_a(id, "BasePart") => {
            let mut c = cframe(w, id);
            let r = |d: f32| d.to_radians();
            c.rotation = CFrame::angles(r(v.x()), r(v.y()), r(v.z())).rotation;
            Some(set_cframe_alone(w, id, c))
        }
        ("Enabled", Value::Bool(b)) if w.dom.is_a(id, "BaseScript") => {
            let pid = w.dom.property_id(id, "Disabled")?;
            Some(w.dom.set_property(id, pid, Value::Bool(!b)))
        }
        // A player's team, team color and neutrality follow each other
        // (creator-docs Player.Team, Player.TeamColor; measured: set to the
        // second team of a color, it joins the first).
        ("Team", Value::Ref(team)) if w.dom.is_a(id, "Player") => {
            let team = team.map(InstanceId::from_ref);
            let color = team.and_then(|t| w.dom.brick_color_property(t, "TeamColor"));
            let first = color.and_then(|c| spawning::team_of_color(&w.dom, c));
            Some(spawning::join_team(&mut w.dom, id, first.or(team)))
        }
        ("TeamColor", Value::BrickColor(b)) if w.dom.is_a(id, "Player") => {
            Some(spawning::take_color(&mut w.dom, id, *b))
        }
        ("Neutral", Value::Bool(b)) if w.dom.is_a(id, "Player") => {
            Some(spawning::set_neutral(&mut w.dom, id, *b))
        }
        ("WorldPivot", Value::CFrame(c)) if w.dom.is_a(id, "Model") => {
            let pid = w.dom.property_id(id, "WorldPivotData")?;
            Some(w.dom.set_property(id, pid, Value::OptionalCFrame(Some(*c))))
        }
        _ => None,
    }
}

/// Sound properties with rules of their own (observed in Studio 0.743):
/// `Volume` is kept within 0 to 10 and `PlaybackSpeed` (and `Pitch`) within
/// 0 to 20, the roll-off distances at 0 or more; a `TimePosition` a script
/// sets is where the next `Play` starts.
fn set_sound(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let n = value.as_number();
    match (name, n) {
        ("Volume", Some(v)) => Some(set_named(
            w,
            id,
            "Volume",
            Value::Float32(v.clamp(0.0, 10.0) as f32),
        )),
        ("PlaybackSpeed" | "Pitch", Some(v)) => Some(set_named(
            w,
            id,
            "PlaybackSpeed",
            Value::Float32(v.clamp(0.0, 20.0) as f32),
        )),
        // Observed: a negative RollOffMinDistance reads 0; a minimum past
        // the maximum is kept as it is. A negative maximum is taken to be
        // 0 the same way (Inference).
        ("RollOffMinDistance" | "EmitterSize" | "RollOffMaxDistance" | "MaxDistance", Some(v)) => {
            let target = if name.contains("Max") {
                "RollOffMaxDistance"
            } else {
                "RollOffMinDistance"
            };
            Some(set_named(w, id, target, Value::Float32(v.max(0.0) as f32)))
        }
        ("TimePosition", Some(t)) => {
            w.sounds.entry(id).or_default().start = t;
            Some(set_named(w, id, "TimePosition", Value::Float64(t)))
        }
        _ => view(w, id, name, SAME_AS).map(|target| set_named(w, id, target, value.clone())),
    }
}

//! Derived properties: values computed from other properties rather than
//! stored. `get` and `set` return `None` for properties stored normally.

use rook_dom::{DomError, InstanceId};
use rook_physics::assembly::Assembly;
use rook_physics::joints;
use rook_types::{CFrame, Color3, Value, Vector3};

use crate::api::methods::characters::spawning;
use crate::types::color::closest_brick_color;

mod attachment;
mod grip;
mod mouse;
mod part;
pub(crate) mod selection;
mod sound;
mod text;
mod views;

use crate::runtime::world::World;
pub(crate) use mouse::copy_icon;
pub(crate) use part::{cframe, part_cframe, part_ids, set_cframe};
use part::{orientation, rotation, saved_fidelity, set_cframe_alone};
use sound::set_sound;
use views::{BRICK_COLOR_OF, SAME_AS, is_text_object, view};
pub(crate) use views::{
    changed_name, content_target, content_value, content_view, stored_property, wrong_ref_class,
};

pub(super) fn set_named(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: Value,
) -> Result<(), DomError> {
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
    if dom.is_a(id, "Tool")
        && let Some(v) = grip::get(w, id, name)
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
        // GuiService.TopbarInset). Approximation: all of it; a server has
        // none.
        // TODO: measure where the CoreScripts' buttons end.
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
            let (_, size) = super::methods::terrain::region_voxels_of(dom, id);
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
        // Read-only, and not where rays start. The default camera's
        // occlusion reads it.
        "NearPlaneZ" if dom.class_name(id) == "Camera" => Some(Value::Float32(
            -super::methods::camera::rays::near_plane(w, id),
        )),
        "RootPart" | "Torso" if dom.class_name(id) == "Humanoid" => Some(Value::Ref(
            dom.parent(id)
                .and_then(|model| super::methods::characters::root_part(dom, model))
                .map(InstanceId::to_ref),
        )),
        "AssemblyRootPart" if dom.is_a(id, "BasePart") => {
            Some(Value::Ref(Assembly::of(dom, id).map(|a| a.root().to_ref())))
        }
        "AssemblyMass" if dom.is_a(id, "BasePart") => Some(Value::Float32(
            Assembly::of(dom, id)
                .map_or_else(|| w.physics.part_mass(dom, id), |a| a.mass(dom, &w.physics)),
        )),
        "AssemblyCenterOfMass" if dom.is_a(id, "BasePart") => {
            Some(Value::Vector3(Vector3(Assembly::of(dom, id).map_or_else(
                || cframe(w, id).position,
                |a| a.center_of_mass(dom, &w.physics),
            ))))
        }
        "CollisionFidelity" if dom.is_a(id, "TriangleMeshPart") => {
            Some(Value::Enum(saved_fidelity(dom, id)))
        }
        "Velocity" if dom.is_a(id, "BasePart") => Some(Value::Vector3(Vector3(
            rook_physics::assembly::part_velocity(dom, &w.physics, id),
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
            let all = crate::audio_api::all_wires(w);
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
    if w.dom.is_a(id, "Tool")
        && let Some(done) = grip::set(w, id, name, value)
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
        ("Part0" | "Part1" | "Enabled", v) if w.dom.class_name(id) == "WeldConstraint" => Some(
            set_named(w, id, name, v.clone()).and_then(|()| joints::capture_weld(&mut w.dom, id)),
        ),
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
        // A camera's roll lasts until its CameraType changes (observed).
        ("CameraType", v) if w.dom.class_name(id) == "Camera" => {
            let before = w.dom.enum_item_property(id, "CameraType");
            let done = set_named(w, id, "CameraType", v.clone());
            if w.dom.enum_item_property(id, "CameraType") != before {
                w.camera_rolls.remove(&id);
            }
            Some(done)
        }
        ("CFrame", Value::CFrame(c)) if w.dom.is_a(id, "BasePart") => Some(set_cframe(w, id, *c)),
        // Observed in Studio 0.747: a zero direction isn't taken, the old
        // one stays.
        ("LightDirection", Value::Vector3(v))
            if w.dom.class_name(id) == "ViewportFrame" && v.0 == glam::Vec3::ZERO =>
        {
            Some(Ok(()))
        }
        // Measured: kept within the canvas as laid out now.
        ("CanvasPosition", Value::Vector2(v)) if w.dom.class_name(id) == "ScrollingFrame" => {
            Some(w.gui.set_canvas_position(&mut w.dom, id, v.0))
        }
        // A truss keeps at most one axis other than 2 (observed; conformance
        // case `content/truss`).
        // Observed: a Size with a NaN or infinite number in it is ignored.
        ("Size", Value::Vector3(v)) if w.dom.is_a(id, "BasePart") && !v.0.is_finite() => {
            Some(Ok(()))
        }
        ("Size", Value::Vector3(v)) if w.dom.class_name(id) == "TrussPart" => {
            let pid = w.dom.property_id(id, "Size")?;
            let size = rook_types::truss_size(v.0);
            Some(w.dom.set_property(id, pid, Value::Vector3(Vector3(size))))
        }
        // Assembly velocities belong to the whole assembly; the deprecated
        // Velocity and RotVelocity set them too (observed). Files save
        // those two; loading moves them (rook_formats::into_dom).
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

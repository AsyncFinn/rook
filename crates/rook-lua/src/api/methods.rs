//! Native implementations of Roblox member functions.
//!
//! `native(owner, name)` maps a member (by the class that declares it in
//! the API dump) to its implementation. Members without one raise
//! "not implemented in Rook yet" and are counted. A short list of members
//! whose effect only matters to systems Rook doesn't have yet (network
//! ownership, preloading) are *inert*: they do nothing, return nil
//! and are counted separately, so scripts keep running.

use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use rook_dom::{DomError, InstanceId};
use rook_reflection::database;
use rook_scheduler::Queue;
use rook_signal::{SignalKey, SignalKind};
use rook_types::{CFrame, Value, Vector3};

use super::instance::push_instance;
use rook_physics::assembly::{self, Assembly};
use rook_physics::{joints, surfaces};

use super::properties::{cframe, set_cframe};
use super::signal::LSignal;
use crate::args::Args;
use crate::convert::{to_lua, vector3_to_lua};
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::threads::{dispatch, fire};
use crate::types::LCFrame;
use crate::world::{Job, World};

mod assemblies;
mod attributes;
mod audio;
mod bindables;
mod characters;
mod materials;
mod models;
mod services;
mod sound;
mod terrain;
mod tree;

use assemblies::*;
use attributes::*;
use bindables::*;
use characters::*;
use materials::*;
use models::*;
use services::*;
use tree::*;

pub(crate) use bindables::{get_callback, set_callback};
pub(crate) use models::pivot_of;
pub(crate) use terrain::region_voxels_of as terrain_region_voxels;
pub(crate) use tree::{Core, Found, fast};

pub(crate) type RawMethod = fn(&Lua, InstanceId, Args) -> ApiResult<Outcome>;

/// Members that are accepted and ignored for now (see module docs). Each
/// one is documented in `docs/behavior/stubs.md` with what will take it over;
/// a test enforces that.
const INERT: &[(&str, &str)] = &[
    ("BasePart", "SetNetworkOwner"),
    ("BasePart", "SetNetworkOwnershipAuto"),
    ("ContentProvider", "Preload"),
    // Pre-approves an asset for LoadAsset; only matters once assets load.
    ("InsertService", "ApproveAssetId"),
    ("InsertService", "ApproveAssetVersionId"),
    // FunctionalTest reports results to Roblox's internal test harness.
    ("FunctionalTest", "Passed"),
    ("FunctionalTest", "Failed"),
    ("FunctionalTest", "Error"),
    ("FunctionalTest", "Warn"),
];

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if INERT.contains(&(owner, name)) {
        return Some(inert);
    }
    Some(match (owner, name) {
        ("Object", "IsA") => is_a,
        ("Object", "GetPropertyChangedSignal") => property_changed_signal,
        ("Instance", "FindFirstChild") => find_first_child,
        ("Instance", "FindFirstChildOfClass") => find_first_child_of_class,
        ("Instance", "FindFirstChildWhichIsA") => find_first_child_which_is_a,
        ("Instance", "FindFirstAncestor") => find_first_ancestor,
        ("Instance", "FindFirstAncestorOfClass") => find_first_ancestor_of_class,
        ("Instance", "FindFirstAncestorWhichIsA") => find_first_ancestor_which_is_a,
        ("Instance", "FindFirstDescendant") => find_first_descendant,
        ("Instance", "GetChildren" | "children") => get_children,
        ("Instance", "GetDescendants") => get_descendants,
        ("Instance", "IsDescendantOf") => is_descendant_of,
        ("Instance", "IsAncestorOf") => is_ancestor_of,
        ("Instance", "Destroy") => destroy,
        ("Instance", "Remove") => remove,
        ("Instance", "ClearAllChildren") => clear_all_children,
        ("Instance", "Clone") => clone,
        ("Instance", "GetFullName") => get_full_name,
        ("Instance", "GetAttribute") => get_attribute,
        ("Instance", "SetAttribute") => set_attribute,
        ("Instance", "GetAttributes") => get_attributes,
        ("Instance", "GetAttributeChangedSignal") => attribute_changed_signal,
        ("Instance", "AddTag") => add_tag,
        ("Instance", "RemoveTag") => remove_tag,
        ("Instance", "HasTag") => has_tag,
        ("Instance", "GetTags") => get_tags,
        ("Instance", "GetActor") => nil,
        ("ServiceProvider", "GetService" | "service") => get_service,
        ("ServiceProvider", "FindService") => find_service,
        ("DataModel", "IsLoaded") => true_,
        ("DataModel", "BindToClose") => bind_to_close,
        ("Debris", "AddItem" | "addItem") => debris_add_item,
        ("MaterialService", "GetBaseMaterialOverride") => get_base_material_override,
        ("MaterialService", "SetBaseMaterialOverride") => set_base_material_override,
        ("MaterialService", "GetMaterialVariant") => get_material_variant,
        ("RunService", "IsRunning") => true_,
        ("RunService", "IsStudio" | "IsRunMode") => false_,
        ("RunService", "IsServer") => is_server,
        ("RunService", "IsClient") => is_client,
        ("BindableEvent", "Fire") => bindable_fire,
        ("BindableFunction", "Invoke") => bindable_invoke,
        ("Players", "GetPlayers" | "getPlayers" | "players") => get_players,
        ("Players", "GetPlayerFromCharacter" | "playerFromCharacter") => player_from_character,
        ("Players", "GetPlayerByUserId") => player_by_user_id,
        ("Humanoid", "TakeDamage") => take_damage,
        ("PVInstance", "GetPivot") => get_pivot,
        ("PVInstance", "PivotTo") => pivot_to,
        ("Model", "GetPrimaryPartCFrame") => get_primary_part_cframe,
        ("Model", "SetPrimaryPartCFrame") => set_primary_part_cframe,
        ("Model", "GetModelCFrame") => get_model_cframe,
        ("Model", "TranslateBy") => translate_by,
        ("Model", "MoveTo" | "move" | "moveTo") => model_move_to,
        ("Model", "GetExtentsSize" | "GetModelSize") => get_extents_size,
        ("Model", "GetBoundingBox") => get_bounding_box,
        ("BasePart", "GetMass" | "getMass") => get_mass,
        ("BasePart", "GetJoints") => get_joints,
        ("BasePart", "GetConnectedParts") => get_connected_parts,
        ("BasePart", "GetRootPart") => get_root_part,
        ("BasePart", "IsGrounded") => is_grounded,
        ("BasePart", "MakeJoints" | "makeJoints") => part_make_joints,
        ("BasePart", "BreakJoints" | "breakJoints") => part_break_joints,
        ("Model", "MakeJoints" | "makeJoints") => model_make_joints,
        ("Model", "BreakJoints" | "breakJoints") => model_break_joints,
        ("Workspace", "MakeJoints") => workspace_make_joints,
        ("Workspace", "BreakJoints") => workspace_break_joints,
        ("Motor", "SetDesiredAngle") => set_desired_angle,
        ("Lighting", "GetMinutesAfterMidnight") => get_minutes_after_midnight,
        ("Lighting", "SetMinutesAfterMidnight") => set_minutes_after_midnight,
        ("Sound", "Play") => sound::sound_play,
        ("Sound", "Resume") => sound::sound_resume,
        ("Sound", "Stop") => sound::sound_stop,
        ("Sound", "Pause") => sound::sound_pause,
        ("SoundService", "SetListener") => sound::set_listener,
        ("SoundService", "GetListener") => sound::get_listener,
        ("SoundService", "PlayLocalSound") => sound::play_local_sound,
        ("SoundService", "GetMixerTime") => audio::get_mixer_time,
        ("AudioPlayer", "Play") => audio::audio_play,
        ("AudioPlayer", "Stop") => audio::audio_stop,
        ("AudioPlayer", "Cancel") => audio::audio_cancel,
        (c, "GetInputPins") if crate::audio::pins(c).is_some() => audio::get_input_pins,
        (c, "GetOutputPins") if crate::audio::pins(c).is_some() => audio::get_output_pins,
        (c, "GetConnectedWires") if crate::audio::pins(c).is_some() => audio::get_connected_wires,
        ("AudioEmitter" | "AudioListener", "SetDistanceAttenuation") => {
            audio::set_distance_attenuation
        }
        ("AudioEmitter" | "AudioListener", "GetDistanceAttenuation") => {
            audio::get_distance_attenuation
        }
        ("AudioEmitter" | "AudioListener", "SetAngleAttenuation") => audio::set_angle_attenuation,
        ("AudioEmitter" | "AudioListener", "GetAngleAttenuation") => audio::get_angle_attenuation,
        ("AudioEmitter" | "AudioListener", "GetAudibilityFor") => audio::get_audibility_for,
        ("AudioEmitter", "GetInteractingListeners") => audio::get_interacting_listeners,
        ("AudioListener", "GetInteractingEmitters") => audio::get_interacting_emitters,
        ("AudioAnalyzer", "GetSpectrum") => audio::get_spectrum,
        ("Terrain", "FillBlock") => terrain::fill_block,
        ("Terrain", "FillBall") => terrain::fill_ball,
        ("Terrain", "FillCylinder") => terrain::fill_cylinder,
        ("Terrain", "FillWedge") => terrain::fill_wedge,
        ("Terrain", "FillRegion") => terrain::fill_region,
        ("Terrain", "ReplaceMaterial") => terrain::replace_material,
        ("Terrain", "ReadVoxels") => terrain::read_voxels,
        ("Terrain", "WriteVoxels") => terrain::write_voxels,
        ("Terrain", "Clear") => terrain::terrain_clear,
        ("Terrain", "CountCells") => terrain::count_cells,
        ("Terrain", "WorldToCell") => terrain::world_to_cell,
        ("Terrain", "WorldToCellPreferSolid") => terrain::world_to_cell_prefer_solid,
        ("Terrain", "WorldToCellPreferEmpty") => terrain::world_to_cell_prefer_empty,
        ("Terrain", "CellCenterToWorld") => terrain::cell_center_to_world,
        ("Terrain", "CellCornerToWorld") => terrain::cell_corner_to_world,
        ("Terrain", "GetCell") => terrain::get_cell,
        ("Terrain", "SetCell") => terrain::set_cell,
        ("Terrain", "SetCells") => terrain::set_cells_region,
        ("Terrain", "AutowedgeCell") => terrain::autowedge_cell,
        ("Terrain", "AutowedgeCells") => terrain::autowedge_cells,
        ("Terrain", "SetWaterCell") => terrain::set_water_cell,
        ("Terrain", "GetWaterCell") => terrain::get_water_cell,
        ("Terrain", "CopyRegion") => terrain::copy_region,
        ("Terrain", "PasteRegion") => terrain::paste_region,
        ("Terrain", "GetMaterialColor") => terrain::get_material_color,
        ("Terrain", "SetMaterialColor") => terrain::set_material_color,
        _ => return super::services::native(owner, name),
    })
}

/// Methods written in Luau because they yield in ways a single Rust call
/// can't express. Each source evaluates to the method function; it gets
/// the standard library functions it uses as chunk arguments, so scripts
/// replacing globals can't break it.
pub(crate) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    match (owner, name) {
        // Confirmed: creator-docs Instance:WaitForChild (returns nil after
        // timeOut; without one, warns about a possible infinite yield after
        // 5 seconds). Warning text is Inference.
        ("Instance", "WaitForChild") => Some(include_str!("luau/wait_for_child.luau")),
        _ => super::services::luau_source(owner, name),
    }
}

/// Run a DOM mutation and fire the signals it caused.
pub(crate) fn mutate<R>(
    lua: &Lua,
    f: impl FnOnce(&mut World) -> Result<R, DomError>,
) -> ApiResult<R> {
    let start = with_world(lua, |w| w.dom.journal().end());
    let result = with_world(lua, f);
    dispatch(lua, start);
    result.map_err(ScriptError::from)
}

fn ret_instance(lua: &Lua, id: Option<InstanceId>) -> ApiResult<Outcome> {
    match id {
        Some(id) => Outcome::ret(lua, push_instance(lua, id)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

fn ret_instances(lua: &Lua, ids: Vec<InstanceId>) -> ApiResult<Outcome> {
    super::instance::ret_instance_array(lua, &ids)
}

fn inert(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, ())
}

fn nil(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LuaValue::Nil)
}

fn true_(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, true)
}

fn false_(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, false)
}

fn is_server(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, !with_world(lua, |w| w.is_client))
}

fn is_client(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, with_world(lua, |w| w.is_client))
}

#[cfg(test)]
pub(crate) fn inert_members() -> &'static [(&'static str, &'static str)] {
    INERT
}

/// Whether a member is inert, for reports.
pub(crate) fn is_inert(owner: &str, name: &str) -> bool {
    INERT.contains(&(owner, name))
}

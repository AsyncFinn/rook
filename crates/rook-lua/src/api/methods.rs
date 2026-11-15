//! Roblox member functions, one folder or file per topic in `methods/`.
//!
//! Each topic's `native(owner, name)` maps a member (by the class that
//! declares it in the API dump) to its implementation, and `luau_source`
//! does the same for members written in Luau. Members without one raise
//! "not implemented in Rook yet" and are counted. *Inert* members, whose
//! effect only matters to systems Rook doesn't have, do nothing, return
//! nil and are counted separately, so scripts keep running.

use crate::scheduler::Queue;
use crate::signal::{SignalKey, SignalKind};
use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use rook_dom::{DomError, InstanceId};
use rook_reflection::database;
use rook_types::{CFrame, Value, Vector3};

use super::instance::{push_instance, settle_joints, settle_joints_within};
use rook_physics::assembly::{self, Assembly};
use rook_physics::joints;

use super::properties::{cframe, part_cframe, part_ids, set_cframe};
use super::signal::LSignal;
use crate::ctx::args::Args;
use crate::ctx::convert::{to_lua, vector3_to_lua};
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::threads::{dispatch, fire, fire_with};
use crate::types::LCFrame;
use crate::world::{Job, World};

pub(crate) mod audio;
pub(crate) mod camera;
pub(crate) mod characters;
pub(crate) mod content;
mod context_actions;
pub(crate) mod core;
mod effects;
pub(crate) mod gui;
mod input;
pub(crate) mod instances;
pub(crate) mod localization;
mod materials;
pub(crate) mod physics;
mod player_scripts;
pub(crate) mod prompts;
pub(crate) mod remotes;
pub(crate) mod terrain;
pub(crate) mod tools;
mod tweens;
mod user_settings;
pub(crate) mod web;

pub(crate) type RawMethod = fn(&Lua, InstanceId, &Args) -> ApiResult<Outcome>;

/// Members that are accepted and ignored for now (see module docs). Each
/// one is documented in `docs/guide/stubs.md` with what will take it over;
/// a test enforces that.
const INERT: &[(&str, &str)] = &[
    ("BasePart", "SetNetworkOwner"),
    ("BasePart", "SetNetworkOwnershipAuto"),
    ("ContentProvider", "Preload"),
    // Pre-approves an asset for LoadAsset, which doesn't check approvals.
    ("InsertService", "ApproveAssetId"),
    ("InsertService", "ApproveAssetVersionId"),
    // FunctionalTest reports results to Roblox's internal test harness.
    ("FunctionalTest", "Passed"),
    ("FunctionalTest", "Failed"),
    ("FunctionalTest", "Error"),
    ("FunctionalTest", "Warn"),
    // Rook draws no default loading screen, so there is none to remove.
    ("ReplicatedFirst", "RemoveDefaultLoadingScreen"),
    // The legacy camera's pan mode: nothing a script sees changes.
    ("Camera", "SetCameraPanMode"),
];

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if INERT.contains(&(owner, name)) {
        return Some(inert);
    }
    first(
        &[
            instances::native,
            physics::native,
            characters::native,
            tools::native,
            camera::native,
            gui::native,
            prompts::native,
            audio::native,
            effects::native,
            materials::native,
            terrain::native,
            input::native,
            player_scripts::native,
            user_settings::native,
            remotes::native,
            content::native,
            web::native,
            localization::native,
            core::native,
        ],
        owner,
        name,
    )
}

/// Methods written in Luau because they yield in ways a single Rust call
/// can't express. Each source evaluates to the method function; it gets
/// the standard library functions it uses as chunk arguments, so scripts
/// replacing globals can't break it.
pub(crate) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    first(
        &[
            instances::luau_source,
            tweens::luau_source,
            context_actions::luau_source,
            core::luau_source,
            gui::luau_source,
            remotes::luau_source,
            web::luau_source,
        ],
        owner,
        name,
    )
}

/// The first of a topic's tables that has the member: each member is in
/// one table, so the order only decides how soon it's found.
fn first<T>(tables: &[fn(&str, &str) -> Option<T>], owner: &str, name: &str) -> Option<T> {
    tables.iter().find_map(|table| table(owner, name))
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

pub(crate) fn ret_instance(lua: &Lua, id: Option<InstanceId>) -> ApiResult<Outcome> {
    match id {
        Some(id) => Outcome::ret(lua, push_instance(lua, id)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

pub(crate) fn ret_instances(lua: &Lua, ids: &[InstanceId]) -> ApiResult<Outcome> {
    super::instance::ret_instance_array(lua, ids)
}

fn inert(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, ())
}

fn nil(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LuaValue::Nil)
}

fn true_(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, true)
}

fn false_(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, false)
}

#[cfg(test)]
pub(crate) fn inert_members() -> &'static [(&'static str, &'static str)] {
    INERT
}

/// Whether a member is inert, for reports.
pub(crate) fn is_inert(owner: &str, name: &str) -> bool {
    INERT.contains(&(owner, name))
}

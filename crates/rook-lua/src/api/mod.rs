//! The Roblox API surface: Instance, signals, globals, scripts.

pub mod camera;
pub(crate) mod globals;
pub(crate) mod input;
pub(crate) mod instance;
pub(crate) mod methods;
pub(crate) mod physics;
pub(crate) mod properties;
pub(crate) mod queries;
pub(crate) mod remotes;
pub(crate) mod scripts;
pub(crate) mod services;
pub(crate) mod signal;

use mlua::Lua;

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    instance::install(lua)?;
    signal::install(lua)?;
    globals::install(lua)?;
    scripts::install(lua)?;
    remotes::install(lua)?;
    Ok(())
}

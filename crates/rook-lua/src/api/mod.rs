//! The Roblox API surface: Instance, signals, globals, scripts.

pub(crate) mod globals;
pub(crate) mod instance;
pub(crate) mod methods;
pub(crate) mod properties;
pub(crate) mod scripts;
pub(crate) mod services;
pub(crate) mod signal;

use mlua::Lua;

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    instance::install(lua)?;
    signal::install(lua)?;
    globals::install(lua)?;
    scripts::install(lua)?;
    Ok(())
}

//! The Roblox API surface: Instance, signals, globals, scripts.

pub(crate) mod globals;
pub(crate) mod instance;
pub(crate) mod methods;
pub(crate) mod properties;
pub(crate) mod scripts;
pub(crate) mod signal;

pub(crate) use methods::pivot_of;

use mlua::Lua;

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    instance::install(lua)?;
    signal::install(lua)?;
    globals::install(lua)?;
    scripts::install(lua)?;
    methods::assets::install(lua)?;
    methods::web::install(lua)?;
    methods::http::install(lua)?;
    methods::remotes::install(lua)?;
    methods::text_chat::install(lua)?;
    Ok(())
}

//! `Vector3` is Luau's native `vector` (as on Roblox, where
//! `type(Vector3.new())` is `"vector"`), with a metatable for its members.
//! The rest are userdata. Member lists and constructors follow the
//! creator-docs datatype pages.

pub(crate) mod color;
pub(crate) mod content;
pub(crate) mod datetime;
pub(crate) mod enums;
pub(crate) mod format;
pub(crate) mod gui;
pub(crate) mod members;
pub(crate) mod opaque;
pub(crate) mod random;
pub(crate) mod sequences;
pub(crate) mod space;
pub(crate) mod tween_info;

pub(crate) use color::{LBrickColor, LColor3};
pub(crate) use content::LContent;
pub(crate) use format::{fmt_f32, fmt_g};
pub(crate) use gui::font::LFont;
pub(crate) use gui::udim::{LUDim, LUDim2};
pub(crate) use opaque::{LAxes, LFaces, LOther, LPhysicalProperties};
pub(crate) use sequences::{LColorSequence, LNumberRange, LNumberSequence};
pub(crate) use space::cframe::LCFrame;
pub(crate) use space::geometry::{LRay, LRect, LRegion3, LRegion3int16};
pub(crate) use space::vector::{LVector2, LVector2int16, LVector3int16};

use mlua::Lua;

/// An error raised to scripts with Roblox's message.
pub(crate) fn err(msg: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(msg.into())
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    space::vector::install(lua)?;
    space::cframe::install(lua)?;
    color::install(lua)?;
    gui::udim::install(lua)?;
    sequences::install(lua)?;
    space::geometry::install(lua)?;
    enums::install(lua)?;
    gui::font::install(lua)?;
    content::install(lua)?;
    opaque::install(lua)?;
    tween_info::install(lua)?;
    random::install(lua)?;
    space::query::install(lua)?;
    datetime::install(lua)?;
    Ok(())
}

#[cfg(test)]
mod tests;

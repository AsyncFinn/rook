//! Roblox datatypes as Lua values.
//!
//! `Vector3` is Luau's native `vector` (as on Roblox, where
//! `type(Vector3.new())` is `"vector"`), with a metatable for its members.
//! The rest are userdata. Each module installs its constructor table into
//! the globals.
//!
//! Member lists and constructors follow the creator-docs datatype pages.
//! Known gap: errors raised by datatype methods reach `pcall` as mlua
//! error objects rather than plain strings; Instance members don't have
//! this problem (see `ctx.rs`).

pub(crate) mod cframe;
pub(crate) mod color;
pub(crate) mod enums;
pub(crate) mod geometry;
pub(crate) mod misc;
pub(crate) mod query;
pub(crate) mod sequences;
pub(crate) mod udim;
pub(crate) mod vector;

pub(crate) use cframe::LCFrame;
pub(crate) use color::{LBrickColor, LColor3};
pub(crate) use geometry::{LRay, LRect, LRegion3};
pub(crate) use misc::Opaque;
pub(crate) use sequences::{LColorSequence, LNumberRange, LNumberSequence};
pub(crate) use udim::{LUDim, LUDim2};
pub(crate) use vector::{LVector2, LVector2int16, LVector3int16};

use mlua::Lua;

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    vector::install(lua)?;
    cframe::install(lua)?;
    color::install(lua)?;
    udim::install(lua)?;
    sequences::install(lua)?;
    geometry::install(lua)?;
    enums::install(lua)?;
    misc::install(lua)?;
    query::install(lua)?;
    Ok(())
}

/// Format like C's `%.9g`, which is how Roblox prints datatype components
/// (32-bit floats shown with 9 significant digits).
pub(crate) fn fmt_g(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let exp = x.abs().log10().floor() as i32;
    let precision = precision.max(1);
    let p = precision as i32;
    if exp < -4 || exp >= p {
        let s = format!("{:.*e}", precision - 1, x);
        let (mantissa, e) = s.split_once('e').expect("exponent form");
        let mantissa = strip_zeros(mantissa);
        let e: i32 = e.parse().expect("exponent");
        let sign = if e < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", e.abs())
    } else {
        let decimals = (p - 1 - exp).max(0) as usize;
        strip_zeros(&format!("{x:.decimals$}")).to_owned()
    }
}

fn strip_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

pub(crate) fn fmt_f32(x: f32) -> String {
    fmt_g(f64::from(x), 9)
}

#[cfg(test)]
mod tests {
    use super::fmt_g;

    #[test]
    fn formats_like_printf_g() {
        assert_eq!(fmt_g(1.0, 9), "1");
        assert_eq!(fmt_g(0.1f32 as f64, 9), "0.100000001");
        assert_eq!(fmt_g(-2.5, 9), "-2.5");
        assert_eq!(fmt_g(1e10, 9), "1e+10");
        assert_eq!(fmt_g(0.00001, 9), "1e-05");
        assert_eq!(fmt_g(123456789.0, 9), "123456789");
    }
}

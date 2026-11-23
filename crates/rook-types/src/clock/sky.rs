//! Where the clock puts the sun and the moon (`Lighting:GetSunDirection`,
//! `GetMoonDirection`, `GetMoonPhase`), which the renderer lights by too.

use std::f32::consts::{PI, TAU};

use glam::Vec3;

use super::Clock;

/// Direction to the sun at `clock` and `latitude` in degrees. Roblox turns
/// the sun around the Z axis once a day, rising at +X, on a circle tilted
/// towards +Z by the latitude less 23.5°, any latitude taken as an angle
/// as it is (Confirmed: conformance case `lighting/clock`, eight times at
/// latitudes 0, 41.733 and -30, and latitudes -400 to 1e9 at 14:00). In
/// 32-bit floats, the tilt as (latitude − 23.5) × π / 180 in that order:
/// at latitude 1e9 only that rounding gives Studio's sun. NaN and
/// infinite latitudes give NaN.
pub fn sun_direction(clock: Clock, latitude: f32) -> Vec3 {
    let h = (clock.hours() as f32 - 12.0) / 24.0 * TAU;
    let tilt = (latitude - 23.5) * PI / 180.0;
    Vec3::new(-h.sin() * tilt.cos(), h.cos() * tilt.cos(), tilt.sin())
}

/// The moon for a sun direction: opposite the sun's daily circle, the sun
/// turned half a day (Confirmed: `GetMoonDirection` at every time and
/// latitude of `lighting/clock`).
pub fn moon_direction(sun: Vec3) -> Vec3 {
    Vec3::new(-sun.x, -sun.y, sun.z)
}

/// `Lighting:GetMoonPhase()`. Studio 0.748 read 0.75 at midnight and at
/// noon (Confirmed: `lighting/clock`); Lighting has no date for it to
/// follow, so it's taken to be fixed (Inference).
pub const MOON_PHASE: f64 = 0.75;

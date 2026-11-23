use super::*;

/// The sun's light in Studio's light units on a white face square to it,
/// per unit of `Brightness`, for its height `y` (the sine of its
/// elevation): 0.5 by day (measured: white and black SmoothPlastic at six
/// angles to the sun, Brightness 0.5, 1 and 3, all within 0.02), turning
/// yellow and dimming in the last few degrees above the horizon (measured
/// at five heights: (0.40, 0.40, 0) at y 0.024, (0.70, 0.70, 0.03) at
/// 0.048, (0.91, 0.92, 0.51) at 0.096, white from about 0.15; fitted,
/// Approximation). docs/behavior/lighting/sun.md.
fn sun_color(y: f32) -> [f32; 3] {
    let warm = 1.0 - (-y.max(0.0) / 0.04).exp();
    let t = ((y - 0.045) / 0.105).clamp(0.0, 1.0);
    let blue = t * t * (3.0 - 2.0 * t);
    [0.5 * warm, 0.5 * warm, 0.5 * blue]
}

/// The moon's light per unit of `Brightness` (measured once, at 66° up:
/// 0.031 units on white; Approximation elsewhere, faded in as the sun's).
const MOON_LIGHT: f32 = 0.031;

/// The direction light comes from and its color in light units: the sun
/// while it's up, else the moon (`Lighting:GetMoonDirection()`).
pub(super) fn sun_and_moon(sun: Vec3, brightness: f32) -> (Vec3, [f32; 3]) {
    if sun.y > 0.0 {
        return (sun, sun_color(sun.y).map(|c| c * brightness));
    }
    let moon = rook_types::moon_direction(sun);
    let fade = 1.0 - (-moon.y / 0.04).exp();
    (moon, [MOON_LIGHT * fade * brightness; 3])
}

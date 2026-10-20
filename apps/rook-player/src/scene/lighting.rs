//! Lighting, the sun and the sky; local lights in [`local`].

mod local;

use super::*;

/// Direction to the sun for a clock time and latitude. Roblox turns the sun
/// around the Z axis once a day, rising at +X, on a circle tilted towards
/// +Z by the latitude less 23.5° (Confirmed: `Lighting:GetSunDirection()`
/// in Studio 0.742 for latitudes 0, 41.733 and -30, 2026-10-13).
fn sun_direction(hours: f32, latitude_degrees: f32) -> Vec3 {
    let h = (hours - 12.0) / 24.0 * std::f32::consts::TAU;
    let tilt = (latitude_degrees - 23.5).to_radians();
    Vec3::new(-h.sin() * tilt.cos(), h.cos() * tilt.cos(), tilt.sin())
}

/// The sun's light in Studio's light units on a white face square to it,
/// per unit of `Brightness`, for its height `y` (the sine of its
/// elevation): 0.5 by day (measured: white and black SmoothPlastic at six
/// angles to the sun, Brightness 0.5, 1 and 3, all within 0.02), turning
/// yellow and dimming in the last few degrees above the horizon (measured
/// at five heights: (0.40, 0.40, 0) at y 0.024, (0.70, 0.70, 0.03) at
/// 0.048, (0.91, 0.92, 0.51) at 0.096, white from about 0.15; fitted,
/// Approximation). docs/behavior/lights.md, "The sun".
fn sun_color(y: f32) -> [f32; 3] {
    let warm = 1.0 - (-y.max(0.0) / 0.04).exp();
    let t = ((y - 0.045) / 0.105).clamp(0.0, 1.0);
    let blue = t * t * (3.0 - 2.0 * t);
    [0.5 * warm, 0.5 * warm, 0.5 * blue]
}

/// The moon's light per unit of `Brightness` (measured once, at 66° up:
/// 0.031 units on white; Approximation elsewhere, faded in as the sun's).
const MOON_LIGHT: f32 = 0.031;

/// The direction light comes from and its colour in light units: the sun
/// while it's up, else the moon, which Roblox puts opposite the sun's
/// daily circle (Confirmed: `Lighting:GetMoonDirection()` in Studio 0.743
/// at midnight, latitude 0, is the noon sun's direction).
fn sun_and_moon(sun: Vec3, brightness: f32) -> (Vec3, [f32; 3]) {
    if sun.y > 0.0 {
        return (sun, sun_color(sun.y).map(|c| c * brightness));
    }
    let moon = Vec3::new(-sun.x, -sun.y, sun.z);
    let fade = 1.0 - (-moon.y / 0.04).exp();
    (moon, [MOON_LIGHT * fade * brightness; 3])
}

/// The Lighting service's look, and how bright the sky is
/// ([`daylight`]).
pub(super) fn lighting(dom: &Dom) -> (Lighting, f32) {
    let mut l = Lighting::default();
    let Some(id) = dom.find_service("Lighting") else {
        return (l, 1.0);
    };
    let hours = dom.clock_time(id) as f32;
    let latitude = dom
        .float_property(id, "GeographicLatitude")
        .unwrap_or(41.733);
    let sun = sun_direction(hours, latitude);
    let brightness = dom.float_property(id, "Brightness").unwrap_or(2.0);
    (l.sun_direction, l.sun_color) = sun_and_moon(sun, brightness);
    if let Some(c) = dom.color3_property(id, "Ambient").map(Color3::to_array) {
        l.ambient = c;
    }
    if let Some(c) = dom
        .color3_property(id, "OutdoorAmbient")
        .map(Color3::to_array)
    {
        l.outdoor_ambient = c;
    }
    if let Some(c) = dom.color3_property(id, "FogColor").map(Color3::to_array) {
        l.fog_color = c;
    }
    if let Some(s) = dom.float_property(id, "FogStart") {
        l.fog_start = s;
    }
    if let Some(e) = dom.float_property(id, "FogEnd") {
        l.fog_end = e;
    }
    // Off draws no shadows (Confirmed: creator-docs Lighting.GlobalShadows;
    // defaults to on).
    if let Some(s) = dom.bool_property(id, "GlobalShadows") {
        l.global_shadows = s;
    }
    // Observed in Studio 0.742: a place saved without it reads 0, and at 0
    // nothing reflects the sky (docs/behavior/materials.md).
    l.environment_specular = dom
        .float_property(id, "EnvironmentSpecularScale")
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    // The sky lights surfaces as a second ambient, added to Ambient before
    // the tone curve (observed in Studio 0.742; docs/behavior/materials.md,
    // "The sky's diffuse light"). Read as 0 when unsaved, like
    // EnvironmentSpecularScale, so archived places are lit as before.
    l.environment_diffuse = dom
        .float_property(id, "EnvironmentDiffuseScale")
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    // Scales all light, the sky's too, about as a multiplier on linear
    // light: ExposureCompensation -0.6 took white SmoothPlastic under
    // Ambient alone from 179 to 148 and the sky from (85, 163, 202) to
    // (61, 129, 174) (observed in Studio 0.742; Approximation, as Studio
    // scales before its tone curve).
    l.exposure = dom
        .float_property(id, "ExposureCompensation")
        .unwrap_or(0.0)
        .exp2();
    l.tonemapper = tonemapper(dom, id);
    let day = daylight(sun);
    l.sky_top = l.sky_top.map(|c| c * day);
    l.sky_horizon = l.sky_horizon.map(|c| c * day);
    l.sky_bottom = l.sky_horizon;
    l.reflection = l.sky_horizon;
    (l, day)
}

/// The tone curve of the first enabled ColorGradingEffect in Lighting
/// (`TonemapperPreset`: Default 0, Retro 1); without one, Studio's default
/// curve (measured: the Ambient ramp of places without one follows it;
/// docs/behavior/tone-mapping.md).
fn tonemapper(dom: &Dom, lighting: InstanceId) -> rook_render::Tonemapper {
    let grading = dom.children(lighting).iter().copied().find(|&c| {
        dom.class_name(c) == "ColorGradingEffect" && dom.bool_property(c, "Enabled").unwrap_or(true)
    });
    match grading.and_then(|c| dom.enum_property(c, "TonemapperPreset")) {
        Some(1) => rook_render::Tonemapper::Retro,
        _ => rook_render::Tonemapper::Default,
    }
}

/// How bright the sky is for the sun's height: a night sky darkens towards
/// black as the sun sets (Approximation; Studio's sky draws black at
/// midnight and then lights nothing).
fn daylight(sun_direction: Vec3) -> f32 {
    (sun_direction.y * 4.0 + 0.5).clamp(0.05, 1.0)
}

/// The skybox images of a Sky, in the order Up, Back, Front, Left, Right,
/// Down (`rook_render::SKYBOX_FACES`): (`Content` property, legacy name).
pub(super) const SKYBOX: [(&str, &str); 6] = [
    ("SkyboxUpContent", "SkyboxUp"),
    ("SkyboxBackContent", "SkyboxBk"),
    ("SkyboxFrontContent", "SkyboxFt"),
    ("SkyboxLeftContent", "SkyboxLf"),
    ("SkyboxRightContent", "SkyboxRt"),
    ("SkyboxDownContent", "SkyboxDn"),
];

/// The sky a place without a Sky shows: the skybox in the user's Roblox
/// install (`PlatformContent/pc/textures/sky/sky512_*.tex`, DDS files), in
/// [`SKYBOX`] order. Without an install, Rook's own plain sky.
pub(super) const DEFAULT_SKYBOX: [&str; 6] = [
    "rbxasset://textures/sky/sky512_up.tex",
    "rbxasset://textures/sky/sky512_bk.tex",
    "rbxasset://textures/sky/sky512_ft.tex",
    "rbxasset://textures/sky/sky512_lf.tex",
    "rbxasset://textures/sky/sky512_rt.tex",
    "rbxasset://textures/sky/sky512_dn.tex",
];

impl SceneCache {
    /// The Lighting service's look, with the place's skybox (its Sky, or
    /// the install's default sky without one): once the six images have
    /// loaded they are drawn behind everything, the plain sky colours
    /// become their averages (the Up image above, the four sides at the
    /// horizon, the Down image below), and so does what reflective parts take on (observed:
    /// Studio's reflections follow the Sky, a black one turning a
    /// 0.3-reflective yellow part olive). The sun, moon and stars, and
    /// `SkyboxOrientation`, aren't drawn yet.
    pub(super) fn lighting(&mut self, dom: &Dom) -> Lighting {
        let (mut l, day) = lighting(dom);
        let sky = dom.find_service("Lighting").and_then(|id| {
            dom.children(id)
                .iter()
                .copied()
                .find(|&c| dom.class_name(c) == "Sky")
        });
        let mut averages = Vec::new();
        let mut hashes = Vec::new();
        for (i, (content, legacy)) in SKYBOX.iter().enumerate() {
            let url = match sky {
                Some(sky) => content_url(dom, sky, &[content, legacy]),
                None => DEFAULT_SKYBOX[i].to_owned(),
            };
            let Some(r) = ContentRef::parse(&url) else {
                return l;
            };
            // The sky is all around the camera: first.
            let state = self.files.state(&r, Want::Image, 0.0);
            let key = (r, Want::Image);
            match state {
                FileState::Loaded(hash) => {
                    let Some(image) = self.scene.images.get(&ImageHash(hash)) else {
                        return l;
                    };
                    // The 1 x 1 mip is the image's average.
                    let px = image.mips.last().expect("mip chain");
                    averages.push([px[0], px[1], px[2]].map(|c| f32::from(c) / 255.0));
                    hashes.push(ImageHash(hash));
                }
                FileState::Loading => {
                    // All six are asked for at once.
                    self.sky_files.insert(key);
                }
                FileState::Failed => return l,
            }
        }
        if averages.len() < SKYBOX.len() {
            return l;
        }
        let mean = |faces: &[[f32; 3]]| {
            let n = faces.len() as f32;
            [0, 1, 2].map(|i| faces.iter().map(|f| f[i]).sum::<f32>() / n)
        };
        l.sky_top = averages[0];
        l.sky_horizon = mean(&averages[1..5]);
        l.sky_bottom = averages[5];
        l.reflection = mean(&averages);
        // The images aren't darkened for the night, so the light they give
        // is.
        l.environment_diffuse *= day;
        l.skybox = hashes.try_into().ok();
        l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_sun_is_high_and_midnight_below() {
        assert!(sun_direction(12.0, 41.733).y > 0.7);
        assert!(sun_direction(0.0, 41.733).y < 0.0);
        assert!(sun_direction(6.0, 0.0).y.abs() < 1e-4);
    }

    /// `Lighting:GetSunDirection()` in Studio.
    #[test]
    fn sun_matches_studio() {
        for (hours, latitude, studio) in [
            (14.0, 41.733, Vec3::new(-0.4749, 0.8225, 0.3129)),
            (9.0, 0.0, Vec3::new(0.6485, 0.6485, -0.3987)),
            (14.0, -30.0, Vec3::new(-0.2974, 0.5151, -0.8038)),
        ] {
            let d = sun_direction(hours, latitude);
            assert!(d.distance(studio) < 1e-3, "{hours} {latitude}: {d}");
        }
    }
}

//! Lighting, the sun and the sky.

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

pub(super) fn lighting(dom: &Dom) -> Lighting {
    let mut l = Lighting::default();
    let Some(id) = dom.find_service("Lighting") else {
        return l;
    };
    let hours = dom.clock_time(id) as f32;
    let latitude = number(dom, id, "GeographicLatitude").unwrap_or(41.733);
    l.sun_direction = sun_direction(hours, latitude);
    if let Some(c) = color(dom, id, "Ambient") {
        l.ambient = c;
    }
    if let Some(c) = color(dom, id, "OutdoorAmbient") {
        l.outdoor_ambient = c;
    }
    if let Some(b) = number(dom, id, "Brightness") {
        l.brightness = b;
    }
    if let Some(c) = color(dom, id, "FogColor") {
        l.fog_color = c;
    }
    if let Some(s) = number(dom, id, "FogStart") {
        l.fog_start = s;
    }
    if let Some(e) = number(dom, id, "FogEnd") {
        l.fog_end = e;
    }
    // Off draws no shadows (Confirmed: creator-docs Lighting.GlobalShadows;
    // defaults to on).
    if let Some(s) = boolean(dom, id, "GlobalShadows") {
        l.global_shadows = s;
    }
    // A night sky darkens towards black as the sun sets (Approximation
    // until skyboxes are supported).
    let day = (l.sun_direction.y * 4.0 + 0.5).clamp(0.05, 1.0);
    l.sky_top = l.sky_top.map(|c| c * day);
    l.sky_horizon = l.sky_horizon.map(|c| c * day);
    l.reflection = l.sky_horizon;
    l
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
    /// horizon), and so does what reflective parts take on (observed:
    /// Studio's reflections follow the Sky, a black one turning a
    /// 0.3-reflective yellow part olive). The sun, moon and stars, and
    /// `SkyboxOrientation`, aren't drawn yet.
    pub(super) fn lighting(&mut self, dom: &Dom) -> Lighting {
        let mut l = lighting(dom);
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
            let state = self.files.state(&r, Want::Image);
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
        l.reflection = mean(&averages);
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

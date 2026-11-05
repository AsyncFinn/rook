//! The sky: a Sky's six images, or the install's default sky.

use super::*;

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

/// The time of day's tint on the sky, per channel of light units, at
/// `ClockTime` `hours` (`Lighting::sky_tint`): 1 from 6:30 to 17:30, rising
/// at dawn through gray and falling at dusk through orange, 0 in the night.
/// Measured in Studio 0.745 on the default sky at the zenith, 30 degrees up
/// and the horizon, which all take the same share within 0.05, and the same
/// at latitudes 0 and 60, so it goes by the clock, not the sun's height
/// (docs/behavior/lighting/sky.md). Linear between the measured times.
pub(super) fn sky_tint(hours: f32) -> [f32; 3] {
    const TINTS: [(f32, [f32; 3]); 15] = [
        (6.0, [0.0, 0.0, 0.0]),
        (6.1, [0.14, 0.11, 0.09]),
        (6.2, [0.28, 0.24, 0.22]),
        (6.29, [0.45, 0.41, 0.39]),
        (6.38, [0.65, 0.63, 0.61]),
        (6.44, [0.82, 0.80, 0.78]),
        (6.5, [1.0, 1.0, 1.0]),
        (17.5, [1.0, 1.0, 1.0]),
        (17.62, [0.72, 0.66, 0.60]),
        (17.71, [0.55, 0.45, 0.37]),
        (17.8, [0.40, 0.29, 0.21]),
        (17.9, [0.25, 0.15, 0.08]),
        (18.0, [0.14, 0.06, 0.01]),
        (18.3, [0.0, 0.0, 0.0]),
        (24.0, [0.0, 0.0, 0.0]),
    ];
    let h = hours.rem_euclid(24.0);
    let Some(i) = TINTS.iter().position(|&(at, _)| at > h) else {
        return [0.0; 3];
    };
    if i == 0 {
        return [0.0; 3];
    }
    let ((h0, a), (h1, b)) = (TINTS[i - 1], TINTS[i]);
    let t = (h - h0) / (h1 - h0);
    [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t)
}

impl SceneCache {
    /// Add the place's skybox to `l` (its Sky, or the install's default
    /// sky without one): once the six images have loaded they are drawn
    /// behind everything and reflected by parts, and the plain sky colors
    /// become their averages (the Up image above, the four sides at the
    /// horizon, the Down image below). The sun, moon and stars, and
    /// `SkyboxOrientation`, aren't drawn yet.
    pub(super) fn sky(&mut self, dom: &Dom, l: &mut Lighting) {
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
                return;
            };
            // The sky is all around the camera: first.
            let state = self.files.state(&r, Want::Image, 0.0);
            let key = (r, Want::Image);
            match state {
                FileState::Loaded(hash) => {
                    let Some(image) = self.scene.images.get(&ImageHash(hash)) else {
                        return;
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
                FileState::Failed => return,
            }
        }
        if averages.len() < SKYBOX.len() {
            return;
        }
        let mean = |faces: &[[f32; 3]]| {
            let n = faces.len() as f32;
            [0, 1, 2].map(|i| faces.iter().map(|f| f[i]).sum::<f32>() / n)
        };
        l.sky_top = averages[0];
        l.sky_horizon = mean(&averages[1..5]);
        l.sky_bottom = averages[5];
        l.skybox = hashes.try_into().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sky_is_untinted_by_day_and_black_at_night() {
        assert_eq!(sky_tint(14.0), [1.0; 3]);
        assert_eq!(sky_tint(6.5), [1.0; 3]);
        assert_eq!(sky_tint(17.5), [1.0; 3]);
        assert_eq!(sky_tint(0.0), [0.0; 3]);
        assert_eq!(sky_tint(5.0), [0.0; 3]);
        assert_eq!(sky_tint(20.0), [0.0; 3]);
        assert_eq!(sky_tint(38.0), [1.0; 3]);
    }

    /// Studio's sky at the measured times.
    #[test]
    fn the_tint_follows_studio_at_dawn_and_dusk() {
        for (hours, studio) in [(6.29, [0.45, 0.41, 0.39]), (17.8, [0.40, 0.29, 0.21])] {
            let t = sky_tint(hours);
            for c in 0..3 {
                assert!((t[c] - studio[c]).abs() < 1e-4, "{hours}: {t:?}");
            }
        }
        let between = sky_tint(6.25);
        assert!(between[0] > 0.28 && between[0] < 0.45, "{between:?}");
    }
}

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

impl SceneCache {
    /// Add the place's skybox to `l` (its Sky, or the install's default
    /// sky without one): once the six images have loaded they are drawn
    /// behind everything and reflected by parts, and the plain sky colors
    /// become their averages (the Up image above, the four sides at the
    /// horizon, the Down image below). The sun, moon and stars, and
    /// `SkyboxOrientation`, aren't drawn yet.
    pub(super) fn sky(&mut self, dom: &Dom, l: &mut Lighting, day: f32) {
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
        // The images aren't darkened for the night, so the light they give
        // is.
        l.environment_diffuse *= day;
        l.skybox = hashes.try_into().ok();
    }
}

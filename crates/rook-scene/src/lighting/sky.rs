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

/// The sky a place shows without a Sky, or with one missing an image: the
/// skybox in Roblox's packages or install
/// (`PlatformContent/pc/textures/sky/sky512_*.tex`, DDS files), in
/// [`SKYBOX`] order. Without those, Rook's own plain sky.
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

enum Faces {
    /// All loaded: their images and average colors.
    Ready(Vec<ImageHash>, Vec<[f32; 3]>),
    Loading,
    /// One will never load.
    Failed,
}

impl SceneCache {
    /// Add the place's skybox to `l`: its Sky's, or the default sky without
    /// one or until all six of its images are there, as Studio draws a Sky
    /// with an image that won't load
    /// (docs/behavior/lighting/sky.md#a-sky-that-wont-load). The images are
    /// drawn behind everything and reflected by parts, and the plain sky
    /// colors become their averages (the Up image above, the four sides at
    /// the horizon, the Down image below). The sun, moon and stars, and
    /// `SkyboxOrientation`, aren't drawn yet.
    pub(super) fn sky(&mut self, dom: &Dom, l: &mut Lighting) {
        let sky = dom.find_service("Lighting").and_then(|id| {
            dom.children(id)
                .iter()
                .copied()
                .find(|&c| dom.class_name(c) == "Sky")
        });
        let place = sky.map(|sky| SKYBOX.map(|names| content_url(dom, sky, &[names.0, names.1])));
        let faces = match place.map(|urls| self.sky_faces(&urls)) {
            Some(ready @ Faces::Ready(..)) => ready,
            _ => self.sky_faces(&DEFAULT_SKYBOX.map(str::to_owned)),
        };
        let Faces::Ready(hashes, averages) = faces else {
            return;
        };
        let mean = |faces: &[[f32; 3]]| {
            let n = faces.len() as f32;
            [0, 1, 2].map(|i| faces.iter().map(|f| f[i]).sum::<f32>() / n)
        };
        l.sky_top = averages[0];
        l.sky_horizon = mean(&averages[1..5]);
        l.sky_bottom = averages[5];
        l.skybox = hashes.try_into().ok();
    }

    fn sky_faces(&mut self, urls: &[String; 6]) -> Faces {
        let mut averages = Vec::new();
        let mut hashes = Vec::new();
        let mut loading = false;
        for url in urls {
            let Some(r) = ContentRef::parse(url) else {
                return Faces::Failed;
            };
            // The sky is all around the camera: first.
            let state = self.files.state(&r, Want::Image, 0.0);
            let key = (r, Want::Image);
            match state {
                FileState::Loaded(hash) => {
                    let Some(image) = self.images.info(ImageHash(hash)) else {
                        loading = true;
                        continue;
                    };
                    averages.push(image.average);
                    hashes.push(ImageHash(hash));
                }
                FileState::Loading => {
                    // All six are asked for at once.
                    self.sky_files.insert(key);
                    loading = true;
                }
                FileState::Failed => return Faces::Failed,
            }
        }
        if loading {
            Faces::Loading
        } else {
            Faces::Ready(hashes, averages)
        }
    }
}

#[cfg(test)]
mod tests {
    use rook_types::Value;

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

    const FACES: [&str; 6] = [
        "rbxassetid://1",
        "rbxassetid://2",
        "rbxassetid://3",
        "rbxassetid://4",
        "rbxassetid://5",
        "rbxassetid://6",
    ];

    /// A 1 x 1 BMP of one gray.
    fn image(gray: u8) -> Vec<u8> {
        let mut bmp = b"BM".to_vec();
        for word in [58u32, 0, 54, 40, 1, 1] {
            bmp.extend(word.to_le_bytes());
        }
        bmp.extend([1, 0, 24, 0]);
        bmp.extend([0u8; 24]);
        bmp.extend([gray, gray, gray, 0]);
        bmp
    }

    /// A cache for a place whose Sky names `faces`, with the default sky
    /// (gray 200) in a content folder, and those of [`FACES`] that `have`
    /// says exist, as gray 100, as overrides.
    #[expect(
        clippy::disallowed_methods,
        reason = "tests set up files on the machine running them"
    )]
    fn place_with_sky(name: &str, have: &[&str]) -> (Dom, SceneCache) {
        let dir = std::env::temp_dir().join(format!("rook-sky-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sky_dir = dir.join("folder/textures/sky");
        std::fs::create_dir_all(&sky_dir).unwrap();
        std::fs::create_dir_all(dir.join("overrides")).unwrap();
        for url in DEFAULT_SKYBOX {
            let file = url.rsplit('/').next().unwrap();
            std::fs::write(sky_dir.join(file), image(200)).unwrap();
        }
        for url in have {
            let id = url.rsplit('/').next().unwrap();
            std::fs::write(dir.join("overrides").join(format!("{id}.bmp")), image(100)).unwrap();
        }
        let mut dom = Dom::new();
        let lighting = dom.get_service("Lighting").unwrap();
        let sky = dom.create_by_name("Sky").unwrap();
        dom.set_parent(sky, Some(lighting)).unwrap();
        for ((name, _), url) in SKYBOX.iter().zip(FACES) {
            let content = rook_types::Content::from_uri(url.to_owned());
            dom.set_property_by_name(sky, name, Value::Content(content))
                .unwrap();
        }
        let mut config = rook_content::Config::empty();
        config.folders = vec![dir.join("folder")];
        config.overrides_dir = Some(dir.join("overrides"));
        let content = Arc::new(Resolver::new(config));
        let cache = SceneCache::new(&dom, Floors::default(), content, glam::Vec3::ZERO);
        (dom, cache)
    }

    /// The gray the scene's sky came out at the top, once loading settles.
    fn settled_gray(dom: &Dom, cache: &mut SceneCache) -> f32 {
        // A failed place sky asks for the default one, so twice.
        cache.poll_files(dom, true);
        cache.poll_files(dom, true);
        assert!(cache.scene.lighting.skybox.is_some());
        cache.scene.lighting.sky_top[0]
    }

    #[test]
    fn a_sky_with_all_its_images_draws_them() {
        let (dom, mut cache) = place_with_sky("whole", &FACES);
        assert!((settled_gray(&dom, &mut cache) - 100.0 / 255.0).abs() < 0.01);
    }

    /// Measured in Studio: one image that won't load gives the default sky
    /// on every side.
    #[test]
    fn a_sky_missing_an_image_draws_the_default_sky() {
        let (dom, mut cache) = place_with_sky("missing", &FACES[1..]);
        assert!((settled_gray(&dom, &mut cache) - 200.0 / 255.0).abs() < 0.01);
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

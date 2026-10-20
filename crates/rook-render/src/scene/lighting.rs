//! The scene's light: the sun, sky, ambient and tone curve, and local
//! lights.

use super::*;

/// Where each skybox image goes, in [`Lighting::skybox`] order: (the
/// direction it covers, the image's right, the image's up), measured in
/// Studio 0.742 (2026-10-13) with six labelled images. The four sides are
/// upright and unmirrored seen from inside, the Left image at +X and the
/// Right one at -X (the opposite of what the names suggest); Up's image has
/// its top towards +X, Down's towards -X.
pub const SKYBOX_FACES: [(Vec3, Vec3, Vec3); 6] = [
    (Vec3::Y, Vec3::NEG_Z, Vec3::X),
    (Vec3::Z, Vec3::NEG_X, Vec3::Y),
    (Vec3::NEG_Z, Vec3::X, Vec3::Y),
    (Vec3::X, Vec3::Z, Vec3::Y),
    (Vec3::NEG_X, Vec3::NEG_Z, Vec3::Y),
    (Vec3::NEG_Y, Vec3::NEG_Z, Vec3::NEG_X),
];

/// Scene lighting, mapped from the Lighting service.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Unit vector pointing towards the sun, or towards the moon while
    /// the sun is down: where [`sun_color`](Self::sun_color) comes from.
    pub sun_direction: Vec3,
    /// sRGB colours.
    pub ambient: [f32; 3],
    pub outdoor_ambient: [f32; 3],
    /// The sun's (or moon's) light on a white face square to it, in
    /// Studio's light units per channel, `Brightness` included
    /// (docs/behavior/lights.md, "The sun").
    pub sun_color: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_start: f32,
    pub fog_end: f32,
    pub sky_top: [f32; 3],
    pub sky_horizon: [f32; 3],
    /// The sky below the horizon (the skybox's Down image).
    pub sky_bottom: [f32; 3],
    /// What reflective parts take on (`Reflectance`): the sky's colour,
    /// the average of the place's skybox when it has one (sRGB).
    pub reflection: [f32; 3],
    /// The skybox's six images, in [`SKYBOX_FACES`] order (Up, Back,
    /// Front, Left, Right, Down), drawn behind everything once all six
    /// are in [`Scene::images`]; `None` draws the plain sky colour.
    pub skybox: Option<[ImageHash; 6]>,
    /// `Lighting.GlobalShadows`: false draws no shadows at all.
    pub global_shadows: bool,
    /// `Lighting.EnvironmentSpecularScale`, 0 to 1: how much parts reflect
    /// the sky, and how far metals trade their diffuse light for it.
    pub environment_specular: f32,
    /// `Lighting.EnvironmentDiffuseScale`, 0 to 1: how much the sky lights
    /// surfaces (on top of Ambient), dimmed as the sky darkens at night.
    pub environment_diffuse: f32,
    /// `Lighting.ExposureCompensation` as a multiplier on linear light
    /// (2 to the power of it), for everything drawn, the sky included.
    pub exposure: f32,
    /// The tone curve the frame goes through last
    /// (`ColorGradingEffect.TonemapperPreset`).
    pub tonemapper: Tonemapper,
}

/// `ColorGradingEffect.TonemapperPreset`: how light becomes the screen's
/// pixels (docs/behavior/tone-mapping.md).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tonemapper {
    /// Studio's curve, which darkens the darks and rolls off the brights;
    /// places without a ColorGradingEffect get it too.
    #[default]
    Default,
    /// No curve: the light's square root is the pixel.
    Retro,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            sun_direction: Vec3::new(0.3, 0.8, 0.5).normalize(),
            ambient: [0.27; 3],
            outdoor_ambient: [0.27; 3],
            sun_color: [1.0; 3],
            fog_color: [0.75; 3],
            fog_start: 0.0,
            fog_end: 100_000.0,
            sky_top: [0.36, 0.56, 0.86],
            sky_horizon: [0.75, 0.84, 0.95],
            sky_bottom: [0.75, 0.84, 0.95],
            reflection: [0.75, 0.84, 0.95],
            skybox: None,
            global_shadows: true,
            environment_specular: 0.0,
            environment_diffuse: 0.0,
            exposure: 1.0,
            tonemapper: Tonemapper::Default,
        }
    }
}

/// How a [`LocalLight`] shines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LightShape {
    /// A PointLight: every way from `position`.
    Point,
    /// A SpotLight: a cone `angle` degrees across (`SpotLight.Angle`, 0 to
    /// 180) around `direction` (unit, the face it shines from).
    Spot { direction: Vec3, angle: f32 },
    /// A SurfaceLight: from the centre of a face of its part (`position`),
    /// over a cone `angle` degrees across around `direction` (unit, the
    /// face's normal).
    Surface { direction: Vec3, angle: f32 },
}

/// A PointLight, SpotLight or SurfaceLight, placed in the world
/// (`docs/behavior/lights.md`). The renderer turns its Roblox properties
/// into light as Studio does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalLight {
    pub position: Vec3,
    pub shape: LightShape,
    /// `Light.Color`, sRGB 0..1.
    pub color: [f32; 3],
    /// `Light.Brightness`.
    pub brightness: f32,
    /// `Range`, in studs.
    pub range: f32,
    /// `Light.Shadows`: parts block its light.
    pub shadows: bool,
    /// The box (min, max) of the part it hangs from, which doesn't block
    /// its light.
    pub part: (Vec3, Vec3),
}

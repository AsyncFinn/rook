//! The Lighting service's look: the sun and moon ([`sun`]), the sky
//! ([`sky`]), the tone curve, and local lights ([`local`]).

mod local;
mod sky;
mod sun;

use super::*;
use sun::{daylight, sun_and_moon, sun_direction};

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
    match grading.and_then(|c| dom.enum_item_property(c, "TonemapperPreset")) {
        Some("Retro") => rook_render::Tonemapper::Retro,
        _ => rook_render::Tonemapper::Default,
    }
}

impl SceneCache {
    /// The Lighting service's look, with the place's skybox.
    pub(super) fn lighting(&mut self, dom: &Dom) -> Lighting {
        let (mut l, day) = lighting(dom);
        self.sky(dom, &mut l, day);
        l
    }
}

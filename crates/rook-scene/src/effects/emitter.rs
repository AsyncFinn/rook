//! ParticleEmitter, and the emitters every particle effect draws with
//! (docs/behavior/effects/particle-emitters.md).

use rook_render::{
    Curve, EmitterLook, EmitterShape, Flipbook, FlipbookMode, InOut, Orientation, Sequence, Spawn,
};

use super::*;

impl Reader<'_, '_> {
    /// The emitters effect `id` draws with now: always as many for its
    /// class, disabled where Studio draws nothing (not under a part or an
    /// Attachment).
    pub(super) fn emitters(&mut self, id: InstanceId) -> Vec<Emitter> {
        let dom = self.dom;
        if dom.class_name(id) == "Explosion" {
            return explosion::emitters(dom, id, self.textures);
        }
        let frame = holder_frame(dom, id);
        let enabled = frame.is_some() && dom.bool_property(id, "Enabled") != Some(false);
        let time_scale = dom
            .float_property(id, "TimeScale")
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let looks = match dom.class_name(id) {
            "Fire" => classic::fire(dom, id, self.textures),
            "Smoke" => vec![classic::smoke(dom, id, self.textures)],
            "Sparkles" => vec![classic::sparkles(dom, id, self.textures)],
            _ => {
                let mut look = self.particle_emitter(id);
                // Nowhere to emit from: nothing shows, scripts' bursts
                // included.
                if frame.is_none() {
                    look.texture = None;
                }
                vec![look]
            }
        };
        let frame = frame.unwrap_or(CFrame::IDENTITY);
        looks
            .into_iter()
            .enumerate()
            .map(|(layer, look)| Emitter {
                key: key(id, layer),
                position: frame.position,
                rotation: frame.rotation,
                enabled,
                time_scale,
                emitted: self.calls.emitted,
                cleared: self.calls.cleared,
                look: Arc::new(look),
            })
            .collect()
    }

    /// A ParticleEmitter's look. Under a part its particles are born in
    /// the part's `Shape` (its bounding box by default); under an
    /// Attachment, at the Attachment (creator-docs).
    fn particle_emitter(&mut self, id: InstanceId) -> EmitterLook {
        let dom = self.dom;
        let parent = dom.parent(id);
        let half = parent
            .filter(|&p| is_part(dom, p))
            .and_then(|p| dom.vector3_property(p, "Size"))
            .map_or(Vec3::ZERO, |size| size * 0.5);
        let direction = dom
            .enum_property(id, "EmissionDirection")
            .and_then(NormalId::from_u32)
            .unwrap_or(NormalId::Top)
            .normal();
        let shape = match dom.enum_item_property(id, "Shape") {
            Some("Sphere") => EmitterShape::Sphere,
            Some("Cylinder") => EmitterShape::Cylinder,
            Some("Disc") => EmitterShape::Disc,
            _ => EmitterShape::Box,
        };
        let in_out = match dom.enum_item_property(id, "ShapeInOut") {
            Some("Inward") => InOut::Inward,
            Some("InAndOut") => InOut::Both,
            _ => InOut::Outward,
        };
        let transparency =
            number_sequence(dom, id, "Transparency").unwrap_or(Sequence::constant(0.0));
        let alpha = Sequence {
            curve: Curve(
                transparency
                    .curve
                    .0
                    .iter()
                    .map(|&(t, v)| (t, 1.0 - v))
                    .collect(),
            ),
            envelope: transparency.envelope,
        };
        // Studio draws a particle's square 2 × Size across (measured: Size
        // 1 drew 1.98 studs, Size 4 drew 8).
        let mut size = number_sequence(dom, id, "Size").unwrap_or(Sequence::constant(1.0));
        for (_, v) in &mut size.curve.0 {
            *v *= 2.0;
        }
        for e in &mut size.envelope {
            *e *= 2.0;
        }
        let orientation = match dom.enum_item_property(id, "Orientation") {
            Some("FacingCameraWorldUp") => Orientation::FacingCameraWorldUp,
            Some("VelocityParallel") => Orientation::VelocityParallel,
            Some("VelocityPerpendicular") => Orientation::VelocityPerpendicular,
            _ => Orientation::FacingCamera,
        };
        EmitterLook {
            texture: self.texture(id),
            flipbook: flipbook(dom, id),
            rate: dom
                .float_property(id, "Rate")
                .unwrap_or(20.0)
                .clamp(0.0, 10_000.0),
            lifetime: dom
                .number_range_property(id, "Lifetime")
                .map(|r| [r.min, r.max])
                .unwrap_or([5.0, 10.0]),
            spawn: Spawn::Shape {
                shape,
                half,
                surface: dom.enum_item_property(id, "ShapeStyle") == Some("Surface"),
                in_out,
                partial: dom.float_property(id, "ShapePartial").unwrap_or(1.0),
            },
            speed: dom
                .number_range_property(id, "Speed")
                .map(|r| [r.min, r.max])
                .unwrap_or([5.0, 5.0]),
            direction,
            spread_angle: dom
                .vector2_property(id, "SpreadAngle")
                .map(|v| v.0)
                .unwrap_or(Vec2::ZERO),
            world_acceleration: dom
                .vector3_property(id, "Acceleration")
                .unwrap_or(Vec3::ZERO),
            // Drag halves the speed every 1 / Drag seconds (creator-docs).
            drag: dom.float_property(id, "Drag").unwrap_or(0.0) * std::f32::consts::LN_2,
            locked: dom.bool_property(id, "LockedToPart") == Some(true),
            size,
            squash: number_sequence(dom, id, "Squash").map_or(Curve::constant(0.0), |s| s.curve),
            rotation: dom
                .number_range_property(id, "Rotation")
                .map(|r| [r.min, r.max])
                .unwrap_or([0.0, 0.0]),
            spin: dom
                .number_range_property(id, "RotSpeed")
                .map(|r| [r.min, r.max])
                .unwrap_or([0.0, 0.0]),
            orientation,
            color: color_sequence(dom, id, "Color").unwrap_or(Curve::constant([1.0; 3])),
            brightness: Curve::constant(
                dom.float_property(id, "Brightness").unwrap_or(1.0).max(0.0),
            ),
            alpha,
            light_emission: dom.float_property(id, "LightEmission").unwrap_or(0.0),
            light_influence: dom.float_property(id, "LightInfluence").unwrap_or(0.0),
            z_offset: dom.float_property(id, "ZOffset").unwrap_or(0.0),
            ..Default::default()
        }
    }
}

/// A ParticleEmitter's flipbook, when its texture is laid out as one.
fn flipbook(dom: &Dom, id: InstanceId) -> Option<Flipbook> {
    let (columns, rows) = match dom.enum_item_property(id, "FlipbookLayout")? {
        "Grid2x2" => (2, 2),
        "Grid4x4" => (4, 4),
        "Grid8x8" => (8, 8),
        "Custom" => (
            dom.float_property(id, "FlipbookSizeX")
                .unwrap_or(1.0)
                .max(1.0) as u32,
            dom.float_property(id, "FlipbookSizeY")
                .unwrap_or(1.0)
                .max(1.0) as u32,
        ),
        _ => return None,
    };
    let mode = match dom.enum_item_property(id, "FlipbookMode") {
        Some("OneShot") => FlipbookMode::OneShot,
        Some("PingPong") => FlipbookMode::PingPong,
        Some("Random") => FlipbookMode::Random,
        _ => FlipbookMode::Loop,
    };
    // At most 30 frames a second (creator-docs).
    let framerate = dom
        .number_range_property(id, "FlipbookFramerate")
        .map(|r| [r.min, r.max])
        .unwrap_or([1.0, 1.0])
        .map(|f| f.clamp(0.0, 30.0));
    Some(Flipbook {
        columns,
        rows,
        mode,
        framerate,
        start_random: dom.bool_property(id, "FlipbookStartRandom") == Some(true),
    })
}

//! ParticleEmitter, and the emitters every particle effect draws with
//! (docs/behavior/particle-emitters.md).

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
        let enabled = frame.is_some() && boolean(dom, id, "Enabled") != Some(false);
        let time_scale = number(dom, id, "TimeScale").unwrap_or(1.0).clamp(0.0, 1.0);
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
            .and_then(|p| vector3(dom, p, "Size"))
            .map_or(Vec3::ZERO, |size| size * 0.5);
        let direction = enum_value(dom, id, "EmissionDirection")
            .and_then(NormalId::from_u32)
            .unwrap_or(NormalId::Top)
            .normal();
        let shape = match enum_value(dom, id, "Shape") {
            Some(1) => EmitterShape::Sphere,
            Some(2) => EmitterShape::Cylinder,
            Some(3) => EmitterShape::Disc,
            _ => EmitterShape::Box,
        };
        let in_out = match enum_value(dom, id, "ShapeInOut") {
            Some(1) => InOut::Inward,
            Some(2) => InOut::Both,
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
        let orientation = match enum_value(dom, id, "Orientation") {
            Some(1) => Orientation::FacingCameraWorldUp,
            Some(2) => Orientation::VelocityParallel,
            Some(3) => Orientation::VelocityPerpendicular,
            _ => Orientation::FacingCamera,
        };
        EmitterLook {
            texture: self.texture(id),
            flipbook: flipbook(dom, id),
            rate: number(dom, id, "Rate").unwrap_or(20.0).clamp(0.0, 10_000.0),
            lifetime: number_range(dom, id, "Lifetime").unwrap_or([5.0, 10.0]),
            spawn: Spawn::Shape {
                shape,
                half,
                surface: enum_value(dom, id, "ShapeStyle") == Some(1),
                in_out,
                partial: number(dom, id, "ShapePartial").unwrap_or(1.0),
            },
            speed: number_range(dom, id, "Speed").unwrap_or([5.0, 5.0]),
            direction,
            spread_angle: vector2(dom, id, "SpreadAngle").unwrap_or(Vec2::ZERO),
            world_acceleration: vector3(dom, id, "Acceleration").unwrap_or(Vec3::ZERO),
            // Drag halves the speed every 1 / Drag seconds (creator-docs).
            drag: number(dom, id, "Drag").unwrap_or(0.0) * std::f32::consts::LN_2,
            locked: boolean(dom, id, "LockedToPart") == Some(true),
            size,
            squash: number_sequence(dom, id, "Squash").map_or(Curve::constant(0.0), |s| s.curve),
            rotation: number_range(dom, id, "Rotation").unwrap_or([0.0, 0.0]),
            spin: number_range(dom, id, "RotSpeed").unwrap_or([0.0, 0.0]),
            orientation,
            color: color_sequence(dom, id, "Color").unwrap_or(Curve::constant([1.0; 3])),
            brightness: Curve::constant(number(dom, id, "Brightness").unwrap_or(1.0).max(0.0)),
            alpha,
            light_emission: number(dom, id, "LightEmission").unwrap_or(0.0),
            light_influence: number(dom, id, "LightInfluence").unwrap_or(0.0),
            z_offset: number(dom, id, "ZOffset").unwrap_or(0.0),
            ..Default::default()
        }
    }
}

/// A ParticleEmitter's flipbook, when its texture is laid out as one.
fn flipbook(dom: &Dom, id: InstanceId) -> Option<Flipbook> {
    let (columns, rows) = match enum_value(dom, id, "FlipbookLayout")? {
        1 => (2, 2),
        2 => (4, 4),
        3 => (8, 8),
        4 => (
            number(dom, id, "FlipbookSizeX").unwrap_or(1.0).max(1.0) as u32,
            number(dom, id, "FlipbookSizeY").unwrap_or(1.0).max(1.0) as u32,
        ),
        _ => return None,
    };
    let mode = match enum_value(dom, id, "FlipbookMode") {
        Some(1) => FlipbookMode::OneShot,
        Some(2) => FlipbookMode::PingPong,
        Some(3) => FlipbookMode::Random,
        _ => FlipbookMode::Loop,
    };
    // At most 30 frames a second (creator-docs).
    let framerate = number_range(dom, id, "FlipbookFramerate")
        .unwrap_or([1.0, 1.0])
        .map(|f| f.clamp(0.0, 30.0));
    Some(Flipbook {
        columns,
        rows,
        mode,
        framerate,
        start_random: boolean(dom, id, "FlipbookStartRandom") == Some(true),
    })
}

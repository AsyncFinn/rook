//! Beam and Trail as the renderer's ribbons
//! (docs/behavior/effects/beams-and-trails.md), and a FloorWire's images
//! (docs/behavior/rendering/adornments.md).

use rook_render::{Anchor, Curve, RibbonLook, Sequence, TextureMode};

use super::*;

/// A Beam's or Trail's Attachments in the world, when both are under
/// parts.
pub(super) fn anchors(dom: &Dom, id: InstanceId) -> Option<[Anchor; 2]> {
    let anchor = |name| {
        let frame = attachment_frame(dom, dom.ref_property(id, name)?)?;
        Some(Anchor {
            position: frame.position,
            rotation: frame.rotation,
        })
    };
    Some([anchor("Attachment0")?, anchor("Attachment1")?])
}

impl Reader<'_, '_> {
    pub(super) fn beam(&mut self, id: InstanceId) -> Beam {
        let dom = self.dom;
        let ends = anchors(dom, id);
        let width = |name| dom.float_property(id, name).unwrap_or(1.0);
        Beam {
            enabled: ends.is_some() && dom.bool_property(id, "Enabled") != Some(false),
            ends: ends.unwrap_or(
                [Anchor {
                    position: Vec3::ZERO,
                    rotation: Mat3::IDENTITY,
                }; 2],
            ),
            curve: [
                dom.float_property(id, "CurveSize0").unwrap_or(0.0),
                dom.float_property(id, "CurveSize1").unwrap_or(0.0),
            ],
            width: [width("Width0"), width("Width1")],
            segments: dom.float_property(id, "Segments").unwrap_or(10.0).max(1.0) as u32,
            texture_speed: dom.float_property(id, "TextureSpeed").unwrap_or(1.0),
            texture_offset: self.calls.texture_offset,
            z_offset: dom.float_property(id, "ZOffset").unwrap_or(0.0),
            look: self.ribbon_look(id),
        }
    }

    pub(super) fn trail(&mut self, id: InstanceId) -> Trail {
        let dom = self.dom;
        let ends = anchors(dom, id);
        Trail {
            enabled: ends.is_some() && dom.bool_property(id, "Enabled") != Some(false),
            ends: ends.map_or([Vec3::ZERO; 2], |[a, z]| [a.position, z.position]),
            // Between 0.01 and 20 seconds (creator-docs).
            lifetime: dom
                .float_property(id, "Lifetime")
                .unwrap_or(2.0)
                .clamp(0.01, 20.0),
            min_length: dom.float_property(id, "MinLength").unwrap_or(0.1).max(0.0),
            max_length: dom.float_property(id, "MaxLength").unwrap_or(0.0).max(0.0),
            width_scale: number_sequence(dom, id, "WidthScale").unwrap_or(Sequence::constant(1.0)),
            cleared: self.calls.cleared,
            look: self.ribbon_look(id),
        }
    }

    /// FloorWire `id`'s images along its wire: unlit, untouched by its
    /// `Color3`, faded by its `Transparency` (measured), drawn while it's
    /// `Visible` and has both its parts.
    pub(super) fn floor_wire(&mut self, id: InstanceId) -> FloorWire {
        let dom = self.dom;
        let shown = dom.bool_property(id, "Visible") != Some(false);
        let path = super::adornments::floor_wire::path(dom, id).filter(|_| shown);
        let size = dom
            .vector2_property(id, "TextureSize")
            .map_or(Vec2::ONE, |v| Vec2::new(v.x(), v.y()));
        let transparency = dom.float_property(id, "Transparency").unwrap_or(0.0);
        FloorWire {
            path: path.unwrap_or_default(),
            size,
            spacing: dom
                .float_property(id, "StudsBetweenTextures")
                .unwrap_or(4.0),
            velocity: dom.float_property(id, "Velocity").unwrap_or(2.0),
            cycle_offset: dom.float_property(id, "CycleOffset").unwrap_or(0.0),
            look: RibbonLook {
                texture: self.texture(id),
                texture_mode: TextureMode::Stretch,
                texture_length: 1.0,
                color: Curve::constant([1.0; 3]),
                alpha: Sequence::constant(1.0 - transparency),
                brightness: 1.0,
                light_emission: 0.0,
                light_influence: 0.0,
                face_camera: false,
            },
        }
    }

    /// What Beam and Trail share.
    fn ribbon_look(&mut self, id: InstanceId) -> RibbonLook {
        let dom = self.dom;
        let transparency =
            number_sequence(dom, id, "Transparency").unwrap_or(Sequence::constant(0.5));
        RibbonLook {
            texture: self.texture(id),
            texture_mode: match dom.enum_item_property(id, "TextureMode") {
                Some("Wrap") => TextureMode::Wrap,
                Some("Static") => TextureMode::Static,
                _ => TextureMode::Stretch,
            },
            texture_length: dom.float_property(id, "TextureLength").unwrap_or(1.0),
            color: color_sequence(dom, id, "Color").unwrap_or(Curve::constant([1.0; 3])),
            alpha: Sequence::from(Curve(
                transparency
                    .curve
                    .0
                    .iter()
                    .map(|&(t, v)| (t, 1.0 - v))
                    .collect(),
            )),
            brightness: dom.float_property(id, "Brightness").unwrap_or(1.0).max(0.0),
            light_emission: dom.float_property(id, "LightEmission").unwrap_or(0.0),
            light_influence: dom.float_property(id, "LightInfluence").unwrap_or(0.0),
            face_camera: dom.bool_property(id, "FaceCamera") == Some(true),
        }
    }
}

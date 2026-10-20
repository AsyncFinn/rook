//! Beams and Trails: strips of an image between two Attachments
//! (docs/behavior/effects/beams-and-trails.md).

use glam::{Mat3, Vec3};

use super::ImageHash;
use super::emitters::{Curve, Sequence};

/// An Attachment where it is now, in the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub position: Vec3,
    /// Its axes: x is its Axis, y its SecondaryAxis.
    pub rotation: Mat3,
}

/// `Enum.TextureMode`: how a ribbon's image repeats along it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureMode {
    /// `TextureLength` times over its whole length.
    #[default]
    Stretch,
    /// Once every `TextureLength` studs, from the start.
    Wrap,
    /// Once every `TextureLength` studs, where it was laid (a Trail's
    /// stays put in the world as it's drawn out).
    Static,
}

/// How a ribbon looks: shared by Beam and Trail.
#[derive(Debug, Clone, PartialEq)]
pub struct RibbonLook {
    /// Its image; none draws it in its colour alone.
    pub texture: Option<ImageHash>,
    pub texture_mode: TextureMode,
    pub texture_length: f32,
    /// sRGB colour and opacity along it (a Beam from Attachment0 to
    /// Attachment1; a Trail from its newest end to its oldest), and its
    /// brightness on its light units.
    pub color: Curve<[f32; 3]>,
    pub alpha: Sequence,
    pub brightness: f32,
    /// 0 covers what's behind by its opacity; 1 adds to it.
    pub light_emission: f32,
    /// 0 in its own colour; 1 lit like a part.
    pub light_influence: f32,
    /// Turned to face the camera, or flat as its Attachments lie.
    pub face_camera: bool,
}

/// A Beam: a curve from one Attachment to the other (a cubic Bézier whose
/// inner points lie `curve` studs along each Attachment's Axis, the
/// second backwards), `segments` straight pieces, as wide as `width` at
/// each end across their SecondaryAxis (measured: the width runs along the
/// Attachments' up).
#[derive(Debug, Clone, PartialEq)]
pub struct Beam {
    /// Off (or without both Attachments): not drawn.
    pub enabled: bool,
    pub ends: [Anchor; 2],
    pub curve: [f32; 2],
    pub width: [f32; 2],
    pub segments: u32,
    /// Image repeats a second the image moves from Attachment0 towards
    /// Attachment1 (`TextureSpeed`), and where it starts (scripts'
    /// `SetTextureOffset`).
    pub texture_speed: f32,
    pub texture_offset: f32,
    /// Studs towards the camera it's drawn at.
    pub z_offset: f32,
    pub look: RibbonLook,
}

/// A Trail: while enabled, the line between its Attachments is laid down
/// as they move, and each piece fades over `lifetime` seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Trail {
    pub ends: [Vec3; 2],
    pub enabled: bool,
    pub lifetime: f32,
    /// A new piece starts once either end has moved this far; until then
    /// the newest piece stretches.
    pub min_length: f32,
    /// The longest it gets, in studs (0: no limit).
    pub max_length: f32,
    /// Its width over a piece's life, as a share of the distance between
    /// its Attachments, about their midpoint.
    pub width_scale: Sequence,
    /// How many times scripts have cleared it (`Clear`).
    pub cleared: u64,
    pub look: RibbonLook,
}

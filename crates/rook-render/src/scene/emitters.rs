//! Particle emitters: what the renderer simulates and draws for Fire,
//! Smoke and Sparkles (docs/behavior/effects.md), in terms general enough
//! for ParticleEmitter.

use std::sync::Arc;

use glam::{Mat3, Vec3};

use super::ImageHash;

/// A value over a particle's life: keypoints of (age as a share of its
/// lifetime, 0 to 1, value), linear between them and flat past the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve(pub Vec<(f32, f32)>);

impl Curve {
    pub fn constant(value: f32) -> Self {
        Self(vec![(0.0, value)])
    }

    /// The value at `t` (0 to 1).
    pub fn at(&self, t: f32) -> f32 {
        let points = &self.0;
        let Some(&(t0, v0)) = points.first() else {
            return 0.0;
        };
        if t <= t0 {
            return v0;
        }
        for pair in points.windows(2) {
            let ((a, va), (b, vb)) = (pair[0], pair[1]);
            if t <= b {
                let s = if b > a { (t - a) / (b - a) } else { 1.0 };
                return va + (vb - va) * s;
            }
        }
        points.last().map_or(0.0, |p| p.1)
    }
}

/// How an emitter's particles are born, move and look. Each particle
/// draws its texture facing the camera.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitterLook {
    /// Its image, in [`super::Scene::images`]; none draws nothing.
    pub texture: Option<ImageHash>,
    /// Particles born per second.
    pub rate: f32,
    /// Each particle lives a random time between these, in seconds.
    pub lifetime: [f32; 2],
    /// Particles are born at a random point within this many studs of the
    /// emitter.
    pub spawn_radius: f32,
    /// Each particle starts at `velocity` (along the emitter's axes) plus
    /// a random velocity of up to `spread` studs per second in any
    /// direction.
    pub velocity: Vec3,
    pub spread: f32,
    /// Added to the velocity every second, along the emitter's axes.
    pub acceleration: Vec3,
    /// The share of its velocity a particle loses every second
    /// (exponentially).
    pub drag: f32,
    /// Its width (and height) in studs over its life, times a random
    /// factor between `size_spread`'s two.
    pub size: Curve,
    pub size_spread: [f32; 2],
    /// Turning, in degrees per second, random between the two (either
    /// way).
    pub spin: [f32; 2],
    /// sRGB colour, times `brightness` (on its light units, as Studio's
    /// particle shaders scale them) over its life: above 1 glows.
    pub color: [f32; 3],
    pub brightness: Curve,
    /// Opacity over its life, 0 to 1.
    pub alpha: Curve,
    /// 0 covers what's behind by its opacity; 1 adds to it (light).
    pub light_emission: f32,
    /// Lit by the scene's light as a surface facing the camera, or drawn
    /// in its own colour.
    pub lit: bool,
}

/// An emitter where it is now. Particles live in the world: one that's
/// born stays where it was born when the emitter moves.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    pub position: Vec3,
    /// Its axes in the world (its part's or Attachment's): Fire, Smoke
    /// and Sparkles go along their part's UpVector (observed: upside
    /// down, Smoke sinks and Fire burns downward).
    pub rotation: Mat3,
    /// Off: no new particles; those alive live on.
    pub enabled: bool,
    /// How fast its particles' time runs (0 freezes them).
    pub time_scale: f32,
    pub look: Arc<EmitterLook>,
}

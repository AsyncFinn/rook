//! Particle emitters: what the renderer simulates and draws for Fire,
//! Smoke, Sparkles and ParticleEmitter (docs/behavior/effects/fire-smoke-and-sparkles.md,
//! docs/behavior/effects/particle-emitters.md).

use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};

use super::ImageHash;

/// What a [`Curve`] holds: numbers, or colors.
pub trait Lerp: Copy + Default {
    fn lerp(a: Self, b: Self, s: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(a: Self, b: Self, s: f32) -> Self {
        a + (b - a) * s
    }
}

impl Lerp for [f32; 3] {
    fn lerp(a: Self, b: Self, s: f32) -> Self {
        [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * s)
    }
}

/// A value over a particle's life (or along a ribbon): keypoints of (time,
/// 0 to 1, value), linear between them and flat past the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve<T = f32>(pub Vec<(f32, T)>);

impl<T: Lerp> Curve<T> {
    pub fn constant(value: T) -> Self {
        Self(vec![(0.0, value)])
    }

    /// The value at `t` (0 to 1).
    pub fn at(&self, t: f32) -> T {
        self.at_moved(t, |_, v| v)
    }

    /// The value at `t`, each keypoint's value first passed through
    /// `point` (with its index).
    pub fn at_moved(&self, t: f32, point: impl Fn(usize, T) -> T) -> T {
        let points = &self.0;
        let Some(&(t0, v0)) = points.first() else {
            return T::default();
        };
        if t <= t0 {
            return point(0, v0);
        }
        for (i, pair) in points.windows(2).enumerate() {
            let ((a, va), (b, vb)) = (pair[0], pair[1]);
            if t <= b {
                let s = if b > a { (t - a) / (b - a) } else { 1.0 };
                return T::lerp(point(i, va), point(i + 1, vb), s);
            }
        }
        let last = points.len() - 1;
        point(last, points[last].1)
    }
}

/// A NumberSequence: a curve whose keypoints each move by a random amount
/// up to their envelope, chosen per particle (creator-docs
/// ParticleEmitter.Size).
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub curve: Curve,
    /// Each keypoint's envelope; empty when all are 0.
    pub envelope: Vec<f32>,
}

impl Sequence {
    pub fn constant(value: f32) -> Self {
        Self::from(Curve::constant(value))
    }

    /// The value at `t` for the particle whose envelope picks are
    /// `pick(keypoint)`, each -1 to 1.
    pub fn at(&self, t: f32, pick: impl Fn(usize) -> f32) -> f32 {
        if self.envelope.is_empty() {
            return self.curve.at(t);
        }
        self.curve.at_moved(t, |i, v| {
            v + self.envelope.get(i).copied().unwrap_or(0.0) * pick(i)
        })
    }

    /// The largest value it can take.
    pub fn max(&self) -> f32 {
        let envelope = self.envelope.iter().copied().fold(0.0, f32::max);
        self.curve.0.iter().map(|p| p.1).fold(0.0, f32::max) + envelope
    }
}

impl From<Curve> for Sequence {
    fn from(curve: Curve) -> Self {
        Self {
            curve,
            envelope: Vec::new(),
        }
    }
}

/// Where an emitter's particles are born.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spawn {
    /// Evenly within this many studs of the emitter (Fire, Smoke,
    /// Sparkles).
    Ball(f32),
    /// A ParticleEmitter's `Shape`, `half` its part's size along the
    /// emitter's axes (zero under an Attachment: a point).
    Shape {
        shape: EmitterShape,
        half: Vec3,
        /// `ShapeStyle` Surface: on its outside only.
        surface: bool,
        in_out: InOut,
        /// `ShapePartial`: a Sphere's share of its height (from the
        /// direction's pole), a Disc's inner radius.
        partial: f32,
    },
}

/// `Enum.ParticleEmitterShape`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitterShape {
    /// Anywhere in the box; particles go along the direction.
    Box,
    /// In the ellipsoid; particles go out from its center.
    Sphere,
    /// In a cylinder whose axis lies across the direction; particles go
    /// out from the axis (measured).
    Cylinder,
    /// A disc on the face the direction points to; particles go along the
    /// direction (measured).
    Disc,
}

/// `Enum.ParticleEmitterShapeInOut`: which way particles go from the
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InOut {
    Outward,
    Inward,
    /// Either, at random.
    Both,
}

/// `Enum.ParticleOrientation`: how a particle's square turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    FacingCamera,
    /// Facing the camera, but turning about world up only.
    FacingCameraWorldUp,
    /// Lengthwise along its velocity.
    VelocityParallel,
    /// Square on to its velocity.
    VelocityPerpendicular,
}

/// A texture of frames in a grid, left to right then top to bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flipbook {
    pub columns: u32,
    pub rows: u32,
    pub mode: FlipbookMode,
    /// Frames per second, random between the two (OneShot ignores it).
    pub framerate: [f32; 2],
    /// Each particle starts at a random frame.
    pub start_random: bool,
}

/// `Enum.ParticleFlipbookMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlipbookMode {
    Loop,
    /// Once over the particle's life.
    OneShot,
    /// There and back again.
    PingPong,
    /// A random frame each time one is due.
    Random,
}

impl Flipbook {
    pub fn frames(&self) -> u32 {
        (self.columns * self.rows).max(1)
    }
}

/// How an emitter's particles are born, move and look.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitterLook {
    /// Its image, in [`super::Scene::images`]; none draws nothing.
    pub texture: Option<ImageHash>,
    pub flipbook: Option<Flipbook>,
    /// Particles born per second.
    pub rate: f32,
    /// Each particle lives a random time between these, in seconds.
    pub lifetime: [f32; 2],
    pub spawn: Spawn,
    /// Each particle starts at `velocity` (along the emitter's axes) plus
    /// a random velocity of up to `spread` studs per second in any
    /// direction...
    pub velocity: Vec3,
    pub spread: f32,
    /// ...plus a speed between `speed`'s two along `direction` (the
    /// emitter's axes, or the shape's way out), turned at random by up to
    /// `spread_angle` degrees about the two axes across it.
    pub speed: [f32; 2],
    pub direction: Vec3,
    pub spread_angle: Vec2,
    /// Added to the velocity every second: along the emitter's axes, and
    /// in the world.
    pub acceleration: Vec3,
    pub world_acceleration: Vec3,
    /// The share of its velocity a particle loses every second
    /// (exponentially).
    pub drag: f32,
    /// Particles move with the emitter (`LockedToPart`).
    pub locked: bool,
    /// Its width (and height) in studs over its life, times a random
    /// factor between `size_spread`'s two.
    pub size: Sequence,
    pub size_spread: [f32; 2],
    /// Its width is divided, and its height multiplied, by 2 to this
    /// (measured: Squash 1 halves the width and doubles the height).
    pub squash: Curve,
    /// Its first turn in degrees, random between the two, and turning in
    /// degrees per second, random between the two; either way at random
    /// with `spin_either_way`. Positive turns clockwise.
    pub rotation: [f32; 2],
    pub spin: [f32; 2],
    pub spin_either_way: bool,
    pub orientation: Orientation,
    /// sRGB color over its life, times `brightness` (on its light units,
    /// as Studio's particle shaders scale them): above 1 glows.
    pub color: Curve<[f32; 3]>,
    pub brightness: Curve,
    /// Opacity over its life, 0 to 1.
    pub alpha: Sequence,
    /// 0 covers what's behind by its opacity; 1 adds to it (light).
    pub light_emission: f32,
    /// 0 draws in its own color; 1 lit by the scene's light as a surface
    /// facing the camera.
    pub light_influence: f32,
    /// Studs towards the camera it's drawn at, the same size on screen.
    pub z_offset: f32,
}

impl Default for EmitterLook {
    fn default() -> Self {
        Self {
            texture: None,
            flipbook: None,
            rate: 0.0,
            lifetime: [1.0, 1.0],
            spawn: Spawn::Ball(0.0),
            velocity: Vec3::ZERO,
            spread: 0.0,
            speed: [0.0, 0.0],
            direction: Vec3::Y,
            spread_angle: Vec2::ZERO,
            acceleration: Vec3::ZERO,
            world_acceleration: Vec3::ZERO,
            drag: 0.0,
            locked: false,
            size: Sequence::constant(1.0),
            size_spread: [1.0, 1.0],
            squash: Curve::constant(0.0),
            rotation: [0.0, 360.0],
            spin: [0.0, 0.0],
            spin_either_way: false,
            orientation: Orientation::FacingCamera,
            color: Curve::constant([1.0; 3]),
            brightness: Curve::constant(1.0),
            alpha: Sequence::constant(1.0),
            light_emission: 0.0,
            light_influence: 0.0,
            z_offset: 0.0,
        }
    }
}

/// An emitter where it is now. Particles live in the world: one that's
/// born stays where it was born when the emitter moves, unless the look
/// is `locked`.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    /// Which emitter it is (its instance and layer): its particles stay
    /// with it while the scene's list changes around it.
    pub key: u64,
    pub position: Vec3,
    /// Its axes in the world (its part's or Attachment's): Fire, Smoke
    /// and Sparkles go along their part's UpVector (observed: upside
    /// down, Smoke sinks and Fire burns downward).
    pub rotation: Mat3,
    /// Off: no new particles; those alive live on.
    pub enabled: bool,
    /// How fast its particles' time runs (0 freezes them).
    pub time_scale: f32,
    /// How many particles scripts have asked for at once (`Emit`), and
    /// how many times they've cleared it, since it was made: the renderer
    /// acts on the change.
    pub emitted: u64,
    pub cleared: u64,
    pub look: Arc<EmitterLook>,
}

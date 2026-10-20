//! Beams and Trails as strips of quads, built on the CPU each frame: a
//! Beam's curve from its Attachments (and the camera, when it faces it),
//! and a Trail's pieces, laid down as its Attachments move and aged here
//! (docs/behavior/effects/beams-and-trails.md).

mod beam;
mod trail;

use glam::Vec3;

use crate::scene::{Beam, RibbonLook, TextureMode, Trail};
use beam::beam;
use trail::TrailState;

/// One edge across a strip: where its two sides are, how far along its
/// image it is, and its colour and opacity there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Edge {
    pub(crate) sides: [Vec3; 2],
    pub(crate) u: f32,
    pub(crate) color: [f32; 3],
    pub(crate) alpha: f32,
}

/// A ribbon ready to draw: quads between consecutive edges.
#[derive(Debug)]
pub(crate) struct Strip<'a> {
    pub(crate) look: &'a RibbonLook,
    pub(crate) edges: Vec<Edge>,
}

/// The scene's Beams and Trails, and the Trails' pieces (by the Trail's
/// index in the scene).
#[derive(Debug, Default)]
pub(crate) struct Ribbons {
    beams: Vec<Beam>,
    trails: Vec<Trail>,
    states: Vec<TrailState>,
}

impl Ribbons {
    pub(crate) fn set(&mut self, beams: &[Beam], trails: &[Trail]) {
        self.beams.clear();
        self.beams.extend_from_slice(beams);
        self.states.resize_with(trails.len(), TrailState::default);
        for (trail, state) in trails.iter().zip(&mut self.states) {
            if trail.cleared != state.cleared {
                state.pieces.clear();
                state.cleared = trail.cleared;
            }
        }
        self.trails.clear();
        self.trails.extend_from_slice(trails);
    }

    /// Age the Trails' pieces by `dt` seconds and lay new ones where their
    /// Attachments have moved far enough.
    pub(crate) fn step(&mut self, dt: f32) {
        for (trail, state) in self.trails.iter().zip(&mut self.states) {
            state.step(trail, dt);
        }
    }

    pub(crate) fn any(&self) -> bool {
        !self.beams.is_empty() || self.states.iter().any(|s| !s.pieces.is_empty())
    }

    /// Every ribbon as seen from `camera` at `time` seconds (which
    /// scrolls Beams' images).
    pub(crate) fn strips(&self, camera: Vec3, time: f32) -> Vec<Strip<'_>> {
        let beams = self.beams.iter().filter_map(|b| beam(b, camera, time));
        let trails = self
            .trails
            .iter()
            .zip(&self.states)
            .filter_map(|(t, s)| s.strip(t, camera));
        beams.chain(trails).collect()
    }
}

/// A unit vector across `way` as seen from `camera` at `at`: the width of
/// a ribbon that faces the camera.
pub(super) fn across_view(way: Vec3, at: Vec3, camera: Vec3) -> Vec3 {
    let side = way.cross(camera - at);
    if side.length_squared() < 1e-10 {
        return Vec3::Y;
    }
    side.normalize()
}

#[cfg(test)]
mod tests;

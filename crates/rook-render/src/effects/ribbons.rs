//! Beams, Trails and FloorWires' images as strips of quads, built on the
//! CPU each frame: a Beam's curve from its Attachments (and the camera,
//! when it faces it), a Trail's pieces, laid down as its Attachments move
//! and aged here (docs/behavior/effects/beams-and-trails.md), and a
//! FloorWire's images where they have moved to along it.

mod beam;
mod floor_wire;
mod trail;

use glam::Vec3;

use crate::scene::{Beam, FloorWire, RibbonLook, TextureMode, Trail};
use beam::beam;
use floor_wire::floor_wire;
use trail::TrailState;

/// One edge across a strip: where its two sides are, how far along its
/// image it is, and its color and opacity there.
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

/// The scene's Beams, Trails and FloorWires, and the Trails' pieces (by
/// the Trail's index in the scene).
#[derive(Debug, Default)]
pub(crate) struct Ribbons {
    beams: Vec<Beam>,
    trails: Vec<Trail>,
    states: Vec<TrailState>,
    floor_wires: Vec<FloorWire>,
}

impl Ribbons {
    pub(crate) fn set(&mut self, beams: &[Beam], trails: &[Trail], floor_wires: &[FloorWire]) {
        self.beams.clear();
        self.beams.extend_from_slice(beams);
        self.floor_wires.clear();
        self.floor_wires.extend_from_slice(floor_wires);
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
        !self.beams.is_empty()
            || !self.floor_wires.is_empty()
            || self.states.iter().any(|s| !s.pieces.is_empty())
    }

    /// Every ribbon as seen from `camera` at `time` seconds (which
    /// scrolls Beams' images and moves FloorWires').
    pub(crate) fn strips(&self, camera: Vec3, time: f32) -> Vec<Strip<'_>> {
        let beams = self.beams.iter().filter_map(|b| beam(b, camera, time));
        let trails = self
            .trails
            .iter()
            .zip(&self.states)
            .filter_map(|(t, s)| s.strip(t, camera));
        let wires = self.floor_wires.iter().flat_map(|w| floor_wire(w, time));
        beams.chain(trails).chain(wires).collect()
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

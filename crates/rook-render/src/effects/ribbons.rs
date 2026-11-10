//! Beams, Trails and FloorWires' images as strips of quads, built on the
//! CPU each frame: a Beam's curve from its Attachments (and the camera,
//! when it faces it), a Trail's pieces, laid down as its Attachments move
//! and aged here (docs/behavior/effects/beams-and-trails.md), and a
//! FloorWire's images where they have moved to along it.

mod beam;
mod floor_wire;
mod trail;

use glam::Vec3;
use rustc_hash::FxHashMap;

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

/// The scene's Beams, Trails and FloorWires by their slots, and the
/// Trails' pieces.
#[derive(Debug, Default)]
pub(crate) struct Ribbons {
    beams: Vec<Option<Beam>>,
    trails: Vec<Option<Trail>>,
    /// By the Trail's slot.
    states: Vec<TrailState>,
    floor_wires: Vec<Option<FloorWire>>,
}

impl Ribbons {
    /// Take a new scene's ribbons. Trails keep their pieces by `key`,
    /// wherever their slot is now.
    pub(crate) fn replace(
        &mut self,
        beams: &[Option<Beam>],
        trails: &[Option<Trail>],
        floor_wires: &[Option<FloorWire>],
    ) {
        self.beams = beams.to_vec();
        self.floor_wires = floor_wires.to_vec();
        let mut old: FxHashMap<u64, TrailState> =
            (self.states.drain(..)).map(|s| (s.key, s)).collect();
        for trail in trails {
            let state = trail.as_ref().and_then(|t| old.remove(&t.key));
            self.states.push(state.unwrap_or_default());
        }
        self.trails = trails.to_vec();
        for i in 0..trails.len() {
            self.take_trail(i);
        }
    }

    pub(crate) fn set_beam(&mut self, i: usize, beam: Option<&Beam>) {
        put(&mut self.beams, i, beam);
    }

    pub(crate) fn set_floor_wire(&mut self, i: usize, wire: Option<&FloorWire>) {
        put(&mut self.floor_wires, i, wire);
    }

    /// Take the Trail now in slot `i` (`None`: it's gone, with its pieces).
    /// Another Trail taking the slot starts with none.
    pub(crate) fn set_trail(&mut self, i: usize, trail: Option<&Trail>) {
        put(&mut self.trails, i, trail);
        if self.states.len() <= i {
            self.states.resize_with(i + 1, TrailState::default);
        }
        self.take_trail(i);
    }

    /// Act on slot `i`'s Trail as it is now: a new Trail starts afresh, and
    /// scripts' `Clear` drops the pieces.
    fn take_trail(&mut self, i: usize) {
        let state = &mut self.states[i];
        let Some(trail) = &self.trails[i] else {
            *state = TrailState::default();
            return;
        };
        if state.key != trail.key {
            *state = TrailState {
                key: trail.key,
                ..TrailState::default()
            };
        }
        if trail.cleared != state.cleared {
            state.pieces.clear();
            state.cleared = trail.cleared;
        }
    }

    /// Age the Trails' pieces by `dt` seconds and lay new ones where their
    /// Attachments have moved far enough.
    pub(crate) fn step(&mut self, dt: f32) {
        for (trail, state) in self.trails.iter().zip(&mut self.states) {
            if let Some(trail) = trail {
                state.step(trail, dt);
            }
        }
    }

    pub(crate) fn any(&self) -> bool {
        self.beams.iter().any(Option::is_some)
            || self.floor_wires.iter().any(Option::is_some)
            || self.states.iter().any(|s| !s.pieces.is_empty())
    }

    /// Every ribbon as seen from `camera` at `time` seconds (which
    /// scrolls Beams' images and moves FloorWires').
    pub(crate) fn strips(&self, camera: Vec3, time: f32) -> Vec<Strip<'_>> {
        let beams = (self.beams.iter().flatten()).filter_map(|b| beam(b, camera, time));
        let trails = (self.trails.iter().zip(&self.states))
            .filter_map(|(t, s)| s.strip(t.as_ref()?, camera));
        let wires = (self.floor_wires.iter().flatten()).flat_map(|w| floor_wire(w, time));
        beams.chain(trails).chain(wires).collect()
    }
}

fn put<T: Clone>(list: &mut Vec<Option<T>>, i: usize, value: Option<&T>) {
    if list.len() <= i {
        list.resize(i + 1, None);
    }
    list[i] = value.cloned();
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

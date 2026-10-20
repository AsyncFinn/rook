//! Beams and Trails as strips of quads, built on the CPU each frame: a
//! Beam's curve from its Attachments (and the camera, when it faces it),
//! and a Trail's pieces, laid down as its Attachments move and aged here
//! (docs/behavior/effects/beams-and-trails.md).

use glam::Vec3;

use crate::scene::{Beam, RibbonLook, TextureMode, Trail};

/// The most pieces a Beam is drawn in (`Segments`).
const MAX_SEGMENTS: u32 = 1000;
/// The most pieces a Trail keeps.
const MAX_PIECES: usize = 4096;

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

/// Where a Trail was at one moment, and how long ago.
#[derive(Debug, Clone, Copy)]
struct Piece {
    ends: [Vec3; 2],
    age: f32,
    /// Studs of trail laid before it (a Static image stays where it was
    /// laid).
    laid: f32,
}

#[derive(Debug, Default)]
struct TrailState {
    /// Oldest first.
    pieces: Vec<Piece>,
    /// The trail's `cleared` acted on so far.
    cleared: u64,
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

impl TrailState {
    fn step(&mut self, trail: &Trail, dt: f32) {
        let lifetime = trail.lifetime.max(0.01);
        for p in &mut self.pieces {
            p.age += dt;
        }
        // A changed Lifetime counts for the pieces already laid
        // (creator-docs).
        self.pieces.retain(|p| p.age < lifetime);
        if !trail.enabled {
            return;
        }
        let moved = self.pieces.last().is_none_or(|last| {
            let far = (0..2)
                .map(|i| last.ends[i].distance(trail.ends[i]))
                .fold(0.0, f32::max);
            far >= trail.min_length.max(1e-3)
        });
        if moved {
            let laid = self.pieces.last().map_or(0.0, |last| {
                last.laid + midpoint(last.ends).distance(midpoint(trail.ends))
            });
            self.pieces.push(Piece {
                ends: trail.ends,
                age: 0.0,
                laid,
            });
            if self.pieces.len() > MAX_PIECES {
                self.pieces.remove(0);
            }
        }
    }

    /// The trail from its Attachments (while enabled) back through its
    /// pieces, newest first.
    fn strip<'a>(&self, trail: &'a Trail, camera: Vec3) -> Option<Strip<'a>> {
        let mut points: Vec<Piece> = Vec::with_capacity(self.pieces.len() + 1);
        // The Attachments' own place, unless a piece was just laid there.
        if trail.enabled
            && let Some(last) = self.pieces.last()
            && last.ends != trail.ends
        {
            let laid = last.laid + midpoint(last.ends).distance(midpoint(trail.ends));
            points.push(Piece {
                ends: trail.ends,
                age: 0.0,
                laid,
            });
        }
        points.extend(self.pieces.iter().rev());
        if points.len() < 2 {
            return None;
        }
        // How far along from the newest end each point is, cut at
        // MaxLength.
        let mut along = vec![0.0];
        for pair in points.windows(2) {
            let step = midpoint(pair[0].ends).distance(midpoint(pair[1].ends));
            along.push(along.last().unwrap() + step);
        }
        if trail.max_length > 0.0 {
            let keep = along.iter().take_while(|&&d| d <= trail.max_length).count();
            points.truncate(keep.max(2));
            along.truncate(points.len());
        }
        let total = along.last().copied().unwrap_or(0.0).max(1e-6);
        let lifetime = trail.lifetime.max(0.01);
        let look = &trail.look;
        let length = look.texture_length.max(1e-3);
        let last = points.len() - 1;
        let edges = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let t = (p.age / lifetime).clamp(0.0, 1.0);
                let scale = trail.width_scale.at(t, |_| 0.0).clamp(0.0, 1.0);
                let mid = midpoint(p.ends);
                let half = (p.ends[1] - p.ends[0]) * 0.5 * scale;
                let half = if look.face_camera {
                    let way = midpoint(points[i.saturating_sub(1)].ends)
                        - midpoint(points[(i + 1).min(last)].ends);
                    across_view(way, mid, camera) * half.length()
                } else {
                    half
                };
                let u = match look.texture_mode {
                    TextureMode::Stretch => along[i] / total * look.texture_length,
                    TextureMode::Wrap => along[i] / length,
                    TextureMode::Static => -p.laid / length,
                };
                Edge {
                    sides: [mid - half, mid + half],
                    u,
                    color: look.color.at(t),
                    alpha: look.alpha.at(t, |_| 0.0).clamp(0.0, 1.0),
                }
            })
            .collect();
        Some(Strip { look, edges })
    }
}

/// A Beam's strip: its Bézier curve in `segments` pieces.
fn beam<'a>(b: &'a Beam, camera: Vec3, time: f32) -> Option<Strip<'a>> {
    if !b.enabled {
        return None;
    }
    let [a, z] = b.ends;
    let p0 = a.position;
    let p3 = z.position;
    let p1 = p0 + a.rotation.x_axis * b.curve[0];
    let p2 = p3 - z.rotation.x_axis * b.curve[1];
    let n = b.segments.clamp(1, MAX_SEGMENTS);
    let points: Vec<(f32, Vec3, Vec3)> = (0..=n)
        .map(|i| {
            let s = i as f32 / n as f32;
            let r = 1.0 - s;
            let at = p0 * (r * r * r)
                + p1 * (3.0 * r * r * s)
                + p2 * (3.0 * r * s * s)
                + p3 * (s * s * s);
            let tangent =
                (p1 - p0) * (3.0 * r * r) + (p2 - p1) * (6.0 * r * s) + (p3 - p2) * (3.0 * s * s);
            (s, at, tangent)
        })
        .collect();
    let mut along = vec![0.0];
    for pair in points.windows(2) {
        along.push(along.last().unwrap() + pair[0].1.distance(pair[1].1));
    }
    if along[n as usize] <= 0.0 && b.width == [0.0, 0.0] {
        return None;
    }
    let look = &b.look;
    let length = look.texture_length.max(1e-3);
    // One cycle a second at TextureSpeed 1, from Attachment0 towards
    // Attachment1; SetTextureOffset goes the other way (creator-docs).
    let scroll = time * b.texture_speed - b.texture_offset;
    let edges = points
        .iter()
        .zip(&along)
        .map(|(&(s, at, tangent), &d)| {
            let width = b.width[0] + (b.width[1] - b.width[0]) * s;
            let to_camera = (camera - at).normalize_or_zero();
            let across = if look.face_camera {
                across_view(tangent, at, camera)
            } else {
                // Along the Attachments' up however the curve runs
                // (observed: a steep end stays an upright band).
                a.rotation
                    .y_axis
                    .lerp(z.rotation.y_axis, s)
                    .normalize_or(Vec3::Y)
            };
            let at = at + to_camera * b.z_offset;
            let u = match look.texture_mode {
                TextureMode::Stretch => s * look.texture_length,
                TextureMode::Wrap | TextureMode::Static => d / length,
            } - scroll;
            Edge {
                sides: [at - across * width * 0.5, at + across * width * 0.5],
                u,
                color: look.color.at(s),
                alpha: look.alpha.at(s, |_| 0.0).clamp(0.0, 1.0),
            }
        })
        .collect();
    Some(Strip { look, edges })
}

fn midpoint(ends: [Vec3; 2]) -> Vec3 {
    (ends[0] + ends[1]) * 0.5
}

/// A unit vector across `way` as seen from `camera` at `at`: the width of
/// a ribbon that faces the camera.
fn across_view(way: Vec3, at: Vec3, camera: Vec3) -> Vec3 {
    let side = way.cross(camera - at);
    if side.length_squared() < 1e-10 {
        return Vec3::Y;
    }
    side.normalize()
}

#[cfg(test)]
mod tests;

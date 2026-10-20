//! A Trail's pieces, laid down as its Attachments move and aged, and the
//! strip through them.

use super::*;

/// The most pieces a Trail keeps.
const MAX_PIECES: usize = 4096;

/// Where a Trail was at one moment, and how long ago.
#[derive(Debug, Clone, Copy)]
pub(super) struct Piece {
    ends: [Vec3; 2],
    age: f32,
    /// Studs of trail laid before it (a Static image stays where it was
    /// laid).
    laid: f32,
}

#[derive(Debug, Default)]
pub(super) struct TrailState {
    /// Oldest first.
    pub(super) pieces: Vec<Piece>,
    /// The trail's `cleared` acted on so far.
    pub(super) cleared: u64,
}

impl TrailState {
    pub(super) fn step(&mut self, trail: &Trail, dt: f32) {
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
    pub(super) fn strip<'a>(&self, trail: &'a Trail, camera: Vec3) -> Option<Strip<'a>> {
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

fn midpoint(ends: [Vec3; 2]) -> Vec3 {
    (ends[0] + ends[1]) * 0.5
}

//! Where the world GUI's canvases cut in among the transparent parts.
//!
//! Blending order only shows where two things cover the same pixel, so a
//! canvas needn't go exactly where its distance sorts it: anywhere after
//! the farther transparent instances its box on the screen overlaps and
//! before the nearer ones it overlaps draws the same frame. Canvases that
//! can share a place are drawn there together, in their own order, so a
//! frame breaks the parts' runs as few times as it can (the usual way to
//! bound translucent sorting cost: order only what overlaps). Overlaps are
//! found through a coarse grid of screen tiles, each listing the instances
//! that reach it in drawing order.

use glam::{Mat4, Vec2, Vec3};

use crate::batch::cull::Aabb;

/// Tiles across and down the screen.
const TILES: usize = 16;

/// A box on the screen in normalized device coordinates: its low and high
/// corners (empty when low passes high).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    low: Vec2,
    high: Vec2,
}

impl Rect {
    const SCREEN: Self = Self {
        low: Vec2::splat(-1.0),
        high: Vec2::splat(1.0),
    };

    /// What `points` cover on the screen `view_proj` draws, clamped to it;
    /// the whole screen if any is at or behind the camera's plane.
    pub(crate) fn of(view_proj: Mat4, points: impl IntoIterator<Item = Vec3>) -> Self {
        let mut low = Vec2::splat(f32::INFINITY);
        let mut high = Vec2::splat(f32::NEG_INFINITY);
        for p in points {
            let clip = view_proj * p.extend(1.0);
            if clip.w <= 1e-4 {
                return Self::SCREEN;
            }
            let at = clip.truncate().truncate() / clip.w;
            low = low.min(at);
            high = high.max(at);
        }
        Self {
            low: low.max(Vec2::splat(-1.0)),
            high: high.min(Vec2::splat(1.0)),
        }
    }

    /// What a world box, grown by `margin`, covers.
    fn of_box(view_proj: Mat4, b: Aabb, margin: f32) -> Self {
        let (min, max) = (b.min - margin, b.max + margin);
        let corners = (0..8).map(|i| {
            Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            )
        });
        Self::of(view_proj, corners)
    }

    fn is_empty(&self) -> bool {
        self.low.cmpgt(self.high).any()
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.low.cmple(other.high).all() && other.low.cmple(self.high).all()
    }

    /// The tiles it reaches, across and down.
    fn tiles(&self) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let tile = |v: f32| (((v + 1.0) * 0.5 * TILES as f32) as usize).min(TILES - 1);
        (
            tile(self.low.x)..tile(self.high.x) + 1,
            tile(self.low.y)..tile(self.high.y) + 1,
        )
    }
}

/// The frame's grid, kept to fill again without allocating.
#[derive(Debug, Default)]
pub(crate) struct Cuts {
    rects: Vec<Rect>,
    /// Each tile's instances, in drawing order: tile `t`'s are
    /// `listed[starts[t]..starts[t + 1]]`.
    starts: Vec<u32>,
    listed: Vec<u32>,
}

impl Cuts {
    /// Where each canvas cuts in among the transparent instances: before
    /// which instance it's drawn, `instances.len()` for after them all.
    /// `instances` are each instance's box in the world and squared
    /// distance (not increasing, in drawing order); `canvases` are each
    /// canvas's screen box and squared distance, far to near, all nearer
    /// than the last instance's distance only if drawn after them. The
    /// places never decrease, so the canvases stay in their order.
    pub(crate) fn places(
        &mut self,
        view_proj: Mat4,
        margin: f32,
        instances: impl ExactSizeIterator<Item = (Aabb, f32)> + Clone,
        canvases: &[(Rect, f32)],
    ) -> Vec<u32> {
        let n = instances.len();
        let distances: Vec<f32> = instances.clone().map(|(_, d)| d).collect();
        self.grid(view_proj, margin, instances.map(|(b, _)| b));
        let mut places = Vec::with_capacity(canvases.len());
        // The group being gathered: where it may go.
        let mut group = (0, n);
        let mut grouped = 0;
        for &(rect, distance) in canvases {
            // Where its distance sorts it, and how far either way it may
            // move before an instance it overlaps.
            let sorted = distances.partition_point(|&d| d >= distance);
            let (low, high) = self.room(rect, sorted, n);
            let together = (group.0.max(low), group.1.min(high));
            if together.0 <= together.1 {
                group = together;
            } else {
                // The group goes as early as it can, leaving the most room
                // after it; the next starts no earlier.
                places.extend(std::iter::repeat_n(group.0 as u32, grouped));
                grouped = 0;
                group = (low.max(group.0), high);
            }
            grouped += 1;
        }
        places.extend(std::iter::repeat_n(group.0 as u32, grouped));
        places
    }

    /// List each instance in the tiles its screen box reaches.
    fn grid(&mut self, view_proj: Mat4, margin: f32, boxes: impl Iterator<Item = Aabb>) {
        self.rects.clear();
        self.rects
            .extend(boxes.map(|b| Rect::of_box(view_proj, b, margin)));
        self.starts.clear();
        self.starts.resize(TILES * TILES + 1, 0);
        let reached = |r: &Rect| {
            let (xs, ys) = r.tiles();
            ys.flat_map(move |y| xs.clone().map(move |x| y * TILES + x))
        };
        for r in self.rects.iter().filter(|r| !r.is_empty()) {
            for t in reached(r) {
                self.starts[t + 1] += 1;
            }
        }
        for t in 0..TILES * TILES {
            self.starts[t + 1] += self.starts[t];
        }
        let mut next: Vec<u32> = self.starts[..TILES * TILES].to_vec();
        self.listed.clear();
        self.listed.resize(self.starts[TILES * TILES] as usize, 0);
        for (i, r) in self.rects.iter().enumerate().filter(|(_, r)| !r.is_empty()) {
            for t in reached(r) {
                self.listed[next[t] as usize] = i as u32;
                next[t] += 1;
            }
        }
    }

    /// The places a canvas covering `rect` may go, from just after the
    /// last instance before `sorted` it overlaps to just before the first
    /// one from `sorted` on (`n`: none).
    fn room(&self, rect: Rect, sorted: usize, n: usize) -> (usize, usize) {
        let (mut low, mut high) = (0, n);
        if rect.is_empty() {
            return (sorted, sorted);
        }
        let (xs, ys) = rect.tiles();
        for y in ys {
            for t in xs.clone().map(|x| y * TILES + x) {
                let list = &self.listed[self.starts[t] as usize..self.starts[t + 1] as usize];
                let at = list.partition_point(|&i| (i as usize) < sorted);
                let overlap = |&&i: &&u32| self.rects[i as usize].overlaps(&rect);
                let before = list[..at].iter().rev().take_while(|&&i| i as usize >= low);
                if let Some(&i) = before.into_iter().find(overlap) {
                    low = i as usize + 1;
                }
                let after = list[at..].iter().take_while(|&&i| (i as usize) < high);
                if let Some(&i) = after.into_iter().find(overlap) {
                    high = i as usize;
                }
            }
        }
        (low, high)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A box a stud across at `x`, 10 studs ahead of a camera at the
    /// origin looking down -Z.
    fn at(x: f32) -> Aabb {
        Aabb {
            min: Vec3::new(x - 0.5, -0.5, -10.5),
            max: Vec3::new(x + 0.5, 0.5, -9.5),
        }
    }

    fn view() -> Mat4 {
        crate::Camera::look_at(Vec3::ZERO, Vec3::NEG_Z).view_projection(1.0)
    }

    fn screen(x: f32) -> Rect {
        Rect::of_box(view(), at(x), 0.0)
    }

    /// Canvases that overlap no instance between them share one place;
    /// one an instance between them overlaps keeps its side of it.
    #[test]
    fn canvases_share_a_place_unless_an_overlap_divides_them() {
        let mut cuts = Cuts::default();
        // Instances at x -3, 0, 3, far to near.
        let instances = [(at(-3.0), 30.0), (at(0.0), 20.0), (at(3.0), 10.0)];
        // Far to near: between the first two, then between the last two,
        // neither overlapping anything: one place.
        let apart = [(screen(5.0), 25.0), (screen(-5.0), 15.0)];
        let places = cuts.places(view(), 0.0, instances.into_iter(), &apart);
        assert_eq!(places[0], places[1]);
        // Both now cover the middle instance: the farther must come before
        // it, the nearer after it.
        let divided = [(screen(0.0), 25.0), (screen(0.0), 15.0)];
        let places = cuts.places(view(), 0.0, instances.into_iter(), &divided);
        assert_eq!(places, [0, 2]);
    }

    /// A canvas nearer than an instance it overlaps comes after it, and
    /// one farther comes before it.
    #[test]
    fn overlapping_canvases_keep_their_side() {
        let mut cuts = Cuts::default();
        let instances = [(at(0.0), 20.0)];
        let places = cuts.places(view(), 0.0, instances.into_iter(), &[(screen(0.0), 30.0)]);
        assert_eq!(places, [0]);
        let places = cuts.places(view(), 0.0, instances.into_iter(), &[(screen(0.0), 10.0)]);
        assert_eq!(places, [1]);
    }
}

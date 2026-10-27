//! Which level of detail each mesh file draws at, by Studio's rule
//! (docs/behavior/meshes/file-meshes.md): the camera's distance to the
//! part's box, scaled to a 70 degree view, against fixed thresholds, with
//! a margin so a camera hovering at a threshold doesn't flip the level
//! every frame. Each scene entry keeps its level; image layers and
//! adornments draw at their part's, so a decal never parts from its mesh.
//! Levels are picked only for instances in runs a pass draws, so the cost
//! follows what's seen, not the place's size.

use std::sync::atomic::{AtomicU8, Ordering};

use super::*;

/// Where each level after the full one begins: studs from the part's box,
/// seen with a 70 degree field of view (`RenderFidelity` Performance in
/// Studio 0.744, observed to 0.01 stud). Levels past these aren't drawn.
const THRESHOLDS: [f32; 4] = [250.0, 500.0, 2000.0, 3000.0];

/// How much nearer than its threshold the camera comes before a coarser
/// level gives way to a finer one again. Studio has none: there a camera
/// hovering at a threshold swaps the level back and forth.
const MARGIN: f32 = 0.05;

/// No level picked yet.
const UNSET: u8 = u8::MAX;

#[derive(Debug, Default)]
pub(crate) struct Detail {
    /// Each scene entry's own entry or the part it's drawn over, whose
    /// level it shares.
    owners: Vec<u32>,
    fidelity: Vec<Fidelity>,
    /// Each entry's last level. Atomic so passes can pick while sharing the
    /// renderer; a level is only ever set to what the same inputs give.
    levels: Vec<AtomicU8>,
    eye: Vec3,
    /// Squared distances from a box where each threshold and its margin
    /// fall in this frame's view.
    coarser: [f32; THRESHOLDS.len()],
    finer: [f32; THRESHOLDS.len()],
}

impl Detail {
    pub(crate) fn clear(&mut self) {
        self.owners.clear();
        self.fidelity.clear();
        self.levels.clear();
    }

    /// Entry `i` is drawn as `part`.
    pub(crate) fn set(&mut self, i: usize, part: &PartInstance) {
        // A layer may come before its part: scene indices are reused.
        let owner = part.drawn_over.unwrap_or(i);
        let len = i.max(owner) + 1;
        if self.owners.len() < len {
            self.owners.resize(len, 0);
            self.fidelity.resize(len, Fidelity::Precise);
            self.levels.resize_with(len, || AtomicU8::new(UNSET));
        }
        self.owners[i] = owner as u32;
        self.fidelity[i] = part.fidelity;
    }

    /// Picks from here on are for `camera`'s view: a narrower view brings
    /// the thresholds nearer, as it does in Studio.
    pub(crate) fn view(&mut self, camera: &Camera) {
        self.eye = camera.position;
        let scale = (camera.fov_y.to_radians() * 0.5).tan() / 35f32.to_radians().tan();
        for (i, t) in THRESHOLDS.iter().enumerate() {
            let at = t / scale.max(1e-6);
            self.coarser[i] = at * at;
            self.finer[i] = (at * (1.0 - MARGIN)).powi(2);
        }
    }

    /// The level entry `i` draws `mesh` at, its box in the world `world`
    /// (`None` for an empty box, which draws nothing at any level).
    pub(crate) fn pick(&self, i: u32, mesh: &GpuMesh, world: Aabb) -> Option<usize> {
        if world.is_empty() {
            return None;
        }
        let owner = self.owners[i as usize] as usize;
        let allowed = match self.fidelity[owner] {
            Fidelity::Precise => 1,
            Fidelity::Automatic => mesh.high_quality,
            Fidelity::Performance => mesh.levels.len(),
        }
        .min(THRESHOLDS.len() + 1);
        if allowed == 1 {
            return Some(0);
        }
        let d2 = (self.eye.clamp(world.min, world.max) - self.eye).length_squared();
        let reached = |limits: &[f32]| limits[..allowed - 1].iter().filter(|&&l| d2 >= l).count();
        // Coarser as soon as a threshold is passed; finer only once the
        // camera is a margin nearer than it.
        let (least, most) = (reached(&self.coarser), reached(&self.finer));
        let last = self.levels[owner].load(Ordering::Relaxed);
        let level = match last {
            UNSET => least,
            last => usize::from(last).clamp(least, most),
        };
        self.levels[owner].store(level as u8, Ordering::Relaxed);
        Some(level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(levels: usize, high_quality: usize) -> GpuMesh {
        GpuMesh {
            page: 0,
            vertices: 0..3,
            indices: 0..3 * levels as u32,
            levels: (0..levels as u32).map(|l| l * 3..l * 3 + 3).collect(),
            high_quality,
            bounds: (Vec3::ZERO, Vec3::ZERO),
        }
    }

    /// A 2-stud box at the origin; the camera `d` studs from its face.
    fn at(detail: &mut Detail, d: f32, fov: f32) -> Aabb {
        let mut camera = Camera::look_at(Vec3::new(0.0, 0.0, 1.0 + d), Vec3::ZERO);
        camera.fov_y = fov;
        detail.view(&camera);
        Aabb {
            min: Vec3::splat(-1.0),
            max: Vec3::splat(1.0),
        }
    }

    fn entry(fidelity: Fidelity) -> Detail {
        let mut detail = Detail::default();
        let part = PartInstance {
            fidelity,
            ..PartInstance::plain(DrawShape::Head, Vec3::ZERO, glam::Mat3::IDENTITY, Vec3::ONE)
        };
        detail.set(0, &part);
        detail
    }

    /// Studio's thresholds from the box, each fidelity's levels, and a
    /// narrower view bringing them nearer.
    #[test]
    fn levels_follow_studios_thresholds() {
        let m = mesh(5, 2);
        let mut d = entry(Fidelity::Performance);
        let level = |d: &mut Detail, studs, fov| {
            let b = at(d, studs, fov);
            d.levels[0].store(UNSET, Ordering::Relaxed);
            d.pick(0, &m, b)
        };
        let seen: Vec<_> = [249.0, 251.0, 501.0, 2001.0, 3001.0, 9000.0]
            .map(|s| level(&mut d, s, 70.0).unwrap())
            .into();
        assert_eq!(seen, [0, 1, 2, 3, 4, 4]);
        // At 20 degrees the first threshold is 250 * tan 35 / tan 10 out.
        assert_eq!(level(&mut d, 990.0, 20.0), Some(0));
        assert_eq!(level(&mut d, 995.0, 20.0), Some(1));
        let mut d = entry(Fidelity::Automatic);
        assert_eq!(level(&mut d, 9000.0, 70.0), Some(1));
        let mut d = entry(Fidelity::Precise);
        assert_eq!(level(&mut d, 9000.0, 70.0), Some(0));
    }

    /// Going out switches at the threshold; coming back, only a margin
    /// nearer.
    #[test]
    fn a_level_holds_within_the_margin() {
        let m = mesh(5, 5);
        let mut d = entry(Fidelity::Automatic);
        let mut level = |studs| {
            let b = at(&mut d, studs, 70.0);
            d.pick(0, &m, b).unwrap()
        };
        assert_eq!(level(240.0), 0);
        assert_eq!(level(251.0), 1);
        assert_eq!(level(245.0), 1);
        assert_eq!(level(251.0), 1);
        assert_eq!(level(236.0), 0);
        // A jump past several thresholds goes straight there.
        assert_eq!(level(2500.0), 3);
    }

    /// A layer draws at its part's level, whatever its own history.
    #[test]
    fn layers_share_their_parts_level() {
        let m = mesh(5, 5);
        let mut d = entry(Fidelity::Automatic);
        let layer = PartInstance {
            drawn_over: Some(0),
            ..PartInstance::plain(DrawShape::Head, Vec3::ZERO, glam::Mat3::IDENTITY, Vec3::ONE)
        };
        d.set(1, &layer);
        let b = at(&mut d, 251.0, 70.0);
        assert_eq!(d.pick(0, &m, b), Some(1));
        let b = at(&mut d, 245.0, 70.0);
        assert_eq!(d.pick(1, &m, b), Some(1));
        assert_eq!(d.pick(1, &m, Aabb::EMPTY), None);
    }
}

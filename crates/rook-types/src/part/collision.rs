//! Collision geometry saved in unions and MeshParts: decoded by
//! `rook-content` from their `PhysicalConfigData` or `PhysicsData`, and
//! turned into colliders by `rook-physics`.

use glam::Vec3;

/// A convex piece of a union or MeshPart, in its own space (a union's
/// `MeshData` space), at its `InitialSize`, centered on its origin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hull {
    pub positions: Vec<[f32; 3]>,
    /// Three vertex indices per triangle.
    pub indices: Vec<u32>,
}

/// A shape's volume, center of mass and inertia at unit density, at
/// `InitialSize`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SavedMass {
    pub volume: f32,
    pub center: Vec3,
    /// About `center`: xx, xy, xz, yy, yz, zz.
    pub inertia: [f32; 6],
}

impl SavedMass {
    /// The same shape stretched by `scale`. Inertia doesn't scale by axis,
    /// but the second moments it's made of do (`x²` by `scale.x²`, and each
    /// by the change in volume), as measured in Studio.
    pub fn scaled(&self, scale: Vec3) -> Self {
        let [xx, xy, xz, yy, yz, zz] = self.inertia;
        let moments = [
            (yy + zz - xx) * 0.5 * scale.x * scale.x,
            (xx + zz - yy) * 0.5 * scale.y * scale.y,
            (xx + yy - zz) * 0.5 * scale.z * scale.z,
        ];
        let grow = scale.x * scale.y * scale.z;
        Self {
            volume: self.volume * grow,
            center: self.center * scale,
            inertia: [
                (moments[1] + moments[2]) * grow,
                xy * scale.x * scale.y * grow,
                xz * scale.x * scale.z * grow,
                (moments[0] + moments[2]) * grow,
                yz * scale.y * scale.z * grow,
                (moments[0] + moments[1]) * grow,
            ],
        }
    }
}

/// What a union or MeshPart collides and is queried as.
#[derive(Debug, Clone, PartialEq)]
pub enum SavedCollision {
    /// Its box (`CSGPHS` `BLOCK`).
    Block,
    /// Convex pieces, as Studio saved them.
    Hulls {
        hulls: Vec<Hull>,
        /// What Studio saved with them, or what they add up to in the
        /// versions that save none.
        mass: SavedMass,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Numbers from Studio: a shape saved with inertia (3, 5, 7), stretched
    /// to twice its length on X, spins as (6, 37, 41).
    #[test]
    fn inertia_scales_through_second_moments() {
        let saved = SavedMass {
            volume: 1.5,
            center: Vec3::new(-0.25, 0.5, 0.0),
            inertia: [3.0, 0.0, 0.0, 5.0, 0.0, 7.0],
        };
        let wide = saved.scaled(Vec3::new(2.0, 1.0, 1.0));
        assert_eq!(wide.volume, 3.0);
        assert_eq!(wide.center, Vec3::new(-0.5, 0.5, 0.0));
        assert_eq!(wide.inertia, [6.0, 0.0, 0.0, 37.0, 0.0, 41.0]);
    }
}

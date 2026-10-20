//! The surface as it's drawn: by texture layer, blending where layers
//! meet, as Studio's terrain shader does (read from its GLSL, Studio
//! 0.742; `docs/behavior/terrain.md`).
//!
//! Each vertex takes its cell's material ([`Mesher::dominant`]) and, for
//! materials whose top and side textures differ, the facing of its smooth
//! normal: its [`Layer`]. A triangle whose three corners share a layer is
//! drawn with it alone. One whose corners differ (a seam) blends the
//! corners' layers across it by their weights (the barycentric ones,
//! sharpened by the layers' height maps), so materials meet over a cell
//! instead of along the voxels' stair steps. Rook draws a seam triangle
//! once per layer it holds: the first corner's layer opaque, the others
//! added over it ([`Seam`]).

use glam::Vec3;

use super::Facing;
use super::nets::{Mesher, NONE};
use crate::TerrainMaterial;

/// One of a material's textures: the material and which way it faces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Layer {
    pub material: TerrainMaterial,
    pub facing: Facing,
}

impl Layer {
    /// How many layers there are: three facings of each material.
    pub const COUNT: usize = TerrainMaterial::ALL.len() * 3;

    /// Its place among the [`Layer::COUNT`] layers.
    pub fn index(self) -> usize {
        self.material as usize * 3 + self.facing as usize
    }
}

/// How a draw of seam triangles is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Seam {
    /// Its layer is the triangles' first corner's: drawn opaque, its share
    /// of the blend only.
    First,
    /// Its share added over the first's.
    Over,
}

/// A seam vertex's triangle: the layers of its three corners, and which
/// corner it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Corner {
    pub layers: [Layer; 3],
    pub corner: u8,
}

/// One layer's part of a chunk's drawn surface, relative to the chunk's
/// corner.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Draw {
    pub layer: Layer,
    /// `None` for the triangles wholly of this layer.
    pub seam: Option<Seam>,
    pub positions: Vec<[f32; 3]>,
    /// Smooth, or flat for the materials with hard edges, as
    /// [`super::Surface::normals`].
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Seams only: each vertex's corner (three vertices per triangle).
    pub corners: Vec<Corner>,
}

impl Mesher<'_> {
    /// The layer of vertex `i`.
    fn layer(&self, i: u32) -> Layer {
        let material = self.materials[i as usize];
        let facing = if material.faces_differ() {
            Facing::of(self.normals[i as usize].normalize_or(Vec3::Y))
        } else {
            Facing::Side
        };
        Layer { material, facing }
    }

    /// Add the chunk's triangles to `draws`, by layer.
    pub(super) fn draws(&self, draws: &mut Vec<Draw>) {
        // Each triangle's draws, sorted so each draw's are together.
        let mut keyed: Vec<((Layer, Option<Seam>), usize)> = Vec::new();
        let mut corners: Vec<[Layer; 3]> = Vec::with_capacity(self.triangles.len());
        for (n, (t, _)) in self.triangles.iter().enumerate() {
            let layers = t.map(|i| self.layer(i));
            corners.push(layers);
            if layers[1] == layers[0] && layers[2] == layers[0] {
                keyed.push(((layers[0], None), n));
                continue;
            }
            keyed.push(((layers[0], Some(Seam::First)), n));
            for (k, l) in layers.iter().enumerate().skip(1) {
                if !layers[..k].contains(l) {
                    keyed.push(((*l, Some(Seam::Over)), n));
                }
            }
        }
        keyed.sort_by_key(|(key, _)| *key);
        let mut remap = vec![NONE; self.positions.len()];
        let mut start = 0;
        while start < keyed.len() {
            let (layer, seam) = keyed[start].0;
            let end = start + keyed[start..].partition_point(|(k, _)| *k == (layer, seam));
            let mut d = Draw {
                layer,
                seam,
                ..Draw::default()
            };
            remap.fill(NONE);
            let flat = layer.material.shape().edges.flat();
            for &(_, n) in &keyed[start..end] {
                let t = self.triangles[n].0;
                let p = t.map(|i| self.positions[i as usize]);
                let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
                let normal = |i: u32| match flat {
                    true => face,
                    false => self.normals[i as usize].normalize_or(Vec3::Y),
                };
                if seam.is_some() || flat {
                    for (k, &i) in t.iter().enumerate() {
                        d.indices.push(d.positions.len() as u32);
                        d.positions.push(p[k].to_array());
                        d.normals.push(normal(i).to_array());
                        if seam.is_some() {
                            d.corners.push(Corner {
                                layers: corners[n],
                                corner: k as u8,
                            });
                        }
                    }
                    continue;
                }
                for &i in &t {
                    let r = &mut remap[i as usize];
                    if *r == NONE {
                        *r = d.positions.len() as u32;
                        d.positions.push(self.positions[i as usize].to_array());
                        d.normals.push(normal(i).to_array());
                    }
                    d.indices.push(*r);
                }
            }
            draws.push(d);
            start = end;
        }
    }
}

//! The draw meshes: each vertex's layout ([`Vertex`]) and the meshes
//! built for each [`DrawShape`]: unit primitives (extents -0.5..0.5) in
//! [`shapes`], Rook's own head in [`head`], trusses in [`truss`] and
//! adornments' shapes in [`adorn`],
//! scaled by the instance's size in the vertex shader; mesh files,
//! unions and terrain in [`files`].

mod adorn;
pub(crate) mod files;
mod head;
mod shapes;
mod truss;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use rook_content::decode::mesh::TerrainMapping;

use rook_types::{NormalId, PartShape, TrussShape};

use crate::scene::DrawShape;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// The point on the shape's bounding box this vertex maps images and
    /// surfaces from: the vertex itself on flat faces and mesh files.
    pub box_point: [f32; 3],
    /// The `NormalId` index of the box face it belongs to, or [`NO_FACE`]
    /// (mesh files: the face is found from the normal), in the low byte;
    /// a union's vertex colour (sRGB) in the three bytes above, drawn
    /// instead of the part's colour when the instance asks for it
    /// (`PartInstance::vertex_colors`). wgpu allows 16 vertex attributes
    /// and the vertex and instance layouts use them all, hence the packing.
    pub face: u32,
}

/// No box face of its own: the shader finds one from the normal.
pub(crate) const NO_FACE: u32 = 255;

/// Added to a union vertex's face: Textures and material textures tile
/// from the face's centre, as they follow the union's UVs in Studio.
pub(crate) const UNION_FACE: u32 = 64;

/// A terrain vertex's face: its material textures are projected onto the
/// plane its surface faces most, in world studs; plus 1 for the axes'
/// planes only, 2 for water and 4 for grass blades
/// (`rook_content::decode::mesh::TerrainMapping`).
pub(crate) const TERRAIN_FACE: u32 = 128;

/// Where a terrain seam vertex's face holds which corners' layer its mesh
/// draws (`TerrainCorner::drawn`, 3 bits). The vertex's UV is its
/// barycentric weights' first two, and its box point its triangle's
/// corners' layers.
pub(crate) const TERRAIN_DRAWN_SHIFT: u32 = 9;

impl Vertex {
    /// A vertex mapped from where it is, its face found from its normal.
    fn plain(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
            box_point: position,
            face: NO_FACE,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// A flat polygon (convex, counter-clockwise seen from outside). Its UVs
    /// put the whole image on the face its normal belongs to, oriented as
    /// Studio orients images on that face (`rook_types::NormalId`).
    fn polygon(&mut self, points: &[Vec3]) {
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let face = NormalId::of_normal(normal);
        let base = self.vertices.len() as u32;
        for p in points {
            let (at, extent) = face.image_position(*p, Vec3::ONE);
            self.vertices.push(Vertex {
                face: face.index() as u32,
                ..Vertex::plain(p.to_array(), normal.to_array(), (at / extent).to_array())
            });
        }
        for i in 1..points.len() as u32 - 1 {
            self.indices
                .extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }
}

impl MeshData {
    /// The smallest box holding every vertex: (min, max).
    pub(crate) fn bounds(&self) -> (Vec3, Vec3) {
        if self.vertices.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO);
        }
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            lo = lo.min(Vec3::from(v.position));
            hi = hi.max(Vec3::from(v.position));
        }
        (lo, hi)
    }
}

pub(crate) fn build(shape: DrawShape) -> MeshData {
    match shape {
        DrawShape::Part(shape @ (PartShape::Block | PartShape::Wedge | PartShape::CornerWedge)) => {
            shapes::polyhedron(shape)
        }
        DrawShape::Part(PartShape::Cylinder) => shapes::cylinder(),
        DrawShape::Part(PartShape::Ball) => shapes::ball(),
        DrawShape::Head => head::head(),
        // Mesh files come with the scene (`Renderer::has_mesh`).
        DrawShape::File(_) => MeshData::default(),
        DrawShape::Truss(shape) => truss::truss(shape),
        DrawShape::Adorn(shape) => adorn::adorn(shape),
    }
}

#[cfg(test)]
mod tests;

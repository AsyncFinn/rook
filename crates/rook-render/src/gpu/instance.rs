//! Each instance's record, as `shaders/common.wgsl` reads it: its pose, color, and flags for how it draws.

use super::*;

/// Instance flags (see `shaders/common.wgsl`).
pub(crate) const CAST: u32 = 1;
pub(crate) const NEON: u32 = 2;
pub(crate) const MODE_FACE: u32 = 1;
pub(crate) const MODE_TILED: u32 = 2;
pub(crate) const MODE_MESH_UV: u32 = 3;
/// A part's material textures' [`TilePattern`], in the bits an image
/// layer's mode takes.
pub(crate) const PATTERN_SHIFT: u32 = 2;
pub(crate) const UPRIGHT: u32 = 1 << 7;
/// Bits 4-6 of a base part without a tile pattern (an image layer keeps
/// its face there): drawn in its own color without light, through the
/// tone curve (`Light::Unlit`) or past it (`Light::Exact`, with
/// [`EXACT`]); turned to face the camera (`PartInstance::faces_camera`).
pub(crate) const UNLIT: u32 = 1 << 4;
pub(crate) const EXACT: u32 = 1 << 5;
pub(crate) const FACES_CAMERA: u32 = 1 << 6;
/// Where the surface patterns start: 3 bits per face.
pub(crate) const SURFACES_SHIFT: u32 = 8;
/// The part has material textures, tiled at `tile.xy` studs.
pub(crate) const MAPS: u32 = 1 << 26;
/// In a terrain instance's surface bits (`MaterialMaps::terrain_color`):
/// its color is a recoloring of its color map, or tints it by the map's
/// alpha.
pub(crate) const TERRAIN_RECOLOR: u32 = 3;
pub(crate) const TERRAIN_TINT: u32 = 1;
/// Glass and ForceField draw their own way (`shaders/parts.wgsl`).
pub(crate) const GLASS: u32 = 1 << 27;
pub(crate) const FORCE_FIELD: u32 = 1 << 28;
/// A wedge or corner wedge, whose slopes take material textures in their
/// own plane.
pub(crate) const SLOPED: u32 = 1 << 29;
/// The material textures go through the mesh's UVs (a SurfaceAppearance),
/// with their alpha mode in the surfaces' bits (a MeshPart has none).
pub(crate) const MAPS_UV: u32 = 1 << 30;
/// The mesh's vertex colors replace the part's color (a union with
/// `UsePartColor` off).
pub(crate) const VERTEX_COLORS: u32 = 1 << 31;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Instance {
    position: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    back: [f32; 3],
    size: [f32; 3],
    color: [f32; 4],
    /// The mesh's bounds in its own units: images on faces cover this box
    /// scaled by `size`.
    box_min: [f32; 3],
    box_max: [f32; 3],
    /// A Texture's studs per tile and offset; for a part with material
    /// textures, their studs per tile, and on terrain their [`Detile`].
    tile: [f32; 4],
    /// CAST, NEON, an image layer's mode or a part's tile pattern (bits
    /// 2-3), an image layer's face (4-6),
    /// UPRIGHT (bit 7), from bit 8 each face's surface pattern + 1, three
    /// bits per face, and the bits from 26 (wgpu allows 16 vertex
    /// attributes in all).
    flags: u32,
    /// Its images' slots in the image table, 16 bits each, in
    /// [`renderer::ImageSet`] order (unused without a table).
    images: [u32; 3],
    /// A part's `Reflectance` (where it shows), roughness and metalness;
    /// w is the blue of a SurfaceAppearance's tint, whose red and green
    /// are in `tile.zw`; where a built-in material's color map's alpha
    /// masks its tint, its `NormalDetail` as `repeats + strength / 2`
    /// (at least 1, `detail_slopes` tells the parts apart); 0 otherwise.
    material: [f32; 4],
}

impl Instance {
    pub(crate) fn new(
        p: &PartInstance,
        (box_min, box_max): (Vec3, Vec3),
        images: [u32; 3],
    ) -> Self {
        let mut flags = 0;
        let mut tile = [1.0, 1.0, 0.0, 0.0];
        let mut material = [0.0; 4];
        if p.upright {
            flags |= UPRIGHT;
        }
        match p.layer {
            Layer::Base => {
                if p.cast_shadow {
                    flags |= CAST;
                }
                if p.vertex_colors {
                    flags |= VERTEX_COLORS;
                }
                flags |= match p.light {
                    Light::Lit => 0,
                    Light::Unlit => UNLIT,
                    Light::Exact => UNLIT | EXACT,
                };
                if p.faces_camera {
                    flags |= FACES_CAMERA;
                }
                // A part without material textures leaves `tile` unread:
                // w > 0 grows whole about xyz, w < 0 is pixels across.
                match p.grows {
                    Some(Grow::Whole { anchor, per_stud }) => {
                        tile = anchor.extend(per_stud).to_array();
                    }
                    Some(Grow::Across { pixels }) => tile = [0.0, 0.0, 0.0, -pixels],
                    None => {}
                }
                flags |= match p.material {
                    Material::Neon => NEON,
                    Material::Glass => GLASS,
                    Material::ForceField => FORCE_FIELD,
                    _ => 0,
                };
                // A map gives the value itself; without one, the material's.
                let shading = p.material.shading(p.material_set);
                let maps = p.maps;
                // Only Studio's Plastic and Glass shaders take Reflectance:
                // Plastic, SmoothPlastic and Glass parts and meshes without
                // a texture (observed: Metal, Wood, Neon and a textured
                // mesh at 1 show none of the sky; `shaders_glsl3.pack` has
                // it in no other shader). Terrain water reflects by its own
                // `WaterReflectance`, in the water shaders.
                let reflects = p.material == Material::Water
                    || matches!(
                        p.material,
                        Material::Plastic | Material::SmoothPlastic | Material::Glass
                    ) && p.texture.is_none()
                        && !maps.on_mesh_uv;
                material = [
                    // In 255ths, as Studio's vertices carry it.
                    if reflects {
                        (p.reflectance.clamp(0.0, 1.0) * 255.0).round() / 255.0
                    } else {
                        0.0
                    },
                    if maps.roughness.is_some() {
                        1.0
                    } else {
                        shading.roughness
                    },
                    if maps.metalness.is_some() {
                        1.0
                    } else {
                        shading.metalness
                    },
                    0.0,
                ];
                if maps.on_mesh_uv {
                    flags |= MAPS | MAPS_UV | (maps.alpha as u32) << SURFACES_SHIFT;
                    let t = maps.tint;
                    tile = [1.0, 1.0, t[0], t[1]];
                    material[3] = t[2];
                } else if maps.any() {
                    // Terrain's maps say how their color is used in the
                    // bits Plastic's surfaces use otherwise; a part's say
                    // whether the color map's alpha masks its tint, as
                    // Studio's part shaders take it, in `material.w`.
                    let mode = match maps.terrain_color {
                        TerrainColor::Recolor => TERRAIN_RECOLOR,
                        TerrainColor::Tint => TERRAIN_TINT,
                        TerrainColor::Plain => 0,
                    };
                    flags |= MAPS | mode << SURFACES_SHIFT;
                    if maps.alpha == MapAlpha::TintMask {
                        material[3] = maps.detail.repeats + maps.detail.strength * 0.5;
                    }
                    if matches!(
                        p.shape,
                        DrawShape::Part(PartShape::Wedge | PartShape::CornerWedge)
                    ) {
                        flags |= SLOPED;
                    }
                    flags |= (maps.pattern as u32) << PATTERN_SHIFT;
                    let [z, w] = if maps.seed != 0 {
                        // Studs the part's textures start in by.
                        let mut rng = Rng::new(maps.seed);
                        (glam::Vec2::new(rng.next(), rng.next()) * maps.studs_per_tile).to_array()
                    } else {
                        [maps.detile.shift, maps.detile.turn]
                    };
                    tile = [maps.studs_per_tile.x, maps.studs_per_tile.y, z, w];
                }
                if p.material.shows_surfaces() {
                    for (face, pattern) in p.surfaces.iter().enumerate() {
                        if let Some(pattern) = pattern {
                            flags |= (*pattern as u32 + 1) << (SURFACES_SHIFT + face as u32 * 3);
                        }
                    }
                }
            }
            Layer::Image { placement, .. } => {
                let (mode, face) = match placement {
                    ImagePlacement::Face(face) => (MODE_FACE, face.index() as u32),
                    ImagePlacement::Tiled(face, t) => {
                        tile = [
                            t.studs_per_tile.x,
                            t.studs_per_tile.y,
                            t.offset.x,
                            t.offset.y,
                        ];
                        (MODE_TILED, face.index() as u32)
                    }
                    ImagePlacement::MeshUv => (MODE_MESH_UV, 0),
                };
                flags |= mode << 2 | face << 4;
            }
        }
        Self {
            position: p.position.to_array(),
            right: p.rotation.x_axis.to_array(),
            up: p.rotation.y_axis.to_array(),
            back: p.rotation.z_axis.to_array(),
            size: p.size.to_array(),
            color: [p.color[0], p.color[1], p.color[2], 1.0 - p.transparency],
            box_min: box_min.to_array(),
            box_max: box_max.to_array(),
            tile,
            flags,
            images,
            material,
        }
    }

    /// This instance where `part` is now (its look unchanged).
    pub(crate) fn placed(mut self, part: &PartInstance) -> Self {
        self.position = part.position.to_array();
        self.right = part.rotation.x_axis.to_array();
        self.up = part.rotation.y_axis.to_array();
        self.back = part.rotation.z_axis.to_array();
        self
    }

    /// This instance carrying Highlight `number` for the mask
    /// (`shaders/parts.wgsl`'s `vs_highlight`) in its color.
    pub(crate) fn numbered(mut self, number: u32) -> Self {
        self.color = [number as f32, 0.0, 0.0, 1.0];
        self
    }

    /// Whether it keeps its size on screen (`PartInstance::grows`): a base
    /// part without material textures or a tile pattern, `tile.w` not 0.
    fn grows(&self) -> bool {
        self.flags & (MAPS | 3 << PATTERN_SHIFT) == 0 && self.tile[3] != 0.0
    }

    /// The box around what this instance draws in the world, before the
    /// shader moves any vertex. A free slot (all zeroes) draws nothing,
    /// nor does one placed where a value isn't a number: its box is empty,
    /// since NaN in a box would spread to every box joined with it and
    /// hide what they hold.
    pub(crate) fn world_box(&self) -> Aabb {
        let size = Vec3::from(self.size);
        if size == Vec3::ZERO {
            return Aabb::EMPTY;
        }
        let (lo, hi) = (
            Vec3::from(self.box_min) * size,
            Vec3::from(self.box_max) * size,
        );
        let rotation = glam::Mat3::from_cols(self.right.into(), self.up.into(), self.back.into());
        let center = Vec3::from(self.position) + rotation * ((lo + hi) * 0.5);
        let mut half = rook_types::box_reach(rotation, ((hi - lo) * 0.5).abs());
        if self.flags & FACES_CAMERA != 0 && self.flags & (3 << PATTERN_SHIFT) == 0 {
            // Turned to the camera in the shader: any way round.
            half = Vec3::splat(half.length());
        }
        if self.grows() {
            // Its size depends on the camera: kept in every view.
            return Aabb {
                min: Vec3::splat(-1e7),
                max: Vec3::splat(1e7),
            };
        }
        let b = Aabb {
            min: center - half,
            max: center + half,
        };
        if b.min.is_finite() && b.max.is_finite() {
            b
        } else {
            Aabb::EMPTY
        }
    }
}

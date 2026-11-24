//! Texture streaming: a texture on a surface in the world loads at its
//! smallest (`Want::Texture(0)`), then larger as it grows on screen, as
//! Roblox's client does (measured in Studio 0.748,
//! docs/behavior/content/asset-fetching.md#streaming): each texture is
//! wanted at the smallest power of two at least as many pixels across as
//! the surface it covers takes on screen, and no smaller than 64. Only
//! what is in view grows, and nothing shrinks again.
//!
//! [`SceneCache::stream_textures`] looks over the drawn parts a few times
//! a second: for a decal its face, for a texture one tile, for a mesh's
//! texture or a decal through its UVs the whole part, sized on screen from
//! the nearest point of the part to the camera.

use glam::{Vec2, Vec3};
use std::sync::Arc;

use rook_content::decode::mesh::Mesh;
use rook_render::{Camera, DrawShape, ImageHash, ImagePlacement, Layer, MeshHash, PartInstance};
use rook_types::NormalId;
use rustc_hash::FxHashMap;

use crate::SceneCache;

/// Frames between looks over the parts.
const EVERY: u32 = 6;
/// The smallest size a texture in view is wanted at, as `Want::Texture`'s
/// exponent (64 pixels: Studio asked for 64 for a decal 28 pixels across).
const SMALLEST: u8 = 6;
/// The largest size there is (Roblox's limit, 4096).
const LARGEST: u8 = 12;

/// The camera and the view's size in pixels, as last given.
#[derive(Debug, Clone, Copy)]
pub(crate) struct View {
    pub camera: Camera,
    pub width: u32,
    pub height: u32,
}

impl View {
    /// Pixels a stud takes on screen one stud from the camera.
    fn pixels_per_stud(&self) -> f32 {
        self.height as f32 / (2.0 * (self.camera.fov_y.to_radians() / 2.0).tan())
    }

    /// Whether a ball at `center` of `radius` is in view at all.
    fn sees(&self, center: Vec3, radius: f32) -> bool {
        let v = self.camera.rotation.transpose() * (center - self.camera.position);
        let depth = -v.z;
        if depth + radius < 0.0 {
            return false;
        }
        let tan_y = (self.camera.fov_y.to_radians() / 2.0).tan();
        let tan_x = tan_y * self.width as f32 / self.height.max(1) as f32;
        let inside =
            |side: f32, tan: f32| side.abs() - radius * (1.0 + tan * tan).sqrt() <= depth * tan;
        inside(v.x, tan_x) && inside(v.y, tan_y)
    }
}

impl SceneCache {
    /// The camera and the view's size from now on, for sizing textures on
    /// screen. Until it's given, textures stay as they first load.
    pub fn set_view(&mut self, camera: &Camera, width: u32, height: u32) {
        self.view = Some(View {
            camera: *camera,
            width: width.max(1),
            height: height.max(1),
        });
    }

    /// Every few frames, ask for each texture in view at the size it takes
    /// on screen; `now` looks over the parts this frame whatever.
    pub(crate) fn stream_textures(&mut self, now: bool) {
        self.stream_tick = self.stream_tick.wrapping_add(1);
        if !now && !self.stream_tick.is_multiple_of(EVERY) {
            return;
        }
        let Some(view) = self.view else {
            return;
        };
        profiling::scope!("SceneCache::stream_textures");
        let mut wanted: FxHashMap<ImageHash, (u8, f32)> = FxHashMap::default();
        let per_stud = view.pixels_per_stud();
        for part in &self.scene.parts {
            let images = surface_images(part);
            if images.iter().all(Option::is_none) {
                continue;
            }
            let half = half_extents(part, &self.scene.meshes);
            if !view.sees(part.position, half.length()) {
                continue;
            }
            let local = part.rotation.transpose() * (view.camera.position - part.position);
            let distance = (local.abs() - half).max(Vec3::ZERO).length().max(0.05);
            for (image, studs) in images.into_iter().flatten() {
                let pixels = studs * per_stud / distance;
                let size = size_for(pixels);
                let entry = wanted.entry(image).or_insert((0, f32::MAX));
                entry.0 = entry.0.max(size);
                entry.1 = entry.1.min(distance);
            }
        }
        for (image, (size, distance)) in wanted {
            self.files.upgrade(image, size, distance);
        }
    }
}

/// The `Want::Texture` exponent for a texture `pixels` across on screen:
/// the smallest power of two at least that, from 64 to 4096.
fn size_for(pixels: f32) -> u8 {
    let exponent = pixels.max(1.0).log2().ceil();
    (exponent as u8).clamp(SMALLEST, LARGEST)
}

/// The images a part's surface draws that stream, and how many studs
/// across each covers once (its longer side): a decal its face, a texture
/// one tile (even one larger than the face), a mesh's texture and a decal through
/// its UVs the whole part.
fn surface_images(part: &PartInstance) -> [Option<(ImageHash, f32)>; 2] {
    let whole = || {
        let s = part.size;
        s.x.max(s.y).max(s.z)
    };
    let face = |face: NormalId| {
        let s = part.size;
        let (u, v) = match face {
            NormalId::Front | NormalId::Back => (s.x, s.y),
            NormalId::Top | NormalId::Bottom => (s.x, s.z),
            NormalId::Left | NormalId::Right => (s.z, s.y),
        };
        Vec2::new(u, v)
    };
    let layer = match part.layer {
        Layer::Image {
            image, placement, ..
        } => {
            let studs = match placement {
                ImagePlacement::Face(n) => face(n).max_element(),
                // One tile, however it compares to the face: a tile larger
                // than the face shows only part of the image, magnified
                // (a sprite sheet cell on a small part).
                ImagePlacement::Tiled(_, tiling) => tiling.studs_per_tile.abs().max_element(),
                ImagePlacement::MeshUv => whole(),
            };
            Some((image, studs))
        }
        Layer::Base => None,
    };
    let texture = part.texture.map(|t| (t, whole()));
    [layer, texture]
}

/// Half the part's size in its own axes, its mesh's bounds for a mesh
/// file (whose `size` is a scale).
fn half_extents(part: &PartInstance, meshes: &FxHashMap<MeshHash, Arc<Mesh>>) -> Vec3 {
    match part.shape {
        DrawShape::File(mesh) => meshes.get(&mesh).map_or(part.size / 2.0, |m| {
            let (lo, hi) = m.bounds();
            let far = Vec3::from(lo).abs().max(Vec3::from(hi).abs());
            far * part.size.abs()
        }),
        DrawShape::Head => Vec3::splat(0.6) * part.size.abs(),
        _ => part.size.abs() / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_the_next_power_of_two_from_64() {
        // Studio 0.748: a 16-stud decal 28, 57, 113, 226, 452, 905, 1414
        // and 2262 pixels across asked for 64, 64, 128, 256, 512, 1024,
        // 2048 and 4096.
        let sizes: Vec<u8> = [
            28.0, 57.0, 113.0, 226.0, 452.0, 905.0, 1414.0, 2262.0, 9000.0,
        ]
        .map(size_for)
        .to_vec();
        assert_eq!(sizes, [6, 6, 7, 8, 9, 10, 11, 12, 12]);
    }

    #[test]
    fn a_texture_is_sized_by_its_tile_even_past_the_face() {
        // DB2's icons: a 1-stud part showing one 120-pixel cell of a
        // 4096-pixel sheet, its tile 34 studs across.
        let tiled = |studs: f32| {
            let mut part = PartInstance::plain(
                DrawShape::Part(rook_types::PartShape::Block),
                Vec3::ZERO,
                glam::Mat3::IDENTITY,
                Vec3::new(1.0, 1.0, 0.001),
            );
            part.layer = Layer::Image {
                image: ImageHash([1; 32]),
                placement: ImagePlacement::Tiled(
                    NormalId::Front,
                    rook_types::Tiling {
                        studs_per_tile: Vec2::splat(studs),
                        offset: Vec2::ZERO,
                    },
                ),
                z: 0,
                on_transparent: false,
            };
            surface_images(&part)[0].map(|(_, studs)| studs)
        };
        assert_eq!(tiled(34.0), Some(34.0));
        assert_eq!(tiled(0.25), Some(0.25), "repeated over the face");
    }

    #[test]
    fn sees_only_what_is_in_view() {
        let view = View {
            camera: Camera::look_at(Vec3::ZERO, Vec3::NEG_Z),
            width: 1600,
            height: 900,
        };
        assert!(view.sees(Vec3::new(0.0, 0.0, -10.0), 1.0));
        assert!(!view.sees(Vec3::new(0.0, 0.0, 10.0), 1.0), "behind");
        assert!(
            !view.sees(Vec3::new(100.0, 0.0, -10.0), 1.0),
            "off to the side"
        );
        assert!(
            view.sees(Vec3::new(0.0, 0.0, 3.0), 5.0),
            "around the camera"
        );
        // 16 studs 100 away at 900 pixels high and 70 degrees: 103 pixels.
        assert!((view.pixels_per_stud() * 16.0 / 100.0 - 102.8).abs() < 0.5);
    }
}

//! Texture streaming, as Roblox's client does it
//! (docs/behavior/content/asset-fetching.md#streaming): a texture on a
//! surface loads at its smallest pack, is wanted at the size it takes on
//! screen, and grows there a level every 0.35 s (`Files::step`). Only what
//! is in view grows, and nothing shrinks again.
//!
//! The scene keeps the entries whose textures can still grow, noted as
//! they're written ([`SceneCache::note_streaming`]);
//! [`SceneCache::stream_textures`] looks over only those, a few times a
//! second, and drops each once its textures can't grow. A decal is sized
//! by its face, a texture by one tile, a mesh's texture by the whole part,
//! each from the part's point nearest the camera.

use glam::{Vec2, Vec3};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rook_content::decode::mesh::Mesh;
use rook_render::{Camera, DrawShape, ImageHash, ImagePlacement, Layer, MeshHash, PartInstance};
use rook_types::NormalId;
use rustc_hash::FxHashMap;

use super::loader::LARGEST;
use crate::SceneCache;

/// Time between looks over the entries that can grow.
const EVERY: Duration = Duration::from_millis(100);
/// The smallest size a texture in view is wanted at, as `Want::Texture`'s
/// exponent (64 pixels: Studio asked for 64 for a decal 28 pixels across).
const SMALLEST: u8 = 6;

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

    /// Note that entry `i` of `scene.parts` was written: it's looked over
    /// while it draws a texture that can still grow.
    pub(crate) fn note_streaming(&mut self, i: usize) {
        let files = &self.files;
        let grows = surface_images(&self.scene.parts[i])
            .iter()
            .flatten()
            .any(|(image, _)| files.can_grow(image));
        if grows {
            self.streaming.insert(i);
        } else {
            self.streaming.remove(&i);
        }
    }

    /// A few times a second (or `now` whatever), ask for each texture in
    /// view that can still grow at the size it takes on screen, and step
    /// the textures growing towards theirs (straight there with `now`,
    /// for screenshots).
    pub(crate) fn stream_textures(&mut self, now: bool) {
        let time = Instant::now();
        let due = self.streamed_at.is_none_or(|at| time >= at + EVERY);
        if (now || due)
            && let Some(view) = self.view
        {
            self.streamed_at = Some(time);
            self.want_textures(view);
        }
        self.files.step(time, now);
    }

    /// Look over the entries whose textures can still grow, dropping those
    /// that can't (whole, at 4096, no longer drawing one), and want each
    /// texture in view at its size on screen.
    fn want_textures(&mut self, view: View) {
        profiling::scope!("SceneCache::want_textures");
        let mut wanted: FxHashMap<ImageHash, (u8, f32)> = FxHashMap::default();
        let per_stud = view.pixels_per_stud();
        let (parts, meshes, files) = (&self.scene.parts, &self.scene.meshes, &self.files);
        self.streaming.retain(|&i| {
            let Some(part) = parts.get(i) else {
                return false;
            };
            let images = surface_images(part).map(|s| s.filter(|(image, _)| files.can_grow(image)));
            if images.iter().all(Option::is_none) {
                return false;
            }
            let half = half_extents(part, meshes);
            if !view.sees(part.position, half.length()) {
                return true;
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
            true
        });
        self.files.want(&wanted);
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
        // A sprite-sheet cell on a small part: a 1-stud part showing one
        // 120-pixel cell of a 4096-pixel sheet, its tile 34 studs across.
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

    /// The scene looks over only the entries drawing a texture that can
    /// still grow: written ones join, released ones and those whose
    /// textures can't grow leave.
    #[test]
    fn looks_over_only_what_can_grow() {
        let mut dom = rook_dom::Dom::new();
        let ws = dom.get_service("Workspace").unwrap();
        let part = dom.create_by_name("Part").unwrap();
        dom.set_parent(part, Some(ws)).unwrap();
        let content = Arc::new(rook_content::Resolver::new(rook_content::Config::empty()));
        let mut cache = SceneCache::new(&dom, crate::Floors::default(), content, Vec3::ZERO);
        assert!(cache.streaming.is_empty(), "a plain part draws no texture");
        let i = cache.index[&part];
        let (small, whole) = (ImageHash([1; 32]), ImageHash([2; 32]));
        cache.files.stream_for_test(small, 32, false);
        cache.files.stream_for_test(whole, 32, true);

        cache.scene.parts[i].texture = Some(whole);
        cache.note_streaming(i);
        assert!(cache.streaming.is_empty(), "a texture that can't grow");
        cache.scene.parts[i].texture = Some(small);
        cache.note_streaming(i);
        assert!(cache.streaming.contains(&i));
        cache.release(i);
        assert!(cache.streaming.is_empty());

        cache.note_streaming(i);
        cache.files.stream_for_test(small, 4096, true);
        cache.set_view(
            &Camera::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO),
            800,
            600,
        );
        cache.stream_textures(true);
        assert!(cache.streaming.is_empty(), "its texture became whole");
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
